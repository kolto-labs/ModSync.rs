//! Tokenize before substituting roots so spaces in folders cannot create arguments.
//!
//! Arguments use the Windows command-line rules on every host, as the reference
//! does: .NET splits `ProcessStartInfo.Arguments` with those rules on Linux and
//! macOS too, so a guide's Arguments mean the same thing everywhere.
use crate::paths::Roots;
use anyhow::{Context, Result, bail, ensure};
use std::ffi::OsString;

pub(super) fn expand(text: &str, roots: &Roots) -> Result<Vec<OsString>> {
    windows_split(text)?
        .into_iter()
        .map(|token| {
            ensure!(!token.contains('\0'), "Arguments contain a NUL character");
            let mut result = OsString::new();
            let mut remaining = token.as_str();
            while let Some(start) = remaining.find("<<") {
                ensure!(
                    !remaining[..start].contains(">>"),
                    "Unexpected variable terminator in Arguments"
                );
                result.push(&remaining[..start]);
                let variable = &remaining[start + 2..];
                let end = variable
                    .find(">>")
                    .context("Unclosed variable in Arguments")?;
                let name = &variable[..end];
                let root = if name.eq_ignore_ascii_case("modDirectory") {
                    &roots.mods
                } else if name.eq_ignore_ascii_case("kotorDirectory")
                    || name.eq_ignore_ascii_case("gameDirectory")
                {
                    &roots.game
                } else {
                    bail!("Unknown Arguments variable <<{name}>>");
                };
                result.push(root.as_os_str());
                remaining = &variable[end + 2..];
            }
            ensure!(
                !remaining.contains(">>"),
                "Unexpected variable terminator in Arguments"
            );
            result.push(remaining);
            Ok(result)
        })
        .collect()
}

/// Windows CRT-style argument quoting. Only space and tab separate arguments;
/// single quotes, `#` and newlines are literal; backslashes only escape quotes
/// and remain unchanged in ordinary Windows paths. An unclosed quote is refused.
pub(crate) fn windows_split(text: &str) -> Result<Vec<String>> {
    let mut chars = text.chars().peekable();
    let mut result = Vec::new();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| matches!(c, ' ' | '\t')) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut token = String::new();
        let mut quoted = false;
        while let Some(&c) = chars.peek() {
            if !quoted && matches!(c, ' ' | '\t') {
                break;
            }
            if c == '\\' {
                let mut count = 0;
                while chars.peek() == Some(&'\\') {
                    count += 1;
                    chars.next();
                }
                if chars.peek() == Some(&'"') {
                    token.extend(std::iter::repeat_n('\\', count / 2));
                    if count % 2 == 1 {
                        token.push('"');
                        chars.next();
                    }
                    // An even run leaves the quote to the ordinary quote branch.
                } else {
                    token.extend(std::iter::repeat_n('\\', count));
                }
            } else if c == '"' {
                chars.next();
                if quoted && chars.peek() == Some(&'"') {
                    token.push('"');
                    chars.next();
                } else {
                    quoted = !quoted;
                }
            } else {
                token.push(c);
                chars.next();
            }
        }
        ensure!(!quoted, "Unclosed quote in Arguments");
        result.push(token);
    }
    Ok(result)
}
