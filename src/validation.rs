use serde::Serialize;

pub mod basic;
pub mod presentation;

/// A component validator's independent return value and collected diagnostics.
/// C# can return false without recording an error; preserve that distinction.
#[derive(Clone, Debug, Default)]
pub struct ComponentOutcome {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Actual stage boundaries; start and completion share the one-based step.
/// Completion messages start with `Completed: `, including failed outcomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageProgress {
    pub stage: Stage,
    pub step: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum Stage {
    #[serde(rename = "Environment")]
    Environment,
    #[serde(rename = "Conflicts")]
    Conflicts,
    #[serde(rename = "InstallOrder")]
    InstallOrder,
    #[serde(rename = "ComponentValidation")]
    ComponentValidation,
    #[serde(rename = "FomodConfiguration")]
    FomodConfiguration,
    #[serde(rename = "DryRun")]
    DryRun,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunIssue {
    pub severity: String,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution: Option<String>,
}
impl DryRunIssue {
    pub(crate) fn skipped(component: &crate::Component) -> Self {
        let mut issue = Self::engine(
            format!(
                "Skipped '{}' during dry-run: source archives or files are not available.",
                component.name
            ),
            Some((component.name.clone(), component.guid.clone(), None)),
        );
        issue.severity = "Warning".into();
        issue.category = "DryRunSkipped".into();
        issue
    }

    pub(crate) fn engine(message: String, target: Option<(String, String, Option<usize>)>) -> Self {
        let (component_name, component_guid, instruction_index) = target
            .map(|(name, guid, index)| (Some(name), Some(guid), index))
            .unwrap_or_default();
        Self {
            severity: "Error".into(),
            category: "EngineSimulation".into(),
            message,
            affected_path: None,
            component_name,
            component_guid,
            instruction_index,
            solution: None,
        }
    }

    pub(crate) fn warning(
        category: String,
        message: String,
        affected_path: String,
        target: Option<(String, String, Option<usize>)>,
    ) -> Self {
        let mut issue = Self::engine(message, target);
        issue.severity = "Warning".into();
        issue.category = category;
        issue.affected_path = Some(affected_path);
        issue
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRun {
    pub is_valid: bool,
    pub has_warnings: bool,
    pub issues: Vec<DryRunIssue>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageResult {
    pub stage: Stage,
    pub passed: bool,
    pub has_warnings: bool,
    pub summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub success: bool,
    pub exit_code: u8,
    pub error_count: usize,
    pub warning_count: usize,
    pub passed_count: usize,
    pub component_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_path: Option<String>,
    pub stages: Vec<StageResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<DryRun>,
}
/// Machine-readable failure report emitted when validation cannot produce a
/// pipeline result at all (guide load, selection, or engine setup failed before
/// any stage ran). Matches the C# `ValidationPipelineJsonFormatter.SerializeError`
/// shape: the scalar counters serialize as zero/false, and `inputPath`, `stages`,
/// and `dryRun` are omitted.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReport {
    pub success: bool,
    pub exit_code: u8,
    pub error_count: usize,
    pub warning_count: usize,
    pub passed_count: usize,
    pub component_count: usize,
    pub error: String,
}
impl ErrorReport {
    pub fn new(error: String) -> Self {
        Self {
            success: false,
            exit_code: 1,
            error_count: 0,
            warning_count: 0,
            passed_count: 0,
            component_count: 0,
            error,
        }
    }
}
impl Report {
    /// Aggregate the basic validator's diagnostics as the reference pipeline does:
    /// component errors suppress that component's warnings, and stage errors
    /// suppress warnings in the report totals. A silent false contributes no OK
    /// message but does not itself fail the stage.
    pub fn component_outcomes(&mut self, outcomes: &[(&crate::Component, ComponentOutcome)]) {
        let mut errors = 0;
        let mut warnings = 0;
        let mut messages = Vec::new();
        for (component, outcome) in outcomes {
            if !outcome.errors.is_empty() {
                errors += outcome.errors.len();
                messages.extend(
                    outcome
                        .errors
                        .iter()
                        .map(|message| format!("ERROR: {}: {message}", component.name)),
                );
            } else if !outcome.warnings.is_empty() {
                warnings += outcome.warnings.len();
                messages.extend(
                    outcome
                        .warnings
                        .iter()
                        .map(|message| format!("WARNING: {}: {message}", component.name)),
                );
            } else if outcome.is_valid {
                messages.push(format!("OK: {}", component.name));
            }
        }
        if errors > 0 {
            self.error_count += errors;
        } else if warnings > 0 {
            self.warning_count += warnings;
        } else {
            self.passed_count += 1;
        }
        self.stages.push(StageResult {
            stage: Stage::ComponentValidation,
            passed: errors == 0,
            has_warnings: warnings > 0,
            summary: if errors > 0 {
                format!("{errors} component error(s)")
            } else if warnings > 0 {
                format!("{warnings} warning(s)")
            } else {
                "All components passed archive validation.".into()
            },
            messages,
        });
    }

    /// Returns the first error, or the first warning when there are no errors.
    ///
    /// The desktop uses this for its "Go to first issue" control, matching
    /// the reference wizard which makes the control available for warnings as
    /// well as errors.
    pub fn first_issue(&self) -> Option<&str> {
        if let Some(stage) = self.stages.iter().find(|stage| !stage.passed) {
            // A failed stage can list soft warnings or passed entries before its error.
            return stage
                .messages
                .iter()
                .find(|message| !message.starts_with("WARNING:") && !message.starts_with("OK:"))
                .or(stage.messages.first())
                .map(String::as_str);
        }
        self.stages
            .iter()
            .find(|stage| stage.has_warnings)
            .and_then(|stage| stage.messages.first().map(String::as_str))
    }

    pub fn new(component_count: usize) -> Self {
        Self {
            success: false,
            exit_code: 1,
            error_count: 0,
            warning_count: 0,
            passed_count: 0,
            component_count,
            input_path: None,
            stages: Vec::new(),
            dry_run: None,
        }
    }
    pub(crate) fn push(&mut self, stage: Stage, outcome: Result<(), String>) -> bool {
        match outcome {
            Ok(()) => {
                self.passed_count += 1;
                self.stages.push(StageResult {
                    stage,
                    passed: true,
                    has_warnings: false,
                    summary: "Passed".into(),
                    messages: Vec::new(),
                });
                true
            }
            Err(error) => {
                self.error_count += 1;
                self.stages.push(StageResult {
                    stage,
                    passed: false,
                    has_warnings: false,
                    summary: "Failed".into(),
                    messages: vec![error],
                });
                false
            }
        }
    }
    pub(crate) fn component_validation(
        &mut self,
        components: &[&crate::Component],
        outcome: Result<Vec<Result<(), Vec<String>>>, String>,
    ) {
        let outcomes = outcome.unwrap_or_else(|error| vec![Err(vec![error])]);
        // One component can report several errors (one per missing archive).
        let errors = outcomes
            .iter()
            .map(|result| result.as_ref().map_or_else(Vec::len, |()| 0))
            .sum::<usize>();
        let passed = errors == 0;
        if passed {
            self.passed_count += 1;
        } else {
            self.error_count += errors;
        }
        self.stages.push(StageResult {
            stage: Stage::ComponentValidation,
            passed,
            has_warnings: false,
            summary: if passed {
                "All components passed archive validation.".into()
            } else {
                format!("{errors} component error(s)")
            },
            messages: outcomes
                .into_iter()
                .enumerate()
                .flat_map(|(index, outcome)| {
                    outcome.map_or_else(
                        // The engine retains component identity in this diagnostic. Give
                        // shared cards and first-issue navigation its explicit severity.
                        |errors| {
                            errors
                                .into_iter()
                                .map(|error| {
                                    if error.starts_with("ERROR:") {
                                        error
                                    } else {
                                        format!("ERROR: {error}")
                                    }
                                })
                                .collect()
                        },
                        |()| vec![format!("OK: {}", components[index].name)],
                    )
                })
                .collect(),
        });
    }
    /// The report without a dry run: plain `validate`'s step simulation failed after
    /// every stage passed. The component stage fails with that error, which replaces
    /// the failing component's `OK:` line (or is added when no component is named).
    pub(crate) fn simulation_failure(&mut self, component: Option<usize>, error: String) {
        let message = format!("ERROR: {error}");
        self.error_count += 1;
        let Some(stage) = self
            .stages
            .iter_mut()
            .find(|stage| stage.stage == Stage::ComponentValidation)
        else {
            self.stages.push(StageResult {
                stage: Stage::ComponentValidation,
                passed: false,
                has_warnings: false,
                summary: "1 component error(s)".into(),
                messages: vec![message],
            });
            return;
        };
        if stage.passed {
            self.passed_count = self.passed_count.saturating_sub(1);
        }
        stage.passed = false;
        stage.summary = "1 component error(s)".into();
        match component.and_then(|index| stage.messages.get_mut(index)) {
            Some(line) => *line = message,
            None => stage.messages.push(message),
        }
    }
    pub(crate) fn install_order(&mut self, outcome: Result<(), String>) {
        let passed = self.push(Stage::InstallOrder, outcome);
        if passed {
            self.stages.last_mut().unwrap().summary = "Install order is correct.".into();
        }
    }
    /// Gate warnings are counted and shown. Alone they keep the stage passing,
    /// like dry-run warnings; with gate errors they are listed before them. Each
    /// error is one message, in gate order, and counts once in `errorCount`.
    pub(crate) fn fomod_configuration(&mut self, warnings: Vec<String>, errors: Vec<String>) {
        let mut errors = errors.into_iter();
        if let Some(error) = errors.next() {
            // As in C#, soft warnings stay in a failed stage, before the errors.
            self.push(Stage::FomodConfiguration, Err(error));
            let rest: Vec<String> = errors.collect();
            self.error_count += rest.len();
            self.stages.last_mut().unwrap().messages.extend(rest);
            if !warnings.is_empty() {
                self.warning_count += warnings.len();
                let stage = self.stages.last_mut().unwrap();
                stage.has_warnings = true;
                stage.messages.splice(
                    0..0,
                    warnings
                        .into_iter()
                        .map(|warning| format!("WARNING: {warning}")),
                );
            }
            return;
        }
        if warnings.is_empty() {
            self.push(Stage::FomodConfiguration, Ok(()));
            self.stages.last_mut().unwrap().summary =
                "All detected FOMOD archives are configured.".into();
            return;
        }
        self.warning_count += warnings.len();
        self.stages.push(StageResult {
            stage: Stage::FomodConfiguration,
            passed: true,
            has_warnings: true,
            summary: format!(
                "All detected FOMOD archives are configured ({} warning(s)).",
                warnings.len()
            ),
            messages: warnings
                .into_iter()
                .map(|warning| format!("WARNING: {warning}"))
                .collect(),
        });
    }
    pub(crate) fn dry_run(&mut self, issues: Vec<DryRunIssue>) {
        let errors = issues
            .iter()
            .filter(|issue| {
                issue.severity.eq_ignore_ascii_case("error")
                    || issue.severity.eq_ignore_ascii_case("critical")
            })
            .count();
        let warnings = issues
            .iter()
            .filter(|issue| issue.severity.eq_ignore_ascii_case("warning"))
            .count();
        let has_errors = errors > 0;
        let has_warnings = warnings > 0;
        let messages = issues
            .iter()
            .map(|issue| {
                format!(
                    "{}: [{}] {}",
                    issue.severity.to_ascii_uppercase(),
                    issue.category,
                    issue.message
                )
            })
            .collect();
        if has_errors {
            self.error_count += errors;
        } else if has_warnings {
            self.warning_count += warnings;
        } else {
            self.passed_count += 1;
        }
        self.stages.push(StageResult {
            stage: Stage::DryRun,
            passed: !has_errors,
            has_warnings,
            summary: if has_errors {
                format!(
                    "✗ Validation failed with {errors} error(s) and {warnings} warning(s).\n\nThe following issues must be resolved before installation:\n"
                )
            } else if has_warnings {
                format!(
                    "⚠ Validation passed with {warnings} warning(s).\n\nYou may proceed, but review the following warnings:\n"
                )
            } else {
                "✓ Dry-run validation passed successfully. All instructions appear to be correct."
                    .into()
            },
            messages,
        });
        self.dry_run = Some(DryRun {
            is_valid: !has_errors,
            has_warnings,
            issues,
        });
    }
    pub(crate) fn declared_conflicts(
        &mut self,
        guide: &crate::Guide,
        components: &[&crate::Component],
        selected: &std::collections::BTreeSet<String>,
    ) {
        let selected: std::collections::BTreeSet<_> = selected
            .iter()
            .map(|id| crate::guid::key_ascii(id))
            .collect();
        let name = |id: &String| {
            guide
                .components
                .iter()
                .find(|component| crate::guid::eq_ascii(&component.guid, id))
                .map_or_else(|| id.clone(), |component| component.name.clone())
        };
        let mut messages = Vec::new();
        let mut errors = 0;
        let mut warnings = 0;
        for component in components {
            let missing: Vec<_> = component
                .dependencies
                .iter()
                .filter(|id| !selected.contains(&crate::guid::key_ascii(id)))
                .map(&name)
                .collect();
            if !missing.is_empty() {
                warnings += 1;
                messages.push(format!(
                    "WARNING: {}: missing dependencies: {}",
                    component.name,
                    missing.join(", ")
                ));
            }
            let incompatible: Vec<_> = component
                .restrictions
                .iter()
                .filter(|id| selected.contains(&crate::guid::key_ascii(id)))
                .map(&name)
                .collect();
            if !incompatible.is_empty() {
                errors += 1;
                messages.push(format!(
                    "ERROR: {}: incompatible with: {}",
                    component.name,
                    incompatible.join(", ")
                ));
            }
        }
        // The reference counts component diagnostics, with errors taking precedence.
        if errors > 0 {
            self.error_count += errors;
        } else if warnings > 0 {
            self.warning_count += warnings;
        } else {
            self.passed_count += 1;
        }
        self.stages.push(StageResult {
            stage: Stage::Conflicts,
            passed: errors == 0,
            has_warnings: warnings > 0,
            summary: if errors > 0 {
                format!("{errors} restriction conflict(s)")
            } else if warnings > 0 {
                format!("{warnings} dependency warning(s)")
            } else {
                "No dependency or restriction conflicts.".into()
            },
            messages,
        });
    }
    pub(crate) fn finish(mut self) -> Self {
        self.success = self.error_count == 0;
        self.exit_code = u8::from(!self.success);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_counts_individual_issues_with_reference_error_precedence() {
        for (severities, counts) in [
            (vec!["Error", "Critical", "Warning"], (2, 0, 0)),
            (vec!["Warning", "Warning"], (0, 2, 0)),
            (vec![], (0, 0, 1)),
        ] {
            let mut report = Report::new(1);
            report.dry_run(
                severities
                    .iter()
                    .map(|severity| {
                        let mut issue = DryRunIssue::engine("fixture".into(), None);
                        issue.severity = (*severity).into();
                        issue
                    })
                    .collect(),
            );
            assert_eq!(
                (
                    report.error_count,
                    report.warning_count,
                    report.passed_count
                ),
                counts
            );
            assert_eq!(
                report.dry_run.unwrap().has_warnings,
                severities.contains(&"Warning")
            );
        }
    }

    #[test]
    fn first_issue_prefers_errors_then_falls_back_to_warnings() {
        let mut report = Report::new(0);
        report.stages = vec![
            StageResult {
                stage: Stage::Environment,
                passed: true,
                has_warnings: true,
                summary: "Warning".into(),
                messages: vec!["first warning".into()],
            },
            StageResult {
                stage: Stage::Conflicts,
                passed: false,
                has_warnings: false,
                summary: "Failed".into(),
                messages: vec!["later error".into()],
            },
        ];

        assert_eq!(report.first_issue(), Some("later error"));
        report.stages[1].passed = true;
        assert_eq!(report.first_issue(), Some("first warning"));
    }

    #[test]
    fn json_shapes_match_the_pinned_csharp_formatter_oracle() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../docs/parity/validation-json-oracle/result.json"
        ))
        .unwrap();
        let report = Report {
            success: false,
            exit_code: 1,
            error_count: 1,
            warning_count: 1,
            passed_count: 1,
            component_count: 1,
            input_path: Some("/guides/fixture.json".into()),
            stages: vec![
                StageResult {
                    stage: Stage::Environment,
                    passed: true,
                    has_warnings: false,
                    summary: "Passed".into(),
                    messages: Vec::new(),
                },
                StageResult {
                    stage: Stage::DryRun,
                    passed: false,
                    has_warnings: true,
                    summary: "Dry-run validation failed.".into(),
                    messages: vec!["ERROR: [CopyFile] Source file does not exist".into()],
                },
            ],
            dry_run: Some(DryRun {
                is_valid: false,
                has_warnings: true,
                issues: vec![
                    DryRunIssue {
                        severity: "Error".into(),
                        category: "CopyFile".into(),
                        message: "Source file does not exist".into(),
                        affected_path: Some("/mods/missing.tga".into()),
                        component_name: Some("Fixture".into()),
                        component_guid: Some("11111111-1111-1111-1111-111111111111".into()),
                        instruction_index: Some(2),
                        solution: Some("Restore the source file".into()),
                    },
                    DryRunIssue {
                        severity: "Warning".into(),
                        category: "DeleteFile".into(),
                        message: "Attempting to delete non-existent file".into(),
                        affected_path: Some("/game/missing.tga".into()),
                        component_name: Some("Fixture".into()),
                        component_guid: Some("11111111-1111-1111-1111-111111111111".into()),
                        instruction_index: None,
                        solution: None,
                    },
                ],
            }),
        };
        let error = ErrorReport {
            success: false,
            exit_code: 7,
            error_count: 0,
            warning_count: 0,
            passed_count: 0,
            component_count: 0,
            error: "fixture failure".into(),
        };
        assert_eq!(serde_json::to_value(report).unwrap(), oracle["report"]);
        assert_eq!(serde_json::to_value(error).unwrap(), oracle["error"]);
    }
}
