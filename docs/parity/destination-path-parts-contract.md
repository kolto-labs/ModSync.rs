# Destination path parts C# validation refuses

This contract covers a `Copy`, `Move`, `CleanList` or `DelDuplicate` step whose
Destination has a part C# `PathValidator.IsValidPath` refuses and Rust's path
resolution let through: a `.` part (`<<kotorDirectory>>/Override/.`), `..` inside a
longer name (`a..b`, `..x`) or a control character (a tab). C# validation refuses
such a guide, so C# `install` (which validates first) refuses it; Rust's plain
`validate` and `install` installed it, and for `..` inside a name or a control
character every Rust report passed it too. The reference is pinned C# `d31ccbd3`; C#
paths are under `src/ModSync.Core/` there. Rust lines before this slice are at
`a5b2053`. Evidence:
[destination-path-parts-proof](destination-path-parts-proof/README.md). F05 and F08
stay partial.

## Why this gap

[full-pipeline-destination-contract.md](full-pipeline-destination-contract.md) left
the `.` part as a known difference: both full reports fail it, but Rust's plain
`validate` and `install` resolved `Nope/.` to `Nope` and installed there, while C#
`install` refused. Rust has no `--skip-validation` (it refuses the flag by name), so
the C# install path to compare with is the one that validates first, and the Rust
report itself already refused the guide: the desktop Validate step blocked it while
`modsync install` installed it. Checking C#'s rule while confirming this found two
more parts it refuses that Rust passed everywhere, report included.

## Triage

**C# at `d31ccbd3`.** `install` runs the full pipeline (`WizardFull`) unless
`--skip-validation` is given and stops with `Validation failed. Fix issues above before
installing.` when it fails (`CLI/ModBuildConverter.cs:3576-3603`); the wizard's
Validate page runs the same preset before its install. In that pipeline
`ComponentValidation.Run` checks every instruction of the component and of all its
options, whatever their selection or instruction conditions
(`ComponentValidation.cs:246-262`). Its `default` arm (Copy, Move, CleanList,
DelDuplicate) resolves the Destination and adds `Destination cannot be found! Got
'<path>'` when `PathValidator.IsValidPath` fails (`ComponentValidation.cs:331-365`,
the test at `:342-345`); AttemptFixes cannot repair it, since it creates the folder
only for a valid path (`:349`). `IsValidPath` (`FileSystemUtils/PathValidator.cs:30-96`)
refuses, among others, a character below U+0020 (`:66-69`, `ContainsNonPrintableChars`
at `:145`) and any part that ends in `.` or a space or contains `..`
(`HasInvalidWindowsFileNameParts`, `:166-194`), so a `.` part, `a..b` and `..x`
fail. Run live: when the folder before the bad part exists, or the bad part is not
`.`, the check fails in the Environment stage, which ends the report there with one
error; for `Nope/.` (a missing folder) the Environment stage passes and the
`ComponentValidation` stage fails. Either way the report fails and `install` refuses
before any write. A DelDuplicate or CleanList into such a path also fails at
execution, as `TryGetValidDirectoryInfo` gives no folder (`Instruction.cs:395`,
`FileSystemUtils/PathHelper.cs:85-107`), and they throw (`Instruction.cs:581-584`,
`:716-719`). `install --skip-validation` (`CLI/ModBuildConverter.cs:3629`) copies and
moves into these folders.

**Rust at `a5b2053`.** `paths::relative` (`src/paths.rs:97-105`, `checked_relative`
at `:129-148`) drops a `.` part and lets through `..` inside a longer name and control
characters other than NUL. The archive stage (`validate_component_archives`,
`src/engine.rs:953-1105`) refused a Destination with a `.` part (`has_dot_part`,
`:210-214`, used at `:1052`), so the basic and full reports and the desktop Validate
step failed it, but nothing refused `a..b`, `..x` or a tab. Plain `validate`
(`Engine::validate`, `:518-524`) and the install preflight (`:1543-1549`) run the FOMOD
gate and the step simulation, not the archive stage; Copy and Move resolve the
Destination with `relative`, so they passed and `install` wrote into `Override`,
`Nope`, the game root, `a..b`, `Over..ride`, `Override/..x` or `Override/a<TAB>b`.
DelDuplicate and CleanList refused a `.` part at execution (`:2636-2643`,
`:2711-2717`), so they already failed everywhere, but ran into `a..b`.

**User-visible difference** (Copy `<<modDirectory>>/m.txt` into each Destination):

```
Destination                         C# install   Rust before            Rust after
<<kotorDirectory>>/Override/.       refused      installs (report fails) refused, no write
<<kotorDirectory>>/Nope/.           refused      installs in Nope       refused, no write
<<kotorDirectory>>/a..b (exists)    refused      installs, report passes refused, no write
<<kotorDirectory>>/Override/a<TAB>b refused      installs, report passes refused, no write
<<kotorDirectory>>/Override/.x      installs     installs               installs
```

## Behaviour after this slice

- **The rule.** `invalid_destination` (`src/engine.rs:206-216`) is true when a part
  of the authored Destination, split at `/` and `\`, is `.`, contains `..` and is not
  `..` itself (a bare `..` stays `Unsafe path`, from `relative`), or contains a
  character below U+0020. The rest of `IsValidPath` (the other invalid characters, a
  part ending in `.` or a space, a colon, device names) was already refused by
  `relative`, with its own wording.
- **Reports.** The archive stage applies the rule where it applied `has_dot_part`
  (`src/engine.rs:1119-1125`), with the same per-step message, `Destination cannot be
  found! Got '<folder>'`, so `a..b`, `..x` and control characters now fail the basic
  and full reports, `--phase remaining` and the desktop Validate step too.
- **Plain `validate` and `install`.** `destination_gate` (`src/engine.rs:546-593`)
  runs after the FOMOD gate in `Engine::validate` (`:537-544`, so also in
  `validate_remaining` and the desktop's validate job) and in the install preflight
  before any write (`:1613-1614`, context `Installation blocked before any write`;
  this covers `install_remaining`, managed and checkpoint installs and the desktop
  install). It checks the same steps as the archive stage: every Copy, Move, CleanList
  and DelDuplicate step of each selected, not yet completed component and of each of
  its immediate options, selected or not, whose instruction conditions are met
  (`applies_with`, `:226-237`, now also behind `instruction_applies`, `:2053-2055`). It
  stops at the first one with `<component> / step <n> (<action>): Destination is not a
  valid path (a `.` part, `..` inside a name or a control character): <Destination>`
  (`<component> / <option> / step <n>` for an option's step). A Destination that does
  not resolve at all is left to the simulation, which reports it as before.
- **Execution.** DelDuplicate and CleanList refuse the same parts at execution
  (`src/engine.rs:2699-2706`, `:2774-2781`), as C# fails them, so the dry run reports
  an error for `a..b` as it did for a `.` part. Copy and Move execution is unchanged:
  the dry run does not fail them, as C#'s does not (`Nope/.` passes C#'s DryRun stage).

Callers checked. `Engine::validate` has three callers: plain `validate` in the CLI
(`src/main.rs:3379`), `validate_remaining` (`src/engine.rs:1765-1773`; CLI `--phase
remaining`) and the desktop validate job (`src/ui.rs:4181`). The install preflight is
reached by every install entry point (`install`, `install_observed`,
`install_remaining*`, the desktop's `install_observed_report`). The report path
(`validate_report_with_options_observed`) runs the archive stage and is otherwise
unchanged. `analyze_conflicts` (the `conflicts` command) simulates writes without the
FOMOD gate or this one, as before; it is an analysis, not an install gate. The
archive-stage and execution checks are the only other uses of the rule.

## Proof

[probe.py](destination-path-parts-proof/probe.py) runs 25 cases (4 controls, 12 with
a `.` part, 5 with `..` inside a name, 2 with a tab, and the DelDuplicate and CleanList
`.` cases that already failed) through the full report with and without a dry run,
the basic dry-run report, `install` and, for Rust, plain `validate`, and compares them
with the live C# CLI with a HoloPatcher stub, plus Rust `install` against C# `install
--skip-validation` (200 checks).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 138 match / 62 known difference / 0 fail |
| this slice, recorded reference | 138 match / 62 known difference / 0 fail |
| `a5b2053` (pre-fix), recorded reference | 82 match / 39 known difference / 79 fail |

On `a5b2053` the 11 Copy and Move `.` cases C# refuses (all but the unmet-condition
one) installed and passed plain `validate` (three failures each: `install`,
`install_skip`, `plain`), and the seven `..` and tab cases also passed every report
(six or seven each, 46). The controls, the unmet-condition case and the DelDuplicate
and CleanList `.` cases gave the same results on both builds.

## Known differences

- **The report stops in a different stage (predates this slice).** Where C# fails the
  part in its Environment stage, its report ends there (`errorCount` 1, no passed
  stages); Rust's Environment stage runs no component checks, so it passes and the
  `ComponentValidation` stage fails, and `passedCount` differs. The decision matches.
  As recorded for other destination failures in
  [full-pipeline-destination-contract.md](full-pipeline-destination-contract.md).
  19 cases, 38 checks (`full_dry/counts` and `full_dry/stages`).
- **The folder shown.** Rust's archive-stage message names the resolved folder
  without a `.` part (`<root>/game/Nope`, C# `<root>/game/Nope/.`), as recorded in
  [missing-destination-contract.md](missing-destination-contract.md). One check
  (`dot_missing`), where C#'s report reaches that stage.
- **`install --skip-validation` (deliberate).** C# copies and moves into these folders
  when validation is skipped; Rust has no such flag and its install refuses. 16 checks.
  The [root-destination proof](root-destination-proof/README.md) compares Rust's
  install with C# `ModComponent.InstallAsync` alone, which installs too; its three
  `.` Copy/Move cases now count Rust's refusal as a known difference.
- **Steps whose instruction conditions are not met.** C# checks every step; Rust's
  archive stage, plain `validate` and `install` leave out a step whose Dependencies or
  Restrictions exclude it, as they already did for every other Destination check. Such
  a step never runs, so nothing is written there; C# refuses the guide. One case,
  seven checks.
- **The root folders.** C# applies `IsValidPath` to the whole resolved path, so a
  game or mod folder whose own path has such a part fails every Copy into it; Rust
  checks only the authored part after the placeholder. Not probed.
- **Other actions.** C# checks a Patcher Destination against the game folder only and
  a Rename Destination only for one exact `Override` spelling, and Rust's checks of
  those are unchanged. Not probed.
- **Platform.** The reference ran on Linux under `unshare -Urnm` with a tmpfs over
  `/Override`.
