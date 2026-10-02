//! Qualified Unicode DNS Host path: original label admission, Bidi strip,
//! canonical composition, then the shared pinned invariant lowercase mapping.
use crate::unicode_case::{ASSIGNED, invariant_lower_key};

pub(super) fn canonical(authority: &str) -> Option<String> {
    if authority.bytes().filter(|&byte| byte == b'@').count() > 1 {
        return None;
    }
    let host_port = authority
        .split_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = host_port
        .find(':')
        .map_or((host_port, ""), |index| host_port.split_at(index));
    if host.is_ascii() || !super::ip_host::valid_port(port) || !admitted(host) {
        return None;
    }
    let stripped: String = host
        .chars()
        .filter(|&c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}'))
        .collect();
    let normalized = icu_normalizer::ComposingNormalizerBorrowed::new_nfc().normalize(&stripped);
    let canonical = invariant_lower_key(&normalized);
    Some(if canonical == "loopback" {
        "localhost".into()
    } else {
        canonical
    })
}

fn admitted(host: &str) -> bool {
    // The reference admits the authored UTF-16 labels before stripping Bidi or
    // normalizing. A single trailing dot (including an IRI dot) is permitted.
    let mut labels = host
        .split(['.', '\u{3002}', '\u{ff0e}', '\u{ff61}'])
        .peekable();
    while let Some(label) = labels.next() {
        if label.is_empty() {
            return labels.peek().is_none()
                && host.ends_with(['.', '\u{3002}', '\u{ff0e}', '\u{ff61}']);
        }
        let first = label.chars().next().expect("Nonempty label");
        if first < '\u{a0}' && !first.is_ascii_alphanumeric() {
            return false;
        }
        let mut weighted = if label.is_ascii() { 0 } else { 4 };
        for character in label.chars() {
            let scalar = character as u32;
            let index = ASSIGNED.partition_point(|(start, _)| *start <= scalar);
            if index == 0 || scalar > ASSIGNED[index - 1].1 {
                return false;
            }
            if character < '\u{a0}'
                && !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
            {
                return false;
            }
            for unit in character.encode_utf16(&mut [0; 2]) {
                weighted += 1 + usize::from(*unit > 0xff);
            }
            if weighted > 63 {
                return false;
            }
        }
    }
    true
}
