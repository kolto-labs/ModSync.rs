//! Bounded represented-value subset of the reference scalar/list string getters.
use super::keys::Format;
use anyhow::{Context as _, Result, bail, ensure};
use serde_json::Value;

const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_FIELD_ITEMS: usize = 4096;
const MAX_ITEMS: usize = 100_000;

#[derive(Default)]
pub(super) struct Context {
    culture: Option<crate::profiles::NumberCultureSnapshot>,
    bytes: usize,
    items: usize,
}

// .NET 9 Char.IsWhiteSpace, without Boolean.Parse's additional NUL trimming.
fn whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}'
        | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}'
        | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}
fn blank(text: &str) -> bool {
    text.chars().all(whitespace)
}
impl Context {
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("Text intake size overflow"))?;
        ensure!(
            self.bytes <= MAX_OUTPUT,
            "Generated guide text exceeds 8 MiB"
        );
        Ok(())
    }
    fn visit(&mut self, field_items: &mut usize) -> Result<()> {
        *field_items += 1;
        self.items += 1;
        ensure!(
            *field_items <= MAX_FIELD_ITEMS && self.items <= MAX_ITEMS,
            "Text collection exceeds intake item bound"
        );
        Ok(())
    }
    fn signed(&mut self, magnitude: u64, negative: bool, collection: bool) -> Result<String> {
        if !negative {
            return Ok(magnitude.to_string());
        }
        if !collection {
            return Ok(format!("-{magnitude}"));
        }
        if self.culture.is_none() {
            self.culture = Some(crate::profiles::NumberCultureSnapshot::current()?);
        }
        Ok(self
            .culture
            .as_ref()
            .context("Missing captured number culture")?
            .format_signed_magnitude(magnitude, true))
    }
    /// Exact decimal digits for an integer-shaped lexeme beyond Int64,
    /// using the lazily captured NegativeSign (collections only).
    fn signed_digits(&mut self, lexeme: &str) -> Result<String> {
        let (negative, digits) = match lexeme.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, lexeme),
        };
        if !negative {
            return crate::profiles::NumberCultureSnapshot::invariant()
                .format_signed_digits(digits, false);
        }
        if self.culture.is_none() {
            self.culture = Some(crate::profiles::NumberCultureSnapshot::current()?);
        }
        self.culture
            .as_ref()
            .context("Missing captured number culture")?
            .format_signed_digits(digits, true)
    }
    fn primitive(&mut self, value: &Value, format: Format, collection: bool) -> Result<String> {
        Ok(match value {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            Value::Bool(value) => if *value { "True" } else { "False" }.into(),
            Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    self.signed(value.unsigned_abs(), value < 0, collection)?
                } else if let Some(value) = value.as_u64() {
                    if collection {
                        value.to_string()
                    } else if format == Format::Json {
                        String::new()
                    } else {
                        bail!("Wide unsigned scalar text requires native-type evidence");
                    }
                } else if let Some(lexeme) = crate::json_native::wide_integer_lexeme(value) {
                    // Beyond Int64 and integer-shaped: collections format
                    // the exact decimal digits using the lazily captured
                    // NegativeSign; scalar text has no native-type evidence.
                    if collection {
                        self.signed_digits(lexeme)?
                    } else if format == Format::Json {
                        String::new()
                    } else {
                        bail!("Wide integer scalar text requires native-type evidence");
                    }
                } else {
                    let value = value
                        .as_f64()
                        .ok_or_else(|| anyhow::anyhow!("Unsupported numeric text"))?;
                    ensure!(
                        value.is_finite()
                            && value.fract() == 0.0
                            && value >= i32::MIN as f64
                            && value <= i32::MAX as f64,
                        "Floating text outside the proven integral-i32 formatting profile"
                    );
                    self.signed(value.abs() as u64, value.is_sign_negative(), collection)?
                }
            }
            Value::Array(_) | Value::Object(_) => {
                bail!("Complex collection text requires native-type formatting")
            }
        })
    }
    fn collect(
        &mut self,
        value: &Value,
        depth: usize,
        format: Format,
        visited: &mut usize,
        output: &mut Vec<String>,
    ) -> Result<()> {
        self.visit(visited)?;
        if depth != 0
            && let Value::Array(items) = value
        {
            for item in items {
                self.collect(item, depth - 1, format, visited, output)?;
            }
        } else {
            // Bound before cloning an authored collection string.
            if let Value::String(text) = value {
                self.charge(text.len())?;
            }
            let text = self.primitive(value, format, true)?;
            if !matches!(value, Value::String(_)) {
                self.charge(text.len())?;
            }
            if !blank(&text) {
                output.push(text);
            }
        }
        Ok(())
    }
    pub(super) fn scalar(&mut self, value: &Value, format: Format) -> Result<String> {
        if let Value::String(text) = value {
            return Ok(text.clone());
        }
        if let Value::Array(items) = value {
            let mut output = Vec::new();
            let mut visited = 0;
            for item in items {
                self.collect(item, 1, format, &mut visited, &mut output)?;
            }
            self.charge(output.len().saturating_sub(1) * 2)?;
            Ok(output.join(", "))
        } else {
            let text = self.primitive(value, format, false)?;
            self.charge(text.len())?;
            Ok(text)
        }
    }
    fn split(&mut self, text: &str) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        let mut visited = 0;
        for part in text.split([',', ';']) {
            self.visit(&mut visited)?;
            let part = part.trim_matches(whitespace);
            if !part.is_empty() {
                self.charge(part.len())?;
                result.push(Value::String(part.into()));
            }
        }
        Ok(result)
    }
    pub(super) fn list(&mut self, value: &Value, format: Format) -> Result<Vec<Value>> {
        if let Value::Array(items) = value {
            let mut output = Vec::new();
            let mut visited = 0;
            // Getter consolidation, then List<string>'s nested enumerable branch.
            for item in items {
                self.collect(item, 2, format, &mut visited, &mut output)?;
            }
            if output.len() == 1 && output[0].contains([',', ';']) {
                return self.split(&output[0]);
            }
            Ok(output.into_iter().map(Value::String).collect())
        } else {
            let text = self.scalar(value, format)?;
            self.split(&text)
        }
    }
}
