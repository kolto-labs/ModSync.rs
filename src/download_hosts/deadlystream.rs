//! DeadlyStream anonymous page and session acquisition (host-providers plan, slice H2).
//!
//! A DeadlyStream resource page has no direct archive link. This module reads the page
//! with a small per-session cookie jar, finds the CSRF key, requests the selection page
//! and returns every archive behind it with its safe file name. The orchestrator then
//! transfers the chosen archive through `download::fetch_to_with_context`, so staging,
//! hash and archive checks and no-clobber publishing stay shared. The direct downloader
//! still rejects HTML; only this bounded reader ever parses a page.
//!
//! Every request stays on one origin: the page's own, or the explicit loopback fixture
//! base. Redirects are followed here, one vetted hop at a time. The cookie jar is host
//! only and never sent to another origin. No stored credential is used or sent.
//! Source only; not built or exercised.
use super::{Provider, provider};
use crate::{
    acquisition::ManualReason,
    download::{self, LoopbackCa},
};
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use reqwest::{
    Method, StatusCode, Url,
    blocking::{Client, Response},
    header,
};
use std::{
    collections::BTreeMap,
    io::Read,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// Largest page body read; a page is markup, never an archive.
const MAX_PAGE_BYTES: u64 = 2 * 1024 * 1024;
/// Redirect hops one request may follow.
const MAX_REDIRECTS: usize = 5;
/// Download links accepted from one page.
const MAX_LINKS: usize = 32;
/// Selection pages that may lead to further selection pages.
const MAX_DEPTH: usize = 2;
/// Every request of one resolution, redirect hops included.
const MAX_REQUESTS: usize = 200;
/// Cookies kept per session.
const MAX_COOKIES: usize = 32;

/// One archive behind a DeadlyStream page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// A safe single file name from Content-Disposition or the link.
    pub name: String,
    /// The authorized archive endpoint (under the fixture base in fixture mode).
    pub url: String,
    /// The page that offered the link; sent as `Referer` to the archive endpoint.
    pub referer: String,
    /// The session cookies at the end of resolution, for the archive endpoint only.
    pub cookie: Option<String>,
}

/// Normalizes an accepted page form: an exact DeadlyStream host (`download_hosts::provider`,
/// not a substring match), https (http is upgraded, as C# does), the default port, no
/// credentials, and neither query nor fragment (as C# `NormalizeDeadlyStreamUrl`, so an
/// authored `?do=download` link is read through its page). A path is required.
pub fn page_url(raw: &str) -> Result<Url> {
    let mut url = Url::parse(raw.trim())?;
    ensure!(
        matches!(provider(url.as_str())?, Provider::DeadlyStream),
        "Not a DeadlyStream URL"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "DeadlyStream URLs must not carry credentials"
    );
    if url.scheme() == "http" {
        url.set_scheme("https")
            .map_err(|()| anyhow::anyhow!("Cannot upgrade the DeadlyStream URL to HTTPS"))?;
    }
    ensure!(
        url.port().is_none(),
        "DeadlyStream URLs must use the default HTTPS port"
    );
    url.set_query(None);
    url.set_fragment(None);
    ensure!(
        !url.path().trim_matches('/').is_empty(),
        "DeadlyStream URL is not a resource or topic page"
    );
    Ok(url)
}

/// Where requests may go.
struct Transport<'a> {
    /// The one origin every request must stay on: the fixture base, or the page's own.
    origin: Url,
    fixture: bool,
    loopback_ca: Option<&'a LoopbackCa>,
}

impl<'a> Transport<'a> {
    fn new(page: &Url, base: Option<&str>, loopback_ca: Option<&'a LoopbackCa>) -> Result<Self> {
        let (mut origin, fixture) = match base {
            Some(base) => {
                ensure!(
                    loopback_ca.is_some(),
                    "The DeadlyStream fixture base requires a loopback CA"
                );
                let base = Url::parse(base.trim()).context("Invalid DeadlyStream fixture base")?;
                ensure!(
                    download::eligible_loopback(&base),
                    "The DeadlyStream fixture base must be an HTTPS loopback URL"
                );
                (base, true)
            }
            None => (page.clone(), false),
        };
        ensure!(
            origin.username().is_empty() && origin.password().is_none(),
            "The DeadlyStream base must not carry credentials"
        );
        origin.set_path("/");
        origin.set_query(None);
        origin.set_fragment(None);
        Ok(Self {
            origin,
            fixture,
            loopback_ca,
        })
    }

    /// The transport location of a logical DeadlyStream URL: itself, or under the fixture
    /// base. Logical identity stays with the caller.
    fn locate(&self, url: &Url) -> Url {
        if !self.fixture {
            return url.clone();
        }
        let mut moved = self.origin.clone();
        moved.set_path(url.path());
        moved.set_query(url.query());
        moved
    }

    /// The only gate for a page link or redirect target. A fixture run maps a link to the
    /// logical DeadlyStream host onto the fixture base; anything else must already be on
    /// the session's origin. A loopback CA additionally confines every hop to loopback.
    fn admit(&self, link: Url) -> Result<Url> {
        let link = if self.fixture
            && link.origin() != self.origin.origin()
            && link.scheme() == "https"
            && matches!(provider(link.as_str()), Ok(Provider::DeadlyStream))
        {
            self.locate(&link)
        } else {
            link
        };
        ensure!(
            link.origin() == self.origin.origin(),
            "Refusing a DeadlyStream link outside the page's origin ({})",
            link.host_str().unwrap_or("no host")
        );
        ensure!(
            link.username().is_empty() && link.password().is_none(),
            "Refusing a DeadlyStream link that carries credentials"
        );
        if self.loopback_ca.is_some() {
            ensure!(
                download::eligible_loopback(&link),
                "Loopback CA requires an HTTPS loopback endpoint"
            );
        }
        Ok(link)
    }
}

/// One vetted response. `body` is read only for a web page or an error status.
struct Reply {
    status: StatusCode,
    /// Where the last hop landed.
    url: Url,
    content_type: String,
    disposition: Option<String>,
    retry_after: Option<u64>,
    body: Option<String>,
}

impl Reply {
    fn is_html(&self) -> bool {
        self.content_type.contains("text/html") || self.content_type.contains("application/xhtml")
    }

    /// Rate limits, timeouts and server errors are transient; any other status is final.
    fn require_success(&self) -> Result<()> {
        if self.status.is_success() {
            return Ok(());
        }
        let error = anyhow::anyhow!("DeadlyStream answered HTTP status {}", self.status);
        Err(
            if self.status.is_server_error()
                || self.status == StatusCode::REQUEST_TIMEOUT
                || self.status == StatusCode::TOO_MANY_REQUESTS
            {
                download::transient(self.retry_after, error)
            } else {
                error
            },
        )
    }
}

fn header_text(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// The manual boundary a page shows, judged before any parsing and whatever its status.
/// `body` is lower-cased. A guest page carries a sign-in form beside its download links,
/// so a password field or captcha script next to a `do=download` link is not a wall.
fn gate(
    status: StatusCode,
    challenge: bool,
    url: &Url,
    body: Option<&str>,
) -> Option<ManualReason> {
    if challenge {
        return Some(ManualReason::Captcha);
    }
    let login_path = url
        .path()
        .trim_start_matches('/')
        .to_ascii_lowercase()
        .starts_with("login");
    if login_path || status == StatusCode::UNAUTHORIZED {
        return Some(ManualReason::LoginRequired);
    }
    let body = body?;
    let has_link = body.contains("do=download");
    // The two challenge phrases C# also checks, which `gate_reason` lacks.
    if !has_link
        && (body.contains("one moment, please") || body.contains("cf-browser-verification"))
    {
        return Some(ManualReason::Captcha);
    }
    match download::gate_reason(false, body) {
        ManualReason::Captcha if !has_link => Some(ManualReason::Captcha),
        ManualReason::LoginRequired if !has_link => Some(ManualReason::LoginRequired),
        ManualReason::FreeUserWait => Some(ManualReason::FreeUserWait),
        _ => None,
    }
}

fn gate_message(reason: ManualReason) -> &'static str {
    match reason {
        ManualReason::Captcha => {
            "DeadlyStream requires a browser challenge; open the page in a browser."
        }
        ManualReason::LoginRequired => {
            "DeadlyStream requires a login for this file; open the page in a browser."
        }
        ManualReason::FreeUserWait => {
            "DeadlyStream requires a timed download; open the page in a browser."
        }
        ManualReason::HtmlLanding | ManualReason::Unsupported => {
            "DeadlyStream returned a web page instead of a file; use the browser download flow."
        }
    }
}

struct Session<'a> {
    client: Client,
    transport: Transport<'a>,
    /// Cookie name to value, for the one origin.
    jar: BTreeMap<String, String>,
    /// The logical page a person can open; what a manual boundary points at.
    manual_url: String,
    cancelled: &'a AtomicBool,
    requests: usize,
}

impl Session<'_> {
    fn cookie_header(&self) -> String {
        self.jar
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Host-only jar: `Domain`, `Path` and `Secure` are ignored because every request
    /// goes to the one https origin. `Max-Age<=0` or an empty value removes the cookie.
    fn store_cookies(&mut self, headers: &header::HeaderMap) {
        for line in headers.get_all(header::SET_COOKIE) {
            let Ok(text) = line.to_str() else {
                continue;
            };
            let mut parts = text.split(';');
            let Some((name, value)) = parts.next().and_then(|pair| pair.split_once('=')) else {
                continue;
            };
            let (name, value) = (name.trim(), value.trim());
            if name.is_empty() {
                continue;
            }
            let expired = parts.any(|attribute| {
                attribute
                    .trim()
                    .to_ascii_lowercase()
                    .strip_prefix("max-age=")
                    .is_some_and(|age| age.trim().parse::<i64>().is_ok_and(|age| age <= 0))
            });
            if expired || value.is_empty() {
                self.jar.remove(name);
            } else if self.jar.contains_key(name) || self.jar.len() < MAX_COOKIES {
                self.jar.insert(name.to_owned(), value.to_owned());
            }
        }
    }

    /// One request with the session cookies, following at most `MAX_REDIRECTS` hops that
    /// `Transport::admit` allows. The reply is classified for a gate before any caller
    /// parses it; its status is judged by the caller.
    fn send(&mut self, method: Method, url: &Url, referer: Option<&Url>) -> Result<Reply> {
        let mut current = url.clone();
        for _ in 0..=MAX_REDIRECTS {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err(download::cancelled(anyhow::anyhow!("Download cancelled")));
            }
            self.requests += 1;
            ensure!(
                self.requests <= MAX_REQUESTS,
                "The DeadlyStream page needs more than {MAX_REQUESTS} requests; stopping"
            );
            let mut request = self
                .client
                .request(method.clone(), current.clone())
                .header(header::ACCEPT, "text/html,application/xhtml+xml,*/*;q=0.8");
            if !self.jar.is_empty() {
                request = request.header(header::COOKIE, self.cookie_header());
            }
            if let Some(referer) = referer {
                request = request.header(header::REFERER, referer.as_str());
            }
            // The URL is dropped from errors: it can carry a session CSRF key.
            let mut response = request
                .send()
                .map_err(|error| download::request_failure(error.without_url().into()))?;
            self.store_cookies(response.headers());
            let status = response.status();
            if status.is_redirection()
                && let Some(location) = header_text(&response, "location")
            {
                let next = current
                    .join(&location)
                    .context("DeadlyStream redirected to an invalid location")?;
                current = self
                    .transport
                    .admit(next)
                    .context("DeadlyStream redirect refused")?;
                continue;
            }
            let content_type = header_text(&response, "content-type")
                .unwrap_or_default()
                .to_ascii_lowercase();
            let disposition = header_text(&response, "content-disposition");
            let retry_after = header_text(&response, "retry-after")
                .and_then(|value| value.trim().parse::<u64>().ok());
            let challenge = header_text(&response, "cf-mitigated")
                .is_some_and(|value| value.to_ascii_lowercase().contains("challenge"));
            let html =
                content_type.contains("text/html") || content_type.contains("application/xhtml");
            let body = if method != Method::HEAD && (html || !status.is_success()) {
                let mut bytes = Vec::new();
                (&mut response)
                    .take(MAX_PAGE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| download::request_failure(error.into()))?;
                ensure!(
                    bytes.len() as u64 <= MAX_PAGE_BYTES,
                    "A DeadlyStream page is larger than {MAX_PAGE_BYTES} bytes"
                );
                Some(String::from_utf8_lossy(&bytes).into_owned())
            } else {
                None
            };
            let reply = Reply {
                status,
                url: current,
                content_type,
                disposition,
                retry_after,
                body,
            };
            let lowered = reply.body.as_deref().map(str::to_ascii_lowercase);
            if let Some(reason) = gate(status, challenge, &reply.url, lowered.as_deref()) {
                return Err(download::manual_required(
                    &self.manual_url,
                    reason,
                    gate_message(reason),
                ));
            }
            return Ok(reply);
        }
        bail!("DeadlyStream redirected more than {MAX_REDIRECTS} times")
    }

    /// A successful web page.
    fn page(&mut self, url: &Url, referer: Option<&Url>) -> Result<Reply> {
        let reply = self.send(Method::GET, url, referer)?;
        reply.require_success()?;
        Ok(reply)
    }

    /// Names the archive behind `link`, or follows a selection page to its links, at most
    /// `MAX_DEPTH` levels. A named file is pushed to `out`. HEAD comes first, as in C#;
    /// GET (headers only for a file) is the fallback.
    fn probe(
        &mut self,
        link: &Url,
        referer: &Url,
        depth: usize,
        out: &mut Vec<File>,
    ) -> Result<()> {
        let head = self.send(Method::HEAD, link, Some(referer))?;
        let reply = if head.status.is_success() && !head.is_html() {
            head
        } else {
            self.send(Method::GET, link, Some(referer))?
        };
        reply.require_success()?;
        if reply.is_html() {
            let html = reply.body.as_deref().unwrap_or("");
            if !is_selection(html) {
                // A page where a file was promised, and no gate explains it.
                return Err(download::manual_required(
                    &self.manual_url,
                    ManualReason::HtmlLanding,
                    gate_message(ManualReason::HtmlLanding),
                ));
            }
            if depth >= MAX_DEPTH {
                bail!("DeadlyStream selection pages nest deeper than {MAX_DEPTH} levels");
            }
            let inner = links(confirmed(&anchors(html)), &reply.url, &self.transport)?;
            if inner.is_empty() {
                bail!("A DeadlyStream selection page lists no download link");
            }
            for next in &inner {
                self.probe(next, &reply.url, depth + 1, out)?;
            }
            return Ok(());
        }
        let name = file_name(&reply).context("DeadlyStream did not name a download")?;
        download::one_component(&name)
            .with_context(|| format!("DeadlyStream lists an unsafe file name {name:?}"))?;
        out.push(File {
            name,
            url: link.to_string(),
            referer: referer.to_string(),
            cookie: None,
        });
        Ok(())
    }
}

/// Content-Disposition first, then the last path segment of the landing URL.
fn file_name(reply: &Reply) -> Option<String> {
    reply
        .disposition
        .as_deref()
        .and_then(download::disposition_filename)
        .or_else(|| {
            reply
                .url
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(download::percent_decode))
        })
        .filter(|name| !name.trim().is_empty() && !name.contains('?'))
}

/// Every archive a DeadlyStream page offers, in page order, over one anonymous session:
/// the page (cookies, CSRF key), the `?do=download&csrfKey=` selection page (or, without
/// one, the page's own `?do=download` links, which is what a topic's attachments are),
/// then each link's file name. `page` is the authored URL; `base` is the explicit
/// per-invocation fixture base (loopback HTTPS with `loopback_ca` only). Names are safe
/// single components and unique ignoring ASCII case. Errors keep their class: a gate is
/// `ManualRequired`, a rate limit or server error `Transient`, a stop `Cancelled`; a
/// missing CSRF key or link is a plain failure. A link is never guessed.
pub fn resolve(
    page: &str,
    base: Option<&str>,
    loopback_ca: Option<&LoopbackCa>,
    cancelled: &AtomicBool,
) -> Result<Vec<File>> {
    let logical = page_url(page)?;
    let transport = Transport::new(&logical, base, loopback_ca)?;
    let start = transport.admit(transport.locate(&logical))?;
    let client = download::page_client(loopback_ca, transport.fixture)?;
    let mut session = Session {
        client,
        transport,
        jar: BTreeMap::new(),
        manual_url: logical.to_string(),
        cancelled,
        requests: 0,
    };
    let first = session.page(&start, None)?;
    let html = first
        .body
        .as_deref()
        .context("DeadlyStream did not return a web page")?;
    let csrf = csrf_key(html);
    // (download link, the page that offered it)
    let mut sources: Vec<(Url, Url)> = Vec::new();
    if let Some(key) = &csrf {
        let mut select = first.url.clone();
        select.set_query(None);
        select
            .query_pairs_mut()
            .append_pair("do", "download")
            .append_pair("csrfKey", key);
        let reply = session.send(Method::GET, &select, Some(&first.url))?;
        match reply.require_success() {
            Ok(()) => {
                if let Some(selection) = reply.body.as_deref().filter(|body| is_selection(body)) {
                    let found = links(
                        confirmed(&anchors(selection)),
                        &reply.url,
                        &session.transport,
                    )?;
                    sources.extend(found.into_iter().map(|link| (link, reply.url.clone())));
                }
            }
            // A rate limit or server error is retried; any other refusal falls back to
            // the page's own links, as C# does.
            Err(error) if download::classify(&error).is_some() => return Err(error),
            Err(_) => {}
        }
    }
    if sources.is_empty() {
        let found = links(all_links(&anchors(html)), &first.url, &session.transport)?;
        sources.extend(found.into_iter().map(|link| (link, first.url.clone())));
    }
    if sources.is_empty() {
        if csrf.is_none() {
            bail!(
                "The DeadlyStream page has no CSRF key and no download link; its layout may have changed"
            );
        }
        bail!("The DeadlyStream page lists no download link; its layout may have changed");
    }
    // As C#, a link that fails plainly (stale, no name, unsafe name) is skipped so the
    // page's other files stay usable; a gate, rate limit or stop ends the resolution.
    // Only a page with no usable file fails, with the last reason.
    let mut found = Vec::new();
    let mut last_error = None;
    for (link, referer) in &sources {
        let before = found.len();
        match session.probe(link, referer, 0, &mut found) {
            Ok(()) => {}
            Err(error) if download::classify(&error).is_some() => return Err(error),
            Err(error) => {
                found.truncate(before);
                last_error = Some(error);
            }
        }
    }
    if found.is_empty() {
        return Err(
            last_error.unwrap_or_else(|| anyhow::anyhow!("The DeadlyStream page lists no file"))
        );
    }
    let cookie = session.cookie_header();
    let cookie = (!cookie.is_empty()).then_some(cookie);
    let mut files: Vec<File> = Vec::new();
    for mut file in found {
        let same_url = files
            .iter()
            .find(|other| other.name.eq_ignore_ascii_case(&file.name))
            .map(|other| other.url == file.url);
        match same_url {
            Some(true) => {}
            Some(false) => bail!(
                "The DeadlyStream page offers more than one different download named {}; name the file under Files",
                file.name
            ),
            None => {
                file.cookie.clone_from(&cookie);
                files.push(file);
            }
        }
    }
    ensure!(!files.is_empty(), "The DeadlyStream page lists no file");
    Ok(files)
}

fn is_selection(html: &str) -> bool {
    html.contains("Download your files") || html.contains("data-action=\"download\"")
}

/// The CSRF key from a hidden input, then a script value, then any link, as C#
/// `ExtractCsrfKey`. A value that is not printable ASCII of a sane length is not a key.
fn csrf_key(html: &str) -> Option<String> {
    static INPUT: OnceLock<Regex> = OnceLock::new();
    static VALUE: OnceLock<Regex> = OnceLock::new();
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    let input = INPUT.get_or_init(|| {
        Regex::new(r#"(?is)<input\b[^>]*\bname\s*=\s*["']csrfKey["'][^>]*>"#).unwrap()
    });
    let value = VALUE.get_or_init(|| Regex::new(r#"(?is)\bvalue\s*=\s*["']([^"']+)["']"#).unwrap());
    let script =
        SCRIPT.get_or_init(|| Regex::new(r#"(?i)csrfKey["']?\s*[:=]\s*["']([^"']+)["']"#).unwrap());
    let link = LINK.get_or_init(|| Regex::new(r#"(?i)csrfKey=([^&"'<>\s]+)"#).unwrap());
    input
        .find(html)
        .and_then(|tag| value.captures(tag.as_str()))
        .or_else(|| script.captures(html))
        .or_else(|| link.captures(html))
        .map(|captures| html_decode(&captures[1]))
        .filter(|key| key.len() <= 256 && key.bytes().all(|byte| byte.is_ascii_graphic()))
}

/// The entities a page attribute uses, decoded once (C# decodes after joining, which
/// would leave `&amp;` in a relative link's query).
fn html_decode(text: &str) -> String {
    static ENTITY: OnceLock<Regex> = OnceLock::new();
    let entity = ENTITY.get_or_init(|| {
        Regex::new(r"&(#[xX][0-9a-fA-F]{1,6}|#[0-9]{1,7}|[a-zA-Z]{2,8});").unwrap()
    });
    entity
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let name = &captures[1];
            let decoded = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => name
                    .strip_prefix('#')
                    .and_then(|number| match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => number.parse::<u32>().ok(),
                    })
                    .and_then(char::from_u32),
            };
            decoded.map_or_else(|| captures[0].to_owned(), |decoded| decoded.to_string())
        })
        .into_owned()
}

struct Anchor {
    /// Entity-decoded `href`.
    href: String,
    data_action: Option<String>,
    class: Option<String>,
}

/// The `<a>` tags with a quoted `href` (at most 4096). No HTML parser is available, so an
/// unquoted attribute is not read; the layout change then shows as a missing link.
fn anchors(html: &str) -> Vec<Anchor> {
    static TAG: OnceLock<Regex> = OnceLock::new();
    static ATTRIBUTE: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r"(?is)<a\s[^>]*>").unwrap());
    let attribute = ATTRIBUTE.get_or_init(|| {
        Regex::new(r#"(?is)\s(href|data-action|class)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap()
    });
    tag.find_iter(html)
        .take(4096)
        .filter_map(|found| {
            let mut anchor = Anchor {
                href: String::new(),
                data_action: None,
                class: None,
            };
            for captures in attribute.captures_iter(found.as_str()) {
                let value = html_decode(
                    captures
                        .get(2)
                        .or_else(|| captures.get(3))
                        .map_or("", |value| value.as_str()),
                );
                match captures[1].to_ascii_lowercase().as_str() {
                    "href" => anchor.href = value,
                    "data-action" => anchor.data_action = Some(value),
                    _ => anchor.class = Some(value),
                }
            }
            (!anchor.href.trim().is_empty()).then_some(anchor)
        })
        .collect()
}

fn action_download(anchor: &Anchor) -> bool {
    anchor.data_action.as_deref() == Some("download") && anchor.href.contains("?do=download")
}

fn confirmed_download(anchor: &Anchor) -> bool {
    anchor.href.contains("?do=download") && anchor.href.contains("&confirm=1")
}

fn button_download(anchor: &Anchor) -> bool {
    anchor
        .class
        .as_deref()
        .is_some_and(|class| class.contains("ipsButton"))
        && anchor.href.contains("?do=download")
        && anchor.href.contains("&r=")
}

/// The first of C#'s three selection-page selectors that matches anything.
fn confirmed(anchors: &[Anchor]) -> Vec<&str> {
    const SELECTORS: [fn(&Anchor) -> bool; 3] =
        [action_download, confirmed_download, button_download];
    for select in SELECTORS {
        let hrefs: Vec<&str> = anchors
            .iter()
            .filter(|anchor| select(anchor))
            .map(|anchor| anchor.href.as_str())
            .collect();
        if !hrefs.is_empty() {
            return hrefs;
        }
    }
    Vec::new()
}

/// Every `?do=download` link: a file page's button, or a topic's attachments.
fn all_links(anchors: &[Anchor]) -> Vec<&str> {
    anchors
        .iter()
        .filter(|anchor| anchor.href.contains("?do=download"))
        .map(|anchor| anchor.href.as_str())
        .collect()
}

/// Resolves entity-decoded hrefs against `base`, drops fragments and repeats (ignoring
/// ASCII case), and keeps only links `Transport::admit` allows. Links refused for leaving
/// the origin are an error only when nothing else is left.
fn links(hrefs: Vec<&str>, base: &Url, transport: &Transport<'_>) -> Result<Vec<Url>> {
    let mut found: Vec<Url> = Vec::new();
    let mut refused = 0usize;
    for href in hrefs {
        let Ok(mut url) = base.join(href.trim()) else {
            refused += 1;
            continue;
        };
        url.set_fragment(None);
        match transport.admit(url) {
            Ok(url) => {
                if !found
                    .iter()
                    .any(|seen| seen.as_str().eq_ignore_ascii_case(url.as_str()))
                {
                    found.push(url);
                }
            }
            Err(_) => refused += 1,
        }
    }
    ensure!(
        found.len() <= MAX_LINKS,
        "The DeadlyStream page lists more than {MAX_LINKS} download links"
    );
    if found.is_empty() && refused > 0 {
        bail!(
            "The DeadlyStream page links {refused} download(s) outside its own origin; refusing them"
        );
    }
    Ok(found)
}
