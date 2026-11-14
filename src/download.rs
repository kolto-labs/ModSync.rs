use crate::acquisition::ManualReason;
use anyhow::{Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// Idle timeout. reqwest's blocking `timeout` bounds connect-through-headers and then
/// each individual body read, not the whole transfer. A stalled stream therefore fails
/// after this long without progress, while a slow but steady body of any size never
/// times out. There is deliberately no whole-transfer cap.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// TCP/TLS connect bound, inside the idle timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest `Retry-After` honored before a retry.
pub const RETRY_AFTER_CAP: Duration = Duration::from_secs(60);
/// Bytes of a suspect body inspected for challenge, login or countdown markers.
const SNIFF_LIMIT: u64 = 64 * 1024;

/// Per-invocation trust for an isolated HTTPS loopback probe. Never persisted.
#[derive(Clone)]
pub struct LoopbackCa(reqwest::Certificate);

impl LoopbackCa {
    pub fn from_pem_file(path: &Path) -> Result<Self> {
        let pem = std::fs::read(path)
            .map_err(|_| anyhow::anyhow!("Cannot read loopback CA certificate"))?;
        let certificate = reqwest::Certificate::from_pem(&pem)
            .map_err(|_| anyhow::anyhow!("Invalid loopback CA certificate"))?;
        // The rustls reqwest backend defers PEM/DER validation until client build.
        ensure!(
            !reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|_| anyhow::anyhow!("Invalid loopback CA certificate"))?
                .is_empty(),
            "Invalid loopback CA certificate"
        );
        reqwest::blocking::Client::builder()
            .use_rustls_tls()
            .no_proxy()
            .add_root_certificate(certificate.clone())
            .build()
            .map_err(|_| anyhow::anyhow!("Invalid loopback CA certificate"))?;
        Ok(Self(certificate))
    }
}

pub(crate) fn eligible_loopback(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && match url.host() {
            Some(host) => {
                host.to_string() == "localhost"
                    || host
                        .to_string()
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            }
            None => false,
        }
}

/// A blocking client for a provider's own page requests: https only, the shared timeouts
/// and no automatic redirects, so the caller vets every hop and decides which cookies go
/// where. `no_proxy` (an explicit fixture base) or a loopback CA disables proxies, and a
/// loopback CA is trusted, as in `fetch_inner`.
pub(crate) fn page_client(
    loopback_ca: Option<&LoopbackCa>,
    no_proxy: bool,
) -> Result<reqwest::blocking::Client> {
    client_with_policy(loopback_ca, no_proxy, reqwest::redirect::Policy::none())
}

/// As [`page_client`], with the caller's own redirect policy (it vets every hop).
pub(crate) fn client_with_policy(
    loopback_ca: Option<&LoopbackCa>,
    no_proxy: bool,
    policy: reqwest::redirect::Policy,
) -> Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(IDLE_TIMEOUT)
        .redirect(policy)
        .user_agent(concat!("ModSync/", env!("CARGO_PKG_VERSION")));
    if no_proxy {
        builder = builder.no_proxy();
    }
    if let Some(ca) = loopback_ca {
        builder = builder
            .add_root_certificate(ca.0.clone())
            .no_proxy()
            .resolve("localhost", "127.0.0.1:0".parse().expect("literal address"));
    }
    Ok(builder.build()?)
}

/// Why a fetch failed. Callers decide retries and fallbacks from this, never from text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "class")]
pub enum FailureClass {
    /// Connect/reset/timeout, or HTTP 408/429/5xx that is not a web page.
    Transient {
        retry_after_secs: Option<u64>,
    },
    /// The host served a web page (challenge, login, countdown or landing page).
    /// Never retried and never automated around.
    ManualRequired {
        url: String,
        reason: ManualReason,
    },
    Cancelled,
    /// Everything else: other 4xx, HTTPS refusal, hash or archive verify failure, unsafe names.
    Permanent,
}

#[derive(Debug)]
pub struct FetchError {
    pub class: FailureClass,
    error: anyhow::Error,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

pub(crate) fn fail(class: FailureClass, error: impl Into<anyhow::Error>) -> anyhow::Error {
    anyhow::Error::new(FetchError {
        class,
        error: error.into(),
    })
}

/// The file name of a [`Destination::Directory`] publish (for an unnamed one, the name the
/// transfer revealed) is already taken when the transfer ends, or is taken by a concurrent
/// publish before this one's no-clobber move. The transfer and its
/// digest are complete, but nothing was published: the present file is untouched and the
/// temp file is removed. `sha256` is the digest of the transferred bytes (for a Peer
/// source that learns the name before transferring, the authored digest every copy must
/// have), so a caller can decide whether the present file is the same file without
/// replacing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DestinationExists {
    pub path: PathBuf,
    pub sha256: String,
}

impl fmt::Display for DestinationExists {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Download destination already exists: {}",
            self.path.display()
        )
    }
}

impl std::error::Error for DestinationExists {}

/// The [`DestinationExists`] carried by `error`, whether bare or inside a classified
/// [`FetchError`].
pub fn destination_exists(error: &anyhow::Error) -> Option<&DestinationExists> {
    error.chain().find_map(|e| {
        e.downcast_ref::<DestinationExists>().or_else(|| {
            e.downcast_ref::<FetchError>()
                .and_then(|fetch| fetch.error.downcast_ref::<DestinationExists>())
        })
    })
}

/// A manual boundary found before any transfer, e.g. a free Nexus key.
pub fn manual_required(url: &str, reason: ManualReason, message: &str) -> anyhow::Error {
    fail(
        FailureClass::ManualRequired {
            url: url.to_owned(),
            reason,
        },
        anyhow::anyhow!(message.to_owned()),
    )
}

/// A transient failure from a host API, e.g. a rate limit with its `Retry-After` hint.
pub fn transient(retry_after_secs: Option<u64>, error: anyhow::Error) -> anyhow::Error {
    fail(FailureClass::Transient { retry_after_secs }, error)
}

/// Tag a stop as `FailureClass::Cancelled`, so callers that classify errors (for example
/// the download orchestrator's race) see a clean stop rather than an ordinary failure.
/// For adapters whose own cancellation path does not already attach this class.
pub fn cancelled(error: anyhow::Error) -> anyhow::Error {
    fail(FailureClass::Cancelled, error)
}

/// The typed class attached by this module, if any.
pub fn classify(error: &anyhow::Error) -> Option<&FailureClass> {
    error
        .chain()
        .find_map(|e| e.downcast_ref::<FetchError>())
        .map(|e| &e.class)
}

/// Delay before retry number `attempt` (1-based), or `None` when the class is not retryable.
pub fn transient_delay(error: &anyhow::Error, attempt: usize) -> Option<Duration> {
    let Some(FailureClass::Transient { retry_after_secs }) = classify(error) else {
        return None;
    };
    Some(match retry_after_secs {
        Some(secs) => Duration::from_secs(*secs).min(RETRY_AFTER_CAP),
        None => Duration::from_millis(500)
            .saturating_mul(1 << attempt.saturating_sub(1).min(6))
            .min(Duration::from_secs(30)),
    })
}

/// Where a fetch publishes.
#[derive(Clone, Copy, Debug)]
pub enum Destination<'a> {
    /// A known file path. It must not exist.
    File(&'a Path),
    /// A directory. The name is `name` when given, else the server's Content-Disposition
    /// or final URL segment. Either way it must reduce to one normal path component,
    /// checked after the transfer and before publish.
    Directory {
        directory: &'a Path,
        name: Option<&'a str>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchProgress {
    Transferring { bytes: u64, total: Option<u64> },
    Verifying,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub path: PathBuf,
    /// Lower-case hex SHA-256 of the published bytes.
    pub sha256: String,
}

/// Download direct archive URLs only. Host landing pages remain a browser workflow.
pub fn fetch(
    url: &str,
    destination: &Path,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<()> {
    fetch_with_options(
        url,
        destination,
        sha256,
        cancelled,
        &crate::archive::Options::default(),
    )
}

pub fn fetch_with_options(
    url: &str,
    destination: &Path,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
) -> Result<()> {
    fetch_to(
        url,
        Destination::File(destination),
        sha256,
        cancelled,
        options,
        &mut |_| {},
    )
    .map(|_| ())
}

/// Every error returned here carries a [`FailureClass`]; unclassified causes are `Permanent`.
/// The temp file lives in the destination directory and is removed on every error path.
pub fn fetch_to(
    url: &str,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
) -> Result<Fetched> {
    fetch_to_with_loopback_ca(url, destination, sha256, cancelled, options, progress, None)
}

/// As [`fetch_to`], with explicit trust restricted to HTTPS loopback on every hop.
pub fn fetch_to_with_loopback_ca(
    url: &str,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
    loopback_ca: Option<&LoopbackCa>,
) -> Result<Fetched> {
    fetch_to_with_context(
        url,
        destination,
        sha256,
        cancelled,
        options,
        progress,
        &TransferContext::with_loopback_ca(loopback_ca),
    )
}

/// Provider-supplied context for one archive transfer. The default adds nothing.
#[derive(Clone, Default)]
pub struct TransferContext<'a> {
    /// Explicit trust restricted to HTTPS loopback on every hop.
    pub loopback_ca: Option<&'a LoopbackCa>,
    /// `Cookie` for the first request only, for the endpoint that authorized the link.
    pub cookie: Option<String>,
    /// `Referer` for the first request.
    pub referer: Option<String>,
    /// Redirects must stay on the first request's origin, so neither the cookie nor the
    /// referer can be carried to another one.
    pub confine_origin: bool,
}

impl<'a> TransferContext<'a> {
    pub fn with_loopback_ca(loopback_ca: Option<&'a LoopbackCa>) -> Self {
        Self {
            loopback_ca,
            ..Self::default()
        }
    }
}

/// As [`fetch_to_with_loopback_ca`], with a provider's [`TransferContext`]. The direct
/// downloader's HTML, status, hash and archive checks all still apply.
pub fn fetch_to_with_context(
    url: &str,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
    context: &TransferContext<'_>,
) -> Result<Fetched> {
    let started = std::time::Instant::now();
    let mut observed_bytes = 0;
    let mut observed_progress = |update| {
        if let FetchProgress::Transferring { bytes, .. } = update {
            observed_bytes = observed_bytes.max(bytes);
        }
        progress(update);
    };
    let result = fetch_inner(
        url,
        destination,
        sha256,
        cancelled,
        options,
        &mut observed_progress,
        context,
    )
    .map_err(|error| {
        if classify(&error).is_some() {
            error
        } else if cancelled.load(Ordering::Relaxed) {
            fail(FailureClass::Cancelled, error)
        } else {
            fail(FailureClass::Permanent, error)
        }
    });
    let name = result
        .as_ref()
        .ok()
        .and_then(|fetched| fetched.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| match destination {
            Destination::File(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            Destination::Directory { name, .. } => name.unwrap_or("unnamed archive").into(),
        });
    let error = result.as_ref().err().map(|error| format!("{error:#}"));
    crate::telemetry::global().record_download(
        &name,
        result.is_ok(),
        started.elapsed(),
        observed_bytes,
        Some("https"),
        error.as_deref(),
    );
    result
}

fn fetch_inner(
    raw_url: &str,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
    context: &TransferContext<'_>,
) -> Result<Fetched> {
    check_cancelled(cancelled)?;
    let loopback_ca = context.loopback_ca;
    let url = reqwest::Url::parse(raw_url)?;
    ensure!(url.scheme() == "https", "Downloads require HTTPS");
    if loopback_ca.is_some() {
        ensure!(
            eligible_loopback(&url),
            "Loopback CA requires an HTTPS loopback endpoint"
        );
    }
    let directory = match destination {
        Destination::File(path) => {
            ensure!(!path.exists(), "Download destination already exists");
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Choose a destination folder"))?
        }
        Destination::Directory { directory, name } => {
            ensure!(directory.is_dir(), "Choose a destination folder");
            if let Some(name) = name {
                // A provider may reveal a name absent from the original plan.
                // Retain the staged digest so the orchestrator can verify and
                // adopt an existing file without ever overwriting it.
                one_component(name)?;
            }
            directory
        }
    };
    let mut builder = reqwest::blocking::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(IDLE_TIMEOUT);
    if let Some(ca) = loopback_ca {
        builder = builder
            .add_root_certificate(ca.0.clone())
            .no_proxy()
            .resolve("localhost", "127.0.0.1:0".parse().expect("literal address"))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if !eligible_loopback(attempt.url()) {
                    attempt.error("Loopback CA redirect requires an HTTPS loopback endpoint")
                } else if attempt.previous().len() >= 10 {
                    attempt.error("Loopback CA redirect limit exceeded")
                } else {
                    attempt.follow()
                }
            }));
    }
    if context.confine_origin {
        let origin = url.origin();
        let loopback = loopback_ca.is_some();
        builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.url().origin() != origin {
                attempt.error("Redirect leaves the download's origin")
            } else if loopback && !eligible_loopback(attempt.url()) {
                attempt.error("Loopback CA redirect requires an HTTPS loopback endpoint")
            } else if attempt.previous().len() > 5 {
                attempt.error("Too many redirects")
            } else {
                attempt.follow()
            }
        }));
    }
    let client = builder.build()?;
    let mut request = client.get(url);
    if let Some(cookie) = &context.cookie {
        request = request.header(reqwest::header::COOKIE, cookie.as_str());
    }
    if let Some(referer) = &context.referer {
        request = request.header(reqwest::header::REFERER, referer.as_str());
    }
    // Resolved origin links can carry authorization independently of cookies or CA policy.
    // Never retain their URL in transport diagnostics (including redirected CDN URLs).
    check_cancelled(cancelled)?;
    let mut response = request
        .send()
        .map_err(|error| network_failure(error.without_url().into()))?;
    check_cancelled(cancelled)?;
    let final_url = response.url().clone();
    let status = response.status();
    // Read before the prefix: once body bytes are consumed, `content_length` is the
    // remaining length, not the download's total.
    let total = response.content_length();
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase()
    };
    let content_type = header("content-type");
    let html = content_type.contains("text/html") || content_type.contains("application/xhtml");
    let challenge_header = header("cf-mitigated").contains("challenge");
    let retry_after = header("retry-after").trim().parse::<u64>().ok();
    let disposition = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    // Every response's bounded prefix is read before any status decision, so a challenge,
    // login or countdown page is a manual boundary whatever its status or content type.
    // For an archive the prefix is kept and becomes the first bytes of the file.
    let mut prefix = Vec::new();
    if let Err(error) = (&mut response).take(SNIFF_LIMIT).read_to_end(&mut prefix)
        && status.is_success()
    {
        return Err(network_failure(redact_response_read_error(error).into()));
    }
    check_cancelled(cancelled)?;
    let body = text_prefix(&prefix);
    let markers = body
        .as_deref()
        .map_or(ManualReason::HtmlLanding, |body| gate_reason(false, body));
    let page = html
        || challenge_header
        || markers != ManualReason::HtmlLanding
        || body.as_deref().is_some_and(looks_like_page);
    if page {
        let reason = if challenge_header {
            ManualReason::Captcha
        } else {
            markers
        };
        // A dead link's HTML error page is a broken URL, not a gate.
        if reason == ManualReason::HtmlLanding
            && matches!(
                status,
                reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE
            )
        {
            return Err(fail(
                FailureClass::Permanent,
                anyhow::anyhow!("HTTP status {status}"),
            ));
        }
        let message = match reason {
            ManualReason::HtmlLanding => {
                "Server returned a web page. Use the host's browser download flow.".to_owned()
            }
            ManualReason::Captcha => {
                format!("Host requires a browser challenge (HTTP {status}); open it in a browser.")
            }
            ManualReason::LoginRequired => {
                format!("Host requires a login (HTTP {status}); open it in a browser.")
            }
            ManualReason::FreeUserWait => {
                format!(
                    "Host requires a timed or free-user download (HTTP {status}); open it in a browser."
                )
            }
            ManualReason::Unsupported => {
                format!("Host download is not automated (HTTP {status}); open it in a browser.")
            }
        };
        return Err(fail(
            FailureClass::ManualRequired {
                url: raw_url.to_owned(),
                reason,
            },
            anyhow::anyhow!(message),
        ));
    }
    if !status.is_success() {
        let class = if status.is_server_error()
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            FailureClass::Transient {
                retry_after_secs: retry_after,
            }
        } else {
            FailureClass::Permanent
        };
        return Err(fail(class, anyhow::anyhow!("HTTP status {status}")));
    }
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    hash.update(&prefix);
    temp.write_all(&prefix)?;
    let mut bytes = prefix.len() as u64;
    progress(FetchProgress::Transferring { bytes, total });
    loop {
        check_cancelled(cancelled)?;
        let n = response
            .read(&mut buffer)
            .map_err(|error| network_failure(redact_response_read_error(error).into()))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        temp.write_all(&buffer[..n])?;
        bytes += n as u64;
        progress(FetchProgress::Transferring { bytes, total });
    }
    let digest = format!("{:x}", hash.finalize());
    let fallback_name = disposition
        .as_deref()
        .and_then(disposition_filename)
        .or_else(|| {
            final_url
                .path_segments()
                .and_then(|mut s| s.next_back().map(percent_decode))
        });
    publish_staged(
        Staged {
            temp,
            digest,
            fallback_name,
        },
        destination,
        sha256,
        cancelled,
        options,
        progress,
    )
}

/// A finished staging file: its bytes are in `temp` (inside the destination directory),
/// `digest` is their lower-case hex SHA-256 and `fallback_name` names an unnamed
/// [`Destination::Directory`] publish.
pub(crate) struct Staged {
    pub temp: tempfile::NamedTempFile,
    pub digest: String,
    pub fallback_name: Option<String>,
}

/// The shared tail of every transfer: authored hash, one-component name, no-clobber
/// destination, archive verification, then the no-clobber move. Nothing is published on
/// any error, and dropping `staged.temp` removes the staging file.
pub(crate) fn publish_staged(
    staged: Staged,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
) -> Result<Fetched> {
    let Staged {
        temp,
        digest,
        fallback_name,
    } = staged;
    check_cancelled(cancelled)?;
    if let Some(expected) = sha256 {
        ensure!(
            digest.eq_ignore_ascii_case(expected),
            "SHA-256 mismatch; download discarded"
        );
    }
    let path = match destination {
        Destination::File(path) => path.to_path_buf(),
        Destination::Directory { directory, name } => {
            let name = match name {
                Some(name) => name.to_owned(),
                None => fallback_name
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("Server did not name the download"))?,
            };
            let path = directory.join(one_component(&name)?);
            if path.exists() {
                // Never replaced. The caller decides from the digest whether the present
                // file is this download; `temp` is removed on return.
                return Err(anyhow::Error::new(DestinationExists {
                    path,
                    sha256: digest,
                }));
            }
            path
        }
    };
    progress(FetchProgress::Verifying);
    crate::archive::verify(temp.path(), &path, options, cancelled)?;
    temp.as_file().sync_all()?;
    // Verification and syncing can be lengthy. A stop observed at this boundary
    // discards staging rather than publishing the now-cancelled transfer.
    check_cancelled(cancelled)?;
    if let Err(error) = temp.persist_noclobber(&path) {
        // The name was taken while this transfer verified (a concurrent publish): the
        // same outcome as finding it present above, with the finished digest.
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            return Err(anyhow::Error::new(DestinationExists {
                path,
                sha256: digest,
            }));
        }
        return Err(error.error.into());
    }
    Ok(Fetched {
        path,
        sha256: digest,
    })
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(fail(
            FailureClass::Cancelled,
            anyhow::anyhow!("Download cancelled"),
        ))
    } else {
        Ok(())
    }
}

/// The one-component rule for any server- or guide-supplied filename: exactly one normal
/// path component, with the Windows name rules `paths::relative` applies everywhere.
pub fn one_component(name: &str) -> Result<&str> {
    let name = crate::updates::validate_download_file_name(name)?;
    let relative = crate::paths::relative(name, false)?;
    ensure!(
        relative.as_os_str() == name,
        "Downloaded filename {name:?} is not a single file name"
    );
    Ok(name)
}

/// Blocking response reads wrap reqwest errors inside I/O errors. Preserve the I/O
/// kind and reqwest timeout/status metadata while removing any attached response URL.
fn redact_response_read_error(error: std::io::Error) -> std::io::Error {
    let kind = error.kind();
    if error.get_ref().is_none() {
        return error;
    }
    let inner = error.into_inner().expect("I/O error has an inner error");
    let inner = match inner.downcast::<reqwest::Error>() {
        Ok(error) => return std::io::Error::new(kind, error.without_url()),
        Err(inner) => inner,
    };
    match inner.downcast::<std::io::Error>() {
        Ok(error) => std::io::Error::new(kind, redact_response_read_error(*error)),
        Err(inner) => std::io::Error::new(kind, inner),
    }
}

/// Connect/reset/abort/timeout are transient; TLS, DNS and certificate errors are not.
pub(crate) fn network_failure(error: anyhow::Error) -> anyhow::Error {
    let mut transient = false;
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error.as_ref());
    while let Some(e) = source {
        if e.downcast_ref::<reqwest::Error>()
            .is_some_and(|e| e.is_timeout())
        {
            transient = true;
        }
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind::*;
            if matches!(
                io.kind(),
                ConnectionRefused
                    | ConnectionReset
                    | ConnectionAborted
                    | NotConnected
                    | BrokenPipe
                    | TimedOut
                    | UnexpectedEof
            ) {
                transient = true;
            }
        }
        source = e.source();
    }
    fail(
        if transient {
            FailureClass::Transient {
                retry_after_secs: None,
            }
        } else {
            FailureClass::Permanent
        },
        error,
    )
}

/// The lower-cased prefix when it is text (valid UTF-8, allowing a cut final character).
/// Archive formats are binary and never pass this check.
fn text_prefix(prefix: &[u8]) -> Option<String> {
    let text = match std::str::from_utf8(prefix) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&prefix[..error.valid_up_to()]).ok()?
        }
        Err(_) => return None,
    };
    Some(text.to_ascii_lowercase())
}

/// An HTML document whatever the Content-Type header says.
fn looks_like_page(body: &str) -> bool {
    let body = body.trim_start_matches('\u{feff}').trim_start();
    body.starts_with("<!doctype html")
        || body.starts_with("<html")
        || (body.starts_with('<')
            && ["<head", "<body", "<title", "<form", "<script"]
                .iter()
                .any(|tag| body.contains(tag)))
}

/// Classifies a failed host API request (for example Nexus link resolution) with the same
/// transient rules as a transfer: connect/reset/timeout and HTTP 408/429/5xx retry.
pub fn request_failure(error: anyhow::Error) -> anyhow::Error {
    if classify(&error).is_some() {
        return error;
    }
    let status = error
        .chain()
        .find_map(|e| e.downcast_ref::<reqwest::Error>())
        .and_then(reqwest::Error::status);
    match status {
        Some(status)
            if status.is_server_error()
                || status == reqwest::StatusCode::REQUEST_TIMEOUT
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS =>
        {
            fail(
                FailureClass::Transient {
                    retry_after_secs: None,
                },
                error,
            )
        }
        Some(_) => fail(FailureClass::Permanent, error),
        None => network_failure(error),
    }
}

/// Marker-based gate reason for an HTML (or challenge-flagged) response body prefix.
/// The body is already lower-cased. An unexplained page is `HtmlLanding`, itself gated.
pub(crate) fn gate_reason(challenge_header: bool, body: &str) -> ManualReason {
    const CHALLENGE: &[&str] = &[
        "/cdn-cgi/challenge-platform",
        "cf_chl_",
        "cf-turnstile",
        "g-recaptcha",
        "h-captcha",
        "hcaptcha.com",
        "<title>just a moment",
        "attention required! | cloudflare",
        "checking your browser",
    ];
    const LOGIN: &[&str] = &["type=\"password\"", "type='password'", "type=password"];
    const WAIT: &[&str] = &[
        "slow download",
        "id=\"countdown\"",
        "class=\"countdown\"",
        "download will start in",
    ];
    if challenge_header || CHALLENGE.iter().any(|m| body.contains(m)) {
        ManualReason::Captcha
    } else if LOGIN.iter().any(|m| body.contains(m)) {
        ManualReason::LoginRequired
    } else if WAIT.iter().any(|m| body.contains(m)) {
        ManualReason::FreeUserWait
    } else {
        ManualReason::HtmlLanding
    }
}

/// RFC 6266 `filename*` (RFC 5987, UTF-8) wins over `filename`. Parameters are split on
/// `;` outside quoted strings only, so a quoted name may hold `;`: DeadlyStream sends
/// `filename="Emperor Turnip&#39;s Gizka.rar"`, which a plain split cut to
/// `"Emperor Turnip&#39`.
pub(crate) fn disposition_filename(value: &str) -> Option<String> {
    let mut plain = None;
    for part in disposition_parameters(value) {
        let Some((key, raw)) = part.split_once('=') else {
            continue;
        };
        let raw = raw.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "filename*" => {
                let mut pieces = raw.splitn(3, '\'');
                let (charset, _, encoded) = (pieces.next()?, pieces.next()?, pieces.next()?);
                if charset.eq_ignore_ascii_case("utf-8") {
                    return Some(percent_decode(encoded));
                }
            }
            "filename" => {
                plain = Some(
                    raw.strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(raw)
                        .replace("\\\"", "\"")
                        .replace("\\\\", "\\"),
                );
            }
            _ => {}
        }
    }
    plain
}

/// The `;`-separated parts of a Content-Disposition value, trimmed. A `;` inside a
/// quoted string (where `\` escapes the next character) belongs to the value.
fn disposition_parameters(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => {
                parts.push(value[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());
    parts
}

/// Decodes `%XX` before the one-component check, so an encoded separator is still rejected.
pub(crate) fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        let pair = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| hex(bytes[i + 1]).zip(hex(bytes[i + 2])))
            .flatten();
        match pair {
            Some((h, l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
