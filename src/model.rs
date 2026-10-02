mod action;
mod boolean;
mod getter;
pub(crate) mod keys;
mod text;
pub(crate) mod yaml;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    #[serde(deserialize_with = "boolean::deserialize")]
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
#[serde(rename_all = "PascalCase")]
pub struct Instruction {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub guid: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent: String,
    #[serde(default = "action::unset", deserialize_with = "action::deserialize")]
    pub action: String,
    #[serde(default, deserialize_with = "strings")]
    pub source: Vec<String>,
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub arguments: String,
    #[serde(default = "yes", deserialize_with = "boolean::deserialize")]
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
        Self::load_for_install_with_selection_default(path, true)
    }

    /// Draft install Markdown while retaining authored all-false selection.
    /// CLI adapters apply profile or default-all selection after this ingestion.
    pub fn load_for_install_preserving_selection(path: &Path) -> Result<Self> {
        Self::load_for_install_with_selection_default(path, false)
    }

    fn load_for_install_with_selection_default(
        path: &Path,
        default_selection: bool,
    ) -> Result<Self> {
        let mut guide = if default_selection {
            Self::load(path)?
        } else {
            Self::load_preserving_selection(path)?
        };
        if canonical_format(path.extension().and_then(|s| s.to_str()).unwrap_or("")) == Some("md") {
            crate::draft::generate(&mut guide);
        }
        Ok(guide)
    }

    /// Load an instruction file without replacing authored all-false selection.
    /// Acquisition applies its transient defaults on a separate execution clone.
    pub fn load_preserving_selection(path: &Path) -> Result<Self> {
        Self::load_with_selection_default(path, false)
    }

    pub fn load(path: &Path) -> Result<Self> {
        Self::load_with_selection_default(path, true)
    }

    fn load_with_selection_default(path: &Path, default_selection: bool) -> Result<Self> {
        let text = text::read(path).with_context(|| format!("Read {}", path.display()))?;
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let hint = canonical_format(extension);
        Self::parse_observed(&text, hint.unwrap_or("auto"), default_selection)
    }

    pub fn parse(text: &str, format: &str) -> Result<Self> {
        Self::parse_observed(text, format, true)
    }

    /// Parse an authored document without choosing components on its behalf.
    pub fn parse_preserving_selection(text: &str, format: &str) -> Result<Self> {
        Self::parse_observed(text, format, false)
    }

    /// Markdown's fenced instruction blocks parse through here: the C# Markdown ingest path
    /// never runs load-time duplicate-option removal, so neither may its embedded guides.
    pub(crate) fn parse_without_dedup(
        text: &str,
        format: &str,
        default_selection: bool,
    ) -> Result<Self> {
        Self::parse_with(text, format, false, default_selection)
    }

    fn parse_observed(text: &str, format: &str, default_selection: bool) -> Result<Self> {
        let started = std::time::Instant::now();
        let result = Self::parse_with(text, format, true, default_selection);
        let normalized = format.trim().to_ascii_lowercase();
        let kind = canonical_format(&normalized).unwrap_or(
            if normalized.is_empty() || normalized == "auto" {
                "auto"
            } else {
                "unsupported"
            },
        );
        let error = result.as_ref().err().map(|error| format!("{error:#}"));
        crate::telemetry::global().record_parsing_operation(
            kind,
            result.is_ok(),
            result.as_ref().map_or(0, |guide| guide.components.len()),
            started.elapsed(),
            error.as_deref(),
        );
        result
    }

    pub(crate) fn parse_yaml_without_dedup(
        text: &str,
        default_selection: bool,
        budget: &mut yaml::Budget,
    ) -> Result<Self> {
        Self::parse_with_yaml_budget(text, "yaml", false, default_selection, budget)
    }

    fn parse_with(text: &str, format: &str, dedup: bool, default_selection: bool) -> Result<Self> {
        Self::parse_with_yaml_budget(
            text,
            format,
            dedup,
            default_selection,
            &mut yaml::Budget::default(),
        )
    }

    fn parse_with_yaml_budget(
        text: &str,
        format: &str,
        dedup: bool,
        default_selection: bool,
        yaml_budget: &mut yaml::Budget,
    ) -> Result<Self> {
        ensure!(
            text.len() as u64 <= text::GUIDE_LIMIT,
            "Guide exceeds 8 MiB"
        );
        let text = text.trim_start_matches('\u{feff}');
        let format = format.trim().to_ascii_lowercase();
        if format.is_empty() || format == "auto" {
            for hint in ["toml", "md", "yaml"] {
                if let Ok(guide) = Self::parse_with(text, hint, dedup, default_selection) {
                    return Ok(guide);
                }
            }
            let terminal = if text.trim_start().starts_with('<') {
                "xml"
            } else {
                "json"
            };
            return Self::parse_with(text, terminal, dedup, default_selection).with_context(|| {
                format!("Autodetect instruction format: terminal {terminal} parser")
            });
        }
        let mut guide: Self = match canonical_format(&format).unwrap_or(&format) {
            "toml" => Self::from_value_with_format(
                toml::from_str(text).context("Parse ModSync TOML")?,
                keys::Format::Toml,
            )?,
            "json" => {
                let value = serde_json::from_str(text)?;
                Self::from_value_with_format(value, keys::Format::Json)?
            }
            "xml" => Self::from_value_with_format(crate::xml::parse(text)?, keys::Format::Xml)?,
            "yaml" => {
                let mut guide = Self::default();
                let mut text_context = getter::Context::default();
                let separators = regex::Regex::new(r"(?m)^---[ \t]*(?:#[^\r\n]*)?\r?$")?;
                let directive_start = regex::Regex::new(r"(?m)^%")?;
                let mut directives = String::new();
                for source in separators.split(text) {
                    if source.lines().any(|line| line.starts_with('%'))
                        && source.lines().all(|line| {
                            line.trim().is_empty()
                                || line.starts_with('%')
                                || line.trim_start().starts_with('#')
                        })
                    {
                        directives.push_str(source);
                        directives.push_str("\n---\n");
                        continue;
                    }
                    // A directive after an explicit document end belongs to the next
                    // document, whose start marker was removed by the split above.
                    let next_directive = directive_start
                        .find(source)
                        .filter(|m| source[..m.start()].trim_end().ends_with("..."));
                    let (source, next_directives) =
                        next_directive.map_or((source, ""), |m| source.split_at(m.start()));
                    let marked_metadata = source
                        .trim_start()
                        .get(..10)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("# Metadata"));
                    let document_source = format!("{}{}", std::mem::take(&mut directives), source);
                    for value in yaml::documents(&document_source, yaml_budget)? {
                        if value.is_null() {
                            continue;
                        }
                        if marked_metadata || value.get("fileFormatVersion").is_some() {
                            ensure!(value.is_object(), "YAML metadata must be a mapping");
                            guide.metadata.insert("metadata".into(), value);
                        } else {
                            let parsed = Self::from_value_with_text_context(
                                value,
                                keys::Format::Yaml,
                                &mut text_context,
                            )?;
                            guide.components.extend(parsed.components);
                            guide.metadata.extend(parsed.metadata);
                        }
                    }
                    if !next_directives.is_empty() {
                        directives.push_str(next_directives);
                        directives.push_str("\n---\n");
                    }
                }
                ensure!(directives.is_empty(), "YAML directives require a document");
                guide
            }
            "md" => crate::markdown::parse_with_selection_default(text, default_selection)?,
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
        // C# runs RemoveDuplicateOptions only in DeserializeModComponentFromString
        // (TOML/JSON/YAML/XML); Markdown ingest goes through MarkdownParser and keeps duplicates.
        if dedup && canonical_format(&format) != Some("md") {
            for component in &mut guide.components {
                remove_duplicate_options(component);
            }
        }
        if default_selection && !guide.components.iter().any(|c| c.is_selected) {
            for component in &mut guide.components {
                component.is_selected = true;
            }
        }
        Ok(guide)
    }

    pub(crate) fn from_value(value: serde_json::Value) -> Result<Self> {
        Self::from_value_with_format(value, keys::Format::Internal)
    }

    pub(crate) fn from_value_with_format(
        value: serde_json::Value,
        format: keys::Format,
    ) -> Result<Self> {
        Self::from_value_with_text_context(value, format, &mut getter::Context::default())
    }

    fn from_value_with_text_context(
        mut value: serde_json::Value,
        format: keys::Format,
        text_context: &mut getter::Context,
    ) -> Result<Self> {
        use serde_json::Value;
        crate::draft::validate_raw_review_aliases(&value)?;
        if value.is_array() {
            value = serde_json::json!({"thisMod": value});
        } else if value
            .as_object()
            .and_then(|object| keys::getter(object, "Guid"))
            .is_some()
        {
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
        let mut work = 0;
        let components = object
            .get_mut("thisMod")
            .and_then(Value::as_array_mut)
            .context("Guide needs a component array")?;
        if format == keys::Format::Json {
            let original = components.len();
            components.retain(Value::is_object);
            let ignored = original - components.len();
            if ignored != 0 {
                eprintln!("Warning: ignored {ignored} nonobject JSON component entries");
            }
        }
        let mut access = Vec::with_capacity(components.len());
        for component in components {
            access.push(keys::component(component, format, false, 0, &mut work)?);
        }
        crate::draft::validate_review_records(&value)?;
        if format == keys::Format::Json {
            crate::resources::prepare_json(&mut value);
        }
        for (component, access) in value
            .get_mut("thisMod")
            .and_then(Value::as_array_mut)
            .context("Guide needs a component array")?
            .iter_mut()
            .zip(&access)
        {
            normalize_component(component, access, format, false, text_context)?;
        }
        Ok(serde_json::from_value(value)?)
    }

    pub fn render(&self, format: &str) -> Result<String> {
        const CONTENT_FIELDS: &[&str] = &[
            "preambleContent",
            "epilogueContent",
            "widescreenWarningContent",
            "aspyrExclusiveWarningContent",
            "installationWarningContent",
        ];
        let mut metadata = self
            .metadata
            .get("metadata")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        for key in CONTENT_FIELDS {
            if let Some(content) = self.metadata.get(*key) {
                let fields = metadata
                    .as_object_mut()
                    .context("Guide metadata must be an object")?;
                let pascal = format!("{}{}", key[..1].to_ascii_uppercase(), &key[1..]);
                if !fields.contains_key(&pascal) {
                    fields.entry(*key).or_insert_with(|| content.clone());
                }
            }
        }
        if let Some(fields) = metadata.as_object_mut()
            && !fields.is_empty()
        {
            fields
                .entry("fileFormatVersion")
                .or_insert_with(|| "2.0".into());
        }
        Ok(
            match canonical_format(format).context("Save as TOML, JSON, YAML, XML or Markdown")? {
                "toml" => {
                    let mut value = serde_json::to_value(self)?;
                    for key in CONTENT_FIELDS {
                        value.as_object_mut().unwrap().remove(*key);
                    }
                    if metadata.as_object().is_none_or(|m| !m.is_empty()) {
                        value["metadata"] = metadata.clone();
                    }
                    encode_nullable_files(&mut value);
                    let mut budget = crate::json_native::ValueBudget::default();
                    crate::json_native::to_toml_pretty(&value, &mut budget)?
                }
                "json" => serde_json::to_string_pretty(&self.components)?,
                "xml" => crate::xml::render(&serde_json::to_value(&self.components)?)?,
                "yaml" => {
                    let mut text = String::new();
                    let mut budget = crate::json_native::ValueBudget::default();
                    if metadata.as_object().is_none_or(|m| !m.is_empty()) {
                        text.push_str("---\n# Metadata\n");
                        text.push_str(&crate::json_native::to_yaml_string(&metadata, &mut budget)?);
                    }
                    for component in &self.components {
                        text.push_str("---\n");
                        let mut value = serde_json::to_value(component)?;
                        encode_nullable_files(&mut value);
                        yaml_legacy_resources(&mut value);
                        text.push_str(&crate::json_native::to_yaml_string(&value, &mut budget)?);
                    }
                    text
                }
                "md" => crate::markdown::render(self, &metadata)?,
                _ => unreachable!("canonical_format returned an unsupported format"),
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
        use crate::guid::{Identity, key_ascii};
        let selected: Vec<_> = self.components.iter().filter(|c| c.is_selected).collect();
        ensure!(!selected.is_empty(), "Select at least one component");
        let all: BTreeSet<_> = self.components.iter().map(|c| key_ascii(&c.guid)).collect();
        ensure!(
            all.len() == self.components.len(),
            "Duplicate component GUID values in ordering graph"
        );
        let ids: BTreeSet<_> = selected.iter().map(|c| key_ascii(&c.guid)).collect();
        let mut predecessors: BTreeMap<Identity, BTreeSet<Identity>> =
            ids.iter().map(|id| (id.clone(), BTreeSet::new())).collect();
        for c in &selected {
            let id = key_ascii(&c.guid);
            for required in &c.dependencies {
                let required_key = key_ascii(required);
                ensure!(
                    ids.contains(&required_key),
                    "{} requires selected component {required}",
                    c.name
                );
                predecessors.get_mut(&id).unwrap().insert(required_key);
            }
            for conflict in &c.restrictions {
                ensure!(
                    !ids.contains(&key_ascii(conflict)),
                    "{} conflicts with {conflict}",
                    c.name
                );
            }
            for after in &c.install_after {
                let after_key = key_ascii(after);
                ensure!(
                    all.contains(&after_key),
                    "{} references missing InstallAfter {after}",
                    c.name
                );
                if ids.contains(&after_key) {
                    predecessors.get_mut(&id).unwrap().insert(after_key);
                }
            }
            for before in &c.install_before {
                let before_key = key_ascii(before);
                ensure!(
                    all.contains(&before_key),
                    "{} references missing InstallBefore {before}",
                    c.name
                );
                if ids.contains(&before_key) {
                    predecessors
                        .get_mut(&before_key)
                        .unwrap()
                        .insert(id.clone());
                }
            }
        }
        let mut result = Vec::new();
        let mut done = BTreeSet::new();
        while result.len() < selected.len() {
            let next = selected.iter().find(|c| {
                let id = key_ascii(&c.guid);
                !done.contains(&id) && predecessors[&id].is_subset(&done)
            });
            let Some(c) = next else {
                bail!("Dependency/order cycle among selected components");
            };
            done.insert(key_ascii(&c.guid));
            result.push(*c);
        }
        Ok(result)
    }
}

/// C# compares instruction Dependencies and Restrictions as `List<Guid>` and matches Choose
/// sources with `Guid.TryParse`, so case and brace differences do not count.
fn guid_key(value: &str) -> Option<uuid::Uuid> {
    crate::guid::try_parse(value)
}

fn same_guids(left: &[String], right: &[String]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| match (guid_key(a), guid_key(b)) {
                (Some(a), Some(b)) => a == b,
                _ => a == b,
            })
}

/// C# rewrites non-Choose Sources at deserialize time (`Serializer.DeserializePathInDictionary`:
/// `PathHelper.FixPathFormatting` then `PrefixPath`) before `AreInstructionsIdentical` sees
/// them. Use the same idempotent rewrite for stored Sources and equality.
fn normalize_source(path: &str) -> String {
    fn fix(path: &str) -> String {
        let mut out = String::with_capacity(path.len());
        for c in path.trim_start_matches('\\').chars() {
            let c = if c == '\\' { '/' } else { c };
            if c == '/' && out.ends_with('/') && !out[..out.len() - 1].ends_with(':') {
                continue;
            }
            out.push(c);
        }
        if out.len() > 1 {
            out.truncate(out.trim_end_matches('/').len());
        }
        out
    }
    if path.trim().is_empty() {
        return path.to_owned();
    }
    let fixed = fix(path);
    let lower = fixed.to_ascii_lowercase();
    if [
        "<<moddirectory>>",
        "<<kotordirectory>>",
        "<<gamedirectory>>",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
    {
        fixed
    } else {
        fix(&format!("<<modDirectory>>/{fixed}"))
    }
}

/// Port of C# `AreInstructionsIdentical`: instruction Guid and Parent are not compared.
fn same_instructions(left: &[Instruction], right: &[Instruction]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.action == b.action
                && a.destination == b.destination
                && a.arguments == b.arguments
                && a.overwrite == b.overwrite
                && a.source.len() == b.source.len()
                && (a.action == "Choose" && a.source == b.source
                    || a.action != "Choose"
                        && a.source
                            .iter()
                            .zip(&b.source)
                            .all(|(x, y)| normalize_source(x) == normalize_source(y)))
                && same_guids(&a.dependencies, &b.dependencies)
                && same_guids(&a.restrictions, &b.restrictions)
        })
}

/// Port of C# `RemoveDuplicateOptions`: an option whose instructions match an earlier
/// option's is dropped (names are ignored), and its GUID leaves the component's Choose lists.
fn remove_duplicate_options(component: &mut Component) {
    let mut drop = vec![false; component.options.len()];
    let mut dropped = BTreeSet::new();
    for i in 0..component.options.len() {
        if drop[i] {
            continue;
        }
        let kept = &component.options[i];
        for (j, duplicate) in component.options.iter().enumerate().skip(i + 1) {
            if !drop[j] && same_instructions(&kept.instructions, &duplicate.instructions) {
                drop[j] = true;
                if let Some(id) = guid_key(&duplicate.guid) {
                    dropped.insert(id);
                }
                eprintln!(
                    "Component '{}': Duplicate option detected - '{}' (GUID: {}) has identical \
                     instructions to '{}' (GUID: {}). Removing duplicate.",
                    component.name, duplicate.name, duplicate.guid, kept.name, kept.guid
                );
            }
        }
    }
    if !drop.contains(&true) {
        return;
    }
    let mut index = 0;
    component.options.retain(|_| {
        index += 1;
        !drop[index - 1]
    });
    for instruction in &mut component.instructions {
        if instruction.action == "Choose" {
            instruction
                .source
                .retain(|source| guid_key(source).is_none_or(|id| !dropped.contains(&id)));
        }
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
fn normalize_component(
    value: &mut serde_json::Value,
    access: &keys::ComponentAccess,
    format: keys::Format,
    option: bool,
    text_context: &mut getter::Context,
) -> Result<()> {
    use serde_json::Value;
    let object = value
        .as_object_mut()
        .context("Component must be an object")?;
    object.retain(|_, value| !value.is_null());
    if let Some(selected) = object.get_mut("IsSelected") {
        *selected = Value::Bool(boolean::optional_for_format(selected, format)?);
    }
    let text_fields: &[&str] = if option {
        &["Name", "Description"]
    } else {
        &["Name", "Author", "Description", "Directions", "Tier"]
    };
    for field in text_fields {
        if let Some(value) = object.get_mut(*field) {
            *value = Value::String(text_context.scalar(value, format)?);
        }
    }
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
    if !option {
        for field in ["Category", "Language"] {
            if let Some(value) = object.get_mut(field) {
                *value = Value::Array(text_context.list(value, format)?);
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
        for (instruction, evidence) in instructions.iter_mut().zip(&access.instructions) {
            let instruction = instruction
                .as_object_mut()
                .context("Instruction must be an object")?;
            instruction.retain(|_, value| !value.is_null());
            // keys::component already enforced literal Overwrite presence.
            // An admitted but invalid value defaults false, not action-default.
            if let Some(overwrite) = instruction.get_mut("Overwrite") {
                *overwrite = Value::Bool(boolean::optional_for_format(overwrite, format)?);
            }
            let action = action::intake(instruction.get("Action").unwrap_or(&Value::Null))?;
            if let Some(Value::Array(paths)) = instruction.get_mut("Source") {
                paths.retain(|path| {
                    !path.is_null() && !path.as_str().is_some_and(|text| text.trim().is_empty())
                });
            }
            if evidence.source
                && !action.raw_choose
                && let Some(source) = instruction.get_mut("Source")
            {
                match source {
                    Value::String(path) => *path = normalize_source(path),
                    Value::Array(paths) => {
                        for path in paths {
                            if let Value::String(text) = path {
                                *text = normalize_source(text);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if action.canonical == "Delete" {
                instruction.entry("Overwrite").or_insert(Value::Bool(false));
            }
            instruction.insert("Action".into(), Value::String(action.canonical));
        }
    }
    if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
        for (option, access) in options.iter_mut().zip(&access.options) {
            normalize_component(option, access, format, true, text_context)?;
        }
    }
    Ok(())
}

pub(crate) fn yaml_legacy_resources(value: &mut serde_json::Value) {
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

pub(crate) fn encode_nullable_files(value: &mut serde_json::Value) {
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
