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

fn fail(class: FailureClass, error: impl Into<anyhow::Error>) -> anyhow::Error {
    anyhow::Error::new(FetchError {
        class,
        error: error.into(),
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
    fetch_inner(url, destination, sha256, cancelled, options, progress).map_err(|error| {
        if classify(&error).is_some() {
            error
        } else if cancelled.load(Ordering::Relaxed) {
            fail(FailureClass::Cancelled, error)
        } else {
            fail(FailureClass::Permanent, error)
        }
    })
}

fn fetch_inner(
    raw_url: &str,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
) -> Result<Fetched> {
    let url = reqwest::Url::parse(raw_url)?;
    ensure!(url.scheme() == "https", "Downloads require HTTPS");
    let directory = match destination {
        Destination::File(path) => {
            ensure!(!path.exists(), "Download destination already exists");
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Choose a destination folder"))?
        }
        Destination::Directory { directory, name } => {
            ensure!(directory.is_dir(), "Choose a destination folder");
            if let Some(name) = name {
                ensure!(
                    !directory.join(one_component(name)?).exists(),
                    "Download destination already exists"
                );
            }
            directory
        }
    };
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(IDLE_TIMEOUT)
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| network_failure(e.into()))?;
    let final_url = response.url().clone();
    let status = response.status();
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
        return Err(network_failure(error.into()));
    }
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
    let total = response.content_length();
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    hash.update(&prefix);
    temp.write_all(&prefix)?;
    let mut bytes = prefix.len() as u64;
    progress(FetchProgress::Transferring { bytes, total });
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(fail(
                FailureClass::Cancelled,
                anyhow::anyhow!("Download cancelled"),
            ));
        }
        let n = response
            .read(&mut buffer)
            .map_err(|e| network_failure(e.into()))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        temp.write_all(&buffer[..n])?;
        bytes += n as u64;
        progress(FetchProgress::Transferring { bytes, total });
    }
    let digest = format!("{:x}", hash.finalize());
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
                None => disposition
                    .as_deref()
                    .and_then(disposition_filename)
                    .or_else(|| {
                        final_url
                            .path_segments()
                            .and_then(|mut s| s.next_back().map(percent_decode))
                    })
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("Server did not name the download"))?,
            };
            let path = directory.join(one_component(&name)?);
            ensure!(!path.exists(), "Download destination already exists");
            path
        }
    };
    progress(FetchProgress::Verifying);
    crate::archive::verify(temp.path(), &path, options, cancelled)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(&path).map_err(|e| e.error)?;
    Ok(Fetched {
        path,
        sha256: digest,
    })
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

/// Connect/reset/abort/timeout are transient; TLS, DNS and certificate errors are not.
fn network_failure(error: anyhow::Error) -> anyhow::Error {
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
fn gate_reason(challenge_header: bool, body: &str) -> ManualReason {
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

/// RFC 6266 `filename*` (RFC 5987, UTF-8) wins over `filename`.
fn disposition_filename(value: &str) -> Option<String> {
    let mut plain = None;
    for part in value.split(';').map(str::trim) {
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

/// Decodes `%XX` before the one-component check, so an encoded separator is still rejected.
fn percent_decode(text: &str) -> String {
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
