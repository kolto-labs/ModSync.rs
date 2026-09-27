//! Pure projection of shared validation outcomes into the wizard's human report.
use super::{DryRunIssue, Report, Stage};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Presentation {
    pub cards: Vec<Card>,
    pub log_lines: Vec<String>,
    pub summary: String,
    pub details: String,
    pub status: String,
}

impl Presentation {
    fn card(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.cards.push(Card {
            title: title.into(),
            message: message.into(),
        });
    }

    /// Errors take priority even when a warning occurs earlier in the output.
    pub fn first_issue_card(&self, warning_count: usize) -> Option<usize> {
        self.cards
            .iter()
            .position(|c| c.title.starts_with("❌"))
            .or_else(|| {
                (warning_count > 0)
                    .then(|| self.cards.iter().position(|c| c.title.starts_with("⚠️")))
                    .flatten()
            })
    }
}

/// Works on the full flushed page log, including caller-owned start/finish lines.
pub fn first_issue_log(full_log: &str, warning_count: usize) -> Option<usize> {
    full_log
        .split('\n')
        .position(|line| line.contains("ERROR:") || line.contains('❌'))
        .or_else(|| {
            (warning_count > 0)
                .then(|| {
                    full_log
                        .split('\n')
                        .position(|line| line.contains("WARNING:") || line.contains('⚠'))
                })
                .flatten()
        })
}

/// Exact known-category solution fallback from ValidationPipelineDialogMapper.
/// Unknown engine categories are deliberately left unexplained.
pub fn solution_for_issue(issue: &DryRunIssue) -> &str {
    if let Some(solution) = issue.solution.as_deref() {
        return solution;
    }
    match issue.category.as_str() {
        "ArchiveValidation" | "ExtractArchive" => {
            "Verify the archive exists and is not corrupted. Try re-downloading from the mod link."
        }
        "MoveFile" | "CopyFile" if issue.message.contains("does not exist") => {
            "The required file is missing. This may indicate an incomplete mod archive or incorrect source path."
        }
        "MoveFile" | "CopyFile" if issue.message.contains("already exists") => {
            "File conflict detected. This may be expected - ensure mod installation order is correct."
        }
        "DeleteFile" => {
            "File does not exist to delete. This may indicate incorrect instruction order."
        }
        "ExecuteProcess" => {
            "The required executable is missing. Verify the mod archive was extracted correctly."
        }
        _ => "",
    }
}

pub fn project(report: &Report, selected_count: usize) -> Presentation {
    let mut view = Presentation {
        summary: if report.success {
            "✅ Validation Passed"
        } else {
            "❌ Validation Failed"
        }
        .into(),
        status: if report.success {
            "Validation complete"
        } else {
            "Validation failed"
        }
        .into(),
        details: if !report.success {
            "Please resolve all critical errors before continuing. Check the results above for details.".into()
        } else if report.warning_count > 0 {
            // Rust warnings can require review rather than automatic remediation.
            format!(
                "Validation passed with {} warning(s). Review the results before continuing.",
                report.warning_count
            )
        } else {
            "Everything looks good! You're ready to proceed with the installation.".into()
        },
        ..Presentation::default()
    };
    for (index, stage) in report.stages.iter().enumerate() {
        let heading = match stage.stage {
            Stage::Environment => "Validating installation environment".into(),
            Stage::Conflicts => format!("Checking conflicts for {selected_count} selected mod(s)"),
            Stage::InstallOrder => "Validating mod installation order".into(),
            Stage::ComponentValidation => "Validating mod archives".into(),
            Stage::FomodConfiguration => "Checking FOMOD configuration".into(),
            Stage::DryRun => "Running instruction execution validation (dry-run)".into(),
        };
        view.log_lines
            .push(format!("Step {}: {heading}", index + 1));
        if stage.stage == Stage::DryRun {
            project_dry_run(&mut view, report);
            continue;
        }
        let before = view.cards.len();
        for message in &stage.messages {
            if stage.stage == Stage::ComponentValidation
                && let Some(detail) = message.strip_prefix("OK:")
            {
                view.log_lines.push(format!("  ✅ {}", detail.trim()));
                continue;
            }
            view.log_lines.push(format!("  {message}"));
            for (prefix, icon) in [("WARNING:", "⚠️"), ("ERROR:", "❌")] {
                if let Some(detail) = message.strip_prefix(prefix) {
                    let detail = detail.trim();
                    let name = detail
                        .find(':')
                        .filter(|&i| i > 0)
                        .map(|i| detail[..i].trim())
                        .unwrap_or("Unknown");
                    view.card(format!("{icon} {name}"), detail);
                    break;
                }
            }
        }
        let icon = if !stage.passed {
            "❌"
        } else if stage.has_warnings {
            "⚠️"
        } else {
            "✅"
        };
        match stage.stage {
            Stage::Environment => {
                view.log_lines.push(if stage.passed {
                    "  ✅ Environment validation passed".into()
                } else {
                    format!("  ❌ Environment validation failed: {}", stage.summary)
                });
                if stage.passed || view.cards.len() == before {
                    view.card(
                        if stage.passed {
                            "✅ Environment"
                        } else {
                            "❌ Environment Error"
                        },
                        &stage.summary,
                    );
                }
            }
            Stage::Conflicts | Stage::InstallOrder => {
                if stage.messages.is_empty() {
                    if !stage.passed {
                        view.log_lines.push(format!("  ❌ {}", stage.summary));
                    } else if !stage.has_warnings {
                        view.log_lines.push(
                            if stage.stage == Stage::Conflicts {
                                "  ✅ No conflicts"
                            } else {
                                "  ✅ Install order is correct"
                            }
                            .into(),
                        );
                    }
                }
                view.card(
                    format!(
                        "{icon} {}",
                        if stage.stage == Stage::Conflicts {
                            "Conflicts"
                        } else {
                            "Install Order"
                        }
                    ),
                    &stage.summary,
                );
            }
            Stage::ComponentValidation => {
                if !stage.passed || stage.has_warnings {
                    view.card(format!("{icon} Archive Validation"), &stage.summary);
                }
            }
            Stage::FomodConfiguration => view.card(
                format!(
                    "{} FOMOD Configuration",
                    if stage.passed { "✅" } else { "❌" }
                ),
                &stage.summary,
            ),
            Stage::DryRun => unreachable!(),
        }
    }
    view
}

fn project_dry_run(view: &mut Presentation, report: &Report) {
    let Some(dry) = &report.dry_run else {
        return;
    };
    if dry.is_valid && !dry.has_warnings {
        view.log_lines
            .push("  ✅ All instructions validated successfully".into());
        view.card(
            "✅ Instruction Execution",
            "All instructions validated successfully. Dry-run completed without errors.",
        );
        return;
    }
    let is_error = |issue: &&DryRunIssue| {
        issue.severity.eq_ignore_ascii_case("Error")
            || issue.severity.eq_ignore_ascii_case("Critical")
    };
    let errors = dry.issues.iter().filter(is_error).count();
    let warnings = dry
        .issues
        .iter()
        .filter(|i| i.severity.eq_ignore_ascii_case("Warning"))
        .count();
    view.log_lines.push(format!(
        "  Found {errors} error(s) and {warnings} warning(s)"
    ));
    let total = if errors > 0 { errors } else { warnings };
    if total == 0 {
        return;
    }
    let icon = if errors > 0 { "❌" } else { "⚠️" };
    for issue in dry
        .issues
        .iter()
        .filter(|i| {
            if errors > 0 {
                is_error(i)
            } else {
                i.severity.eq_ignore_ascii_case("Warning")
            }
        })
        .take(5)
    {
        view.log_lines
            .push(format!("    {icon} [{}] {}", issue.category, issue.message));
        let solution = solution_for_issue(issue);
        let message = if solution.is_empty() {
            issue.message.clone()
        } else {
            format!("{} — {solution}", issue.message)
        };
        view.card(
            format!(
                "{icon} {} ({})",
                issue.component_name.as_deref().unwrap_or("Unknown"),
                issue.category
            ),
            message,
        );
    }
    let overflow = if total > 5 {
        format!(" Showing first 5 of {total}.")
    } else {
        String::new()
    };
    view.card(
        format!("{icon} Instruction Execution"),
        if errors > 0 {
            format!("Dry-run validation failed with {errors} error(s).{overflow}")
        } else {
            format!("Dry-run validation passed with {warnings} warning(s).{overflow}")
        },
    );
}

/// The caller owns UTC timestamp formatting and flushing the complete page log.
pub fn format_report(
    view: &Presentation,
    report: &Report,
    generated_utc: &str,
    full_log: &str,
) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "ModSync — Validation Report\nGenerated (UTC): {generated_utc}\n\n{}\n",
        view.summary
    );
    if !view.details.trim().is_empty() {
        writeln!(text, "{}", view.details).unwrap();
    }
    writeln!(
        text,
        "\nErrors: {}  Warnings: {}  Passed checks: {}",
        report.error_count, report.warning_count, report.passed_count
    )
    .unwrap();
    if !view.cards.is_empty() {
        text.push_str("\n--- Results ---\n");
        for card in &view.cards {
            writeln!(text, "{}\n{}\n", card.title, card.message).unwrap();
        }
    }
    if !full_log.is_empty() {
        text.push_str("--- Log ---\n");
        text.push_str(full_log);
    }
    text.trim_end().to_owned()
}
