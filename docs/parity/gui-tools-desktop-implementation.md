# Desktop shared GUI tools — source implementation

## Conflicts

The desktop retains the existing report field and worker message signature while binding the single owned analysis receiver to its captured input stamp. Changed inputs and cancellation discard the result. A dedicated window consumes `conflicts::Presentation`, showing ordered writers, instruction numbers, winning components, component counts, limitations and JSON copy. Analysis failures remain visible separately from empty successful reports.

## Updates

The provider worker uses `check_with_cancel`; request and body cancellation belong to the shared provider implementation. Successful fresh results are applied only when both the captured desktop context and `StoredSummary::may_apply_to` match. The provider result remains inspectable when stale.

Cancelled checks retain their partial summary in memory, never apply the cloned guide, never replace the prior persisted snapshot, and send a non-success terminal. Complete checks save the timestamped summary independently of guide metadata application; a storage failure does not hide the provider report. Reload displays absent, valid, mismatched, unsupported and corrupt states without applying metadata. Existing component badges consume shared metadata.

## Editor

The native editor holds a separate component or recursive option draft. Metadata, reference lists, instruction fields and conditions, resources and options are committed through shared transactions after explicit Apply. Invalid drafts leave the live guide unchanged. A draft is bound to the guide identity and cannot overwrite intervening changes. Successful edits invalidate install validation and old phase receipts.

Component/option insertion, removal and ordering use typed operations. Instruction changes are staged in the selected component draft and committed as a validated component replacement, preserving all fields. Guide metadata uses typed set/remove operations. The public legacy helper signatures and frozen test-facing UI fields remain unchanged.

Raw documents, local files and explicitly requested URLs enter the same preview/apply path. Configurable raw and individual-field regex profiles expose Rust regex flags, pattern editing and reset to the existing Markdown parser. Preview JSON includes proposed operations, warnings, capture offsets and unmatched lines. Editing import inputs invalidates the preview. Confirmation checks its guide identity before any mutation. Rendered preview documents use the shared 8 MiB limit.

File reads, URL downloads, resource inventory, variable projection and guide saves run on the owned desktop worker. URL previews use the shared cancellable, bounded, redirect-checked downloader. Save validates the full guide and uses the distinct-input save boundary when a local input exists. Native file/directory pickers run in that worker; dismiss an outstanding picker to finish its operation or shutdown. Picking a path alone never saves or imports.

Resource browsing lists exact registered archives and per-resource errors without extraction. Archive members are explicitly relative to a user-specified extraction directory. Shared variable projection checks roots before insertion into a draft source or destination.

## Evidence and scope

This document records source behavior. No tests, fixtures, native probes, downloads, installs or restores were executed for this slice. Production builds and strict Clippy are parent-owned after source freeze. Native focus/keyboard behavior, provider integration and editor save/reopen acceptance remain unexecuted. Existing checkpoint defaults and telemetry behavior are unchanged by this slice.

## F20 merge review

`ui/merge_panel.rs` provides an immutable merge review alongside the existing editor. Every shared merge option and heuristic is exposed. The loaded guide supplies the existing source; a bounded ordinary UTF-8 incoming file is parsed without replacing authored selection flags. The active guide, its selection, and completion state are never replaced by this workflow.

Preview runs on the application-owned worker and displays candidate counts, the complete copyable preview, loss warnings and validation failures. Changes to current application inputs, incoming path or options invalidate the staged preview. Explicit apply/save re-reads the incoming source and recomputes through `merge::apply_preview`; when the loaded guide has a file, its disk identity is captured and checked again as well. Both source canonical paths are protected from output replacement. The separate output uses the shared validated atomic save boundary.

A save waits for the sharing mutation barrier, rechecks context, and remains owned through cancellation or close. Cancellation is checked before publication; a save already published reports its actual output rather than claiming cancellation undid it. Read-only preview does not acquire install mutation authority. Native file dialogs run on the worker; cancellation or close waits for dismissal. Source review and production compilation do not establish native picker or merge runtime acceptance.

## Managed deployment and legacy shell adapters

The managed deployment window reads the shared bounded inventory and retains its service roots and immutable review identity. Inventory warnings disable mutation. Uninstall and purge require an explicit confirmation and a sharing pause acknowledgement. The UI clears previous completion and validation before dispatch, because a failure can follow partial filesystem changes. The receipt keeps deleted, restored, transferred, skipped and warning paths, cancellation and error state. Only `complete` permits the conditional sharing resume. A stale result may describe its captured roots but cannot authorize the current context. An inventory badge is limited to the matching captured context.

The legacy shell now exposes File, Tools, Help, About and More menus, Home, mode/tab navigation, a separate raw guide window and an inspectable output log. Unsupported repair and standalone patcher operations are disabled with explanations. Static external links are explicit clicks. Native window decorations remain responsible for minimize/maximize/close. Drop handling accepts supported guide suffixes only while idle and asks for explicit loading; it never starts installation. New file-open, save and raw parse/render work uses the owned application receiver and preserves the current guide on parsing failure. A parsed raw replacement is bound to the current guide context and requires explicit confirmation. Saves and log publication happen away from repaint.

Directory text fields commit trimmed text on Enter and show missing-directory feedback. The editor exposes a focusable component keyboard control: arrows navigate, Ctrl+arrows reorder within siblings, Delete uses the shared remove transaction, and Space toggles top-level selection through dependency and game-policy selection. Nested-option selection remains in its dedicated controls. Text editors retain ordinary text keyboard behavior. The log inspector supports native TextEdit selection/copy shortcuts and explicit all-copy, save, clear and auto-scroll. It does not infer business outcomes from raw output lines or invent severity/timestamps absent from the source.

Rich preamble/component content is rendered by the shared bounded CommonMark presentation module. Unsupported constructs retain the raw/copy fallback. Summary cards use shared spoiler projections, explicit spoiler URL reveal, option/instruction detail and safe web links. Component download actions filter the existing selected-guide acquisition candidates by that component's original GUID; instruction links select a separate editor draft. The raw warning and epilogue surfaces also use this renderer as a documented presentation enhancement.

Generated-instruction review now has an explicit per-component resolution panel. Authors apply instruction/options edits first, then provide one bounded decision for each original gap, condition and rejected instruction plus a final note. The shared `ResolveDraft` transaction rechecks the applied component identity and preserves original diagnostics. There is no bulk acceptance action. Later authoring changes invalidate the bound receipt through the shared library.

These are source-level adapters. Exact menu geometry, widget focus/announcement behavior, screen-reader trees, native drag/drop, picker cancellation, high-DPI layout, and platform control-by-control parity remain unexecuted acceptance work. The new surfaces are not evidence of successful runtime installation, uninstall, network export or game behavior.

## Selection publication and small shell actions (2026-10-02)

Legacy Save Selection now dispatches through the owned shell worker. It captures
the guide, FOMOD choices path, completed-prefix IDs and application input identity;
the shared bounded sidecar reader overlays only remaining components. The file
dialog and atomic guide publication run on the worker. Cancellation is checked
before publication, and a successful save remains reported after late cancellation.
Only a matching current input identity and completed prefix may clear unsaved
records or promote the saved guide's sharing provenance. The original synchronous
helper remains intact for frozen compatibility callers.

The raw-document window offers a separate UUIDv4 helper and explicit copy action.
Generating an ID never rewrites guide/component identities. Dropped-guide
confirmation initially focuses Cancel, accepts Escape/backdrop dismissal, makes
cancellation win simultaneous activation, and restores prior focus on dismissal.
Help exposes the pinned reference's explicit issue-form and sponsorship links;
these actions do not submit an issue or payment.

Save/GUID source review found no actionable blocker. Native dialog, focus,
clipboard and file-publication acceptance remains deferred.

## Editor sections and local archive generation (2026-10-02)

The editor exposes Expand All/Collapse All for Basic Information, Description,
Dependencies, Instructions and Options. The sections start collapsed, matching
the pinned reference. Their state only changes presentation; Apply remains
reachable and existing sharing, busy and confirmation gates remain in force.

Auto-generate from local archive captures the fully parsed component/option
draft, raw resource/metadata text, guide identity, current input stamp and mode.
The native picker and shared resolved-filename inference run on the editor worker.
The selected archive and expanded contents each have a 256 MiB limit, with shared
ordinary-path, entry/model and provenance-history checks. Nested options use a
one-component temporary guide for inference.

Append uses shared merge rules; replacement clears instructions only and retains
options and review history. Diagnostics and the candidate are displayed before
explicit adoption into the editor draft. Existing Apply remains the whole-guide
transaction. Generated content still requires the established draft-review
resolution. Relevant edits, cancellation and changed contexts discard preview
authority. Archives are not copied to the mod directory, and generation never
installs or saves a guide. Referenced archive basenames must exist in the selected
mod directory before a later installation.

Native picker cancellation waits for the dialog to return. Source review and
production compilation do not establish picker, decoder, focus or instruction
parity at runtime.

Provider-link generation is now an explicit alternative in the same preview
panel. It captures the active UI acquisition key, mod directory and source mode;
only a key digest enters the internal preview fingerprint. The shared filename
discovery API runs on the owned worker, and errors, cancellation, empty results
or any partial-discovery diagnostics reject candidate creation. Displayed errors
use fixed diagnostic codes. No archive transfer is requested. Present cached
archives use the shared 256 MiB expanded-content limit per archive; absent
archives use filename-based inference requiring review.

Editor and shell workers retain their thread handles and cancellation tokens.
Normal polling joins only finished workers without changing cancellation state.
Close waits for both receivers and worker settlement; teardown cancels and joins.
If a worker panics after queuing a terminal, a final message drain preserves the
actual saved result before reporting the thread failure. It cannot clear a newer
job's receiver after its own terminal was delivered.

Provider generation is disabled in isolated probe launches because the shared
filename API has no probe endpoint/CA parameters. The worker checks the captured
allowance before any provider lookup, and the allowance also binds preview
authority. Local archive generation remains available. No production provider
requests were made to implement or review this adapter.
