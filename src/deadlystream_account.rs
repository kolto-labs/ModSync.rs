//! Check a DeadlyStream (Invision Community) account by signing in through the
//! site's login form, the way a browser does.
//!
//! The password is sent once, over HTTPS, to `deadlystream.com` only, and is
//! never stored. A successful sign-in is recognized by the member cookie the
//! site sets. The session is not kept: the acquisition code still downloads
//! anonymously ([`crate::download_hosts::deadlystream`]).
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::Read,
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const LOGIN_URL: &str = "https://deadlystream.com/login/";
const PAGE_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub username: String,
}

static CSRF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)name="csrfKey"\s+value="([a-f0-9]{16,})"|"csrfKey"\s*:\s*"([a-f0-9]{16,})"|data-csrfkey="([a-f0-9]{16,})""#).unwrap()
});
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static ERROR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)class="[^"]*ipsMessage_error[^"]*"[^>]*>(.*?)</"#).unwrap()
});

fn on_site(url: &reqwest::Url) -> bool {
    matches!(
        url.host_str(),
        Some("deadlystream.com" | "www.deadlystream.com")
    )
}

fn read(response: reqwest::blocking::Response) -> Result<String> {
    let mut body = Vec::new();
    response.take(PAGE_LIMIT + 1).read_to_end(&mut body)?;
    ensure!(
        body.len() as u64 <= PAGE_LIMIT,
        "The DeadlyStream page is too large"
    );
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// The CSRF key on a DeadlyStream page.
pub fn csrf_key(html: &str) -> Option<String> {
    CSRF.captures(html).and_then(|c| {
        (1..=3)
            .find_map(|i| c.get(i))
            .map(|m| m.as_str().to_owned())
    })
}

/// A small cookie jar for one sign-in: name to value, for deadlystream.com only.
#[derive(Default)]
struct Cookies(BTreeMap<String, String>);

impl Cookies {
    fn store(&mut self, headers: &reqwest::header::HeaderMap) {
        for value in headers.get_all(reqwest::header::SET_COOKIE) {
            let Ok(text) = value.to_str() else { continue };
            let pair = text.split(';').next().unwrap_or_default();
            if let Some((name, value)) = pair.split_once('=')
                && self.0.len() < 64
            {
                self.0
                    .insert(name.trim().to_owned(), value.trim().to_owned());
            }
        }
    }
    fn header(&self) -> String {
        self.0
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
    fn signed_in(&self) -> bool {
        self.0
            .iter()
            .any(|(name, value)| name.ends_with("member_id") && !value.is_empty() && value != "0")
    }
}

/// Send a request and follow up to five same-site redirects, keeping cookies.
fn send(
    client: &reqwest::blocking::Client,
    cookies: &mut Cookies,
    mut request: reqwest::blocking::RequestBuilder,
) -> Result<reqwest::blocking::Response> {
    for _ in 0..=5 {
        let response = request
            .header(reqwest::header::COOKIE, cookies.header())
            .send()
            .context(
                "Couldn't reach DeadlyStream. Check your internet connection and try again.",
            )?;
        cookies.store(response.headers());
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .context("DeadlyStream sent an incomplete redirect")?;
        let next = response.url().join(location)?;
        ensure!(on_site(&next), "DeadlyStream redirected to another site");
        request = client.get(next);
    }
    bail!("DeadlyStream redirected too many times")
}

/// Sign in with a username (or email) and password. Returns the account on
/// success, or the site's reason on failure.
pub fn sign_in(username: &str, password: &str, cancelled: &AtomicBool) -> Result<Account> {
    let username = username.trim();
    ensure!(
        !username.is_empty(),
        "Enter your DeadlyStream username or email"
    );
    ensure!(!password.is_empty(), "Enter your DeadlyStream password");
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("ModSync/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut cookies = Cookies::default();
    let page = send(&client, &mut cookies, client.get(LOGIN_URL))?
        .error_for_status()
        .context("DeadlyStream did not show its sign-in page. Try again in a few minutes.")?;
    let csrf = csrf_key(&read(page)?)
        .context("DeadlyStream's sign-in form could not be found. Try again in a few minutes.")?;
    ensure!(!cancelled.load(Ordering::Acquire), "Sign-in cancelled");
    let form = client.post(LOGIN_URL).form(&[
        ("_processLogin", "usernamepassword"),
        ("auth", username),
        ("password", password),
        ("csrfKey", csrf.as_str()),
        ("remember_me", "1"),
    ]);
    let response = send(&client, &mut cookies, form)?
        .error_for_status()
        .context("DeadlyStream refused the sign-in request. Try again in a few minutes.")?;
    let html = read(response)?;
    if cookies.signed_in() {
        return Ok(Account {
            username: username.to_owned(),
        });
    }
    let reason = ERROR
        .captures(&html)
        .map(|c| TAG.replace_all(&c[1], "").trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "The username or password was not accepted.".into());
    bail!(reason)
}
