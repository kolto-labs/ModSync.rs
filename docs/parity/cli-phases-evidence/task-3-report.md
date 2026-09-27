# Task 3 desktop adapter report

Status: implementation and required checks passed; awaiting independent review. Task 3 is NOT marked complete.

BASE: `312ee167a4732f4095f8c966595fd3b2185396d9`.
HEAD: `3d106ef0c3cc9687864bcdf0a3c988bbd2c6d7c2`.
Commit owns only `src/ui.rs`. Exact BASE..HEAD diff: `task-3-review.diff`.

Before starting, recorded Task 2 fix-round-1 independent PASS and Task 2 completion `4010aece72e901203c71ddc5ed6b61a0005f2ca5..312ee167a4732f4095f8c966595fd3b2185396d9` in `progress.md`.

## Implementation

The desktop preparation adapter now delegates to `modsync::phase::prepare`. Pre-base Validate maps to All/None; pre-base Conflicts and Install map to Base/None; all three remaining operations use Remaining/Retained with the successful completed IDs. The phase preview uses the same shared API without external choices. The adapter owns routing and choices-file I/O only; it has no independent widescreen projection, prefix validation, or choices-application policy.

The existing guarded success overlay remains: capture serialized loaded guide and roots, retain the effective ordered Base definitions during work, and overlay them only after matching guarded success. Completed IDs are derived from those effective definitions. Terminal install messages now clear pending definitions and dispatch context on failures/stale completions as well as success, ensuring failed definitions cannot be promoted later. Preparation errors use the existing Done failure handler without starting an engine operation. Choices-file parse/read errors preserve the existing `FOMOD choices:` setup diagnostic.

No selection, navigation, worker/report schema, receipt comparison, recovery, engine, CLI, shared API, or FOMOD editor/save logic was changed. The existing empty-widescreen navigation skip still dispatches no operation. User-owned AGENTS edits and untracked durable plan/preflight files remain untouched.

## Focused evidence

Three new colocated tests cover:

- All and Base prepared-guide equality at identical post-selection inputs, including external choices, ordered Base-only selection, no completed IDs, no input mutation, and exact temporary-root byte manifests. Invalid All still reaches the full desktop pipeline and returns a failed report rather than an early preparation rejection.
- A real desktop Base Move consumes its source, writes actual bytes and a receipt, and overlays the exact effective definition. Stable GUI Retained and CLI-style Reconstruct preparation compare equal and their full remaining reports compare equal. A changed completed choice and a suffix-only external edit both preserve the GUI historical prefix while reconstructed definitions fail receipt verification. Failed reconstruction installs leave the exact full temporary-root manifest unchanged. GUI remaining validation/conflicts are also mutation-free and remaining installation writes only the suffix without replaying the consumed Base. Final receipts retain the exact Base definition and include both completed definitions; pending markers are absent.
- Empty, duplicate, unknown and mismatched retained IDs reject Validate/Conflicts/Install with no state promotion and unchanged full temporary-root manifests. The duplicate case uses three selected components so it reaches the duplicate guard independently of exhausted-suffix rejection. A failed Base worker clears pending definitions/context, leaves game/mod bytes unchanged, and creates no completion receipt or pending marker. Ordinary engine lock/empty state directories may be created on that failed Base attempt; the test does not claim a wholly unchanged temporary root for this case.

The existing stale completion regression now also covers root changes, alongside guide replacement and reset. The existing consumed-source external FOMOD before/after-Base test remains unchanged and passes all its editor/external/unchanged branches. Empty-widescreen navigation and the existing failed-choice-setup diagnostic tests pass.

## Commands and results

Commands ran serially from `/home/brunner56/Workspaces/ModSync.rs`, with `login:false` and `CARGO_TARGET_DIR=/tmp/modsync-rs-target` for every Cargo command.

- `cargo test --bin modsync ui::tests:: -- --test-threads=1`: 30 passed, 0 failed; `task-3-focused.log`.
- `cargo fmt --all`, followed by `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed; `task-3-clippy.log`.
- `cargo test --all-targets`: 229 passed, 0 failed, 1 intentional ignored 513 MiB fixture; `task-3-all-targets.log`.
- `cargo test --no-default-features`: 199 passed, 0 failed, 1 intentional ignored; `task-3-no-default.log`.
- `git diff --check`: passed.

During test development, an Engine ownership compile error was fixed by constructing a fresh engine per consumed operation. The first executable focused pass exposed two incorrect new-test assumptions (remaining validation represents a definition mismatch as a failed report, and failed Base may create lock/state directories) and an existing setup-diagnostic regression. Tests were corrected to the existing engine contract; the production choices setup diagnostic was restored. Final focused/full checks above include those corrections.

## Self-review and remaining obligations

No shared API defect was found. All/None retains Task 2's corrected delayed engine validation. Base/Remaining strict preparation failures cannot promote completion or dispatch filesystem mutations. Receipt definitions are used only by tests as evidence, never as production preparation input. Historical Base definitions continue to come from the successful desktop worker, not current choices or a receipt reader.

The frontend comparison begins after selection and uses the same shared preparation API the CLI adapter uses. It does not claim equal CLI/GUI selection semantics, native GUI interaction proof, full exact C# parity, actual patcher/native FOMOD dialog proof, or a real K1/K2 full build. Tests use temporary fixtures and actual shared engine operations only.

Task 4 retains `COMPATIBILITY.md`, `GUI-CLI.md`, `ACCEPTANCE.md`, integration review and durable status obligations. Document retained GUI versus separate-invocation CLI reconstruction behavior, including whole-payload suffix-only choice-edit rejection, the precise explicit `--direct-markdown` phased exclusion, normal Markdown's existing evidence contract, and remaining conflict receipt verification. No push or public PR was performed. Cargo target ownership is released for the parent after these completed checks.
