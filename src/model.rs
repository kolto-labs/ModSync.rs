mod action;
mod boolean;
mod getter;
pub(crate) mod keys;
mod text;
pub(crate) mod yaml;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
/// The loaders drop blank Source list entries, so a single blank path comes from a
/// scalar `Source = ""` (or a `guide-edit` transaction, which bypasses the loader
/// filter); path resolution refuses it either way. Write it back as a
/// scalar so a saved or converted non-Choose guide keeps that refusal instead of
/// reading back as an empty list. A canonical Choose scalar reloads as an empty Choose.
fn source_out<S: Serializer>(source: &[String], s: S) -> Result<S::Ok, S::Error> {
    match source {
        [path] if path.trim().is_empty() => s.serialize_str(path),
        _ => source.serialize(s),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Guide {
    #[serde(rename = "thisMod", alias = "Components")]
    pub components: Vec<Component>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, serde_json::Value>,
    /// How `ordered` treats this guide; set by the loader and the CLI, never written.
    #[serde(skip)]
    pub ordering: Ordering,
}

/// Install-order rules that depend on how a guide was read and run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ordering {
    /// The file was read as Markdown. The C# Markdown ingest drops InstallBefore and
    /// InstallAfter, so they neither order nor stop the install (Dependencies still do).
    pub markdown: bool,
    /// Install the selection in list order, as C# does for `--direct-markdown`
    /// (`preserveInputOrder`).
    pub preserve_input_order: bool,
    /// Keep selected widescreen-only components after the base build, for the
    /// Base/Remaining phase split.
    pub widescreen_last: bool,
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
    #[serde(default, deserialize_with = "strings", serialize_with = "source_out")]
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

    /// Load as the reference CLI's `FileLoadingService.LoadFromFileAsync` does for
    /// validate and install. A Markdown extension goes through guide ingestion, which
    /// keeps the authored selection; any other file goes through
    /// `DeserializeModComponentFromString`, which selects every component when the file
    /// selects none.
    pub fn load_like_file_loader(path: &Path) -> Result<Self> {
        let markdown =
            canonical_format(path.extension().and_then(|s| s.to_str()).unwrap_or("")) == Some("md");
        Self::load_for_install_with_selection_default(path, !markdown)
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
        let mut result = Self::parse_with(text, format, true, default_selection);
        let normalized = format.trim().to_ascii_lowercase();
        let kind = canonical_format(&normalized).unwrap_or(
            if normalized.is_empty() || normalized == "auto" {
                "auto"
            } else {
                "unsupported"
            },
        );
        if let Ok(guide) = &mut result {
            // C#'s Markdown ingest drops InstallBefore/InstallAfter; `ordered` follows it.
            guide.ordering.markdown = match kind {
                "auto" => Self::markdown_text(text),
                kind => kind == "md",
            };
        }
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
            "toml" => {
                let mut guide = Self::from_value_with_format(
                    toml::from_str(text).context("Parse ModSync TOML")?,
                    keys::Format::Toml,
                )?;
                select_last_modified_key(&mut guide, true);
                guide
            }
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
                select_last_modified_key(&mut guide, false);
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
        if matches!(format, keys::Format::Yaml | keys::Format::Xml) {
            crate::draft::canonicalize_text_review_records(&mut value);
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
        let format =
            canonical_format(format).context("Save as TOML, JSON, YAML, XML or Markdown")?;
        if matches!(format, "toml" | "yaml") {
            normalize_last_modified(&mut metadata, format == "toml");
        }
        Ok(match format {
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
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let output = self.render(path.extension().and_then(|s| s.to_str()).unwrap_or("toml"))?;
        let path = std::path::absolute(path)?;
        crate::recovery::atomic_write(&path, output.as_bytes())
    }

    /// The GUIDs a Dependency or Restriction (of a component or an instruction) finds
    /// selected: each selected top-level component and its selected immediate options.
    /// C# `FindComponentFromGuid` looks no deeper, and its loader keeps no option below
    /// an option, so a GUID naming a nested option counts as not selected.
    pub fn selected_ids(&self) -> BTreeSet<String> {
        self.components
            .iter()
            .filter(|c| c.is_selected)
            .flat_map(|c| std::iter::once(c).chain(c.options.iter().filter(|o| o.is_selected)))
            .map(|c| c.guid.to_ascii_lowercase())
            .collect()
    }

    /// The order C# installs the selected components in
    /// (`InstallCoordinator.GetOrderedInstallList`): a breadth-first sort of every
    /// top-level component, selected or not, by Dependencies, InstallAfter and
    /// InstallBefore, starting from the dependency resolver's order as C# loads a guide
    /// (file order when that reports an error). A Markdown guide orders by Dependencies
    /// alone from file order, and `preserve_input_order` keeps the list order, as in C#.
    /// With `widescreen_last` (phased installs) selected widescreen-only components
    /// follow the base build unless a selected base component must follow one. Rust
    /// refuses Dependencies that are not selected, Restrictions conflicts, missing
    /// references and cycles among the selection; C# installs those components anyway.
    /// A Dependency may name an immediate option of a component, as C#
    /// `FindComponentFromGuid` looks those up too (not options nested below them): it is
    /// met when the option's and its component's selection flags are set, whether or not
    /// a Choose step runs the option, and it adds no ordering edge, as in C#.
    pub fn ordered(&self) -> Result<Vec<&Component>> {
        use crate::guid::key_ascii;
        let Ordering {
            markdown,
            preserve_input_order,
            widescreen_last,
        } = self.ordering;
        let selected: Vec<_> = self.components.iter().filter(|c| c.is_selected).collect();
        ensure!(!selected.is_empty(), "Select at least one component");
        let index: BTreeMap<_, _> = self
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| (key_ascii(&c.guid), i))
            .collect();
        ensure!(
            index.len() == self.components.len(),
            "Duplicate component GUID values in ordering graph"
        );
        let find = |guid: &String| index.get(&key_ascii(guid)).copied();
        let chosen = |i: usize| self.components[i].is_selected;
        let met: BTreeSet<_> = selected
            .iter()
            .flat_map(|c| std::iter::once(*c).chain(c.options.iter().filter(|o| o.is_selected)))
            .map(|c| key_ascii(&c.guid))
            .collect();
        for c in &selected {
            for required in &c.dependencies {
                ensure!(
                    met.contains(&key_ascii(required)),
                    "{} requires selected component {required}",
                    c.name
                );
            }
            for conflict in &c.restrictions {
                ensure!(
                    !find(conflict).is_some_and(chosen),
                    "{} conflicts with {conflict}",
                    c.name
                );
            }
            for after in order_references(&c.install_after, markdown) {
                ensure!(
                    find(after).is_some(),
                    "{} references missing InstallAfter {after}",
                    c.name
                );
            }
            for before in order_references(&c.install_before, markdown) {
                ensure!(
                    find(before).is_some(),
                    "{} references missing InstallBefore {before}",
                    c.name
                );
            }
        }
        if preserve_input_order {
            return Ok(selected);
        }
        let n = self.components.len();
        let base = match markdown {
            true => (0..n).collect(),
            false => match self.resolve_dependencies()? {
                Resolution {
                    errors,
                    order: Some(order),
                } if errors.is_empty() => order,
                _ => (0..n).collect(),
            },
        };
        let mut next = vec![Vec::new(); n];
        let mut indegree = vec![0_usize; n];
        for &i in &base {
            let c = &self.components[i];
            for j in c
                .dependencies
                .iter()
                .chain(order_references(&c.install_after, markdown))
                .filter_map(find)
            {
                next[j].push(i);
                indegree[i] += 1;
            }
            for j in order_references(&c.install_before, markdown)
                .iter()
                .filter_map(find)
            {
                next[i].push(j);
                indegree[j] += 1;
            }
        }
        let mut queue: std::collections::VecDeque<_> =
            base.iter().copied().filter(|&i| indegree[i] == 0).collect();
        let mut placed = vec![false; n];
        let mut order = Vec::with_capacity(n);
        while let Some(i) = queue.pop_front() {
            placed[i] = true;
            if chosen(i) {
                order.push(i);
            }
            for &j in &next[i] {
                indegree[j] -= 1;
                if indegree[j] == 0 {
                    queue.push_back(j);
                }
            }
        }
        if let Some(&i) = base.iter().find(|&&i| !placed[i] && chosen(i)) {
            bail!(
                "Dependency/order cycle: {} cannot be ordered",
                self.components[i].name
            );
        }
        if widescreen_last {
            let widescreen = |i: usize| crate::phase::widescreen(&self.components[i]);
            let mut later = vec![false; n];
            let mut stack: Vec<_> = order.iter().copied().filter(|&i| widescreen(i)).collect();
            while let Some(i) = stack.pop() {
                for &j in &next[i] {
                    if !later[j] {
                        later[j] = true;
                        stack.push(j);
                    }
                }
            }
            if !order.iter().any(|&i| later[i] && !widescreen(i)) {
                order.sort_by_key(|&i| widescreen(i));
            }
        }
        Ok(order.into_iter().map(|i| &self.components[i]).collect())
    }

    /// Whether `load(path)` reads the file as Markdown: by its extension, or by content
    /// when the extension names no format (the C# file loader's hint rule).
    pub fn markdown_input(path: &Path) -> Result<bool> {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        Ok(match canonical_format(extension) {
            Some(format) => format == "md",
            None => {
                let text = text::read(path).with_context(|| format!("Read {}", path.display()))?;
                Self::markdown_text(&text)
            }
        })
    }

    /// Whether `parse(text, "auto")` reads `text` as Markdown.
    pub fn markdown_text(text: &str) -> bool {
        Self::parse_with(text, "toml", true, true).is_err()
            && Self::parse_with(text, "md", true, true).is_ok()
    }

    /// C# `convert` (`ModBuildConverter.HandleDependencyResolutionErrors` over
    /// `DependencyResolverService.ResolveDependencies`): order every top-level component,
    /// selected or not, by InstallBefore/InstallAfter with a depth-first post-order,
    /// visiting components in guide order and each one's edges in authored order. Missing,
    /// self and circular references fail unless `ignore_errors`, which then orders around
    /// missing references but still refuses a cycle: C# writes an empty guide there.
    /// Returns the warning lines C# logs when `ignore_errors` lets errors through.
    pub fn sort_like_dependency_resolver(&mut self, ignore_errors: bool) -> Result<Vec<String>> {
        let Resolution { errors, order } = self.resolve_dependencies()?;
        let listed = listed_errors(&errors);
        ensure!(
            errors.is_empty() || ignore_errors,
            "Dependency resolution failed with {} errors. Use --ignore-errors flag to attempt loading in best possible order.\n{}",
            errors.len(),
            listed.join("\n")
        );
        let order = order.context(
            "Dependency resolution failed: a circular InstallBefore/InstallAfter order cannot be resolved, even with --ignore-errors. Remove the cycle from the guide.",
        )?;
        self.reorder(order);
        if errors.is_empty() {
            return Ok(Vec::new());
        }
        let mut warnings = vec![format!(
            "Dependency resolution failed with {} errors:",
            errors.len()
        )];
        warnings.extend(listed);
        Ok(warnings)
    }

    /// C# validate and install (`HandleDependencyResolutionErrors` without
    /// `--ignore-errors`, which Rust refuses there): a missing, self or circular
    /// InstallBefore/InstallAfter reference on any top-level component, selected or not,
    /// stops the run; otherwise the components take the resolver's order, as in convert.
    pub fn sort_for_install(&mut self) -> Result<()> {
        let Resolution { errors, order } = self.resolve_dependencies()?;
        ensure!(
            errors.is_empty(),
            "Dependency resolution failed with {} errors. Fix these InstallBefore/InstallAfter references in the guide (validate and install do not take --ignore-errors):\n{}",
            errors.len(),
            listed_errors(&errors).join("\n")
        );
        self.reorder(order.context("Circular InstallBefore/InstallAfter order")?);
        Ok(())
    }

    fn reorder(&mut self, order: Vec<usize>) {
        let mut slots: Vec<_> = std::mem::take(&mut self.components)
            .into_iter()
            .map(Some)
            .collect();
        self.components = order.into_iter().filter_map(|i| slots[i].take()).collect();
    }

    /// C# `DependencyResolverService.ResolveDependencies` over the top-level components:
    /// its errors in its order, and its depth-first post-order (`None` on a cycle).
    fn resolve_dependencies(&self) -> Result<Resolution> {
        use crate::guid::{key_ascii, option_text};
        let n = self.components.len();
        let mut index = BTreeMap::new();
        for (i, c) in self.components.iter().enumerate() {
            ensure!(
                index.insert(key_ascii(&c.guid), i).is_none(),
                "Duplicate Guid: {}",
                c.guid
            );
        }
        let find = |guid: &String| index.get(&key_ascii(guid)).copied();
        let mut errors = Vec::new();
        let mut graph: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, c) in self.components.iter().enumerate() {
            let lists = [
                ("InstallBefore", &c.install_before),
                ("InstallAfter", &c.install_after),
            ];
            for (field, list) in lists {
                for guid in list.iter().filter(|g| find(g).is_none()) {
                    errors.push(format!(
                        "{}: {field} references non-existent component with GUID: {}",
                        c.name,
                        option_text(guid)
                    ));
                }
            }
            for (field, list) in lists {
                if list.iter().any(|g| find(g) == Some(i)) {
                    errors.push(format!(
                        "{}: Component references itself in {field}",
                        c.name
                    ));
                }
            }
        }
        let mut edge = |from: usize, to: usize| {
            if !graph[from].contains(&to) {
                graph[from].push(to);
            }
        };
        for (i, c) in self.components.iter().enumerate() {
            for j in c.install_before.iter().filter_map(find) {
                edge(j, i);
            }
            for j in c.install_after.iter().filter_map(find) {
                edge(i, j);
            }
        }
        // C# reconstructs a cycle through the first component (in guide order) whose
        // edges hold the current one.
        let mut first_predecessor = vec![None; n];
        for (k, edges) in graph.iter().enumerate() {
            for &to in edges {
                first_predecessor[to].get_or_insert(k);
            }
        }
        let (mut visited, mut stack) = (vec![false; n], vec![false; n]);
        for root in 0..n {
            if !visited[root]
                && let Some(cycle) =
                    detect_cycle(root, &graph, &first_predecessor, &mut visited, &mut stack)
            {
                let mut names = cycle
                    .iter()
                    .take(SHOWN_ERRORS)
                    .map(|&i| self.components[i].name.as_str())
                    .collect::<Vec<_>>()
                    .join(" → ");
                if cycle.len() > SHOWN_ERRORS {
                    names.push_str(&format!(" → … ({} components)", cycle.len()));
                }
                errors.push(format!("{names}: Circular dependency detected: {names}"));
            }
        }
        let mut order = Vec::with_capacity(n);
        let (mut done, mut open) = (vec![false; n], vec![false; n]);
        let complete =
            (0..n).all(|root| post_order(root, &graph, &mut done, &mut open, &mut order));
        Ok(Resolution {
            errors,
            order: complete.then_some(order),
        })
    }
}

/// The InstallBefore/InstallAfter entries that order a guide: none for Markdown, whose
/// C# ingest drops them.
fn order_references(list: &[String], markdown: bool) -> &[String] {
    if markdown { &[] } else { list }
}

const SHOWN_ERRORS: usize = 50;

struct Resolution {
    errors: Vec<String>,
    order: Option<Vec<usize>>,
}

/// The error lines C# logs, at most `SHOWN_ERRORS` of them.
fn listed_errors(errors: &[String]) -> Vec<String> {
    let mut listed = errors
        .iter()
        .take(SHOWN_ERRORS)
        .map(|e| format!("  - {e}"))
        .collect::<Vec<_>>();
    if errors.len() > SHOWN_ERRORS {
        listed.push(format!("  … and {} more", errors.len() - SHOWN_ERRORS));
    }
    listed
}

/// C# `DependencyResolverService.DetectCycleDFS`, including its early return that leaves
/// the walked components on the recursion stack and its first-predecessor reconstruction.
/// The reconstruction stops at the first repeated component: a C# walk that repeats one
/// never finishes, so this cannot cut short a walk C# completes.
fn detect_cycle(
    current: usize,
    graph: &[Vec<usize>],
    first_predecessor: &[Option<usize>],
    visited: &mut [bool],
    stack: &mut [bool],
) -> Option<Vec<usize>> {
    visited[current] = true;
    stack[current] = true;
    for &next in &graph[current] {
        if !visited[next] {
            if let Some(cycle) = detect_cycle(next, graph, first_predecessor, visited, stack) {
                return Some(cycle);
            }
        } else if stack[next] {
            let mut cycle = Vec::new();
            let mut seen = vec![false; graph.len()];
            let mut temp = current;
            while temp != next && !seen[temp] {
                seen[temp] = true;
                cycle.push(temp);
                match first_predecessor[temp] {
                    Some(previous) => temp = previous,
                    None => break,
                }
            }
            cycle.push(next);
            cycle.reverse();
            return Some(cycle);
        }
    }
    stack[current] = false;
    None
}

/// C# `DependencyResolverService.TopologicalSortDFS`: a component follows everything it
/// must be installed after. False on a cycle.
fn post_order(
    current: usize,
    graph: &[Vec<usize>],
    done: &mut [bool],
    open: &mut [bool],
    order: &mut Vec<usize>,
) -> bool {
    if open[current] {
        return false;
    }
    if done[current] {
        return true;
    }
    done[current] = true;
    open[current] = true;
    for &next in &graph[current] {
        if !post_order(next, graph, done, open, order) {
            return false;
        }
    }
    open[current] = false;
    order.push(current);
    true
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
            // YAML (no implicit typing) and XML carry these typed resource
            // fields as text; consumers read them as numbers/Booleans.
            if matches!(format, keys::Format::Yaml | keys::Format::Xml)
                && let Some(resource) = resource.as_object_mut()
            {
                for field in ["FileSize", "ContentLength", "SchemaVersion"] {
                    if let Some(value) = resource.get_mut(field)
                        && let Some(number) =
                            value.as_str().and_then(|text| text.parse::<u64>().ok())
                    {
                        *value = number.into();
                    }
                }
                if let Some(value) = resource.get_mut("UpdateAvailable")
                    && let Some(text) = value.as_str()
                {
                    *value = Value::Bool(match text.to_ascii_lowercase().as_str() {
                        "true" => true,
                        "false" => false,
                        _ => bail!("Invalid boolean for UpdateAvailable"),
                    });
                }
            }
        }
    }
    if format == keys::Format::Yaml {
        // Matches the XML intake in xml.rs: desktop and install flows read these
        // as Booleans, so text spellings must not silently read as false.
        for field in ["WidescreenOnly", "AspyrExclusive"] {
            if let Some(value) = object.get_mut(field)
                && let Some(text) = value.as_str()
            {
                *value = Value::Bool(
                    text.to_ascii_lowercase()
                        .parse::<bool>()
                        .with_context(|| format!("Invalid boolean for {field}"))?,
                );
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
            // C# skips `PrefixPath` for a raw Action of Choose and reads a scalar
            // Source through the getter's string-to-list split, so `""`, `"  "` or
            // `" , "` is an empty Choose. Already-normalized values (the Markdown
            // second pass, which receives the scalar `""` written by `source_out`)
            // are not split, so a canonical-only Choose such as `Action = 9` stays refused.
            if action.raw_choose
                && format != keys::Format::Internal
                && let Some(source) = instruction.get_mut("Source")
                && let Value::String(text) = source
            {
                *source = Value::Array(choose_sources(text));
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

/// `GetValue<List<string>>` on a string: split on `;` and `,`, trim, drop blanks.
fn choose_sources(text: &str) -> Vec<serde_json::Value> {
    text.split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| serde_json::Value::String(part.into()))
        .collect()
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

/// Reference guide `lastModified` intake key choice
/// (`docs/parity/lastmodified-contract.md`): TOML reads only `lastModified`,
/// so a `LastModified` key is never stored and never saved. YAML reads
/// `LastModified` when present (with no fallback when it fails to parse),
/// else `lastModified`. The chosen value keeps its position as `lastModified`.
fn select_last_modified_key(guide: &mut Guide, toml: bool) {
    let Some(fields) = guide
        .metadata
        .get_mut("metadata")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    let Some(pascal) = fields.get("LastModified").cloned() else {
        return;
    };
    if toml {
        fields.shift_remove("LastModified");
        last_modified_warning(
            "Guide LastModified metadata is not read from TOML by the reference and is omitted on save".into(),
            &pascal,
        );
    } else {
        fields.shift_remove("lastModified");
        *fields = std::mem::take(fields)
            .into_iter()
            .map(|(key, value)| match key.as_str() {
                "LastModified" => ("lastModified".to_owned(), value),
                _ => (key, value),
            })
            .collect();
    }
}

/// Reference guide `lastModified` save contract
/// (`docs/parity/lastmodified-contract.md`): the reference stores the value
/// from invariant `DateTime.TryParse(..., DateTimeStyles.None)`, omits it when
/// that fails, writes YAML with the `O` pattern and TOML as a native local
/// date-time truncated to milliseconds. Forms outside the bounded shared date
/// model keep the authored value with a warning. The intake already chose the
/// key (`select_last_modified_key`).
fn normalize_last_modified(metadata: &mut serde_json::Value, toml: bool) {
    use crate::profiles::InvariantDate;
    use serde_json::Value;
    const TOML_DATETIME: &str = "$__toml_private_datetime";
    let Some(fields) = metadata.as_object_mut() else {
        return;
    };
    let Some(authored) = fields.get("lastModified") else {
        return;
    };
    let text = match authored {
        Value::Null => None,
        Value::Bool(value) => Some(if *value { "True" } else { "False" }.to_owned()),
        Value::String(text) => Some(text.clone()),
        // Integer ToString is culture-neutral; other number text is not.
        Value::Number(number) if !number.as_str().contains(['.', 'e', 'E']) => {
            Some(number.as_str().to_owned())
        }
        Value::Object(object) if object.len() == 1 => match object.get(TOML_DATETIME) {
            Some(Value::String(text)) => Some(tomlyn_text(text)),
            _ => return retain_last_modified("value is not a date-time scalar", authored),
        },
        _ => return retain_last_modified("value is not a date-time scalar", authored),
    };
    let outcome = text.map_or(InvariantDate::Rejected, |text| {
        crate::profiles::invariant_try_parse(&text)
    });
    match outcome {
        InvariantDate::Parsed { round_trip } => {
            let value = if toml {
                // Tomlyn writes a DateTime as a local date-time with
                // millisecond precision and no offset.
                let millis = &round_trip[20..23];
                let mut local = round_trip[..19].to_owned();
                if millis != "000" {
                    local.push('.');
                    local.push_str(millis);
                }
                serde_json::json!({ TOML_DATETIME: local })
            } else {
                Value::String(round_trip)
            };
            fields["lastModified"] = value;
        }
        InvariantDate::Rejected => {
            last_modified_warning(
                "Guide lastModified metadata is not a reference DateTime and is omitted on save"
                    .into(),
                authored,
            );
            fields.shift_remove("lastModified");
        }
        InvariantDate::Unsupported(reason) => retain_last_modified(&reason, authored),
    }
}
fn retain_last_modified(reason: &str, authored: &serde_json::Value) {
    last_modified_warning(
        format!(
            "Guide lastModified metadata retains its authored value: {}",
            reason.chars().take(512).collect::<String>()
        ),
        authored,
    );
}
/// Log a lastModified warning once per message and authored value in a row,
/// so repeated renders of one guide (the desktop raw preview) do not repeat it.
fn last_modified_warning(message: String, authored: &serde_json::Value) {
    static LAST: std::sync::Mutex<Option<(String, serde_json::Value)>> =
        std::sync::Mutex::new(None);
    let key = (message, authored.clone());
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if last.as_ref() != Some(&key) {
        crate::logging::warning(&key.0);
        *last = Some(key);
    }
}
/// Tomlyn's `TomlDateTime.ToString()` omits the fraction when its
/// millisecond component is zero.
fn tomlyn_text(text: &str) -> String {
    let Some(dot) = text.find('.') else {
        return text.to_owned();
    };
    let digits = text[dot + 1..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if text[dot + 1..dot + 1 + digits.min(3)]
        .bytes()
        .all(|b| b == b'0')
    {
        format!("{}{}", &text[..dot], &text[dot + 1 + digits..])
    } else {
        text.to_owned()
    }
}
