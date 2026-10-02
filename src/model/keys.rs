//! Format-specific known-property access. Dynamic dictionary keys stay authored.
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Internal,
    Toml,
    Json,
    Yaml,
    Xml,
}
impl Format {
    fn converted(self) -> bool {
        matches!(self, Self::Json | Self::Xml)
    }
    fn insensitive(self) -> bool {
        self.converted() || self == Self::Yaml
    }
}
const COMPONENT: &[&str] = &[
    "Guid",
    "Name",
    "Author",
    "Heading",
    "Category",
    "Language",
    "Tier",
    "Description",
    "DescriptionSpoilerFree",
    "Directions",
    "DirectionsSpoilerFree",
    "InstallationMethod",
    "DownloadInstructions",
    "DownloadInstructionsSpoilerFree",
    "UsageWarning",
    "UsageWarningSpoilerFree",
    "Screenshots",
    "ScreenshotsSpoilerFree",
    "ExcludedDownloads",
    "ModLink",
    "ModLinkFilenames",
    "Dependencies",
    "Restrictions",
    "InstallBefore",
    "InstallAfter",
    "IsSelected",
    "Instructions",
    "Options",
    "ResourceRegistry",
];
const OPTION: &[&str] = &[
    "Name",
    "Description",
    "Guid",
    "Restrictions",
    "Dependencies",
    "Instructions",
    "IsSelected",
];
const INSTRUCTION: &[&str] = &[
    "Guid",
    "Parent",
    "Action",
    "Source",
    "Destination",
    "Arguments",
    "Overwrite",
    "Dependencies",
    "Restrictions",
];
const RESOURCE: &[&str] = &[
    "ContentKey",
    "MetadataHash",
    "FileSize",
    "Files",
    "HandlerMetadata",
    "FirstSeen",
    "LastVerified",
    "PrimaryUrl",
];
#[derive(Default)]
pub(super) struct ComponentAccess {
    pub instructions: Vec<InstructionAccess>,
    pub options: Vec<ComponentAccess>,
}
#[derive(Default)]
pub(super) struct InstructionAccess {
    pub source: bool,
    // Kept separately from canonical output: the reference preprocesses these
    // exact fields before performing its case-insensitive getter fallback.
    #[allow(dead_code)] // Captured for the existing opaque Rust GUID-string contract.
    pub dependencies: bool,
    #[allow(dead_code)] // No new GUID coercion is introduced by property projection.
    pub restrictions: bool,
}
pub(crate) fn getter(object: &Map<String, Value>, field: &str) -> Option<String> {
    object
        .contains_key(field)
        .then(|| field.to_owned())
        .or_else(|| {
            object
                .keys()
                .find(|key| crate::unicode_case::ordinal_eq(key, field))
                .cloned()
        })
}
fn known(key: &str, fields: &[&str]) -> bool {
    fields
        .iter()
        .any(|field| crate::unicode_case::ordinal_eq(key, field))
}
fn prepare_dictionary(
    object: &mut Map<String, Value>,
    format: Format,
    fields: &[&str],
) -> Result<()> {
    if format == Format::Yaml {
        let mut seen = std::collections::BTreeSet::new();
        for key in object.keys() {
            ensure!(
                seen.insert(crate::unicode_case::ordinal_key(key)),
                "YAML property aliases collide"
            );
        }
    } else if format.converted() {
        let old = std::mem::take(object);
        let mut spellings = std::collections::BTreeMap::<String, String>::new();
        for (key, value) in old {
            if !known(&key, fields) {
                object.insert(key, value);
                continue;
            }
            // All known schema names begin with ASCII. Preserve the remainder,
            // including all-caps spelling significant to ordinal instructions.
            let mut chars = key.chars();
            let first = chars.next().context("Empty known property")?;
            let normalized = format!(
                "{}{}",
                crate::unicode_case::ordinal_key(&first.to_string()),
                chars.as_str()
            );
            let folded = crate::unicode_case::ordinal_key(&normalized);
            let retained = spellings.entry(folded).or_insert(normalized).clone();
            object.insert(retained, value);
        }
    }
    Ok(())
}
fn project(object: &mut Map<String, Value>, field: &str, selected: Option<String>) {
    let value = selected.and_then(|key| object.remove(&key));
    object.retain(|key, _| !crate::unicode_case::ordinal_eq(key, field));
    if let Some(value) = value {
        object.insert(field.into(), value);
    }
}
/// XML scalar/list coercion must use the retained virtual dictionary spelling.
pub(crate) fn xml_dictionary(
    object: &mut Map<String, Value>,
    instruction: bool,
    option: bool,
) -> Result<()> {
    prepare_dictionary(
        object,
        Format::Xml,
        if instruction {
            INSTRUCTION
        } else if option {
            OPTION
        } else {
            COMPONENT
        },
    )
}
pub(crate) fn resource(object: &mut Map<String, Value>, format: Format) -> Result<()> {
    prepare_dictionary(object, format, RESOURCE)?;
    for field in RESOURCE {
        let selected = getter(object, field);
        project(object, field, selected);
    }
    Ok(())
}
pub(super) fn component(
    value: &mut Value,
    format: Format,
    option: bool,
    depth: usize,
    work: &mut usize,
) -> Result<ComponentAccess> {
    ensure!(depth <= 64, "Component property nesting exceeds 64");
    *work = work.checked_add(1).context("Property work overflow")?;
    ensure!(*work <= 100_000, "Component/property record bound exceeded");
    let object = value
        .as_object_mut()
        .context("Component must be an object")?;
    *work = work
        .checked_add(object.len())
        .context("Property work overflow")?;
    ensure!(*work <= 100_000, "Component/property record bound exceeded");
    let fields = if option { OPTION } else { COMPONENT };
    prepare_dictionary(object, format, fields)?;
    for field in fields {
        let selected = if *field == "Instructions" && !option && !format.insensitive() {
            object.contains_key(*field).then(|| (*field).to_owned())
        } else if *field == "ResourceRegistry" && !format.insensitive() {
            ["ResourceRegistry", "resourceRegistry"]
                .into_iter()
                .find(|key| object.contains_key(*key))
                .map(str::to_owned)
        } else {
            getter(object, field)
        };
        if *field == "Instructions" && selected.is_none() && getter(object, field).is_some() {
            crate::logging::warning(
                "Ignored noncanonical executable container omitted while loading guide",
            );
        }
        project(object, field, selected);
    }
    if let Some(registry) = object
        .get_mut("ResourceRegistry")
        .and_then(Value::as_object_mut)
    {
        for metadata in registry.values_mut() {
            *work += 1;
            ensure!(*work <= 100_000, "Component/property record bound exceeded");
            if let Some(metadata) = metadata.as_object_mut() {
                *work = work
                    .checked_add(metadata.len())
                    .context("Property work overflow")?;
                ensure!(*work <= 100_000, "Component/property record bound exceeded");
                resource(metadata, format)?;
            }
        }
    }
    let mut access = ComponentAccess::default();
    if let Some(instructions) = object.get_mut("Instructions").and_then(Value::as_array_mut) {
        for instruction in instructions {
            *work += 1;
            ensure!(*work <= 100_000, "Component/property record bound exceeded");
            let instruction = instruction
                .as_object_mut()
                .context("Instruction must be an object")?;
            *work = work
                .checked_add(instruction.len())
                .context("Property work overflow")?;
            ensure!(*work <= 100_000, "Component/property record bound exceeded");
            prepare_dictionary(
                instruction,
                if format == Format::Yaml {
                    Format::Internal
                } else {
                    format
                },
                INSTRUCTION,
            )?;
            // SerializeIntoDictionary ignores null properties after JSON collapse.
            instruction.retain(|_, value| !value.is_null());
            let evidence = InstructionAccess {
                source: instruction.contains_key("Source"),
                dependencies: instruction.contains_key("Dependencies"),
                restrictions: instruction.contains_key("Restrictions"),
            };
            for field in INSTRUCTION {
                let selected = if *field == "Overwrite" {
                    instruction
                        .contains_key(*field)
                        .then(|| (*field).to_owned())
                } else {
                    getter(instruction, field)
                };
                project(instruction, field, selected);
            }
            access.instructions.push(evidence);
        }
    }
    if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
        for option in options {
            access
                .options
                .push(component(option, format, true, depth + 1, work)?);
        }
    }
    Ok(access)
}
