use super::{
    ConditionalFiles, FileInstall, FlagCondition, GroupSelection, GroupType, ModuleConfig,
    PluginSelection, StepSelection,
};
use anyhow::{Result, bail, ensure};
use quick_xml::{Reader, events::Event};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

#[derive(Default)]
struct Node {
    name: String,
    text: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Node>,
}
impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children
            .iter()
            .find(|n| n.name.eq_ignore_ascii_case(name))
    }
    fn attr(&self, name: &str) -> &str {
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map_or("", |(_, v)| v)
    }
    fn children(&self, name: &str) -> impl Iterator<Item = &Node> {
        self.children
            .iter()
            .filter(move |n| n.name.eq_ignore_ascii_case(name))
    }
}

const NODE_LIMIT: usize = 100_000;
const ATTRIBUTE_LIMIT: usize = 100_000;
const STRING_LIMIT: usize = 32 * 1024 * 1024;

#[derive(Default)]
struct Budget {
    nodes: usize,
    attributes: usize,
    strings: usize,
}
impl Budget {
    fn strings(&mut self, bytes: usize) -> Result<()> {
        ensure!(
            bytes <= STRING_LIMIT.saturating_sub(self.strings),
            "FOMOD XML retained string budget exceeded"
        );
        self.strings += bytes;
        Ok(())
    }
}

fn tree(text: &str, cancelled: &AtomicBool) -> Result<Node> {
    super::metadata::check(cancelled)?;
    ensure!(
        text.len() <= super::metadata::INPUT_LIMIT,
        "FOMOD metadata exceeds 8 MiB"
    );
    let mut budget = Budget::default();
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    let mut stack = vec![Node::default()];
    loop {
        super::metadata::check(cancelled)?;
        match reader.read_event()? {
            Event::Start(start) => {
                ensure!(stack.len() < 128, "FOMOD XML nesting is too deep");
                ensure!(budget.nodes < NODE_LIMIT, "FOMOD XML node budget exceeded");
                budget.nodes += 1;
                let local = start.local_name();
                let name = std::str::from_utf8(local.as_ref())?;
                budget.strings(name.len())?;
                let mut node = Node {
                    name: name.to_owned(),
                    ..Default::default()
                };
                for attr in start.attributes() {
                    super::metadata::check(cancelled)?;
                    ensure!(
                        budget.attributes < ATTRIBUTE_LIMIT,
                        "FOMOD XML attribute budget exceeded"
                    );
                    budget.attributes += 1;
                    let attr = attr?;
                    let local = attr.key.local_name();
                    let name = std::str::from_utf8(local.as_ref())?;
                    let value = attr.decoded_and_normalized_value(
                        quick_xml::XmlVersion::Implicit1_0,
                        reader.decoder(),
                    )?;
                    budget.strings(name.len())?;
                    budget.strings(value.len())?;
                    node.attrs.insert(name.to_owned(), value.into_owned());
                }
                stack.push(node);
            }
            Event::End(_) => {
                ensure!(stack.len() > 1, "Unexpected FOMOD XML end tag");
                let node = stack.pop().unwrap();
                if stack.len() > 1 {
                    budget.strings(node.text.len())?;
                    stack.last_mut().unwrap().text.push_str(&node.text);
                }
                stack.last_mut().unwrap().children.push(node);
            }
            Event::Text(text) => {
                let value = text.xml10_content()?;
                budget.strings(value.len())?;
                stack.last_mut().unwrap().text.push_str(&value);
            }
            Event::CData(text) => {
                let value = text.xml10_content()?;
                budget.strings(value.len())?;
                stack.last_mut().unwrap().text.push_str(&value);
            }
            Event::GeneralRef(reference) => {
                let decoded = reference.decode()?;
                budget.strings(decoded.len().saturating_add(2))?;
                let encoded = format!("&{decoded};");
                let value = quick_xml::escape::unescape(&encoded)?;
                budget.strings(value.len())?;
                stack.last_mut().unwrap().text.push_str(&value);
            }
            Event::DocType(_) => bail!("FOMOD XML DTDs and external entities are forbidden"),
            Event::Eof => break,
            _ => {}
        }
    }
    super::metadata::check(cancelled)?;
    ensure!(stack.len() == 1, "Unclosed FOMOD XML element");
    let mut root = stack.pop().unwrap();
    ensure!(
        root.children.len() == 1 && root.text.trim().is_empty(),
        "FOMOD XML needs one root"
    );
    Ok(root.children.remove(0))
}

fn condition(node: &Node) -> Result<FlagCondition> {
    let mut children = Vec::new();
    for child in &node.children {
        children.push(match child.name.to_ascii_lowercase().as_str() {
            "flagdependency" => FlagCondition::Flag {
                name: child.attr("flag").into(),
                value: child.attr("value").into(),
            },
            // The pinned C# mapper logs these as unmodeled and evaluates them
            // as true. Preserve that deterministic behavior without opening
            // or probing authored game paths during validation.
            "filedependency" => {
                FlagCondition::Always(format!("fileDependency '{}'", child.attr("file")))
            }
            "gamedependency" => {
                FlagCondition::Always(format!("gameDependency '{}'", child.attr("version")))
            }
            "dependencies" => condition(child)?,
            other => bail!("Unsupported FOMOD visibility dependency: {other}"),
        });
    }
    Ok(if node.attr("operator").eq_ignore_ascii_case("Or") {
        FlagCondition::Any(children)
    } else {
        FlagCondition::All(children)
    })
}

/// Read the selection projection of ModuleConfig.xml. This does not authorize installation
/// or map file directives; the engine's unconfigured-FOMOD rejection remains mandatory.
pub fn parse_selection_xml(text: &str) -> Result<Vec<StepSelection>> {
    Ok(parse_module_xml(text)?.steps)
}

/// Parse module data without resolving paths or executing file directives.
pub fn parse_module_xml(text: &str) -> Result<ModuleConfig> {
    parse_module_xml_with_cancel(text, &AtomicBool::new(false))
}

/// Parse bounded metadata, observing cancellation at each XML event and mapping boundaries.
pub fn parse_module_xml_with_cancel(text: &str, cancelled: &AtomicBool) -> Result<ModuleConfig> {
    let root = tree(text, cancelled)?;
    ensure!(
        root.name.eq_ignore_ascii_case("config"),
        "FOMOD root must be config"
    );
    let mut steps = Vec::new();
    if let Some(install_steps) = root.child("installSteps") {
        for step in install_steps.children("installStep") {
            let mut groups = Vec::new();
            if let Some(optional) = step.child("optionalFileGroups") {
                for group in optional.children("group") {
                    let kind = match group.attr("type").to_ascii_lowercase().as_str() {
                        "selectexactlyone" => GroupType::SelectExactlyOne,
                        "selectatmostone" => GroupType::SelectAtMostOne,
                        "selectatleastone" => GroupType::SelectAtLeastOne,
                        "selectall" => GroupType::SelectAll,
                        _ => GroupType::SelectAny,
                    };
                    let mut plugins = Vec::new();
                    if let Some(list) = group.child("plugins") {
                        for plugin in list.children("plugin") {
                            let descriptor = plugin.child("typeDescriptor");
                            let plugin_type = descriptor
                                .and_then(|d| {
                                    d.child("type").or_else(|| {
                                        d.child("dependencyType")
                                            .and_then(|d| d.child("defaultType"))
                                    })
                                })
                                .map_or("", |t| t.attr("name"));
                            let plugin_type = crate::fomod::PluginType::from_xml(plugin_type);
                            let required = plugin_type == crate::fomod::PluginType::Required;
                            let flags = plugin
                                .child("conditionFlags")
                                .map(|f| {
                                    f.children("flag")
                                        .map(|f| (f.attr("name").into(), f.text.trim().into()))
                                        .collect()
                                })
                                .unwrap_or_default();
                            plugins.push(PluginSelection {
                                name: plugin.attr("name").into(),
                                description: plugin
                                    .child("description")
                                    .map_or("", |n| n.text.trim())
                                    .into(),
                                image_path: plugin
                                    .child("image")
                                    .map_or("", |n| n.attr("path"))
                                    .into(),
                                files: files(plugin.child("files")),
                                plugin_type,
                                required,
                                selected: required
                                    || plugin_type == crate::fomod::PluginType::Recommended,
                                flags,
                            });
                        }
                    }
                    groups.push(GroupSelection::new(
                        group.attr("name").into(),
                        kind,
                        plugins,
                    ));
                }
            }
            steps.push(StepSelection {
                name: step.attr("name").into(),
                groups,
                visible: step.child("visible").map(condition).transpose()?,
            });
        }
    }
    let mut conditional_files = Vec::new();
    if let Some(patterns) = root
        .child("conditionalFileInstalls")
        .and_then(|n| n.child("patterns"))
    {
        for pattern in patterns.children("pattern") {
            conditional_files.push(ConditionalFiles {
                condition: pattern
                    .child("dependencies")
                    .map(condition)
                    .transpose()?
                    .unwrap_or_else(|| FlagCondition::All(vec![])),
                files: files(pattern.child("files")),
            });
        }
    }
    let config = ModuleConfig {
        name: root
            .child("moduleName")
            .map_or("", |n| n.text.as_str())
            .into(),
        required_files: files(root.child("requiredInstallFiles")),
        steps,
        conditional_files,
    };
    super::metadata::check(cancelled)?;
    Ok(config)
}

fn files(container: Option<&Node>) -> Vec<FileInstall> {
    container
        .into_iter()
        .flat_map(|n| &n.children)
        .filter(|n| n.name.eq_ignore_ascii_case("file") || n.name.eq_ignore_ascii_case("folder"))
        .map(|n| FileInstall {
            source: n.attr("source").into(),
            destination: n.attr("destination").into(),
            priority: n.attr("priority").trim().parse().unwrap_or_default(),
            is_folder: n.name.eq_ignore_ascii_case("folder"),
        })
        .collect()
}

/// Parse optional info.xml fields, preserving authored whitespace and mixed text.
/// Like the reference parser, root names are unrestricted and the first matching
/// direct child supplies each field. DTDs remain forbidden at the shared XML boundary.
pub fn parse_info_xml(text: &str) -> Result<super::FomodInfo> {
    parse_info_xml_with_cancel(text, &AtomicBool::new(false))
}

/// Parse bounded optional metadata with the caller's cancellation flag.
pub fn parse_info_xml_with_cancel(text: &str, cancelled: &AtomicBool) -> Result<super::FomodInfo> {
    let root = tree(text, cancelled)?;
    let value = |name| root.child(name).map_or("", |n| n.text.as_str()).to_owned();
    let info = super::FomodInfo {
        name: value("Name"),
        author: value("Author"),
        version: value("Version"),
        website: value("Website"),
        description: value("Description"),
    };
    super::metadata::check(cancelled)?;
    Ok(info)
}
