# A Patcher step's Destination

This contract covers a `Patcher` step whose Destination is not the game folder's
placeholder: none at all, `<<kotorDirectory>>/Override`, `<<modDirectory>>` or
`<<kotorDirectory>>/`. C# always runs the patcher on the game folder and only
validates the Destination; Rust ran the patcher on the Destination, and its reports
failed every such step. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Rust lines before this slice are at `d4f30d1`. Evidence:
[patcher-destination-proof](patcher-destination-proof/README.md). Tracker item:
F05-PATCHER ("each backend/platform, named INI/index, ..."); F05-PATCHER and F09 stay
partial.

## Why this gap

It changes a file outcome and a pass/fail decision. With Destination
`<<kotorDirectory>>/Override` (a natural guess for a mod that patches Override files)
Rust's install ran HoloPatcher with `--game-dir <game>/Override`, so the patch went
into a nested folder the game never reads; with `<<modDirectory>>` it patched the mod
folder as if it were the game. A Patcher step with no Destination installed correctly
from the CLI, but the desktop Validate step (the full report) failed it, so the
desktop app would not install a guide the C# app installs. Matching C# is not less
safe: the patcher then only ever targets the game folder the user picked, and a
Destination Rust cannot resolve (outside the roots, `..`) is still refused.

## Triage

**C# at `d31ccbd3`.** `Instruction.ExecuteTSLPatcherAsync` never reads the step's
Destination: the dry run's `TslPatcherVfsSimulator.SimulateInstallAsync` gets
`MainConfig.DestinationPath` (`Instruction.cs:1308-1325`), and the real run passes it
as `--game-dir` (`:1349-1356`). `ComponentValidation.ParseDestinationWithAction`
warns `Destination must be <<gameDirectory>> with 'Patcher' action, setting it now
automatically.` for an empty Destination and sets it (`ComponentValidation.cs:265-272`);
for any other text than `<<gameDirectory>>` or `<<kotorDirectory>>` (compared with
`OrdinalIgnoreCase`) it reports `'Destination' key must be either null or string
literal '<<gameDirectory>>' (legacy: '<<kotorDirectory>>') for this action. Got
'<Destination>'` and, with AttemptFixes (on by default), sets it to
`<<gameDirectory>>` (`:273-292`). The placeholder in another case
(`<<KotorDirectory>>`) passes that test and falls to the path check (`:331-366`),
where it is not replaced, so `Destination cannot be found!` (it cannot be created
either). The full pipeline's
Environment stage runs this validation with AttemptFixes first
(`Services/InstallationService.cs:1006-1008`), so the full report and `install`
(which validates first) see a fixed Destination; the basic report keeps the error or
warning. In `RunComponentValidationStage` a component with errors shows them, one
without errors but with warnings shows `WARNING: <component>: <warning>` and the stage
counts its warnings instead of a pass
(`Services/Validation/InstallationValidationPipeline.cs:128-150`, `:374-425`).

**Rust at `d4f30d1`.** `process_path` used the Destination as the patcher's game
folder when it was not empty (`src/engine.rs:3226-3230`), for the dry-run simulation
(`:3237`) and the real run (`patcher_command`, `:3247`). The archive/destination stage
of the reports resolved the Destination as a path and required the game folder
(`:1241-1249`), so an empty Destination failed with `Path must start with
<<modDirectory>> or <<kotorDirectory>>: ` and any other with `Patcher destination must
be the game directory`, ending the component's checks, in the basic and the full
report alike. Recovery (a missing Patcher Source) refused a non-game Destination too
(`src/engine/source_resolution.rs:240-248`). Plain `validate` and `install` do not run
that stage, so they ran or simulated the patcher on the Destination.

**User-visible difference** (`mods/p/TSLPatcher.exe` installs `n.tga` into `Override`):

```
Destination                  C# CLI install      Rust before                 Rust after
(none)                       game/Override       game/Override; reports fail game/Override
<<kotorDirectory>>/Override  game/Override       game/Override/Override      game/Override
<<modDirectory>>             game/Override       mods/Override               game/Override
<<kotorDirectory>>           game/Override       game/Override               game/Override
/tmp/elsewhere               game/Override       refused                     refused (kept)
```

## Behaviour after this slice

- **The patcher's game folder.** `process_path` (`src/engine.rs:3270-3278`) now always
  uses the game root, for the simulation and the real run. The `Patcher` arm
  (`:3178-3195`) still resolves a non-empty Destination as a path, so an unsafe one
  (outside the roots, `..`, an unknown or miscased placeholder) is still refused
  before any write (fail closed), and logs `Patcher Destination '<Destination>' is not
  used; the patcher runs on the game directory` when it names another folder.
  Recovery no longer requires the game folder
  (`src/engine/source_resolution.rs:240-241`; it still resolves the Destination at
  `:215-217`).
- **The reports.** The archive/destination stage (`validate_component_archives`,
  `src/engine.rs:1256-1280`) now gives, in the basic report only, C#'s warning for an
  empty Destination and C#'s error for any text other than `<<gameDirectory>>` or
  `<<kotorDirectory>>` (ignoring case, `game_placeholder`, `:205-213`), one per step,
  and goes on; the full report (`validate --full`, the desktop Validate step) passes
  them, as C#'s Environment stage has fixed them. The placeholder in another case is
  still resolved and refused, as before (C# fails it too).
- **Warnings in that stage.** `validate_component_archives` now returns warnings with
  its errors (`ArchiveOutcome`, `src/validation.rs:162-163`), and
  `Report::component_validation` (`:277-344`) shows them as C# does: `WARNING:
  <component>: <warning>` lines for a component with no error, counted in
  `warningCount` instead of a pass, with `hasWarnings`. `simulation_failure`
  (`:345-393`), used by the report without a dry run, takes those warnings back out
  of the count when it fails the stage, and its error replaces the failing
  component's `OK:` or `WARNING:` lines, as C# lists only errors for a component with
  errors.
- So plain `validate`, `install`, `conflicts`, both reports, `--phase remaining` and
  the desktop run the patcher, or simulate it, on the game folder, and the desktop
  Validate step passes a Patcher step with no Destination.

Callers checked: `process_path` serves Patcher, Execute and Run; only Patcher read a
Destination there. `validate_component_archives` has one caller (`validate_archives`,
whose only caller is the report's ComponentValidation stage). `component_validation`
and `simulation_failure` are called only by the report. The rootless basic validation
(`src/validation/basic.rs:584-598`) already gave C#'s warning and error and is
unchanged. Managed provenance and checkpoints record what the patcher changed under
the game folder and read no Destination.

## Proof

[patcher-destination-proof](patcher-destination-proof/README.md) runs 9 cases against
the live C# CLI with a HoloPatcher stub that writes `<game-dir>/Override/n.tga`: the
full and basic dry-run reports (decision, counts and the ComponentValidation stage),
`install` (exit and final tree) and Rust's plain `validate` against C#'s install
decision, 72 checks.

| Binary | patcher-destination |
| --- | --- |
| this slice, live C# CLI | 63 match / 9 known difference / 0 fail |
| this slice, recorded references | 63 match / 9 known difference / 0 fail |
| `d4f30d1` (pre-fix), recorded references | 34 match / 8 known difference / 30 fail |

On `d4f30d1` the five fixed cases fail 27 checks and the known `unsafe_destination`
case fails three more (its stage messages and full-report counts used the old
wording); see the proof README.

## Known differences

- **A Destination Rust cannot resolve (deliberate, fail-closed).** `/tmp/elsewhere`,
  `..`, a whitespace-only Destination or an unknown placeholder: C# sets it to the game folder and installs; Rust
  still refuses the step before any write (`Path must start with <<modDirectory>> or
  <<kotorDirectory>>`), so its reports also fail the dry run (`unsafe_destination`).
  Such a Destination is a guide error, and the step never needs it.
- **The placeholder in another case (unchanged).** `<<KotorDirectory>>`: both refuse.
  C# fails its Environment stage (full report) or names the unresolved path
  (`Destination cannot be found! Got '<cwd>/<<KotorDirectory>>'`, basic report); Rust
  names the unknown placeholder and its dry run refuses the step too, so the counts
  and the stage text differ (`placeholder_case`).
- **Other C# ComponentValidation warnings (not changed).** C# also warns about
  `tslpatcher.exe` in a non-Patcher Source and about an Extract Source without the
  archive's name; the rooted Rust reports do not (the rootless basic validation does).
- **Not probed.** KPatcher and OdyPatcher (the same `patcher_command`), a missing
  Patcher Source with a non-game Destination (recovery), Windows.

## Notes for guide authors

A Patcher step needs no Destination: the patcher always runs on the game folder, in
C# and now in Rust. Write `Destination = "<<kotorDirectory>>"` or leave it out; any
other value is reported by the basic report and ignored.
