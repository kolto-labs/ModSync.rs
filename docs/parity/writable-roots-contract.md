# Read-only game or mod folder in the full report and install

This contract covers the game and mod folder checks of C#'s Environment stage: C#
fails `validate --full` (and `--full --dry-run-only`) when it cannot write to the
game folder or the mod folder, and its `install` refuses before installing; Rust did
not check, so the full report passed (exit 0) and `install` took its recovery
snapshot, then stopped at the first write and left that snapshot to restore. The
reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Rust lines before this slice are at `2de5a47`. Evidence:
[writable-roots-proof](writable-roots-proof/README.md). Tracker item: F08 ("remaining
backend classes, exact reports ..."); F08 stays partial.

## Why this gap

It changes a pass/fail decision, and Rust was the less safe side: the full report
said `Validation passed` for a game folder on a read-only mount (or one the user
cannot write to), and `install` then started, created a recovery snapshot and failed
on its first file, so the user had to restore before retrying. The C# CLI fails the
report and refuses the install before anything is written. Matching C# only adds a
refusal; the check itself writes nothing.

Candidates looked at and skipped:

- **Case-insensitive duplicates in the game folder** (C#'s last Environment check,
  `Services/InstallationService.cs:935-942`, `:1059-1065`). Live, C# fails `validate
  --full` with `You have duplicate files/folders in your installation directory ...`;
  Rust already refuses the same trees (`Case-colliding files: <path>`, its engine scan,
  `src/engine.rs:576-626` after this slice), only without a report. Both fail. Rust
  also refuses a case duplicate in the mod folder, which C# passes (fail closed). Not
  changed.
- **Restrictions and Dependencies** in C#'s Environment stage: Rust's Conflicts and
  InstallOrder stages fail the same guides (documented in
  [dry-run-only-environment-contract.md](dry-run-only-environment-contract.md)).
- **The patcher test run** (`holopatcher --install`, exit code 2): Rust validation
  runs no external program (AGENTS.md); deliberate.

## Triage

**C# at `d31ccbd3`.** `validate --full` runs the Environment stage first
(`Services/Validation/InstallationValidationPipeline.cs:66-82`), which calls
`InstallationService.ValidateInstallationEnvironmentAsync` (`:270-289`). That checks
both folders with `UtilityHelper.IsDirectoryWritable` (`Services/InstallationService.cs:946-948`),
which creates a randomly named file with `DeleteOnClose` in the folder and returns
false on `UnauthorizedAccessException` or `IOException` (`Utility/Utility.cs:156-195`).
With a confirmation callback it offers to fix the permissions (`:949-966`); `validate`
passes none (`CLI/ModBuildConverter.cs:3205-3213`), `install` does
(`:3576-3598`, `BuildInstallConfirmationCallback`). An unwritable folder sets the
message `The Install directory is not writable! Please ensure administrative
privileges or reinstall KOTOR to a directory with write access.` or `The Mod directory
is not writable! Please ensure administrative privileges or choose a new mod
directory.` (`InstallationService.cs:1029-1043`). Each later problem replaces the
message, so the mod folder's wins over the game folder's, and the components'
`Some components failed to validate. ...` (`:1051-1057`) wins over both. The stage
fails with that one message and the report counts one error and stops.
`install` runs this pipeline before installing and returns 1 when it fails
(`ModBuildConverter.cs:3576-3598`).

**Rust at `2de5a47`.** The full report's Environment stage checked only the recovery
state and, under `--dry-run-only`, the component checks (`src/engine.rs:884-901`). Its
install preflight (`:1896-1983`) never looked at the folders, so `install` created
its snapshot (`recovery::begin`, `:1995`) and failed at the first write: `Install
stopped. Restore before retrying; snapshot: ...: A / step 1 (Copy): Read-only file
system (os error 30)`. A guide that only reads from a read-only mod folder installed.

**User-visible difference** (folders bind-mounted read-only):

```
Case                         C# validate --full   Rust before   Rust after
game folder read-only        fails (Environment)  passes        fails (Environment)
mod folder read-only         fails (Environment)  passes        fails (Environment)
both read-only               fails, mod message   passes        fails, mod message
game/Override read-only      passes               passes        passes

Case                         Rust install before                 Rust install after
game folder read-only        snapshot, stops at the first write  refused, nothing written
mod folder read-only         installs (reads only)               refused, nothing written
```

## Behaviour after this slice

- **The full report.** The Environment stage (`src/engine.rs:926-942`) now fails with
  C#'s message when the mod folder, else the game folder, cannot be written to
  (`unwritable_root`, `:424-453`), after the recovery-state check. Under
  `--dry-run-only` it also runs the component checks first, and their message wins,
  as in C# (`Report::environment_components`, `src/validation.rs:281-313`). The stage
  counts one error and the report stops, as in C#. The basic report (no `--full`) and
  plain `validate` are unchanged, as C# does not run its Environment stage there.
- **Install.** The install preflight (`src/engine.rs:1942-1946`) refuses the same
  folders right after the recovery-state check, before the gates, the simulation and
  the snapshot: `Installation blocked before any write: <C#'s message>`. Rust does not
  offer to change the folder's permissions (C# `install` asks, then tries chmod and
  chown, and may run sudo); deliberate.
- **How Rust checks.** On Unix it asks the operating system (`access(2)` with write
  and search permission), which also reports a read-only mount, so validation writes
  no probe file. Windows is not checked (see below).
- **Callers.** `validate --full` (with or without `--dry-run`/`--dry-run-only`), the
  `--phase remaining` report and the desktop Validate step use the full report
  (`src/ui.rs:4148-4155`), as C#'s GUI validation runs the same Environment check.
  `install` (also `--phase remaining`, managed and checkpoint installs) and the desktop
  install share `install_observed_report`.

## Proof

[writable-roots-proof](writable-roots-proof/README.md) runs 6 cases, each with its
folders bind-mounted read-only under `unshare -Urnm` on both sides, against the live
C# CLI with a HoloPatcher stub: `validate --output json --full` (decision, counts,
stages, the Environment message), the same in text output (decision), `--full
--dry-run-only` (decision, counts, Environment message), the lines Rust lists after
that message, and a Rust-only `install` check (exit, error line, Override listing,
an unfinished install left behind), 59 checks.

| Binary | writable-roots |
| --- | --- |
| this slice, live C# CLI | 58 match / 1 known difference / 0 fail |
| this slice, recorded references | 58 match / 1 known difference / 0 fail |
| `2de5a47` (pre-fix), recorded references | 28 match / 0 known difference / 31 fail |

On `2de5a47` the three read-only cases fail all eight report checks and `install`
(24 + 3); `game_readonly_missing_archive` fails three report checks (its full report
failed later, in ComponentValidation) and `install` (Rust refused the missing
archive, not the folder). The controls are the same on both binaries.

## Known differences

- **A component error and a read-only folder in the full report.**
  `game_readonly_missing_archive` (`full/environment`): C#'s Environment stage also
  runs the component checks, so their message replaces the read-only one. Rust's
  full report runs them later, in its ComponentValidation stage, so its Environment
  stage gives the read-only message. Both fail the Environment stage with one error;
  under `--dry-run-only` both give the component message.
- **Install asks no question.** C# `install` offers to fix the permissions and, with
  `--yes`, tries chmod, chown and sudo on the folder; Rust refuses and leaves the
  permissions alone (and refuses `--yes`, as before).
- **Windows is not checked.** C# creates a probe file there too; Rust passes the
  folders, and an install stops at its first write as before.
- **How the folder is tested.** C# creates and deletes a file; Rust asks the kernel.
  They can differ where permission is decided by something `access(2)` does not see
  (for example a network file system that refuses the create).

## Notes for guide authors

Nothing changes for guides. Users whose game folder needs administrator rights (for
example under `Program Files` on a shared machine) must fix its permissions, or move
the game, before Rust validates or installs, as with the C# app.
