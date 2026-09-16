use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

fn strings<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Value::deserialize(d)? {
        Value::One(s) => vec![s],
        Value::Many(v) => v,
    })
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Guide {
    #[serde(rename = "thisMod", alias = "Components")]
    pub components: Vec<Component>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Component {
    pub guid: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub directions: String,
    pub tier: String,
    #[serde(deserialize_with = "strings")]
    pub category: Vec<String>,
    pub is_selected: bool,
    pub dependencies: Vec<String>,
    pub restrictions: Vec<String>,
    pub install_before: Vec<String>,
    pub install_after: Vec<String>,
    pub instructions: Vec<Instruction>,
    pub options: Vec<Component>,
    pub resource_registry: BTreeMap<String, serde_json::Value>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Instruction {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub guid: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent: String,
    pub action: String,
    #[serde(default, deserialize_with = "strings")]
    pub source: Vec<String>,
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub arguments: String,
    #[serde(default = "yes")]
    pub overwrite: bool,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub restrictions: Vec<String>,
}

impl Guide {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("Read {}", path.display()))?;
        Self::parse(
            &text,
            path.extension().and_then(|s| s.to_str()).unwrap_or("toml"),
        )
    }

    pub fn parse(text: &str, format: &str) -> Result<Self> {
        let text = text.trim_start_matches('\u{feff}');
        let guide: Self = match format.to_ascii_lowercase().as_str() {
            "toml" => toml::from_str(text).context("Parse ModSync TOML")?,
            "json" => {
                if text.trim_start().starts_with('[') {
                    Self {
                        components: serde_json::from_str(text)?,
                        ..Self::default()
                    }
                } else {
                    serde_json::from_str(text)?
                }
            }
            "yaml" | "yml" => serde_yaml_ng::from_str(text)?,
            "md" => {
                let mut guides = Vec::new();
                for (index, block) in text.split("```").enumerate() {
                    if index % 2 == 0 {
                        continue;
                    }
                    if let Some((lang, content)) = block.split_once('\n')
                        && matches!(lang.trim(), "toml" | "json" | "yaml" | "yml")
                    {
                        guides.push(Self::parse(content, lang.trim())?);
                    }
                }
                ensure!(
                    !guides.is_empty(),
                    "Markdown needs a fenced TOML, JSON, or YAML instruction block. Prose is not executable instructions."
                );
                Self {
                    components: guides.into_iter().flat_map(|g| g.components).collect(),
                    ..Self::default()
                }
            }
            other => bail!("Unsupported instruction format: {other}"),
        };
        ensure!(!guide.components.is_empty(), "The guide has no components");
        let mut ids = BTreeSet::new();
        fn check(c: &Component, ids: &mut BTreeSet<String>) -> Result<()> {
            ensure!(
                !c.guid.trim().is_empty() && !c.name.trim().is_empty(),
                "Every component and option needs a Guid and Name"
            );
            ensure!(
                ids.insert(c.guid.to_ascii_lowercase()),
                "Duplicate Guid: {}",
                c.guid
            );
            for option in &c.options {
                check(option, ids)?;
            }
            Ok(())
        }
        for c in &guide.components {
            check(c, &mut ids)?;
        }
        Ok(guide)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let output = match path.extension().and_then(|s| s.to_str()).unwrap_or("toml") {
            "toml" => toml::to_string_pretty(self)?,
            "json" => serde_json::to_string_pretty(self)?,
            "yaml" | "yml" => serde_yaml_ng::to_string(self)?,
            _ => bail!("Save as .toml, .json, or .yaml"),
        };
        fs::write(path, output)?;
        Ok(())
    }

    pub fn selected_ids(&self) -> BTreeSet<String> {
        fn collect(c: &Component, ids: &mut BTreeSet<String>) {
            if c.is_selected {
                ids.insert(c.guid.to_ascii_lowercase());
                for option in &c.options {
                    collect(option, ids);
                }
            }
        }
        let mut ids = BTreeSet::new();
        for c in &self.components {
            collect(c, &mut ids);
        }
        ids
    }

    /// Stable topological sort: keep guide order whenever graph constraints allow it.
    pub fn ordered(&self) -> Result<Vec<&Component>> {
        let selected: Vec<_> = self.components.iter().filter(|c| c.is_selected).collect();
        ensure!(!selected.is_empty(), "Select at least one component");
        let all: BTreeSet<_> = self
            .components
            .iter()
            .map(|c| c.guid.to_ascii_lowercase())
            .collect();
        let ids: BTreeSet<_> = selected
            .iter()
            .map(|c| c.guid.to_ascii_lowercase())
            .collect();
        let mut predecessors: BTreeMap<String, BTreeSet<String>> =
            ids.iter().map(|id| (id.clone(), BTreeSet::new())).collect();
        for c in &selected {
            let id = c.guid.to_ascii_lowercase();
            for required in &c.dependencies {
                ensure!(
                    ids.contains(&required.to_ascii_lowercase()),
                    "{} requires selected component {required}",
                    c.name
                );
                predecessors
                    .get_mut(&id)
                    .unwrap()
                    .insert(required.to_ascii_lowercase());
            }
            for conflict in &c.restrictions {
                ensure!(
                    !ids.contains(&conflict.to_ascii_lowercase()),
                    "{} conflicts with {conflict}",
                    c.name
                );
            }
            for after in &c.install_after {
                let after = after.to_ascii_lowercase();
                ensure!(
                    all.contains(&after),
                    "{} references missing InstallAfter {after}",
                    c.name
                );
                if ids.contains(&after) {
                    predecessors.get_mut(&id).unwrap().insert(after);
                }
            }
            for before in &c.install_before {
                let before = before.to_ascii_lowercase();
                ensure!(
                    all.contains(&before),
                    "{} references missing InstallBefore {before}",
                    c.name
                );
                if ids.contains(&before) {
                    predecessors.get_mut(&before).unwrap().insert(id.clone());
                }
            }
        }
        let mut result = Vec::new();
        let mut done = BTreeSet::new();
        while result.len() < selected.len() {
            let next = selected.iter().find(|c| {
                let id = c.guid.to_ascii_lowercase();
                !done.contains(&id) && predecessors[&id].is_subset(&done)
            });
            let Some(c) = next else {
                bail!("Dependency/order cycle among selected components");
            };
            done.insert(c.guid.to_ascii_lowercase());
            result.push(*c);
        }
        Ok(result)
    }
}
