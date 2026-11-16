//! Details for a mod's web page: title, description, author and screenshots,
//! for showing players what a mod is before they install it.
//!
//! DeadlyStream pages are read over HTTPS and parsed for their description and
//! screenshot gallery; Nexus Mods details come from the Nexus API through
//! [`crate::updates::NexusClient`]. Screenshots can be fetched as `data:` URLs
//! because both sites refuse images embedded from other pages. Nothing is
//! written to disk. Requests are limited to the two hosts, HTTPS only, with
//! bounded response sizes.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::Serialize;
use std::{
    io::Read,
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const PAGE_LIMIT: u64 = 4 * 1024 * 1024;
const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;
const DESCRIPTION_LIMIT: usize = 4000;
const MAX_IMAGES: usize = 12;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub title: String,
    pub description: String,
    pub author: String,
    /// Screenshot addresses, full size where the page links them.
    pub images: Vec<String>,
    /// The page the details came from.
    pub page_url: String,
    /// `deadlystream` or `nexus`.
    pub source: String,
}

/// Which supported site a link points to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    DeadlyStream(String),
    Nexus { domain: String, mod_id: String },
}

fn host_of(url: &reqwest::Url) -> String {
    url.host_str().unwrap_or_default().to_ascii_lowercase()
}

fn on_host(host: &str, site: &str) -> bool {
    host == site || host.ends_with(&format!(".{site}"))
}

/// Recognize a DeadlyStream file page or a Nexus Mods mod page.
pub fn page(url: &str) -> Option<Page> {
    let parsed = reqwest::Url::parse(url.trim()).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let host = host_of(&parsed);
    let parts: Vec<&str> = parsed
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect();
    if on_host(&host, "deadlystream.com") {
        // https://deadlystream.com/files/file/1234-name/
        if parts.len() >= 3 && parts[0] == "files" && parts[1] == "file" {
            let id: String = parts[2].chars().take_while(char::is_ascii_digit).collect();
            if !id.is_empty() {
                return Some(Page::DeadlyStream(format!(
                    "https://deadlystream.com/files/file/{}/",
                    parts[2]
                )));
            }
        }
        return None;
    }
    if on_host(&host, "nexusmods.com") && parts.len() >= 3 && parts[1] == "mods" {
        let mod_id: String = parts[2].chars().take_while(char::is_ascii_digit).collect();
        if !mod_id.is_empty() && parts[0].chars().all(|c| c.is_ascii_alphanumeric()) {
            return Some(Page::Nexus {
                domain: parts[0].to_ascii_lowercase(),
                mod_id,
            });
        }
    }
    None
}

fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("ModSync/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = host_of(attempt.url());
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if allowed_host(&host) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()?)
}

fn allowed_host(host: &str) -> bool {
    on_host(host, "deadlystream.com") || on_host(host, "nexusmods.com")
}

fn get(url: &str, limit: u64, cancelled: &AtomicBool) -> Result<(Vec<u8>, String)> {
    ensure!(!cancelled.load(Ordering::Acquire), "Cancelled");
    let parsed = reqwest::Url::parse(url).context("Not a web address")?;
    ensure!(parsed.scheme() == "https", "Only https addresses are read");
    ensure!(
        allowed_host(&host_of(&parsed)),
        "Only DeadlyStream and Nexus Mods pages are read"
    );
    let response = client()?.get(parsed).send()?.error_for_status()?;
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let mut body = Vec::new();
    response.take(limit + 1).read_to_end(&mut body)?;
    ensure!(body.len() as u64 <= limit, "The page is too large");
    ensure!(!cancelled.load(Ordering::Acquire), "Cancelled");
    Ok((body, kind))
}

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static BREAK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>|</p>|</li>|</h\d>|</div>").unwrap());
static BLANK_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n\s*\n+").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\r\f]+").unwrap());
static SCRIPT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(script|style|noscript)[^>]*>.*?</(script|style|noscript)>").unwrap()
});
static META: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?is)<meta\s[^>]*>"#).unwrap());
static ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)\b(href|src|data-src|data-full|content|property|name)\s*=\s*"([^"]*)""#)
        .unwrap()
});
static PROFILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\s[^>]*href="[^"]*/profile/[^"]*"[^>]*>\s*([^<]{1,80}?)\s*</a>"#).unwrap()
});
static H1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<h1[^>]*>(.*?)</h1>").unwrap());
static BBCODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\[/?[a-zA-Z*][^\]]{0,200}\]").unwrap());

/// Decode the HTML entities that appear in page text.
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Readable text from an HTML fragment: tags removed, line breaks kept.
pub fn html_text(fragment: &str) -> String {
    let text = SCRIPT.replace_all(fragment, "");
    let text = BREAK.replace_all(&text, "\n");
    let text = TAG.replace_all(&text, "");
    let text = decode_entities(&text);
    let text = SPACES.replace_all(&text, " ");
    let joined = text.lines().map(str::trim).collect::<Vec<_>>().join("\n");
    let text = BLANK_LINES.replace_all(joined.trim(), "\n\n");
    limit(&text, DESCRIPTION_LIMIT)
}

fn limit(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{}...", cut.trim_end())
}

fn meta(html: &str, key: &str) -> Option<String> {
    META.find_iter(html).find_map(|tag| {
        let mut named = false;
        let mut content = None;
        for capture in ATTR.captures_iter(tag.as_str()) {
            match capture[1].to_ascii_lowercase().as_str() {
                "property" | "name" if capture[2].eq_ignore_ascii_case(key) => named = true,
                "content" => content = Some(decode_entities(&capture[2])),
                _ => {}
            }
        }
        if named { content } else { None }
    })
}

/// The `<div>` element starting at `start`, including nested divs.
fn div_at(html: &str, start: usize) -> &str {
    let lower = html[start..].to_ascii_lowercase();
    let mut depth = 0usize;
    let mut index = 0;
    while index < lower.len() {
        let open = lower[index..].find("<div");
        let close = lower[index..].find("</div");
        match (open, close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                index += o + 4;
            }
            (_, Some(c)) => {
                depth = depth.saturating_sub(1);
                index += c + 5;
                if depth == 0 {
                    let end = lower[index..].find('>').map_or(index, |gt| index + gt + 1);
                    return &html[start..start + end];
                }
            }
            _ => break,
        }
    }
    &html[start..]
}

/// The file description block of a DeadlyStream (Invision Community) page.
fn deadlystream_description(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let from = lower.find("about this file").unwrap_or(0);
    let marker = lower[from..].find("ipstype_richtext")?;
    let at = from + marker;
    let start = lower[..at].rfind("<div")?;
    let text = html_text(div_at(html, start));
    (!text.is_empty()).then_some(text)
}

const IMAGE_EXCLUDE: &[&str] = &[
    "avatar",
    "/profile",
    "profile_",
    "photo-thumb",
    "default_photo",
    "set_resources",
    "logo",
    "favicon",
    "/icon",
    "emoticon",
    "/style_",
    "themes/",
    "/theme/",
    "badge",
    "spinner",
    "sprite",
    "rank",
    "placeholder",
    "loading",
    "blank.",
    "/r/",
    "/reactions/",
    "deadly_stream",
    "_banner",
    "/comment",
];
const IMAGE_CONTENT: &[&str] = &[
    "/downloads/screens/",
    "/uploads/",
    "/gallery/",
    "screenshot",
    "/attachments/",
];

fn absolute(url: &str) -> String {
    let url = decode_entities(url.trim());
    if let Some(rest) = url.strip_prefix("//") {
        format!("https://{rest}")
    } else if url.starts_with('/') {
        format!("https://deadlystream.com{url}")
    } else {
        url
    }
}

fn looks_like_screenshot(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let image = [".jpg", ".jpeg", ".png", ".gif", ".webp"]
        .iter()
        .any(|ext| {
            lower
                .split(['?', '#'])
                .next()
                .is_some_and(|path| path.ends_with(ext))
        });
    image
        && lower.starts_with("https://")
        && !IMAGE_EXCLUDE.iter().any(|word| lower.contains(word))
        && IMAGE_CONTENT.iter().any(|word| lower.contains(word))
}

/// Screenshot links on a DeadlyStream page, preferring the file's gallery.
fn deadlystream_images(html: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for capture in ATTR.captures_iter(html) {
        if !matches!(
            capture[1].to_ascii_lowercase().as_str(),
            "href" | "src" | "data-src" | "data-full"
        ) {
            continue;
        }
        let url = absolute(&capture[2]);
        if looks_like_screenshot(&url) && !found.contains(&url) {
            found.push(url);
        }
    }
    if let Some(og) = meta(html, "og:image").map(|url| absolute(&url))
        && looks_like_screenshot(&og)
        && !found.contains(&og)
    {
        found.push(og);
    }
    let gallery: Vec<String> = found
        .iter()
        .filter(|url| url.contains("/downloads/screens/"))
        .cloned()
        .collect();
    let mut images = if gallery.is_empty() { found } else { gallery };
    images.truncate(MAX_IMAGES);
    images
}

/// Parse a DeadlyStream file page.
pub fn parse_deadlystream(html: &str, page_url: &str) -> ModInfo {
    let title = H1
        .captures(html)
        .map(|capture| html_text(&capture[1]))
        .filter(|title| !title.is_empty())
        .or_else(|| meta(html, "og:title"))
        .unwrap_or_default();
    let title = title
        .split(" - DeadlyStream")
        .next()
        .unwrap_or(&title)
        .trim()
        .to_owned();
    let description = deadlystream_description(html)
        .or_else(|| meta(html, "og:description").map(|text| limit(text.trim(), DESCRIPTION_LIMIT)))
        .unwrap_or_default();
    let author = PROFILE
        .captures(html)
        .map(|capture| decode_entities(capture[1].trim()))
        .unwrap_or_default();
    ModInfo {
        title,
        description,
        author,
        images: deadlystream_images(html),
        page_url: page_url.to_owned(),
        source: "deadlystream".into(),
    }
}

/// Nexus descriptions are BBCode; keep the text.
fn bbcode_text(text: &str) -> String {
    let text = text.replace("<br />", "\n").replace("<br>", "\n");
    let text = BBCODE.replace_all(&text, "");
    html_text(&text)
}

/// Details for a DeadlyStream or Nexus Mods page; `None` for other sites.
/// Nexus details need an API key.
pub fn fetch(
    url: &str,
    nexus_key: Option<String>,
    cancelled: &AtomicBool,
) -> Result<Option<ModInfo>> {
    match page(url) {
        Some(Page::DeadlyStream(page_url)) => {
            let (body, _) = get(&page_url, PAGE_LIMIT, cancelled)?;
            Ok(Some(parse_deadlystream(
                &String::from_utf8_lossy(&body),
                &page_url,
            )))
        }
        Some(Page::Nexus { domain, mod_id }) => {
            let key = nexus_key
                .filter(|key| !key.trim().is_empty())
                .context("Add a Nexus Mods API key in Settings to see Nexus mod details")?;
            let mut client = crate::updates::NexusClient::new(Some(key))?;
            let info = client.get_mod_info_with_cancel(&domain, &mod_id, cancelled)?;
            let description = if info.description.trim().is_empty() {
                info.summary.clone()
            } else {
                bbcode_text(&info.description)
            };
            Ok(Some(ModInfo {
                title: info.name,
                description,
                author: info.author,
                images: info.picture_url.into_iter().collect(),
                page_url: format!("https://www.nexusmods.com/{domain}/mods/{mod_id}"),
                source: "nexus".into(),
            }))
        }
        None => Ok(None),
    }
}

/// A screenshot from DeadlyStream or Nexus Mods: its media type and bytes.
pub fn fetch_image(url: &str, cancelled: &AtomicBool) -> Result<(String, Vec<u8>)> {
    let (body, kind) = get(url, IMAGE_LIMIT, cancelled)?;
    let kind = kind
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let kind = match kind.as_str() {
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" => kind,
        _ => match body.get(..4) {
            Some([0xff, 0xd8, ..]) => "image/jpeg".into(),
            Some([0x89, b'P', b'N', b'G']) => "image/png".into(),
            Some([b'G', b'I', b'F', _]) => "image/gif".into(),
            _ => bail!("The address is not an image"),
        },
    };
    Ok((kind, body))
}

/// A screenshot as a `data:` URL, so an app can show it without the site's
/// hotlink protection getting in the way.
pub fn image_data_url(url: &str, cancelled: &AtomicBool) -> Result<String> {
    use base64::Engine as _;
    let (kind, body) = fetch_image(url, cancelled)?;
    Ok(format!(
        "data:{kind};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(body)
    ))
}
