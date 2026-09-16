use crate::{Component, Instruction};

fn normalize(path: &str) -> String {
    path.replace('/', "\\").trim_end_matches('\\').to_owned()
}
fn wildcard(path: &str) -> bool {
    path.contains(['*', '?'])
}
fn pattern_matches(input: &str, pattern: &str) -> bool {
    fn format(path: &str) -> String {
        let path = path.trim_start_matches('\\').replace('\\', "/");
        let mut result = String::new();
        for c in path.chars() {
            if c == '/' && result.ends_with('/') && !result.ends_with(":/") {
                continue;
            }
            result.push(c);
        }
        if result.len() > 1 {
            result.truncate(result.trim_end_matches('/').len());
        }
        result
    }
    let input = format(input);
    let pattern = format(pattern);
    let inputs: Vec<_> = input.split('/').collect();
    let patterns: Vec<_> = pattern.split('/').collect();
    inputs.len() == patterns.len()
        && inputs.iter().zip(patterns).all(|(input, pattern)| {
            let regex = format!(
                "^{}$",
                regex::escape(pattern)
                    .replace("\\*", ".*")
                    .replace("\\?", ".")
            );
            regex::Regex::new(&regex).is_ok_and(|r| r.is_match(input))
        })
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
                if let Some(position) = choose
                    .source
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(&duplicate.guid))
                {
                    if choose
                        .source
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case(&primary_id))
                    {
                        choose.source.remove(position);
                    } else {
                        choose.source[position] = primary_id.clone();
                    }
                }
            }
        }
        index += 1;
    }
}
