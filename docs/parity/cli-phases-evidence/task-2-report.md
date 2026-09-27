# Task 2 report

Status: implementation and executable regressions finished; independent review required before Task 2 acceptance.
BASE: `4010aece72e901203c71ddc5ed6b61a0005f2ca5`.
HEAD: `9714ab1a59d9633280c79418287a52692bd17073` (only the two owned source/test files committed).
Owned files: `src/main.rs`, `tests/cli_phases.rs` only.

## Implementation

Shared InstallArgs now exposes `--phase all|base|remaining`, default All. Existing CLI profile/reset/filter/name/GUID/option preparation runs first; external choices are parsed and handed to the reviewed shared `phase::prepare`. Remaining uses `CompletionContext::Reconstruct`; All and Base use None. No phase projection, completion receipt reader/editor, choice application, coordinator, or new recovery format was duplicated in the CLI.

All/Base call ordinary engine operations. Remaining calls existing remaining validate/report/conflict/install methods with internal prepared IDs. Remaining report is always the existing full-plus-dry-run operation: redundant `--full`/`--dry-run` are accepted and `--dry-run-only` rejects. Report-shaping flags reject on phased install/conflicts. Phased Direct Markdown rejects before mutation. Phased report selection warnings do not leak into stderr; All retains its existing behavior.

Preparation/prefix errors produce one ErrorReport on stdout, empty stderr and exit 1. Receipt/pipeline errors produce one Report with success=false and inputPath, empty stderr and exit 1. Normal/JSONL preflight errors have a nonzero exit, diagnostic stderr, and no invented stdout event. Existing report and event schemas are unchanged.

## Executable evidence

`task-2-focused.log` contains exact executable invocations and complete stdout/stderr. `progress.md` records commands, SHA256 values and proof boundaries; the focused log includes complete path-to-SHA256 manifests for the happy route after Base and Remaining. Test assertions compare exact bytes (not just hashes) of every path, including recovery snapshots/receipt/pending, for validation, conflicts and rejected calls.

Six executable tests cover:

- One unchanged selected guide: All report sees Base+Wide; Base Move consumes its source and writes exact Base only; Remaining validation (fixed report and JSONL) and conflicts see only Wide without writes; Remaining installation writes exact Wide while preserving exact Base and records ordered Base+Wide. Successful phases have no pending marker. Both redundant report flags are accepted.
- Missing/corrupt/wrong-root receipts, changed Base definition and tampered Base bytes reject validation/conflicts/install without any further tree or recovery mutation.
- Empty base/prefix/suffix, non-prefix reconstruction and exhausted selected guides reject. Direct Markdown and incompatible phase/report flags reject.
- Missing/corrupt/omitted/changed choices and suffix-only whole-payload changes reject. Stable unchanged choices validate and install successfully across invocations.
- Real temporary patcher failure after the suffix's first copy leaves pending; all Remaining operations reject; explicit CLI restore reinstates exact Base receipt/tree and clears pending; fixing the external stub then permits suffix-only retry.
- Explicit repeated Base with Copy/Overwrite succeeds under ordinary engine semantics; consumed-Move repeated Base fails without writes, and repeated Remaining fails receipt verification.

## Rulings, self-review and limitations

Parent accepted the repeat-policy clarification: a fresh explicitly requested Base retains ordinary engine behavior, with no automatic replay. A universal successful-Base repeat prohibition would require a later shared restart policy and was not invented in the CLI. Tests include direct repeat-Copy evidence, so the broader brief phrase “repeated phases reject” must not be used as a blanket product claim.

Choices tests prove the current serialized whole-payload metadata/reconstruction contract; they do not claim full FOMOD UI parity. Even suffix-only choice edits can invalidate reconstructed Base definitions. No actual game installation, desktop visual parity, full-build proof, publication, or push is claimed.

Self-review checked default-All dispatch compatibility, unchanged schemas and engine verification, report-vs-normal error routing, post-selection shared preparation, and ownership. All-mode warning behavior was explicitly preserved in the final adjustment. No shared API defect was found. No UI/shared engine/recovery files or user-owned AGENTS/planning documents were changed. Task 1's deferred P3 duplicate-ID test isolation remains deferred; Task 4 owns COMPATIBILITY and integration documentation.

## Checks

Every Cargo invocation used `CARGO_TARGET_DIR=/tmp/modsync-rs-target`, `login:false`, and serialized execution.

- `cargo test --test cli_phases -- --nocapture --test-threads=1`: exit 0; 6 passed, 0 failed. The detailed log predates only the final All-warning compatibility condition; the final complete suites rerun all six tests.
- `cargo fmt --all -- --check`: exit 0.
- `cargo clippy --all-targets -- -D warnings`: exit 0; `task-2-clippy.log`.
- `cargo test --all-targets`: exit 0; 225 passed, 0 failed, 1 intentional ignored; `task-2-all-targets.log`.
- `cargo test --no-default-features`: exit 0; 198 passed, 0 failed, 1 intentional ignored; `task-2-no-default.log`.
- `git diff --check`: exit 0.

Review package: `task-2-review.diff`, exact BASE..HEAD for the owned commit. Parent must independently review before marking Task 2 complete.

## Fix round 1 — All report compatibility

Review finding: P2, All preparation called `Guide::ordered()` before the existing report pipeline, turning invalid dependency/order/empty-selection Report failures into ErrorReport documents. Fix BASE is `9714ab1a59d9633280c79418287a52692bd17073`. Parent expanded ownership to `src/phase.rs` and `tests/phase_preparation.rs` for the necessary shared correction.

The shared function now defers All/None ordering and empty-selection checks to existing engine operations. Choices still apply through the same shared path. Base still validates its selected projection eagerly; Remaining still validates its ordered completed prefix and nonempty suffix eagerly. No main.rs bypass, synthesized report, duplicated preparation, receipt reader or policy exception was added.

Ruling: preserving the existing All pipeline contract takes priority over uniform eager preparation checks. Cost if wrong is delayed error timing for callers. Explicit `--direct-markdown` transformation remains excluded from phases; ordinary Markdown ingestion is allowed and retains receipt definition verification.

New real-executable regression covers default and explicit All for both a missing selected dependency and an empty selection. It asserts exit 1, empty stderr, one Report with inputPath and correct componentCount, failed Conflicts/InstallOrder/ComponentValidation stages, absence of the scalar ErrorReport error field, and exact unchanged manifests. An unmatched category filter creates empty selection, because the existing loader defaults an entirely unselected file to all selected. The loader's rejection of an actually component-free guide remains unchanged.

Red/green evidence: `task-2-fix1-before.log` shows the new regression initially failing on missing inputPath with the original scalar ErrorReport (cargo exit 101). Final `task-2-fix1-focused.log` passes all seven CLI tests plus four shared preparation tests. Shared coverage now expects empty/invalid All preparation to succeed while engine ordering remains invalid, verifies external choices are still applied exactly, and confirms invalid Base remains rejected.

Self-review: the production correction is five changed lines in shared preparation, keeps the context matrix intact, and does not move phase logic into adapters. Task 1's deferred duplicate-ID P3 is unchanged. No GUI files, owned user edits, durable plans, or recovery semantics were changed.

Fix HEAD: `312ee167a4732f4095f8c966595fd3b2185396d9`; committed only `src/phase.rs`, `tests/cli_phases.rs`, `tests/phase_preparation.rs`. Exact BASE..HEAD package: `task-2-fix1-review.diff`. Independent re-review remains required; no push.

Final fix checks (same serialized Cargo target and login:false): `cargo fmt --all -- --check` exit 0; `cargo test --test cli_phases --test phase_preparation` exit 0, 11 passed; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --all-targets` exit 0, 226 passed/0 failed/1 intentional ignored; `cargo test --no-default-features` exit 0, 199 passed/0 failed/1 intentional ignored; `git diff --check` exit 0. Logs use the `task-2-fix1-` prefix.
