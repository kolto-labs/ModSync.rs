# Dependencies that name an option

This contract covers a component whose `Dependencies` name an immediate option of
another component (an `[[thisMod.Options]]` entry) instead of a component. C# looks such
a GUID up among each component's immediate options too and treats the dependency as met
when that option is selected, so it validates and installs the guide. Rust looked only at top-level
components, so it refused the guide everywhere: the install-order stage, the FOMOD
gate and the dry run of every report, plain `validate` and `install`, all with
`<component> requires selected component <option GUID>`. The reference is pinned C#
`d31ccbd3`; C# paths are under `src/ModSync.Core/` there. Rust lines before this slice
are at `0bc0efa`. Evidence: [option-dependencies-proof](option-dependencies-proof/README.md).
F04 stays partial.

## Why this gap

The audit lists dependency handling as open (F04-D, F04-B: options) in
[engine-actions-archives-audit.md](engine-actions-archives-audit.md). This one changes a
pass/fail decision on a guide both sides consider well formed: Rust's own selection
layer already follows a Dependency that names an option (`option_index`,
`src/selection.rs:17-27`, used by `component_checked`, `:30-56`), and its engine's
per-component check and the report's Conflicts stage already count a selected option
as met, yet `Guide::ordered` refused the guide before either ran.

## Triage

**C# at `d31ccbd3`.** A component's Dependencies and Restrictions are checked by
`GetConflictingComponents` (`ModComponent.cs:1934-1992`), which resolves each GUID with
`FindComponentFromGuid` (`:2030-2063`): it walks the top-level components and, for each,
its immediate options, and returns the first component or option with that GUID. A
Dependency is unmet when nothing is found or the found entry's own `IsSelected` is
false (`:1959-1975`); a Restriction conflicts when the found entry is selected
(`:1981-1990`). `ShouldInstallComponent` (`:1994-2006`), the install loop's dependency
check (`:1820-1827`), instruction conditions (`ShouldRunInstruction`, `:2008-2028`) and
the validation pipeline's Conflicts stage (`Services/Validation/InstallationValidationPipeline.cs:308`)
all use it. Install order ignores a GUID that is not a top-level component
(`Installation/InstallCoordinator.cs:117-125`), so a Dependency on an option adds no
ordering edge. Run live: B with `Dependencies = ["<A's option>"]` validates (every
stage passes, `errorCount` 0) and installs when the option and A are selected; it also
installs when the option is not selected or when A is not selected (C# installs
components with unmet Dependencies, as recorded in
[install-order-contract.md](install-order-contract.md)).

**Rust at `0bc0efa`.** `Guide::ordered` (`src/model.rs:553-678`) checks each selected
component's Dependencies with `find(required).is_some_and(chosen)` (`:572-581`), where
`find` indexes top-level components only, so a Dependency naming an option failed with
`<name> requires selected component <guid>`. Every validation and install path calls
`ordered`: the report's install-order stage (`src/engine.rs:766-769`), the FOMOD gate
that plain `validate` and the install preflight run first (`check_fomod_configuration`,
`:1247`, hence `Installation blocked before any write: FOMOD configuration check
failed:`), the step simulation (`validate_dry_run`, `:870`), checkpoint resume
(`:1543`), the completed prefix and remaining phase (`:1810`, `:1856`),
`src/engine/checkpoint_coordinator.rs:14`, `src/phase.rs`, `src/recovery.rs` and the
desktop (`src/ui.rs`). The engine's own
per-component check (`check_component`, `src/engine.rs:2022-2050`) uses
`selected_guid_keys` (`:362-368`, from `Guide::selected_ids`, `src/model.rs:527-541`),
which holds the selected components and the selected options of selected components,
so it already accepted the dependency.

**User-visible difference** (A with a selected option `O`, B with `Dependencies = ["O"]`):

```
                          C# install   Rust before                     Rust after
O and A selected          installs     refused (every report, install) installs
O not selected            installs     refused                         refused (unchanged)
O selected, A not         installs B   refused                         refused (unchanged)
```

## Behaviour after this slice

- **The rule.** `Guide::ordered` (`src/model.rs:557-687`) treats a Dependency as met
  when its GUID names a selected top-level component, as before, or an immediate
  selected option of a selected component (`met`, `:578-582`). An option nested below
  another option does not count, as C# `FindComponentFromGuid` looks no deeper. GUIDs
  compare as before (`key_ascii`, so a braced or upper-case spelling matches). The
  message for an unmet Dependency is unchanged.
- **Flags, not installs.** The Dependency is met by the `IsSelected` flags of the
  option and its component, not by the option having run: as in C#, an option that no
  Choose step names installs no files yet satisfies the Dependency.
- **Order.** A Dependency on an option adds no ordering edge (the index still holds
  top-level components only), as in C#: listed before A, B installs first.
- **Unchanged.** A Dependency on an option that is not selected, or whose component is
  not selected, is still refused (Rust refuses unmet Dependencies, deliberately; see
  [install-order-contract.md](install-order-contract.md)). Restrictions are unchanged:
  `ordered` checks them against components, and `check_component` already refuses a
  Restriction naming a selected option, as C# `GetConflictingComponents` would
  (C# installs it anyway, the documented Restrictions difference). InstallBefore and
  InstallAfter naming an option stay a missing reference in both (C#'s resolver
  reports it too).

Callers checked. Every caller of `ordered` listed above gets the same answer for every
guide whose Dependencies name components only, since `met` holds exactly the selected
components plus their immediate selected options; for a Dependency on a selected option they
now proceed to the checks they already ran (`check_component` and the FOMOD gate's
hard-dependency scope, which follows component GUIDs only, as C#
`ExpandWithHardDependencies` does). `conflicts` and the desktop's install order and
Validate step use the same function. The selection layer is unchanged.

## Proof

[probe.py](option-dependencies-proof/probe.py) runs 11 cases (a component Dependency
control, seven with a selected option: plain, listed before its component, braced
upper-case GUID, with the component too, the second of two options, an option reached
by a Choose step, a chain through a second component's option; a selected option
nested below A's selected option; and the two unchanged refusals) through the full report with a dry run, the basic dry-run report, `install`
and, for Rust, plain `validate`, and compares them with the live C# CLI with a
HoloPatcher stub (66 checks).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 49 match / 17 known difference / 0 fail |
| this slice, recorded reference | 49 match / 17 known difference / 0 fail |
| `0bc0efa` (pre-fix), recorded reference | 7 match / 17 known difference / 42 fail |

On `0bc0efa` every check of the seven selected-option cases failed (Rust refused every
report and the install; 42). The control, the nested option and the two unchanged
refusals gave the same results on both builds. The first version of this change
(`462a8f3`) counted nested options too and installed B in the nested case, which C#
refuses.

## Known differences

- **Unselected option or component (deliberate, unchanged).** When the named option is
  not selected, or its component is not, C# installs B anyway; Rust refuses before any
  write, as for any unmet Dependency. C# reads the option's own `IsSelected` even when
  its component is not selected; Rust counts an option only with its component, as its
  install does (an option of an unselected component never runs). Two cases, six
  checks each.
- **A nested option.** C# finds no option below an immediate option, so B's Dependency
  is missing: its reports pass with one warning and its install
  installs A, then fails at B with `DependencyViolation`. Rust refuses the unmet
  Dependency in every report and before any write, and its Conflicts stage does not
  warn (it counts nested selected options, as `check_component` does). The decision of
  `install` matches; five checks differ. For the same reason a Restriction naming a
  selected nested option makes Rust refuse (`B conflicts with N`) in validate, install
  and `conflicts`; C# finds no such option and sees no conflict (read from the code, not
  run live). Fail-closed.
- **Desktop auto-selection.** Ticking B in the desktop selects its option `O` but not
  `O`'s component A (`component_checked` and `option_index`,
  `src/selection.rs:43-54`), as C#'s `HandleComponentToOptionDependencies` does
  (`src/ModSync.GUI/Services/ComponentSelectionService.cs:552-583`). The desktop
  Validate step then refuses `B requires selected component <O>` until A is selected,
  where C# installs. The CLI `--select` path selects A too. Selection code is unchanged
  in this slice; not probed.
- **`--phase base`.** When a base component depends on an option of a widescreen-only
  component, `--phase base` refuses with `<component> requires selected component
  <option>` (still before any write) instead of the clearer `The base build cannot be
  installed first` message. Not probed.
- **An option GUID equal to a component GUID.** The Rust loader refuses the guide with
  `Duplicate Guid` (`src/model.rs:350-354`); C# accepts it (reported by review). Outside
  this slice; not probed.
- **Markdown.** Not probed; the Markdown reader keeps Dependencies, and the same rule
  applies.
- **Desktop.** The desktop uses the same `ordered`; not exercised natively.
- **Platform.** The reference ran on Linux under `unshare -Urnm` with a tmpfs over
  `/Override`.
