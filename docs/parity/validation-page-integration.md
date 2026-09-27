# Validation page integration checkpoint

## Final archive diagnostic correction — 2026-09-27

Whole-package review found one P2 combined-path defect: known component archive errors lacked a severity marker, so the shared presentation displayed only an aggregate card and first-issue navigation could select the final generic failure. Scoped specification/quality review approves `099834c` and closes F1. That commit fixes the shared failed-outcome mapping with an idempotent `ERROR:` prefix, preserving component identity/detail. **Raw Report message values now include that prefix; serialized schema, stages, counters and warning behavior are unchanged.** This qualifies earlier transport-compatibility statements: schema stability does not imply unchanged diagnostic text.

Separate [focused archive-fix evidence](validation-archive-diagnostic-fix/README.md) records missing Extract **1/1/4**, corrupt Extract **3/0/3**, unchanged missing Copy **0/1/5**, native actionable cards, automatic/manual first-issue targeting and actual clipboard readback. Strict installs reject before snapshots/pending/game/mod content writes; expected lock files and empty state directories outside content roots are recorded, not hidden. The earlier47c3e67 native package retains its original binding. No Rust tests were created/adapted/run, and no full-parity or all-target compilation claim follows.

Documentation preparation, 2026-09-27. **Final native handoff is archived; whole-package review and its sole corrective scoped review are complete; full C# parity remains open.** This checkpoint records reviewed production contracts and evidence ownership; it does not declare exact C# GUI or full feature parity.

## Delivered shared contracts

Task 1 (`915ec69..9d77f4b`) adds pure `validation::presentation::{project, format_report}` and first-issue/solution helpers, plus typed `StageProgress` and additive `validate_report_with_options_observed` / `validate_report_remaining_observed` engine APIs. Original entrypoints retain no-op observers. Progress brackets actual stages; setup failure cannot fabricate completed stages. Human report formatting includes the reviewed final whitespace trim. CLI report serialization and ordinary Event transport stay unchanged.

Task 1b (`9d77f4b..88b2dd9`, including reached-option correction) makes report Conflicts use declared dependency/restriction semantics, scopes ComponentValidation to read-only archive/destination checks, and handles typed ordinary Copy/Move/Extract source absence as one component-scoped DryRunSkipped warning. It stops subsequent simulated instructions of that component and continues to the next component, preserving earlier virtual effects. Malformed paths, corrupt archives and known FOMOD state failures remain errors. Reached Choose options reuse executor selection/applicability helpers and retain their own FomodChoices/ResourceRegistry; completed subtrees are excluded from remaining checks.

Standalone file-conflict APIs, strict ordinary validate, actual installation preflight/no-skip behavior, pending recovery guards, completion roots/serialized definitions/game hashes and stage observer boundaries remain protected. No second installer, receipt reader/editor or GUI diagnostic engine was introduced. Direct disposable CLI receipts and independent reviews are in [missing-source fix evidence](validation-missing-source-fix/) and the task scratch review files. Before/after manifests prove unchanged recorded files at checkpoints, not absence of every transient syscall.

Historical measurement correction: pre-fix partial `--dry-run` **2/0/1** was measured; alleged full **2/0/4** was inferred. The independently source-bound full report at9d77f4b was **3/0/3**, documented in [the authoritative receipt](validation-missing-source-evidence/current/receipt.json). Task 1 report and review annotations preserve this history. Corrected missing Copy is **0/1/5**, matching pinned native C#; present/restored Copy is **0/0/6**. Missing Copy must not be described as a hard Environment failure. Rust pending recovery and C# absent bundled patcher are distinct hard failures despite equal1/0/0 counters.

## Delivered desktop surface and current evidence ownership

`src/ui/validation_page.rs` owns page-local state and rendering; `src/ui.rs` routes it and transports run-ID/input-stamp-bound stage and terminal messages. It consumes shared projection and validation. Separate log/result scrolling, splitter, reset, shared human-report copy with timed feedback, first-issue navigation and themes are implemented. Utility validation remains reachable from Guide/settings, preserving base-only settings, trust controls and conditional patcher controls. Late/stale/cancelled work cannot make navigation ready. Small-window interaction, projection resets, empty progress headers, theme-state tokens and terminal stop reasons received corrective source review.

| Evidence location | Source binding | Claim boundary |
|---|---|---|
| [Pinned C# reference](validation-reference/README.md) | d31ccbd | Native clean0/0/6, missing-Copy0/1/5 and missing-patcher1/0/0; clipboard readback unresolved |
| `/tmp/modsync-rust-validation-task2/` root receipt | 556b165 | Earlier clean/copy/warning/error/theme/small captures; retains original binding |
| `/tmp/modsync-rust-validation-task2/final/` | 5548736 | Later utility/stale/cancel evidence; not evidence from47c3e67 |
| `/tmp/modsync-rust-validation-task2/fix3/` | 47c3e67 | Parent/reviewer confirm actual600×400 run, Stop confirmation and cancelled Next blocked; bounded core matrix and full binding now archived |
| Durable final native package | `47c3e67`, [archive](validation-native/README.md) | Final source/binary/fixture hashes independently reconciled; integrated review pending; no whole-page acceptance |

The final binary SHA256 is `5a77c2c24d9f067a1033f60f765ece53a971955a921c6e437f67b9a73eedb34c`. The copied executable, current source/font files and every final receipt artifact were independently hashed before archival; core before/after manifests are equal. Historical native artifacts and prior test logs are never rebound to a later commit. This documentation task ran no compiler, tests or GUI interaction; compiler availability must be obtained from the parent, not inferred from an earlier release note.

## Explicit rulings and remaining limits

- Warning footer remains truthful: “Validation passed with {n} warning(s). Review the results before continuing.” C# promises automatic handling, which the shared backend does not generally establish. This remains an exact-copy deviation.
- Raw `Report` JSON preserves absent optional solution fields. Pure shared presentation derives only known source-grounded solution fallbacks. Projected text is not literal serialized-solution equality and must not rewrite raw severity.
- The user defers all creation, porting or expansion of Rust tests until full C# feature parity: repository, inline, temporary and custom Rust harnesses alike. Existing tests remain untouched. Obsolete inline worker fixture API adaptation and missing-source error assertions are deferred; do not introduce a fake unused legacy production message variant or remove tests to appease them. Production-only builds/lints do not prove all-target/test compilation.
- Rust proactively disables Next until valid current success; pinned C# can leave Next clickable and apply a click-time guard. Retain the safety gate and disclose the difference.
- Bundled Noto Emoji fallback preserves primary font faces but renders monochrome symbols. egui/Avalonia font metrics, glyph rendering, focus/accessibility and exact typography are not established as equivalent.
- Registry automatic extraction, Move subfolder/target-basename fallback, mixed-source aggregation, archive heuristics/AttemptFixes and general nested-option parity remain open. The reached-option FOMOD fix proves its bounded execution-scope cases, not all option semantics.
- C# native clipboard readback, bilateral normalized human report comparison, more-than-five mixed issues, full keyboard/accessibility, DPI, exact theme metrics and supported-platform coverage remain open unless the final handoff supplies matching direct evidence. No fabricated projection fixture or screenshot replaces a real driven pipeline case.

Task 3 must preserve the successful real Copy fallback, classify warning and independent hard error separately, package full source/binary/input/capture/clipboard/manifests with per-case bindings, and obtain final specification/quality review. F08/F24/F25/F26 remain partial and F29 unverified. No full-build, live-game or whole-GUI acceptance follows from this page slice.
