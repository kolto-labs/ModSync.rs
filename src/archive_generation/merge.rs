use crate::reference_paths::pattern_matches;
use crate::{Component, Instruction};

fn normalize(path: &str) -> String {
    path.replace('/', "\\").trim_end_matches('\\').to_owned()
}
fn wildcard(path: &str) -> bool {
    path.contains(['*', '?'])
}

fn patterns_overlap(a: &str, b: &str) -> bool {
    let a: Vec<_> = a.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let b: Vec<_> = b.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    for (a, b) in a.iter().zip(&b).take(a.len().min(b.len()) - 1) {
        if a.eq_ignore_ascii_case(b) {
            continue;
        }
        if wildcard(a) && !wildcard(b) {
            if !pattern_matches(b, a) {
                return false;
            }
        } else if wildcard(b) && !wildcard(a) {
            if !pattern_matches(a, b) {
                return false;
            }
        } else {
            return false;
        }
    }
    let a = a.last().unwrap();
    let b = b.last().unwrap();
    a.eq_ignore_ascii_case(b)
        || *a == "*"
        || *b == "*"
        || (wildcard(a) && !wildcard(b) && pattern_matches(b, a))
        || (wildcard(b) && !wildcard(a) && pattern_matches(a, b))
}

fn sources_match(a: &str, b: &str) -> bool {
    let a = normalize(a);
    let b = normalize(b);
    if a.eq_ignore_ascii_case(&b) {
        return true;
    }
    match (wildcard(&a), wildcard(&b)) {
        (true, true) if patterns_overlap(&a, &b) => return true,
        (true, false) if pattern_matches(&b, &a) => return true,
        (false, true) if pattern_matches(&a, &b) => return true,
        _ => {}
    }
    // C# Path.GetFileName follows the host separator rules after normalization.
    // Preserve that distinction instead of treating Unix backslashes as separators.
    let a = std::path::Path::new(&a)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("");
    let b = std::path::Path::new(&b)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("");
    match (wildcard(a), wildcard(b)) {
        (true, true) => false,
        (false, false) => a.eq_ignore_ascii_case(b),
        (true, false) => pattern_matches(b, a),
        (false, true) => pattern_matches(a, b),
    }
}

pub(super) fn equivalent(a: &Instruction, b: &Instruction) -> bool {
    if a.action != b.action
        || (a.source.is_empty() != b.source.is_empty())
        || !b
            .source
            .iter()
            .all(|b| a.source.iter().any(|a| sources_match(a, b)))
    {
        return false;
    }
    if matches!(
        a.action.as_str(),
        "Move" | "Copy" | "Rename" | "Patcher" | "Delete" | "CleanList"
    ) {
        let x = normalize(&a.destination);
        let y = normalize(&b.destination);
        if !(x.eq_ignore_ascii_case(&y)
            || (wildcard(&x) && pattern_matches(&y, &x))
            || (wildcard(&y) && pattern_matches(&x, &y)))
        {
            return false;
        }
    }
    if matches!(a.action.as_str(), "DelDuplicate" | "Execute" | "Patcher")
        && !a.arguments.eq_ignore_ascii_case(&b.arguments)
    {
        return false;
    }
    !matches!(a.action.as_str(), "Move" | "Copy" | "Rename") || a.overwrite == b.overwrite
}

pub(super) fn folder_covered(component: &Component, source: &str) -> bool {
    component
        .instructions
        .iter()
        .filter(|i| matches!(i.action.as_str(), "Move" | "Copy"))
        .flat_map(|i| &i.source)
        .any(|existing| {
            if sources_match(existing, source) {
                return true;
            }
            let parent = normalize(existing);
            let child = normalize(source);
            let prefix = parent.trim_end_matches(['*', '\\']).to_lowercase();
            let child = child.trim_end_matches(['*', '\\']).to_lowercase();
            parent.ends_with("\\*")
                && child
                    .strip_prefix(&prefix)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with('\\'))
        })
}

pub(super) fn consolidate(component: &mut Component) {
    // Match the reference's snapshot ordering: identify all overlapping candidates
    // before adding any of their instructions to the primary option.
    let mut index = 0;
    while index < component.options.len() {
        let duplicates: Vec<_> = ((index + 1)..component.options.len())
            .filter(|&other| {
                component.options[other].instructions.iter().any(|b| {
                    component.options[index]
                        .instructions
                        .iter()
                        .any(|a| equivalent(a, b))
                })
            })
            .collect();
        let primary_id = component.options[index].guid.clone();
        for (removed, other) in duplicates.into_iter().enumerate() {
            let duplicate = component.options.remove(other - removed);
            for mut step in duplicate.instructions {
                if !component.options[index]
                    .instructions
                    .iter()
                    .any(|a| equivalent(a, &step))
                {
                    step.guid.clear();
                    step.parent = primary_id.clone();
                    component.options[index].instructions.push(step);
                }
            }
            for choose in component
                .instructions
                .iter_mut()
                .filter(|i| i.action == "Choose")
            {
                // Choose entries name options by C# `Guid.ToString()` text.
                if let Some(position) = choose
                    .source
                    .iter()
                    .position(|s| crate::guid::names_option(&duplicate.guid, s))
                {
                    if choose
                        .source
                        .iter()
                        .any(|s| crate::guid::names_option(&primary_id, s))
                    {
                        choose.source.remove(position);
                    } else {
                        choose.source[position] = crate::guid::option_text(&primary_id);
                    }
                }
            }
        }
        index += 1;
    }
}
