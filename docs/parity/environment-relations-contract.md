# Restrictions and Dependencies in the full report's Environment stage

This contract covers the Environment stage of `validate --full` (also with `--dry-run`
or `--dry-run-only`): C# checks there that no selected component restricts another
selected one and that every Dependency is selected, and fails the report with one
error when not. Rust's Environment stage passed such a guide and three or more later
stages failed it. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Rust lines before this slice are at `1ab3d27`. Evidence:
[environment-relations-proof](environment-relations-proof/README.md). Tracker item:
F08 ("remaining backend classes, exact reports ..."); F08 stays partial.

## Why this gap

Both programs fail these guides, so no pass/fail decision changes; the report does. A
guide that lists two mods which cannot be installed together (a `Restrictions` entry)
is common, and selecting every mod, the CLI default, selects both. C# then reports one
failed Environment stage with its component message and stops; Rust reported a passed
Environment stage and then three errors (`Conflicts`, `InstallOrder`,
`FomodConfiguration`), or four with `--dry-run-only` (its dry run fails it too), so a
script reading `errorCount` or the failed stage saw a different report, and the text
output never
said which component could not be installed in C#'s words. The dry-run-only slice
recorded this as "Not changed" ([dry-run-only-environment-contract.md](dry-run-only-environment-contract.md),
Known differences). The fix refuses nothing new and loosens nothing: it only ends the
report earlier with C#'s message.

Candidates looked at and skipped:

- **A wildcard Delete in the game folder** (`Delete <<kotorDirectory>>/Override/*.txt`
  also removed a game file named `Y.TXT`; C# on Linux matches ordinally and keeps it).
  Wildcard case is a documented, deliberate Rust choice: COMPATIBILITY.md ("Directory
  Sources and wildcard rules for file actions": "Rust matching stays case-insensitive
  where C# wildcard matching is ordinal", which covers every action's Source, Delete
  included) and the delete-directory contract (Known differences, "Case"). No change
  and no new note needed.
- **C#'s load-time selection fixes.** With `--use-file-selection`, C#'s loader
  (`AutoFixComponentIssues`, `Services/ModComponentSerializationService.cs:906-910`,
  `:5569-5629`, AttemptFixes on) selects every Dependency (even of an unselected
  component) and deselects whatever a selected component restricts, so C# validates
  and installs a selection the file did not make. Rust keeps the file's selection and
  fails it. Documented as deliberately not reproduced (COMPATIBILITY.md, "The
  reference's guide/directory autofixes are deliberately not reproduced"); a change
  there would make Rust install mods nobody selected.
- **`--select` and Dependencies.** C#'s `--select` filters select no Dependency, so its
  Environment stage fails `A` when `A` depends on a mod in another category; Rust's
  `--select` adds the dependency closure (a documented Rust selection policy,
  COMPATIBILITY.md "CLI behaviour changes"). Not changed.
- **The full Environment stage's archive checks.** Without `--dry-run-only`, C#'s
  Environment stage also fails a missing archive; Rust reports it in its
  ComponentValidation stage (recorded in several contracts, among them
  full-pipeline-destination). Changing that moves many recorded proofs; left for a
  separate slice.

## Triage

**C# at `d31ccbd3`.** Without `--select` or `--use-file-selection`, `validate` selects
every component (`CLI/ModBuildConverter.cs:3196-3203`); with `--select` it keeps the
components the filters select (`ApplySelectionFilters`, `:1370`), with no dependency
closure. `validate --full` runs the Environment stage first
(`Services/Validation/InstallationValidationPipeline.cs:66-82`), which calls
`InstallationService.ValidateInstallationEnvironmentAsync`. For each selected component
(`Services/InstallationService.cs:969-1010`) it looks up every Restriction with
`ModComponent.FindComponentsFromGuidList` (top-level components and their immediate
options, `ModComponent.cs:2031-2092`; a GUID that names neither is skipped) and logs
`Cannot install '<A>' due to '<C>' being selected for install.` for each selected one
(`:976-991`), then logs `Cannot install '<A>' due to '<B>' not being selected for
install.` for each Dependency found but not selected (`:993-1005`), and runs
`ComponentValidation` (`:1007-1009`). Any of these sets `Some components failed to
validate. Check the output/console window for details.` and a second line about
antivirus or missing mods (`:1051-1057`), which replaces the read-only folder messages
(`:1029-1042`) and is replaced only by the case-duplicate message (`:1059-1065`). The
stage fails with that one message, the report counts one error and stops
(`InstallationValidationPipeline.cs:75-81`). The `Cannot install` lines go only to the
log. The basic report (no `--full`) has no Environment or Conflicts stage, and its dry
run does not check Restrictions, so it passes a restriction conflict.

**Rust at `1ab3d27`.** The full report's Environment stage
(`src/engine.rs:964-997`) checked the recovery state, the writable folders and, with
`--dry-run-only` only, each component's archive checks (`Report::environment_components`,
`src/validation.rs:277-316`). Restrictions and Dependencies were checked later: the
Conflicts stage (`src/engine.rs:1000-1013`, `Report::declared_conflicts`) counted one
error per restricting component and one warning per missing Dependency;
`Guide::ordered` (`src/model.rs:554`) failed the InstallOrder stage; the FOMOD gate's
readiness check (`Engine::check_component`, `src/engine.rs:2470-2491`: `A conflicts
with <GUID>`, `A requires <GUID>`) failed the FomodConfiguration stage; under
`--dry-run-only` the dry run failed it again.

**User-visible difference** (`validate -i G -s mods -g game --output json --full`):

```
Case                                  C#                         Rust before                 Rust after
A restricts C, all selected           Environment fails,         Environment passes;         as C#, plus one
                                        1 error, report ends       3 errors (Conflicts,        `Cannot install` line
                                                                   InstallOrder, FOMOD)
same, --select category:X (A and C)   as above                   as above                    as C#
A and C restrict each other           as above                   4 errors                    as C#, two lines
same + A's archive missing            as above                   4 errors                    as C#
same + mod folder read-only           component message wins     folder message, 1 error     as C#
```

## Behaviour after this slice

- **The Environment stage.** After the recovery-state check, the full report's
  Environment stage (`src/engine.rs:1010-1063`) now collects, for every selected
  component (not completed under `--phase remaining`), the errors C# logs:
  `environment_relations` (`src/engine.rs:481-525`) looks each Restriction and
  Dependency up among the top-level components and their immediate options, as
  `FindComponentFromGuid` does, skips a GUID that names neither, and tests it against
  the set the Conflicts stage uses (selected components, their selected immediate
  options, and completed components). A selected Restriction gives `Cannot install
  '<A>' due to '<C>' being selected for install.`; a Dependency not selected gives
  `Cannot install '<A>' due to '<B>' not being selected for install.`
- **The message.** `Report::environment_components` (`src/validation.rs:277-321`) now
  takes those lines with the `--dry-run-only` archive outcomes: when there are any, the
  stage fails with C#'s `Some components failed to validate. ...` message (replacing a
  read-only folder message, as in C#), counts one error and the report ends. Rust lists
  the `Cannot install` lines after the message, then any archive errors (`ERROR: A:
  ...`); C# writes them to its log only.
- **Callers.** The full report serves `validate --full` (with or without `--dry-run`,
  `--dry-run-only`), the `--phase remaining` report and the desktop Validate step,
  which now show the failed Environment stage. `install`, plain `validate`,
  `conflicts` and the basic report (no `--full`) are unchanged: they still refuse a
  restriction conflict or an unselected Dependency before any write
  (`Installation blocked before any write: FOMOD configuration check failed: A
  conflicts with <GUID>`). The Conflicts stage and its Dependency warning are unchanged;
  they are reached only when the Environment stage passes, as in C#.

## Proof

[environment-relations-proof](environment-relations-proof/README.md) runs 10 cases on
both sides under `unshare -Urnm` against the live C# CLI with a HoloPatcher stub:
`validate --output json --full` (decision, counts with `componentCount`, stages, the
Environment message), text `--full` (decision, C#'s message present), `--full
--dry-run-only` (decision, counts, message), the basic report `--dry-run` (decision),
the lines Rust lists after the message, and a Rust-only `install` check, 118 checks.

| Binary | environment-relations |
| --- | --- |
| this slice, live C# CLI | 85 match / 33 known difference / 0 fail |
| this slice, recorded references | 85 match / 33 known difference / 0 fail |
| `1ab3d27` (pre-fix), recorded references | 53 match / 21 known difference / 44 fail |

On `1ab3d27` `restrict_all`, `restrict_select` and `restrict_mutual` each fail seven
checks (`full/counts`, `full/stages`, `full/environment`, `dro/counts`,
`dro/environment`, `text/message`, `details`); `restrict_missing_archive` fails five
(its `--dry-run-only` report already failed the Environment stage on the missing
archive); `restrict_readonly_mods` fails four (the stage failed with the folder
message, so only the message, the text and `details` differ); the known cases
`depends_file_selection` and `restrict_option` fail seven each because Rust's
documented result changed (see below). The decisions, `install` and the controls are
the same on both binaries.

## Known differences

- **The basic report** (`basic/decision` in four restriction cases): C#'s basic report
  passes a restriction conflict; Rust's dry run refuses it (fail closed, unchanged).
- **`--select` with a Dependency in another category** (`depends_select`, 9 checks):
  C#'s Environment stage fails `A`; Rust's `--select` adds the dependency closure, so
  every stage passes (documented selection policy). Rust's `install` installs both.
- **`--use-file-selection` with an unselected Dependency** (`depends_file_selection`,
  10 checks): C#'s loader selects the Dependency and passes; Rust keeps the file's
  selection, so its Environment stage now fails `A` with C#'s message (before: two
  later stages failed it, InstallOrder and FomodConfiguration). Rust's `install` still refuses it before any write (C#'s
  install was not run).
- **A Restriction naming a selected option** (`restrict_option`, 10 checks): C#'s
  loader deselects the option and passes; Rust keeps it, so its Environment stage now
  fails `A` (fail closed, documented in the nested-option-references slice; before:
  Conflicts and FomodConfiguration failed it).
- **The component lines.** Rust adds `ERROR: Cannot install ...` lines after C#'s
  message (checked as `details`); C# writes them to its log only.
- **An option under an unselected component** counts as not selected in Rust (as in
  its Conflicts stage and install order); C# reads the option's own flag.
- **Not changed.** Without `--dry-run-only`, a component's archive or destination
  error still fails Rust's ComponentValidation stage, where C#'s Environment stage
  fails it; when such a component also has a restriction conflict, both report the
  Environment failure.

## Notes for guide authors

Nothing changes for guides. When `validate --full` now fails with `Some components
failed to validate` and a `Cannot install 'A' due to 'C' being selected for install.`
line, deselect one of the two mods (`--select` fewer categories, or set `IsSelected`
and use `--use-file-selection`), or select the missing Dependency, and validate again.
