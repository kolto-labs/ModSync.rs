# A missing Patcher source in the validation dry run

This contract covers the missing-input row of F05-PATCHER from the
[actions audit](engine-actions-archives-audit.md): what the validation dry run
(`validate --dry-run`, the desktop Validate step) reports when a component's own
`Patcher` step names a patcher that is not in the mod folder. The reference is pinned
C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Evidence: [patcher-missing-source-proof](patcher-missing-source-proof/README.md). F05
and F09 stay partial.

## Why this gap

A guide's patcher mods (TSLPatcher/HoloPatcher folders) are separate downloads, and a
user often validates before every download is in place. For a missing Copy, Move or
Extract source both C# and Rust report the component as skipped with a warning
(`DryRunSkipped`) and validation passes. For a missing Patcher folder or executable C#
does the same, but Rust failed the whole report with `Missing external program/mod`,
so `validate --dry-run` exited 1 and the desktop Validate step blocked a guide that C#
lets through with a warning. The same Rust code already treated the miss as a typed
missing source and then threw the type away, so the fix is one line and needs no new
behaviour.

Leads checked live and not chosen: a `DelDuplicate` into a missing folder already fails
the archive stage in both (C# passes only its dry run, after creating the folder while
validating; [missing-destination-contract.md](missing-destination-contract.md), known
difference), so the report's decision already matches. Execute/Run is behind the
deliberate `--allow-processes` gate.

## Triage at `0f71baa`

**C# behaviour.** `ModComponent.ExecuteSingleInstructionAsync` resolves a Patcher
step's source with `TryResolveInstructionSourcesAsync` (`ModComponent.cs:1630-1641`,
helper at `:975-1029`). When `SetRealPaths` finds no file (and auto-extraction finds
nothing), it logs `Missing patcher folder/archive for '<name>'; run Fetch Downloads or
extract mod files to the workspace.` and returns `ActionExitCode.FileNotFoundPre`, the
code a missing Copy, Move or Extract source returns (`:1575-1623`). The component's
loop maps `FileNotFoundPre` to `InstallExitCode.MissingSourceFiles`
(`ModComponent.cs:1862-1866`), and the dry run turns that into the warning `Skipped
'<name>' during dry-run: source archives or files are not available.` with category
`DryRunSkipped`, no instruction index, and drops the component's earlier issues
(`Services/Validation/DryRunValidator.cs:515-527`). `validate` exits 0. In a chosen
option the same miss becomes `OptionalInstallFailed` and a `DryRun` error
(`ModComponent.cs:1857-1860`, [option-failure-contract.md](option-failure-contract.md)).
Install fails the component.

**Rust at `0f71baa`.** `Engine::resolve_instruction` binds a Patcher source; a direct
miss (`src/engine/source_resolution.rs:128-140`) enters recovery, and a final miss
there is the typed `MissingSourceFiles` (`:376`). The Patcher branch then replaced
that error with an untyped `Missing external program/mod: <Source>`
(`src/engine/source_resolution.rs:163-169`). `validate_dry_run` reports only a
`MissingSourceFiles` error as `DryRunIssue::skipped` (`src/engine.rs:830-836`,
`src/validation.rs:58`), so the Patcher miss became `ERROR: [EngineSimulation]
<component> / step <n> (Patcher): Missing external program/mod: <Source>`, counted in
`errorCount`, and `validate --dry-run` exited 1.

## Behaviour after this slice

- A component whose own Patcher step names a literal Source that is missing (the
  whole patcher folder, its `tslpatchdata` folder, or the executable inside a present
  folder) is now `WARNING: [DryRunSkipped] Skipped '<name>' during dry-run: source
  archives or files are not available.` in the `DryRun` stage, with the component's
  name and GUID and no instruction index, as in C#. `warningCount` counts it and the
  report passes. Earlier steps' warnings of that component are dropped with the skip,
  as C# drops its issues; later steps are not run; other components are checked as
  before.
- Unchanged: a present patcher is simulated as before; a step whose instruction
  conditions are not met, or a component that is not selected, is not run; a Patcher
  miss in a chosen option still fails its component (now through
  `OptionalInstallFailed::wrap`, which turns the typed miss into an error, the same
  message); `install` and the desktop install still refuse the guide with `Missing
  external program/mod: <Source>`; plain text-mode `validate` (no dry run) still
  stops with that error. The JSON report without `--dry-run` never simulates Patcher
  steps, so it reports success for such a guide, as before this slice (tracked with
  the F08 open item on the non-dry-run report); C# plain `validate` fails every
  Patcher guide in these fixtures with `'path' cannot be null or whitespace`.

`Engine::resolve_instruction` now wraps the same message in `MissingSourceFiles`
(`src/engine/source_resolution.rs:163-175`); the type's comment lists its producers
(`src/engine.rs:277-281`). Callers checked: `resolve_instruction` is called by the
Extract, Copy/Move and Patcher branches of `Engine::execute`, and only the Patcher
branch reaches this mapping. The type is inspected only by `validate_dry_run` (the
skip) and `OptionalInstallFailed::wrap` (the option error). Install, checkpoints,
`conflicts` and the desktop install only check success and print the same text. The
second `Missing external program/mod` check in `Engine::process_path`
(`src/engine.rs:2783-2788` at `0f71baa`) runs only after a successful binding and is
unchanged.

## Proof

[probe.py](patcher-missing-source-proof/probe.py) runs 18 cases against the live C#
CLI and the production release binary: 17 dry-run cases with four checks each
(`decision`: exit code and `success`; `counts`: `errorCount` and `warningCount`;
`dryrun`: the `DryRun` stage messages as `skipped`, `error` or `warning`; `issues`:
each issue's severity, componentName and componentGuid, plus category and
instructionIndex for warnings) and one install case (`decision`), 69 checks.

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 58 match / 11 known difference / 0 fail |
| this slice, recorded reference | 58 match / 11 known difference / 0 fail |
| `0f71baa` (pre-fix), recorded reference | 26 match / 11 known difference / 32 fail |

On `0f71baa` each of the eight missing-patcher cases (missing folder, missing
`tslpatchdata` with a namespace index, missing executable in a present folder, after
a present Copy, before later steps, beside a passing component, two failing
components, after a kept-file warning) failed validation with an error where C# skips
with a warning. The controls match on both builds: a present patcher (also followed by
a Rename of its installed file), an unmet instruction condition, a missing Copy
source, a Patcher miss in a chosen option (an error in both), an unselected
component, and install with the miss.

## Known differences

- **Other refusals come first.** A missing Patcher Source whose step Rust also refuses
  for another reason stays an error: an unsafe path (`<<modDirectory>>/../x`, a
  patcher folder symlinked outside the roots), a non-game Destination, or Arguments
  that are neither an INI name nor an index. Those checks run in recovery before the
  miss is typed (`src/engine/source_resolution.rs:206-258`); C# skips the component.
  Fail-closed.
- **Source shapes Rust refuses.** A Patcher Source that is a wildcard
  (`<<modDirectory>>/pat*/TSLPatcher.exe`) or an empty list fails the Rust dry run
  (`Wildcards are not allowed here`, `Patcher needs exactly one source to bind its
  configuration and logs`); C# finds no file and skips the component. Rust binds a
  patcher to exactly one literal file or folder before it runs anything (fail-closed,
  [source-resolution-contract.md](source-resolution-contract.md)). Two cases, eight
  known-difference checks.
- **Folder Source.** A Patcher Source that names a present folder
  (`<<modDirectory>>/patch`) binds that folder in Rust and the patcher is simulated;
  C# `SetRealPaths` finds no file there and skips the component. One case, three
  known-difference checks (the decision matches).
- **Error issues.** For the option case the probe compares only an error's severity,
  component name and GUID: Rust's category is `EngineSimulation` with the step index,
  C#'s is `DryRun` without one (option-failure contract).
- **Recovery.** A missing Patcher Source that an archive in the mod folder would
  provide goes through the shared auto-extract recovery first; a final miss there is
  now skipped too. Not in the probe (the C# CLI needs a resource registry for it).
