//! YamlDotNet's default object-reader scalar profile over decoded YAML events.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, mem::MaybeUninit};
use unsafe_libyaml as unsafe_sys;

const BYTES: usize = 8 * 1024 * 1024;
const WORK: usize = 100_000;
const DEPTH: usize = 128;

#[derive(Default)]
pub(crate) struct Budget {
    input: usize,
    decoded: usize,
    expanded: usize,
    events: usize,
    visits: usize,
}
fn charge(counter: &mut usize, amount: usize, limit: usize) -> Result<()> {
    *counter = counter
        .checked_add(amount)
        .context("YAML budget overflow")?;
    ensure!(
        *counter <= limit,
        "YAML intake exceeds its bounded resource profile"
    );
    Ok(())
}

enum Event {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    Scalar {
        text: String,
        tag: Option<String>,
        anchor: Option<String>,
        plain: bool,
    },
    Sequence {
        tag: Option<String>,
        anchor: Option<String>,
    },
    Mapping {
        tag: Option<String>,
        anchor: Option<String>,
    },
    SequenceEnd,
    MappingEnd,
    Alias(String),
}

struct Parser<'a> {
    raw: Box<MaybeUninit<unsafe_sys::yaml_parser_t>>,
    _input: &'a str,
}
impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Result<Self> {
        let mut raw = Box::new(MaybeUninit::uninit());
        // The initialized parser must remain at this heap address until deletion.
        unsafe {
            ensure!(
                unsafe_sys::yaml_parser_initialize(raw.as_mut_ptr()).ok,
                "Initialize YAML parser"
            );
            unsafe_sys::yaml_parser_set_encoding(raw.as_mut_ptr(), unsafe_sys::YAML_UTF8_ENCODING);
            unsafe_sys::yaml_parser_set_input_string(
                raw.as_mut_ptr(),
                input.as_ptr(),
                input.len() as _,
            );
        }
        Ok(Self { raw, _input: input })
    }
    fn next(&mut self, budget: &mut Budget) -> Result<Event> {
        charge(&mut budget.events, 1, WORK)?;
        let mut raw = MaybeUninit::<unsafe_sys::yaml_event_t>::uninit();
        // libyaml owns event buffers; EventOwner releases them on every conversion exit.
        let raw = unsafe {
            ensure!(
                unsafe_sys::yaml_parser_parse(self.raw.as_mut_ptr(), raw.as_mut_ptr()).ok,
                "Invalid YAML syntax"
            );
            raw.assume_init()
        };
        let owner = EventOwner(raw);
        let raw = &owner.0;
        unsafe {
            Ok(match raw.type_ {
                unsafe_sys::YAML_STREAM_START_EVENT => Event::StreamStart,
                unsafe_sys::YAML_STREAM_END_EVENT => Event::StreamEnd,
                unsafe_sys::YAML_DOCUMENT_START_EVENT => Event::DocumentStart,
                unsafe_sys::YAML_DOCUMENT_END_EVENT => Event::DocumentEnd,
                unsafe_sys::YAML_SEQUENCE_END_EVENT => Event::SequenceEnd,
                unsafe_sys::YAML_MAPPING_END_EVENT => Event::MappingEnd,
                unsafe_sys::YAML_ALIAS_EVENT => Event::Alias(
                    c_string(raw.data.alias.anchor, budget)?.context("Missing YAML alias")?,
                ),
                unsafe_sys::YAML_SCALAR_EVENT => {
                    let scalar = raw.data.scalar;
                    let length = usize::try_from(scalar.length).context("YAML scalar length")?;
                    charge(&mut budget.decoded, length, BYTES)?;
                    ensure!(!scalar.value.is_null(), "Missing YAML scalar buffer");
                    let text =
                        std::str::from_utf8(std::slice::from_raw_parts(scalar.value, length))?
                            .to_owned();
                    Event::Scalar {
                        text,
                        tag: c_string(scalar.tag, budget)?,
                        anchor: c_string(scalar.anchor, budget)?,
                        plain: scalar.style == unsafe_sys::YAML_PLAIN_SCALAR_STYLE,
                    }
                }
                unsafe_sys::YAML_SEQUENCE_START_EVENT => {
                    let node = raw.data.sequence_start;
                    Event::Sequence {
                        tag: c_string(node.tag, budget)?,
                        anchor: c_string(node.anchor, budget)?,
                    }
                }
                unsafe_sys::YAML_MAPPING_START_EVENT => {
                    let node = raw.data.mapping_start;
                    Event::Mapping {
                        tag: c_string(node.tag, budget)?,
                        anchor: c_string(node.anchor, budget)?,
                    }
                }
                _ => bail!("Unsupported YAML parser event"),
            })
        }
    }
}
impl Drop for Parser<'_> {
    fn drop(&mut self) {
        unsafe { unsafe_sys::yaml_parser_delete(self.raw.as_mut_ptr()) }
    }
}
struct EventOwner(unsafe_sys::yaml_event_t);
impl Drop for EventOwner {
    fn drop(&mut self) {
        unsafe { unsafe_sys::yaml_event_delete(&mut self.0) }
    }
}
// The library guarantees NUL termination; stop at the explicit bound before copying.
unsafe fn c_string(pointer: *const u8, budget: &mut Budget) -> Result<Option<String>> {
    if pointer.is_null() {
        return Ok(None);
    }
    let mut length = 0;
    while unsafe { *pointer.add(length) } != 0 {
        ensure!(length < 4096, "YAML tag or anchor exceeds intake bound");
        length += 1;
    }
    charge(&mut budget.decoded, length, BYTES)?;
    Ok(Some(
        std::str::from_utf8(unsafe { std::slice::from_raw_parts(pointer, length) })?.to_owned(),
    ))
}

enum Node {
    Pending,
    Scalar(Value),
    Sequence(Vec<usize>),
    Mapping(Vec<(usize, usize)>),
}
struct Document {
    nodes: Vec<Node>,
    anchors: BTreeMap<String, usize>,
}
impl Document {
    fn read(
        &mut self,
        event: Event,
        key: bool,
        depth: usize,
        parser: &mut Parser<'_>,
        budget: &mut Budget,
    ) -> Result<usize> {
        ensure!(depth <= DEPTH, "YAML nesting exceeds intake bound");
        if let Event::Alias(name) = &event {
            return self
                .anchors
                .get(name)
                .copied()
                .context("YAML alias precedes its anchor declaration");
        }
        let (tag, anchor) = match &event {
            Event::Scalar { tag, anchor, .. }
            | Event::Sequence { tag, anchor }
            | Event::Mapping { tag, anchor } => (tag.as_deref(), anchor.clone()),
            _ => bail!("Expected YAML node"),
        };
        ensure!(self.nodes.len() < WORK, "Too many YAML nodes");
        let id = self.nodes.len();
        self.nodes.push(Node::Pending);
        // Existing completed anchors stay authoritative until the replacement completes.
        if let Some(anchor) = &anchor {
            self.anchors.entry(anchor.clone()).or_insert(id);
        }
        let null = tag == Some("tag:yaml.org,2002:null");
        let node = match event {
            Event::Scalar {
                text, tag, plain, ..
            } => Node::Scalar(scalar(&text, tag.as_deref(), plain, key)?),
            Event::Sequence { tag, .. } => {
                if null {
                    skip_container(parser, budget, depth)?;
                    Node::Scalar(Value::Null)
                } else {
                    ensure!(
                        tag.as_deref().is_none_or(|tag| tag == "!"),
                        "Unsupported YAML sequence tag"
                    );
                    let mut items = Vec::new();
                    loop {
                        let event = parser.next(budget)?;
                        if matches!(event, Event::SequenceEnd) {
                            break;
                        }
                        items.push(self.read(event, false, depth + 1, parser, budget)?);
                    }
                    Node::Sequence(items)
                }
            }
            Event::Mapping { tag, .. } => {
                if null {
                    skip_container(parser, budget, depth)?;
                    Node::Scalar(Value::Null)
                } else {
                    ensure!(
                        tag.as_deref()
                            .is_none_or(|tag| matches!(tag, "!" | "tag:yaml.org,2002:map")),
                        "Unsupported YAML mapping tag"
                    );
                    let mut entries = Vec::new();
                    loop {
                        let event = parser.next(budget)?;
                        if matches!(event, Event::MappingEnd) {
                            break;
                        }
                        let key = self.read(event, true, depth + 1, parser, budget)?;
                        let event = parser.next(budget)?;
                        let value = self.read(event, false, depth + 1, parser, budget)?;
                        entries.push((key, value));
                    }
                    Node::Mapping(entries)
                }
            }
            _ => bail!("Expected YAML value"),
        };
        self.nodes[id] = node;
        if let Some(anchor) = anchor {
            self.anchors.insert(anchor, id);
        }
        Ok(id)
    }
    fn project(
        &self,
        id: usize,
        depth: usize,
        active: &mut [bool],
        budget: &mut Budget,
    ) -> Result<Value> {
        ensure!(
            depth <= DEPTH && !active[id],
            "Cyclic or excessively nested YAML aliases are unsupported"
        );
        charge(&mut budget.visits, 1, WORK)?;
        active[id] = true;
        let value = match &self.nodes[id] {
            Node::Pending => bail!("Unresolved YAML anchor"),
            Node::Scalar(value) => {
                charge(
                    &mut budget.expanded,
                    value.as_str().map_or(32, str::len),
                    BYTES,
                )?;
                value.clone()
            }
            Node::Sequence(items) => Value::Array(
                items
                    .iter()
                    .map(|id| self.project(*id, depth + 1, active, budget))
                    .collect::<Result<_>>()?,
            ),
            Node::Mapping(entries) => {
                let mut object = Map::new();
                for (key, value) in entries {
                    let key = self.project(*key, depth + 1, active, budget)?;
                    let key = key.as_str().context("YAML mapping keys must resolve to strings; null or complex keys are unsupported")?;
                    let value = self.project(*value, depth + 1, active, budget)?;
                    object.insert(key.to_owned(), value);
                }
                Value::Object(object)
            }
        };
        active[id] = false;
        Ok(value)
    }
}
fn skip_container(parser: &mut Parser<'_>, budget: &mut Budget, depth: usize) -> Result<()> {
    let mut open = 1;
    while open != 0 {
        match parser.next(budget)? {
            Event::Sequence { .. } | Event::Mapping { .. } => {
                open += 1;
                ensure!(depth + open <= DEPTH, "YAML nesting exceeds intake bound");
            }
            Event::SequenceEnd | Event::MappingEnd => open -= 1,
            Event::Scalar { .. } | Event::Alias(_) => {}
            _ => bail!("Unexpected end of null-tagged YAML container"),
        }
    }
    Ok(())
}

pub(crate) fn documents(text: &str, budget: &mut Budget) -> Result<Vec<Value>> {
    charge(&mut budget.input, text.len(), BYTES)?;
    let mut parser = Parser::new(text)?;
    ensure!(
        matches!(parser.next(budget)?, Event::StreamStart),
        "Missing YAML stream start"
    );
    let mut documents = Vec::new();
    loop {
        match parser.next(budget)? {
            Event::StreamEnd => break,
            Event::DocumentStart => {
                let mut document = Document {
                    nodes: Vec::new(),
                    anchors: BTreeMap::new(),
                };
                let event = parser.next(budget)?;
                let root = document.read(event, false, 0, &mut parser, budget)?;
                ensure!(
                    matches!(parser.next(budget)?, Event::DocumentEnd),
                    "Missing YAML document end"
                );
                documents.push(document.project(
                    root,
                    0,
                    &mut vec![false; document.nodes.len()],
                    budget,
                )?);
            }
            _ => bail!("Expected YAML document"),
        }
    }
    Ok(documents)
}
pub(crate) fn one(text: &str, budget: &mut Budget) -> Result<Value> {
    let mut docs = documents(text, budget)?;
    ensure!(
        docs.len() <= 1,
        "Metadata must contain at most one YAML document"
    );
    Ok(docs.pop().unwrap_or(Value::Null))
}

fn scalar(text: &str, tag: Option<&str>, plain: bool, key: bool) -> Result<Value> {
    ensure!(
        tag.is_none_or(|tag| matches!(
            tag,
            "!" | "tag:yaml.org,2002:str"
                | "tag:yaml.org,2002:null"
                | "tag:yaml.org,2002:bool"
                | "tag:yaml.org,2002:int"
                | "tag:yaml.org,2002:float"
        )),
        "Unknown or timestamp YAML tags require a qualified wire contract"
    );
    if tag == Some("tag:yaml.org,2002:null")
        || plain && !key && matches!(text, "" | "~" | "null" | "Null" | "NULL")
    {
        return Ok(Value::Null);
    }
    let value = typed(text, tag)?;
    if !key {
        return Ok(value);
    }
    // The reference converts valid Boolean/Int32/Double keys and keys its
    // dictionary by the converted value, which no schema property can match.
    // Keep the converted key as text so the field stays preserved metadata.
    Ok(match value {
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(number) => number.to_string().into(),
        value => value,
    })
}

fn typed(text: &str, tag: Option<&str>) -> Result<Value> {
    match tag {
        None | Some("!" | "tag:yaml.org,2002:str") => Ok(text.into()),
        Some("tag:yaml.org,2002:bool") => {
            // .NET Regex '$' permits one final LF, not arbitrary whitespace.
            let text = text.strip_suffix('\n').unwrap_or(text);
            if ["true", "y", "yes", "on"]
                .iter()
                .any(|v| text.eq_ignore_ascii_case(v))
            {
                Ok(true.into())
            } else if ["false", "n", "no", "off"]
                .iter()
                .any(|v| text.eq_ignore_ascii_case(v))
            {
                Ok(false.into())
            } else {
                bail!("Invalid explicitly tagged YAML Boolean")
            }
        }
        Some("tag:yaml.org,2002:int") => Ok(integer(text)?.into()),
        Some("tag:yaml.org,2002:float") => {
            crate::profiles::NumberCultureSnapshot::parse_yaml_double(text)
        }
        Some("tag:yaml.org,2002:timestamp") => {
            bail!("Explicit YAML timestamps require a qualified DateTime wire contract")
        }
        Some(_) => bail!("Unsupported YAML tag"),
    }
}

fn ascii_white(c: char) -> bool {
    matches!(c, '\t'..='\r' | ' ')
}
fn unsigned_decimal(text: &str) -> Result<u64> {
    let text = text.trim_start_matches(ascii_white);
    let text = text.trim_end_matches('\0').trim_end_matches(ascii_white);
    let (text, negative) = if let Some(text) = text.strip_prefix('-') {
        (text, true)
    } else {
        (text.strip_prefix('+').unwrap_or(text), false)
    };
    ensure!(
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()),
        "Invalid YAML integer digits"
    );
    let magnitude: u64 = text.parse().context("YAML integer magnitude overflow")?;
    ensure!(!negative || magnitude == 0, "Negative YAML unsigned chunk");
    Ok(magnitude)
}
fn integer(text: &str) -> Result<i32> {
    ensure!(
        !text.is_empty() && text.len() <= 4096,
        "Explicit YAML integer exceeds intake profile"
    );
    let (digits, negative) = if let Some(rest) = text.strip_prefix('-') {
        (rest, true)
    } else {
        (text.strip_prefix('+').unwrap_or(text), false)
    };
    ensure!(!digits.is_empty(), "Missing YAML integer digits");
    let magnitude = if let Some(rest) = digits.strip_prefix('0') {
        if rest.is_empty() {
            0
        } else {
            let (radix, rest) = if let Some(rest) = rest.strip_prefix('b') {
                (2, rest)
            } else if let Some(rest) = rest.strip_prefix('x') {
                (16, rest)
            } else {
                (8, rest)
            };
            let rest = rest.replace('_', "");
            let rest = if radix == 16 {
                rest.trim_start_matches(ascii_white)
                    .trim_end_matches('\0')
                    .trim_end_matches(ascii_white)
            } else {
                rest.as_str()
            };
            let rest = rest
                .strip_prefix('+')
                .filter(|_| radix != 16)
                .unwrap_or(rest);
            ensure!(
                !rest.is_empty()
                    && rest.bytes().all(|b| match radix {
                        2 => matches!(b, b'0'..=b'1'),
                        8 => matches!(b, b'0'..=b'7'),
                        _ => b.is_ascii_hexdigit(),
                    }),
                "Invalid YAML radix integer digits"
            );
            u64::from_str_radix(rest, radix).context("YAML integer magnitude overflow")?
        }
    } else {
        let mut result = 0_u64;
        for part in digits.split(':') {
            let part = part.replace('_', "");
            // The pinned helper does not constrain sexagesimal chunks to <60.
            result = result
                .wrapping_mul(60)
                .wrapping_add(unsigned_decimal(&part)?);
        }
        result
    };
    if negative {
        let signed = if magnitude == 1_u64 << 63 {
            i64::MIN
        } else {
            -i64::try_from(magnitude).context("YAML signed integer overflow")?
        };
        Ok(i32::try_from(signed).context("Explicit YAML integer must fit Int32")?)
    } else {
        Ok(i32::try_from(magnitude).context("Explicit YAML integer must fit Int32")?)
    }
}
