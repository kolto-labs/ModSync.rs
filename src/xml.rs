use anyhow::{Context, Result, bail, ensure};
use quick_xml::{Reader, events::Event};
use serde_json::{Map, Value};

#[derive(Default)]
struct Element {
    name: String,
    text: String,
    array: bool,
    fields: Map<String, Value>,
}

/// Read Newtonsoft's repeated-element collection shape without resolving DTDs or entities.
pub fn parse(text: &str) -> Result<Value> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    let mut stack = vec![Element::default()];
    loop {
        match reader.read_event()? {
            Event::Start(start) | Event::Empty(start) => {
                // Empty tags are expanded by the reader configuration below.
                ensure!(stack.len() < 128, "XML nesting is too deep");
                let name = decode_name(std::str::from_utf8(start.name().as_ref())?)?;
                let mut array = false;
                for attr in start.attributes() {
                    let attr = attr?;
                    let key = std::str::from_utf8(attr.key.as_ref())?;
                    if key == "json:Array" {
                        array = attr.value.as_ref() == b"true";
                    }
                    ensure!(
                        key.starts_with("xmlns") || key == "json:Array",
                        "Unsupported XML attribute: {key}"
                    );
                }
                stack.push(Element {
                    name,
                    array,
                    ..Default::default()
                });
            }
            Event::End(_) => {
                ensure!(stack.len() > 1, "Unexpected XML end tag");
                let element = stack.pop().unwrap();
                let value = if element.fields.is_empty() {
                    Value::String(element.text)
                } else {
                    ensure!(
                        element.text.trim().is_empty(),
                        "Mixed XML content is unsupported"
                    );
                    Value::Object(element.fields)
                };
                let value = if element.array {
                    Value::Array(vec![value])
                } else {
                    value
                };
                let parent = stack.last_mut().unwrap();
                match parent.fields.entry(element.name) {
                    serde_json::map::Entry::Vacant(entry) => {
                        entry.insert(value);
                    }
                    serde_json::map::Entry::Occupied(mut entry) => {
                        if let Value::Array(values) = entry.get_mut() {
                            values.push(value);
                        } else {
                            let old = entry.insert(Value::Null);
                            entry.insert(Value::Array(vec![old, value]));
                        }
                    }
                }
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
                let reference = format!("&{};", reference.decode()?);
                stack
                    .last_mut()
                    .unwrap()
                    .text
                    .push_str(&quick_xml::escape::unescape(&reference)?);
            }
            Event::DocType(_) => bail!("XML DTDs and external entities are forbidden"),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(stack.len() == 1, "Unclosed XML element");
    let root = stack.pop().unwrap();
    ensure!(
        root.fields.len() == 1 && root.text.trim().is_empty(),
        "XML needs exactly one root"
    );
    let (name, mut value) = root.fields.into_iter().next().unwrap();
    if name != "ModComponents" {
        let object = value.as_object_mut().context("XML root needs components")?;
        value = object
            .remove("ModComponents")
            .or_else(|| object.remove("thisMod"))
            .context("XML has no ModComponents")?;
    }
    let mut components = array(value, "ModComponents");
    for component in &mut components {
        normalize(component, false, false)?;
    }
    Ok(Value::Array(components))
}

fn array(value: Value, name: &str) -> Vec<Value> {
    match value {
        Value::Null => vec![],
        Value::String(s) if s.is_empty() => vec![],
        Value::Array(values) => values,
        Value::Object(object)
            if object.len() == 1
                && object.keys().any(|key| {
                    key.eq_ignore_ascii_case(name)
                        || key.eq_ignore_ascii_case(name.strip_suffix('s').unwrap_or(name))
                }) =>
        {
            array(object.into_iter().next().unwrap().1, name)
        }
        value => vec![value],
    }
}
fn normalize(value: &mut Value, instruction: bool, option: bool) -> Result<()> {
    crate::draft::validate_raw_review_aliases(value)?;
    let object = value
        .as_object_mut()
        .context("XML component/instruction must be an object")?;
    crate::model::keys::xml_dictionary(object, instruction, option)?;
    for field in [
        "IsSelected",
        "Overwrite",
        "WidescreenOnly",
        "AspyrExclusive",
    ] {
        let Some(key) = crate::model::keys::getter(object, field) else {
            continue;
        };
        if let Some(Value::String(value)) = object.get(&key) {
            let parsed = value
                .to_ascii_lowercase()
                .parse::<bool>()
                .with_context(|| format!("Invalid boolean for {field}"))?;
            object.insert(key, Value::Bool(parsed));
        }
    }
    for field in [
        "Category",
        "Language",
        "Dependencies",
        "Restrictions",
        "InstallBefore",
        "InstallAfter",
        "Source",
    ] {
        if let Some(key) = crate::model::keys::getter(object, field)
            && let Some(value) = object.remove(&key)
        {
            object.insert(key, Value::Array(array(value, field)));
        }
    }
    if instruction {
        return Ok(());
    }
    for field in ["Instructions", "Options"] {
        if let Some(key) = crate::model::keys::getter(object, field)
            && let Some(value) = object.remove(&key)
        {
            let mut values = array(value, field);
            for value in &mut values {
                normalize(value, field == "Instructions", field == "Options")?;
            }
            object.insert(key, Value::Array(values));
        }
    }
    for field in ["ResourceRegistry", "ModLinkFilenames"] {
        if let Some(key) = crate::model::keys::getter(object, field)
            && let Some(value) = object.remove(&key)
        {
            let mut registry = Map::new();
            for entry in array(value, field) {
                let mut entry = entry
                    .as_object()
                    .context("XML resource entry must be an object")?
                    .clone();
                let url = entry
                    .remove("Url")
                    .or_else(|| entry.remove("url"))
                    .or_else(|| entry.remove("PrimaryUrl"))
                    .or_else(|| entry.remove("primaryUrl"))
                    .context("XML resource needs Url")?;
                let url = url.as_str().context("Resource Url must be text")?;
                crate::model::keys::resource(&mut entry, crate::model::keys::Format::Xml)?;
                for field in ["FileSize", "ContentLength", "SchemaVersion"] {
                    if let Some(Value::String(text)) = entry.get(field)
                        && let Ok(number) = text.parse::<u64>()
                    {
                        entry.insert(field.into(), number.into());
                    }
                }
                if let Some(Value::Object(files)) = entry.get_mut("Files") {
                    for value in files.values_mut() {
                        if let Value::String(text) = value {
                            *value = if text.is_empty() {
                                Value::Null
                            } else {
                                Value::Bool(
                                    text.parse().context("Invalid resource file selection")?,
                                )
                            };
                        }
                    }
                } else if entry.get("Files").is_some_and(|v| v == "") {
                    entry.insert("Files".into(), Value::Object(Map::new()));
                }
                if entry.get("HandlerMetadata").is_some_and(|v| v == "") {
                    entry.insert("HandlerMetadata".into(), Value::Object(Map::new()));
                }
                let mut value = entry
                    .remove("Value")
                    .or_else(|| entry.remove("value"))
                    .unwrap_or(Value::Object(entry));
                if field.eq_ignore_ascii_case("ModLinkFilenames")
                    && let Value::Object(files) = &mut value
                {
                    for flag in files.values_mut() {
                        if let Value::String(text) = flag
                            && let Ok(boolean) = text.parse::<bool>()
                        {
                            *flag = Value::Bool(boolean);
                        }
                    }
                }
                registry.insert(url.to_owned(), value);
            }
            object.insert(key, Value::Object(registry));
        }
    }
    Ok(())
}

pub fn render(components: &Value) -> Result<String> {
    let mut out =
        String::from("<ModSync xmlns:json=\"http://james.newtonking.com/projects/json\">");
    for component in components.as_array().context("Expected component array")? {
        let mut component = component.clone();
        rewrite_resources(&mut component);
        write_value("ModComponents", &component, &mut out, false)?;
    }
    out.push_str("</ModSync>");
    Ok(out)
}
fn rewrite_resources(component: &mut Value) {
    for field in ["ResourceRegistry", "ModLinkFilenames"] {
        if let Some(Value::Object(entries)) = component.get_mut(field) {
            let rewritten: Vec<_> = entries
                .iter()
                .map(|(url, metadata)| {
                    let mut entry = metadata
                        .as_object()
                        .cloned()
                        .unwrap_or_else(|| Map::from_iter([("Value".into(), metadata.clone())]));
                    entry.insert("Url".into(), Value::String(url.clone()));
                    Value::Object(entry)
                })
                .collect();
            component[field] = Value::Array(rewritten);
        }
    }
    if let Some(Value::Array(options)) = component.get_mut("Options") {
        for option in options {
            rewrite_resources(option);
        }
    }
}
fn write_value(name: &str, value: &Value, out: &mut String, singleton: bool) -> Result<()> {
    if let Value::Array(values) = value {
        for value in values {
            write_value(name, value, out, values.len() == 1)?;
        }
        return Ok(());
    }
    let name = encode_name(name);
    out.push('<');
    out.push_str(&name);
    if singleton {
        out.push_str(" json:Array=\"true\"");
    }
    out.push('>');
    match value {
        Value::Object(fields) => {
            // The C# legacy option reader uses Guid as the boundary between records.
            for key in ["Guid", "Name"] {
                if let Some(value) = fields.get(key) {
                    write_value(key, value, out, false)?;
                }
            }
            for (key, value) in fields {
                if key != "Guid" && key != "Name" {
                    write_value(key, value, out, false)?;
                }
            }
        }
        Value::String(text) => {
            ensure!(
                text.chars().all(|c| matches!(c, '\t' | '\n' | '\r')
                    || c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}'),
                "Invalid XML character"
            );
            out.push_str(&quick_xml::escape::escape(text).replace('\r', "&#xD;"));
        }
        Value::Null => {}
        value => out.push_str(&value.to_string()),
    }
    out.push_str("</");
    out.push_str(&name);
    out.push('>');
    Ok(())
}
fn encode_name(name: &str) -> String {
    let mut result = String::new();
    for (i, character) in name.char_indices() {
        if (character.is_ascii_alphabetic()
            || character == '_'
            || i > 0 && (character.is_ascii_digit() || matches!(character, '-' | '.')))
            && !(character == '_' && name[i..].starts_with("_x"))
        {
            result.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                result.push_str(&format!("_x{unit:04X}_"));
            }
        }
    }
    result
}
fn decode_name(name: &str) -> Result<String> {
    let mut units = Vec::new();
    let mut rest = name;
    while !rest.is_empty() {
        if rest.starts_with("_x")
            && rest.as_bytes().get(6) == Some(&b'_')
            && let Some(hex) = rest.get(2..6)
            && let Ok(unit) = u16::from_str_radix(hex, 16)
        {
            units.push(unit);
            rest = &rest[7..];
        } else {
            let character = rest.chars().next().unwrap();
            units.extend_from_slice(character.encode_utf16(&mut [0; 2]));
            rest = &rest[character.len_utf8()..];
        }
    }
    String::from_utf16(&units).context("Invalid XML name encoding")
}
