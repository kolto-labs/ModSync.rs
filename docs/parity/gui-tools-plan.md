# GUI tools parity implementation plan (F19, F21, F22)

## Status and authority

This plan closes the gaps recorded in `docs/parity/gui-tools-audit.md` for the acceptance-ledger domains F19, F21, and F22 (`docs/parity/ACCEPTANCE.md:54-57`). The pinned behavioral oracle is the C# tree at `d31ccbd3bfcddb4705917eafa6483715c166de3b`; its conflict, editor, and update behaviors are identified below by file and line (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:19-224`; `C# src/ModSync.Core/Services/ComponentEditorService.cs:18-311`; `C# src/ModSync.Core/Services/ModUpdateCheckService.cs:19-232`).

The plan is implementation-ready but does not authorize a release, upstream pull request, mutation of a game/mod tree, or a parity claim without the acceptance artifacts named here (`docs/parity/ACCEPTANCE.md:54-57`; `COMPATIBILITY.md:121-125`).

## Binding constraints

1. **One shared behavior path.** Conflict analysis, guide mutations/imports/resource enumeration, and update checking/persistence belong in production library modules; GUI and CLI remain adapters over the same actions (`src/engine.rs:777-783`; `src/editor.rs:4-88`; `src/updates.rs:327-463`; `src/main.rs:655-693`).
2. **No installer fork in UI.** Ordered selection, VFS execution, stop-on-failure, and remaining-phase validation stay in `engine.rs` (`src/engine.rs:898-907`; `src/engine.rs:950-1043`). F19 presentation work must consume `conflicts::Report`, not reproduce instruction execution in `ui.rs` (`src/conflicts.rs:1-84`; `src/ui.rs:1578-1598`).
3. **Fail closed.** Do not copy the C# analyzer's catch-and-continue behavior; Rust conflict analysis continues to stop on a failed instruction and report unsupported external effects as limitations (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:115-160`; `src/engine.rs:777-783`; `src/engine.rs:1003-1043`).
4. **Transactional edits.** An editor command succeeds only if the cloned guide passes identity, reference, ordering, and cycle validation; failure leaves the caller's guide byte-for-byte unchanged (`src/model.rs:178-203`; `src/model.rs:320-390`).
5. **Read-only validation and browsing.** Conflict preview and editor resource browsing may read archives/game/mod inputs but must not mutate them or launch external programs; production save actions use the existing atomic-write boundary (`src/engine.rs:777-783`; `src/model.rs:231-300`; `src/recovery.rs:295-302`).
6. **No silent format parity.** Preserve each input format only where `Guide::parse` and the selected serializer support it; do not claim Markdown export because it is explicitly unsupported (`src/model.rs:97-176`; `src/model.rs:231-300`).
7. **No Rust tests or Rust harnesses in this phase.** Verification uses the production CLI and native desktop executable directly, and stores probe inputs/outputs under `docs/parity/gui-tools-probes/` as specified below (`docs/parity/ACCEPTANCE.md:54-57`). The ordinary repository check list remains documented elsewhere but is not a substitute for these acceptance probes (`COMPATIBILITY.md:121-125`).
8. **No build during this documentation slice.** Implementation builds, when later authorized, use the host command and a lane-owned target directory with `-j 2`; this plan itself changes only the two GUI-tools documents.
9. **Serialized `ui.rs` ownership.** No GUI-tools task may edit `src/ui.rs` until the existing install-progress, completion-pages, and mod-selection owners have released it, in that order; current editor and conflict surfaces are concentrated there (`src/ui.rs:1578-1598`; `src/ui.rs:2666-2843`).
10. **Serialized `engine.rs` ownership.** The planned path does not require an engine edit because existing APIs already analyze full and remaining installs (`src/engine.rs:777-783`; `src/engine.rs:898-907`). If implementation discovers an engine requirement, stop and queue a separately reviewed engine-owned change after the active engine lane; do not fold it into a UI commit (`src/engine.rs:950-1043`).
11. **Compatibility truth follows behavior.** Update `COMPATIBILITY.md` only in the implementation slice that changes a supported behavior or proof boundary; its present F19/F21/F22 entries remain partial until the relevant evidence gates pass (`COMPATIBILITY.md:121-125`; `COMPATIBILITY.md:621-639`).

## C# audit translated into requirements

| Requirement | Required behavior | Oracle referent | Rust starting point |
| --- | --- | --- | --- |
| R19.1 | Run a non-mutating, ordered VFS preview that attributes each game-path write and chooses the last distinct component writer. | `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:19-30`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:70-110`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:169-224` | `src/engine.rs:777-783`; `src/conflicts.rs:37-84` |
| R19.2 | Present summary, path groups, ordered competing components/instruction indexes, winner emphasis, per-component counts, limitations, empty/error/busy states, and JSON copy. | `C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138`; `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65` | `src/conflicts.rs:1-27`; `src/ui.rs:1578-1598` |
| R19.3 | Tie the result to the guide/selection/root inputs that launched it and ignore obsolete completion messages. | C# dialog owns one modal run (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml.cs:54-108`); Rust validation/install already carry run identity (`src/ui.rs:17-30`). | Conflict messages currently lack identity (`src/ui.rs:17-30`; `src/ui.rs:125-126`). |
| R21.1 | Add/remove/reorder components, top-level instructions, options, and option instructions through one shared transaction API. | `C# src/ModSync.Core/Services/ComponentEditorService.cs:81-220`; `C# src/ModSync.Core/Services/ComponentManagerService.cs:157-201` | `src/editor.rs:4-88`; `src/model.rs:45-84` |
| R21.2 | Edit metadata, dependencies, restrictions, install-before/after, action fields, overwrite, option fields, resource URLs, and executable fields without breaking identities/references. | `C# src/ModSync.Core/Services/ComponentEditorService.cs:224-311`; `C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:107-340`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:75-169` | `src/model.rs:26-84`; `src/model.rs:320-390`; `src/ui.rs:2666-2843` |
| R21.3 | Parse and preview raw component replacement, confirmed URL-sourced guides, and configurable Markdown/regex import before application. | `C# src/ModSync.Core/Services/ComponentEditorService.cs:18-79`; `C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:321-388` | Safe URL fetch/whole-guide load exists at `src/protocol.rs:62-152`; the editor integration gap is recorded at `COMPATIBILITY.md:125`. |
| R21.4 | Enumerate a component's registered resources read-only and project chosen sources/destinations back to supported variables. | `C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`; `C# src/ModSync.GUI/Dialogs/ModFilesBrowserDialog.axaml.cs:75-178` | Resource registrations exist at `src/model.rs:26-84`; native browser gap is recorded at `COMPATIBILITY.md:125`. |
| R21.5 | Save only after a successful full-guide validation and expose the same mutation semantics through a scriptable CLI surface. | The C# main window wires GUI events to editor services (`C# src/ModSync.GUI/MainWindow.axaml.cs:4749-4799`; `C# src/ModSync.GUI/MainWindow.axaml.cs:5236-5350`; `C# src/ModSync.GUI/MainWindow.axaml.cs:5890-5934`; `C# src/ModSync.GUI/MainWindow.axaml.cs:6238-6373`). | Atomic save/validation exist (`src/model.rs:178-203`; `src/model.rs:231-300`; `src/model.rs:320-390`), but current edit helpers are not transactional (`src/editor.rs:4-88`). |
| R22.1 | Check unique provider resources, respect rate limits, preserve successful partial results, and apply version metadata through one shared service. | `C# src/ModSync.Core/Services/ModUpdateCheckService.cs:50-203` | `src/updates.rs:351-444` |
| R22.2 | Persist and reload a timestamped structured result without treating that snapshot as authoritative over the guide. | `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:17-145` | `src/updates.rs:8-30`; `src/updates.rs:450-463` |
| R22.3 | Show check-now/busy, badges, last result, updates, errors, rate state, and an explicit retry action; reject obsolete completions. | `C# src/ModSync.GUI/Services/MenuBuilderService.cs:411-415`; `C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104` | `src/ui.rs:17-30`; `src/ui.rs:192-210`; `src/ui.rs:920-933`; `src/ui.rs:1251-1255` |

## Key technical decisions

### KTD-1 — shared editor operations are the unit of mutation

Add an `EditOperation` model and `apply_operations(&Guide, &[EditOperation]) -> Result<Guide>`-style production API in `src/editor.rs`. It clones the guide, resolves targets by stable component GUID plus explicit instruction/option positions, applies operations in order, invokes the complete model validator, and returns a replacement guide only on success (`src/model.rs:178-203`; `src/model.rs:320-390`). The operation set covers component fields/order/removal, dependency/restriction/install-order lists, resource URLs, top-level instruction CRUD/order/fields, option CRUD/order/fields, and nested instruction CRUD/order/fields because those are all present in the oracle surface (`C# src/ModSync.Core/Services/ComponentEditorService.cs:81-311`; `C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:107-340`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:75-169`).

The existing one-off helpers may remain as thin compatibility wrappers, but GUI and new CLI work must call the transaction API so validation cannot diverge by presentation (`src/editor.rs:4-88`; `src/ui.rs:2666-2843`). Component removal must account for dependencies, restrictions, and install-before/after references, not only dependencies (`src/editor.rs:37-51`; `src/model.rs:320-390`).

### KTD-2 — import preview is pure until the user applies it

Put raw-component and Markdown/regex parsing into a presentation-neutral editor import module owned by the F21 shared-library lane. Its preview result contains the proposed component(s), matched/unmatched lines, warnings, and normalized mod links; applying the preview becomes an `EditOperation`, so cancel cannot mutate the guide (`C# src/ModSync.Core/Services/ComponentEditorService.cs:18-79`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Services/FileLoadingService.cs:273-335`). A URL import must obtain a confirmed `Guide` through the existing bounded, redirect-checked `protocol` functions and then enter the same preview; it must not add a second downloader or silently replace the guide (`src/protocol.rs:62-152`; `src/ui.rs:1755-1791`; `C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`). Reuse `Guide::parse` for complete supported guide documents and make unsupported export formats explicit (`src/model.rs:97-176`; `src/model.rs:231-300`).

### KTD-3 — file browsing has a shared inventory and native selection UI

The shared F21 layer returns a read-only tree of files from the selected component's resource registrations plus safe variable-relative candidate strings; it performs no GUI selection and launches no external program (`src/model.rs:26-84`; `C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`). The native UI owns expansion, multi-selection, source/destination mode, and confirmation, then submits selected values through `EditOperation` (`C# src/ModSync.GUI/Dialogs/ModFilesBrowserDialog.axaml.cs:75-178`). Reject traversal, unsupported resource kinds, missing archives, and destinations outside supported variables rather than synthesizing a path (`src/model.rs:320-390`; `COMPATIBILITY.md:125`).

### KTD-4 — conflict projection is immutable and content-neutral

Add a small presentation projection in `src/conflicts.rs` or a new conflict-view module that deterministically sorts component count rows and formats writer labels without changing the underlying report (`src/conflicts.rs:1-27`; `C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138`). Keep the VFS write preview as the parity target; do not add a byte/content diff because the oracle dialog shows paths and writers only (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`). Attach the UI input stamp and a monotonic run identifier to the job/result, matching the stale-result pattern already used by validation/install (`src/ui.rs:17-30`).

### KTD-5 — stored update results are evidence, not guide authority

Extend the stored summary with schema version, checked timestamp, input identity, and structured update/error/rate data, while retaining atomic save/load (`src/updates.rs:8-30`; `src/updates.rs:450-463`; `src/recovery.rs:295-302`). Startup may display a valid snapshot, but it must not overwrite current component metadata; only a newly completed check applied to the matching guide identity may replace the guide (`src/ui.rs:192-210`; `src/ui.rs:1622-1696`). Corrupt, unknown-version, or mismatched snapshots become a visible non-fatal stored-result warning, consistent with the oracle's nullable corrupt-load behavior (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:127-145`).

### KTD-6 — update UI adds no second provider client

The UI owns only state and presentation: check/retry commands, busy state, last-checked text, update rows, error/rate summaries, badges, and stored-result warnings (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104`). Provider calls, headers, timeouts, 429 retry, result application, and persistence remain in `src/updates.rs` (`src/updates.rs:126-319`; `src/updates.rs:351-463`).

## File ownership and serialization

| Lane | Exclusive ownership | May start when | Must not touch |
| --- | --- | --- | --- |
| L1 F21 shared actions | `src/editor.rs` and any new `src/editor_import.rs` / `src/editor_browser.rs` modules; corresponding exports in `src/lib.rs` only during a scheduled handoff | Immediately after confirming no other editor owner; it does not need `ui.rs` or `engine.rs` (`src/editor.rs:4-88`; `src/lib.rs:9`). | `src/ui.rs`, `src/engine.rs`, update/conflict modules |
| L2 F22 shared snapshot | `src/updates.rs`; its schema/fixtures under the eventual probe directory | May run independently of L1 if file ownership remains disjoint (`src/updates.rs:8-30`; `src/updates.rs:450-463`). | `src/ui.rs`, `src/engine.rs`, editor/conflict modules |
| L3 F19 projection | `src/conflicts.rs` or one new projection module plus the associated `src/lib.rs` handoff | May run independently of L1/L2, but `src/lib.rs` edits are serialized with L1 (`src/conflicts.rs:1-84`; `src/lib.rs:44`). | `src/ui.rs`, `src/engine.rs` |
| L4 CLI adapter | `src/main.rs` for the production `guide-edit`/inspection commands; command documentation owned by this lane | After L1's shared API is reviewed (`src/main.rs:655-693`). | `src/ui.rs`, `src/engine.rs` |
| L5 UI leaf modules | New GUI-tools view/state files only, with no root wiring | After L1-L3 API shapes are frozen; current root UI concentration is documented at `src/ui.rs:1578-1598` and `src/ui.rs:2666-2843`. | `src/ui.rs`, `src/engine.rs` |
| L6 root UI integration | `src/ui.rs`, with temporary exclusive ownership of any leaf modules it wires | Only after the install-progress, completion-pages, and mod-selection `ui.rs` owners release the file in that order; then merge F19, F21, and F22 adapters serially (`src/ui.rs:17-30`; `src/ui.rs:43-210`). | `src/engine.rs`; no simultaneous `ui.rs` writer |
| L7 engine exception lane | `src/engine.rs` only | **Not planned.** Create only if a concrete shared-action gap survives review of full/remaining analysis APIs (`src/engine.rs:777-783`; `src/engine.rs:898-907`). Wait for the active engine owner and review the new contract before implementation. | All GUI files and unrelated engine behavior |
| L8 evidence/docs | `docs/parity/gui-tools-probes/**`, `COMPATIBILITY.md`, and `docs/parity/ACCEPTANCE.md` | After each production slice is available; ledger edits only after its acceptance gate passes (`docs/parity/ACCEPTANCE.md:54-57`). | Source files |

The lanes serialize shared manifest/export files such as `src/lib.rs` even when their implementation files are disjoint, because the current public modules are declared centrally (`src/lib.rs:9`; `src/lib.rs:40`; `src/lib.rs:44`).

## Execution slices

### Slice 1 — freeze contracts and probe corpus

Owner: evidence/docs lane only.

1. Create `docs/parity/gui-tools-probes/manifest.json` with Rust/C# SHAs, input hashes, expected domain, command, exit status, and artifact hashes; the ledger requires domain-specific native/differential evidence (`docs/parity/ACCEPTANCE.md:54-57`).
2. Copy only redistributable, minimal guide/archive inputs into domain subdirectories, or record external immutable paths plus hashes when copying is not permitted; conflict and validation probes must not mutate game/mod inputs (`src/engine.rs:777-783`; `src/model.rs:320-390`).
3. Record expected C# behavior from the pinned paths, including the deliberate Rust divergence that stops conflict analysis on failure (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:115-160`; `src/engine.rs:777-783`).

Exit gate: every planned assertion maps to R19.1-R22.3 and an immutable input; no source edit begins with an undefined expected result (`docs/parity/ACCEPTANCE.md:54-57`).

### Slice 2 — F21 shared editing transaction

Owner: L1 exclusively.

1. Introduce typed operations for all component, reference/order, instruction, option, nested-instruction, resource URL, and executable fields represented by the model (`src/model.rs:26-84`).
2. Resolve component targets by GUID and validate bounds for ordered children; preserve operation order and report the first failing operation with its index, matching the repository's ordered/stop-on-failure boundary (`src/model.rs:178-203`; `src/model.rs:320-390`).
3. Apply to a clone, run full validation, and return the clone only on success; route old helper entry points through the new primitives or mark them internal (`src/editor.rs:4-88`).
4. Implement raw-component, confirmed URL-guide, and Markdown/regex import preview/application without mutating the guide before confirmation; URL retrieval delegates to `protocol` (`src/protocol.rs:62-152`; `C# src/ModSync.Core/Services/ComponentEditorService.cs:18-79`; `C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:321-388`).
5. Implement resource-backed read-only browsing and safe variable-relative path projection (`C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`; `C# src/ModSync.GUI/Dialogs/ModFilesBrowserDialog.axaml.cs:75-178`).

Review gate: a reviewer traces every model field to an operation or an explicit unsupported verdict, and traces every failure path to unchanged input plus a typed error (`src/model.rs:26-84`; `src/model.rs:320-390`).

### Slice 3 — direct production CLI editor probes

Owner: L4, after Slice 2.

1. Add a scriptable production command that reads a guide plus a JSON operation list, invokes the shared transaction once, and writes to an explicit output path atomically (`src/model.rs:231-300`; `src/recovery.rs:295-302`).
2. Add read-only production commands or modes for local/raw import preview and resource inventory so the same data supplied to GUI controls can be captured without a custom Rust harness; use existing `protocol-open` output as the URL-sourced preview input rather than duplicating retrieval (`src/main.rs:73`; `src/main.rs:597-599`; `src/protocol.rs:140-152`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`).
3. Run successful and failing operation sequences against copies in isolated temporary directories; hash the input before/after a rejected sequence to prove no mutation (`src/model.rs:320-390`).
4. Parse the successful output again with the production CLI and compare component GUIDs, references, order, options, nested instructions, resource URLs, and executable fields to the operation ledger (`src/model.rs:26-84`; `src/model.rs:97-203`).

Evidence paths:

- `docs/parity/gui-tools-probes/f21/manifest.json`
- `docs/parity/gui-tools-probes/f21/operations.json`
- `docs/parity/gui-tools-probes/f21/input.*`
- `docs/parity/gui-tools-probes/f21/output.*`
- `docs/parity/gui-tools-probes/f21/rejected-input-before.sha256`
- `docs/parity/gui-tools-probes/f21/rejected-input-after.sha256`
- `docs/parity/gui-tools-probes/f21/import-preview.json`
- `docs/parity/gui-tools-probes/f21/resource-inventory.json`

Exit gate: every shared edit succeeds or fails through the production CLI with stable artifacts; this is necessary but not sufficient for the native F21 ledger gate (`docs/parity/ACCEPTANCE.md:56`).

### Slice 4 — F19 immutable presentation model

Owner: L3 exclusively.

1. Project `conflicts::Report` into deterministic summary, conflict-row, writer-row, winner, component-count, limitation, and JSON-copy values without performing analysis again (`src/conflicts.rs:1-84`).
2. Preserve instruction indexes and GUIDs in the projection even where the C# presenter shows only display labels, because Rust already records those facts and they make receipts auditable (`src/conflicts.rs:1-27`; `C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-64`).
3. Do not add byte preview or catch-and-continue semantics (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:115-160`).

Review gate: for a fixed report JSON, projection ordering and winner/count output are deterministic and no VFS or filesystem action occurs in the projection layer (`src/conflicts.rs:60-84`).

### Slice 5 — F22 timestamped stored result

Owner: L2 exclusively.

1. Version the summary schema and add checked time plus guide/input identity while preserving updates, errors, checked count, and rate state (`src/updates.rs:8-30`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:17-46`).
2. Make loading distinguish absent, valid, corrupt, unsupported-version, and identity-mismatch results; none may mutate the guide (`src/updates.rs:450-463`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:127-145`).
3. Keep provider calls and metadata application unchanged unless a concrete parity defect is found; the existing client already owns timeout, headers, validation, 429 retry, rate capture, and result application (`src/updates.rs:126-319`; `src/updates.rs:351-444`).
4. Define a stale-result merge rule: apply a completed check only when its captured input identity matches; otherwise retain it as an inspectable obsolete result and leave the current guide unchanged (`src/ui.rs:17-30`; `src/ui.rs:192-210`).

Review gate: stored data is round-trippable and corrupt/old/mismatched data produces a typed display state, never a guide mutation (`src/recovery.rs:295-302`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:127-145`).

### Slice 6 — UI leaf views without `ui.rs`

Owner: L5 exclusively.

1. Build a conflict view from the immutable projection with busy, success, empty, limited, and error states; include component counts, competing writers, winner emphasis, and JSON copy (`C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138`; `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`).
2. Build editor panels for component metadata/references/order, instruction action-specific fields, option metadata/references, nested instructions, raw/regex import preview, and resource source/destination browsing (`C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:89-340`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:61-169`; `C# src/ModSync.GUI/Dialogs/RegexImportDialog.axaml.cs:52-133`).
3. Build an update-results panel with check/retry intent messages, last checked time, badges, update details, errors, rate state, and stored-result warning (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104`).
4. Leaf views emit intents and render immutable state only; they do not invoke `engine`, mutate `Guide`, call Nexus, enumerate archives, or write snapshots (`src/engine.rs:777-783`; `src/editor.rs:4-88`; `src/updates.rs:351-463`).

Review gate: the leaf API can be wired by messages/state without giving a widget direct ownership of installer, editor, filesystem, or provider services (`src/ui.rs:17-30`; `src/ui.rs:43-103`).

### Slice 7 — serialized `ui.rs` integration

Owner: L6 only, after the pre-existing `ui.rs` sequence releases the file.

1. Integrate F19 first: capture input stamp plus run ID, show the dedicated view, and ignore a result whose identity no longer matches (`src/ui.rs:17-30`; `src/ui.rs:125-126`; `src/ui.rs:1578-1598`).
2. Integrate F22 second: load stored summary at startup, retain typed result state, preserve current guide on corrupt/mismatched/stale results, and keep badges derived from guide metadata (`src/ui.rs:43-103`; `src/ui.rs:192-210`; `src/ui.rs:1251-1255`; `src/ui.rs:1622-1696`).
3. Integrate F21 last: replace direct mutations with shared `EditOperation` submissions and wire import/browser intents; keep save explicit and atomic (`src/ui.rs:2666-2843`; `src/model.rs:231-300`).
4. At each substep, remove superseded inline controls/state rather than retaining two paths for the same action (`src/ui.rs:1578-1598`; `src/ui.rs:2666-2843`).

Review gate: one owner rebases after each preceding `ui.rs` lane, reviews the complete message/state diff for stale-job handling, and hands off only after the three adapters call shared actions exclusively (`src/ui.rs:17-30`; `src/ui.rs:43-210`).

### Slice 8 — direct conflict probes and native F19 acceptance

Owner: L8; no Rust test or custom Rust harness.

1. Run the production CLI conflict command against an isolated copied fixture for: no conflict, two writers, three writers, repeated writes by one component, selection-order reversal, remaining-phase analysis, and an unsupported external effect (`src/conflicts.rs:37-84`; `src/engine.rs:898-907`; `src/engine.rs:950-1043`).
2. Hash the game/mod fixture trees before and after and require identical manifests, proving the preview did not mutate them (`src/engine.rs:777-783`).
3. Run the native GUI against the same fixture and capture the summary, path rows, all competing components/instruction indexes, winner, counts, limitations, empty state, error state, and copied JSON (`C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138`; `docs/parity/ACCEPTANCE.md:54`).
4. Start a run, change the input selection, then complete the old run and capture that the obsolete result is rejected (`src/ui.rs:17-30`; `src/ui.rs:125-126`).

Evidence paths:

- `docs/parity/gui-tools-probes/f19/manifest.json`
- `docs/parity/gui-tools-probes/f19/cli-report.json`
- `docs/parity/gui-tools-probes/f19/tree-before.sha256`
- `docs/parity/gui-tools-probes/f19/tree-after.sha256`
- `docs/parity/gui-tools-probes/f19/native-summary.png`
- `docs/parity/gui-tools-probes/f19/native-empty.png`
- `docs/parity/gui-tools-probes/f19/native-limitation.png`
- `docs/parity/gui-tools-probes/f19/stale-run.json`

Exit gate: F19 may move from partial only when report semantics and the native dialog receipt meet `docs/parity/ACCEPTANCE.md:54`; prior evidence at `COMPATIBILITY.md:635-639` remains supporting history rather than proof of the new UI.

### Slice 9 — native F21 acceptance

Owner: L8 after Slice 7.

1. In the native app, perform every operation represented by the production CLI ledger: component fields/order, all four reference lists, resource URLs, executable fields, top-level instruction fields/order, options, and nested instructions (`src/model.rs:26-84`; `C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:89-340`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:61-169`).
2. Exercise raw replacement, confirmed and declined URL import, successful and rejected regex imports, source file/folder browsing, destination browsing, cancel paths, and save/reopen (`src/protocol.rs:62-152`; `C# src/ModSync.Core/Services/ComponentEditorService.cs:18-79`; `C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`; `C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`).
3. Compare the reopened native output with the successful CLI artifact by GUID, order, references, options/instructions, and serialized supported fields; record any presentation-only difference separately (`src/model.rs:26-84`; `src/model.rs:97-203`).
4. Attempt removal/reorder/import operations that would create missing references or cycles and capture the visible error plus unchanged file hash (`src/model.rs:320-390`).

Evidence paths:

- `docs/parity/gui-tools-probes/f21/native-actions.json`
- `docs/parity/gui-tools-probes/f21/native-editor.png`
- `docs/parity/gui-tools-probes/f21/native-option-editor.png`
- `docs/parity/gui-tools-probes/f21/native-import-preview.png`
- `docs/parity/gui-tools-probes/f21/native-file-browser.png`
- `docs/parity/gui-tools-probes/f21/native-output.*`
- `docs/parity/gui-tools-probes/f21/native-output.sha256`
- `docs/parity/gui-tools-probes/f21/reopen-inspection.json`

Exit gate: F21 closes only when every supported editor action round-trips natively without identity/reference corruption, exactly as required by `docs/parity/ACCEPTANCE.md:56`.

### Slice 10 — direct update probes and native F22 acceptance

Owner: L8 after Slices 5 and 7.

1. Use the production `update-apply` command for deterministic offline metadata transitions and run the unmodified production `update-check` CLI/native app through an acceptance-owned, network-isolated HTTPS replay proxy with immutable response files; do not add a test-only application endpoint, Rust test, or custom Rust harness (`src/main.rs:655-693`; `src/updates.rs:126-444`).
2. Capture first-check baseline, equal version, changed version, blank version, per-resource error, one-429 retry, exhausted-rate early stop, and partial-success artifacts; drive metadata-only cases through `update-apply` and transport/rate cases through the controlled proxy (`src/updates.rs:282-319`; `src/updates.rs:327-444`; `C# src/ModSync.Core/Services/ModUpdateCheckService.cs:50-203`).
3. Restart the native app and prove a valid stored result is visible, a corrupt/unknown-version snapshot produces a non-fatal warning, and neither case overwrites the loaded guide (`src/updates.rs:450-463`; `src/ui.rs:1622-1696`).
4. Capture check/retry, busy, update badge/detail, no-update, error, rate-state, stale-result, and stored-result states in the native UI (`src/ui.rs:920-933`; `src/ui.rs:1251-1255`; `docs/parity/ACCEPTANCE.md:57`).
5. Record the disposable proxy configuration, response hashes, certificate fingerprint/trust scope, request transcript with secrets redacted, production executable hash, commands, and exit statuses; if the controlled network cannot exercise the production binary transparently, leave F22 unproved rather than adding a product-only test seam or substituting source inspection (`docs/parity/ACCEPTANCE.md:57`; `src/updates.rs:126-319`).

Evidence paths:

- `docs/parity/gui-tools-probes/f22/manifest.json`
- `docs/parity/gui-tools-probes/f22/offline-transitions.json`
- `docs/parity/gui-tools-probes/f22/update-summary.json`
- `docs/parity/gui-tools-probes/f22/reload-result.json`
- `docs/parity/gui-tools-probes/f22/corrupt-result.json`
- `docs/parity/gui-tools-probes/f22/stale-run.json`
- `docs/parity/gui-tools-probes/f22/native-badges.png`
- `docs/parity/gui-tools-probes/f22/native-results.png`
- `docs/parity/gui-tools-probes/f22/native-rate-error.png`
- `docs/parity/gui-tools-probes/f22/controlled-provider/responses/`
- `docs/parity/gui-tools-probes/f22/controlled-provider/proxy-config.json`
- `docs/parity/gui-tools-probes/f22/controlled-provider/requests.redacted.jsonl`
- `docs/parity/gui-tools-probes/f22/controlled-provider/receipt.json`

Exit gate: F22 remains partial until controlled API, persistence, badge, action, and native-state evidence satisfies `docs/parity/ACCEPTANCE.md:57`; offline transition receipts alone do not close the domain.

### Slice 11 — final review and ledger update

Owner: L8 after all domain gates.

1. Review shared-action versus presentation boundaries: GUI code must not duplicate VFS execution, guide validation/mutation, archive inventory, provider calls, or snapshot serialization (`src/engine.rs:777-783`; `src/model.rs:320-390`; `src/updates.rs:351-463`).
2. Review source ownership history to ensure no implementation commit mixed `src/ui.rs` with an unreviewed `src/engine.rs` change (`src/engine.rs:898-907`; `src/ui.rs:17-30`).
3. Run the repository's normal formatting/lint/build gates only when the implementation phase and host capacity authorize them; record exact command, exit status, target directory, and commit rather than treating a source review as a build (`COMPATIBILITY.md:121-125`).
4. Update `COMPATIBILITY.md` with the exact supported and unsupported boundaries, then mark only the individually proved F19/F21/F22 ledger rows (`COMPATIBILITY.md:121-125`; `docs/parity/ACCEPTANCE.md:54-57`).
5. Have one reviewer audit each claim against the manifest hashes and native evidence; a file's existence is not proof unless its manifest ties it to the reviewed commit and command (`docs/parity/ACCEPTANCE.md:54-57`).

## Review gates

| Gate | Reviewer question | Required evidence | Failure action |
| --- | --- | --- | --- |
| G1 shared API | Can CLI and GUI perform the action through the same production API with no UI dependency? | Production command/output receipts for editor and updates; deterministic conflict projection (`src/main.rs:655-693`; `src/conflicts.rs:60-84`). | Return to L1/L2/L3; do not patch around it in `ui.rs`. |
| G2 invariant safety | Does every rejected edit leave identity, references, ordering, and bytes unchanged? | Before/after hashes plus model validator output (`src/model.rs:178-203`; `src/model.rs:320-390`). | Fix the transaction boundary before UI wiring. |
| G3 read-only safety | Are conflict preview and resource browsing non-mutating and free of external launches? | Tree manifests and command record (`src/engine.rs:777-783`; `C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`). | Reject the path; do not waive it in presentation. |
| G4 async identity | Can an old conflict/update completion alter current state? | Stale-run receipts for both domains (`src/ui.rs:17-30`; `src/ui.rs:125-126`; `src/ui.rs:192-210`). | Keep the UI slice open until obsolete completions are ignored. |
| G5 native parity | Can a user reach and complete every required action in the native app? | Domain screenshots plus action ledger and reopened artifacts (`docs/parity/ACCEPTANCE.md:54-57`). | Leave the ledger row partial. |
| G6 external semantics | Were provider status, rate, error, and persistence claims exercised against controlled responses? | Controlled-provider manifest and outputs (`docs/parity/ACCEPTANCE.md:57`; `src/updates.rs:126-319`). | Mark F22 unproved; source presence is insufficient. |
| G7 compatibility truth | Do docs state exactly what is supported and what remains unproved? | Reviewed diffs in `COMPATIBILITY.md` and `docs/parity/ACCEPTANCE.md` (`COMPATIBILITY.md:121-125`; `docs/parity/ACCEPTANCE.md:54-57`). | Revert only the premature status claim. |

## Open questions with defaults

1. **Does “conflict preview” require file-content or byte diffs?** Default: **no**. The pinned dialog shows destination paths and competing writers, while the analyzer is described as a simulated VFS write preview (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:19-30`).
2. **Should Rust continue after a failed instruction because C# does?** Default: **no**. Preserve Rust's stop-on-failure behavior and surface the error; the C# continuation can yield an incomplete report (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:115-160`; `src/engine.rs:777-783`).
3. **Should a stored update result overwrite current guide metadata on startup?** Default: **no**. Display it as a timestamped snapshot; badges remain derived from the loaded guide until a matching fresh check applies (`src/updates.rs:397-444`; `src/ui.rs:1251-1255`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:17-46`).
4. **What is the CLI editing contract?** Default: one explicit JSON operation list applied transactionally to one input and one distinct output path, because ordered operations and unchanged-on-failure behavior are directly inspectable (`src/model.rs:231-300`; `src/model.rs:320-390`).
5. **Are configurable Markdown/regex profiles required or is the default parser enough?** Default: port the configurable preview/apply semantics before claiming F21, because the pinned editor exposes profile editing, reset, preview, and confirmation (`C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:41-61`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:321-388`).
6. **May a hidden test-only provider URL be added for F22?** Default: **no**. Exercise the unchanged production CLI and native app through an external, disposable controlled HTTPS replay proxy, record its immutable inputs and trust scope, and leave the gate unproved if transparent replay is unavailable (`src/main.rs:655-693`; `src/updates.rs:126-319`; `docs/parity/ACCEPTANCE.md:57`).
7. **When may GUI-tools touch `src/ui.rs`?** Default: only after install-progress, completion-pages, and mod-selection release their serialized ownership, followed by one GUI-tools owner integrating F19, F22, and F21 in the order defined in Slice 7 (`src/ui.rs:17-30`; `src/ui.rs:43-210`; `src/ui.rs:2666-2843`).
8. **When may GUI-tools touch `src/engine.rs`?** Default: never in the planned work; existing full and remaining conflict APIs are sufficient. A concrete missing contract must be reviewed and queued behind the active engine owner before that default changes (`src/engine.rs:777-783`; `src/engine.rs:898-907`).

## Definition of done

- F19 is done only when shared conflict semantics, stale-result handling, the dedicated native view, immutable tree receipts, and native writer/winner/count/limitation evidence satisfy the ledger (`src/conflicts.rs:1-84`; `docs/parity/ACCEPTANCE.md:54`).
- F21 is done only when every shared edit/import/browser action is reachable through production CLI and native UI, failures are non-mutating, and save/reopen preserves identities and references (`src/model.rs:178-203`; `src/model.rs:231-300`; `src/model.rs:320-390`; `docs/parity/ACCEPTANCE.md:56`).
- F22 is done only when update checks, timestamped storage/reload, badges, detailed native states, stale-result rejection, and controlled provider behavior satisfy the ledger (`src/updates.rs:351-463`; `docs/parity/ACCEPTANCE.md:57`).
- The overall slice is done only when `COMPATIBILITY.md` and the ledger describe the proved state without upgrading any source-only or offline-only receipt into native/external proof (`COMPATIBILITY.md:121-125`; `docs/parity/ACCEPTANCE.md:54-57`).
