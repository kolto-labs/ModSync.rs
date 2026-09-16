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
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::One(s)) => vec![s],
        Some(Value::Many(v)) => v,
        None => Vec::new(),
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
    #[serde(deserialize_with = "strings")]
    pub dependencies: Vec<String>,
    #[serde(deserialize_with = "strings")]
    pub restrictions: Vec<String>,
    #[serde(deserialize_with = "strings")]
    pub install_before: Vec<String>,
    #[serde(deserialize_with = "strings")]
    pub install_after: Vec<String>,
    pub instructions: Vec<Instruction>,
    pub options: Vec<Component>,
    pub resource_registry: serde_json::Map<String, serde_json::Value>,
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
    #[serde(deserialize_with = "action")]
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
    #[serde(deserialize_with = "strings")]
    pub dependencies: Vec<String>,
    #[serde(default)]
    #[serde(deserialize_with = "strings")]
    pub restrictions: Vec<String>,
}

impl Guide {
    /// File-open/install ingestion drafts prose Markdown, matching the C# file loader.
    /// Conversion remains opt-in through draft::generate.
    pub fn load_for_install(path: &Path) -> Result<Self> {
        let mut guide = Self::load(path)?;
        if canonical_format(path.extension().and_then(|s| s.to_str()).unwrap_or("")) == Some("md") {
            crate::draft::generate(&mut guide);
        }
        Ok(guide)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("Read {}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes).replace('\u{fffd}', "_");
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let hint = canonical_format(extension);
        Self::parse(&text, hint.unwrap_or("auto"))
    }

    pub fn parse(text: &str, format: &str) -> Result<Self> {
        let text = text.trim_start_matches('\u{feff}');
        let format = format.trim().to_ascii_lowercase();
        if format.is_empty() || format == "auto" {
            for hint in ["toml", "md", "yaml", "xml", "json"] {
                if let Ok(guide) = Self::parse(text, hint) {
                    return Ok(guide);
                }
            }
            bail!("Cannot detect a supported instruction format");
        }
        let mut guide: Self = match canonical_format(&format).unwrap_or(&format) {
            "toml" => Self::from_value(toml::from_str(text).context("Parse ModSync TOML")?)?,
            "json" => {
                let mut value = serde_json::from_str(text)?;
                crate::resources::prepare_json(&mut value);
                Self::from_value(value)?
            }
            "xml" => Self::from_value(crate::xml::parse(text)?)?,
            "yaml" => {
                let mut guide = Self::default();
                for document in serde_yaml_ng::Deserializer::from_str(text) {
                    let value = serde_json::Value::deserialize(document)?;
                    if value.is_null() {
                        continue;
                    }
                    if value.get("fileFormatVersion").is_some() {
                        guide.metadata.insert("metadata".into(), value);
                    } else {
                        let parsed = Self::from_value(value)?;
                        guide.components.extend(parsed.components);
                        guide.metadata.extend(parsed.metadata);
                    }
                }
                guide
            }
            "md" => crate::markdown::parse(text)?,
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
        if !guide.components.iter().any(|c| c.is_selected) {
            for component in &mut guide.components {
                component.is_selected = true;
            }
        }
        Ok(guide)
    }

    pub(crate) fn from_value(mut value: serde_json::Value) -> Result<Self> {
        use serde_json::Value;
        if value.is_array() {
            value = serde_json::json!({"thisMod": value});
        } else if value.get("Guid").is_some() {
            value = serde_json::json!({"thisMod": [value]});
        }
        let object = value
            .as_object_mut()
            .context("Guide must contain components")?;
        if !object.contains_key("thisMod")
            && let Some(components) = object.remove("Components")
        {
            object.insert("thisMod".into(), components);
        }
        for component in object
            .get_mut("thisMod")
            .and_then(Value::as_array_mut)
            .context("Guide needs a component array")?
        {
            normalize_component(component)?;
        }
        Ok(serde_json::from_value(value)?)
    }

    pub fn render(&self, format: &str) -> Result<String> {
        Ok(
            match canonical_format(format).context("Save as TOML, JSON, YAML or XML")? {
                "toml" => {
                    let mut value = serde_json::to_value(self)?;
                    encode_nullable_files(&mut value);
                    toml::to_string_pretty(&value)?
                }
                "json" => serde_json::to_string_pretty(&self.components)?,
                "xml" => crate::xml::render(&serde_json::to_value(&self.components)?)?,
                "yaml" => {
                    let mut text = String::new();
                    if let Some(metadata) = self.metadata.get("metadata") {
                        text.push_str("---\n# Metadata\n");
                        text.push_str(&serde_yaml_ng::to_string(metadata)?);
                    }
                    for component in &self.components {
                        text.push_str("---\n");
                        let mut value = serde_json::to_value(component)?;
                        encode_nullable_files(&mut value);
                        yaml_legacy_resources(&mut value);
                        text.push_str(&serde_yaml_ng::to_string(&value)?);
                    }
                    text
                }
                _ => bail!("Markdown export is not yet implemented"),
            },
        )
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let output = self.render(path.extension().and_then(|s| s.to_str()).unwrap_or("toml"))?;
        let path = std::path::absolute(path)?;
        crate::recovery::atomic_write(&path, output.as_bytes())
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

fn canonical_format(format: &str) -> Option<&'static str> {
    match format.trim().to_ascii_lowercase().as_str() {
        "toml" | "tml" => Some("toml"),
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "xml" => Some("xml"),
        "md" | "markdown" | "mdown" | "mkdn" | "mkd" | "mdtxt" | "mdtext" | "text" => Some("md"),
        _ => None,
    }
}
fn action<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value.eq_ignore_ascii_case("TSLPatcher") || value.eq_ignore_ascii_case("HoloPatcher") {
        return Ok("Patcher".into());
    }
    Ok([
        "Unset",
        "Extract",
        "Execute",
        "Patcher",
        "Move",
        "Copy",
        "Rename",
        "Delete",
        "DelDuplicate",
        "Choose",
        "Run",
        "CleanList",
    ]
    .into_iter()
    .find(|action| action.eq_ignore_ascii_case(&value))
    .unwrap_or(&value)
    .to_owned())
}
fn normalize_component(value: &mut serde_json::Value) -> Result<()> {
    use serde_json::Value;
    let object = value
        .as_object_mut()
        .context("Component must be an object")?;
    object.retain(|_, value| !value.is_null());
    if let Some(tier) = object.get_mut("Tier")
        && let Some(text) = tier.as_str()
    {
        let lower = text.to_lowercase();
        *tier = [
            ("essential", "1 - Essential"),
            ("recommend", "2 - Recommended"),
            ("suggest", "3 - Suggested"),
            ("option", "4 - Optional"),
        ]
        .into_iter()
        .find(|(name, _)| lower.contains(name))
        .map_or(text.trim(), |(_, normalized)| normalized)
        .into();
    }
    for field in ["Category", "Language"] {
        if let Some(value) = object.get_mut(field) {
            let string = value.as_str().or_else(|| {
                value
                    .as_array()
                    .filter(|a| a.len() == 1)
                    .and_then(|a| a[0].as_str())
            });
            if let Some(string) = string {
                *value = Value::Array(
                    string
                        .split([',', ';'])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(|s| Value::String(s.into()))
                        .collect(),
                );
            }
        }
    }
    crate::resources::normalize_registry(object);
    if let Some(registry) = object
        .get_mut("ResourceRegistry")
        .and_then(Value::as_object_mut)
    {
        for resource in registry.values_mut() {
            if let Some(files) = resource.get_mut("Files").and_then(Value::as_object_mut) {
                for flag in files.values_mut() {
                    if let Value::String(text) = flag {
                        *flag = match text.to_ascii_lowercase().as_str() {
                            "null" => Value::Null,
                            "true" => Value::Bool(true),
                            "false" => Value::Bool(false),
                            _ => bail!("Invalid resource file selection: {text}"),
                        };
                    }
                }
            }
        }
    }
    crate::resources::migrate(object);
    if let Some(instructions) = object.get_mut("Instructions").and_then(Value::as_array_mut) {
        for instruction in instructions {
            let instruction = instruction
                .as_object_mut()
                .context("Instruction must be an object")?;
            instruction.retain(|_, value| !value.is_null());
            if instruction
                .get("Action")
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case("Delete"))
            {
                instruction.entry("Overwrite").or_insert(Value::Bool(false));
            }
        }
    }
    if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
        for option in options {
            normalize_component(option)?;
        }
    }
    Ok(())
}

fn yaml_legacy_resources(value: &mut serde_json::Value) {
    use serde_json::{Map, Value};
    let Some(component) = value.as_object_mut() else {
        return;
    };
    // C# YAML preprocessing flattens ResourceRegistry into a list and then
    // recovers URLs through the preserved ModLinkFilenames compatibility map.
    // Emit the same derived map without replacing the richer registry itself.
    let mut legacy = Map::new();
    if let Some(registry) = component.get("ResourceRegistry").and_then(Value::as_object) {
        for (url, resource) in registry {
            let files = resource
                .get("Files")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            legacy.insert(url.clone(), Value::Object(files));
        }
    }
    if let Some(existing) = component.get("ModLinkFilenames").and_then(Value::as_object) {
        for (url, files) in existing {
            if !legacy.keys().any(|key| key.eq_ignore_ascii_case(url)) {
                legacy.insert(url.clone(), files.clone());
            }
        }
    }
    if !legacy.is_empty() {
        component.insert("ModLinkFilenames".into(), Value::Object(legacy));
    }
    if let Some(options) = component.get_mut("Options").and_then(Value::as_array_mut) {
        for option in options {
            yaml_legacy_resources(option);
        }
    }
}

fn encode_nullable_files(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Array(values) => {
            for value in values {
                encode_nullable_files(value);
            }
        }
        Value::Object(object) => {
            if let Some(Value::Object(registry)) = object.get_mut("ResourceRegistry") {
                for resource in registry.values_mut() {
                    if let Some(Value::Object(files)) = resource.get_mut("Files") {
                        for flag in files.values_mut() {
                            if flag.is_null() {
                                *flag = Value::String("null".into());
                            }
                        }
                    }
                }
            }
            for value in object.values_mut() {
                encode_nullable_files(value);
            }
        }
        _ => {}
    }
}
