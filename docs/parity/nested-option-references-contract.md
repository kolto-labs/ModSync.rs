# Restrictions and step conditions that name a nested option

This contract covers a GUID that names an option nested below another option (an
`[[thisMod.Options.Options]]` entry) in a component's `Restrictions` or in a step's
`Dependencies` or `Restrictions`. The C# loader keeps no option below an option, so for
C# such a GUID names nothing: a Restriction finds no conflict, a step Dependency is
unmet and a step Restriction is met. Rust counted a selected nested option as
selected, so it refused a guide C# installs (a component Restriction) and ran or skipped
steps C# skips or runs (step conditions). The reference is pinned C# `d31ccbd3`; C#
paths are under `src/ModSync.Core/` there. Rust lines before this slice are at
`c386d33`. Evidence:
[nested-option-references-proof](nested-option-references-proof/README.md). F04 stays
partial.

## Why this gap

[option-dependencies-contract.md](option-dependencies-contract.md) made
`Guide::ordered` count immediate options only, as C# `FindComponentFromGuid` does, and
recorded that `check_component` and the report's Conflicts stage still counted nested
options (its Known differences). The same selected set decides every instruction
condition, so the gap is wider than that note: it changes which files are installed,
not only the decision. The engine was also inconsistent with itself: `ordered` refused
a component Dependency on a nested option while `check_component` accepted it.

## Triage

**C# at `d31ccbd3`.** `DeserializeOptions`
(`Services/ModComponentSerializationService.cs:2406-2467`) reads an option's Name,
Description, Guid, Restrictions, Dependencies, Instructions and IsSelected and never its
`Options`, so the loaded model has no option below an option. A component's
Restrictions are checked by `GetConflictingComponents` (`ModComponent.cs:1934-1992`),
which resolves each GUID with `FindComponentFromGuid` (`:2030-2063`, top-level
components and their immediate options); a GUID it does not find is no conflict
(`:1981-1990`) and an unmet Dependency (`:1959-1975`). A step runs only when
`ShouldRunInstruction` (`:2008-2028`) finds no conflict for its Dependencies and
Restrictions; the install loop and the dry run skip it otherwise (`:1835-1838`,
`:1566`). The validation pipeline's Conflicts stage
(`Services/Validation/InstallationValidationPipeline.cs:292-337`) uses the same
function. A Choose step of an option picks from that option's own
`Options` (`Instruction.GetChosenOptions`, `Instruction.cs:1609-1611`), which is always
empty. Run live: a Restriction of `B` naming a selected nested option passes every
report and installs; a step whose Dependencies name it is skipped and a step whose
Restrictions name it runs, in a component's own steps and in a chosen option's steps.

**Rust at `c386d33`.** `Guide::selected_ids` (`src/model.rs:527-541`) collected each
selected component and, recursively, every selected option below it.
`selected_guid_keys` (`src/engine.rs:362-368`) turns it into the engine's `selected`
set, used by `check_component` (`:2022-2050`; Dependencies `:2035-2041`, Restrictions
`:2042-2048`) and by `instruction_applies` (`:2053-2055`, through `applies_with`,
`:226-237`), which every simulation, install, the FOMOD gate (`:1242`), the destination
gate (`:554-555`) and checkpoint runs (`src/engine/checkpoint_coordinator.rs:57`) use.
The report's Conflicts stage reads `selected_ids` directly (`src/engine.rs:748`,
`Report::declared_conflicts`, `src/validation.rs:461-532`). So a Restriction of `B`
naming a selected nested option failed the Conflicts, FomodConfiguration and DryRun
stages, plain `validate` and `install` (`Installation blocked before any write: FOMOD
configuration check failed: B conflicts with <N>`), and a step condition naming it was
evaluated as if the option existed.

**User-visible difference** (`A` with a selected option `O`, which has a selected
option `N`):

```
                                  C#                Rust before          Rust after
B Restrictions = [N]              installs          refused              installs
B step Dependencies = [N]         step skipped      step runs            step skipped
B step Restrictions = [N]         step runs         step skipped         step runs
O's step Dependencies = [N]       step skipped      step runs            step skipped
O's step Restrictions = [N]       step runs         step skipped         step runs
```

## Behaviour after this slice

- **The rule.** `Guide::selected_ids` (`src/model.rs:527-538`) holds each selected
  top-level component and its selected immediate options, the GUIDs C#
  `FindComponentFromGuid` can find selected. A GUID naming an option nested below an
  option counts as not selected wherever the engine checks selection: component
  Dependencies and Restrictions (`check_component`), step Dependencies and Restrictions
  (`instruction_applies`, so a Choose step too), the FOMOD and destination gates, and
  the report's Conflicts stage. `Guide::ordered` already counted immediate options only.
- **Unchanged.** An option selected under an unselected component still counts as not
  selected (C# reads its own `IsSelected`; documented in the option-failure and
  option-dependencies contracts). A Restriction naming a selected immediate option is
  still refused (deliberate, below). A component Dependency on a nested option was
  refused by `ordered` and still is; the Conflicts stage now reports it as a missing
  dependency warning, as C# does. The selection layer, the loader and the model keep
  nested options; a Choose step of an option still runs the nested options it names.

Callers checked. `selected_ids` has two callers, `selected_guid_keys` and the
Conflicts stage; `selected_guid_keys` feeds the engine's `selected` set at every entry
point (`src/engine.rs:555`, `:879`, `:978`, `:1242`, `:1603`, `:1879`,
`src/engine/checkpoint_coordinator.rs:57`). For a guide without nested options the set
is unchanged. The selected set is computed on each run and not stored in checkpoints,
receipts or journals. The desktop uses the same engine and report.

## Proof

[probe.py](nested-option-references-proof/probe.py) runs 10 cases (a Restriction of `B`
naming `N`, selected and not; a step of `B` whose Dependencies or Restrictions name `N`,
and the two controls naming `O`; a step of the chosen `O` whose Dependencies or
Restrictions name `N`; and two unchanged differences) through the full report with a
dry run, the basic dry-run report, `install` and, for Rust, plain `validate`, and
compares them with the live C# CLI with a HoloPatcher stub (70 checks).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 62 match / 8 known difference / 0 fail |
| this slice, recorded reference | 62 match / 8 known difference / 0 fail |
| `c386d33` (pre-fix), recorded reference | 51 match / 8 known difference / 11 fail |

On `c386d33` every check of `restriction_nested` failed (Rust refused every report and
the install; 7), and the install check of the four step-condition cases (4). The
controls and the two unchanged differences gave the same results on both builds.

## Known differences

- **A Restriction naming a selected immediate option (deliberate, unchanged).** C#'s
  CLI loader runs `AutoFixComponentIssues` with AttemptFixes on
  (`Services/ModComponentSerializationService.cs:906-910`, `:5568-5629`), which
  deselects a selected component or immediate option that a selected component's
  Restrictions name, so its reports pass and it installs without that option. Rust
  refuses a Restrictions conflict before any write and changes no selection, as for a
  component (see [install-order-contract.md](install-order-contract.md)). Seven checks
  (`restriction_option`).
- **A Choose step of an option (Rust extension, unchanged).** Rust loads options below
  an option and runs the ones a Choose step of their option names; C# loads none, so
  that Choose chooses nothing and the nested option's files are not installed. The
  reports agree; one check (`nested_chosen`'s install, where Rust also writes
  `Override/y.txt`). Such a nested option still counts as not selected for conditions,
  as C# would count a GUID it cannot find.
- **Message wording.** For a component Dependency on a nested option, the Conflicts
  stage now warns `<B>: missing dependencies: <GUID>`; C# names it `ModComponent
  Undefined with GUID.`. Not probed here (the option-dependencies proof's
  `option_nested` case covers the decision).
- **Other formats.** Only TOML is probed. The rule is in the shared engine, so JSON,
  YAML, XML and Markdown guides with nested options behave the same (Markdown
  carries them in its metadata blocks, and Rust's export writes them); all four were
  checked ad hoc against the live CLI in review. Every C# loader drops nested
  options.
- **A nested option GUID equal to a top-level GUID.** The Rust loader refuses it
  (`Duplicate Guid`, `src/model.rs:345-361`); C# drops the nested option and installs
  (fail-closed; see the option/component GUID collision note in
  [option-dependencies-contract.md](option-dependencies-contract.md)).
- **Side effects in the Rust extension.** A chosen nested option whose own component
  Dependencies name a sibling nested option is now refused by `check_component`, and
  the selection layer still warns `Restriction conflict` for nested options (warnings
  only).
- **Desktop.** The desktop uses the same engine and report; not exercised natively.
  Its selection layer still selects and shows nested options.
- **Platform.** The reference ran on Linux under `unshare -Urnm` with a tmpfs over
  `/Override`.
