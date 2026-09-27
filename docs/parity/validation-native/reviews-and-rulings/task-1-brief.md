# Task 1 worker brief

## Latest explicit user ruling

Defer **all creating, porting or expanding Rust tests**, including inline test modules and test files, until the overall full C# feature-parity goal is complete. This explicit user ordering takes precedence over skill/AGENTS test-writing defaults and earlier plan gates. Preserve existing tests and recorded evidence. The cost is deferred automated regression coverage for subsequent changes; use builds, direct disposable CLI probes, driven native GUI probes and independent specification/quality reviews now. Existing tests may still be run without adding or expanding them, under parent compiler ownership.

Task 1 is approved over `915ec69..9d77f4b`; its fix-round-1 P2 formatter finding is addressed in `.superpowers/sdd/validation-page-plan/task-1-fix1-review.md`. Preserve its pre-ruling tests and evidence. `phase_task_review` is now auditing missing-source shared-backend behavior and exact full-CLI measurement. A Task 1b may be inserted before UI work after that audit; its scope is not yet defined.

## Global Constraints

- Apply the latest explicit user ruling above in every worker brief: no new, ported or expanded Rust tests until overall feature parity is complete. Preserve existing tests/evidence; do not remove them to satisfy this ordering.
- BASE Rust `915ec69a21d90fac0395932d54e1622f8c049a18`, branch `codex/rust-port`; C# oracle `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Re-read current status/HEAD before work. User-owned dirty AGENTS.md must remain untouched. Other agents own audit/runtime docs; never revert their edits.
- Product implementers are not alone in this checkout. Own only the paths listed in the task; serialized product edits. Parent remains hands-off. Do not spawn subagents. Implementers must commit only their owned task changes for SDD review, using explicit file paths when staging and preserving all other changes. Publication/push remain with the designated integrator after final reviews under existing authorization. Do not modify C# without a separate parent instruction. The audit/planning worker alone has a no-commit restriction.
- Parent controls the compiler slot. Reconfirm the active owner with parent; no build/test/compiler process while another worker owns that slot. Read/edit work may proceed independently. Use the existing parent-approved controlled target directory and bounded jobs; never clean an unowned target or stop another process.
- Shared validation never mutates mod/game trees or runs external programs. Tests use isolated temporary roots. Preserve guide order, completed-prefix verification, no replay of consumed base Move sources, and pending-snapshot retry block.
- Preserve CLI flags, JSON shape, ordinary Event contracts and stdout/stderr/exit behavior. Additive APIs wrap old methods with no-op progress observers. CLI must remain usable with `--no-default-features` and never import UI types.
- Preserve trust/consent controls, stale-input invalidation, close/Stop safety and successful single-step fallback when available. “Exact C#” does not authorize making validation unsafe or pretending synthetic progress is real. Existing proactive Next gating vs C# click-time guard is an explicitly documented difference; do not weaken it.
- Exact text, dimensions, colors, stage/card rules and source pointers have one source: `docs/parity/validation-parity-audit.md` (the specification). Required native evidence and ownership are in `docs/parity/validation-runtime-plan.md`. Do not replace those requirements with a new simplified interpretation.
- Full C# GUI parity remains open beyond this page. Automated tests/source/synthetic cards do not prove native parity; historical d4fdb45 screenshots do not cover current or new code.


## Task 1 brief: shared presentation and genuine stage progress

**Status:** Approved `915ec69..9d77f4b`, including addressed fix-round-1 P2. Contract below records delivered scope; do not redispatch it or add tests. Future backend work requires the pending audit and a separately defined Task 1b.

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

**Specification review:** independently check every audit mapping row, golden report line, enabled-stage schedule, actual callback sites, JSON/CLI compatibility and read-only roots. **Quality review:** check ownership, duplication, callback panic/error behavior, counter drift, unknown handling and evidence independence. Task 1 reviews passed; Task 2 also waits for the pending backend/full-CLI audit and any resulting bounded Task 1b.


Reconfirm compiler ownership with parent before builds or unchanged existing-test runs. No Rust test creation, porting or expansion until overall full C# feature parity is complete.
