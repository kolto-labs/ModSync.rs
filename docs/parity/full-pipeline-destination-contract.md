# Missing destination folders in the full validation pipeline

This contract covers what the full validation pipeline (`validate --full`, `validate
--full --dry-run`, the desktop Validate step, the `--phase remaining` report) decides
for a `Copy`, `Move`, `CleanList` or `DelDuplicate` step whose Destination folder does
not exist yet, and how that agrees with plain `validate` and `install`. It follows
[missing-destination-contract.md](missing-destination-contract.md), which compared the
basic report (`validate --output json --dry-run`) only. The reference is pinned C#
`d31ccbd3`; C# paths are under `src/ModSync.Core/` there unless they start with
`src/ModSync.GUI/`. Evidence:
[full-pipeline-destination-proof](full-pipeline-destination-proof/README.md). F08
stays partial.

## Why this gap

The previous two slices left this lead open: C# fails every basic `validate` of a
guide that copies into a missing folder and passes `install -y --skip-validation`,
which creates the folder; Rust's report failed it while Rust's plain `validate` and
`install` passed it. The first plan was to make plain `validate` and the install
preflight refuse too. Run live, C# says otherwise. With a HoloPatcher in its
`Resources` folder (the full pipeline needs one; the probe adds a stub), C#
`install` *without* `--skip-validation` validates first, passes and installs the
file, and `validate --full` and `validate --full --dry-run` pass. Only the basic
report, which has no Environment stage, refuses. So Rust's plain `validate` and
`install` already agreed with C#; the surface that differed was Rust's full report,
and with it the desktop Validate step. That step gates the desktop install, so a
guide that copies into a folder the game does not have yet (a fresh install without
`Override`, a new subfolder) could not be installed from the desktop app, while the
C# wizard validates and installs it. Making plain `validate` and `install` refuse
would have copied the basic report's refusal into the install path and blocked
guides C# installs.

## Triage

**C# at `d31ccbd3`.** `install` runs `ValidationPipelineOptions.WizardFull` (full
validation plus a dry run, `Services/Validation/ValidationPipelineOptions.cs:53-58`)
unless `--skip-validation` is given, and stops with `Validation failed. Fix issues
above before installing.` when it fails (`CLI/ModBuildConverter.cs:3576-3603`). The
wizard's Validate page runs the same preset
(`src/ModSync.GUI/Dialogs/WizardPages/ValidatePage.axaml.cs:367-376`). In that
pipeline the Environment stage runs first and ends the run when it fails
(`Services/Validation/InstallationValidationPipeline.cs:66-84`, `:270-291`). It calls
`InstallationService.ValidateInstallationEnvironmentAsync`
(`Services/InstallationService.cs:794-1072`), which needs a HoloPatcher in
`Resources` (`:875-881`) and then runs `ComponentValidation.Run` for every selected
component (`:967-1010`). AttemptFixes is on by default in the CLI
(`CLI/ModBuildConverter.cs:323`) and the GUI
(`src/ModSync.GUI/Models/AppSettings.cs:39`). In `ParseDestinationWithAction`'s
`default` arm (Copy, Move, CleanList, DelDuplicate) a missing folder adds `Destination
cannot be found!`, and AttemptFixes then creates the folder with
`Directory.CreateDirectory` and sets `success = true` (`ComponentValidation.cs:331-365`).
So the Environment stage passes, the `ComponentValidation` stage that follows finds the
folder and prints `OK: <component>`, and the dry run passes. The error added in the
Environment stage never reaches the report. When the folder cannot be created (an
empty Destination, or a file in the way) `Run` returns false and the Environment
stage fails with `Some components failed to validate...` (`:1051-1056`); a missing
archive does the same, because `VerifyExtractPaths` fails before the destination
checks. A `.` part still passes the Environment stage but fails `ComponentValidation`
(`Got '<path>/.'`). `install --skip-validation` runs `ComponentValidation.Run` per
component too (`Services/InstallationService.cs:1101-1111`), which creates the
folder the same way.

The basic report (`validate --dry-run` without `--full`) has no Environment stage, so
its `ComponentValidation` stage reports the folder (and creates it; a second run
passes), as [missing-destination-contract.md](missing-destination-contract.md)
records.

**Rust at `38c6ca9`.** Every report mode runs the same archive stage:
`validate_report_with_options_observed` calls `validate_archives(guide, &mut emit)`
(`src/engine.rs:708-715`), and `validate_component_archives`
(`src/engine.rs:945`) adds `Destination cannot be found! Got '<path>'` for every
Copy, Move, CleanList or DelDuplicate step whose folder is not on disk
(`:1031-1042`), with or without `full_validation`. Plain `validate`
(`src/engine.rs:518-524`) and the install preflight (`self.run`, `:1527`) simulate the
steps instead, and a virtual Copy, Move or CleanList into a missing folder succeeds,
as the real step does.

**User-visible difference** (`validate --output json --full --dry-run`, Copy into
`<<kotorDirectory>>/Nope`):

```
C#    ComponentValidation: OK: A                                success true, exit 0
      (game/Nope now exists; install without --skip-validation installs Nope/m.txt)
Rust  ComponentValidation: ERROR: A: Destination cannot be found! Got '<root>/game/Nope'
                                                                success false, exit 1
      (plain validate and install pass and install Nope/m.txt)
```

## Behaviour after this slice

- In the full pipeline (`full_validation`: `validate --full`, `--full --dry-run`, the
  desktop Validate step, `--phase remaining`), the archive stage no longer reports a
  missing Copy, Move or CleanList Destination folder that could be created: no `.`
  part, and no file at the folder or above it (`src/engine.rs:1043-1057`,
  `folder_creatable` at `:1107-1112`). The stage prints `OK: <component>`, as C# does
  after its Environment stage has created the folder. Rust still creates nothing
  while validating: the dry run and the step simulation of the report without a dry
  run create the folder virtually when they execute the step, so a later step into
  it, an Extract into it or a second component into it passes too, and install
  creates it. `validate_archives` takes `full_validation` from the report
  (`:711`, `:901-907`), and `validate_component_archives` passes it on
  (`:921`, `:953-957`).
- Unchanged: the basic report (`validate --output json` with or without `--dry-run`,
  no `--full`) still reports every missing folder, as C#'s basic report does. A
  DelDuplicate into a missing folder is still reported in every mode, since Rust's
  DelDuplicate needs the folder (Known differences). An empty Destination, a `.`
  part, a file in the way, a missing archive and every other destination failure are
  still errors. Plain `validate`, `install`, `conflicts` and the install preflight are
  unchanged; they already passed these guides, as C# install does.

Callers checked: the archive stage has one caller, the report pipeline
(`src/engine.rs:708-715`). It is reached from the CLI with `args.full`
(`src/main.rs:3363-3369`), from `validate_report_remaining_observed` with full
validation always on (`src/engine.rs:1716-1725`, the CLI `--phase remaining` at
`src/main.rs:3361` and the desktop checkpoint/phase resume), and from the desktop
Validate step with full validation always on (`src/ui.rs:4149-4155`). The
`validate_report` and `validate_report_with_dry_run` wrappers (full validation on)
have no production caller outside `src/engine.rs`. Plain `validate`, the install
preflight (`src/engine.rs:1549`), `conflicts` and checkpoint resume do not run the
archive stage. The basic validator without roots
([basic-validation-without-roots.md](basic-validation-without-roots.md)) is
unchanged.

## Proof

[probe.py](full-pipeline-destination-proof/probe.py) runs 19 cases against the live
C# CLI (a copy of the reference build with a HoloPatcher stub) and the production
release binary: `validate --full --dry-run` (decision, counts, stages), `validate
--full` (decision, counts), `install` without `--skip-validation` on the C# side
(decision and the files left), Rust's plain `validate` against its full report
(modes), and the basic `--dry-run` decision for five cases, 138 checks.

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 120 match / 18 known difference / 0 fail |
| this slice, recorded reference | 120 match / 18 known difference / 0 fail |
| `38c6ca9` (pre-fix), recorded reference | 42 match / 18 known difference / 78 fail |

On `38c6ca9` each of the 13 cases with a creatable missing Copy, Move or CleanList folder
(a Copy into `Nope`, into a missing
`Override`, nested, under a differently cased parent, into the game or mod root's
`Nope`, a Move, a CleanList, two steps or two components into one folder, an Extract
then a Copy into a new folder, a chosen option's and an unselected option's Copy)
failed six checks: the full report's decision and counts with and without a dry run,
its stages, and plain `validate` deciding differently from the report. Install
matched on both builds. The basic `--dry-run` cases fail in both, on both builds.

## Known differences

- **DelDuplicate into a missing folder (deliberate, predates this slice).** C#
  creates the folder in its Environment stage, so the full report, the dry run and
  `install` pass; Rust's DelDuplicate needs the folder, so its full report, plain
  `validate` and `install` refuse the guide before any write. One case, six checks.
- **Other destination failures fail in a different stage.** An empty Destination, a
  file in the way of the folder and a missing archive fail C#'s Environment stage
  (`Some components failed to validate...`), which ends C#'s report there; Rust's
  Environment stage passes and its archive stage (and dry run) reports them, so
  `errorCount`, `passedCount` and the stages differ. The decision matches. Three
  cases, nine checks.
- **A `.` part.** Both full reports fail it; Rust prints the folder without the `.`
  (as recorded in [missing-destination-contract.md](missing-destination-contract.md)).
  C#'s install validates first and refuses it; Rust's plain `validate` and `install`
  resolve `Nope/.` to `Nope` and install there, so they decide differently from the
  Rust report. Left as is: the path is safe and the same folder. Three checks.
- **C# creates folders while validating (deliberate).** Rust validation never
  changes the game or mod folders. After a C# full validation the folders exist on
  disk even if the user never installs, including the folder of an unselected
  option's step; Rust leaves no trace. The probe compares files, not empty folders.
- **The C# Environment stage needs a patcher.** Without a HoloPatcher (or a selected
  KPatcher/OdyPatcher) in C#'s `Resources` folder the full pipeline fails its
  Environment stage for every guide, so C# `install` without `--skip-validation`
  refuses everything there. The probe adds a stub that exits 2 to `--install`, as the
  stage expects; Rust's Environment stage does not run a patcher.
- **Destinations into a Peer staging folder or with a device name.** A creatable
  folder no longer passes the full report when it lies inside a Peer download staging
  folder (`<<modDirectory>>/.peer-<nonce>.part/`) or has a Windows device name such as
  `COM¹`; every mode refuses those paths before any write, where C# creates and installs
  them. See [reserved-staging-paths-contract.md](reserved-staging-paths-contract.md).
- **Platform.** The reference ran on Linux with `DOTNET_ROLL_FORWARD=Major` under
  `unshare -Urnm`, with a tmpfs over `/Override`. Windows paths are not probed.
