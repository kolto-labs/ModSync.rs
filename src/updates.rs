use crate::{Component, Guide};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Versions(pub BTreeMap<String, String>);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Summary {
    pub checked_count: usize,
    pub skipped_count: usize,
    #[serde(default)]
    pub rate_limit_reached: bool,
    pub updates_found: Vec<Update>,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct Update {
    pub component_name: String,
    pub url: String,
    pub installed_version: String,
    pub latest_version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct NexusModInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub updated_timestamp: i64,
    #[serde(default = "available_by_default")]
    pub available: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct NexusModFile {
    #[serde(default)]
    pub file_id: i64,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub category_name: String,
    #[serde(default, deserialize_with = "lowercase_optional")]
    pub md5: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct NexusMd5Result {
    #[serde(default)]
    pub mod_info: Option<NexusModInfo>,
    #[serde(default)]
    pub mod_id: i64,
    #[serde(default)]
    pub file: Option<NexusModFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NexusValidateResult {
    pub is_valid: bool,
    pub user_name: String,
    pub is_premium: bool,
    pub message: String,
}

fn lowercase_optional<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?
        .unwrap_or_default()
        .to_ascii_lowercase())
}

fn available_by_default() -> bool {
    true
}

pub struct NexusClient {
    client: reqwest::blocking::Client,
    base_url: reqwest::Url,
    api_key: Option<String>,
    daily_remaining: Option<i64>,
    hourly_remaining: Option<i64>,
    /// When set, a 429 is returned as a typed transient error instead of waited out here,
    /// so the caller's retry policy owns the wait (and no lock is held while waiting).
    defer_rate_limits: bool,
}

/// Longest Nexus `Retry-After` honored, for every caller.
pub const NEXUS_RETRY_AFTER_CAP: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NxmDownloadLink {
    pub file_name: String,
    pub uri: String,
}

/// Why an nxm handoff cannot authorize its download. Terminal: never retried, never
/// answered with a different resource, and never worked around through a peer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NxmAuthorization {
    /// No one-time key and no Nexus API key.
    Missing,
    /// A one-time key without a usable `expires` timestamp.
    Invalid,
    /// Nexus refused the one-time key with this HTTP status. Expiry is judged by Nexus,
    /// not the local clock, which may run fast.
    Refused(u16),
}

impl std::fmt::Display for NxmAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const AGAIN: &str = "click \"Mod Manager Download\" on the website again";
        match self {
            Self::Missing => {
                f.write_str("The nxm link has no download key and no Nexus API key is configured")
            }
            Self::Invalid => write!(
                f,
                "The nxm link's download key has no valid expiry; {AGAIN}"
            ),
            Self::Refused(status) => write!(
                f,
                "Nexus Mods refused the nxm download key (HTTP {status}); it may have expired, so {AGAIN}"
            ),
        }
    }
}

impl std::error::Error for NxmAuthorization {}

/// Checks that a server-supplied download filename is exactly one normal path
/// component, so joining it onto a directory cannot escape that directory.
/// Both `/` and `\\` count as separators on every platform.
pub fn validate_download_file_name(file_name: &str) -> Result<&str> {
    anyhow::ensure!(
        !file_name.trim().is_empty(),
        "Downloaded filename is required"
    );
    anyhow::ensure!(
        !file_name.contains('\0'),
        "Downloaded filename contains a NUL byte"
    );
    anyhow::ensure!(
        !file_name.contains(['/', '\\']),
        "Downloaded filename {file_name:?} contains a path separator"
    );
    anyhow::ensure!(
        file_name != "." && file_name != "..",
        "Downloaded filename {file_name:?} is a relative directory reference"
    );
    anyhow::ensure!(
        !file_name.contains(':'),
        "Downloaded filename {file_name:?} contains a drive prefix or stream separator"
    );
    let mut components = std::path::Path::new(file_name).components();
    anyhow::ensure!(
        matches!(
            (components.next(), components.next()),
            (Some(std::path::Component::Normal(part)), None) if part == file_name
        ),
        "Downloaded filename {file_name:?} is not a single file name"
    );
    Ok(file_name)
}

pub fn record_nxm_download(
    component: &mut Component,
    registry_url: &str,
    file_name: &str,
) -> Result<()> {
    validate_download_file_name(file_name)?;
    let metadata = component
        .resource_registry
        .get_mut(registry_url)
        .ok_or_else(|| anyhow::anyhow!("Resolved registry URL is no longer present"))?;
    let files = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Resource metadata must be an object"))?
        .entry("Files")
        .or_insert_with(|| Value::Object(Default::default()))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Resource Files must be an object"))?;
    files.insert(file_name.into(), Value::Bool(true));
    Ok(())
}

/// Production Nexus Mods API base.
const NEXUS_API_BASE: &str = "https://api.nexusmods.com/v1/";

/// Most redirect hops one Nexus API request may follow. `Attempt::previous` also holds
/// the original URL, so the policy compares with `>` like reqwest's `Policy::limited`.
const NEXUS_API_REDIRECT_LIMIT: usize = 10;

/// The one redirect policy every `NexusClient` uses, bound to that client's own API
/// base origin. A redirect is followed only when scheme, host and port all equal the
/// base's, so the custom `apikey` header (which reqwest does not strip across hosts)
/// never reaches another host or port, and https is never downgraded to http. The
/// policy runs before the redirect target is contacted, so a refused hop sends nothing.
fn api_redirect_policy(base_url: &reqwest::Url) -> reqwest::redirect::Policy {
    let origin = base_url.origin();
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.url().origin() != origin {
            let target = attempt.url().origin().ascii_serialization();
            attempt.error(format!(
                "Refusing Nexus API redirect to another origin: {target}"
            ))
        } else if attempt.previous().len() > NEXUS_API_REDIRECT_LIMIT {
            attempt.error("Too many Nexus API redirects")
        } else {
            attempt.follow()
        }
    })
}

impl NexusClient {
    pub fn new(api_key: Option<String>) -> Result<Self> {
        Self::build(api_key, NEXUS_API_BASE)
    }

    /// Requires https, except for loopback hosts, which may use http so a
    /// local fake API can be used without a certificate authority. Redirects
    /// are followed only within the base URL's origin, so the scheme check
    /// and the API key cannot be carried to another host.
    pub fn with_base_url(api_key: Option<String>, base_url: &str) -> Result<Self> {
        Self::build(api_key, base_url)
    }

    /// Both constructors end here, so both get `api_redirect_policy` for their own base.
    fn build(api_key: Option<String>, base_url: &str) -> Result<Self> {
        let mut base_url = reqwest::Url::parse(base_url)?;
        // Relative API paths join under the base's last segment, so `.../v1` and `.../v1/`
        // both become `.../v1/` (a bare `join("./")` would drop `v1`).
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }
        let base_url = base_url.join("./")?;
        let loopback = base_url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        anyhow::ensure!(
            base_url.scheme() == "https" || (loopback && base_url.scheme() == "http"),
            "Nexus API base URL must use https (http is allowed only for loopback hosts)"
        );
        let redirect = api_redirect_policy(&base_url);
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .redirect(redirect)
                .build()?,
            base_url,
            api_key,
            daily_remaining: None,
            hourly_remaining: None,
            defer_rate_limits: false,
        })
    }

    /// Return HTTP 429 as a `download::FailureClass::Transient` error carrying the capped
    /// `Retry-After`, instead of sleeping inside the client. Used by the download
    /// orchestrator, whose cancel-aware retry loop then owns the wait.
    pub fn defer_rate_limits(&mut self) {
        self.defer_rate_limits = true;
    }

    pub fn is_rate_limit_exhausted(&self) -> bool {
        self.daily_remaining.is_some_and(|remaining| remaining <= 0)
            || self
                .hourly_remaining
                .is_some_and(|remaining| remaining <= 0)
    }

    pub fn get_mod_info(&mut self, game_domain: &str, mod_id: &str) -> Result<NexusModInfo> {
        self.get_json(&format!("games/{game_domain}/mods/{mod_id}.json"))
    }

    pub fn get_mod_files(&mut self, game_domain: &str, mod_id: &str) -> Result<Vec<NexusModFile>> {
        #[derive(Deserialize)]
        struct Files {
            #[serde(default)]
            files: Vec<NexusModFile>,
        }
        Ok(self
            .get_json::<Files>(&format!("games/{game_domain}/mods/{mod_id}/files.json"))?
            .files)
    }

    pub fn nxm_download_link(&mut self, link: &crate::protocol::Link) -> Result<NxmDownloadLink> {
        let crate::protocol::Link::Nxm {
            game_domain,
            mod_id,
            file_id,
            key,
            expires,
            ..
        } = link
        else {
            bail!("Expected an nxm:// link");
        };
        let keyed = key.as_deref().is_some_and(|value| !value.is_empty());
        // With an API key, Nexus decides whether a forwarded one-time key still matters.
        // Without one, the key alone must authorize, so check its shape before any
        // request. Whether it has expired is Nexus's answer, not the local clock's.
        if !self
            .api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            if !keyed {
                return Err(NxmAuthorization::Missing.into());
            }
            if *expires == 0 {
                return Err(NxmAuthorization::Invalid.into());
            }
        }
        let metadata = self
            .get_json::<NexusModFile>(&format!(
                "games/{game_domain}/mods/{mod_id}/files/{file_id}.json"
            ))
            .ok();
        let mut url = self.base_url.join(&format!(
            "games/{game_domain}/mods/{mod_id}/files/{file_id}/download_link.json"
        ))?;
        if let Some(key) = key.as_deref().filter(|key| !key.is_empty()) {
            // Built by URL query encoding, never interpolation, so reserved characters
            // stay part of the key. Spaces become %20 rather than form `+`; a literal `+`
            // is already %2B, so strict and form decoders both recover the exact key.
            // Unlike `download_orchestrator::nxm_url`, whose form `+` only ever goes back
            // through `protocol::parse` (`query_pairs`), this URL goes to the Nexus server,
            // whose decoder may not treat `+` as a space.
            url.query_pairs_mut()
                .append_pair("key", key)
                .append_pair("expires", &expires.to_string());
            let query = url.query().unwrap_or_default().replace('+', "%20");
            url.set_query(Some(&query));
        }
        let value = self.get_url_value(url).map_err(|error| {
            let status = error
                .chain()
                .find_map(|e| e.downcast_ref::<reqwest::Error>())
                .and_then(reqwest::Error::status);
            match status {
                Some(status) if keyed && matches!(status.as_u16(), 401 | 403 | 410) => {
                    anyhow::Error::new(NxmAuthorization::Refused(status.as_u16()))
                }
                _ => error,
            }
        })?;
        let links = value
            .as_array()
            .or_else(|| value.get("download_links").and_then(Value::as_array));
        let uri = links
            .and_then(|links| links.first())
            .and_then(|link| link.get("URI").or_else(|| link.get("uri")))
            .and_then(Value::as_str)
            .filter(|uri| reqwest::Url::parse(uri).is_ok_and(|url| url.scheme() == "https"))
            .ok_or_else(|| anyhow::anyhow!("Nexus returned no HTTPS download mirror"))?;
        let file_name = metadata
            .map(|metadata| metadata.file_name)
            .filter(|name| !name.is_empty())
            .or_else(|| {
                reqwest::Url::parse(uri)
                    .ok()
                    .and_then(|url| url.path_segments()?.next_back().map(str::to_owned))
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("nexus_{game_domain}_{mod_id}_{file_id}"));
        validate_download_file_name(&file_name)?;
        Ok(NxmDownloadLink {
            file_name,
            uri: uri.into(),
        })
    }

    pub fn md5_search(&mut self, game_domain: &str, md5: &str) -> Result<Vec<NexusMd5Result>> {
        let md5 = md5.trim();
        anyhow::ensure!(!md5.is_empty(), "md5 hash must be provided");
        #[derive(Deserialize)]
        struct Search {
            #[serde(rename = "mod")]
            mod_info: Option<NexusModInfo>,
            #[serde(default)]
            mod_id: i64,
            #[serde(rename = "file_details")]
            file: Option<NexusModFile>,
        }
        Ok(
            serde_json::from_value::<Vec<Search>>(self.get_value(&format!(
                "games/{game_domain}/mods/md5_search/{}.json",
                md5.to_ascii_lowercase()
            ))?)?
            .into_iter()
            .map(|entry| NexusMd5Result {
                mod_info: entry.mod_info,
                mod_id: entry.mod_id,
                file: entry.file,
            })
            .collect(),
        )
    }

    pub fn validate(&mut self) -> Result<NexusValidateResult> {
        if self
            .api_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
        {
            return Ok(NexusValidateResult {
                is_valid: false,
                user_name: String::new(),
                is_premium: false,
                message: "No Nexus Mods API key configured.".into(),
            });
        }
        let response = self.request(self.base_url.join("users/validate.json")?)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(NexusValidateResult {
                is_valid: false,
                user_name: String::new(),
                is_premium: false,
                message: "API key is invalid or unauthorized.".into(),
            });
        }
        let value: Value = serde_json::from_str(
            &response
                .error_for_status()
                .map_err(redact_query)?
                .text()
                .map_err(redact_query)?,
        )?;
        Ok(NexusValidateResult {
            is_valid: true,
            user_name: value["name"].as_str().unwrap_or_default().into(),
            is_premium: value["is_premium"].as_bool().unwrap_or(false),
            message: "API key is valid.".into(),
        })
    }

    fn get_json<T: DeserializeOwned>(&mut self, path: &str) -> Result<T> {
        Ok(serde_json::from_value(self.get_value(path)?)?)
    }

    fn get_value(&mut self, path: &str) -> Result<Value> {
        self.get_url_value(self.base_url.join(path)?)
    }

    fn get_url_value(&mut self, url: reqwest::Url) -> Result<Value> {
        let mut response = self.request(url.clone())?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let delay = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map_or(NEXUS_RETRY_AFTER_CAP, Duration::from_secs)
                .min(NEXUS_RETRY_AFTER_CAP);
            drop(response);
            if self.defer_rate_limits {
                return Err(crate::download::transient(
                    Some(delay.as_secs()),
                    anyhow::anyhow!("Nexus Mods rate limit (HTTP 429) for {}", url.path()),
                ));
            }
            std::thread::sleep(delay);
            response = self.request(url)?;
        }
        let response = response.error_for_status().map_err(redact_query)?;
        Ok(serde_json::from_str(
            &response.text().map_err(redact_query)?,
        )?)
    }

    fn request(&mut self, url: reqwest::Url) -> Result<reqwest::blocking::Response> {
        let mut request = self
            .client
            .get(url)
            .header("Application-Name", "ModSync")
            .header("Application-Version", "2.0.0a1")
            .header(reqwest::header::ACCEPT, "application/json")
            .header(
                reqwest::header::USER_AGENT,
                "ModSync/2.0.0a1 (https://github.com/th3w1zard1/ModSync)",
            );
        if let Some(key) = self.api_key.as_deref().filter(|key| !key.trim().is_empty()) {
            request = request.header("apikey", key);
        }
        let response = request.send().map_err(redact_query)?;
        self.daily_remaining =
            header_count(&response, "X-RL-Daily-Remaining").or(self.daily_remaining);
        self.hourly_remaining =
            header_count(&response, "X-RL-Hourly-Remaining").or(self.hourly_remaining);
        Ok(response)
    }
}

/// Drops the query from the URL a reqwest error reports, so an nxm download `key` (sent
/// in the `download_link.json` query) never reaches error text; the path is kept.
fn redact_query(mut error: reqwest::Error) -> reqwest::Error {
    if let Some(url) = error.url_mut() {
        url.set_query(None);
    }
    error
}

fn header_count(response: &reqwest::blocking::Response, name: &str) -> Option<i64> {
    response.headers().get(name)?.to_str().ok()?.parse().ok()
}

pub fn apply(guide: &mut Guide, versions: &Versions) -> Summary {
    let mut summary = Summary::default();
    for component in &mut guide.components {
        for (url, metadata) in &mut component.resource_registry {
            let Some(key) = nexus_key(url) else {
                summary.skipped_count += 1;
                continue;
            };
            let Some(version) = versions
                .0
                .get(&key)
                .filter(|version| !version.trim().is_empty())
            else {
                continue;
            };
            let Value::Object(metadata) = metadata else {
                continue;
            };
            apply_metadata(&mut summary, &component.name, url, metadata, version);
        }
    }
    summary
}

pub fn check(guide: &mut Guide, client: &mut NexusClient) -> Summary {
    let mut summary = Summary::default();
    let mut cache = BTreeMap::<String, Result<NexusModInfo, String>>::new();
    for component in &mut guide.components {
        for (url, metadata) in &mut component.resource_registry {
            let Some(key) = nexus_key(url) else {
                summary.skipped_count += 1;
                continue;
            };
            let Value::Object(metadata) = metadata else {
                continue;
            };
            if !cache.contains_key(&key) {
                if client.is_rate_limit_exhausted() {
                    summary.rate_limit_reached = true;
                    return summary;
                }
                let Some((domain, mod_id)) = key.split_once('/') else {
                    summary
                        .errors
                        .push(format!("{key}: invalid Nexus resource key"));
                    continue;
                };
                cache.insert(
                    key.clone(),
                    client
                        .get_mod_info(domain, mod_id)
                        .map_err(|error| error.to_string()),
                );
            }
            let info = match cache.get(&key).expect("cached Nexus request") {
                Ok(info) => info,
                Err(error) => {
                    let entry = format!("{key}: {error}");
                    if !summary.errors.contains(&entry) {
                        summary.errors.push(entry);
                    }
                    continue;
                }
            };
            apply_metadata(&mut summary, &component.name, url, metadata, &info.version);
        }
    }
    summary
}

pub fn component_update_available(component: &Component) -> bool {
    component.resource_registry.values().any(|metadata| {
        metadata
            .get("UpdateAvailable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })
}

fn apply_metadata(
    summary: &mut Summary,
    component_name: &str,
    url: &str,
    metadata: &mut serde_json::Map<String, Value>,
    version: &str,
) {
    let version = version.trim();
    if version.is_empty() {
        return;
    }
    let installed = metadata
        .get("ModVersion")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    metadata.insert("LatestKnownVersion".into(), json!(version));
    metadata.insert(
        "LastUpdateCheck".into(),
        json!(chrono::Utc::now().to_rfc3339()),
    );
    summary.checked_count += 1;
    if installed.is_empty() {
        metadata.insert("ModVersion".into(), json!(version));
        metadata.insert("UpdateAvailable".into(), json!(false));
    } else {
        let available = !installed.eq_ignore_ascii_case(version);
        metadata.insert("UpdateAvailable".into(), json!(available));
        if available {
            summary.updates_found.push(Update {
                component_name: component_name.into(),
                url: url.into(),
                installed_version: installed,
                latest_version: version.into(),
            });
        }
    }
}

pub fn load_versions(path: &std::path::Path) -> Result<Versions> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub fn save_summary(directory: &std::path::Path, summary: &Summary) -> Result<()> {
    crate::recovery::atomic_write(
        &directory.join("update-check-last.json"),
        &serde_json::to_vec_pretty(summary)?,
    )
}

pub fn load_summary(directory: &std::path::Path) -> Result<Option<Summary>> {
    let path = directory.join("update-check-last.json");
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&std::fs::read(path)?)?))
}

fn nexus_key(url: &str) -> Option<String> {
    let url = reqwest::Url::parse(url).ok()?;
    if !url.host_str()?.eq_ignore_ascii_case("www.nexusmods.com")
        && !url.host_str()?.eq_ignore_ascii_case("nexusmods.com")
    {
        return None;
    }
    let segments: Vec<_> = url.path_segments()?.collect();
    (segments.len() >= 3 && segments[1].eq_ignore_ascii_case("mods"))
        .then(|| format!("{}/{}", segments[0].to_ascii_lowercase(), segments[2]))
}
