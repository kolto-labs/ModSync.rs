# Step simulation in the validation report without a dry run

This contract covers the F08 open item "the non-dry-run report's roots and FOMOD
stage" from the [tracker](../plans/2026-09-28-1130-remaining-work-tracker-plan.md):
what `validate` with both roots and without `--dry-run` decides when its report output
is asked for (`--output json`, `--output text`, `--validation-report`,
`--errors-only`, `--full`), compared with plain `validate` (no output flag, or
`--json`) on the same guide. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Evidence:
[plain-report-simulation-proof](plain-report-simulation-proof/README.md). F08 stays
partial.

## Why this gap

Rust had two answers for one command. Plain `validate -i G -s mods -g game` simulates
every step and refuses a guide whose Copy, Move, Patcher, strict Delete, Rename or
CleanList source is missing, or whose Execute or Run step lacks `--allow-processes`. The same
command with `--output json` (or any other report flag) ran only the archive/destination
and FOMOD stages, so it printed `"success": true` and exited 0 for that guide; only a
missing Extract archive failed it. A script or CI job that reads the JSON report was
told a guide is valid that the same validator refuses in text mode, and that `install`
refuses. C# never reports success there (below), so the Rust report was the only
answer that passed. Found by the review of the
[patcher missing-source slice](patcher-missing-source-contract.md); pre-existing on
`main`.

## Triage

**C# at `d31ccbd3`.** `RunValidateAsync` (`CLI/ModBuildConverter.cs:3014`) sends plain
`validate`, text or JSON, through the same `InstallationValidationPipeline.RunAsync`
call (`:3205-3217`); JSON prints the report (`:3219-3226`) and text logs it and returns
its exit code (`:3228-3260`). Without `--full`, `--dry-run` or `--dry-run-only` it
never sets the mod or game directory (`s_config` is filled only at `:3137-3143`). So:

- The `ComponentValidation` stage (`Services/Validation/InstallationValidationPipeline.cs:374-426`)
  runs `ComponentValidation.Run` (`ComponentValidation.cs:42-44`): archive membership
  and destinations only. It checks a Copy, Move, Rename, Delete, CleanList, Execute or
  Patcher Source only against the component's archives, never on disk; with no
  archive only an Extract is an error (`Missing Required Archives`, `:75-88`).
- The `FomodConfiguration` stage then fails every guide: `Cannot check FOMOD
  configuration: mod directory is not set or does not exist.` with one error
  (`InstallationValidationPipeline.cs:440-449`).
- A Patcher step whose Destination is `<<kotorDirectory>>` falls through to the
  `default` destination branch (`ComponentValidation.cs:331-341`), the placeholder
  resolves to an empty path without roots (`Utility/Utility.cs:27-37` replaces it with
  a null root), and `PathHelper.GetCaseSensitivePath` throws
  `'path' cannot be null or whitespace` (`FileSystemUtils/PathHelper.cs:441-446`). The
  CLI catches it as `Error during validation: ...` (`ModBuildConverter.cs:3262-3275`),
  the JSON error shape with exit 1.
- With `--full` (no dry run) the roots are set, but the `Environment` stage
  (`InstallationValidationPipeline.cs:270-291`) fails first here: `HoloPatcher could
  not be found in the Resources directory`.

So C# plain `validate` exits 1 for every guide in the proof, whatever is missing. That
is an accident of the basic CLI path (no roots), not a decision about sources; with
roots and a packaged HoloPatcher (`--full`), C# would pass a missing loose source,
since its report runs no step without a dry run.

**Rust at `fd9ec46`.** `pipeline_mode` (`src/main.rs:2977-2982`) sends every report
flag to `Engine::validate_report_with_options` (`:3359-3372`), and the plain command to
`Engine::validate` (`:3376-3380`). `validate` (`src/engine.rs:518-524`) checks recovery
state and the FOMOD gate, then `run` (`:1735-1738`) simulates every remaining component
in install order with `run_ordered` (`:1740`), stopping at the first error: a component
without instructions (`:1755-1759`), a missing source, the Execute trust gate. The
report (`validate_report_with_options_observed`, `:597-755`) ran the archive stage
(`validate_archives`, `:708-715` and `:865`), the FOMOD stage (`:716-733`) and, only
with a dry run, the `DryRun` stage (`:734-753`). Without a dry run nothing executed a
step, so the report passed every guide the stages above did not catch.

## Behaviour after this slice

- The report without a dry run, once every stage has passed (`errorCount` 0), runs
  plain `validate`'s simulation on a fresh virtual engine: the remaining components in
  install order, each with `run_ordered`, stopping at the first failure
  (`Engine::simulate_steps`, `src/engine.rs:774-789`, called at `:734-750`). A failure
  fails the `ComponentValidation` stage: its `OK: <component>` line becomes `ERROR:
  <error>`, with the wording plain `validate` prints after `Error:` (for example
  `ERROR: B / step 1 (Patcher): Missing external program/mod:
  <<modDirectory>>/patch/TSLPatcher.exe`), the summary becomes `1 component
  error(s)`, `errorCount` gains one, `passedCount` loses that stage, `success` is false
  and the exit code is 1 (`Report::simulation_failure`, `src/validation.rs:324-353`).
  An error that names no component (an ordering error; the FOMOD stage orders the
  guide first, so it normally fails there) is added to the stage instead.
- Every report mode without a dry run decides as plain `validate`: `--output json`,
  `--output text`, `--validation-report`, `--errors-only`, and `--full` (whose earlier
  stages also have to pass). Missing Copy, Move, Patcher, strict Delete, Rename and
  CleanList sources, an Execute or Run step without `--allow-processes`, a chosen
  option's miss and a component without instructions now fail the report, as they
  fail plain `validate` and `install`.
- Unchanged: a report that already has an error is not simulated, so a missing
  archive, a missing destination, a failed FOMOD stage or a failed `--full` stage are
  reported as before and counted once. Soft warnings from the simulation are not
  added. The dry-run report (`--dry-run`, `--dry-run-only`, the desktop Validate step,
  `--phase remaining`) is unchanged: there a missing source stays `DryRunSkipped`.
  Rootless validation (a root missing) is unchanged
  ([basic-validation-without-roots.md](basic-validation-without-roots.md)). Plain
  `validate`, `install` and `conflicts` are unchanged.

Callers checked: `validate_report_with_options_observed` is reached from the CLI
pipeline (`src/main.rs:3363`, the only caller with `dry_run` false), the desktop
Validate step (`src/ui.rs:4152`, always with a dry run) and
`validate_report_remaining_observed` (`src/engine.rs:1693`, always full with a dry
run). The `validate_report` and `validate_report_with_dry_run` wrappers have no caller
outside `src/engine.rs`. The simulation runs on its own engine (`fresh_engine`), so it
writes nothing and runs no program, as plain `validate`.

## Proof

[probe.py](plain-report-simulation-proof/probe.py) runs 19 cases against the live C#
CLI and the production release binary, with four checks per case and three per control:
`decision` (exit code and `success` of `--output json`, against C#), `modes` (plain
`validate`, `--output text`, `--validation-report` and `--output json` decide alike),
`report` (`errorCount`, and the `ComponentValidation` stage's `passed` and messages)
and `wording` (the stage's `ERROR:` line is plain `validate`'s error text), 69 checks.

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 63 match / 6 known difference / 0 fail |
| this slice, recorded reference | 63 match / 6 known difference / 0 fail |
| `fd9ec46` (pre-fix), recorded reference | 15 match / 6 known difference / 48 fail |

On `fd9ec46` every one of the 12 refused cases (a missing Copy, Move, Patcher,
Execute without trust, strict Delete, Rename and CleanList source, a miss after a
present Copy, a miss in the second of three components, a chosen option's Patcher
miss, a missing Copy with `--full`, a component without instructions) reported
success with `OK:` lines while plain `validate` failed, so all four checks of each
failed. The seven controls are the same on both builds.

## Known differences

- **C# fails every basic report.** Without `--full` or a dry run, C# plain `validate`
  fails on the FOMOD stage (no mod directory) or, for a Patcher guide, on the
  null-path exception, whatever the guide holds; `--full` fails its environment stage
  without a packaged HoloPatcher. Rust uses the roots it is given (as before,
  [basic-validation-without-roots.md](basic-validation-without-roots.md)) and passes a
  guide whose steps all simulate. Six controls, one `decision` check each.
- **C# does not simulate without a dry run.** Where C# has roots and a HoloPatcher
  (`--full`), its report would pass a missing loose source, which only its dry run
  reports (as `DryRunSkipped`). Rust's report without a dry run refuses it, as Rust's
  plain `validate` and `install` do (fail-closed). Not in the probe: the
  reference CLI here has no packaged HoloPatcher.
- **Report shape.** C# has no stage for a simulated step; Rust reports the failure in
  `ComponentValidation`, one error, with its own wording (`<component> / step <n>
  (<Action>): ...`), not C#'s `ERROR: <component>: <message>` form.
- **Stops at the first failure.** As plain `validate`, the simulation stops at the
  first failing component; later components keep their `OK:` lines although they were
  not simulated.
- **Regression fixture.** The FOMOD S2a proof's `registered_only_shared` case gave its
  second component no instructions and expected `--validation-report` to pass; plain
  `validate` already refused that guide. Its second component now copies the note too
  (224/224 on this slice and on `fd9ec46`; see
  [fomod-s2a-proof/README.md](fomod-s2a-proof/README.md)).
