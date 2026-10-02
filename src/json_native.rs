//! Native (non-JSON) serialization of `serde_json::Value` for TOML and YAML
//! output, preserving integer/decimal lexeme origin without leaking the
//! `arbitrary_precision` feature's private `Number` wrapper protocol into
//! those formats.
//!
//! See `docs/parity/json-number-origin-contract.md` for the full contract.

use anyhow::{Result, bail, ensure};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde_json::{Number, Value};

/// Shared preflight bounds for one save (all documents / Markdown blocks).
#[derive(Default)]
pub(crate) struct ValueBudget {
    bytes: usize,
    nodes: usize,
}

const MAX_LOGICAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 128;

impl ValueBudget {
    /// Walk `value`, charging its logical size (string/key/lexeme bytes) and
    /// node count against the shared budget, and rejecting depth beyond
    /// `MAX_DEPTH`. This is a preflight rejection *before* any serializer
    /// allocates output; it is independent of, and does not replace, the
    /// existing encoded-output acceptance check elsewhere.
    pub(crate) fn check(&mut self, value: &Value) -> Result<()> {
        self.walk(value, 0)
    }

    fn charge_bytes(&mut self, n: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(n)
            .ok_or_else(|| anyhow::anyhow!("Guide document logical size overflow"))?;
        ensure!(
            self.bytes <= MAX_LOGICAL_BYTES,
            "Guide document logical size exceeds 8 MiB across this save"
        );
        Ok(())
    }

    fn charge_node(&mut self) -> Result<()> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Guide document node count overflow"))?;
        ensure!(
            self.nodes <= MAX_NODES,
            "Guide document exceeds 100,000 nodes across this save"
        );
        Ok(())
    }

    fn walk(&mut self, value: &Value, depth: usize) -> Result<()> {
        ensure!(
            depth <= MAX_DEPTH,
            "Guide document exceeds the maximum nesting depth of 128"
        );
        self.charge_node()?;
        match value {
            Value::Null | Value::Bool(_) => {}
            Value::Number(n) => self.charge_bytes(n.as_str().len())?,
            Value::String(s) => self.charge_bytes(s.len())?,
            Value::Array(items) => {
                for item in items {
                    self.walk(item, depth + 1)?;
                }
            }
            Value::Object(map) => {
                for (key, item) in map {
                    self.charge_bytes(key.len())?;
                    self.walk(item, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}

/// Classify a JSON number lexeme and return the integer lexeme text when the
/// number is integer-shaped (no `.`, `e`, or `E`) but exceeds `i64`/`u64`
/// range. Callers that need to inspect wide integers before falling back to
/// `as_f64` should check this first.
pub(crate) fn wide_integer_lexeme(n: &Number) -> Option<&str> {
    let lexeme = n.as_str();
    let is_integer_shaped = !lexeme.contains(['.', 'e', 'E']);
    if is_integer_shaped && n.as_i64().is_none() && n.as_u64().is_none() {
        Some(lexeme)
    } else {
        None
    }
}

/// Wraps a `&Value` so that `Serialize` renders scalars/maps/sequences
/// natively instead of delegating `Number` to `serde_json::Number::serialize`
/// (which emits the private `arbitrary_precision` wrapper protocol under
/// non-JSON formats).
struct NativeValue<'a>(&'a Value);

impl Serialize for NativeValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Number(n) => serialize_number(n, serializer),
            Value::String(s) => serializer.serialize_str(s),
            Value::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(&NativeValue(item))?;
                }
                seq.end()
            }
            Value::Object(map) => {
                let mut ser_map = serializer.serialize_map(Some(map.len()))?;
                for (key, item) in map {
                    ser_map.serialize_entry(key, &NativeValue(item))?;
                }
                ser_map.end()
            }
        }
    }
}

fn serialize_number<S: Serializer>(n: &Number, serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::Error as _;
    let lexeme = n.as_str();
    if !lexeme.contains(['.', 'e', 'E']) {
        // Integer-shaped lexeme.
        if let Ok(v) = lexeme.parse::<i64>() {
            return serializer.serialize_i64(v);
        }
        if let Ok(v) = lexeme.parse::<u64>() {
            return serializer.serialize_u64(v);
        }
        return Err(S::Error::custom(format!(
            "Integer literal `{lexeme}` exceeds the i64/u64 range and cannot be represented natively"
        )));
    }
    // Decimal/exponent-shaped lexeme.
    let v: f64 = lexeme
        .parse()
        .map_err(|_| S::Error::custom(format!("Invalid numeric literal `{lexeme}`")))?;
    if !v.is_finite() {
        return Err(S::Error::custom(format!(
            "Numeric literal `{lexeme}` overflows to a non-finite value and cannot be represented natively"
        )));
    }
    serializer.serialize_f64(v)
}

/// Serialize `value` to pretty-printed TOML, natively (see module docs).
/// Charges `budget` with `value`'s logical size before serializing.
pub(crate) fn to_toml_pretty(value: &Value, budget: &mut ValueBudget) -> Result<String> {
    budget.check(value)?;
    match toml::to_string_pretty(&NativeValue(value)) {
        Ok(text) => Ok(text),
        Err(err) => bail!("{err}"),
    }
}

/// Serialize `value` to YAML, natively (see module docs).
/// Charges `budget` with `value`'s logical size before serializing.
pub(crate) fn to_yaml_string(value: &Value, budget: &mut ValueBudget) -> Result<String> {
    budget.check(value)?;
    match serde_yaml_ng::to_string(&NativeValue(value)) {
        Ok(text) => Ok(text),
        Err(err) => bail!("{err}"),
    }
}
