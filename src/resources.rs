//! Legacy resource migration shared by every structured guide loader.
use serde_json::{Map, Value, json};
use sha1::{Digest, Sha1};

// Mirrors UrlNormalizer for content identity; the original URL remains the key.
fn normalized_url(input: &str) -> String {
    let Ok(url) = reqwest::Url::parse(
        &input
            .trim()
            .replace('\r', "%0D")
            .replace('\n', "%0A")
            .replace('\t', "%09"),
    ) else {
        return input.to_owned();
    };
    let mut path = String::new();
    let bytes = url.path().as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
            && (byte.is_ascii_alphanumeric() || b"-._~".contains(&byte))
        {
            path.push(byte as char);
            i += 3;
            continue;
        }
        // URL paths are percent-encoded ASCII.
        path.push(bytes[i] as char);
        i += 1;
    }
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => (),
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    let path = if path.is_empty() || path == "/" {
        String::new()
    } else {
        format!("/{}", segments.join("/"))
    };
    let mut result = format!(
        "{}://{}",
        url.scheme(),
        url.host_str().unwrap_or("").to_lowercase()
    );
    if let Some(port) = url.port().or_else(|| match url.scheme() {
        "ftp" => Some(21),
        _ => None,
    }) {
        result.push_str(&format!(":{port}"));
    }
    result.push_str(&path);
    if let Some(query) = url.query() {
        let mut parts: Vec<_> = query.split('&').filter(|p| !p.is_empty()).collect();
        if parts.len() > 1 {
            // .NET ordinal comparison uses UTF-16 code units.
            parts.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            result.push_str(&format!("?{}", parts.join("&")));
        } else {
            result.push('?');
            result.push_str(query);
        }
    }
    result
}

fn insert(registry: &mut Map<String, Value>, url: &str, files: Map<String, Value>) {
    if url.trim().is_empty() || registry.keys().any(|key| key.eq_ignore_ascii_case(url)) {
        return;
    }
    let hash = format!("{:x}", Sha1::digest(normalized_url(url).as_bytes()));
    registry.insert(
        url.into(),
        json!({"ContentKey": hash, "MetadataHash": hash,
        "Files": files, "HandlerMetadata": {}, "FileSize": 0}),
    );
}

pub(crate) fn migrate(component: &mut Map<String, Value>) {
    let links = component.remove("ModLink");
    let filenames = component
        .remove("ModLinkFilenames")
        .or_else(|| component.remove("modLinkFilenames"));
    if links.is_none() && filenames.is_none() {
        return;
    }
    let mut registry = component
        .remove("ResourceRegistry")
        .and_then(|r| r.as_object().cloned())
        .unwrap_or_default();
    let links: Vec<String> = match links {
        Some(Value::String(text)) => text
            .split([';', ','])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        Some(Value::Array(items)) => items
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    for url in links {
        insert(&mut registry, &url, Map::new());
    }
    if let Some(Value::Object(urls)) = filenames {
        for (url, value) in urls {
            let mut files = Map::new();
            if let Value::Object(entries) = value {
                for (filename, flag) in entries {
                    let flag = match flag {
                        Value::Bool(_) => flag,
                        Value::String(s) if s.trim().eq_ignore_ascii_case("true") => {
                            Value::Bool(true)
                        }
                        Value::String(s) if s.trim().eq_ignore_ascii_case("false") => {
                            Value::Bool(false)
                        }
                        _ => Value::Null,
                    };
                    let key = files
                        .keys()
                        .find(|k: &&String| k.eq_ignore_ascii_case(&filename))
                        .cloned()
                        .unwrap_or(filename);
                    files.insert(key, flag);
                }
            }
            insert(&mut registry, &url, files);
        }
    }
    component.insert("ResourceRegistry".into(), Value::Object(registry));
}

// The reference JSON dictionary converter capitalizes non-URL keys, including
// legacy filenames. Keep this format-specific behavior out of TOML/YAML/XML.
pub(crate) fn prepare_json(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(prepare_json),
        Value::Object(object) => {
            for field in ["ModLinkFilenames", "modLinkFilenames"] {
                if let Some(Value::Object(urls)) = object.get_mut(field) {
                    for files in urls.values_mut() {
                        if let Value::Object(entries) = files {
                            let old = std::mem::take(entries);
                            for (key, value) in old {
                                let mut chars = key.chars();
                                let key = chars
                                    .next()
                                    .map(|first| {
                                        format!("{}{}", first.to_uppercase(), chars.as_str())
                                    })
                                    .unwrap_or_default();
                                entries.insert(key, value);
                            }
                        }
                    }
                }
            }
            if let Some(options) = object.get_mut("Options") {
                prepare_json(options);
            }
            for field in ["thisMod", "Components"] {
                if let Some(items) = object.get_mut(field) {
                    prepare_json(items);
                }
            }
        }
        _ => (),
    }
}
