use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub user_consented: bool,
    #[serde(default)]
    pub consent_date: Option<String>,
    #[serde(default)]
    pub anonymous_user_id: String,
    #[serde(default = "yes")]
    pub collect_usage_data: bool,
    #[serde(default = "yes")]
    pub collect_performance_metrics: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default = "default_client_version")]
    pub client_version: String,
    #[serde(skip)]
    pub signing_secret: Option<String>,
}
fn default_client_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}
fn yes() -> bool {
    true
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            user_consented: false,
            consent_date: None,
            anonymous_user_id: uuid::Uuid::new_v4().to_string(),
            collect_usage_data: true,
            collect_performance_metrics: true,
            endpoint: String::new(),
            session_id: String::new(),
            client_version: default_client_version(),
            signing_secret: None,
        }
    }
}

fn secret(directory: &Path) -> Option<String> {
    std::env::var("MODSYNC_SIGNING_SECRET")
        .ok()
        .or_else(|| std::env::var("KOTORMODSYNC_SIGNING_SECRET").ok())
        .or_else(|| std::fs::read_to_string(directory.join("telemetry.key")).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

impl Config {
    pub fn load(directory: &Path) -> Result<Self> {
        let path = directory.join("telemetry_config.json");
        let mut config = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("Parse telemetry configuration")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(error).context("Read telemetry configuration"),
        };
        if config.anonymous_user_id.trim().is_empty() {
            config.anonymous_user_id = uuid::Uuid::new_v4().to_string();
        }
        config.session_id = uuid::Uuid::new_v4().to_string();
        config.signing_secret = secret(directory);
        Ok(config)
    }

    pub fn save(&self, directory: &Path) -> Result<()> {
        crate::recovery::atomic_write(
            &directory.join("telemetry_config.json"),
            &serde_json::to_vec_pretty(self)?,
        )
    }

    pub fn set_consent(&mut self, enabled: bool) {
        self.user_consented = enabled;
        self.enabled = enabled;
        self.consent_date = enabled.then(|| chrono::Utc::now().to_rfc3339());
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub name: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Export {
    Disabled,
    Sent,
}
impl Config {
    pub fn active(&self) -> bool {
        self.enabled
            && self.user_consented
            && self
                .signing_secret
                .as_deref()
                .is_some_and(|secret| !secret.is_empty())
    }
}

pub fn signature(secret: &str, session: &str, path: &str, timestamp: i64) -> String {
    let mut key = secret.as_bytes().to_vec();
    if key.len() > 64 {
        key = Sha256::digest(&key).to_vec();
    }
    key.resize(64, 0);
    let message = format!("POST|{path}|{timestamp}|{session}");
    let mut inner = Sha256::new();
    inner.update(key.iter().map(|byte| byte ^ 0x36).collect::<Vec<_>>());
    inner.update(message.as_bytes());
    let mut outer = Sha256::new();
    outer.update(key.iter().map(|byte| byte ^ 0x5c).collect::<Vec<_>>());
    outer.update(inner.finalize());
    outer
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn export(config: &Config, path: &str, event: &Event) -> Result<Export> {
    if !config.active() || !config.collect_usage_data || config.endpoint.trim().is_empty() {
        return Ok(Export::Disabled);
    }
    anyhow::ensure!(path.starts_with('/'), "Telemetry path must start with '/'");
    anyhow::ensure!(
        !config.session_id.trim().is_empty(),
        "Telemetry session ID is required"
    );
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock predates Unix epoch")?
        .as_secs() as i64;
    let endpoint = format!("{}{}", config.endpoint.trim_end_matches('/'), path);
    let body = serde_json::to_vec(event).context("Serialize telemetry event")?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Build telemetry HTTP client")?;
    let response = client
        .post(&endpoint)
        .header(
            "X-KMS-Signature",
            signature(
                config.signing_secret.as_deref().unwrap(),
                &config.session_id,
                path,
                timestamp,
            ),
        )
        .header("X-KMS-Timestamp", timestamp.to_string())
        .header("X-KMS-Session-ID", &config.session_id)
        .header("X-KMS-Client-Version", &config.client_version)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .with_context(|| format!("Send telemetry event to {endpoint}"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "Telemetry endpoint returned {}",
        response.status()
    );
    Ok(Export::Sent)
}
