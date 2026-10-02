//! Shared OTLP endpoint admission for readiness and transport routing.
use reqwest::Url;

const SIGNAL_SUFFIXES: [&str; 2] = ["/v1/traces", "/v1/metrics"];

pub(super) fn base(endpoint: &str) -> Result<String, &'static str> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err("An OTLP endpoint is required for remote telemetry.");
    }
    if endpoint.len() > 8192 {
        return Err("The OTLP endpoint exceeds the 8192-byte limit.");
    }
    let mut url = Url::parse(endpoint).map_err(|_| "The OTLP endpoint is not a valid URL.")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("The OTLP endpoint must be an absolute HTTP or HTTPS URL.");
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("The OTLP endpoint cannot contain credentials, a query or a fragment.");
    }
    let path = url.path().trim_end_matches('/');
    let prefix = SIGNAL_SUFFIXES
        .iter()
        .find_map(|suffix| path.strip_suffix(suffix));
    // Detect alternate reserved spellings without using their decoded spelling
    // to route requests. Arbitrary encoded prefix bytes remain untouched.
    let decoded = decoded_ascii_lowercase(path.as_bytes());
    if prefix.is_none()
        && SIGNAL_SUFFIXES
            .iter()
            .any(|suffix| decoded.ends_with(suffix.as_bytes()))
    {
        return Err("An OTLP signal suffix must use literal /v1/traces or /v1/metrics.");
    }
    let prefix = prefix.unwrap_or(path).trim_end_matches('/');
    let decoded_prefix = decoded_ascii_lowercase(prefix.as_bytes());
    if SIGNAL_SUFFIXES
        .iter()
        .any(|suffix| decoded_prefix.ends_with(suffix.as_bytes()))
    {
        return Err("The OTLP endpoint contains repeated or ambiguous signal suffixes.");
    }
    let prefix = prefix.to_owned();
    url.set_path(&prefix);
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

/// One decoding pass solely for reserved-suffix refusal, never URL construction.
fn decoded_ascii_lowercase(bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == b'%'
            && let Some(pair) = bytes.get(position + 1..position + 3)
            && let (Some(high), Some(low)) = (
                char::from(pair[0]).to_digit(16),
                char::from(pair[1]).to_digit(16),
            )
        {
            result.push(((high * 16 + low) as u8).to_ascii_lowercase());
            position += 3;
        } else {
            result.push(bytes[position].to_ascii_lowercase());
            position += 1;
        }
    }
    while result.last() == Some(&b'/') {
        result.pop();
    }
    result
}
