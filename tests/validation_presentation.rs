use modsync::validation::{
    DryRun, DryRunIssue,
    presentation::{first_issue_log, solution_for_issue},
};
use modsync::validation::{
    Report, Stage, StageResult,
    presentation::{format_report, project},
};

fn stage(kind: Stage, passed: bool, warned: bool, messages: &[&str]) -> StageResult {
    StageResult {
        stage: kind,
        passed,
        has_warnings: warned,
        summary: "Summary".into(),
        messages: messages.iter().map(|s| (*s).into()).collect(),
    }
}
fn issue(severity: &str, category: &str, message: &str) -> DryRunIssue {
    DryRunIssue {
        severity: severity.into(),
        category: category.into(),
        message: message.into(),
        affected_path: None,
        component_name: None,
        component_guid: None,
        instruction_index: None,
        solution: None,
    }
}

#[test]
fn prefixes_preserve_whole_detail_and_suppress_duplicate_environment_failure() {
    let mut report = Report::new(3);
    report.warning_count = 1;
    report.stages = vec![stage(
        Stage::Environment,
        false,
        true,
        &[
            "WARNING: Alpha: caution",
            "ERROR: Beta: broken",
            "error: ignored",
            "ERROR: no colon",
            "ERROR: : leading",
        ],
    )];
    let view = project(&report, 3);
    assert_eq!(
        view.cards
            .iter()
            .map(|c| (c.title.as_str(), c.message.as_str()))
            .collect::<Vec<_>>(),
        [
            ("⚠️ Alpha", "Alpha: caution"),
            ("❌ Beta", "Beta: broken"),
            ("❌ Unknown", "no colon"),
            ("❌ Unknown", ": leading")
        ]
    );
    assert_eq!(view.first_issue_card(1), Some(1));
    assert_eq!(first_issue_log(&view.log_lines.join("\n"), 1), Some(2));
    report.stages[0].messages.clear();
    assert_eq!(project(&report, 3).cards[0].title, "❌ Environment Error");
}

#[test]
fn aggregates_and_fallback_logs_follow_stage_order() {
    let mut report = Report::new(2);
    report.stages = vec![
        stage(Stage::Environment, true, false, &[]),
        stage(Stage::Conflicts, true, false, &[]),
        stage(Stage::InstallOrder, false, false, &[]),
        stage(
            Stage::ComponentValidation,
            true,
            true,
            &["WARNING: A: archive"],
        ),
        stage(Stage::FomodConfiguration, true, true, &[]),
    ];
    let view = project(&report, 2);
    assert_eq!(
        view.cards
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        [
            "✅ Environment",
            "✅ Conflicts",
            "❌ Install Order",
            "⚠️ A",
            "⚠️ Archive Validation",
            "✅ FOMOD Configuration"
        ]
    );
    assert_eq!(
        view.log_lines,
        [
            "Step 1: Validating installation environment",
            "  ✅ Environment validation passed",
            "Step 2: Checking conflicts for 2 selected mod(s)",
            "  ✅ No conflicts",
            "Step 3: Validating mod installation order",
            "  ❌ Summary",
            "Step 4: Validating mod archives",
            "  WARNING: A: archive",
            "Step 5: Checking FOMOD configuration"
        ]
    );
    for kind in [
        Stage::Conflicts,
        Stage::InstallOrder,
        Stage::ComponentValidation,
        Stage::FomodConfiguration,
    ] {
        report.stages = vec![stage(kind, false, false, &[])];
        assert!(project(&report, 2).cards[0].title.starts_with("❌"));
    }
}

#[test]
fn dry_run_missing_clean_informational_and_capped_severity() {
    let mut report = Report::new(1);
    report.stages = vec![stage(Stage::DryRun, true, false, &[])];
    assert!(project(&report, 1).cards.is_empty());
    report.dry_run = Some(DryRun {
        is_valid: true,
        has_warnings: false,
        issues: vec![],
    });
    assert_eq!(
        project(&report, 1).cards[0].message,
        "All instructions validated successfully. Dry-run completed without errors."
    );
    for errors in [false, true] {
        let mut issues = vec![issue("Warning", "CopyFile", "file does not exist"); 7];
        if errors {
            issues.extend(vec![issue("Critical", "UnknownCategory", "broken"); 6]);
        }
        report.dry_run = Some(DryRun {
            is_valid: !errors,
            has_warnings: true,
            issues,
        });
        let view = project(&report, 1);
        assert_eq!(view.cards.len(), 6);
        assert_eq!(
            view.cards[5].message,
            if errors {
                "Dry-run validation failed with 6 error(s). Showing first 5 of 6."
            } else {
                "Dry-run validation passed with 7 warning(s). Showing first 5 of 7."
            }
        );
        assert_eq!(
            view.cards[0].title,
            if errors {
                "❌ Unknown (UnknownCategory)"
            } else {
                "⚠️ Unknown (CopyFile)"
            }
        );
        assert_eq!(
            view.log_lines[1],
            if errors {
                "  Found 6 error(s) and 7 warning(s)"
            } else {
                "  Found 0 error(s) and 7 warning(s)"
            }
        );
    }
    report.dry_run = Some(DryRun {
        is_valid: false,
        has_warnings: true,
        issues: vec![issue("Information", "", "info")],
    });
    assert!(project(&report, 1).cards.is_empty());
}

#[test]
fn solution_rules_are_exact_and_do_not_guess_engine_categories() {
    for category in ["ArchiveValidation", "ExtractArchive"] {
        assert_eq!(
            solution_for_issue(&issue("Error", category, "")),
            "Verify the archive exists and is not corrupted. Try re-downloading from the mod link."
        );
    }
    for category in ["MoveFile", "CopyFile"] {
        assert_eq!(
            solution_for_issue(&issue("Error", category, "a does not exist")),
            "The required file is missing. This may indicate an incomplete mod archive or incorrect source path."
        );
        assert_eq!(
            solution_for_issue(&issue("Error", category, "a already exists")),
            "File conflict detected. This may be expected - ensure mod installation order is correct."
        );
        assert_eq!(
            solution_for_issue(&issue("Error", category, "DOES NOT EXIST")),
            ""
        );
    }
    assert_eq!(
        solution_for_issue(&issue("Error", "DeleteFile", "")),
        "File does not exist to delete. This may indicate incorrect instruction order."
    );
    assert_eq!(
        solution_for_issue(&issue("Error", "ExecuteProcess", "")),
        "The required executable is missing. Verify the mod archive was extracted correctly."
    );
    assert_eq!(
        solution_for_issue(&issue("Error", "EngineSimulation", "a does not exist")),
        ""
    );
    let mut custom = issue("Error", "CopyFile", "does not exist");
    custom.solution = Some("Explicit solution".into());
    assert_eq!(solution_for_issue(&custom), "Explicit solution");
}

#[test]
fn exact_human_report_uses_aggregate_counts_and_full_flushed_log() {
    let mut report = Report::new(9);
    report.error_count = 7;
    report.warning_count = 2;
    report.passed_count = 4;
    report.stages = vec![stage(Stage::Environment, false, false, &[])];
    let view = project(&report, 9);
    assert_eq!(
        format_report(
            &view,
            &report,
            "2026-09-27 12:34:56",
            "Starting validation...\nERROR: full log\n"
        ),
        "ModSync — Validation Report\nGenerated (UTC): 2026-09-27 12:34:56\n\n❌ Validation Failed\nPlease resolve all critical errors before continuing. Check the results above for details.\n\nErrors: 7  Warnings: 2  Passed checks: 4\n\n--- Results ---\n❌ Environment Error\nSummary\n\n--- Log ---\nStarting validation...\nERROR: full log"
    );
    report.success = true;
    let warned = project(&report, 9);
    assert_eq!(warned.status, "Validation complete");
    assert_eq!(
        warned.details,
        "Validation passed with 2 warning(s). Review the results before continuing."
    );
    assert_eq!(first_issue_log("WARNING: a\n", 0), None);
    assert_eq!(first_issue_log("WARNING: a\n", 1), Some(0));
}

#[test]
fn clean_archive_has_log_but_no_card() {
    let mut report = Report::new(1);
    report.success = true;
    report.stages.push(StageResult {
        stage: Stage::ComponentValidation,
        passed: true,
        has_warnings: false,
        summary: "Archives valid".into(),
        messages: vec!["OK: Example".into()],
    });
    let view = project(&report, 1);
    assert!(view.cards.is_empty());
    assert_eq!(
        view.log_lines,
        ["Step 1: Validating mod archives", "  ✅ Example"]
    );
    assert_eq!(
        format_report(&view, &report, "2026-09-27 12:34:56", ""),
        "ModSync — Validation Report\nGenerated (UTC): 2026-09-27 12:34:56\n\n✅ Validation Passed\nEverything looks good! You're ready to proceed with the installation.\n\nErrors: 0  Warnings: 0  Passed checks: 0"
    );
}

#[test]
fn report_trims_only_final_whitespace_with_or_without_log() {
    use modsync::validation::presentation::{Card, Presentation};
    let report = Report::new(0);
    let view = Presentation {
        cards: vec![Card {
            title: "Card title  ".into(),
            message: "  First line \t\n\nLast line \t".into(),
        }],
        summary: "Summary".into(),
        details: "Details".into(),
        ..Presentation::default()
    };
    assert_eq!(
        format_report(&view, &report, "2026-09-27 12:34:56", ""),
        "ModSync — Validation Report\nGenerated (UTC): 2026-09-27 12:34:56\n\nSummary\nDetails\n\nErrors: 0  Warnings: 0  Passed checks: 0\n\n--- Results ---\nCard title  \n  First line \t\n\nLast line"
    );
    assert_eq!(
        format_report(
            &view,
            &report,
            "2026-09-27 12:34:56",
            "  First log line \t\n\nLast log line\n \t\r\n\u{a0}\u{2003}"
        ),
        "ModSync — Validation Report\nGenerated (UTC): 2026-09-27 12:34:56\n\nSummary\nDetails\n\nErrors: 0  Warnings: 0  Passed checks: 0\n\n--- Results ---\nCard title  \n  First line \t\n\nLast line \t\n\n--- Log ---\n  First log line \t\n\nLast log line"
    );
    assert_eq!(
        format_report(&view, &report, "2026-09-27 12:34:56", " \t\r\n"),
        "ModSync — Validation Report\nGenerated (UTC): 2026-09-27 12:34:56\n\nSummary\nDetails\n\nErrors: 0  Warnings: 0  Passed checks: 0\n\n--- Results ---\nCard title  \n  First line \t\n\nLast line \t\n\n--- Log ---"
    );
}
