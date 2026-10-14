# Delete, Copy, Move and Extract with an empty Source list

This contract covers the empty-Source case of F05-DELETE, F05-COPY, F05-MOVE and
F05-EXTRACT from the [actions audit](engine-actions-archives-audit.md): an instruction whose
`Source` list is empty after loading. That means `Source = []`, a list of blank
strings such as `Source = [""]`, or no `Source` key. The reference is pinned C#
`d31ccbd3`, and C# paths below are under `src/ModSync.Core/` there. Evidence:
[empty-source-proof](empty-source-proof/README.md). F05 stays partial.

## Triage at `fba4eb1`

**C# loading.** A missing `Source` key loads as an empty list
(`Services/ModComponentSerializationService.cs:2185`, `:2392`). Empty-string
entries are kept, but wildcard expansion skips them
(`FileSystemUtils/PathHelper.cs:639`) and so does the literal pass of
`SetRealPaths` (`Instruction.cs:316-322`), so they resolve to nothing. The probe
uses `""`; a whitespace-only entry was not run against the reference.

**C# Delete.** Execution calls `SetRealPaths(skipExistenceCheck: true)`
(`ModComponent.cs:1586-1590`), which gives an empty path list without throwing.
`Instruction.DeleteFile` (`Instruction.cs:828-933`) loops over nothing. Then:

- `Overwrite = false`, or omitted (false for Delete): it logs "No files to
  delete, skipping this instruction." and returns `Success` (`:894-896`).
- `Overwrite = true`: it returns `FileNotFoundPost` (`:897-901`).
  `ExecuteInstructionsAsync` maps that to `MissingSourceFiles` (`ModComponent.cs:1862-1866`).
  Install fails the component. The dry run reports the `DryRunSkipped` warning
  instead (`Services/Validation/DryRunValidator.cs:515-527`).

**C# Copy, Move and Extract.** `TryResolveInstructionSourcesAsync`
(`ModComponent.cs:975-1029`) calls `SetRealPaths`, which throws
`FileNotFoundException` when expansion finds nothing (`Instruction.cs:350-367`).
Auto-extraction and the subfolder remap find nothing either. For Move, the
redundant-target fallback returns false when `Source` is empty
(`ModComponent.cs:1298-1301`). The action returns `FileNotFoundPre`
(`ModComponent.cs:1575-1583` for Extract, `:1597-1624` for Copy and Move), which
is `MissingSourceFiles`: install fails, and the dry run reports `DryRunSkipped`.
The C# component and archive checks raise no error for an Extract that names no
archive (the reference dry run has no Error issue).

**Rust at `fba4eb1`.** The loader drops blank Source entries
(`src/model.rs:814-816`), so all three spellings arrive as an empty list. The
`Delete` branch (`src/engine.rs:2229-2230`) and Copy/Move/Extract source resolution
(`src/engine/source_resolution.rs:142-146`) call `matches`. That function rejects
an empty list with the untyped error `Instruction has no Source`
(`src/engine.rs:1836`). An Extract hit the same error earlier, in archive
validation (`src/engine.rs:824-827`, the dry-run report's `ComponentValidation`
stage) and in the FOMOD gate (`src/engine.rs:1094-1098`). So:

- An empty Delete with `Overwrite` false or omitted failed the dry-run report,
  plain `validate`, `install` and `conflicts`. The reference installs it as a
  no-op and runs the later steps.
- An empty Delete with `Overwrite = true`, and an empty Copy, Move or Extract, were
  refused by validate and install, as in C#. The dry-run report had an `Error`,
  though, where C# reports the `DryRunSkipped` warning and passes.

**Status.** An open gap. The install decision and the final tree were wrong for a
non-strict empty Delete, and the dry-run report was wrong for every case.

## Behaviour after this slice

- **Non-strict empty Delete.** With `Overwrite` false or omitted, the step is a
  no-op. Install emits `Delete has no Source; nothing to delete` and continues with
  the later steps. Every verb accepts it: the dry-run report, plain `validate`,
  `install` and `conflicts`.
- **Strict empty Delete.** With `Overwrite = true`, the step fails with the typed
  missing-source cause `Delete has no Source and Overwrite is true`.
- **Empty Copy, Move or Extract.** The step fails with the typed missing-source
  cause `<Action> has no Source`. Source recovery (auto-extraction, the subfolder
  variant, the redundant-Move fallback) is not tried, because it has nothing to
  bind. This holds for any `Overwrite`. Archive validation and the FOMOD gate skip
  an Extract with no Source, since it names no archive to inspect.
- **How those failures surface.** `validate --output json --dry-run` replaces the
  component's issues with the single `DryRunSkipped` warning and passes, as C#
  does. Plain `validate` exits non-zero, the missing-source policy of the install
  gate. `install` is refused by its validation before any write. `conflicts`
  exits 1. The desktop validation page and installer run the same engine.
- **Unchanged.** Rename with no Source is still rejected by every verb, and the
  C# dry run and install reject it too (`No source paths available for Rename
  instruction`, `UnknownError`). The probe keeps it as a control. Non-empty
  sources behave as before.

## Known differences and limits

- **Timing.** C# install runs the earlier steps before the component fails.
  Rust validates the whole selection first and refuses with no game writes. Both
  reject the guide. The probe compares only the decision in failure cases.
- **Messages.** C# logs `MissingSourceFiles` and its own text. Rust names the
  action and the empty Source.
- **Other actions.** CleanList, Patcher, Execute and Run keep their existing
  empty-Source errors. DelDuplicate reads Source as an extension list, and an
  empty list selects the default `.dds`/`.tpc`/`.tga` set.

- **Scalar empty Source in XML and Markdown.** Resolved after `3911e74`: a scalar empty
  or whitespace Source (`<Source></Source>`, `Source = ""` or `Source: ''` in Markdown
  metadata) is no longer read as an empty list, so it does not take the behaviour above.
  It is kept as `[""]` and refused, as in TOML, JSON and YAML. C# refuses the XML guide
  at load and drops the Markdown metadata block. A self-closing `<Source />`, and a
  Choose (which C# does not path-normalize), are still an empty list, as in C#. See [scalar-empty-source-contract.md](scalar-empty-source-contract.md).
- **Report without `--dry-run`.** `validate --output json` and `validate --full` (which
  the desktop runs before its dry run) now pass an empty Extract with no errors, as
  C# `ComponentValidationService.IsArchiveValidAsync` does with no sources. Empty Copy
  and Move already passed there. Correction (2026-10-04): the C# CLI report's archive
  stage is `ComponentValidation`, not that service, and a live run gives
  `Missing Required Archives: []` for an empty Extract whenever the component has no
  archive (so `[Extract [], Extract nope.zip]` gives two errors in C# and one in Rust);
  Rust keeps passing the empty Extract. Recorded as a known difference in
  [missing-archives-contract.md](missing-archives-contract.md). Superseded
  (2026-10-04): once its stages pass, the report without `--dry-run` now simulates the
  steps as plain `validate` does, so an empty Extract fails it (`Extract has no
  Source`), as it fails plain `validate` and the C# report; the archive stage itself
  still passes it. This is the CLI report only; the desktop report always includes a
  dry run and is unchanged. See [plain-report-simulation-contract.md](plain-report-simulation-contract.md).
- **An Extract Destination (since
  [extract-destination-contract.md](extract-destination-contract.md)).** The
  `extract_empty` and `extract_nokey` cases name Destination `Override`, which Rust's
  basic report now fails with the Destination error. The C# CLI basic report fails
  them too, but with `Missing Required Archives: []` (`VerifyExtractPaths` stops
  ComponentValidation before the Destination check), and its full report fails in the
  Environment stage, where Rust's full report passes (an older empty-Source gap, open
  in the tracker). The reference here is `DryRunValidator` alone, which passes. The
  proof counts those two dry-run checks as known differences (49 match / 2 known / 0
  fail).
- **The reports' archive stage (since
  [empty-extract-report-contract.md](empty-extract-report-contract.md), 2026-10-05).**
  An own Extract with no Source in a component with no archive that exists now fails
  the archive stage with `Missing Required Archives: []`, as C# `VerifyExtractPaths`
  does, so the basic and full reports (with or without `--dry-run`) and the desktop
  Validate step fail it; before, the reports with a dry run passed it with
  `DryRunSkipped`. This supersedes the two notes above for that case. Beside an
  archive that exists, the empty Extract still passes the reports (as in C#) and fails
  plain `validate` and `install`. The proof now also compares both reports with the
  live C# CLI (86 match / 3 known / 0 fail; 7 fail on `b8b4d57`).
- **Proof coverage.** The probe covers TOML only; the other loaders were checked by
  hand in review of 29f9d59.

## Notes for guide authors

- A Delete with no Source and no `Overwrite`, often a placeholder left during
  editing, now passes and does nothing, as in C#. It used to block validation.
- Through `guide-edit`, a Delete inserted without `Overwrite` takes the model
  default of `true` (see [delete-strict-contract.md](delete-strict-contract.md)).
  With an empty Source it therefore fails as strict. Set `Overwrite = false`, or
  remove the step.
- An empty Copy, Move or Extract now shows only as a `DryRunSkipped` warning in the
  dry-run report, which passes. Plain `validate` and `install` still reject it.
  Read the warning, not only the pass/fail, when you check a guide with the report.
