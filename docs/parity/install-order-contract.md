# Install order and reference checks in validate and install

This contract covers the order in which `install` runs the selected components, the
order `validate` lists them in, and when both refuse a guide for its `InstallBefore`
and `InstallAfter` references. The reference is pinned C# `d31ccbd3`; C# paths are
under `src/ModSync.Core/` there unless they start with `src/ModSync.GUI/`.
Evidence: [install-order-proof](install-order-proof/README.md). F04 stays partial.

## Why this gap

Install order decides which mod's file wins when two write the same path, so a
different order gives a different game. Rows F04-D and F04-F of the audit
([engine-actions-archives-audit.md](engine-actions-archives-audit.md)) left the install
order uncompared, and the convert slice
([convert-dependency-order-contract.md](convert-dependency-order-contract.md), Known
differences) recorded that C# validate and install also refuse bad references. A first
live run confirmed both: for `A`, `B` (InstallAfter `A`), `C`, the C# CLI installs
`A, C, B` and Rust installed `A, B, C`; and a missing InstallAfter on an unselected
component stops C# validate and install, where Rust validated and installed.

Candidates checked and not chosen: `merge` and `convert --merge` taking
`--ignore-errors` (C# accepts the flag there but does not use it, so nothing a user
sees changes), and C#'s load-time reordering in other verbs, which this slice covers
for validate and install.

## Triage at `e4c0b4d`

**C# behaviour.** `validate` and `install` load the guide
(`CLI/ModBuildConverter.cs:3089`, `:3417`). A non-Markdown guide is reordered while it
loads by `DependencyResolverService.ResolveDependencies` when that reports no error,
and kept in file order otherwise (`Services/ModComponentSerializationService.cs:871-892`).
The CLI then calls `HandleDependencyResolutionErrors` (`:3092`, `:3418`, `:823-877`),
the same check `convert` runs:

- every top-level component takes part, selected or not;
- a missing, self or circular InstallBefore/InstallAfter reference stops the run
  unless `--ignore-errors`: validate prints `Error loading instruction file:
  Dependency resolution failed with N errors. Use --ignore-errors flag to attempt
  loading in best possible order.` (`:3094-3101`, as the `error` of `--output json`),
  install `Error during installation: ...` (`:3678-3681`), exit 1, before any report
  or game write;
- otherwise the components take the resolver's depth-first order, which is the order
  the report lists them in.

A Markdown guide goes through guide ingestion, which drops InstallBefore and
InstallAfter (it keeps Dependencies), so it is never reordered or refused for them.

`install` then runs `InstallationService.InstallAllSelectedComponentsAsync`
(`Services/InstallationService.cs:1145-1219`). `InstallCoordinator.InitializeAsync`
orders the whole list with `GetOrderedInstallList` (`Installation/InstallCoordinator.cs:65-67`,
`:85-87`, `:97-186`), and the selected components are installed in that order
(`InstallationService.cs:1219`). `GetOrderedInstallList` is a breadth-first (Kahn)
sort over every component, selected or not:

- edges: each Dependencies and InstallAfter target before the component, the
  component before each InstallBefore target, in list order and each list in authored
  order; targets not in the guide are skipped (`:115-148`);
- the queue starts with every component that has nothing before it, in list order,
  and a component joins the back of the queue when its last predecessor is placed
  (`:150-166`);
- components a cycle leaves unplaced are appended in list order (`:168-177`).

So a component waits for its layer: `A`, `B` (InstallAfter `A`), `C` installs as
`A, C, B`. An unselected component still takes part, so it can delay what follows it.

`validate --dry-run` simulates the selected components in list order
(`Services/Validation/DryRunValidator.cs:455`, `:499`), not in the install order.

**Rust at `e4c0b4d`.** Validate, install and `conflicts` load with
`load_validation_guide` (`src/main.rs:956-966`), which neither reorders nor checks
references. Every install, validate dry run, phase, checkpoint and desktop path orders
with `Guide::ordered` (`src/model.rs:519-590`; callers `src/engine.rs:671`, `:739`,
`:1088`, `:1384`, `:1649`, `:1695`, `src/engine/checkpoint_coordinator.rs:14`,
`src/phase.rs:82`, `:95`, `:100`, `src/recovery.rs:277`, `:301`, `src/ui.rs:299`,
`:2058`, `:3935`, `:5061`): a stable sort of the selected components only, each time
taking the first component in file order whose predecessors are placed. It refuses
missing references and cycles only among selected components.

## Behaviour after this slice

- `Guide::sort_for_install` (`src/model.rs:689-703`) is the C# check without
  `--ignore-errors`: any missing, self or circular InstallBefore/InstallAfter
  reference on any top-level component stops the run with `Dependency resolution
  failed with N errors. Fix these InstallBefore/InstallAfter references in the guide
  (validate and install do not take --ignore-errors):` and one `  - <name>: <message>`
  line per error (the same lines `convert --ignore-errors` prints); otherwise the
  components take the resolver's order. `load_validation_guide`
  (`src/main.rs:956-973`) calls it for every non-Markdown guide (the extension rule,
  or the content when the extension names no format), so validate (with or without
  roots), install and `conflicts` refuse before any report, and the report lists the
  components in C#'s order. `--output json` puts the message in `error`, as C# does.
- `Guide::ordered` (`src/model.rs:519-637`) is `GetOrderedInstallList`: it starts
  from the resolver's order (file order when the resolver reports an error, as the C#
  loader does), sorts every component breadth-first by Dependencies, InstallAfter and
  InstallBefore, and returns the selected ones. Every caller above gets the new order,
  so install, the validate dry run, phases, checkpoints and the desktop agree.
- Kept from before, fail-closed: a selected component whose Dependencies target is
  not selected, a Restrictions conflict, a missing reference on a selected component,
  and a selected component a cycle leaves unplaced are refused (the last now reads
  `Dependency/order cycle: <name> cannot be ordered`); C# appends that component at
  the end.
- Selected widescreen-only components (`WidescreenOnly = true`) come after the base
  components, unless a selected base component must follow one of them (then the
  breadth-first order stands). The C# wizard offers them on its own widescreen pages
  after the base install and installs the ones chosen there in list order
  (`src/ModSync.GUI/Dialogs/WizardPages/WidescreenInstallingPage.axaml.cs:87-115`); its
  base install takes everything selected, so a widescreen component already selected
  in the file goes in its breadth-first place there. Rust's Base and Remaining phases
  need the base build to be the first part of the order.

The resolver code is shared with `convert`: `resolve_dependencies` (`:713-799`) holds
what `sort_like_dependency_resolver` did (errors, graph, cycle walk, post-order), and
`convert`'s messages and results are unchanged (its probe is at baseline).

## Proof

[probe.py](install-order-proof/probe.py) runs 34 cases against the live C# CLI and
the production binary, with `--use-file-selection` throughout: 19 installs (`decision`,
`order` of the installed components, `files` with contents) and 15 validate runs
(`decision`, `shape` of the result, `errors` N, `listing` of the archive stage,
`dryrun` messages), 132 checks.

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 127 match / 5 known difference / 0 fail |
| this slice, recorded reference | 127 match / 5 known difference / 0 fail |
| `e4c0b4d` (pre-fix), recorded reference | 56 match / 2 known difference / 74 fail |

On `e4c0b4d` every ordering case installed the stable order (InstallAfter, a
Dependencies link, InstallBefore, a link through an unselected component, a chain,
a guide the resolver reorders, mixed edges, two targets, JSON and YAML input, a
Markdown Dependencies link), the report listed components in file order, and every
refusal case validated or installed where C# stopped (missing InstallAfter and
InstallBefore on an unselected component, a self reference, cycles among unselected
components or with one unselected member, JSON, the report without roots, install).
Selected missing references and cycles were refused on both builds, but inside the
report rather than before it. Controls (no references, an already ordered guide, a
Dependencies target listed after its dependant, a widescreen component a base component follows, Markdown
references on an unselected component) match on both.

## Known differences

- **Widescreen-only components (deliberate).** The C# CLI installs a selected
  widescreen-only component in its breadth-first place (`A`, `B` after `A`, widescreen
  `E`: `A, E, B`); Rust installs it after the base build (`A, B, E`). Within the
  widescreen components Rust keeps the breadth-first order; the C# wizard's widescreen
  page uses list order. Two known-difference checks.
- **Markdown references.** C# drops InstallBefore and InstallAfter from a Markdown
  guide; Rust keeps them and orders by them, so `A`, `B` (InstallAfter `A`), `C`
  installs as `A, C, B` in Rust and `A, B, C` in C#. Rust does not refuse a Markdown
  guide for them at load, as C# does not, but still refuses a missing reference on a
  selected component. The pre-fix build matched this case by chance (its stable order
  equalled the file order); other Markdown guides with references differed before too.
  Two known-difference checks.
- **Validate's dry run (deliberate).** Rust simulates the order it installs in; C#
  simulates the list order and then installs in the breadth-first order. A component
  that moves a file a later-installed component creates is skipped with a
  `DryRunSkipped` warning in C# and simulated cleanly in Rust. The pre-fix build
  matched C# here because it simulated and installed in list order. One
  known-difference check.
- **`--ignore-errors` (deliberate, unchanged).** C# validate and install accept it and
  go on in the best order they can; Rust refuses the flag by name
  (COMPATIBILITY.md), so a bad reference anywhere in the guide must be fixed.
- **Cycles through Dependencies.** C#'s load check does not look at Dependencies, and
  its install appends what a cycle leaves; Rust refuses a selected component in such a
  cycle, as before. Not in the probe.
- **Messages.** Rust names the errors after a one-line summary; C# logs `❌` lines,
  the affected GUIDs and a three-point hint. The probe compares the count.
- **Desktop.** The desktop does not run the load check (the C# GUI does not either); its
  install order follows `Guide::ordered` like the CLI. Not exercised natively.
- **Platform.** The reference ran on Linux under `unshare -rn` with
  `DOTNET_ROLL_FORWARD=Major`. Nothing here depends on the platform.
