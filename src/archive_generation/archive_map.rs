//! Operator-supplied guide link to local archive filename map for `convert --archive-map`.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

const MAX_MAP_BYTES: u64 = 16 * 1024 * 1024;

/// Flat `{ "<guide link>": "<filename>" | ["<filename>", ...] }` document. Keys are
/// stored in canonical form so a guide link carrying a query or fragment still matches.
#[derive(Debug, Clone, Default)]
pub struct ArchiveMap {
    entries: BTreeMap<String, Vec<String>>,
}

impl ArchiveMap {
    pub fn load(path: &Path) -> Result<Self> {
        let display = path.display();
        let metadata =
            fs::metadata(path).with_context(|| format!("Cannot read archive map '{display}'"))?;
        ensure!(
            metadata.is_file(),
            "Archive map '{display}' is not a regular file"
        );
        ensure!(
            metadata.len() <= MAX_MAP_BYTES,
            "Archive map '{display}' exceeds {MAX_MAP_BYTES} bytes"
        );
        let text = fs::read_to_string(path)
            .with_context(|| format!("Cannot read archive map '{display}'"))?;
        Self::parse(&text).with_context(|| format!("Invalid archive map '{display}'"))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(text).context("Archive map is not valid JSON")?;
        let Value::Object(object) = value else {
            bail!("Archive map must be a JSON object of link to filename(s)");
        };
        let mut entries = BTreeMap::new();
        for (link, value) in object {
            let names: Vec<String> = match value {
                Value::String(name) => vec![name],
                Value::Array(items) => items
                    .into_iter()
                    .map(|item| match item {
                        Value::String(name) => Ok(name),
                        _ => bail!("Archive map value for '{link}' must contain only strings"),
                    })
                    .collect::<Result<_>>()?,
                _ => bail!("Archive map value for '{link}' must be a string or array of strings"),
            };
            ensure!(
                !names.is_empty(),
                "Archive map value for '{link}' is an empty list"
            );
            ensure!(
                names.iter().all(|name| !name.trim().is_empty()),
                "Archive map value for '{link}' contains an empty filename"
            );
            let canonical = canonical_link(&link);
            if let Some(existing) = entries.get(&canonical) {
                ensure!(
                    *existing == names,
                    "Archive map has conflicting entries for '{canonical}'"
                );
            }
            entries.insert(canonical, names);
        }
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Filenames mapped to the canonical form of `link`, if any.
    pub fn lookup(&self, link: &str) -> Option<&[String]> {
        self.entries.get(&canonical_link(link)).map(Vec::as_slice)
    }
}

/// Canonical guide link: scheme kept, query, fragment and userinfo dropped, host
/// lowercased. Mirrors `clean_url` in the CI cache tooling.
pub fn canonical_link(url: &str) -> String {
    let url = url.trim();
    let without_fragment = url.split_once('#').map_or(url, |(head, _)| head);
    let without_query = without_fragment
        .split_once('?')
        .map_or(without_fragment, |(head, _)| head);
    let Some((scheme, rest)) = without_query.split_once(':') else {
        return without_query.to_owned();
    };
    let valid_scheme = scheme
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid_scheme {
        return without_query.to_owned();
    }
    let scheme = scheme.to_ascii_lowercase();
    let Some(authority_and_path) = rest.strip_prefix("//") else {
        return format!("{scheme}:{rest}");
    };
    let (authority, path) = authority_and_path
        .find('/')
        .map_or((authority_and_path, ""), |index| {
            authority_and_path.split_at(index)
        });
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .to_ascii_lowercase();
    format!("{scheme}://{host}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_link_drops_query_fragment_and_userinfo() {
        assert_eq!(
            canonical_link("HTTPS://User@Example.COM:8443/files/12-a/?do=download&r=1#top"),
            "https://example.com:8443/files/12-a/"
        );
        assert_eq!(canonical_link("https://example.com"), "https://example.com");
        assert_eq!(canonical_link("example-mod.zip?x=1"), "example-mod.zip");
    }
}
