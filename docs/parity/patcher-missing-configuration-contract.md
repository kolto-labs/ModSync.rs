# A patcher without changes.ini in the validation dry run

This contract covers one more row of F05-PATCHER from the
[actions audit](engine-actions-archives-audit.md): what the validation dry run
(`validate --dry-run`, the desktop Validate step) reports when a component's Patcher
step finds its patcher executable but no `changes.ini` to simulate. The reference is
pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there. Evidence:
[patcher-missing-configuration-proof](patcher-missing-configuration-proof/README.md).
F05 and F09 stay partial.

## Why this gap

Found by the review of the [patcher missing-source slice](patcher-missing-source-contract.md)
and confirmed live. A patcher download whose `tslpatchdata` folder is missing or holds
no `changes.ini` (a partial extraction, or an INI one folder too deep or under another
name) passes the C# dry run with no issue. Rust passed it too but added a `PatcherSimulation` warning for each such step, so
`warningCount`, the `DryRun` stage and the desktop Validate step's warning cards
differed from C# on every such guide. The decision was already the same.

Leads checked live and not chosen: a Copy into a missing Destination folder fails every
C# `validate` (the basic mode on its FOMOD stage, the dry run with `Destination cannot
be found!`) and passes C# install (`-y --skip-validation`), which creates the folder;
Rust's report fails it and Rust's plain `validate` and install pass it. Plain
`validate` has no C# counterpart with the same roots, and changing what it checks
reaches the desktop and install preflight callers, so it is left for its own slice.
A `DelDuplicate` into a missing folder already fails the archive stage in both
([missing-destination-contract.md](missing-destination-contract.md)).

## Triage

**C# at `d31ccbd3`.** In the dry run the instruction's file system is the virtual one,
so `Instruction.ExecuteTSLPatcherAsync` (`Instruction.cs:1289`) takes the
`VirtualFileSystemProvider` branch (`:1308-1326`) for each resolved source: it calls
`TslPatcherVfsSimulator.SimulateInstallAsync` with the patcher folder and continues.
`SimulateInstallAsync` (`TSLPatcher/TslPatcherVfsSimulator.cs:23-51`) looks for
`changes.ini` in that folder and in its `tslpatchdata` folder only (`FindChangesIniPath`,
`:112-131`); when neither exists it logs the verbose line `[Simulation] No changes.ini
found under '<dir>' — skipping TSLPatcher VFS simulation.` (`:44-50`) and returns. The
step returns `ActionExitCode.Success` (`Instruction.cs:1450`), and no issue reaches the
report. The simulator never reads `Arguments` or `namespaces.ini`.

**Rust at `c22836a`.** `Engine::execute` sends a virtual Patcher step to
`Engine::simulate_patcher` (`src/engine.rs:2847-2861`).
`simulation_configuration` (`src/engine/patcher_simulation.rs:103-182`) picks the
named INI, the `namespaces.ini` entry, or `changes.ini` in the patcher folder or its
`tslpatchdata` folder (`:177-181`), and returns `None` only for that last lookup.
`simulate_patcher` turned `None` into `validation_warning("PatcherSimulation", "No
changes.ini found; no InstallList effects inferred", <patcher folder>)`
(`:197-204`), which `validate_dry_run` appends to the report (`src/engine.rs:864`):

```
C#    DryRun: (no messages)                                       warningCount 0
Rust  WARNING: [PatcherSimulation] No changes.ini found; no InstallList effects inferred
                                                                  warningCount 1
```

## Behaviour after this slice

- A Patcher step whose executable is present but which has no `changes.ini` in the
  patcher folder or its `tslpatchdata` folder (and no named INI or `namespaces.ini`)
  simulates nothing and adds nothing to the report, as in C#. `validate --dry-run`,
  `--dry-run-only`, `--phase remaining` and the desktop Validate step no longer count a
  warning for it. The step's progress event now says `No changes.ini found under
  <patcher folder>; skipped InstallList simulation`, as C# logs it; plain `validate`
  prints it as `Patcher / 0: ...`, the dry run discards step events as before.
  (`src/engine/patcher_simulation.rs:197-209`.)
- Unchanged: a present `changes.ini` is simulated as before, including an empty one or
  one without `[InstallList]` (no effects, no warning, as in C#). An InstallList file
  missing from `tslpatchdata` keeps its `unknown payload` warning (deliberate, below).
  A named INI or a `namespaces.ini` entry that is missing still fails the step, and a
  namespace index above `0` without `namespaces.ini` still fails
  ([patcher-install-list-simulation.md](patcher-install-list-simulation.md)). A missing
  executable or patcher folder is still `DryRunSkipped`
  ([patcher-missing-source-contract.md](patcher-missing-source-contract.md)).
  `install` never simulates: it hands the folder to the patcher backend, which needs
  that configuration (not compared here).

Callers checked: `simulate_patcher` has one caller, the virtual Patcher branch of
`Engine::execute` (`src/engine.rs:2859`), which plain `validate`, the dry run, the
report's step simulation, `conflicts` and install preflight share. Of these only the
dry run reads `validation_warnings` (`src/engine.rs:864`); the others never showed the
warning. `simulation_configuration` is also called by archive recovery
(`src/engine/source_resolution.rs:854`), which still requires a configuration before
it adopts an archive for a Patcher step; that call does not go through this branch.

## Proof

[probe.py](patcher-missing-configuration-proof/probe.py) runs 18 dry-run cases against
the live C# CLI and the production release binary, four checks each (`decision`,
`counts`, `dryrun`, `issues`), 72 checks.

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 69 match / 3 known difference / 0 fail |
| this slice, recorded reference | 69 match / 3 known difference / 0 fail |
| `c22836a` (pre-fix), recorded reference | 39 match / 3 known difference / 30 fail |

On `c22836a` each of the ten cases without an INI (no `tslpatchdata`, a `tslpatchdata`
with a payload but no INI, an empty `tslpatchdata`, the INI one folder too deep or under
another name, namespace index `0`, before and after a Copy, two components, beside a
complete patcher) had one `PatcherSimulation` warning per component where C# has none,
so `counts`, `dryrun` and `issues` failed. The seven controls and the known difference
are the same on both builds.

## Known differences

- **Unknown payloads (deliberate).** When `changes.ini` lists a file that
  `tslpatchdata` does not hold, C# writes an empty file in its place silently; Rust
  marks the target as unknown content and warns `InstallList predicts file presence
  but payload content is unknown` (`src/engine/patcher_simulation.rs:259-266`), so a
  later step that needs the bytes fails instead of reading an invented empty file
  ([patcher-install-list-simulation.md](patcher-install-list-simulation.md)). One case,
  three known-difference checks (the decision matches).
- **Configuration choice (deliberate, predates this slice).** C# always simulates
  `changes.ini` and ignores `Arguments`; Rust follows a named INI or the
  `namespaces.ini` entry the step selects, and fails when that selection is missing or
  out of range. Not in the probe.
- **The progress event.** C# logs its line only at verbose level; Rust emits it as an
  ordinary step event. It is not part of any report.
  C# also always logs the non-verbose `[Simulation] Applied TSLPatcher VFS simulation
  from changes.ini` (`Instruction.cs:1322-1323`), even when nothing was found. Rust's
  plain `validate` follows the skip event with its generic `Patcher: InstallList
  presence simulated; binary patch effects remain unmodeled` line (`src/engine.rs:1867-1869`).
- **Case of `changes.ini` and `tslpatchdata` (predates this slice).** Rust finds them
  ignoring case, as C# does on Windows, so `tslpatchdata/Changes.INI`, a root
  `CHANGES.INI` and a `TSLPatchData/` folder are simulated. C# on Linux checks files
  on disk case-sensitively and misses them, so a later step that needs an installed
  file can be `DryRunSkipped` in C# only. Both pass. Checked live, not in the probe.
- **Folder Source without a configuration.** With `Source` naming the patcher folder
  or its `tslpatchdata` (exe present, no INI), C# skips the component (`DryRunSkipped`,
  one warning); Rust keeps the folder Source bound, as
  [patcher-missing-source-contract.md](patcher-missing-source-contract.md) records, and
  now reports no warning (before this slice its one warning was the PatcherSimulation
  one). The decision is the same. Checked live, not in the probe.
