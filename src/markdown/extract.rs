//! Field extraction that follows the reference's default `MarkdownImportProfile`.
//!
//! The reference runs one `Regex.Match` per field over the whole component
//! section with `IgnoreCase | Multiline | Singleline` and keeps the first
//! (leftmost) match only. These matchers reproduce the observable result of
//! those patterns without a backtracking regex engine, including the quirks
//! the guides depend on: labels are case-insensitive, most free-text labels
//! accept a missing colon (so a prose word such as "screenshots" can start the
//! field), and a whitespace-only first match yields no value.

/// Leftmost match of a field pattern. `Some(None)` means the pattern matched
/// but the captured group was blank, which the reference treats as absent.
type Found = Option<Option<(usize, usize)>>;

const FIELD_TITLES: &[&str] = &[
    "Name",
    "Author",
    "Description",
    "Masters",
    "Non-English Functionality",
    "Installation Method",
    "Installation Instructions",
    "Install Instructions",
    "Download Instructions",
    "Usage Warning",
    "Screenshots",
    "Known Bugs",
    "Installation Warning",
    "Compatibility Warning",
    "Steam Notes",
];
const ADMONITIONS: &[&str] = &["note", "warning", "tip", "danger", "info"];

/// Free-text fields: fenced admonition form first, then the labelled forms.
#[derive(Clone, Copy)]
pub(super) enum Field {
    Description,
    DownloadInstructions,
    Directions,
    UsageWarning,
    Screenshots,
    KnownBugs,
    InstallationWarning,
    CompatibilityWarning,
    SteamNotes,
}

impl Field {
    pub(super) const ALL: [(Field, &'static str); 9] = [
        (Field::Description, "Description"),
        (Field::DownloadInstructions, "DownloadInstructions"),
        (Field::Directions, "Directions"),
        (Field::UsageWarning, "UsageWarning"),
        (Field::Screenshots, "Screenshots"),
        (Field::KnownBugs, "KnownBugs"),
        (Field::InstallationWarning, "InstallationWarning"),
        (Field::CompatibilityWarning, "CompatibilityWarning"),
        (Field::SteamNotes, "SteamNotes"),
    ];

    fn titles(self) -> &'static [&'static str] {
        match self {
            Field::Description => &["Description"],
            Field::DownloadInstructions => &["Download Instructions"],
            Field::Directions => &["Installation Instructions", "Install Instructions"],
            Field::UsageWarning => &["Usage Warning"],
            Field::Screenshots => &["Screenshots"],
            Field::KnownBugs => &["Known Bugs"],
            Field::InstallationWarning => &["Installation Warning"],
            Field::CompatibilityWarning => &["Compatibility Warning"],
            Field::SteamNotes => &["Steam Notes"],
        }
    }
}

fn at_ci(text: &str, at: usize, pattern: &str) -> Option<usize> {
    text.get(at..at + pattern.len())
        .filter(|s| s.eq_ignore_ascii_case(pattern))
        .map(|_| at + pattern.len())
}

fn at_cs(text: &str, at: usize, pattern: &str) -> Option<usize> {
    text[at..]
        .starts_with(pattern)
        .then_some(at + pattern.len())
}

fn whitespace_end(text: &str, at: usize) -> usize {
    text[at..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(text.len(), |(i, _)| at + i)
}

fn next_char(text: &str, at: usize) -> usize {
    at + text[at..].chars().next().map_or(1, char::len_utf8)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find(['\r', '\n']).map_or(text.len(), |i| at + i)
}

/// `\r?\n\s*` followed by `tail` at the first non-space character.
fn newline_then(text: &str, at: usize, tail: impl Fn(usize) -> bool) -> bool {
    let after = if text[at..].starts_with("\r\n") {
        at + 2
    } else if text[at..].starts_with('\n') {
        at + 1
    } else {
        return false;
    };
    tail(whitespace_end(text, after))
}

/// The profile's shared field boundary.
fn boundary(text: &str, at: usize) -> bool {
    let bytes = text.as_bytes();
    if text[at..].starts_with("**") {
        let run = text[at + 2..]
            .find(['*', '\r', '\n'])
            .map_or(text.len(), |i| at + 2 + i);
        let content = &text[at + 2..run];
        if content.len() >= 2
            && content.ends_with(':')
            && content[..content.len() - 1].chars().count() <= 100
            && text[run..].starts_with("**")
        {
            return true;
        }
    }
    for title in FIELD_TITLES {
        if let Some(end) = at_ci(text, at, title)
            && text[whitespace_end(text, end)..].starts_with(':')
        {
            return true;
        }
    }
    if let Some(end) = at_ci(text, at, "Category") {
        let amp = whitespace_end(text, end);
        if text[amp..].starts_with('&')
            && let Some(end) = at_ci(text, whitespace_end(text, amp + 1), "Tier")
            && text[whitespace_end(text, end)..].starts_with(':')
        {
            return true;
        }
    }
    let run = |ch: u8| bytes[at..].iter().take_while(|b| **b == ch).count();
    let hashes = run(b'#');
    let space_after = |i: usize| text[i..].chars().next().is_some_and(char::is_whitespace);
    if (hashes >= 2 && space_after(at + 2)) || (hashes >= 3 && space_after(at + 3)) {
        return true;
    }
    if run(b'_') >= 3 || run(b'-') >= 3 {
        return true;
    }
    admonition(text, at).is_some()
}

fn admonition(text: &str, at: usize) -> Option<usize> {
    let after = at_cs(text, at, ":::")?;
    ADMONITIONS.iter().find_map(|kind| at_ci(text, after, kind))
}

/// `(?:(?!\r?\n\s*<boundary>).)*` starting at `start`.
fn body_end(text: &str, start: usize) -> usize {
    let mut at = start;
    while at < text.len() && !newline_then(text, at, |i| boundary(text, i)) {
        at = next_char(text, at);
    }
    at
}

/// `:::kind\s*\r?\n\s*TITLE\s*\r?\n:\s*(?<g>(?:(?!\r?\n\s*:::).)*?)\r?\n\s*:::`
fn fence(text: &str, at: usize, titles: &[&str]) -> Option<(usize, usize)> {
    let kind = admonition(text, at)?;
    let title_start = whitespace_end(text, kind);
    if !text[kind..title_start].contains('\n') {
        return None;
    }
    let title = titles
        .iter()
        .find_map(|title| at_ci(text, title_start, title))?;
    let colon = whitespace_end(text, title);
    if !text[colon..].starts_with(':') || !text[title..colon].ends_with('\n') {
        return None;
    }
    let closes = |i: usize| newline_then(text, i, |j| text[j..].starts_with(":::"));
    let value = colon + 1;
    let mut start = whitespace_end(text, value);
    loop {
        let mut end = start;
        while end < text.len() && !closes(end) {
            end = next_char(text, end);
        }
        if end < text.len() {
            return Some((start, end));
        }
        if start == value {
            return None;
        }
        start = text[..start]
            .char_indices()
            .next_back()
            .map_or(value, |(i, _)| i);
    }
}

/// Labelled free-text body: `LABEL\s*(?<g>body)`.
fn labelled(text: &str, at: usize, field: Field) -> Option<(usize, usize)> {
    let titles = field.titles();
    let label_end = titles.iter().find_map(|title| {
        let bold = at_cs(text, at, "**")
            .and_then(|i| at_ci(text, i, title))
            .and_then(|i| at_cs(text, i, ":**"));
        if bold.is_some() {
            return bold;
        }
        match field {
            Field::Description => at_ci(text, at, title).and_then(|i| at_cs(text, i, ":")),
            Field::Directions => None,
            _ => at_ci(text, at, title).map(|i| at_cs(text, i, ":").unwrap_or(i)),
        }
    });
    if let Some(end) = label_end {
        let start = whitespace_end(text, end);
        return Some((start, body_end(text, start)));
    }
    if matches!(field, Field::Directions) && (at == 0 || text[..at].ends_with('\n')) {
        // (?m)^TITLE:?\s*(?:\r?\n)+(?<g>body): the value starts after the last
        // line break of the whitespace run that follows the label.
        let end = titles.iter().find_map(|title| at_ci(text, at, title))?;
        let end = at_cs(text, end, ":").unwrap_or(end);
        let run = whitespace_end(text, end);
        let start = end + text[end..run].rfind('\n')? + 1;
        return Some((start, body_end(text, start)));
    }
    None
}

fn leftmost(text: &str, matcher: impl Fn(usize) -> Option<(usize, usize)>) -> Found {
    let mut at = 0;
    while at <= text.len() {
        if let Some((start, end)) = matcher(at) {
            let value = &text[start..end];
            return Some((!value.trim().is_empty()).then_some((start, end)));
        }
        if at == text.len() {
            break;
        }
        at = next_char(text, at);
    }
    None
}

fn finish(text: &str, found: Found) -> Option<String> {
    let (start, end) = found??;
    Some(strip_definition_prefix(text[start..end].trim()))
}

/// Reference `StripDefinitionListPrefix`.
fn strip_definition_prefix(value: &str) -> String {
    if !value.contains(':') {
        return value.to_owned();
    }
    let mut stripped = false;
    let lines: Vec<String> = value
        .split("\r\n")
        .flat_map(|line| line.split(['\r', '\n']))
        .map(|line| match line.trim_start().strip_prefix(':') {
            Some(rest) if !rest.starts_with(':') => {
                stripped = true;
                rest.trim_start().to_owned()
            }
            _ => line.to_owned(),
        })
        .collect();
    if stripped {
        lines.join("\n").trim().to_owned()
    } else {
        value.to_owned()
    }
}

pub(super) fn field(text: &str, field: Field) -> Option<String> {
    let titles = field.titles();
    finish(
        text,
        leftmost(text, |at| {
            fence(text, at, titles).or_else(|| labelled(text, at, field))
        }),
    )
}

/// Single-line fields: `(?:\*\*LABEL:\*\*|LABEL:)\s*(?<g>[^\r\n]+)`.
pub(super) fn line_field(text: &str, title: &str) -> Option<String> {
    finish(
        text,
        leftmost(text, |at| {
            let end = at_cs(text, at, "**")
                .and_then(|i| at_ci(text, i, title))
                .and_then(|i| at_cs(text, i, ":**"))
                .or_else(|| at_ci(text, at, title).and_then(|i| at_cs(text, i, ":")))?;
            let start = whitespace_end(text, end);
            Some((start, line_end(text, start)))
        }),
    )
}

/// `(?:\*\*Name:\*\*|Name:)\s*(?:\[(?<name>[^\]]+)\]\([^)]+\)|(?<name_plain>[^\r\n]+))[^\r\n]*`
/// at `at`: the captured group and the value span after the label.
fn name_at(text: &str, at: usize, ignore_case: bool) -> Option<((usize, usize), (usize, usize))> {
    let word = |i: usize, w: &str| {
        if ignore_case {
            at_ci(text, i, w)
        } else {
            at_cs(text, i, w)
        }
    };
    let end = at_cs(text, at, "**")
        .and_then(|i| word(i, "Name"))
        .and_then(|i| at_cs(text, i, ":**"))
        .or_else(|| word(at, "Name").and_then(|i| at_cs(text, i, ":")))?;
    let start = whitespace_end(text, end);
    if text[start..].starts_with('[')
        && let Some(close) = text[start + 1..].find(']').map(|i| start + 1 + i)
        && close > start + 1
        && text[close..].starts_with("](")
        && let Some(paren) = text[close + 2..].find(')').map(|i| close + 2 + i)
        && paren > close + 2
    {
        return Some(((start + 1, close), (start, line_end(text, paren + 1))));
    }
    let end = line_end(text, start);
    (end > start).then_some(((start, end), (start, end)))
}

/// Name field value: a leading `[label](url)` yields the label, otherwise the line.
pub(super) fn name(text: &str) -> Option<String> {
    finish(
        text,
        leftmost(text, |at| name_at(text, at, true).map(|(group, _)| group)),
    )
}

/// Case-sensitive Name field text after its label, used for resource links.
pub(super) fn name_field(text: &str) -> Option<&str> {
    let mut at = 0;
    while at < text.len() {
        if let Some((_, (start, end))) = name_at(text, at, false) {
            return Some(text[start..end].trim());
        }
        at = next_char(text, at);
    }
    None
}

/// `^###\s+(?<heading>.+?)(?:\s*\[.*?\])?\s*$`.
pub(super) fn heading(text: &str) -> Option<String> {
    let line_end_follows = |at: usize| {
        let run = whitespace_end(text, at);
        run == text.len() || text[at..run].contains('\n')
    };
    let tail = |at: usize| {
        let open = whitespace_end(text, at);
        if text[open..].starts_with('[') {
            let mut search = open + 1;
            while let Some(close) = text[search..].find(']').map(|i| search + i) {
                if line_end_follows(close + 1) {
                    return true;
                }
                search = close + 1;
            }
        }
        line_end_follows(at)
    };
    finish(
        text,
        leftmost(text, |at| {
            if at != 0 && !text[..at].ends_with('\n') {
                return None;
            }
            let hashes = at_cs(text, at, "###")?;
            let start = whitespace_end(text, hashes);
            if start == hashes || start == text.len() {
                return None;
            }
            let mut end = next_char(text, start);
            loop {
                if tail(end) {
                    return Some((start, end));
                }
                if end >= text.len() {
                    return None;
                }
                end = next_char(text, end);
            }
        }),
    )
}

/// Case-sensitive `Category & Tier` field and its normalized category list.
pub(super) fn category_tier(text: &str) -> Option<(Vec<String>, String)> {
    let mut at = 0;
    while at < text.len() {
        let label = at_cs(text, at, "**")
            .and_then(|i| category_label(text, i))
            .and_then(|i| at_cs(text, i, ":**"))
            .or_else(|| category_label(text, at).and_then(|i| at_cs(text, i, ":")));
        if let Some(label) = label {
            let start = whitespace_end(text, label);
            let stop = text[start..]
                .find(['/', '\r', '\n'])
                .map_or(text.len(), |i| start + i);
            if stop > start && text[stop..].starts_with('/') {
                let tier_start = whitespace_end(text, stop + 1);
                let tier_end = line_end(text, tier_start);
                if tier_end > tier_start {
                    let category = normalize_category(&text[start..stop]);
                    let categories = category
                        .split('&')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect();
                    return Some((categories, text[tier_start..tier_end].trim().to_owned()));
                }
            }
        }
        at = next_char(text, at);
    }
    None
}

fn category_label(text: &str, at: usize) -> Option<usize> {
    let end = at_cs(text, at, "Category")?;
    let amp = whitespace_end(text, end);
    let after = at_cs(text, amp, "&")?;
    at_cs(text, whitespace_end(text, after), "Tier")
}

fn normalize_category(category: &str) -> String {
    let collapsed = category.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    for (index, part) in collapsed.split(['&', ',']).enumerate() {
        if index > 0 {
            out.push_str(" & ");
        }
        out.push_str(part.trim());
    }
    out.trim().to_owned()
}
