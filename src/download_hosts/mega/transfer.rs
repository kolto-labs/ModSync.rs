//! Anonymous public-file transfer (host-providers plan, slice H3).
//!
//! The `mega` crate cannot use this project's TLS stack, so the public-file protocol runs
//! here over the shared blocking rustls client. The crate is used only for its tested
//! `compute_condensed_mac`, which decides whether the decrypted bytes may be published.
//!
//! Protocol: `POST <api>/cs?id=<n>` with `[{"a":"g","g":1,"ssl":2,"p":"<handle>"}]` returns
//! `[{"s": size, "at": encrypted attributes, "g": download url}]` or a negative error code
//! (bare, or as the only array item). The 32 byte link key folds to the AES key
//! (`k[0..16] ^ k[16..32]`), the CTR nonce (`k[16..24]`) and the meta MAC (`k[24..32]`).
//! Attributes are AES-128-CBC with a zero IV and begin with `MEGA{`. The file is
//! AES-128-CTR (nonce, then a zero 64-bit big-endian counter).
//!
//! Confinement: production talks only to `g.api.mega.co.nz` and to MEGA storage hosts
//! (`*.userstorage.mega.co.nz` or `*.userstorage.mega.nz`) over https. A fixture base needs
//! the run's loopback CA, and then every request, the download address included, must be
//! https loopback with proxies off. Redirects never leave the origin of the request that
//! returned them. Neither the key nor the download address appears in an error, `Debug`
//! or event.
//! Source only; not built or exercised.
use super::MegaLink;
use crate::{
    acquisition::ManualReason,
    download::{self, Destination, FailureClass, FetchProgress, LoopbackCa, Staged},
};
use aes::{
    Aes128,
    cipher::{BlockDecrypt, KeyInit, KeyIvInit, StreamCipher, generic_array::GenericArray},
};
use anyhow::{Result, anyhow, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{StatusCode, Url, blocking::Client, header};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
};

/// The production API request endpoint.
const PRODUCTION_API: &str = "https://g.api.mega.co.nz/cs";
/// Storage host suffixes MEGA serves file bytes from (a label must precede them).
const STORAGE_SUFFIXES: [&str; 2] = [".userstorage.mega.co.nz", ".userstorage.mega.nz"];
/// Largest API reply read; it is a few hundred bytes of JSON.
const MAX_API_BYTES: u64 = 1024 * 1024;
/// Longest encrypted attribute string accepted (base64 characters).
const MAX_ATTRIBUTE_CHARS: usize = 64 * 1024;
/// Redirect hops followed, each on the origin that returned it.
const MAX_REDIRECTS: usize = 3;

/// A resolved public file: its safe name and size, the vetted storage address and the
/// key material. `Debug` shows the name and size only.
pub struct Resolved {
    /// A safe single file name from the decrypted node attributes. `None` only when the
    /// caller did not need a name (the guide supplies the file name) and the node's name
    /// was unusable.
    pub name: Option<String>,
    /// The file size in bytes, from the API. Zero is a valid (empty) file.
    pub size: u64,
    /// The authored link a person can open, for a manual requirement. It may hold the
    /// key, so it appears only in `ManualRequired` (display redaction applies), never in
    /// an error message.
    manual_url: String,
    download: Url,
    client: Client,
    key: [u8; 16],
    iv: [u8; 8],
    mac: [u8; 8],
}

impl fmt::Debug for Resolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resolved")
            .field("name", &self.name)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

fn permanent(message: impl Into<String>) -> anyhow::Error {
    download::fail(
        FailureClass::Permanent,
        anyhow::anyhow!("{}", message.into()),
    )
}

fn transient(retry_after_secs: Option<u64>, message: impl Into<String>) -> anyhow::Error {
    download::transient(retry_after_secs, anyhow::anyhow!("{}", message.into()))
}

/// A body read that failed. Only a kind is kept, because the wrapped transport error can
/// name the download address. A reqwest read timeout arrives as `ErrorKind::Other` around
/// a `reqwest::Error`, so the source chain is searched for it and mapped to `TimedOut`
/// (transient); resets and other kinds pass through unchanged.
fn read_failure(error: std::io::Error) -> anyhow::Error {
    let mut kind = error.kind();
    let mut source: Option<&(dyn std::error::Error + 'static)> = error
        .get_ref()
        .map(|inner| inner as &(dyn std::error::Error + 'static));
    while let Some(cause) = source {
        if cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_timeout)
            || cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::TimedOut)
        {
            kind = std::io::ErrorKind::TimedOut;
        }
        source = cause.source();
    }
    download::network_failure(
        std::io::Error::new(kind, "The MEGA connection was interrupted").into(),
    )
}

fn cancelled_error() -> anyhow::Error {
    download::fail(FailureClass::Cancelled, anyhow!("Download cancelled"))
}

/// A MEGA API error code as a typed failure. Quota and access restrictions are manual
/// boundaries (no Peer fallback around them); retryable codes are transient; the rest are
/// permanent and their messages differ, so a deleted node, a blocked one and a bad key
/// stay distinguishable.
fn api_failure(code: i64, manual_url: &str) -> anyhow::Error {
    match code {
        -1 => transient(None, "MEGA reported an internal error (-1)"),
        -3 => transient(None, "MEGA asked to retry the request (-3)"),
        -4 => transient(None, "MEGA rate limit reached (-4)"),
        -6 | -19 => transient(None, "MEGA reports too many connections to this file"),
        -18 => transient(
            None,
            "MEGA reports the file is temporarily unavailable (-18)",
        ),
        -17 => download::manual_required(
            manual_url,
            ManualReason::FreeUserWait,
            "MEGA transfer quota exceeded (-17); wait and retry, or download it in a browser.",
        ),
        -11 => download::manual_required(
            manual_url,
            ManualReason::LoginRequired,
            "MEGA denied access to this file (-11); open the link in a browser.",
        ),
        -9 => permanent("The MEGA file was not found (-9); the link may have been deleted."),
        -16 => permanent("The MEGA file is blocked or was taken down (-16)."),
        -14 => permanent("MEGA rejected the decryption key (-14)."),
        -2 => permanent("MEGA rejected the request as malformed (-2)."),
        other => permanent(format!("MEGA reported error {other}.")),
    }
}

/// An HTTP status from MEGA. 509 is MEGA's bandwidth limit (a manual boundary); other
/// server errors, 408 and 429 are transient; the rest are permanent.
fn status_failure(
    status: StatusCode,
    retry_after_secs: Option<u64>,
    manual_url: &str,
    what: &str,
) -> anyhow::Error {
    if status.as_u16() == 509 {
        return download::manual_required(
            manual_url,
            ManualReason::FreeUserWait,
            "MEGA bandwidth limit reached (HTTP 509); wait and retry, or download it in a browser.",
        );
    }
    if status.is_server_error()
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
    {
        return transient(retry_after_secs, format!("{what} returned HTTP {status}"));
    }
    permanent(format!("{what} returned HTTP {status}"))
}

fn retry_after(response: &reqwest::blocking::Response) -> Option<u64> {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
}

/// Follows redirects that stay on the origin of the request that returned them.
fn same_origin_policy(url: &Url) -> reqwest::redirect::Policy {
    let origin = url.origin();
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.url().origin() != origin {
            attempt.error("MEGA redirect leaves the request's origin")
        } else if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error("Too many MEGA redirects")
        } else {
            attempt.follow()
        }
    })
}

/// The API request URL: production, or the fixture base (https loopback, with the CA).
fn api_endpoint(api_base: Option<&str>, loopback_ca: Option<&LoopbackCa>) -> Result<Url> {
    let Some(base) = api_base else {
        return Ok(Url::parse(PRODUCTION_API)?);
    };
    ensure!(
        loopback_ca.is_some(),
        "A MEGA fixture base requires a loopback CA"
    );
    let mut url = Url::parse(base.trim())
        .map_err(|_| anyhow!("The MEGA fixture base is not a valid https loopback address"))?;
    ensure!(
        download::eligible_loopback(&url)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "The MEGA fixture base must be an https loopback address"
    );
    let path = format!("{}/cs", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

/// Whether `url` may serve file bytes: a fixture run needs https loopback; production
/// needs https on a MEGA storage host on the default port.
fn storage_admitted(url: &Url, fixture: bool) -> bool {
    if fixture {
        return download::eligible_loopback(url);
    }
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|host| {
            let host = host.trim_end_matches('.').to_ascii_lowercase();
            STORAGE_SUFFIXES.iter().any(|suffix| host.ends_with(suffix))
        })
}

/// Folds the 32 byte link key into the AES key, CTR nonce and meta MAC. Error text never
/// includes the key or the decoder's report about it.
fn split_key(key: &str) -> Result<([u8; 16], [u8; 8], [u8; 8])> {
    let bytes = URL_SAFE_NO_PAD
        .decode(key)
        .map_err(|_| anyhow!("MEGA link has an invalid decryption key"))?;
    ensure!(bytes.len() == 32, "MEGA link has an invalid decryption key");
    let mut aes_key = [0u8; 16];
    for (out, (a, b)) in aes_key.iter_mut().zip(bytes[..16].iter().zip(&bytes[16..])) {
        *out = a ^ b;
    }
    let mut iv = [0u8; 8];
    iv.copy_from_slice(&bytes[16..24]);
    let mut mac = [0u8; 8];
    mac.copy_from_slice(&bytes[24..32]);
    Ok((aes_key, iv, mac))
}

/// Decrypts the node attributes (AES-128-CBC, zero IV) and returns the node name. A wrong
/// key or corrupted attributes are an error, never a panic.
fn node_name(aes_key: &[u8; 16], encoded: &str) -> Result<String> {
    const BAD: &str =
        "MEGA node attributes could not be decrypted (wrong key or corrupted attributes)";
    ensure!(encoded.len() <= MAX_ATTRIBUTE_CHARS, BAD);
    let mut buffer = URL_SAFE_NO_PAD
        .decode(encoded.trim_end_matches('='))
        .map_err(|_| anyhow!(BAD))?;
    ensure!(!buffer.is_empty() && buffer.len().is_multiple_of(16), BAD);
    let cipher = Aes128::new(GenericArray::from_slice(aes_key));
    let mut previous = [0u8; 16];
    for chunk in buffer.chunks_exact_mut(16) {
        let mut cipher_block = [0u8; 16];
        cipher_block.copy_from_slice(chunk);
        cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
        for (byte, mask) in chunk.iter_mut().zip(previous) {
            *byte ^= mask;
        }
        previous = cipher_block;
    }
    ensure!(buffer.starts_with(b"MEGA{"), BAD);
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    let attributes: serde_json::Value =
        serde_json::from_slice(&buffer[4..end]).map_err(|_| anyhow!(BAD))?;
    let name = attributes
        .get("n")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("MEGA node attributes carry no file name"))?;
    ensure!(
        download::one_component(name).is_ok(),
        "The MEGA file name is not a safe single file name"
    );
    Ok(name.to_owned())
}

/// Resolves a public file link anonymously: one API request, then key and attribute
/// decryption, then the vetted storage address. Nothing is downloaded. A quota or access
/// restriction is `ManualRequired` (its URL is `manual_url`, the authored link), a busy API
/// is `Transient`, a deleted, blocked or undecryptable node is `Permanent`. `api_base` is
/// the per-invocation fixture base (https loopback, needs `loopback_ca`); `None` is
/// production. `need_name` is false when the guide names the file: a bad node name then
/// does not fail the resolve (it is validated whenever it will be used).
pub fn resolve(
    link: &MegaLink,
    manual_url: &str,
    need_name: bool,
    api_base: Option<&str>,
    loopback_ca: Option<&LoopbackCa>,
    cancelled: &AtomicBool,
) -> Result<Resolved> {
    let manual_url = manual_url.to_owned();
    let fixture = api_base.is_some();
    let loopback_ca = api_base.and(loopback_ca);
    let (aes_key, iv, mac) = split_key(link.key())?;
    let mut endpoint = api_endpoint(api_base, loopback_ca)?;
    let sequence = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.subsec_nanos() & 0x7fff_ffff);
    endpoint
        .query_pairs_mut()
        .append_pair("id", &sequence.to_string());
    let client = download::client_with_policy(loopback_ca, fixture, same_origin_policy(&endpoint))?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    let body = serde_json::json!([{ "a": "g", "g": 1, "ssl": 2, "p": link.handle() }]).to_string();
    let mut response = client
        .post(endpoint)
        .header(header::CONTENT_TYPE, "text/plain;charset=UTF-8")
        .body(body)
        .send()
        .map_err(|error| download::network_failure(error.without_url().into()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(status_failure(
            status,
            retry_after(&response),
            &manual_url,
            "The MEGA API",
        ));
    }
    let mut reply = Vec::new();
    (&mut response)
        .take(MAX_API_BYTES + 1)
        .read_to_end(&mut reply)
        .map_err(read_failure)?;
    if reply.len() as u64 > MAX_API_BYTES {
        return Err(permanent("The MEGA API reply is too large"));
    }
    let value: serde_json::Value = serde_json::from_slice(&reply)
        .map_err(|_| permanent("The MEGA API returned an unreadable reply"))?;
    let entry = match &value {
        serde_json::Value::Array(items) => items.first(),
        other => Some(other),
    }
    .ok_or_else(|| permanent("The MEGA API returned an empty reply"))?;
    if let Some(code) = entry.as_i64() {
        return Err(if code < 0 {
            api_failure(code, &manual_url)
        } else {
            permanent("The MEGA API returned an unexpected reply")
        });
    }
    // A negative `e` wins even when a `g` is also present.
    if let Some(code) = entry.get("e").and_then(serde_json::Value::as_i64)
        && code < 0
    {
        return Err(api_failure(code, &manual_url));
    }
    let Some(download_address) = entry.get("g").and_then(serde_json::Value::as_str) else {
        return Err(permanent("The MEGA API reply names no download address"));
    };
    let size = entry
        .get("s")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| permanent("The MEGA API reply names no file size"))?;
    let encoded = entry
        .get("at")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| permanent("The MEGA API reply carries no node attributes"))?;
    let name = match node_name(&aes_key, encoded) {
        Ok(name) => Some(name),
        Err(error) if need_name => return Err(permanent(error.to_string())),
        Err(_) => None,
    };

    let mut storage = Url::parse(download_address)
        .map_err(|_| permanent("MEGA returned an invalid download address"))?;
    if !storage_admitted(&storage, fixture) {
        return Err(permanent(
            "MEGA returned a download address outside its storage hosts",
        ));
    }
    // The range suffix MEGA storage expects, appended to the path so a query survives.
    storage
        .path_segments_mut()
        .map_err(|_| permanent("MEGA returned an invalid download address"))?
        .pop_if_empty()
        .push(&format!("0-{size}"));
    // reqwest 0.12.28's blocking builder has no `read_timeout`, but its blocking
    // `Response::read` applies the client timeout afresh to every read call, so the shared
    // `IDLE_TIMEOUT` already bounds a stall between bytes, not the whole transfer.
    let client = download::client_with_policy(loopback_ca, fixture, same_origin_policy(&storage))?;
    Ok(Resolved {
        name,
        size,
        manual_url,
        download: storage,
        client,
        key: aes_key,
        iv,
        mac,
    })
}

/// Cancellation between reads of the integrity pass, which the crate cannot check itself.
struct CancelRead<'a, R> {
    inner: R,
    cancelled: &'a AtomicBool,
}

impl<R: Read> Read for CancelRead<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("cancelled"));
        }
        self.inner.read(buffer)
    }
}

/// Downloads and decrypts the resolved file into staging beside the destination, checks
/// the size and condensed MAC, and only then publishes through the shared no-clobber
/// path. Every error removes the staging file and publishes nothing. Errors carry a
/// `FailureClass`: a stop is `Cancelled`, a short or interrupted transfer `Transient`, a
/// bandwidth limit `ManualRequired`, and a bad MAC or size `Permanent`.
pub fn transfer(
    resolved: &Resolved,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
) -> Result<download::Fetched> {
    transfer_inner(resolved, destination, sha256, cancelled, options, progress).map_err(|error| {
        if download::classify(&error).is_some() {
            error
        } else if cancelled.load(Ordering::Relaxed) {
            download::fail(FailureClass::Cancelled, error)
        } else {
            download::fail(FailureClass::Permanent, error)
        }
    })
}

fn transfer_inner(
    resolved: &Resolved,
    destination: Destination<'_>,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    progress: &mut dyn FnMut(FetchProgress),
) -> Result<download::Fetched> {
    let directory = match destination {
        Destination::File(path) => {
            ensure!(!path.exists(), "Download destination already exists");
            path.parent()
                .ok_or_else(|| anyhow!("Choose a destination folder"))?
        }
        Destination::Directory { directory, name } => {
            ensure!(directory.is_dir(), "Choose a destination folder");
            if let Some(name) = name {
                ensure!(
                    !directory.join(download::one_component(name)?).exists(),
                    "Download destination already exists"
                );
            }
            directory
        }
    };
    if cancelled.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    let mut response = resolved
        .client
        .get(resolved.download.clone())
        .send()
        .map_err(|error| download::network_failure(error.without_url().into()))?;
    let status = response.status();
    // The status is decided before a byte is read: a 509 or an error page is never data.
    if !status.is_success() {
        return Err(status_failure(
            status,
            retry_after(&response),
            &resolved.manual_url,
            "MEGA storage",
        ));
    }
    let is_page = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/html"));
    ensure!(!is_page, "MEGA storage returned a web page, not file data");

    let counter_block = {
        let mut block = [0u8; 16];
        block[..8].copy_from_slice(&resolved.iv);
        block
    };
    let mut ctr = ctr::Ctr128BE::<Aes128>::new(
        GenericArray::from_slice(&resolved.key),
        GenericArray::from_slice(&counter_block),
    );
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut bytes = 0u64;
    let total = Some(resolved.size);
    progress(FetchProgress::Transferring { bytes, total });
    while bytes < resolved.size {
        if cancelled.load(Ordering::Relaxed) {
            return Err(cancelled_error());
        }
        let want = usize::try_from((resolved.size - bytes).min(buffer.len() as u64))
            .unwrap_or(buffer.len());
        let read = response.read(&mut buffer[..want]).map_err(read_failure)?;
        if read == 0 {
            return Err(transient(
                None,
                format!(
                    "The MEGA download ended early: {bytes} of {} bytes",
                    resolved.size
                ),
            ));
        }
        let chunk = &mut buffer[..read];
        ctr.apply_keystream(chunk);
        hash.update(&*chunk);
        temp.write_all(chunk)?;
        bytes += read as u64;
        progress(FetchProgress::Transferring { bytes, total });
    }
    // The body must end at `size`: a longer one is never silently truncated.
    let mut extra = [0u8; 1];
    if response.read(&mut extra).map_err(read_failure)? > 0 {
        return Err(permanent(
            "The MEGA download is longer than the size MEGA reported",
        ));
    }
    temp.flush()?;
    drop(response);

    // Integrity: the condensed MAC of the plaintext must equal the link's meta MAC.
    let reader = CancelRead {
        inner: std::io::BufReader::new(temp.reopen()?),
        cancelled,
    };
    let computed = futures::executor::block_on(::mega::compute_condensed_mac(
        futures::io::AllowStdIo::new(reader),
        resolved.size,
        &resolved.key,
        &resolved.iv,
    ));
    let computed = match computed {
        Ok(computed) => computed,
        Err(_) if cancelled.load(Ordering::Relaxed) => return Err(cancelled_error()),
        Err(_) => {
            return Err(permanent(
                "The MEGA integrity check could not read the download",
            ));
        }
    };
    if computed != resolved.mac {
        return Err(permanent(
            "The MEGA integrity check failed: the bytes do not match the link's key (altered download or wrong key); nothing was saved",
        ));
    }
    download::publish_staged(
        Staged {
            temp,
            digest: format!("{:x}", hash.finalize()),
            fallback_name: resolved.name.clone(),
        },
        destination,
        sha256,
        cancelled,
        options,
        progress,
    )
}
