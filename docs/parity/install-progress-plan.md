# Shared installation progress and Installing page plan

Status: Task 1 shared API approved at `065ffeed4337a9ee9bc317b91998f262e36565bf` (base `659027e7eef54bb489039e3062b36ddd232b5c7d`); Task 2 adapters/UI ready for dispatch. Pinned C# oracle is `d31ccbd3bfcddb4705917eafa6483715c166de3b`. This slice advances F10/F24/F26; it cannot close full C# parity, recovery/checkpoint parity, or full-build acceptance by itself.

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md` and all unrelated worktree state.
- The user has deferred creating, porting, expanding, modifying, or running Rust tests of every kind until full parity is complete. Preserve existing test and inline `#[cfg(test)]` code byte-for-byte. Do not create a temporary Rust harness disguised as a probe.
- Product verification uses production builds, the real CLI, the real native desktop, disposable fixtures, before/after manifests, source/binary hashes, and independent review.
- Keep installation behavior in the shared library. The CLI and GUI may format and retain progress, but neither may infer component order, completion, recovery, or cancellation facts independently.
- Keep `Event { component, step, message }` and its serialized JSON byte-contract unchanged for existing `--json` consumers. The new API is additive.
- Preserve strict guide order, fail-stop behavior, pending-recovery guards, verified Remaining prefixes, and the rule that an earlier failed step is never backfilled later.
- Never claim a per-component checkpoint: current Rust recovery creates one whole-run snapshot before writes. C# derives a checkpoint count by parsing logs, but copying that display would fabricate a fact in Rust.
- Base and verified Remaining installs use one progress contract and renderer. No second installer, coordinator, phase planner, receipt reader, or GUI-only event model.

## Source findings that govern the design

Rust `src/engine.rs` currently exposes only the serialized `Event` stream. `Engine::install` validates and scans, announces snapshot creation through an ordinary `Event`, calls `recovery::begin`, executes ordered components, records completion, and calls `recovery::finish`. `install_remaining` only establishes the verified prefix and delegates to `install`. `run` knows the exact ordered suffix; `component` knows the component and one-based authored instruction index. Cancellation is checked between instructions and while supported long operations run. A failure after `recovery::begin` leaves pending recovery and is wrapped with the restore-before-retry message.

The CLI prints each existing `Event` as either its unchanged JSON object or `component / step: message`. The desktop worker converts the same event to a log string and receives only a terminal `Done`. Its Installing page therefore has no authoritative component total, phase, current operation, or lifecycle state.

The exact pinned-source oracle is [install-progress-csharp-audit.md](install-progress-csharp-audit.md). The active Avalonia Installing page does **not** consume the similarly named shared `InstallationCoordinatorService.InstallProgress`; it uses `InstallationService`'s zero-based pre-component callback, derives elapsed/ETA/rate locally, finds directions by first exact name match, and counts checkpoints/warnings/errors by watching process-global log text. Its callback calls the active component “installed” before execution; any returned non-success exit code is still rendered as 100% complete and forward-ready. These are source-proven reference defects, not parity targets. Rust must match the page composition while exposing its own factual shared execution state.

## Additive shared API

Create `src/install_progress.rs` and expose it from `src/lib.rs`.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Preparing,
    CreatingSnapshot,
    InstallingComponent,
    RunningInstruction,
    ComponentCompleted,
    RecordingCompletion,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerFrame {
    pub name: String,
    pub guid: String,
    pub reached_from_instruction: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub top_level_component_name: Option<String>,
    pub top_level_component_guid: Option<String>,
    pub component_index: usize,
    pub component_total: usize,
    pub owner_name: Option<String>,
    pub owner_guid: Option<String>,
    pub owner_path: Vec<OwnerFrame>,
    pub instruction_index: Option<usize>,
    pub instruction_total: Option<usize>,
    pub operation: Option<String>,
    pub message: String,
    pub snapshot_pending: bool,
}
```

Names may be adjusted during implementation only when review shows a concrete compatibility or ownership problem. Required semantics are fixed:

- Indices are one-based when the corresponding item exists and zero only when no component is active.
- `component_total` is the exact number of ordered selected **top-level guide components** remaining after excluding the verified completed prefix. Recursive option components reached by `Choose` never increment this total or the top-level index.
- `OwnerFrame` carries the case-preserving owner name/GUID and the authored instruction index through which it was reached. `owner_path` begins at the top-level component and appends each reached option. `owner_name`/`owner_guid` identify the component or nested option that owns the active instruction. GUID/path identity, not display name, binds directions and state; duplicate component or option names cannot select the wrong directions.
- `instruction_total` is the authored instruction count for the current **owner frame**. It is not a byte-progress estimate and does not flatten nested `Choose` work into the top-level count.
- `operation` is the authored action for an active instruction. An instruction excluded by dependencies/restrictions emits a factual skipped `RunningInstruction` result for its own owner/path/index and never visits nested Choose options. A reached `Choose` reports its own instruction and then nested owner frames separately.
- `snapshot_pending` changes to true only after `recovery::begin` has succeeded. It remains true through installation and completion recording; the terminal `Completed` fact is emitted only after `recovery::finish` succeeds and therefore carries false.
- No success terminal event is emitted after an error or cancellation. The caller's existing `Result` is the authoritative failure/cancel terminal. Do not add an error string to a progress item merely by guessing how `anyhow` will later wrap it.
- There is no `RollingBack` phase because ordinary Rust install does not automatically roll back. Restore is a separate existing operation. Naming a phase that never occurs would be false parity.

Add observed methods while retaining the old methods as no-op-observer wrappers:

```rust
pub fn install_observed(
    self,
    guide: &Guide,
    emit: impl FnMut(Event),
    progress: impl FnMut(install_progress::Progress),
) -> Result<()>;

pub fn install_remaining_observed(
    self,
    guide: &Guide,
    completed: &[String],
    emit: impl FnMut(Event),
    progress: impl FnMut(install_progress::Progress),
) -> Result<()>;
```

`install` and `install_remaining` call these with a no-op progress observer. Existing event emission sites and JSON remain unchanged. The observed Remaining method calls the same `set_completed_prefix` and shared install body; it cannot reconstruct progress totals in an adapter.

## Observer lifecycle

The shared engine emits only synchronous facts immediately around the real operation:

1. Lock acquisition, clean-state/receipt verification, scan, and the entire non-mutating preflight `run` are **silent** to install-progress observers. Validation/preflight components and instructions are never counted as installation progress.
2. `Preparing` only after silent preflight and the following scan have succeeded, when the exact ordered remaining top-level list is fixed.
3. `CreatingSnapshot` immediately before `recovery::begin`, with `snapshot_pending = false`.
4. A second `CreatingSnapshot` completion fact immediately after `recovery::begin`, with `snapshot_pending = true`.
5. `InstallingComponent` before real execution of each top-level component, with exact top-level index/total and GUID identity.
6. `RunningInstruction` after each owner-frame instruction succeeds or is conditionally skipped, with its real one-based owner-local index, owner-local total, action, outcome and owner path. Do not emit completion for an instruction that returned an error.
7. `ComponentCompleted` only after the complete top-level component, including every reached nested Choose owner, returns success. This is the only fact that increments the installed count/overall percentage.
8. `RecordingCompletion` before persisting the completion receipt, still pending.
9. `Completed` only after both `recovery::record_completion` and `recovery::finish` succeed. This event is the engine commit point and carries `snapshot_pending = false`.

If preparation, silent preflight, snapshot creation, an instruction, receipt persistence, or finish fails, the method returns the actual error and the last emitted progress fact remains the last known fact. The GUI/CLI terminal adapter records that error separately. A cancellation flag observed by an adapter after the engine returned success cannot override `Completed` or turn the committed `Ok(())` into cancellation; the returned engine result and commit-point event win over late UI token state.

The observer may be an ordinary `FnMut`; like the existing event emitter it has no error return. Callers must not panic. Channel send failure may be ignored because it means the consumer is gone, but the engine continues its existing safe operation/cancellation contract.

## Shared facts versus presentation state

Shared engine facts:

- real phase and message
- ordered remaining component index/total
- top-level component and nested owner GUID/path identity
- owner-local authored instruction index/total/action
- whether a recovery snapshot is currently pending
- terminal success only after recovery finalization
- returned cancellation/failure error

Presentation-only desktop state:

- run identity and input stamp
- monotonic start time, elapsed display, ETA, and components/minute
- current owner directions looked up by owner GUID/path from the exact prepared guide retained for this run; never first name match
- interpolation/indeterminate animation between synchronous events
- scroll positions, expanded panels, formatted copy, colors, and transient labels
- warning/error counters derived only from reliable typed facts. Existing external-program stdout/stderr arrives as untyped `Event.message`; it must remain visible in logs but cannot be counted as a warning/error by prose matching. Until the engine exposes typed issue severity, label these counters unavailable or bounded rather than implying process-wide log parity.

Elapsed/ETA/rate must use the native process monotonic clock and completed component boundaries. Do not persist them, put them in CLI JSON, or imply byte-level precision. With zero completed components ETA is `--:--:--`; rate is `0.0 mods/min`. A single active instruction can remain indeterminate.

## CLI adapter and compatibility

`src/main.rs` remains the owner of formatting. Ordinary output and current `--json` continue to use only the legacy `Event` callback. Add an explicit install-only flag such as `--install-progress-json` (final spelling reviewed against Clap conflicts) that emits newline-delimited typed progress objects from the shared observer. It must conflict with legacy `--json` and validation-report mode so one stdout stream has one schema.

Each typed line should be enveloped to remain distinguishable and extensible, for example:

```json
{"type":"installProgress","progress":{...}}
```

On failure the existing process error/exit behavior remains authoritative; do not emit a fake `Completed`. If machine-readable terminal failure is added, it must use the same envelope and actual final `anyhow` text, and it must not change legacy modes. This is optional for the slice; a nonzero exit plus absence of `Completed` is sufficient if documented.

Direct probes must demonstrate legacy event JSON is unchanged at the field and line level and that typed All/Base/Remaining streams expose the same shared phases without adapter-generated totals.

## Desktop state and exact Installing page

Create `src/ui/installing_page.rs` and route both `Page::Installing` and `Page::WidescreenInstalling` through it. `src/ui.rs` owns worker messages and lifecycle wiring only.

Page state contains:

- monotonic run ID and exact input stamp
- current `Progress`, start instant, completed-component timestamps, terminal result
- prepared-run component directions keyed by GUID
- page-local ordered log lines
- stop-confirmation state and display state needed by the renderer

Worker messages carry run ID with every progress and terminal install result. Only the current run with an unchanged input stamp may mutate the page. A new run clears prior progress, metrics, terminal text, and local log. Late messages from an old run are ignored. Base and Remaining create the state from the exact `phase::PreparedPhase` guide actually handed to the engine, never from a later-mutated `self.guide`.

Match the pinned C# composition and exact measurements from the audit: outer vertical page scroll; inner margin 16×12; main card padding 24×20/radius 12; 28pt percent and 16pt count; 12px overall bar; current-info `*,Auto` row; directions max height 60; 6px indeterminate current-mod bar; statistics/showcase columns `*,16,*`; statistic cards padding 20×16/radius 10; explanation across both columns; footer padding 16×12/radius 8. At 600×400 retain the two-column composition and whole-page vertical scrolling because the C# XAML has no responsive stacking rule.

Required content:

- top progress card with percent, `N/M mods installed`, main progress bar, current component, directions, current operation, and indeterminate instruction bar
- statistics card with elapsed, remaining, rate, issues, and truthful recovery-snapshot status
- adjacent “Installing Your Mods” card
- “What's Happening?” card listing real supported operations
- footer warning not to close the app while writes/recovery are pending

The displayed checkpoint area must say `Recovery snapshot`, `Preparing recovery snapshot…`, `Recovery snapshot active`, or `Recovery finalized` as warranted. Omit `CHECKPOINTS CREATED` or display the truthful scalar `1 recovery snapshot` only after `recovery::begin`; do not copy C#'s per-component counter.

Overall percentage uses explicit `ComponentCompleted` facts, never callback start, instruction count, time, or the next component as an inference. While component `i` is active, the installed count remains `i - 1`; the current-mod bar remains indeterminate for the entire active top-level component because the engine has no byte/work denominator. `Completed` displays 100% only after receipt persistence and recovery finalization. This intentionally rejects the C# callback's premature count and returned-non-success false-100% behavior.

During an install Back remains disabled and forward remains disabled. The footer exposes Rust's deliberate safety `Stop Install`; it opens a confirmation and then sets the existing cancellation token. The active C# host has no install Stop control; the old modal's “finish current mod” text is not its contract. Rust stops at the next safe cancellation boundary and requires Restore when a pending snapshot exists. Window close continues to use the existing stronger busy confirmation and must mention pending recovery accurately. After committed engine success forward becomes `Continue`; after failure/cancel it remains blocked and the page retains the actual error and recovery guidance.

The worker captures the exact run ID, input stamp, prepared guide, and `Roots` before moving settings into the engine. Its terminal message carries the engine `Result` plus pending-recovery state read from those captured roots. `handle_message` must never recompute recovery state from current editable UI paths or infer it by parsing an error string. A late cancellation flag cannot rewrite an already committed success. Resetting/changing the visible project may make a terminal result stale for presentation, but it does not change what the engine committed to its captured roots.

An empty Remaining phase is rejected by shared `phase::prepare` (`No remaining components to install`) or intentionally skipped by the existing zero-selected-widescreen route before engine dispatch. It produces no fake engine progress or Completed event. The UI may render a clearly labeled skipped phase from route state, separate from installation success.

## File ownership and task sequence

Implementation tasks are serialized.

| Task | Owned files | Responsibility |
|---|---|---|
| Task 1 | `src/install_progress.rs` (new), `src/lib.rs`, `src/engine.rs` | Add typed shared contract and observed All/Remaining lifecycle; preserve old API/Event JSON and recovery semantics |
| Task 2a | `src/main.rs` | Explicit typed CLI adapter and production typed-sequence/legacy-schema evidence |
| Task 2b | `src/ui.rs`, `src/ui/installing_page.rs` (new), `src/wizard.rs` only if footer gate/label correction is necessary | Run-bound GUI channel/state; exact renderer; Base/Remaining reuse; cancellation/close/navigation behavior |
| Task 3 | `docs/parity/install-progress-native/` (new), `docs/parity/GUI-INVENTORY.md`, `docs/parity/GUI-CLI.md`, `docs/parity/ACCEPTANCE.md`, `COMPATIBILITY.md` | Native/differential evidence, honest status, final integration review and delivery preparation |

Workers do not edit outside ownership without reporting the exact required expansion first. Task 2a starts from approved Task 1 `065ffeed`; Task 2b starts only after Task 2a source review approval. Task 3 does not rewrite source. No new shared projection module is currently justified: typed operational facts already live in `install_progress.rs`, CLI only serializes their envelope, and elapsed/ETA/layout remain GUI presentation. Reopening the approved Task 1 module for adapter formatting would blur that boundary.

## Acceptance probes without Rust tests

Use a temporary fixture root outside the repository and user game trees. Capture fixture manifest/hash before every run and game/mod/recovery manifests at every terminal boundary.

Production probes:

1. Two-component All install with at least two ordinary instructions per component: verify typed phase order, exact index/totals/actions, 100% terminal, guide-order bytes, completion receipt, and finalized recovery.
2. Base install with one deferred widescreen component: verify component total excludes widescreen and only base bytes appear.
3. Retained Remaining install using the real base completion receipt: verify total/index cover only the suffix, completed source is not replayed, dependency context works, and final bytes are ordered.
4. Failure on component two after component one wrote: verify no instruction-success/progress fact for the failed instruction, no `Completed`, nonzero CLI exit, pending recovery, blocked retry, unchanged unexecuted suffix, then real restore returns exact before hashes.
5. Cancellation during a controllable long operation: verify cancellation is observed through the existing token, no later instruction runs, no `Completed`, pending recovery is reported when writes began, and restore is exact. Do not kill or reuse a user process.
6. Failure before `recovery::begin`: verify no pending snapshot and no writes.
7. Legacy `--json` receipt: compare keys/order/schema to the pre-change three-field Event contract and show no new progress lines contaminate it.
8. Typed CLI mode: parse every NDJSON line with an external JSON parser and bind output to the copied executable hash.
9. Silent preflight: use a multi-component fixture and prove no install progress appears before the snapshot boundary and no component is counted twice.
10. Nested Choose with duplicate display names: prove top-level totals remain unchanged, owner paths/GUIDs identify nested work and directions, and a condition-skipped Choose does not emit nested frames.
11. Late cancellation: set cancellation only after committed success is observed and prove the engine result/UI remains successful.
12. Empty Remaining: prove shared rejection or route-level skip emits no fake progress/success.

Native GUI matrix at controlled 1280x800 and 600x400, Light plus focused K1/K2 samples:

- initial/preparing
- recovery snapshot creation/active
- component one and component two with directions/current operation
- successful 100%/Continue
- failure with recovery guidance and blocked Continue
- Stop confirmation, actual cancellation, blocked Continue, then Restore
- base phase followed by retained Remaining/widescreen phase using the same page
- attempted close while active
- long names/directions and small-window scrolling
- no stale prior-run state after rerun or guide/path change

Pinned C# captures should bind page layout/copy/metrics behavior to its source/binary. Rust and C# fixtures need not force identical recovery internals; record deliberate truthful differences.

Every receipt records:

- Rust HEAD/dirty-state hash and explicit preservation of `AGENTS.md`
- product source hashes and copied executable SHA-256
- C# commit/binary hashes for reference captures
- complete command argv, exit code, stdout/stderr hashes
- fixture guide and initial/final tree manifests
- recovery state and receipt hashes
- display geometry/theme/font/DPI and owned process IDs
- exact interaction transcript and screenshot/clipboard hashes
- explicit statement that no Rust tests/harnesses were created, changed, or run

## Review gates and frozen-test risk

Task 1 specification review checks silent preflight, every real lifecycle emission, exact top-level suffix totals, nested owner paths, condition skips, observer ordering, success only after receipt+finish, no success after error, snapshot truth, old method delegation, and unchanged Event serialization. Quality review checks callback ownership, dropped observers, duplication, error wrapping, cancellation polling, and no adapter business logic.

Task 2 specification review checks CLI schema isolation, captured roots/run/input binding, prepared-guide GUID/path identity, both phases, empty Remaining, exact C# page fields, percentage math, late cancellation, cancellation/close/navigation, and truthful recovery text. Quality review checks dropped receiver behavior, channel races, repaint behavior, stale messages, monotonic timing, duplicate names, small-window layout, accessibility labels, external-output limitations, and absence of log/error parsing for business facts.

Task 3 final review covers the whole diff from `659027e`, all probe receipts, docs claims, and user constraints. Findings return to the owning implementer; scoped re-review closes each correction. One fresh whole-package reviewer inspects the final combined diff before authorized normal delivery.

Existing frozen inline tests in `src/ui.rs`, `src/engine.rs`, or other targets may fail to compile if they construct exhaustive `Message`/state values or call changed private helpers. The implementation must prefer additive APIs and fields with existing constructor/default patterns so production builds remain valid. Workers must not edit, remove, `cfg`-hide, or run those tests. Any test-only compile drift is recorded with exact source location for later adaptation after the user lifts the freeze; production compatibility shims must not be added solely to satisfy uncompiled stale tests.

## Completion boundary

This slice is complete only when shared source, CLI adapter, native desktop page, direct All/Base/Remaining/failure/cancel/restore probes, independent reviews, and durable evidence are approved and delivered. It advances F10/F24/F26. It does not establish per-component checkpoint parity, automatic rollback, full C# GUI parity, accessibility/platform equivalence, or F29 full-build/game equivalence.
