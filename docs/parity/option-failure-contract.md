# A failing chosen option in the validation dry run

This contract covers the child-failure mapping of F04-C and F05-CHOOSE from the
[actions audit](engine-actions-archives-audit.md): what the validation dry run
(`validate --dry-run`, the desktop Validate step) reports when an option that a
`Choose` step runs is missing a source. The reference is pinned C# `d31ccbd3`; C#
paths are under `src/ModSync.Core/` there.
Evidence: [option-failure-proof](option-failure-proof/README.md). F04 and F05 stay
partial.

## Why this gap

Optional parts of a mod (a compatibility patch, an alternative texture set) are
options that a `Choose` step runs, and their files are often a separate download. When
one is missing, the C# dry run fails validation for that component. Rust reported the
component as skipped with a warning and passed validation, so `validate` exited 0
for a selection its own install then refused. The C# behaviour is clear in the source
and fails closed, so there is no safety reason to differ.

Candidates checked live and not chosen: an instruction whose `Dependencies` or
`Restrictions` name an option of an unselected component (C# reads that option's own
`IsSelected` flag, so it runs a step for a mod that is not installed; Rust treats the
option as unselected) and an option's own `Dependencies`/`Restrictions` (C# ignores
them when a `Choose` runs the option; Rust refuses, as it does for components). Both
are Rust choices that keep the guide's meaning; see Known differences.

## Triage at `b89484d`

**C# behaviour.** A `Choose` step runs each chosen option's instructions through
`ExecuteInstructionsAsync`; when an option returns anything but `Success`, the step
logs `Failed to install chosen option <n> in main instruction index <i>` and returns
`ActionExitCode.OptionalInstallFailed` (`ModComponent.cs:1648-1667`). The
component's instruction loop maps `OptionalInstallFailed` to
`InstallExitCode.UserCancelledInstall` before it maps `FileNotFoundPre`/`FileNotFoundPost`
to `MissingSourceFiles` (`ModComponent.cs:1857-1866`), so an option's missing source
reaches the component as `UserCancelledInstall`, not `MissingSourceFiles`.

The dry run (`Services/Validation/DryRunValidator.cs:507-547`) turns only
`MissingSourceFiles` into the `DryRunSkipped` warning `Skipped '<name>' during
dry-run: source archives or files are not available.` (`:515-527`). Any other code is
the error `Dry-run failed for '<name>' with exit code UserCancelledInstall.`
(`:535-546`), counted in `errorCount`, and `validate` exits 1. This holds for every
step that reports a missing source: Copy, Move, Extract, a strict (`Overwrite = true`)
Delete, Rename, CleanList, and Execute/Run whose Sources are all wildcards (Rust needs
`--allow-processes` to reach it). The same miss in the component's own steps stays a
`DryRunSkipped` warning. Install fails the component in both cases.

**Rust at `b89484d`.** The `Choose` branch of `Engine::execute`
(`src/engine.rs:2284-2302`) runs each chosen option with `Engine::component` and
returns its error unchanged (`:2300`). A missing source is typed
`MissingSourceFiles` (`src/engine.rs:282`), and `validate_dry_run` turns that type
into the `DryRunSkipped` warning wherever it came from (`src/engine.rs:804-809`,
`DryRunIssue::skipped` at `src/validation.rs:58`). So a missing source inside a chosen
option was reported as a skipped component, and the report passed. Plain `validate`
(without `--dry-run`) and `install` already refused the guide.

## Behaviour after this slice

- A chosen option that fails with a missing source now fails its component in the
  dry run: the `DryRun` stage reports `ERROR: [EngineSimulation] <component> / step
  <n> (Choose): <option> / step <m> (<action>): <cause>`, `errorCount` counts it and
  `validate --dry-run` exits 1, as in C#. Nested `Choose` steps keep the mapping.
- The component's earlier warnings stay in the report, as C# keeps its virtual file
  system's issues for a failed component; a skipped component still drops them.
- Unchanged: a missing source in the component's own steps is still the
  `DryRunSkipped` warning; options that are not selected, not named by a `Choose`, or
  whose step's instruction conditions are not met are not run; install and plain
  `validate` refuse as before; other option failures were already errors.

`Engine::execute`'s `Choose` branch now passes the option's error through
`OptionalInstallFailed::wrap` (`src/engine.rs:2333`, type at `:290-313`), which hides
the `MissingSourceFiles` type behind `OptionalInstallFailed` (the C# exit code's
name). The message text is unchanged. A strict (`Overwrite = true`) Delete that
matches nothing no longer adds the `[DeleteFile] Attempting to delete non-existent
file` warning before it fails (`src/engine.rs:2521-2523`): C# `Instruction.DeleteFile`
skips a missing path without touching its virtual file system (`Instruction.cs:865-879`),
and in the component's own steps the warning was already dropped with the skip, so it
showed only once the option failure kept the component's warnings. Callers checked: the type is only inspected by `validate_dry_run` and by
the Patcher source resolution (`src/engine/source_resolution.rs:164`), which checks
the error inside the Patcher step, before the `Choose` wraps it. Install, plain `validate`, `conflicts`, checkpoints and
the desktop install use the same error text and only check success, so they are
unchanged; the desktop Validate step runs the same report and now fails these guides.

## Proof

[probe.py](option-failure-proof/probe.py) runs 24 cases against the live C# CLI and
the production release binary: 22 dry-run cases with three checks each (`decision`:
exit code and `success`; `counts`: `errorCount` and `warningCount`; `dryrun`: the
`DryRun` stage messages as `skipped`, `error` or `warning`) and two install cases
(`decision`), 68 checks.

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 67 match / 1 known difference / 0 fail |
| this slice, recorded reference | 67 match / 1 known difference / 0 fail |
| `b89484d` (pre-fix), recorded reference | 30 match / 1 known difference / 37 fail |

On `b89484d` every case with a missing source in a chosen option (Copy, a Copy
wildcard, Move, a strict Delete, Rename, CleanList, Extract; a miss after a present
file in the same option; the second of two chosen options; the second of two `Choose`
steps; later steps after the `Choose`; another component that passes; two failing
components) passed validation with a `DryRunSkipped` warning where C# fails it. The
controls match on both builds: the same miss in the component's own steps (literal
and wildcard, also before a failing `Choose`), a present option, an unselected
option, a selected option no `Choose` names, a `Choose` and an option step whose
instruction conditions are not met, an unselected component, and install with and
without the miss.

## Known differences

- **Message.** C# reports `Dry-run failed for '<name>' with exit code
  UserCancelledInstall.` under `[DryRun]`; Rust names the failing step and the missing
  source under `[EngineSimulation]`, as for its other dry-run errors. The probe
  compares the kind of each message.
- **Extract in an option.** A missing archive in an option's Extract also fails Rust's
  archive stage, where C# passes it
  ([missing-archives-contract.md](missing-archives-contract.md), deliberate), so Rust
  counts two errors and C# one. The dry run itself now matches. One `counts`
  known-difference check.
- **Earlier warnings.** Rust warns when a Copy or Move with `Overwrite = false` skips
  an existing file; the C# dry run does not report it. A component that fails in an
  option now keeps such a warning in Rust. Not in the probe.
- **Options of options.** The C# TOML loader keeps no options below an option
  (checked with `convert`), so a nested `Choose` there runs nothing; Rust loads and
  runs them, and a missing source inside fails the component as above. No C#
  oracle.
- **Option conditions (not changed).** C# looks up an instruction's `Dependencies` and
  `Restrictions` among the top-level components and their immediate options, each by
  its own `IsSelected` flag (`ModComponent.cs:1934-2063`), so an option flagged
  selected under an unselected component counts as selected: a step that depends on
  it runs, a step it restricts is skipped. Rust counts an option only when its
  component is selected too. When a `Choose` runs an option, C# ignores the option's
  own `Dependencies` and `Restrictions`; Rust refuses an unselected dependency or a
  restriction conflict before any write, as for components (COMPATIBILITY.md). Checked
  live, not in the probe.
- **Install timing.** C# install runs the component's earlier steps before the
  option fails; Rust refuses before any write. Both refuse; the probe compares the
  decision.
- **Issue target.** The `DryRun` issue for a failing chosen option names the component,
  its GUID and the `Choose` step, as C# does. Before, an option's steps had retargeted
  validation at the option, so the issue named the option and its step; the follow-up
  to this slice resets the target in `validate_dry_run` (`src/engine.rs`).
