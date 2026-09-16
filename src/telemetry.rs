use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub user_consented: bool,
    #[serde(default = "yes")]
    pub collect_usage_data: bool,
    #[serde(default = "yes")]
    pub collect_performance_metrics: bool,
    #[serde(skip)]
    pub signing_secret: Option<String>,
}
fn yes() -> bool {
    true
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            user_consented: false,
            collect_usage_data: true,
            collect_performance_metrics: true,
            signing_secret: None,
        }
    }
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
