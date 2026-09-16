//! Preflight for Markdown guides used as the sole install authority.
use crate::Component;

#[derive(Debug, PartialEq, Eq)]
pub struct Result {
    pub unresolved_components: Vec<String>,
    pub skipped_four_gb: usize,
    pub skipped_widescreen: usize,
    pub drafted_from_prose: usize,
}

impl Result {
    pub fn ready(&self) -> bool {
        self.unresolved_components.is_empty()
    }
}

pub fn apply(components: &mut [Component]) -> Result {
    let mut result = Result {
        unresolved_components: Vec::new(),
        skipped_four_gb: 0,
        skipped_widescreen: 0,
        drafted_from_prose: 0,
    };
    for component in components
        .iter_mut()
        .filter(|component| component.is_selected)
    {
        if component
            .metadata
            .get("WidescreenOnly")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            component.is_selected = false;
            result.skipped_widescreen += 1;
        } else if component.name.trim().eq_ignore_ascii_case("4GB Patcher") {
            component.is_selected = false;
            result.skipped_four_gb += 1;
        } else {
            let needs_review = crate::draft::requires_review(component);
            result.drafted_from_prose += needs_review as usize;
            if needs_review || !has_actions(component) {
                result.unresolved_components.push(component.name.clone());
            }
        }
    }
    result
}

fn has_actions(component: &Component) -> bool {
    !component.instructions.is_empty() || component.options.iter().any(has_actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Instruction;

    #[test]
    fn skips_special_components_and_fails_before_an_empty_component_runs() {
        let mut components = vec![
            Component {
                name: "Reviewed".into(),
                is_selected: true,
                instructions: vec![Instruction {
                    action: "DelDuplicate".into(),
                    guid: String::new(),
                    parent: String::new(),
                    source: Vec::new(),
                    destination: String::new(),
                    arguments: String::new(),
                    overwrite: true,
                    dependencies: Vec::new(),
                    restrictions: Vec::new(),
                }],
                ..Default::default()
            },
            Component {
                name: "Wide".into(),
                is_selected: true,
                metadata: [("WidescreenOnly".into(), serde_json::Value::Bool(true))]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            Component {
                name: "4GB Patcher".into(),
                is_selected: true,
                ..Default::default()
            },
            Component {
                name: "Undrafted".into(),
                is_selected: true,
                ..Default::default()
            },
            Component {
                name: "Needs review".into(),
                is_selected: true,
                instructions: vec![Instruction {
                    action: "DelDuplicate".into(),
                    guid: String::new(),
                    parent: String::new(),
                    source: Vec::new(),
                    destination: String::new(),
                    arguments: String::new(),
                    overwrite: true,
                    dependencies: Vec::new(),
                    restrictions: Vec::new(),
                }],
                metadata: [(
                    "DraftReview".into(),
                    serde_json::json!({"RequiresReview": true}),
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        ];

        let result = apply(&mut components);

        assert_eq!(result.skipped_widescreen, 1);
        assert_eq!(result.skipped_four_gb, 1);
        assert_eq!(result.unresolved_components, ["Undrafted", "Needs review"]);
        assert_eq!(result.drafted_from_prose, 1);
        assert!(!components[1].is_selected && !components[2].is_selected);
        assert!(!result.ready());
    }
}
