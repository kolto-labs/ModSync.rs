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
9. **Serialized `ui.rs` ownership.** `src/ui.rs` follows the merged order in the next section, and nothing else. GUI-tools owns steps 5, 9 and 10 of that order and may not edit `src/ui.rs` at any other time; current editor and conflict surfaces are concentrated there (`src/ui.rs:1578-1598`; `src/ui.rs:2666-2843`).
10. **Serialized `engine.rs` ownership.** The planned path does not require an engine edit because existing APIs already analyze full and remaining installs (`src/engine.rs:777-783`; `src/engine.rs:898-907`). If implementation discovers an engine requirement, stop and queue a separately reviewed engine-owned change after the active engine lane; do not fold it into a UI commit (`src/engine.rs:950-1043`).
11. **Compatibility truth follows behavior.** Update `COMPATIBILITY.md` only in the implementation slice that changes a supported behavior or proof boundary; its present F19/F21/F22 entries remain partial until the relevant evidence gates pass (`COMPATIBILITY.md:121-125`; `COMPATIBILITY.md:621-639`).
12. **No hardcoded colors or fonts in new views.** New GUI-tools leaf files take colors and fonts from the active egui theme and `TextStyle`. They copy C# geometry (order, grouping, sizes, spacing) only, never C# colors or fonts. Gate: `rg -n 'Color32::from_|Color32::[A-Z_]+\b|FontId::' <new files>` returns no hits. The gate covers only files this plan creates, because existing `src/ui/*.rs` already has 27 `Color32`/`FontId` hits (`grep -n -E 'Color32|FontId' src/ui/*.rs | wc -l` returns 27 at `e5f8e1a`). `TextStyle` stays allowed.
13. **Keyboard and accessibility for the new views belong to wizard T7.** GUI-tools leaf views give every control a visible text label and emit no custom focus handling. The focus order, accessible names, status announcements and native accessibility-tree receipts for the conflict, update-results and editor views are cells in wizard-mainwindow-plan Task 7 (step 13 below). GUI-tools does not claim them.

## Merged `ui.rs` order

This is the one binding order for every `src/ui.rs` writer across the GUI-tools and wizard/main-window plans. Install-progress has already shipped (`origin/main` `cf64f53`). Each step starts only after the previous step's owner has delivered and its review has settled.

1. completion-pages T2
2. mod-selection T3 (the Update badge moves into mod_selection_page.rs; the details pane is removed)
3. telemetry F1-F10 (branch lane-telemetry-f27, audit §5)
4. download-orchestration T2b (downloads_panel.rs)
5. gui-tools F19/F22 state only (run ids on Message::Conflicts/Updates, a kept summary, startup load)
6. wizard T2 shell (consumes downloads_panel.rs)
7. wizard T3 pages
8. wizard T4 main window/menus/Global Actions
9. gui-tools F19/F22 views
10. gui-tools F21 editor (sole owner of raw component replacement)
11. wizard T5 (the Raw tab calls the F21 API; the structured log builds on telemetry F9)
12. wizard T6 summary and spoiler variants
13. wizard T7, one keyboard/accessibility pass over both plans' modules

**`src/main.rs` writer order.** The `ui.rs` order does not cover `src/main.rs`, which has four writers. They go in this order: (a) merge of `nxm-traversal-fix`, which adds the hidden `--nexus-api-base` guard to `nxm-download` (`nxm-traversal-fix:src/main.rs:91-92`, `:642-647`); (b) telemetry F7, logging init before dispatch (`lane-telemetry-f27:docs/parity/telemetry-csharp-audit.md:114`); (c) download-orchestration 2b `Gui` probe args (`docs/parity/download-orchestration-plan.md:365`); (d) GUI-tools L4, which lands the update-check guard, the `gui` Nexus probe args and the conflict projection output inside step 5, then the `guide-edit` commands before step 10.

**Dependency on `nxm-traversal-fix`.** The guarded `NexusClient::with_base_url` exists only on that branch (`nxm-traversal-fix:src/updates.rs:159-212`). At the checked-out HEAD `e5f8e1a`, `with_base_url` has no scheme or redirect guard (`src/updates.rs:131-141`). The F22 guard work in L2/L4 starts only after that branch merges.

## C# audit translated into requirements

| Requirement | Required behavior | Oracle referent | Rust starting point |
| --- | --- | --- | --- |
| R19.1 | Run a non-mutating, ordered VFS preview that attributes each game-path write and chooses the last distinct component writer. | `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:19-30`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:70-110`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:169-224` | `src/engine.rs:777-783`; `src/conflicts.rs:37-84` |
| R19.2 | **Oracle:** summary line, path rows, ordered competing writers, `(wins)` emphasis, sorted per-component counts, and the two empty summaries (no selected mods; no conflicts). A busy state exists in C# only as a separate `ProgressDialog` shown before the result dialog. **Rust additions, not oracle parity:** in-view busy state, an error state (C# has none; exceptions propagate), the limitations list, instruction indexes/GUIDs, and JSON copy. Receipts label each addition as such. | Summary/rows/counts/empty: `C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138` (empty at `:30-37`); busy: `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml.cs:68-80`; no error path: `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml.cs:94-108`; no copy control: `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65` | `src/conflicts.rs:1-27`; `src/ui.rs:1578-1598` |
| R19.3 | Tie the result to the guide/selection/root inputs that launched it and ignore obsolete completion messages. | C# dialog owns one modal run (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml.cs:54-108`); Rust validation/install already carry run identity (`src/ui.rs:17-30`). | Conflict messages currently lack identity (`src/ui.rs:17-30`; `src/ui.rs:125-126`). |
| R21.1 | Add/remove/reorder components, top-level instructions, options, and option instructions through one shared transaction API. | `C# src/ModSync.Core/Services/ComponentEditorService.cs:81-220`; `C# src/ModSync.Core/Services/ComponentManagerService.cs:157-201` | `src/editor.rs:4-88`; `src/model.rs:45-84` |
| R21.2 | Edit metadata, dependencies, restrictions, install-before/after, action fields, overwrite, option fields, resource URLs, and executable fields without breaking identities/references. | `C# src/ModSync.Core/Services/ComponentEditorService.cs:224-311`; `C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:107-340`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:75-169` | `src/model.rs:26-84`; `src/model.rs:320-390`; `src/ui.rs:2666-2843` |
| R21.3 | Parse and preview raw component replacement, confirmed URL-sourced guides, and configurable Markdown/regex import before application. | `C# src/ModSync.Core/Services/ComponentEditorService.cs:18-79`; `C# src/ModSync.GUI/Services/ModSyncHandoffService.cs:130-170`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:321-388` | Safe URL fetch/whole-guide load exists at `src/protocol.rs:62-152`; the editor integration gap is recorded at `COMPATIBILITY.md:125`. |
| R21.4 | Enumerate a component's registered resources read-only and project chosen sources/destinations back to supported variables. | `C# src/ModSync.GUI/Services/InstructionBrowsingService.cs:33-214`; `C# src/ModSync.GUI/Dialogs/ModFilesBrowserDialog.axaml.cs:75-178` | Resource registrations exist at `src/model.rs:26-84`; native browser gap is recorded at `COMPATIBILITY.md:125`. |
| R21.5 | Save only after a successful full-guide validation and expose the same mutation semantics through a scriptable CLI surface. | The C# main window wires GUI events to editor services (`C# src/ModSync.GUI/MainWindow.axaml.cs:4749-4799`; `C# src/ModSync.GUI/MainWindow.axaml.cs:5236-5350`; `C# src/ModSync.GUI/MainWindow.axaml.cs:5890-5934`; `C# src/ModSync.GUI/MainWindow.axaml.cs:6238-6373`). | Atomic save/validation exist (`src/model.rs:178-203`; `src/model.rs:231-300`; `src/model.rs:320-390`), but current edit helpers are not transactional (`src/editor.rs:4-88`). |
| R22.1 | Check unique provider resources, respect rate limits, preserve successful partial results, and apply version metadata through one shared service. | `C# src/ModSync.Core/Services/ModUpdateCheckService.cs:50-203` | `src/updates.rs:351-444` |
| R22.2 | Persist and reload a timestamped structured result without treating that snapshot as authoritative over the guide. | `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:17-145` | `src/updates.rs:8-30`; `src/updates.rs:450-463` |
| R22.3 | Show check-now/busy, badges, last result, updates, errors, rate state, and an explicit retry action; reject obsolete completions. The badge is not anchored on the current inline label: mod-selection Q3 moves the `Update` label into `src/ui/mod_selection_page.rs` as a small badge next to the tier badge, and moves the Nexus check to the editor surface. GUI-tools reads that badge and adds no second one. The check-now entry point is the Global Actions `Check for Nexus Updates` item, which wizard T4 places (step 8) and GUI-tools wires to its view (step 9). Rate state lives on `NexusClient` (`daily_remaining`/`hourly_remaining`), not on `Summary`; if the view shows remaining quota, L2 adds those fields to `Summary` (see KTD-5). | `C# src/ModSync.GUI/Services/MenuBuilderService.cs:411-415`; `C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104` | Badge owner: `docs/parity/mod-selection-plan.md:222-223`, `:346` (Q3); rate fields: `src/updates.rs:92-93`; `Summary`: `src/updates.rs:11-21`; messages: `src/ui.rs:17-30`; `src/ui.rs:192-210` |

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

`update-check-last.json` is shared with C#: both sides read and write the same file name in the same settings directory (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:52`; `src/updates.rs:450-463`). Schema changes are therefore **additive only**:

- Keep every C# field and name. That includes `CheckedUtc`, which Rust does not write today (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:18-33`; `src/updates.rs:11-21`). Rust starts writing `CheckedUtc` as a UTC timestamp.
- New fields are PascalCase, matching `#[serde(rename_all = "PascalCase")]` (`src/updates.rs:11-12`), and optional with `#[serde(default)]`: `SchemaVersion`, `InputIdentity`, and, if the view shows remaining quota, `DailyRemaining`/`HourlyRemaining` copied from the client (`src/updates.rs:92-93`).
- A file without `SchemaVersion` or `InputIdentity` is **valid legacy data**, not corruption or an identity mismatch. That covers a C# save, and a C# save that overwrote a Rust one and dropped the Rust fields. It is displayed as a timestamped snapshot and never applied.
- Only unparseable JSON, a `SchemaVersion` newer than the reader knows, or a present `InputIdentity` that differs from the loaded guide produce a stored-result warning.

Atomic save/load stays (`src/recovery.rs:295-302`). Startup may display a valid snapshot, but it must not overwrite current component metadata; only a newly completed check applied to the matching guide identity may replace the guide (`src/ui.rs:192-210`; `src/ui.rs:1622-1696`). The warning is visible and non-fatal, consistent with the oracle's nullable corrupt-load behavior (`C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:127-145`).

### KTD-6 — update UI adds no second provider client

The UI owns only state and presentation: check/retry commands, busy state, last-checked text, update rows, error/rate summaries, badges, and stored-result warnings (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104`). Provider calls, headers, timeouts, 429 retry, result application, and persistence remain in `src/updates.rs` (`src/updates.rs:126-319`; `src/updates.rs:351-463`).

### KTD-7 — F22 proof route reuses the approved `nxm-traversal-fix` guard

Lead ruling. The F22 proof route reuses the guarded override already approved on branch `nxm-traversal-fix`, unchanged, and extends it to the update-check path. There is no replay proxy, no TLS/CA work, and no second provider client.

The existing guard, read with `git show nxm-traversal-fix:src/updates.rs` and `:src/main.rs`:

- A hidden `--nexus-api-base` argument (`nxm-traversal-fix:src/main.rs:91-92`).
- The base URL must be https, except that loopback hosts may use http. Loopback means `localhost` or an address that parses as a loopback IP (`nxm-traversal-fix:src/updates.rs:188-206`).
- The override requires an explicit, non-empty `--nexus-mods-api-key`. It never falls back to settings or environment keys (`nxm-traversal-fix:src/main.rs:642-647`).
- Redirects are followed only within the base URL's origin (`nxm-traversal-fix:src/updates.rs:171-186`).

GUI-tools extends that guard to two places:

1. **`update-check` CLI** (L4 in `src/main.rs`, L2 in `src/updates.rs` if a helper is needed). It gets the same hidden `--nexus-api-base`, the same explicit-key rule, and one extra rule: when `--nexus-api-base` is set, `--settings-dir` is also required, so `save_summary` cannot overwrite the user's real `update-check-last.json` (`src/main.rs:674-692`; `save_summary` at `:692` writes into the settings directory).
2. **`gui` probe args** (L4 in `src/main.rs`, landing in step 5). Hidden `--nexus-api-base` plus explicit `--nexus-mods-api-key` plus a required `--settings-dir`. The GUI update worker builds its client through `NexusClient::with_base_url` only when those args are present, and otherwise through `NexusClient::new` from settings, as today (`src/ui.rs:2552-2597`). This is the same `main.rs` probe-arg handoff that download-orchestration 2b uses (`docs/parity/download-orchestration-plan.md:365`).

The acceptance fake is a plain-http server bound to `127.0.0.1` with immutable response files. Use `http://127.0.0.1:<port>/`, not a public name. It is not a Rust harness and not a product endpoint. Both overrides depend on the `nxm-traversal-fix` merge (see "Dependency on `nxm-traversal-fix`").

## File ownership and serialization

| Lane | Exclusive ownership | May start when | Must not touch |
| --- | --- | --- | --- |
| L1 F21 shared actions | `src/editor.rs` and any new `src/editor_import.rs` / `src/editor_browser.rs` modules; corresponding exports in `src/lib.rs` only during a scheduled handoff | Immediately after confirming no other editor owner; it does not need `ui.rs` or `engine.rs` (`src/editor.rs:4-88`; `src/lib.rs:9`). | `src/ui.rs`, `src/engine.rs`, update/conflict modules |
| L2 F22 shared snapshot | `src/updates.rs`; its schema/fixtures under the eventual probe directory | Snapshot schema work may run independently of L1 if file ownership remains disjoint (`src/updates.rs:8-30`; `src/updates.rs:450-463`). Anything that touches `NexusClient` construction waits for the `nxm-traversal-fix` merge. | `src/ui.rs`, `src/engine.rs`, editor/conflict modules |
| L3 F19 projection | `src/conflicts.rs` or one new projection module plus the associated `src/lib.rs` handoff | May run independently of L1/L2, but `src/lib.rs` edits are serialized with L1 (`src/conflicts.rs:1-84`; `src/lib.rs:44`). | `src/ui.rs`, `src/engine.rs` |
| L4 CLI adapter | `src/main.rs` for: the `update-check` and `gui` Nexus override args (KTD-7); a read-only conflict projection output on the existing `conflicts` command (Slice 4); and the production `guide-edit`/inspection commands (Slice 3). Command documentation is owned by this lane. | Position (d) in the `main.rs` writer order. The override args and projection output land inside queue step 5; `guide-edit` lands after L1 review and before step 10 (`src/main.rs:143-144`; `src/main.rs:655-693`). | `src/ui.rs`, `src/engine.rs` |
| L5 UI leaf modules | New GUI-tools view/state files only, with no root wiring. Subject to constraint 12 (color/font grep gate) and constraint 13 (keyboard/a11y to wizard T7). | After L1-L3 API shapes are frozen; current root UI concentration is documented at `src/ui.rs:1578-1598` and `src/ui.rs:2666-2843`. | `src/ui.rs`, `src/engine.rs` |
| L6 root UI integration | `src/ui.rs`, with temporary exclusive ownership of any leaf modules it wires | Three separate turns in the merged order: step 5 (F19/F22 state), step 9 (F19/F22 views), step 10 (F21 editor). Each turn starts only when the previous step has settled (`src/ui.rs:17-30`; `src/ui.rs:43-210`). | `src/engine.rs`; no simultaneous `ui.rs` writer |
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

4. Expose the projection read-only through the production CLI, so the gate can run under the test freeze. L4 adds a projection output mode to the existing `conflicts` command (`src/main.rs:143-144`; `src/main.rs:1087-1091`). It prints the projection JSON the view renders and copies. This is a `main.rs` handoff at `main.rs` position (d), and it writes nothing.

Review gate (runs with the production binary, no Rust test): run the projection output twice on the same copied fixture and byte-compare the outputs. Run it on a selection-order-reversed copy and check the winner and count lines change as the order predicts. Hash the fixture tree before and after, per G3. Source review confirms the projection function performs no VFS or filesystem action (`src/conflicts.rs:60-84`). In Slice 8, the native JSON-copy clipboard bytes must equal the CLI projection bytes for the same fixture.

Evidence paths: `docs/parity/gui-tools-probes/f19/projection-run1.json`, `projection-run2.json`, `projection-reversed.json`.

### Slice 5 — F22 timestamped stored result

Owner: L2 exclusively.

1. Add the KTD-5 fields additively: write `CheckedUtc`, and add optional PascalCase `SchemaVersion`, `InputIdentity` and, if the view shows quota, `DailyRemaining`/`HourlyRemaining` copied from the client. Keep every existing field and name (`src/updates.rs:11-21`; `src/updates.rs:92-93`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:18-33`).
2. Make loading distinguish absent, valid, **valid legacy** (C#-written, or C#-overwritten with Rust fields dropped), corrupt, newer-version, and identity-mismatch results. Legacy is displayed, never applied, and is not a warning. None may mutate the guide (`src/updates.rs:450-463`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:52`; `:127-145`).
3. Keep provider calls and metadata application unchanged unless a concrete parity defect is found; the existing client already owns timeout, headers, validation, 429 retry, rate capture, and result application (`src/updates.rs:126-319`; `src/updates.rs:351-444`).
4. Define a stale-result merge rule: apply a completed check only when its captured input identity matches; otherwise retain it as an inspectable obsolete result and leave the current guide unchanged (`src/ui.rs:17-30`; `src/ui.rs:192-210`).

5. After the `nxm-traversal-fix` merge, land the KTD-7 update-check override with L4: hidden `--nexus-api-base`, explicit key only, a required `--settings-dir`, and the branch's scheme/loopback/same-origin guard reused unchanged.

Review gate: stored data is round-trippable; a C#-shaped file (only the fields at `IUpdateCheckResultStore.cs:18-33`) loads as valid legacy; corrupt/newer/mismatched data produces a typed display state, never a guide mutation (`src/recovery.rs:295-302`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:127-145`).

### Slice 6 — UI leaf views without `ui.rs`

Owner: L5 exclusively.

1. Build a conflict view from the immutable projection. Oracle parts: summary, rows, component counts, competing writers, winner emphasis, and both empty summaries. Rust additions, labeled as such in receipts: in-view busy (C# uses a separate `ProgressDialog`), error state, limitations, and JSON copy (R19.2; `C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138`; `C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml.cs:68-108`).
2. Build editor panels for component metadata/references/order, instruction action-specific fields, option metadata/references, nested instructions, raw/regex import preview, and resource source/destination browsing (`C# src/ModSync.GUI/Controls/InstructionEditorControl.axaml:89-340`; `C# src/ModSync.GUI/Controls/OptionEditorControl.axaml:61-169`; `C# src/ModSync.GUI/Dialogs/RegexImportDialog.axaml.cs:52-133`).
3. Build an update-results panel with check/retry intent messages, last checked time, badges, update details, errors, rate state, and stored-result warning (`C# src/ModSync.GUI/Services/MenuBuilderService.cs:471-524`; `C# src/ModSync.GUI/Controls/ModListItem.axaml:89-104`).
4. Leaf views emit intents and render immutable state only; they do not invoke `engine`, mutate `Guide`, call Nexus, enumerate archives, or write snapshots (`src/engine.rs:777-783`; `src/editor.rs:4-88`; `src/updates.rs:351-463`).

5. Copy geometry only. Colors and fonts come from the theme and `TextStyle` (constraint 12). Keyboard/accessibility semantics beyond visible text labels are wizard T7 cells (constraint 13).

Review gate: the leaf API can be wired by messages/state without giving a widget direct ownership of installer, editor, filesystem, or provider services (`src/ui.rs:17-30`; `src/ui.rs:43-103`). The constraint-12 grep over the new files returns no hits.

### Slice 7 — serialized `ui.rs` integration, in three turns

Owner: L6 only. Slice 7 is three separate turns in the merged `ui.rs` order. Between turns, other owners hold `src/ui.rs`. Re-resolve line anchors by symbol at the start of each turn, because steps 1-4 and 6-8 move them.

**7a — queue step 5: F19/F22 state only.** No new views.

1. Add a run id and the input stamp to `Message::Conflicts` and `Message::Updates`. Drop any completion whose run id is not current (`src/ui.rs:17-30`; `src/ui.rs:125-126`; `src/ui.rs:192-210`).
2. Keep the latest update `Summary` in `App` instead of logging and discarding it. Load the stored snapshot at startup as a typed state (KTD-5). Never apply a stored snapshot to the guide (`src/ui.rs:43-103`; `src/ui.rs:1622-1696`).
3. Build the GUI update client through the KTD-7 `gui` probe args when present, and from settings otherwise (`src/ui.rs:2552-2597`).
4. Keep the existing inline conflict rendering until 7b. The badge stays wherever mod-selection T3 put it (`src/ui/mod_selection_page.rs`); 7a does not touch it.

**7b — queue step 9: F19/F22 views.**

1. Replace the inline validation-page conflict block with the Slice 6 conflict view (`src/ui.rs:1578-1598`).
2. Wire the Slice 6 update-results view to the Global Actions `Check for Nexus Updates` entry point that wizard T4 placed in step 8, and to the editor-surface check that mod-selection Q3 names (`docs/parity/mod-selection-plan.md:346`; `C# src/ModSync.GUI/Services/MenuBuilderService.cs:411-415`).
3. Remove the superseded inline controls/state rather than keeping two paths for one action.

**7c — queue step 10: F21 editor.** GUI-tools is the sole owner of raw component replacement.

1. Replace direct mutations with shared `EditOperation` submissions. Wire the import and browser intents. Keep save explicit and atomic (`src/ui.rs:2666-2843`; `src/model.rs:231-300`).
2. Publish the raw-replacement and `EditOperation` entry points that wizard T5's Raw tab (step 11) and the T4 menu items call. Neither wizard task re-implements them.
3. Remove the superseded inline editor paths.

Review gate for each turn: the owner rebases on the previous step, reviews the complete message/state diff for stale-job handling, and hands off only when the adapters in that turn call shared actions exclusively (`src/ui.rs:17-30`; `src/ui.rs:43-210`).

### Slice 8 — direct conflict probes and native F19 acceptance

Owner: L8; no Rust test or custom Rust harness.

1. Run the production CLI conflict command against an isolated copied fixture for: no conflict, two writers, three writers, repeated writes by one component, selection-order reversal, remaining-phase analysis, and an unsupported external effect (`src/conflicts.rs:37-84`; `src/engine.rs:898-907`; `src/engine.rs:950-1043`).
2. Hash the game/mod fixture trees before and after and require identical manifests, proving the preview did not mutate them (`src/engine.rs:777-783`).
3. Run the native GUI against the same fixture and capture the summary, path rows, all competing components/instruction indexes, winner, counts, limitations, empty state, error state, and copied JSON. Byte-compare the copied JSON with the Slice 4 CLI projection for the same fixture (`C# src/ModSync.GUI/Services/ConflictsDialogPresenter.cs:21-138`; `docs/parity/ACCEPTANCE.md:54`).
4. Stale-run probe, on the slow fixture below. While a job runs, the central surface is disabled (`src/ui.rs:2892`; `src/ui.rs:3101`; `src/ui.rs:3113`), so selection cannot change mid-run. The reachable trigger is: start analysis, press Activity **Stop** and confirm (`src/ui.rs:3044-3046`), change the selection, and start a new analysis while the first worker thread is still running. Capture that the first run's completion does not replace the second run's state. If the first result is undeliverable by construction (its channel was replaced), record that as the observation; the run-id check stays as the guard. If 7a makes conflict analysis non-blocking, the trigger becomes a direct selection change and the probe records that instead (`src/ui.rs:17-30`; `src/ui.rs:125-126`).

**Slow fixture.** `docs/parity/gui-tools-probes/f19/slow-fixture/`, produced by a committed shell script `make-slow-fixture.sh` (POSIX shell plus `zip`; not Rust). It writes a guide of 400 components. Each component extracts its own archive of 2,000 small files into overlapping `<<kotorDirectory>>/Override` paths. Precondition: the production `conflicts` command takes at least 10 s wall time on the probe host, and the manifest records the measured time. If it takes less, raise the component count and re-measure before running the probe.

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

1. Use the production `update-apply` command for deterministic offline metadata transitions. Run the production `update-check` CLI and the native app against a plain-http fake Nexus API on `http://127.0.0.1:<port>/`, through the KTD-7 guarded override: hidden `--nexus-api-base`, an explicit disposable `--nexus-mods-api-key`, and a disposable `--settings-dir` on every run. The fake is a small stdlib Python server serving immutable response files and logging requests. No TLS/CA work, replay proxy, Rust test, or Rust harness (`src/main.rs:674-692`; `nxm-traversal-fix:src/updates.rs:159-212`).
2. Capture first-check baseline, equal version, changed version, blank version, per-resource error, one-429 retry, exhausted-rate early stop, and partial-success artifacts. Drive metadata-only cases through `update-apply` and transport/rate cases through the fake (`src/updates.rs:282-319`; `src/updates.rs:327-444`; `C# src/ModSync.Core/Services/ModUpdateCheckService.cs:50-203`).
3. Guard probes, CLI and `gui`: `http://` to a non-loopback host is refused; `--nexus-api-base` without an explicit key is refused; a fake response redirecting to another origin is refused; a missing `--settings-dir` with the override is refused. Record the error text and prove the real settings directory's `update-check-last.json` hash is unchanged.
4. Restart the native app and prove: a valid stored result is visible; a C#-shaped legacy file is shown without a warning; a corrupt or newer-version snapshot produces a non-fatal warning; and no case overwrites the loaded guide (`src/updates.rs:450-463`; `src/ui.rs:1622-1696`).
5. Capture check/retry, busy, update badge/detail, no-update, error, rate-state, stale-result, and stored-result states in the native app launched with the `gui` probe args. Read the badge from `src/ui/mod_selection_page.rs` as placed by mod-selection T3 (`docs/parity/mod-selection-plan.md:346`; `docs/parity/ACCEPTANCE.md:57`).
6. Stale-run probe: the fake delays every response by 20 s, below the client's 60 s request timeout (`src/updates.rs:134`), so the old run completes late instead of timing out. Use the Slice 8 Stop, change input, restart trigger.
7. Record the fake's script hash, response-file hashes, bound address, request log with the key redacted, production executable hash, commands, and exit statuses. If the override cannot drive the native app, the native live-check cells stay unproved; do not substitute source inspection.

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
- `docs/parity/gui-tools-probes/f22/legacy-csharp-result.json`
- `docs/parity/gui-tools-probes/f22/guard-refusals.json`
- `docs/parity/gui-tools-probes/f22/fake-nexus/fake_nexus.py`
- `docs/parity/gui-tools-probes/f22/fake-nexus/responses/`
- `docs/parity/gui-tools-probes/f22/fake-nexus/requests.redacted.jsonl`
- `docs/parity/gui-tools-probes/f22/fake-nexus/receipt.json`

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
| G6 external semantics | Were provider status, rate, error, and persistence claims exercised against controlled responses, through the KTD-7 guarded override? | Loopback fake manifest, guard-refusal receipts and outputs (`docs/parity/ACCEPTANCE.md:57`; `nxm-traversal-fix:src/updates.rs:159-212`). | Mark F22 unproved; source presence is insufficient. |
| G8 no hardcoded style | Do new leaf views copy geometry only? | Constraint-12 grep output over the new files. | Replace literals with theme/`TextStyle` values before merge. |
| G7 compatibility truth | Do docs state exactly what is supported and what remains unproved? | Reviewed diffs in `COMPATIBILITY.md` and `docs/parity/ACCEPTANCE.md` (`COMPATIBILITY.md:121-125`; `docs/parity/ACCEPTANCE.md:54-57`). | Revert only the premature status claim. |

## Open questions with defaults

1. **Does “conflict preview” require file-content or byte diffs?** Default: **no**. The pinned dialog shows destination paths and competing writers, while the analyzer is described as a simulated VFS write preview (`C# src/ModSync.GUI/Dialogs/ConflictsDialog.axaml:4-65`; `C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:19-30`).
2. **Should Rust continue after a failed instruction because C# does?** Default: **no**. Preserve Rust's stop-on-failure behavior and surface the error; the C# continuation can yield an incomplete report (`C# src/ModSync.Core/Services/Conflicts/FileConflictAnalyzer.cs:115-160`; `src/engine.rs:777-783`).
3. **Should a stored update result overwrite current guide metadata on startup?** Default: **no**. Display it as a timestamped snapshot; badges remain derived from the loaded guide until a matching fresh check applies. The badge itself is the one mod-selection T3 places in `src/ui/mod_selection_page.rs` (`src/updates.rs:397-444`; `docs/parity/mod-selection-plan.md:346`; `C# src/ModSync.Core/Ports/Updates/IUpdateCheckResultStore.cs:17-46`).
4. **What is the CLI editing contract?** Default: one explicit JSON operation list applied transactionally to one input and one distinct output path, because ordered operations and unchanged-on-failure behavior are directly inspectable (`src/model.rs:231-300`; `src/model.rs:320-390`).
5. **Are configurable Markdown/regex profiles required or is the default parser enough?** Default: port the configurable preview/apply semantics before claiming F21, because the pinned editor exposes profile editing, reset, preview, and confirmation (`C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:41-61`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:132-196`; `C# src/ModSync.GUI/Dialogs/RegexImportDialogViewModel.cs:321-388`).
6. **How is F22 exercised against a controlled provider?** Resolved by lead ruling: reuse the `nxm-traversal-fix` guard (hidden `--nexus-api-base`; https except loopback http; explicit key only; same-origin redirects) and extend it to `update-check` and `gui` (KTD-7). A plain-http fake on `127.0.0.1` needs no TLS/CA work. The replay-proxy route is removed (`nxm-traversal-fix:src/updates.rs:159-212`; `nxm-traversal-fix:src/main.rs:91-92`, `:642-647`).
7. **When may GUI-tools touch `src/ui.rs`?** Only at steps 5, 9 and 10 of the merged `ui.rs` order, as Slice 7a, 7b and 7c.
8. **When may GUI-tools touch `src/engine.rs`?** Default: never in the planned work; existing full and remaining conflict APIs are sufficient. A concrete missing contract must be reviewed and queued behind the active engine owner before that default changes (`src/engine.rs:777-783`; `src/engine.rs:898-907`).

## Definition of done

- F19 is done only when shared conflict semantics, stale-result handling, the dedicated native view, immutable tree receipts, and native writer/winner/count/limitation evidence satisfy the ledger (`src/conflicts.rs:1-84`; `docs/parity/ACCEPTANCE.md:54`).
- F21 is done only when every shared edit/import/browser action is reachable through production CLI and native UI, failures are non-mutating, and save/reopen preserves identities and references (`src/model.rs:178-203`; `src/model.rs:231-300`; `src/model.rs:320-390`; `docs/parity/ACCEPTANCE.md:56`).
- F22 is done only when update checks, timestamped storage/reload, badges, detailed native states, stale-result rejection, and controlled provider behavior satisfy the ledger (`src/updates.rs:351-463`; `docs/parity/ACCEPTANCE.md:57`).
- The overall slice is done only when `COMPATIBILITY.md` and the ledger describe the proved state without upgrading any source-only or offline-only receipt into native/external proof (`COMPATIBILITY.md:121-125`; `docs/parity/ACCEPTANCE.md:54-57`).
