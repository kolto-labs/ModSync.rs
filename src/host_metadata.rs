//! Provider metadata (F13 H4): the C# `IDownloadHandler.GetFileMetadataAsync` whitelists
//! and the C# `CanonicalJson` hash of them. Fetch execution resolves it (never `plan`,
//! dry-run or guide loading) after successful origin acquisition; the orchestrator
//! carries the result separately and records it only at the frontend commit boundary.
//!
//! | Provider | Keys besides `provider` |
//! | --- | --- |
//! | Nexus | `fileId`, `fileName`, `size`, `uploadedTimestamp`, `md5Hash` |
//! | DeadlyStream | `filePageId`, `changelogId`, `fileId`, `version`, `updated`, `size` |
//! | MEGA | `nodeId`, `hash`, `size`, `mtime`, `name` |
//! | Direct | `url`, `contentLength`, `lastModified`, `etag`, `fileName` |
//! | GameFront | none, and no request is made |
//!
//! Sizes and Unix timestamps are integers; DeadlyStream `updated` is an ISO date string. A field the provider
//! does not state is left out rather than written as `0` or `""` (C# writes zero or an
//! empty string for some): no size or version is ever invented. The MEGA fingerprint
//! (`hash`), the Nexus MD5 (`md5Hash`), the metadata hash and a verified archive's
//! `ContentHashSHA256` are different things and are never substituted for each other.
//! The resolver is read-only and is intended for an explicit metadata operation; it never
//! runs during planning or dry-run acquisition.
use crate::{
    download::{self, LoopbackCa},
    download_hosts::{self, Provider},
    updates::NexusClient,
};
use anyhow::{Result, ensure};
use reqwest::{Url, blocking::Client, header};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};

/// One resource's provider metadata, keyed as C# keys `HandlerMetadata`.
pub type Metadata = Map<String, Value>;

/// Every key a provider writes, `provider` included. Recording fills missing values
/// within this set and preserves authored values and unrelated FOMOD prompt state.
pub const PROVIDER_KEYS: [&str; 18] = [
    "provider",
    "url",
    "contentLength",
    "lastModified",
    "etag",
    "fileName",
    "fileId",
    "size",
    "uploadedTimestamp",
    "md5Hash",
    "filePageId",
    "changelogId",
    "version",
    "updated",
    "nodeId",
    "hash",
    "mtime",
    "name",
];

/// Redirect hops a Direct metadata request follows.
const MAX_REDIRECTS: usize = 5;

/// What one resource's metadata request produced. `metadata` is `None` when C# records
/// nothing (no Nexus key, a URL that is not a provider page, a MEGA link that cannot be
/// read); otherwise it holds at least `provider`. `warning` says why a request failed.
#[derive(Debug, Default)]
pub struct Outcome {
    pub metadata: Option<Metadata>,
    pub warning: Option<String>,
}

impl Outcome {
    fn nothing(warning: Option<String>) -> Self {
        Self {
            metadata: None,
            warning,
        }
    }
}

fn provider_only(key: &str) -> Metadata {
    let mut metadata = Metadata::new();
    metadata.insert("provider".into(), Value::String(key.into()));
    metadata
}

/// C# `CanonicalJson.Serialize`: object keys in ordinal (UTF-16 code unit) order, no
/// whitespace, strings in Unicode NFC with `"`, `\`, `\n`, `\r`, `\t`, `\b`, `\f` escaped
/// and other control characters as `\u00xx`, integers in invariant digits. Provider
/// metadata holds only strings and integers; a fractional number (never produced here)
/// uses Rust's shortest form, where C# uses a trimmed `G17`.
pub fn canonical_json(metadata: &Metadata) -> String {
    let mut out = String::new();
    write_object(metadata, &mut out);
    out
}

/// C# `CanonicalJson.ComputeHash`: lower-case hex SHA-256 of the canonical UTF-8 text.
pub fn metadata_hash(metadata: &Metadata) -> String {
    format!("{:x}", Sha256::digest(canonical_json(metadata).as_bytes()))
}

/// The size C# stores as `FileSize`: `size`, else `contentLength`.
pub fn file_size(metadata: &Metadata) -> Option<u64> {
    metadata
        .get("size")
        .or_else(|| metadata.get("contentLength"))
        .and_then(Value::as_u64)
}

fn write_object(map: &Map<String, Value>, out: &mut String) {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    out.push('{');
    for (index, key) in keys.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_string(key, out);
        out.push(':');
        write_value(&map[key.as_str()], out);
    }
    out.push('}');
}

fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::String(text) => write_string(text, out),
        Value::Number(number) => {
            if let Some(whole) = number.as_i64() {
                out.push_str(&whole.to_string());
            } else if let Some(whole) = number.as_u64() {
                out.push_str(&whole.to_string());
            } else {
                match number.as_f64() {
                    Some(real) if real.is_finite() && real.fract() == 0.0 => {
                        out.push_str(&(real as i64).to_string());
                    }
                    Some(real) if real.is_finite() => out.push_str(&real.to_string()),
                    _ => out.push_str("null"),
                }
            }
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => write_object(map, out),
    }
}

fn write_string(text: &str, out: &mut String) {
    let normalized = icu_normalizer::ComposingNormalizerBorrowed::new_nfc().normalize(text);
    out.push('"');
    for c in normalized.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Resolve the provider metadata for one authored URL.  This is deliberately an
/// additive, read-only operation: a failure leaves the caller's existing metadata
/// untouched and a URL without provider-specific identity produces only `provider`.
pub fn resolve(
    url: &str,
    nexus: Option<&mut NexusClient>,
    loopback_ca: Option<&LoopbackCa>,
    cancelled: &AtomicBool,
) -> Outcome {
    resolve_with_endpoints(url, nexus, None, None, loopback_ca, cancelled)
}

/// Per-invocation provider fixture endpoints; never persisted in resource metadata.
pub fn resolve_with_endpoints(
    url: &str,
    nexus: Option<&mut NexusClient>,
    deadlystream_base: Option<&str>,
    mega_api_base: Option<&str>,
    loopback_ca: Option<&LoopbackCa>,
    cancelled: &AtomicBool,
) -> Outcome {
    let result = (|| -> Result<Metadata> {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Metadata lookup cancelled"
        );
        let provider = download_hosts::provider(url)?;
        let metadata = match provider {
            Provider::Nexus => nexus_metadata(url, nexus, cancelled)?,
            Provider::DeadlyStream => download_hosts::deadlystream::metadata(
                url,
                deadlystream_base,
                loopback_ca,
                cancelled,
            )?,
            Provider::Mega => {
                let link = download_hosts::mega::parse_link(url)?;
                download_hosts::mega::resolve(
                    &link,
                    url,
                    true,
                    mega_api_base,
                    loopback_ca,
                    cancelled,
                )?
                .metadata()
                .clone()
            }
            Provider::GameFront => provider_only("gamefront"),
            Provider::Direct => direct_metadata(url, loopback_ca, cancelled)?,
        };
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Metadata lookup cancelled"
        );
        Ok(metadata)
    })();
    match result {
        Ok(metadata) => Outcome {
            metadata: Some(metadata),
            warning: None,
        },
        // reqwest errors must not retain a token-bearing URL. Provider failures can
        // contain manual URLs as their source; retain only the safe outer message.
        Err(_) => Outcome::nothing(Some(
            if cancelled.load(Ordering::Relaxed) {
                "Metadata lookup cancelled"
            } else {
                "Provider metadata unavailable; existing registry values preserved"
            }
            .into(),
        )),
    }
}

fn nexus_metadata(
    url: &str,
    client: Option<&mut NexusClient>,
    cancelled: &AtomicBool,
) -> Result<Metadata> {
    let parsed = Url::parse(url)?;
    let parts: Vec<_> = parsed
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    ensure!(
        parts.len() >= 3 && parts[1].eq_ignore_ascii_case("mods"),
        "Nexus URL is not a mod page"
    );
    let game = parts[0];
    ensure!(
        !game.is_empty()
            && game
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "Invalid Nexus game domain"
    );
    let mod_id = parts[2].parse::<u64>()?;
    let Some((_, file_id)) = parsed
        .query_pairs()
        .find(|(key, _)| key.eq_ignore_ascii_case("file_id"))
    else {
        return Ok(provider_only("nexus"));
    };
    let file_id = file_id.parse::<u64>()?;
    let client = client.ok_or_else(|| anyhow::anyhow!("Nexus metadata requires an API key"))?;
    client.defer_rate_limits();
    let file = client.get_mod_file_with_cancel(
        game,
        &mod_id.to_string(),
        &file_id.to_string(),
        cancelled,
    )?;
    let mut metadata = provider_only("nexus");
    metadata.insert("fileId".into(), file_id.to_string().into());
    if let Some(name) = file.file_name.filter(|name| !name.is_empty()) {
        download::one_component(&name)?;
        metadata.insert("fileName".into(), name.into());
    }
    if let Some(size) = file
        .size_kb
        .and_then(|n| u64::try_from(n).ok())
        .and_then(|n| n.checked_mul(1024))
    {
        metadata.insert("size".into(), size.into());
    }
    if let Some(timestamp) = file.uploaded_timestamp.filter(|n| *n >= 0) {
        metadata.insert("uploadedTimestamp".into(), timestamp.into());
    }
    if let Some(md5) = file.md5.filter(|v| !v.is_empty()) {
        metadata.insert("md5Hash".into(), md5.to_ascii_lowercase().into());
    }
    Ok(metadata)
}

fn direct_metadata(url: &str, ca: Option<&LoopbackCa>, cancelled: &AtomicBool) -> Result<Metadata> {
    let mut parsed = Url::parse(url)?;
    ensure!(
        matches!(parsed.scheme(), "http" | "https")
            && parsed.username().is_empty()
            && parsed.password().is_none(),
        "Direct metadata requires an HTTP(S) URL without credentials"
    );
    parsed.set_fragment(None);
    let client = metadata_client(&parsed, ca)?;
    let head = client
        .head(parsed.clone())
        .timeout(std::time::Duration::from_secs(20))
        .send();
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Metadata lookup cancelled"
    );
    let response = match head {
        Ok(reply) if reply.status().is_success() => reply,
        // A detected gate is terminal. Only unsupported HEAD or a network rejection
        // permits the bounded headers-only Range fallback.
        Ok(reply) if matches!(reply.status().as_u16(), 400 | 405 | 501) => client
            .get(parsed.clone())
            .timeout(std::time::Duration::from_secs(20))
            .header(header::RANGE, "bytes=0-0")
            .send()
            .map_err(|e| e.without_url())?,
        Ok(reply) => anyhow::bail!("Direct metadata returned HTTP {}", reply.status()),
        Err(_) => client
            .get(parsed.clone())
            .timeout(std::time::Duration::from_secs(20))
            .header(header::RANGE, "bytes=0-0")
            .send()
            .map_err(|e| e.without_url())?,
    };
    ensure!(
        response.status().is_success(),
        "Direct metadata returned HTTP {}",
        response.status()
    );
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Metadata lookup cancelled"
    );
    let mut metadata = provider_only("direct");
    metadata.insert(
        "url".into(),
        crate::resources::normalized_url(parsed.as_str()).into(),
    );
    let headers = response.headers();
    ensure!(
        !headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|value| {
                let value = value.to_ascii_lowercase();
                value.contains("text/html") || value.contains("application/xhtml")
            })
            && !headers
                .get("cf-mitigated")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.eq_ignore_ascii_case("challenge")),
        "Direct metadata returned a web page or challenge"
    );
    let name = headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(download::disposition_filename)
        .or_else(|| {
            response
                .url()
                .path_segments()
                .and_then(|mut s| s.next_back())
                .filter(|s| !s.is_empty())
                .map(download::percent_decode)
        });
    if let Some(name) = name {
        download::one_component(&name)?;
        metadata.insert("fileName".into(), name.into());
    }
    let length = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        headers
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .filter(|v| v.starts_with("bytes "))
            .and_then(|v| v.rsplit_once('/'))
            .and_then(|(_, total)| total.parse::<u64>().ok())
    } else {
        headers
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
    };
    if let Some(length) = length {
        metadata.insert("contentLength".into(), length.into());
    }
    if let Some(value) = headers
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
    {
        metadata.insert("lastModified".into(), value.into());
    }
    if let Some(value) = headers.get(header::ETAG).and_then(|v| v.to_str().ok()) {
        metadata.insert("etag".into(), value.trim_matches('"').into());
    }
    Ok(metadata)
}

fn metadata_client(url: &Url, ca: Option<&LoopbackCa>) -> Result<Client> {
    ensure!(
        ca.is_none() || download::eligible_loopback(url),
        "Metadata extra CA requires a loopback HTTPS endpoint"
    );
    let origin = url.origin();
    download::client_with_policy(
        ca,
        ca.is_some(),
        reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.url().origin() != origin
                || attempt.previous().len() >= MAX_REDIRECTS
                || !attempt.url().username().is_empty()
                || attempt.url().password().is_some()
            {
                attempt.error("metadata redirect is outside the original origin")
            } else {
                attempt.follow()
            }
        }),
    )
}
