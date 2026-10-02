# GUI tools parity audit (F19, F21, F22)

## Audit identity and proof boundary

- **Rust target:** `/home/brunner56/Workspaces/ModSync.rs` at `cf64f53` (the audit command was `git rev-parse --short HEAD`).
- **C# oracle:** `/home/brunner56/.codex/worktrees/5813/ModSync` at `d31ccbd3bfcddb4705917eafa6483715c166de3b` (the audit command was `git rev-parse HEAD`).
- **Ledger scope:** F19 is the conflicts UI, F21 is component/instruction/option editing, and F22 is update-check UI and stored results (`docs/parity/ACCEPTANCE.md:54-57`).
- **Existing boundary:** the Rust GUI is an adapter over shared CLI/library behavior, rather than a second installer implementation (`docs/parity/GUI-INVENTORY.md:113-116`; `src/engine.rs:898-907`).
- **Verdict vocabulary:** `implemented` means the behavior is present in production code; `proved` additionally requires the acceptance artifact named by the ledger (`docs/parity/ACCEPTANCE.md:54-57`). Source presence, a prior screenshot, or a serialized result by itself is not promoted to end-to-end parity (`docs/parity/ACCEPTANCE.md:66-73`).

All C# paths below are relative to the pinned oracle. All Rust paths are relative to this repository.

## Executive result

| Domain | Shared-library state | Presentation state | Audit verdict |
| --- | --- | --- | --- |
| F19 conflicts | The Rust engine performs an ordered VFS dry run and returns typed paths, ordered writers, winners, per-component counts, and limitations (`src/engine.rs:777-783`; `src/conflicts.rs:1-27`). | The validation page renders paths, writers, winning rows, limitations, and JSON copy, but it has no dedicated dialog or explicit stale-run guard (`src/ui.rs:17-30`; `src/ui.rs:1578-1598`). | **Partial.** The core attribution model is usable by GUI and CLI, while the C# dialog-shaped result presentation and a fresh native acceptance receipt remain incomplete (`COMPATIBILITY.md:621-639`; `docs/parity/ACCEPTANCE.md:54`). |
| F21 editor | Rust has production helpers for component and instruction add/remove/reorder plus guide parsing, validation, atomic saving, and safe whole-guide `modsync://` loading (`src/editor.rs:4-88`; `src/model.rs:97-203`; `src/model.rs:231-300`; `src/protocol.rs:62-152`). | The GUI exposes component metadata, dependency/restriction lists, and instruction CRUD/reordering, but not option CRUD, raw replacement, editor-integrated URL/regex preview, or the resource-backed file browser (`src/ui.rs:1755-1791`; `src/ui.rs:2666-2843`; `COMPATIBILITY.md:122-125`). | **Partial, with the largest parity gap.** The C# oracle exposes option editing, nested instruction editing, URL handoff, regex preview, and source/destination browsing; Rust has URL transport/whole-guide load but lacks the corresponding editor preview/merge workflow (`C# src/ModSync.Core/Services/ComponentEditorService.cs:18-220`; `C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:61-169`; `C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:89-340`). |
| F22 updates | Rust has a shared Nexus client, rate-limit handling, per-component metadata application, atomic summary persistence, and CLI commands (`src/updates.rs:88-319`; `src/updates.rs:327-463`; `src/main.rs:655-693`). | The GUI starts a background check and shows component badges, but it only logs the returned summary and does not load or retain a stored result model for inspection (`src/ui.rs:192-210`; `src/ui.rs:920-933`; `src/ui.rs:1251-1255`; `src/ui.rs:2552-2597`). | **Partial.** Check and badge behavior exists; stored-result presentation and controlled acceptance evidence do not (`COMPATIBILITY.md:123`; `docs/parity/ACCEPTANCE.md:57`). |

## Shared action versus presentation boundary

| Capability | Shared action that GUI and CLI should call | Presentation-only responsibility | Current boundary |
| --- | --- | --- | --- |
| Conflict analysis | Resolve selection order, execute the VFS preview, normalize destinations, attribute every write, and calculate winners/counts (`src/engine.rs:950-995`; `src/conflicts.rs:37-84`). | Start/cancel affordance, progress text, conflict grouping, winner emphasis, limitations, empty/error states, and copy/export controls (`C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-64`; `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`). | Correctly shared at the analysis layer; Rust presentation is still inline and materially smaller (`src/ui.rs:1578-1598`). |
| Guide editing | Apply typed mutations, enforce identity/reference/order invariants, parse import text, enumerate eligible resources, and save atomically (`src/model.rs:178-203`; `src/model.rs:231-300`; `src/model.rs:320-390`). | Choose fields/files, show validation, ask destructive confirmations, render import preview/highlighting, and manage focus/selection (`C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:89-340`; `C# src/ModSync.GUI/Dialogs/RegexImportDialog.axaml.cs:52-133`). | Rust shares only a small CRUD subset; several edits are currently performed directly in `ui.rs` (`src/editor.rs:4-88`; `src/ui.rs:2666-2843`). |
| Update checking | Perform provider calls, deduplicate resource lookups, apply metadata, persist/load a result snapshot, and expose structured outcomes (`src/updates.rs:126-319`; `src/updates.rs:351-463`). | Check-now affordance, busy/rate/error states, badges, stored-result list/detail view, and links/actions derived from structured results (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:411-415`; `C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104`). | Check execution is shared, but Rust discards the structured summary after logging and guide replacement (`src/ui.rs:192-210`). |

## F19 — conflicts UI audit

### Oracle behavior

1. The C# analyzer creates a VFS preview from the real game and mod roots, records game-directory writes case-insensitively, and treats the last distinct component writer as the winner (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:19-30`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:70-110`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:169-224`).
2. The C# result exposes conflict paths, ordered writer records, winner access, per-component conflict counts, and `HasConflicts` (`C# src/ModSync.Core/Services/Conflicts/ConflictAnalysisResult.cs:15-43`; `C# src/ModSync.Core/Services/Conflicts/FileConflict.cs:16-72`).
3. The presenter formats a summary, path rows, writer labels, `(wins)` markers, and sorted per-component count lines (`C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-64`; `C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:78-138`).
4. The dialog binds the summary and conflict rows, runs analysis with progress, and closes modally; the XAML contains no byte/content comparison panel (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml.cs:35-108`; `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`). Thus “preview” in this audit means a simulated install-write preview, not a byte diff (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:19-30`).
5. The C# analysis catches an ordinary instruction exception, records an error, and continues, so its output can be incomplete after a failed instruction (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:115-160`). This behavior is not a safe parity target because the Rust ledger requires guide order and stop-on-failure semantics (`docs/parity/ACCEPTANCE.md:66-73`).

### Rust behavior and deltas

| Concern | Rust evidence | Verdict |
| --- | --- | --- |
| Typed report | `Report` includes conflicts, per-component counts, analyzed-component count, and limitations; every `FileConflict` exposes writers and `winner_component_guid` (`src/conflicts.rs:1-27`). | Implemented. |
| Write attribution | Tracking normalizes `<<kotorDirectory>>`, compares paths case-insensitively, and collapses adjacent writes by the same component while retaining its latest instruction index (`src/conflicts.rs:37-59`). | Equivalent core intent to the oracle (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:81-110`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:213-224`). |
| Winner/count calculation | Reports retain only paths with at least two distinct writers, select the last writer, and accumulate per-component counts (`src/conflicts.rs:60-84`). | Implemented. |
| Execution order and failure | Selected components are ordered before execution; each instruction is tracked, and a failed instruction aborts the analysis (`src/engine.rs:950-1043`; `src/engine.rs:777-783`). | Intentionally stricter than C# and consistent with the Rust fail-closed boundary (`docs/parity/ACCEPTANCE.md:66-73`). |
| External effects | Instructions that cannot be modeled as VFS writes add an explicit limitation rather than silently claiming complete coverage (`src/engine.rs:1003-1043`; `src/conflicts.rs:60-84`). | Implemented with an honest proof boundary. |
| Remaining-phase analysis | The engine verifies a completed prefix before analyzing remaining work (`src/engine.rs:898-907`). | Implemented shared action. |
| GUI content | The validation page renders summary counts, limitations, path rows, component/instruction labels, winner text, and JSON copy (`src/ui.rs:1578-1598`). | Useful but not dialog parity. |
| Async identity | Validation and install messages carry run identifiers, but `Message::Conflicts` carries only a result (`src/ui.rs:17-30`); the result is assigned directly (`src/ui.rs:125-126`). | Gap: a late result can overwrite newer visible state. |
| Acceptance evidence | Compatibility records a prior native conflict proof and receipt while still classifying F19 as partial (`COMPATIBILITY.md:621-639`). | Prior evidence exists, but the final F19 ledger gate remains open (`docs/parity/ACCEPTANCE.md:54`). |

### F19 audit conclusion

The shared Rust analyzer is already strong enough to remain the single source for CLI and GUI conflict results (`src/engine.rs:777-783`; `src/engine.rs:898-907`; `src/conflicts.rs:1-84`). The missing work is chiefly a stable presentation model, stale-run protection, dialog-level interaction states, and a fresh native receipt tied to the completed UI (`src/ui.rs:17-30`; `src/ui.rs:125-126`; `src/ui.rs:1578-1598`; `docs/parity/ACCEPTANCE.md:54`). No content-diff viewer is required for C# parity because the pinned dialog presents writer attribution rather than file bytes (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`).

## F21 — component, instruction, and option editor audit

### Oracle behavior

1. The C# core service can replace a component from raw TOML and add, delete, or reorder both top-level instructions and option instructions (`C# src/ModSync.Core/Services/ComponentEditorService.cs:18-220`).
2. The same service mutates selection dependencies and restrictions for components and options (`C# src/ModSync.Core/Services/ComponentEditorService.cs:224-311`).
3. The component manager creates identifiers, removes components subject to a dependency guard, and reorders the component list (`C# src/ModSync.Core/Services/ComponentManagerService.cs:157-201`).
4. The GUI management service wraps instruction and option CRUD/reordering, while the editor controls expose action selection, source/destination, overwrite, conditional references, option metadata, and nested instructions (`C# src/ModSync.GUI/Services/InstructionManagementService.cs:15-131`; `C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:89-340`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:61-169`).
5. File/folder browsing is backed by component resources and archive enumeration, then converted into `<<modDirectory>>` paths; destination browsing is a separate operation (`C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`; `C# src/ModSync.GUI/Dialogs/ModFilesBrowserDialog.axaml.cs:75-178`).
6. The regex import dialog recomputes a real parser preview, highlights matches, supports profile edits/reset, and confirms by loading with the selected profile (`C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:41-61`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:321-388`).
7. Editor-mode Markdown loading routes through that dialog before processing imported mod links, while the getting-started import also advertises TOML, Markdown, YAML, XML, and JSON (`C# src/ModSync.GUI/Services/FileLoadingService.cs:273-335`; `C# src/ModSync.GUI/Controls/GettingStartedTab.axaml:132-137`).
8. A confirmed `modsync://` handoff downloads an instruction URL to a temporary file and loads it through the normal instruction-file path (`C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`).
9. The main window wires save, component moves, instruction CRUD/reordering, and option CRUD/reordering to these services (`C# src/ModSync.GUI/MainWindow.axaml.cs:4749-4799`; `C# src/ModSync.GUI/MainWindow.axaml.cs:5236-5350`; `C# src/ModSync.GUI/MainWindow.axaml.cs:5890-5934`; `C# src/ModSync.GUI/MainWindow.axaml.cs:6238-6373`).

### Rust behavior and deltas

| Concern | Rust evidence | Verdict |
| --- | --- | --- |
| Component CRUD/order | Shared helpers add, move, and remove components; removal checks only `Dependencies` references (`src/editor.rs:4-51`). | Partial. The helper does not cover restrictions, install-before/after references, or a full transactional invariant check (`src/model.rs:320-390`). |
| Instruction CRUD/order | Shared helpers add, move, and remove a component's top-level instructions (`src/editor.rs:53-88`). | Implemented only for top-level instructions. |
| Option editing | The model contains options, but `editor.rs` ends after top-level instruction removal and exposes no option mutation API (`src/model.rs:45-84`; `src/editor.rs:53-88`). | Missing shared action and GUI. |
| Metadata/reference editing | The model carries dependencies, restrictions, install-before/after, options, and resource registry data (`src/model.rs:26-84`); the GUI directly edits only a subset (`src/ui.rs:2666-2843`). | Partial and insufficiently centralized. |
| Parse/round trip | `Guide::parse` accepts supported serialized formats, validation checks duplicate non-empty identifiers/names, and save is atomic; Markdown export is explicitly unsupported (`src/model.rs:97-203`; `src/model.rs:231-300`). | Shared foundation exists; editor action parity and round-trip receipts do not. |
| Ordering invariants | Model validation checks dependency, restriction, install-before/after, and cycle constraints (`src/model.rs:320-390`). | Implemented validator, not yet guaranteed as one transaction around every edit. |
| Current GUI | The guide editor supports add/save, component selection/order/removal, direct component fields, dependency/restriction editing, and top-level instruction fields/order/removal (`src/ui.rs:2666-2843`). | Partial surface. |
| Raw replacement | No Rust editor API corresponds to the C# raw TOML replacement path (`src/editor.rs:4-88`; `C# src/ModSync.Core/Services/ComponentEditorService.cs:18-79`). | Missing. |
| URL/regex import | Rust already validates/fetches bounded public instruction URLs and parses a whole guide, and the GUI can confirm/apply that guide (`src/protocol.rs:62-152`; `src/ui.rs:1755-1791`; `COMPATIBILITY.md:122`). Current compatibility separately records URL/regex import as absent from the native editor (`COMPATIBILITY.md:125`). | URL transport exists; editor preview/merge and configurable regex import are missing. |
| Resource file browser | Current compatibility explicitly records the editor file browser as absent (`COMPATIBILITY.md:125`); the Rust UI edits source/destination strings directly (`src/ui.rs:2666-2843`). | Missing. |
| Acceptance evidence | F21 requires a native round trip of every supported editor action without corrupting component identity or references (`docs/parity/ACCEPTANCE.md:56`). | Not proved. |

### F21 audit conclusion

F21 needs a shared editing transaction boundary before more GUI controls are added: the current helper set mutates isolated vectors, while the model's cross-reference and ordering rules live separately (`src/editor.rs:4-88`; `src/model.rs:320-390`). The GUI must consume typed shared operations for component, instruction, option, reference, raw/regex-import, URL-sourced preview, and resource-selection changes, with URL retrieval reused from the existing protocol safety boundary and file pickers/preview rendering left in presentation code (`src/protocol.rs:62-152`; `C# src/ModSync.Core/Services/ComponentEditorService.cs:18-311`; `C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`). This is the largest of the three domains because both the shared action layer and the native presentation are incomplete (`COMPATIBILITY.md:125`; `docs/parity/ACCEPTANCE.md:56`).

## F22 — update-check UI audit

### Oracle behavior

1. The C# service iterates resource registrations, caches unique provider lookups, applies rate-limit early termination, persists outcomes, and records per-resource errors without discarding successful results (`C# src/ModSync.Core/Services/ModUpdateCheckService.cs:19-24`; `C# src/ModSync.Core/Services/ModUpdateCheckService.cs:50-160`).
2. Applying a result preserves state for a blank provider version, records check/latest values, establishes a first-check baseline, and derives availability by case-insensitive version inequality (`C# src/ModSync.Core/Services/ModUpdateCheckService.cs:163-203`).
3. The result store serializes a checked timestamp and component results and offers save/load behavior; corruption returns no snapshot after logging (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:17-75`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:77-145`).
4. The component model exposes the derived badge and provider metadata fields (`C# src/ModSync.Core/ModComponent.cs:760-782`; `C# src/ModSync.Core/ModComponent.cs:2355-2365`).
5. The GUI menu action checks for an API key, constructs the service/store, runs all-mod checks, refreshes badge notifications, and shows summary/rate/error information (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:411-415`; `C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`).
6. The pinned GUI renders an update badge in each mod list item (`C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104`). The check action calls the store's save path but does not call its load path in the menu workflow, so persisted results are durable evidence rather than a fully rehydrated C# result browser (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:127-145`).

### Rust behavior and deltas

| Concern | Rust evidence | Verdict |
| --- | --- | --- |
| Structured result | `Summary` contains checked/skipped counts, update records, errors and a `RateLimitReached` flag (`src/updates.rs:11-21`). Remaining quota is **not** on `Summary`; it lives on the client as `daily_remaining`/`hourly_remaining` (`src/updates.rs:92-93`). | Implemented, but it lacks the oracle snapshot's `CheckedUtc` timestamp (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:20`). Both sides write the same `update-check-last.json` (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:52`; `src/updates.rs:450-463`), so any schema change must be additive. |
| Provider client | The client has a bounded request timeout, version/file/MD5/key validation calls, required headers, rate-state capture, and one 429 retry (`src/updates.rs:126-319`). | Implemented shared layer. |
| Check orchestration | Check deduplicates resource lookups, stops when rate-limited, records errors, and applies each successful result to component metadata (`src/updates.rs:351-444`). | Implemented shared layer. |
| Persistence | Summary save/load uses the shared atomic-write path (`src/updates.rs:450-463`; `src/recovery.rs:295-302`). | Implemented storage primitive. |
| CLI parity | `update-apply` and `update-check` call the same shared update functions and save guide/summary outputs (`src/main.rs:655-693`). | Implemented adapter. |
| GUI execution | The GUI reads settings, starts a worker, invokes `updates::check`, saves the summary, and sends the changed guide/result to the application (`src/ui.rs:2552-2597`). | Implemented asynchronous action. |
| GUI result lifetime | The message handler replaces the guide and logs a summary, but `App` has no stored update-summary field and startup does not load the persisted summary (`src/ui.rs:43-103`; `src/ui.rs:192-210`; `src/ui.rs:1622-1696`). | Missing stored-result presentation. |
| Badge | Components render an `Update` label from stored guide metadata (`src/ui.rs:1251-1255` at `cf64f53`; `src/updates.rs:397-444`). mod-selection Q3 moves it into `src/ui/mod_selection_page.rs` as a badge beside the tier badge and moves the Nexus check to the editor surface (`docs/parity/mod-selection-plan.md:346`). | Implemented; the plan anchors on the post-T3 badge, not this line. |
| Stale completion | `Message::Updates` carries a guide and summary without the input stamp/run identity used by validation/install jobs (`src/ui.rs:17-30`; `src/ui.rs:2552-2597`). | Gap: an obsolete check can replace a newer guide. |
| Acceptance evidence | F22 requires controlled provider fixtures, persisted result checks, and native UI actions (`docs/parity/ACCEPTANCE.md:57`). | Not proved. |

### F22 audit conclusion

The Rust shared update client is more complete than its current GUI: CLI and GUI both use `updates.rs`, but only the CLI exposes the structured result as an output artifact while the GUI reduces it to a log entry and badge-mutated guide (`src/main.rs:655-693`; `src/ui.rs:192-210`; `src/ui.rs:2552-2597`). F22 therefore needs result identity, safe merge/rejection of stale completions, startup loading, and a result view; it does not need a second provider implementation (`src/updates.rs:126-463`; `docs/parity/ACCEPTANCE.md:57`).

## Gaps carried into the plan

| ID | Required change | Evidence that creates the requirement |
| --- | --- | --- |
| GT-19-1 | Add an immutable conflict presentation projection and dedicated native view. Oracle parts: summary, rows, winners, counts and the empty summaries. Rust additions, labeled as such: in-view busy, error state, limitations and JSON copy. | The oracle presenter defines summary/rows/counts/empty (`C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138`); C# busy is a separate `ProgressDialog` and there is no error or copy control (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml.cs:68-108`; `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`). Rust has only the inline subset (`src/ui.rs:1578-1598`). |
| GT-19-2 | Attach input/run identity to conflict jobs and ignore late results. | Other long-running Rust jobs carry identity while conflicts do not (`src/ui.rs:17-30`; `src/ui.rs:125-126`). |
| GT-21-1 | Make every edit a typed, all-or-nothing shared operation followed by complete guide validation. | Current mutation helpers are isolated (`src/editor.rs:4-88`) while full reference/order validation is separate (`src/model.rs:320-390`). |
| GT-21-2 | Add option and nested-instruction CRUD/reordering plus component reference/order edits. | The C# service supports these (`C# src/ModSync.Core/Services/ComponentEditorService.cs:81-311`); Rust does not (`src/editor.rs:4-88`). |
| GT-21-3 | Add shared import preview/application for raw component, URL-sourced guide, and configurable Markdown/regex input without duplicating URL security or retrieval. | The oracle has raw replacement, URL handoff, and a parser-backed profile preview (`C# src/ModSync.Core/Services/ComponentEditorService.cs:18-79`; `C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:321-388`), while Rust already owns safe instruction retrieval in `src/protocol.rs:62-152`. |
| GT-21-4 | Add read-only resource inventory and safe source/destination path projection for the native file browser. | The oracle browsing layer enumerates component resources and restores variables (`C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`; `C# src/ModSync.GUI/Dialogs/ModFilesBrowserDialog.axaml.cs:75-178`). |
| GT-22-1 | Persist a timestamped structured update snapshot and load it without letting it overwrite guide metadata. Changes to the shared file are additive only: keep `CheckedUtc` and every C# field, and load a C#-written file as valid legacy data. | Rust summary lacks a timestamp (`src/updates.rs:11-21`); the oracle snapshot contains `CheckedUtc` and uses the same file name (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:20`, `:52`). |
| GT-22-2 | Keep update results in application state and render check, busy, summary, rate, error, update-detail, and stored-result states. | Rust currently logs and discards the summary (`src/ui.rs:43-103`; `src/ui.rs:192-210`), while the oracle exposes badges and summary/error feedback (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104`). |
| GT-22-3 | Attach guide/input identity to update jobs and reject or explicitly reconcile stale results. | Update messages lack the run/input identity used by validation/install (`src/ui.rs:17-30`; `src/ui.rs:2552-2597`). |
| GT-22-4 | Extend the guarded Nexus override approved on `nxm-traversal-fix` (hidden `--nexus-api-base`; https except loopback http; explicit key only; same-origin redirects) to `update-check` and the `gui` probe args, so a plain-http fake on `127.0.0.1` can drive F22 acceptance. | The guard exists only on that branch and only for `nxm-download` (`nxm-traversal-fix:src/main.rs:91-92`, `:642-647`; `nxm-traversal-fix:src/updates.rs:159-212`); `update-check` on the checked-out HEAD builds `NexusClient::new` from settings/env (`src/main.rs:674-692`). |
| GT-PROOF-1 | Capture direct production CLI and native GUI probes, with immutable inputs, outputs, screenshots, and manifests; do not substitute Rust tests or a custom Rust harness. | The ledger demands native/differential proof (`docs/parity/ACCEPTANCE.md:54-57`) and the current compatibility boundary remains partial (`COMPATIBILITY.md:121-125`; `COMPATIBILITY.md:621-639`). |
