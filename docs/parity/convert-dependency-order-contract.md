# Component order in `convert`

This contract covers the order in which `convert` writes a guide's components, and
when it refuses, for guides with `InstallBefore` and `InstallAfter`. The reference is
pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Evidence: [convert-dependency-order-proof](convert-dependency-order-proof/README.md).
F01 and F04 stay partial.

## Why this gap

`convert` is how a guide is moved between TOML, JSON, YAML, XML and Markdown, and its
output is what the desktop and the next run read. The C# `convert` writes components
in the order its dependency resolver gives, not in file order, and refuses a guide
whose `InstallBefore`/`InstallAfter` name a missing component, the component itself, or
form a cycle. Rust wrote the file order and accepted every such guide, so the same
command gave a differently ordered guide, or a converted file where C# gave none, and
it had no `--ignore-errors`, which C# offers on `convert`. This is user-visible in
every converted guide that uses those fields, with no safety reason to differ.

Candidates checked and not chosen: the FOMOD destination wording
([archive-member-brackets-contract.md](archive-member-brackets-contract.md)) is a Rust
refusal that C# does not make, so there is no C# output to match. `list`,
`fomod-inspect` and `inspect` have no C# verb. Byte-level serializer differences in
`convert` output (field order, defaults written) do not change what a guide loads as;
a cross-load of both programs' TOML, JSON, YAML, XML and Markdown output gave the same
components, instructions and resources.

## Triage at `24897e6`

**C# behaviour.** `RunConvertAsync` ingests the input (file or `--stdin`), then calls
`HandleDependencyResolutionErrors(components, opts.IgnoreErrors, "Convert")`
(`CLI/ModBuildConverter.cs:2040-2046`). That runs
`DependencyResolverService.ResolveDependencies` (`CLI/ModBuildConverter.cs:823-877`,
`Services/DependencyResolverService.cs:48-109`) over every top-level component,
selected or not:

- `ValidateDependencies` (`DependencyResolverService.cs:167-232`) gives one error per
  `InstallBefore` or `InstallAfter` GUID that names no component, and one per list that
  names the component itself.
- `BuildDependencyGraph` (`:234-271`) makes a component depend on each `InstallAfter`
  target, and each `InstallBefore` target depend on the component, in guide order with
  each list in authored order (`HashSet` insertion order).
- `DetectCircularDependencies` (`:273-347`) adds one error per cycle its depth-first
  walk finds.
- With no errors, `PerformTopologicalSort` (`:350-403`) writes a depth-first
  post-order: components in guide order, each after everything it depends on.

With errors and no `--ignore-errors`, the CLI throws `Dependency resolution failed with
N errors. Use --ignore-errors flag to attempt loading in best possible order.`: exit 1,
no output (`CLI/ModBuildConverter.cs:844-864`). With `--ignore-errors` it sorts anyway,
skipping missing targets; a cycle makes the sort throw, the resolver returns an empty
list, and the CLI writes a guide with no components (exit 0, `:838-842`, `:84-106`).
Options are not ordered. Dependencies and Restrictions take no part.

**Rust at `24897e6`.** `convert` (`src/main.rs:2267-2335`) loads with `Guide::load` or
`Guide::parse` (`:2278-2296`), applies drafting and the processing options, and renders
`guide.components` in file order (`:2316`). Nothing checks or orders `InstallBefore`
and `InstallAfter` there; the only consumer is the install order, `Guide::ordered`
(`src/model.rs:519-590`), which is a different algorithm (stable, selected components
only, Dependencies included) and runs only for validate and install. The `Convert`
arguments (`src/main.rs:328-348`) had no `--ignore-errors`.

## Change

- `Guide::sort_like_dependency_resolver(ignore_errors)` (`src/model.rs`) ports
  `ResolveDependencies` as the CLI uses it: the same errors in the same order, the same
  graph, the same cycle walk (`detect_cycle`) and the same post-order (`post_order`).
  GUIDs compare as parsed values (`guid::key_ascii`), as C# `Guid` does. It reorders
  `components` and touches nothing else.
- Fail closed where C# loses data: with `--ignore-errors`, a cycle is refused
  (`Dependency resolution failed: a circular InstallBefore/InstallAfter order cannot be
  resolved, even with --ignore-errors. Remove the cycle from the guide.`, exit 1, no
  output) instead of writing an empty guide. The cycle walk stops after it has visited
  every component; the C# walk can loop for ever (Known differences).
- `convert` gets `--ignore-errors` and calls the method right after loading, before
  drafting, local generation, downloads and selection, as C# does.

Callers checked: only `convert` calls the new method. `merge`, `convert --merge` (which
`cli_compat` rewrites to `merge`), validate, install, `conflicts`, `selection`, the
editor and the desktop loaders are not changed; `Guide::ordered` is untouched, so no
install order changes.

## Proof

[probe.py](convert-dependency-order-proof/probe.py) runs 29 cases against the live C#
CLI and the production binary, four checks each (116): `decision` (exit and whether
the output was written), `order` (component names in the output), `refs` (each
component's InstallBefore and InstallAfter as written) and `errors` (the N of the
failure message).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 108 match / 8 known difference / 0 fail |
| this slice, recorded reference | 108 match / 8 known difference / 0 fail |
| `24897e6` (pre-fix), recorded reference | 57 match / 8 known difference / 51 fail |

On `24897e6` every ordering case wrote the file order (15 cases: after, before, ties,
several targets, both lists, a chain, unselected components, a braced uppercase
reference, JSON and YAML input, JSON, YAML and Markdown output, `--stdin`), and every
error case wrote a guide where C# refused (missing InstallAfter or InstallBefore, also
on an unselected component, a self reference, two- and three-way cycles, JSON input).
`--ignore-errors` was an unknown argument. The controls (no ordering fields, an
already ordered guide, an option-level reference C# does not check) match on both.

## Known differences

- **`--ignore-errors` with a cycle (deliberate).** C# writes a guide with no
  components and exits 0, so `-o` replaces the target with an empty guide. Rust
  refuses and writes nothing. Two cases (a self reference, a two-way cycle), eight
  known-difference checks.
- **A cycle with a later component pointing into it.** For `B after A`, `A after B`,
  `C after A`, the C# cycle walk keeps adding `Guid.Empty` and does not finish (stopped
  after 15 seconds); Rust stops the walk and refuses with two errors (`B → A`, `A → C`,
  the second the C# walk's leftover-stack report). Not in the probe, which would hang.
- **GUID spelling.** C# writes the references it parsed (`aaaaaaaa-…`); Rust keeps
  the authored spelling (`{AAAAAAAA-…}`), as everywhere else. The probe compares them
  without braces and in lower case.
- **Error text.** Rust prints the summary and one `<name>: <message>` line per error;
  C# logs `❌ <name>: <message>` lines, the affected GUIDs and a three-point hint. The
  probe compares the count.
- **Other verbs.** C# also orders every non-Markdown guide when it loads it, without
  failing (`Services/ModComponentSerializationService.cs:871-892`), so its `merge`
  inputs, validate, install and the GUI see the resolver order; its validate and
  install also refuse missing or circular references unless `--ignore-errors`
  (`CLI/ModBuildConverter.cs:3092`, `:3418`), and its install orders again with
  Dependencies (`Installation/InstallCoordinator.cs:97-186`). Rust validate and install
  keep `Guide::ordered`, which checks references only on selected components and
  refuses `--ignore-errors` by name (COMPATIBILITY.md). Install order is F04-D/F04-F
  and is not changed here. `merge` and `convert --merge` do not take `--ignore-errors`
  in Rust; C# accepts it there and does not use it.
- **Platform.** The reference ran on Linux under `unshare -rn` with
  `DOTNET_ROLL_FORWARD=Major`. Nothing here depends on the platform.
