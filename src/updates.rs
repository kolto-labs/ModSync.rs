use crate::Guide;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Versions(pub BTreeMap<String, String>);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Summary {
    pub checked_count: usize,
    pub skipped_count: usize,
    pub updates_found: Vec<Update>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct Update {
    pub component_name: String,
    pub url: String,
    pub installed_version: String,
    pub latest_version: String,
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
            let installed = metadata
                .get("ModVersion")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_owned();
            metadata.insert("LatestKnownVersion".into(), json!(version.trim()));
            metadata.insert(
                "LastUpdateCheck".into(),
                json!(chrono::Utc::now().to_rfc3339()),
            );
            summary.checked_count += 1;
            if installed.is_empty() {
                metadata.insert("ModVersion".into(), json!(version.trim()));
                metadata.insert("UpdateAvailable".into(), json!(false));
            } else {
                let available = !installed.eq_ignore_ascii_case(version.trim());
                metadata.insert("UpdateAvailable".into(), json!(available));
                if available {
                    summary.updates_found.push(Update {
                        component_name: component.name.clone(),
                        url: url.clone(),
                        installed_version: installed,
                        latest_version: version.trim().into(),
                    });
                }
            }
        }
    }
    summary
}

pub fn load_versions(path: &std::path::Path) -> Result<Versions> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
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
