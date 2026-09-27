# C# validation page parity implementation plan

> **For agentic workers:** Use `superpowers:subagent-driven-development`, with fresh implementer, specification reviewer, then code-quality reviewer per task. Parent supplies complete task brief, Global Constraints and the audit reference. Execute serially; do not create nested subagents.

**Goal:** Reproduce the active C# ValidatePage's control composition, text, result mapping and interactions using shared Rust validation outcomes and real pipeline progress, then verify the actual native page.

**Architecture:** Preserve `src/engine.rs` as the shared GUI/CLI validation owner. Add pure report presentation and typed stage progress in the library; add a page-local desktop state/renderer consuming those contracts. No second GUI validation engine or cosmetic-only replacement.

**Tech stack:** Rust/egui/eframe, existing serde report contract and engine worker channels; pinned C#/Avalonia oracle.

## Current integration checkpoint — 2026-09-27

Task 1 shared projection/progress is approved through `9d77f4b`; Task 1b shared source-availability correction and reached-option FOMOD fix are approved through `88b2dd9`. Task 2 implementation is now through `47c3e67`, with actual renderer `src/ui/validation_page.rs`. The task contracts and historical checklist below describe delivered work, not permission to redispatch old tests or builds. Final native evidence is [archived](validation-native/README.md); integrated acceptance is **pending final review**; no full C# GUI parity is claimed.

The pre-fix full missing-Copy report was actually **3/0/3**; partial `--dry-run` was **2/0/1**. The earlier full **2/0/4** was an inference and is not authoritative. Task 1b now measures missing Copy **0/1/5**, clean/restored **0/0/6**, with strict installation and remaining receipt safeguards retained. See [integration rulings](validation-page-integration.md) for current contracts and evidence boundaries.

## Latest explicit user ruling

Defer **all creating, porting or expanding Rust tests**, including inline test modules and test files, until the overall full C# feature-parity goal is complete. This explicit user ordering takes precedence over skill/AGENTS test-writing defaults and earlier plan gates. Preserve existing tests and recorded evidence. The cost is deferred automated regression coverage for subsequent changes; use builds, direct disposable CLI probes, driven native GUI probes and independent specification/quality reviews now. Existing tests may still be run without adding or expanding them, under parent compiler ownership.

Task 1 is approved over `915ec69..9d77f4b`; its fix-round-1 P2 formatter finding is addressed in `.superpowers/sdd/validation-page-plan/task-1-fix1-review.md`. Preserve its pre-ruling tests and evidence. The missing-source audit is complete and **Task 1b is mandatory before Task 2**: production-only shared backend correction, followed by independent specification and quality approval. Its complete dispatch brief is `.superpowers/sdd/validation-page-plan/task-1b-brief.md`, BASE `9d77f4ba9955ea505d3eb7969afdf5b5aee73b72`.

Authoritative missing-source full-report comparison: pinned C# **0 errors / 1 warning / 5 passed**; source-bound Rust production **3 errors / 0 warnings / 3 passed**, failing Conflicts, ComponentValidation and DryRun. The previous alleged full **2/0/4** was extrapolated from measured partial `--dry-run` **2/0/1**, not observed. See `docs/parity/validation-missing-source-audit.md` and `validation-missing-source-evidence/current/receipt.json`; historical logs remain, and the old worker report is annotated with this correction.

## Global Constraints

- Apply the latest explicit user ruling above in every worker brief: no new, ported or expanded Rust tests until overall feature parity is complete. Preserve existing tests/evidence; do not remove them to satisfy this ordering.
- BASE Rust `915ec69a21d90fac0395932d54e1622f8c049a18`, branch `codex/rust-port`; C# oracle `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Re-read current status/HEAD before work. User-owned dirty AGENTS.md must remain untouched. Other agents own audit/runtime docs; never revert their edits.
- Product implementers are not alone in this checkout. Own only the paths listed in the task; serialized product edits. Parent remains hands-off. Do not spawn subagents. Implementers must commit only their owned task changes for SDD review, using explicit file paths when staging and preserving all other changes. Publication/push remain with the designated integrator after final reviews under existing authorization. Do not modify C# without a separate parent instruction. The audit/planning worker alone has a no-commit restriction.
- Parent controls the compiler slot. Reconfirm the active owner with parent; no build/test/compiler process while another worker owns that slot. Read/edit work may proceed independently. Use the existing parent-approved controlled target directory and bounded jobs; never clean an unowned target or stop another process.
- Shared validation never mutates mod/game trees or runs external programs. Direct probes use isolated disposable roots. Preserve guide order, completed-prefix verification, no replay of consumed base Move sources, and pending-snapshot retry block.
- Preserve CLI flags, JSON shape, ordinary Event contracts and stdout/stderr/exit behavior. Additive APIs wrap old methods with no-op progress observers. CLI must remain usable with `--no-default-features` and never import UI types.
- Preserve trust/consent controls, stale-input invalidation, close/Stop safety and successful single-step fallback when available. “Exact C#” does not authorize making validation unsafe or pretending synthetic progress is real. Existing proactive Next gating vs C# click-time guard is an explicitly documented difference; do not weaken it.
- Exact text, dimensions, colors, stage/card rules and source pointers have one source: `docs/parity/validation-parity-audit.md` (the specification). Required native evidence and ownership are in `docs/parity/validation-runtime-plan.md`. Do not replace those requirements with a new simplified interpretation.
- Full C# GUI parity remains open beyond this page. Automated tests/source/synthetic cards do not prove native parity; historical d4fdb45 screenshots do not cover current or new code.

## Preflight

| Check | Observed / action before dispatch | Owner |
|---|---|---|
| Rust branch/base | codex/rust-port /915ec69 verified; dirty AGENTS preexisting | parent verifies again |
| C# oracle | d31ccbd clean at audit; runtime build now separate owner | runtime worker |
| Instructions | Rust AGENTS, C# supplied guide, shared baseline, audit read | each worker |
| Native evidence | Pinned d31ccbd Copy clean/warning/environment-error native reference exists; full renderer comparison still open | runtime worker / Task 3 |
| Build isolation | Historical audit build/probe completed; no current compiler availability is implied. Obtain parent ownership before compiling | parent |
| API contract | Task 1 approved at9d77f4b; Task 1b and reached-option fix approved through88b2dd9; Task 2 consumes those contracts | Task 1b implementer + reviewers |
| Missing-source truth | Pre-fix full Rust3/0/3 measured; corrected shared Copy report0/1/5 matches native C#. No UI severity rewriting | Task 1b |
| Task 2 dependency | Dependency satisfied at88b2dd9. Task 2 source corrections reviewed; final native matrix archived; integrated review pending | parent |
| Working fallback | Preserve strict actual install, standalone conflicts, utility settings and base/remaining receipt flow throughout | Tasks 1b–2 |
| External prerequisites | Native fixtures use disposable roots, no download/provider need | Task 3 |

## File structure and task boundaries

| Path | Responsibility | Owner |
|---|---|---|
| `src/validation/presentation.rs` (new) | pure ordered card/log/summary/report projection, no egui | Task 1 |
| `src/validation.rs` | expose presentation module and stage-progress data | Task 1 |
| `src/engine.rs` | additive observed pipeline methods; emit real stage transitions | Task 1 |
| `src/engine.rs`, `src/validation.rs`; minimal production outcome module only if needed | correct shared declared-conflict/archive/dry-run availability semantics; preserve strict executor/receipt guards | Task 1b |
| `docs/parity/validation-missing-source-fix/` and task scratch receipts | real production CLI probes and byte/hash manifests; no new Rust harness/tests | Task 1b |
| Existing Task 1 test files | preserve approved implementation/evidence; no additions or expansion until overall parity completion | no test-edit owner now |
| `src/ui/validation_page.rs` (delivered desktop-only module) | page-local state, exact six-row renderer | Task 2 |
| `src/ui.rs`, `src/main.rs` module declaration if needed | route, worker observed API and typed channel events | Task 2 |
| Disposable CLI/native probe evidence | stale events, lifecycle, copy/reset/navigation; no new Rust test code | Tasks 2–3 |
| `docs/parity/validation-native/` (new) | fresh receipt/fixtures/clipboard/captures | Task 3 |
| `docs/parity/GUI-INVENTORY.md`, `docs/parity/ACCEPTANCE.md`, `COMPATIBILITY.md` | honest proven/open state | Task 3 |

## Task 1 brief: shared presentation and genuine stage progress

**Status:** Approved `915ec69..9d77f4b`, including addressed fix-round-1 P2. Contract below records delivered scope; do not redispatch it or add tests. Its backend gap is now assigned to the defined Task 1b below.

**Outcome:** Headless library supports the exact C# stage-to-card/log/report mapping and truthful stage progress without changing existing CLI contracts. Own only Task 1 paths. Read audit stage matrix, P/M methods, R/E methods before edits. No GUI implementation in this task.

**Contract to implement:** Add `validation::presentation` with `Card { title: String, message: String }`, `Presentation { cards: Vec<Card>, log_lines: Vec<String>, summary: String, details: String, status: String }`; pure `project(report: &Report, selected_count: usize) -> Presentation`; `format_report(presentation: &Presentation, report: &Report, generated_utc: &str, full_log: &str) -> String`. Timestamp formatting is caller-owned, so golden text is deterministic. Expose first error/warning card/log indices through pure helpers matching audit; use severity prefixes from projected output, not `Report::first_issue()`.

Add `validation::StageProgress { stage: Stage, step: usize, total: usize, message: String }` and additive engine `validate_report_with_options_observed(self, guide, emit, full_validation, dry_run, dry_run_only, progress)` preserving current arguments plus `impl FnMut(StageProgress)`. Existing `validate_report_with_options` calls this with a no-op observer. Add `validate_report_remaining_observed(self, guide, completed, emit, progress)` preserving prefix validation and full/dry-run preset; old remaining method wraps it. Emit before actual stage execution and on actual completion, using same one-based stage step and a count derived from enabled stages. Do not increment completion on a failed setup or manufacture time-based stage changes. Completion header may be derived after final report; worker failure remains explicit.

If existing core diagnostics do not provide C# solution/category/prefix detail, retain unknowns and record exact semantic gaps; add shared deterministic solution fallback only for M's exact known categories/messages. Do not convert arbitrary raw engine strings into guessed component identities. Presentation fallback must still show aggregate failure when there is no prefixed card.

- [ ] Read pinned presenter/mapper and existing serialized report tests; record baseline expected outputs and enabled stage order.
- [x] Preserve Task 1 pre-ruling fixtures/assertions and evidence for projection/report cases; no new or expanded automated coverage now.
- [x] Preserve historical red/green evidence from Task 1; this is no longer a future test-writing gate.
- [ ] Implement pure projection and formatting; include exact C# fallback text/spacing from the audit sources, not paraphrases. Keep raw JSON stable.
- [x] Observed wrappers and original coverage delivered in Task 1; preserve evidence. Future semantic deltas use direct disposable CLI/native probes and independent review rather than new Rust tests.
- [ ] Run `cargo test --test validation_presentation --test validation_progress -j 2`, then relevant existing validation CLI regressions; `cargo test --no-default-features -j 2` when slot available. Parent resolves target path before commands.
- [ ] Self-review diff, commit only owned Task 1 files, and report the commit SHA, exact API changes, passing tests and unresolved semantic gaps. Return DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED with reason and next review entrypoint.

**Specification review:** independently check every audit mapping row, golden report line, enabled-stage schedule, actual callback sites, JSON/CLI compatibility and read-only roots. **Quality review:** check ownership, duplication, callback panic/error behavior, counter drift, unknown handling and evidence independence. Task 1 reviews passed; Task 1b approval subsequently satisfied the Task 2 dependency.

## Task 1b brief: shared missing-source validation semantics

**Delivered and approved through88b2dd9 (original implementation1cb3f4b, reached-option correction88b2dd9), from BASE9d77f4b.** Full dispatch contract: `.superpowers/sdd/validation-page-plan/task-1b-brief.md`. Source specification and direct measured baseline: [validation-missing-source-audit.md](validation-missing-source-audit.md). This task changes production only; no new/ported/expanded Rust tests, inline modules, temporary test files or Rust probe harnesses.

Own bounded `src/engine.rs`/`src/validation.rs` work and direct-probe receipts. Use existing report stage boundaries and executor: declared dependency/restriction analysis for report Conflicts; true read-only archive/destination scope for ComponentValidation; a typed ordinary source-unavailable outcome consumed by report dry-run orchestration as one component-scoped `DryRunSkipped` warning. Shared UI/CLI full report must produce measured Copy baseline **0/1/5** with source missing and **0/0/6** when restored.

Preserve standalone simulated-write conflict APIs, ordinary strict validate, actual installation preflight/no-skip behavior, completion roots/definitions/game hashes, FOMOD known-state failures, pending recovery, and observed stage callback order/totals. No UI reclassification, second installer/coordinator, receipt reader, C# AttemptFixes writes, guessed auto-extraction or Move fallback. The complete brief lists exact existing helpers and the missing helper/type boundary rather than inventing available APIs.

Acceptance is production default/no-default builds, direct real CLI/native disposable probes with copied-binary/source/input hashes and unchanged validation manifests, strict actual-install rejection on isolated missing-source fixtures, then fresh spec and quality review. Existing stale test assertion expecting a report error is documented and preserved under the user instruction. Remaining resolver parity limits stay explicit. Release compiler slot after bounded verification; Task 2 cannot begin until this gate passes.

## Task 2 brief: dedicated native Validation renderer and lifecycle

**Dependency satisfied:** Task 1 is approved at9d77f4b and Task 1b at88b2dd9 after independent specification and quality review. Consume corrected shared outcomes; never encode the old missing-source error into the renderer.

**Outcome:** `Page::Validate` invokes dedicated page renderer, not generic utility `surface(2)`; ordinary controls, copy, log and cards match the audit. Own Task 2 paths only; Task 1 shared contracts are authoritative. Keep legacy utility access functional and preserve widescreen selection/remaining validation callers. Never add a second diagnostic engine.

Use page state with run identity/input stamp, ordered local log, projected results, progress/start time, log expansion, two scroll positions, splitter height and copy feedback expiry. Worker messages must carry run identity with StageProgress and terminal report; only the matching active run may mutate page state. Discarded/setup-failed/cancelled runs keep navigation blocked. Do not allow late progress to recreate cleared results. Existing global activity may mirror events but cannot substitute for page-local flushed log.

- [ ] Drive disposable CLI/native probes for begin/reset, progress, clean/warn/fail completion, setup failure, stale result/progress, new input invalidation, stop, rerun and copied full log. Record actual commands/interactions/outcomes. Do not create, port or expand Rust test code.
- [ ] Hook observed shared APIs through typed Message variants; retain existing full/remaining selection and completion-prefix logic. Keep old worker regression assertions intact.
- [ ] Implement audit's exact six-row heading/action/log/splitter/results/footer, independent scrolling, initial/run/completion visibility and text; apply matched Light/K1/K2 tokens instead of egui default red/yellow/green substitutes. Preserve supported window sizes and sidebar boundary.
- [ ] Add `Copy report`/1500ms `Copied!`, C# human report format, first-issue independent log/card targets and 2px red/amber highlight. New run resets both offsets; warned/failed run expands log automatically; clean collapses.
- [ ] Move utility-only trust/FOMOD/settings/conflict/recovery controls to existing reachable utilities without dropping actions or changing consent. Keep shared library APIs callable from CLI.
- [ ] After parent grants the compiler slot, run builds for default and no-default features, format and strict Clippy; exercise changed paths through direct disposable CLI and native GUI probes and independent reviews. Existing tests may be run unchanged, but new or expanded coverage is deferred. Report safety differences and any source-derived behavior still awaiting native proof.
- [ ] Commit only owned Task 2 files and return the commit SHA, task diff/build/direct-probe evidence for spec review, then quality review. Do not claim native equivalence before native acceptance.

**Review focus:** arbitrary copied JSON cannot satisfy Copy report; global activity cannot satisfy page-local log; a heading and colored report cannot satisfy six-row composition; first stage message cannot satisfy severity-prioritized card/log navigation. C# does not explicitly keyboard-focus the issue despite method name: use native observation for focus claims.

## Task 3 brief: actual native comparison and integration

**Outcome:** Bind new product source/binary to driven C#/Rust validation cases and publish bounded evidence plus accurate inventory state. Own only Task 3 outputs. Coordinate with existing runtime worker and consume its pinned C# captures/fixture; do not rebuild or launch duplicate shared sessions unnecessarily.

- [ ] Obtain task1/task1b/task2 reviewed diffs and current HEAD, dirty source hash, executable hash, C# reference hash, fixture hash, private display and process ownership receipt.
- [ ] First drive the actual successful single-instruction disposable Copy fallback through C# and Rust, then the corrected missing-source warning and independent environment failure; preserve the fallback even if full-build prerequisites fail. Record source/output tree hashes unchanged after validation. Source-generated or fabricated screenshots cannot substitute.
- [ ] Complete audit native matrix: empty/selected, in-flight, clean/warning/error, >5 mixed issues, splitter/reset, immediate copy/full flushed text, manual first issue, stale input, setup fail/cancel. Use identical engine fixtures where possible and explicitly separate injected projection-only fixtures.
- [ ] Capture actual clipboard and compare timestamp/duration-normalized report, counts/card order/first issue; compare matching screenshots at1280×800 and600×400, themes/DPI, keyboard and accessibility. Report blocked cells precisely rather than asserting whole-page parity.
- [ ] Update GUI-INVENTORY/ACCEPTANCE/COMPATIBILITY with proven and open contracts, intentional safety differences and evidence links. Historical d4fdb45 receipt stays unchanged.
- [ ] Final spec and quality reviews cover all changed product files and native claims. Commit owned Task 3 evidence/docs for SDD review; publication remains with the designated integrator after final reviews. Full C# GUI objective remains open beyond page scope.

## Progress and dispatch protocol

Persistent scratch: `.superpowers/sdd/validation-page-plan/progress.md`. Task 1 is approved and the backend audit/probe is complete. Task 1b implementation and fix review are complete through88b2dd9. Task 2 is implemented through47c3e67; Task 3 documentation preparation runs with its final native evidence now archived for integrated review. Each fresh worker receives **latest user ruling + Global Constraints + its complete approved task brief + audit path**, exact ownership, and current compiler-slot status. Worker must not infer approval to compile from dispatch. Parent sends findings back to the implementer and obtains fresh review before proceeding. No user check-in between authorized tasks.
