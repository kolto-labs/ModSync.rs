//! MEGA public single-file links: our own parsing, normalization and redaction.
//!
//! The decryption key of a MEGA link travels in the URL fragment, so it is a
//! secret. Nothing here puts a key into an error, `Debug` output or a report;
//! use [`redact`] or [`MegaLink::redact_in`] before text that may hold a link
//! leaves the process.
//!
//! Folder links are recognized and rejected: folder traversal is unsupported. The
//! `transfer` module resolves and decrypts a single public file (slice H3).

use std::fmt;

mod transfer;
pub use transfer::{Resolved, resolve, transfer};

/// Why a MEGA link could not be used. The messages never contain link text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MegaLinkError {
    /// Not an http(s) link on a MEGA domain.
    NotMegaLink,
    /// A folder link or a file inside a folder.
    FolderUnsupported,
    /// A password-protected link.
    PasswordProtectedUnsupported,
    /// A MEGA path this parser does not recognize.
    Unrecognized,
    /// The link carries no decryption key.
    MissingKey,
    /// The file handle is not 8 base64url characters.
    InvalidHandle,
    /// The key is not a 43 character base64url file key.
    InvalidKey,
}

impl fmt::Display for MegaLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotMegaLink => "not a MEGA link",
            Self::FolderUnsupported => "MEGA folder links are not supported",
            Self::PasswordProtectedUnsupported => "password-protected MEGA links are not supported",
            Self::Unrecognized => "unrecognized MEGA link form",
            Self::MissingKey => "MEGA link has no decryption key",
            Self::InvalidHandle => "MEGA link has an invalid file handle",
            Self::InvalidKey => "MEGA link has an invalid decryption key",
        })
    }
}

impl std::error::Error for MegaLinkError {}

/// A validated public single-file link. The key is secret: `Debug` hides it and
/// there is no `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct MegaLink {
    handle: String,
    key: String,
}

impl fmt::Debug for MegaLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MegaLink")
            .field("handle", &self.handle)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl MegaLink {
    /// The 8 character public file handle.
    pub fn handle(&self) -> &str {
        &self.handle
    }

    /// The 43 character base64url file key. Secret: never log or return it.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// `https://mega.nz/file/<handle>#<key>`, the only form the MEGA client accepts.
    /// Contains the key; pass it to the client and nowhere else.
    pub fn canonical_url(&self) -> String {
        format!("https://mega.nz/file/{}#{}", self.handle, self.key)
    }

    /// The canonical link with the key removed, safe for diagnostics.
    pub fn redacted_url(&self) -> String {
        format!("https://mega.nz/file/{}#<redacted>", self.handle)
    }

    /// Replaces this link's key wherever it appears in `text`, then applies [`redact`].
    pub fn redact_in(&self, text: &str) -> String {
        redact(&text.replace(self.key.as_str(), "<redacted>"))
    }
}

/// Parses and normalizes a MEGA public file link.
///
/// Accepts `http`/`https`, optional `www.` (any subdomain of `mega.nz` or
/// `mega.co.nz`), surrounding whitespace, a missing scheme, percent-encoded
/// fragments (`%23`, `%21`), trailing `/`, `?` or `&` noise after the key and
/// extra path segments after the handle, in these shapes:
///
/// - `/file/<handle>#<key>`
/// - `/embed/<handle>#<key>` and `/embed#!<handle>!<key>`
/// - legacy `/#!<handle>!<key>`
///
/// Folder links (`/folder/..`, `#F!..`) return [`MegaLinkError::FolderUnsupported`].
pub fn parse_link(input: &str) -> Result<MegaLink, MegaLinkError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(MegaLinkError::NotMegaLink);
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let url = reqwest::Url::parse(&candidate).map_err(|_| MegaLinkError::NotMegaLink)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(MegaLinkError::NotMegaLink);
    }
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return Err(MegaLinkError::NotMegaLink);
    }
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let on = |domain: &str| {
        host == domain
            || host
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.'))
    };
    if !(on("mega.nz") || on("mega.co.nz")) {
        return Err(MegaLinkError::NotMegaLink);
    }

    // A `%23` in the path is an encoded fragment separator; decode path and
    // fragment together and split once.
    let mut location = percent_decode(url.path());
    if let Some(fragment) = url.fragment() {
        location.push('#');
        location.push_str(&percent_decode(fragment));
    }
    let (path, fragment) = location.split_once('#').unwrap_or((location.as_str(), ""));

    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    match segments.next() {
        None => legacy(fragment),
        Some("file") => modern(segments.next(), fragment),
        Some("embed") => match segments.next() {
            None => legacy(fragment),
            Some("folder") => Err(MegaLinkError::FolderUnsupported),
            Some("file") => modern(segments.next(), fragment),
            handle => modern(handle, fragment),
        },
        Some("folder") => Err(MegaLinkError::FolderUnsupported),
        Some(_) => Err(MegaLinkError::Unrecognized),
    }
}

/// `#!<handle>!<key>` after the `#`.
fn legacy(fragment: &str) -> Result<MegaLink, MegaLinkError> {
    if fragment.starts_with("F!") {
        return Err(MegaLinkError::FolderUnsupported);
    }
    if fragment.starts_with("P!") {
        return Err(MegaLinkError::PasswordProtectedUnsupported);
    }
    let Some(payload) = fragment.strip_prefix('!') else {
        return Err(MegaLinkError::Unrecognized);
    };
    let (handle, rest) = match payload.split_once('!') {
        Some((handle, rest)) => (handle, Some(rest)),
        None => (payload, None),
    };
    let handle = valid_handle(handle)?;
    let key = take_key(rest.unwrap_or(""))?;
    Ok(MegaLink { handle, key })
}

/// `/file/<handle>#<key>` (or embed) with the handle already split from the path.
fn modern(handle: Option<&str>, fragment: &str) -> Result<MegaLink, MegaLinkError> {
    if fragment.starts_with("F!") {
        return Err(MegaLinkError::FolderUnsupported);
    }
    let handle = valid_handle(handle.unwrap_or(""))?;
    let key = take_key(fragment)?;
    Ok(MegaLink { handle, key })
}

fn is_b64url(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

fn b64url_value(c: char) -> u8 {
    match c {
        'A'..='Z' => c as u8 - b'A',
        'a'..='z' => c as u8 - b'a' + 26,
        '0'..='9' => c as u8 - b'0' + 52,
        '-' => 62,
        _ => 63,
    }
}

fn valid_handle(handle: &str) -> Result<String, MegaLinkError> {
    if handle.len() == 8 && handle.chars().all(is_b64url) {
        Ok(handle.to_owned())
    } else {
        Err(MegaLinkError::InvalidHandle)
    }
}

/// Takes a 43 character file key from the start of `text`. Only `/`, `?`, `&`
/// or `#` may follow it. 32 bytes need 43 characters whose last one carries two
/// spare zero bits.
fn take_key(text: &str) -> Result<String, MegaLinkError> {
    let len = text.chars().take_while(|c| is_b64url(*c)).count();
    if len == 0 {
        return Err(MegaLinkError::MissingKey);
    }
    let (key, rest) = text.split_at(len);
    if !(rest.is_empty() || rest.starts_with(['/', '?', '&', '#'])) {
        return Err(MegaLinkError::InvalidKey);
    }
    let last = key.chars().last().unwrap_or('A');
    if key.len() != 43 || !b64url_value(last).is_multiple_of(4) {
        return Err(MegaLinkError::InvalidKey);
    }
    Ok(key.to_owned())
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let high = (bytes[i + 1] as char).to_digit(16);
            let low = (bytes[i + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Replaces everything after the fragment marker (`#` or `%23`) of any MEGA
/// link found in `text` with `<redacted>`. Covers `#K`, legacy `#!H!K`, `#F!..`,
/// percent-encoded markers and scheme-less links. Already redacted text is unchanged.
pub fn redact(text: &str) -> String {
    static LINK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let link = LINK.get_or_init(|| {
        regex::Regex::new(r#"(?i)\bmega\.(?:co\.)?nz[^\s<>"'#]*?(#|%23)([^\s<>"']*)(<redacted>)?"#)
            .expect("valid MEGA redaction pattern")
    });
    link.replace_all(text, |captures: &regex::Captures<'_>| {
        if captures.get(3).is_some() {
            return captures[0].to_owned();
        }
        let whole = captures.get(0).expect("group 0 always matches");
        let marker = captures.get(1).expect("marker group always matches");
        format!("{}<redacted>", &captures[0][..marker.end() - whole.start()])
    })
    .into_owned()
}
