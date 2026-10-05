# Component checks in `validate --full --dry-run-only`

This contract covers `validate --full --dry-run-only`: the full pipeline without its
per-component archive stage. C# still checks every selected component in that
pipeline's Environment stage and fails the report when one has an error; Rust's
Environment stage did not, so it passed (exit 0) a guide whose archive was missing.
The reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Rust lines before this slice are at `72ed369`. Evidence:
[dry-run-only-environment-proof](dry-run-only-environment-proof/README.md). Tracker
item: F08 ("remaining backend classes, exact reports ..."); F08 stays partial.

## Why this gap

It changes a pass/fail decision, and Rust was the less safe side: `validate --full
--dry-run-only` reported `Validation passed` for a guide whose download is missing or
whose loose Source no archive holds, while the C# CLI fails it. Matching C# only
adds a refusal; nothing new is written or run.

## Triage

**C# at `d31ccbd3`.** `--dry-run-only` (`CLI/ModBuildConverter.cs:591-592`) is passed
to the pipeline as `DryRunOnly` (`:3205-3213`), which skips the ComponentValidation
stage (`Services/Validation/InstallationValidationPipeline.cs:127-150`). With `--full`
the Environment stage still runs first (`:66-82`): `RunEnvironmentStageAsync`
(`:270-289`) calls `InstallationService.ValidateInstallationEnvironmentAsync`, which
runs `ComponentValidation` (AttemptFixes on) on every selected component
(`Services/InstallationService.cs:967-1012`) and, when one fails, returns `Some
components failed to validate. Check the output/console window for details.` and a
second line about antivirus or missing mods (`:1051-1056`). The stage then fails
with that one message, and the report counts one error and stops
(`InstallationValidationPipeline.cs:75-81`). The component's own errors go only to
the log.

**Rust at `72ed369`.** The Environment stage checked only the recovery state
(`src/engine.rs:879-890`). The archive checks run in the ComponentValidation stage,
skipped under `--dry-run-only` (`:929-936`), and the dry run skips a component whose
archive is missing with a `DryRunSkipped` warning. So a missing archive, or a loose
Source no archive holds, passed the report with one warning; a Destination below a
file or an empty one failed only later, in the dry run.

**User-visible difference** (`validate --output json --full --dry-run-only`):

```
Case                              C# CLI                 Rust before            Rust after
Extract of a missing archive      fails (Environment)    passes, 1 warning      fails (Environment)
Copy of a loose file no archive   fails (Environment)    passes, 1 warning      fails (Environment)
  holds
Copy into OV/g.txt/sub (a file)   fails (Environment)    fails (DryRun)         fails (Environment)
Copy with an empty Destination    fails (Environment)    fails (DryRun)         fails (Environment)
```

## Behaviour after this slice

- **The Environment stage.** With `--dry-run-only`, the full report's Environment
  stage (`src/engine.rs:879-902`) now runs the archive/destination checks the full
  report runs in its ComponentValidation stage (`validate_archives` with the full
  pipeline's fixes, `:1135`) on every selected component, after the recovery-state
  check. When one has an error, `Report::environment_components`
  (`src/validation.rs:277-300`) fails the stage with C#'s message, counts one error,
  and the report stops, as C# does. Rust lists each component's errors after that
  message (`ERROR: <component>: <error>`); C# writes them only to its log.
- Warnings do not fail the stage, as in C#. The report without `--full` or without
  `--dry-run-only`, the desktop Validate step and `--phase remaining` (which refuses
  `--dry-run-only`) are unchanged.

Callers checked: only `validate` passes `dry_run_only` (`src/main.rs:3377-3383`); the
desktop and `validate_report` pass `false`. `validate_archives` already served the
ComponentValidation stage, which does not run under `--dry-run-only`, so it still runs
once per report. `environment_components` has one caller.

## Proof

[dry-run-only-environment-proof](dry-run-only-environment-proof/README.md) runs 13
cases against the live C# CLI with a HoloPatcher stub: `validate --output json --full
--dry-run-only` (decision, counts, stages, the Environment stage's message), the same
in text output (decision), `--dry-run-only` without `--full` (decision and counts),
and the lines Rust lists after C#'s message, 104 checks.

| Binary | dry-run-only-environment |
| --- | --- |
| this slice, live C# CLI | 99 match / 5 known difference / 0 fail |
| this slice, recorded references | 99 match / 5 known difference / 0 fail |
| `72ed369` (pre-fix), recorded references | 66 match / 2 known difference / 36 fail |

On `72ed369` the six fixed cases fail 32 checks: the four that passed fail their
decision, text decision, counts, stages, Environment message and `details`; the two
that failed in the dry run fail all but the decisions. `missing_archive_option` fails
four more: it matched C# there (the probe fails a known difference that shows no
difference) and had no `details` line; it now fails earlier (see below).

## Known differences

- **An option's missing archive (fail-closed).** `missing_archive_option`: a chosen
  option extracts a missing archive. C#'s Environment stage checks only the
  component's own steps and passes; its dry run then fails the option. Rust's checks
  include options, as its full report's ComponentValidation stage already does, so
  its Environment stage fails it. Both fail, with one error; the stages differ.
- **A Destination below a file without `--full` (unchanged).** `destination_file`:
  C#'s `--dry-run-only` report (no `--full`) passes a Copy into `OV/g.txt/sub`, its
  virtual dry run copying below a file; Rust's dry run refuses it (`Not a directory`).
- **The component lines.** Rust adds `ERROR: <component>: <error>` lines after C#'s
  message (checked separately as `details`); C# writes those errors to its log only.
- **Not changed.** C#'s Environment stage also fails a selected component whose
  Restrictions name a selected component or whose Dependencies are not selected;
  Rust's Conflicts and InstallOrder stages and its dry run fail those instead (as in
  the full report). C#'s other Environment checks (patcher test, writable folders,
  case-insensitive duplicates) are not compared.

## Notes for guide authors

`validate --full --dry-run-only` now refuses a guide whose downloads are missing, as
the C# CLI does; fetch the archives, or use `--dry-run-only` without `--full` to
check the steps alone.
