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

`install --direct-markdown` passes `preserveInputOrder: opts.DirectMarkdown`
(`CLI/ModBuildConverter.cs:3638`), so the coordinator keeps the list order
(`InstallCoordinator.cs:65-67`, `:85-87`). For a Markdown guide (by extension, or by
content when the extension names no format, `.txt` and no extension included) the
references are gone, so the breadth-first sort sees Dependencies alone from file order.
Widescreen-only components get no special place in the CLI.

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

- `Guide::sort_for_install` (`src/model.rs:728-742`) is the C# check without
  `--ignore-errors`: any missing, self or circular InstallBefore/InstallAfter
  reference on any top-level component stops the run with `Dependency resolution
  failed with N errors. Fix these InstallBefore/InstallAfter references in the guide
  (validate and install do not take --ignore-errors):` and one `  - <name>: <message>`
  line per error (the same lines `convert --ignore-errors` prints); otherwise the
  components take the resolver's order. `load_validation_guide`
  (`src/main.rs:956-973`) calls it for every non-Markdown guide, so validate (with or
  without roots), install and `conflicts` refuse before any report, and the report
  lists the components in C#'s order. `--output json` puts the message in `error`, as
  C# does.
- `Guide` carries `Ordering` (`src/model.rs:48-65`), rules that are never written to
  a file: `markdown` (set by the file loader, `:174-178`, with the rule `convert` uses:
  the extension, or the content when the extension names no format),
  `preserve_input_order` (set for `--direct-markdown`, `src/main.rs:3184-3185`) and
  `widescreen_last` (set for `--phase base` and `--phase remaining`,
  `src/phase.rs:78-79`).
- `Guide::ordered` (`src/model.rs:540-676`) is `GetOrderedInstallList`: it starts from
  the resolver's order (file order when the resolver reports an error, as the C#
  loader does), sorts every component breadth-first by Dependencies, InstallAfter and
  InstallBefore, and returns the selected ones. For a Markdown guide InstallBefore and
  InstallAfter take no part (no order, no missing-reference refusal; Dependencies
  still apply) and the start is file order, as in C#. With `preserve_input_order` it
  returns the selection in list order after the existing checks. Every caller above
  gets the new order, so install, the validate dry run, phases, checkpoints and the
  desktop agree.
- Selected widescreen-only components (`WidescreenOnly = true`) take their
  breadth-first place, as in the C# CLI, in plain installs and `--phase all`. Only with
  `widescreen_last` do they come after the base components, because Rust's Remaining
  phase continues a base prefix; then a selected base component that must follow a
  selected widescreen-only one (directly or through unselected components) keeps the
  breadth-first order, and `--phase base` refuses before any write
  (`src/phase.rs:91-103`): `The base build cannot be installed first: the install
  order puts widescreen-only <W> before <B>. Install with --phase all.` Before, Base
  installed and Remaining then refused (`Completed components must form the selected
  installation prefix in order`).
- Kept from before, fail-closed: a selected component whose Dependencies target is
  not selected, a Restrictions conflict, a missing InstallBefore/InstallAfter target
  of a selected non-Markdown component, and a selected component a cycle leaves
  unplaced are refused (the last now reads `Dependency/order cycle: <name> cannot be
  ordered`).

The resolver code is shared with `convert`: `resolve_dependencies` (`:752-838`) holds
what `sort_like_dependency_resolver` did (errors, graph, cycle walk, post-order), and
`convert`'s messages and results are unchanged (its probe is at baseline).

## Proof

[probe.py](install-order-proof/probe.py) runs 44 cases, 162 checks: 27 installs
(`decision`, `order` of the installed components, `files` with contents; two of them
`--direct-markdown`) and 15 validate runs (`decision`, `shape` of the result,
`errors` N, `listing` of the archive stage, `dryrun` messages) against the live C# CLI,
plus 2 Rust-only `--phase base` installs checked against the documented Rust result
(the C# CLI has no phases).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 161 match / 1 known difference / 0 fail |
| this slice, recorded reference | 161 match / 1 known difference / 0 fail |
| `174482a` (first version of this slice), recorded reference | 138 match / 1 known difference / 23 fail |
| `e4c0b4d` (pre-fix), recorded reference | 77 match / 0 known difference / 85 fail |

On `e4c0b4d` every ordering case installed the stable order (InstallAfter, a
Dependencies link, InstallBefore, a link through an unselected component, a chain,
a guide the resolver reorders, mixed edges, two targets, JSON and YAML input, a
widescreen component a base component follows, Markdown with a Dependencies link or a
backward or InstallBefore reference in `.md`, `.txt` and extensionless files, a
backward reference under `--direct-markdown`), the report listed components in file
order, every refusal case validated or installed where C# stopped, and `--phase base`
installed a base component that must follow a widescreen-only one. Selected missing
references and cycles were refused on both builds, but inside the report rather than
before it. On `174482a` the review findings fail: widescreen-only components moved
behind the base build (`widescreen_root`, `ws_first`, `ws_two`), Markdown references
ordered the install (five Markdown cases), `--direct-markdown` reordered
(`direct_dependency`, `direct_after`), and `--phase base` installed where it must
refuse. Controls (no references, an already ordered guide, a Dependencies target
listed after its dependant, Markdown references on an unselected component, a Base run
with nothing blocked) match on every build that runs them.

## Known differences

- **Validate's dry run (deliberate).** Rust simulates the order it installs in; C#
  simulates the list order and then installs in the breadth-first order. A component
  that moves a file a later-installed component creates is skipped with a
  `DryRunSkipped` warning in C# and simulated cleanly in Rust. The pre-fix build
  matched C# here because it simulated and installed in list order. One
  known-difference check.
- **`--phase base` with a base component behind a widescreen-only one (deliberate,
  Rust-only).** Refused before any write, as above; the C# CLI has no phases, and the
  C# wizard's base install takes everything selected. Two Rust-only cases.
- **Phased widescreen order.** With `--phase base`/`remaining`, Rust installs the
  widescreen-only components after the base build in their breadth-first order; the C#
  wizard's widescreen page installs the ones chosen there in list order
  (`src/ModSync.GUI/Dialogs/WizardPages/WidescreenInstallingPage.axaml.cs:87-115`). Not
  in the probe.
- **`--ignore-errors` (deliberate, unchanged).** C# validate and install accept it and
  go on in the best order they can; Rust refuses the flag by name
  (COMPATIBILITY.md), so a bad reference anywhere in the guide must be fixed.
- **Dependencies on unselected components and Dependencies cycles (deliberate,
  unchanged).** C#'s load check does not look at Dependencies. Its install installs
  the components anyway: checked live, a selected `B` with Dependencies on an
  unselected `U` installs both `U` and `B` (exit 0), and `A`, `B` depending on each
  other plus `C` installs all three. Rust refuses a selected component whose
  Dependencies target is not selected, and a selected component a Dependencies cycle
  leaves unplaced. Not in the probe.
- **Ordering checks in direct Markdown.** `--direct-markdown` keeps C#'s list
  order, but Rust still runs its other ordering checks first (selected Dependencies
  and Restrictions); C# runs none.
- **Messages.** Rust names the errors after a one-line summary; C# logs `❌` lines,
  the affected GUIDs and a three-point hint. The probe compares the count.
- **Desktop.** The desktop does not run the load check (the C# GUI does not either); its
  install order follows `Guide::ordered`, with the Markdown rule from the file loader.
  Not exercised natively.
- **Platform.** The reference ran on Linux under `unshare -rn` with
  `DOTNET_ROLL_FORWARD=Major`. Nothing here depends on the platform.
