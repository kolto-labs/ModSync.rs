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
}
impl Report {
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
