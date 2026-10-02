//! Culture data comes from native providers; numeric syntax follows .NET's
//! Integer then Float|AllowThousands readers, not ICU's more permissive parser.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod icu;
#[cfg(windows)]
mod windows;

#[derive(Clone, Debug)]
pub struct NumberCultureSnapshot {
    pub locale: String,
    pub provider: &'static str,
    decimal: String,
    group: String,
    positive: String,
    negative: String,
    infinity: String,
    negative_infinity: String,
    nan: String,
    negative_space: bool,
}
impl NumberCultureSnapshot {
    pub fn invariant() -> Self {
        Self {
            locale: String::new(),
            provider: "invariant",
            decimal: ".".into(),
            group: ",".into(),
            positive: "+".into(),
            negative: "-".into(),
            infinity: "Infinity".into(),
            negative_infinity: "-Infinity".into(),
            nan: "NaN".into(),
            negative_space: false,
        }
    }
    pub fn current() -> Result<Self> {
        if std::env::var("DOTNET_SYSTEM_GLOBALIZATION_INVARIANT")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        {
            return Ok(Self::invariant());
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            icu::current()
        }
        #[cfg(windows)]
        {
            windows::current()
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            anyhow::bail!("Current-culture numeric data is unavailable on this platform")
        }
    }
    fn validate(self) -> Result<Self> {
        for symbol in [
            &self.decimal,
            &self.group,
            &self.positive,
            &self.negative,
            &self.infinity,
            &self.negative_infinity,
            &self.nan,
        ] {
            ensure!(
                symbol.len() <= 512 && !symbol.contains('\0'),
                "Invalid native number symbol"
            );
        }
        ensure!(
            !self.decimal.is_empty() && !self.positive.is_empty() && !self.negative.is_empty(),
            "Native numeric data lacks required symbols"
        );
        Ok(self)
    }
    fn hyphen(&self) -> bool {
        matches!(
            self.negative.as_str(),
            "\u{2012}"
                | "\u{207b}"
                | "\u{208b}"
                | "\u{2212}"
                | "\u{2796}"
                | "\u{fe63}"
                | "\u{ff0d}"
        )
    }
    fn sign<'a>(&self, input: &'a str) -> (&'a str, bool, bool) {
        if let Some(rest) = symbol(input, &self.positive) {
            (rest, false, true)
        } else if let Some(rest) = symbol(input, &self.negative)
            .or_else(|| self.hyphen().then(|| input.strip_prefix('-')).flatten())
        {
            (rest, true, true)
        } else {
            (input, false, false)
        }
    }
    fn integer_sign<'a>(&self, input: &'a str) -> (&'a str, bool) {
        // The optimized Integer reader uses exact prefix matching, unlike
        // floating MatchChars, which permits ordinary spaces for NBSP symbols.
        if self.hyphen()
            && let Some(rest) = input.strip_prefix('-')
        {
            (rest, true)
        } else if let Some(rest) = input.strip_prefix(&self.positive) {
            (rest, false)
        } else if let Some(rest) = input.strip_prefix(&self.negative) {
            (rest, true)
        } else {
            (input, false)
        }
    }
    pub fn parse(&self, raw: &str) -> Result<Value> {
        ensure!(
            raw.len() <= 16 * 1024 * 1024,
            "Setting value exceeds 16 MiB"
        );
        let text = raw.trim();
        if text.eq_ignore_ascii_case("true") {
            return Ok(true.into());
        }
        if text.eq_ignore_ascii_case("false") {
            return Ok(false.into());
        }
        if text.eq_ignore_ascii_case("null") {
            return Ok(Value::Null);
        }
        let (integer, negative) = self.integer_sign(text);
        let integer = integer.trim_end_matches('\0').trim_end_matches(ascii_white);
        if !integer.is_empty() && integer.bytes().all(|b| b.is_ascii_digit()) {
            let spelling = if negative {
                format!("-{integer}")
            } else {
                integer.into()
            };
            if let Ok(value) = spelling.parse::<i64>() {
                return Ok(value.into());
            }
        }
        if let Some(number) = self.float(text) {
            let value: f64 = number
                .parse()
                .context("Normalized numeric value could not be parsed")?;
            ensure!(
                value.is_finite(),
                "Non-finite numbers cannot be written as valid JSON"
            );
            if value.fract() == 0.0
                && value.abs() < 9_007_199_254_740_992.0
                && !(value == 0.0 && value.is_sign_negative())
            {
                return Ok((value as i64).into());
            }
            return Ok(serde_json::Number::from_f64(value)
                .context("Number is not representable")?
                .into());
        }
        let eq = |a: &str, b: &str| {
            !b.is_empty()
                && a.chars()
                    .flat_map(char::to_uppercase)
                    .eq(b.chars().flat_map(char::to_uppercase))
        };
        let nonfinite = eq(text, &self.infinity)
            || eq(text, &self.negative_infinity)
            || eq(text, &self.nan)
            || text
                .strip_prefix(&self.positive)
                .is_some_and(|v| eq(v, &self.infinity) || eq(v, &self.nan))
            || text
                .strip_prefix(&self.negative)
                .is_some_and(|v| eq(v, &self.nan));
        ensure!(
            !nonfinite,
            "Non-finite numbers cannot be written as valid JSON"
        );
        if (text.starts_with('{') && text.ends_with('}'))
            || (text.starts_with('[') && text.ends_with(']'))
        {
            return serde_json::from_str(text).context("Invalid JSON value");
        }
        Ok(text.into())
    }
    fn float(&self, text: &str) -> Option<String> {
        let (mut rest, negative, signed) = self.sign(text);
        if signed && self.negative_space {
            rest = rest.trim_start_matches(ascii_white);
        }
        let mut result = String::with_capacity(text.len());
        if negative {
            result.push('-');
        }
        let mut digits = false;
        let mut decimal = false;
        loop {
            if let Some(byte) = rest.bytes().next().filter(u8::is_ascii_digit) {
                result.push(char::from(byte));
                rest = &rest[1..];
                digits = true;
            } else if !decimal && let Some(next) = symbol(rest, &self.decimal) {
                result.push('.');
                rest = next;
                decimal = true;
            } else if digits
                && !decimal
                && let Some(next) = symbol(rest, &self.group)
            {
                rest = next;
            } else {
                break;
            }
        }
        if !digits {
            return None;
        }
        if rest.starts_with(['e', 'E']) {
            result.push('e');
            rest = &rest[1..];
            let (next, negative, _) = self.sign(rest);
            rest = next;
            if negative {
                result.push('-');
            }
            let start = rest.len();
            while let Some(byte) = rest.bytes().next().filter(u8::is_ascii_digit) {
                result.push(char::from(byte));
                rest = &rest[1..];
            }
            if start == rest.len() {
                return None;
            }
        }
        rest = rest.trim_start_matches(ascii_white);
        rest.bytes().all(|b| b == 0).then_some(result)
    }
    pub fn set(&self, root: &mut Value, key: &str, raw: Option<&str>) -> Result<()> {
        let value = raw
            .filter(|value| !value.trim().is_empty())
            .map(|raw| self.parse(raw))
            .transpose()?;
        let object = root.as_object_mut().context("Settings must be an object")?;
        match value {
            Some(value) => {
                object.insert(key.to_owned(), value);
            }
            None => {
                object.shift_remove(key);
            }
        }
        Ok(())
    }
}
fn ascii_white(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}')
}
fn symbol<'a>(input: &'a str, expected: &str) -> Option<&'a str> {
    if expected.is_empty() {
        return None;
    }
    let mut position = 0;
    for expected in expected.chars() {
        let actual = input.get(position..)?.chars().next()?;
        if actual != expected && !(actual == ' ' && matches!(expected, '\u{a0}' | '\u{202f}')) {
            return None;
        }
        position += actual.len_utf8();
    }
    input.get(position..)
}
pub fn parse_cli_value_current_culture(raw: &str) -> Result<Value> {
    NumberCultureSnapshot::current()?.parse(raw)
}
pub fn set_cli_value_current_culture(root: &mut Value, key: &str, raw: Option<&str>) -> Result<()> {
    if raw.is_none_or(|value| value.trim().is_empty()) {
        NumberCultureSnapshot::invariant().set(root, key, raw)
    } else {
        NumberCultureSnapshot::current()?.set(root, key, raw)
    }
}
