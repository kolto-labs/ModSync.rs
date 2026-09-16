use super::{FlagCondition, GroupSelection, GroupType, PluginSelection, StepSelection};
use anyhow::{Result, bail, ensure};
use quick_xml::{Reader, events::Event};
use std::collections::BTreeMap;

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

fn tree(text: &str) -> Result<Node> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    let mut stack = vec![Node::default()];
    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                ensure!(stack.len() < 128, "FOMOD XML nesting is too deep");
                let mut node = Node {
                    name: String::from_utf8(start.local_name().as_ref().to_vec())?,
                    ..Default::default()
                };
                for attr in start.attributes() {
                    let attr = attr?;
                    node.attrs.insert(
                        String::from_utf8(attr.key.local_name().as_ref().to_vec())?,
                        attr.decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )?
                        .into_owned(),
                    );
                }
                stack.push(node);
            }
            Event::End(_) => {
                ensure!(stack.len() > 1, "Unexpected FOMOD XML end tag");
                let node = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(node);
            }
            Event::Text(text) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&text.xml10_content()?),
            Event::CData(text) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&text.xml10_content()?),
            Event::GeneralRef(reference) => {
                stack
                    .last_mut()
                    .unwrap()
                    .text
                    .push_str(&quick_xml::escape::unescape(&format!(
                        "&{};",
                        reference.decode()?
                    ))?)
            }
            Event::DocType(_) => bail!("FOMOD XML DTDs and external entities are forbidden"),
            Event::Eof => break,
            _ => {}
        }
    }
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
    let root = tree(text)?;
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
                            let required = plugin_type.eq_ignore_ascii_case("Required");
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
                                required,
                                selected: required
                                    || plugin_type.eq_ignore_ascii_case("Recommended"),
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
    Ok(steps)
}
