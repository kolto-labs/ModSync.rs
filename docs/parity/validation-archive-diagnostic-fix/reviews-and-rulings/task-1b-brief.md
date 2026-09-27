# Task 1b — correct shared missing-source validation before renderer work

Status: ready for fresh implementer dispatch; implementation not started.
BASE: `9d77f4ba9955ea505d3eb7969afdf5b5aee73b72` on `codex/rust-port`.
Prerequisite: Task 1 shared presentation/progress and its formatter fix are approved. Task 2 renderer **must wait for Task 1b implementation and independent specification/quality approval**.

## Mandatory current user instruction

Do not create, port, expand, or add Rust tests of any kind until the overall full C# feature-parity goal is complete. This includes repository test files, inline test modules, temporary Rust test files, custom Rust test/probe harnesses, and test additions disguised as disposable scripts. Preserve existing tests; do not delete or alter their assertions to conceal a behavior change. Use production builds and direct invocations of the real CLI/native app against disposable input files, plus independent review. This user ordering overrides earlier test-writing defaults.

## Read before editing

- `AGENTS.md` and the Global Constraints/latest user ruling in `docs/parity/validation-page-plan.md`.
- `docs/parity/validation-missing-source-audit.md` — source-derived rules, exact source references, action/root/archive matrix, shared correction and remaining resolver limits.
- `docs/parity/validation-missing-source-evidence/current/receipt.json`, `stdout.json`, `build-receipt.json` and unchanged manifests.
- `docs/parity/validation-reference/README.md`, `guide.json`, `receipt.json` — pinned C# native oracle `d31ccbd3bfcddb4705917eafa6483715c166de3b`.
- `.superpowers/sdd/validation-page-plan/progress.md` — compiler ownership and historical count correction.

## Concrete defect and acceptance target

With the exact selected native Copy guide, existing game Override directory, empty ResourceRegistry, no Extract, and only its disposable `mods/sample.tga` removed:

- Pinned C# native: **0 errors / 1 warning / 5 passed**, warning-only valid report.
- Current Rust production, measured with `--validation-report --full --dry-run --use-file-selection`: **3 errors / 0 warnings / 3 passed**, exit 1, empty stderr. Conflicts, ComponentValidation, and DryRun fail.
- Earlier partial `--dry-run` **2/0/1** was measured; alleged full **2/0/4** was an extrapolation and is superseded. Do not reproduce that unsupported full claim.

Correct the shared backend so the missing loose-source fixture produces five clean stages and a passed-with-warning DryRun, global **0/1/5**, success true, and the exact component-scoped issue `Warning`, category `DryRunSkipped`, message `Skipped 'Fixture' during dry-run: source archives or files are not available.` A restored source produces **0/0/6**. This must arise from generic component/archive/source rules, not the fixture name, file extension, UI rewriting, or a fake report.

## Ownership

- `src/engine.rs`: shared report stage work and typed source-outcome propagation through existing resolver/executor.
- `src/validation.rs`: shared report construction needed for declared conflicts, archive results, and component-scoped dry-run warning. Keep public JSON shape unchanged.
- A small production-only shared outcome/module file and `src/lib.rs` only if genuinely necessary; prefer the existing engine/validation boundary. Declare any added path in the handoff.
- This task's report/diff/evidence beneath `.superpowers/sdd/validation-page-plan/`, and durable direct-probe receipts beneath `docs/parity/validation-missing-source-fix/` if needed.

Do not edit `src/ui.rs`, `src/main.rs`, `src/phase.rs`, presentation/renderer modules, C# source, recovery format/verifier, Rust test files/modules, user-owned `AGENTS.md`, or other workers' docs. You are not alone in the repository; preserve unrelated changes and do not revert them. No nested subagents. Commit only explicitly owned production/evidence files for parent review; no push/publication.

## Required production changes

1. **Correct report Conflicts stage semantics.** The C# pipeline checks declared dependency/restriction context. It does not execute Copy to discover source availability. Implement the small shared read-only declared analysis for this report stage using selected components and completed-prefix dependency context. Do not run `analyze_conflicts` and then suppress a missing-source error. Preserve the standalone simulated-write conflict API, its JSON, its no-partial-success rule, and remaining receipt verification unchanged.
2. **Use actual component archive/destination validation scope.** The structured report ComponentValidation stage must not call the complete strict install simulation. Reuse existing safe path/archive primitives to check applicable Extract archives and required archive members/destinations in the source-derived scope. A no-Extract loose Copy/Move source's absence does not itself fail this stage. Missing declared Extract archives, bad destination/path, corruption and relevant archive-member errors remain real errors. No directory creation, C# AttemptFixes writes, new file lookup guessing, or installation side effects. Do not label unchecked archives “validated”.
3. **Represent ordinary unavailability as a typed outcome, not a string filter.** Existing resolver no-match must remain a failure to strict install/ordinary validation. Shared report dry-run orchestration may recognize a narrowly typed missing-source cause for the source-resolving actions, stop the rest of that component, and emit one component-scoped `DryRunSkipped` warning. Use the evolving existing VFS, retaining prior successful virtual effects and inspecting later selected components in guide order. Do not pre-scan against the initial disk tree, fabricate sources, or continue later instructions within the skipped component. Preserve unrelated hard diagnostics; corruption, malformed paths/patterns, permission errors, cancellation and safety/recovery failures are not ordinary absence.
4. **Keep FOMOD and recovery gates independent.** Missing registered archives with prior prompt/configuration state, unreadable/unconfigured FOMOD, pending recovery, receipt root/definition/game-byte mismatch, and unsafe paths remain hard failures. Generic missing Extract source availability belongs to the archive stage/dry-run outcome, not a fabricated duplicate FOMOD inspection error; do not weaken known-state protection. Changes here must follow the cited C# gate distinction and preserve existing registered-state failure cases.
5. **Preserve strict actual install and phase lifecycle.** `Engine::install` still uses strict preflight before snapshot/pending/live writes and still stops on failure. Ordinary `validate()` and standalone conflicts remain strict. No new continuation/replay/backfill/automatic restore policy, no receipt reader/editor, no second installer/coordinator, no completed-ID CLI flag. Base/remaining preparation and completed definitions remain unchanged.
6. **Preserve the observed API contract.** Work remains inside the existing stage boundaries in `validate_report_with_options_observed`; old methods wrap it, remaining uses the same shared path. Keep stage order, enabled-stage total, true start/end notifications, one-based steps, environment early exit and prefix-rejection behavior. Do not emit made-up progress or call observers twice to compensate for skipped execution. Keep stdout/stderr/event/report shapes; changed warning/error values and resulting success exit for this corrected report are intentional semantics.

## Exact existing interfaces to reuse

- `Engine::validate_report_with_options_observed` and `validate_report_remaining_observed`: shared six-stage owner and remaining wrapper. All GUI/CLI report consumers converge here; do not add an adapter-specific path.
- `Engine::validate_dry_run`, `execute`, `instruction_applies`, `validation_target`, `validation_warnings`: existing simulation, selection gates and issue context. Reuse the executor; change orchestration for the typed component outcome.
- `Engine::matches`: currently emits untyped no-match errors; `Roots::resolve` and `Roots::check` already enforce path boundaries. Add only the missing-source distinction; no relaxed matching in real install.
- `Guide::selected_ids`, `Component.dependencies`/`restrictions`: declared-context inputs. `Guide::ordered`/`Engine::check_component` are strict and are not a replacement for warning-capable declared conflict results. Their actual installation protections stay intact.
- `archive::unpack`, `archive::verify`, existing `Roots` methods: existing safe archive/path primitives. There is currently **no** standalone Rust component archive-stage helper and **no** Rust `MissingSourceFiles` enum. Do not claim these exist; implement the minimum shared report/outcome support rather than copying another installer.
- `validate_fomod_configuration`, `recovery::ensure_clean`, `recovery::verify_completion`, ordinary `install` preflight: existing separate gates that must remain authoritative.
- `Report::dry_run` already supports warning-only validity and C# error/warning count precedence. Preserve public report structures and existing presentation mapping; do not change UI severity.

## Deliberate scope limits to report honestly

C# source resolution includes registry auto-extraction, Move subfolder remapping, an already-present destination basename fallback without hash checking, and aggregate multi-source matching. Current Rust `matches` does not provide those semantics. Do not implement speculative lookups or silently import unsafe Move success/continuation inside report code. The measured absence and declared-archive scope correction is this task; enumerate remaining resolver feature gaps with their exact source references for later shared-executor work. Exact full-project parity remains open.

The existing test `missing_copy_source_remains_an_engine_error_without_gui_reclassification` encodes the known wrong report behavior. Do not edit it under the user directive or retain the defect merely to keep it passing. Record this known stale assertion in the handoff and use the allowed production evidence below; no new test-writing dependency.

## Allowed direct acceptance evidence

After obtaining the parent's compiler slot, use `CARGO_TARGET_DIR=/tmp/modsync-rs-target` and bounded jobs (normally `-j2`). Capture current HEAD/source hashes, production build command/output, copied private binary hash, input bytes/options, stdout/stderr/exit, and before/after game/mod/recovery manifests. No real installations, downloads, user GUI processes, or existing recovery state.

- Run the exact pinned guide through the real CLI full report with source present, removed, and restored. Require actual **0/0/6 -> 0/1/5 -> 0/0/6**; inspect all six stage outcomes and exact warning fields/message, not only totals.
- Drive disposable Copy/Move cases under mods/game/kotor roots, zero-match wildcard, preceding source-producing instruction, missing first instruction followed by a write, and earlier successful virtual work followed by missing source. Confirm no actual validation writes and no fictitious later instruction execution within a skipped component.
- Probe missing Extract archive, unreadable/corrupt archive, absent required member, and invalid destination separately. Keep genuine errors and exact scope. Preserve hard FOMOD known-state and receipt/pending failures.
- Probe unsafe path, malformed glob, cancellation and permission failure where the disposable environment can establish them; none may become an ordinary availability warning.
- Invoke real `install`/`install_remaining` only on newly owned disposable fixtures for the warning-only missing-source cases. Require strict rejection before game/mod writes or snapshot/pending/completion promotion, no later-component output, and unchanged completed base definitions. Ordinary lock/state-directory creation may be documented separately. Preserve late runtime failure/restore semantics; no skip option.
- Build default and no-default-feature production variants, run format/appropriate strict Clippy under compiler ownership, and review observed callback boundaries. Do not introduce a Rust harness to inspect callbacks; use existing native observed integration when available and mark source-only progress evidence honestly until driven GUI verification.
- Native acceptance can reuse the existing pinned C# reference; do not duplicate its build/session. The following renderer task will drive corrected shared outcomes visibly. This task closes backend data semantics, not native visual parity.

## Handoff/review gate

Record exact production/evidence diff, commands/exits, binary/input hashes, stage/counter/issue results, unchanged manifests, strict-install observations, known stale existing assertion and remaining resolver limits in `task-1b-report.md` and `progress.md`. Release compiler ownership explicitly. Parent dispatches fresh specification and quality review; fixes stay with the implementer. Only after both approve may Task 2 consume this report contract.
