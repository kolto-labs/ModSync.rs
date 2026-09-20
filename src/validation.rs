use crate::conflicts::Report as ConflictReport;
use serde::Serialize;

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
    /// Returns the first error, or the first warning when there are no errors.
    ///
    /// The desktop uses this for its "Go to first issue" control, matching
    /// the reference wizard which makes the control available for warnings as
    /// well as errors.
    pub fn first_issue(&self) -> Option<&str> {
        self.stages
            .iter()
            .find(|stage| !stage.passed)
            .or_else(|| self.stages.iter().find(|stage| stage.has_warnings))
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
    pub(crate) fn dry_run(&mut self, issues: Vec<DryRunIssue>) {
        let has_errors = issues.iter().any(|issue| {
            issue.severity.eq_ignore_ascii_case("error")
                || issue.severity.eq_ignore_ascii_case("critical")
        });
        let has_warnings = issues
            .iter()
            .any(|issue| issue.severity.eq_ignore_ascii_case("warning"));
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
            self.error_count += 1;
        } else if has_warnings {
            self.warning_count += 1;
        } else {
            self.passed_count += 1;
        }
        self.stages.push(StageResult {
            stage: Stage::DryRun,
            passed: !has_errors,
            has_warnings,
            summary: if has_errors {
                "Dry-run validation failed."
            } else if has_warnings {
                "Dry-run validation passed with warnings."
            } else {
                "Dry-run validation passed."
            }
            .into(),
            messages,
        });
        self.dry_run = Some(DryRun {
            is_valid: !has_errors,
            has_warnings,
            issues,
        });
    }
    pub(crate) fn conflicts(&mut self, conflicts: &ConflictReport) {
        let count = conflicts.conflicts.len();
        let mut messages = conflicts.limitations.clone();
        messages.extend(conflicts.conflicts.iter().map(|conflict| {
            format!(
                "WARNING: {} has {} writers",
                conflict.destination_path,
                conflict.writers.len()
            )
        }));
        let warnings = messages.len();
        self.warning_count += warnings;
        self.stages.push(StageResult {
            stage: Stage::Conflicts,
            passed: true,
            has_warnings: warnings > 0,
            summary: if count == 0 {
                "No simulated file conflicts".into()
            } else {
                format!("{count} simulated file conflicts")
            },
            messages,
        });
        if warnings == 0 {
            self.passed_count += 1;
        }
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
}
