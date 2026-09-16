// Reference wildcard semantics for inference and download planning.
pub(crate) fn pattern_matches(input: &str, pattern: &str) -> bool {
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
