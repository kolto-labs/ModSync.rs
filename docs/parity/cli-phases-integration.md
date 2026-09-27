# Shared GUI/CLI phases — integration evidence

Implementation HEAD: `3d106ef0c3cc9687864bcdf0a3c988bbd2c6d7c2`.
Package BASE: `872b1dec4b5a120661aa604374c38cf043a11943`.
Tasks 1–3 passed independent specification and quality review. Task 4 is complete:
[whole-package final review](cli-phases-evidence/final-review.md) returned
**Specification: PASS; Quality: PASS with nonblocking P3 notes; ready to integrate**
for `872b1de..e157021`. This status update changes documentation only. No full-build,
exact C# GUI parity, new native interaction, or cross-platform proof is claimed.
The original [plan](cli-phases-plan.md) and [preflight](cli-phases-preflight.md)
are historical design inputs. The accepted rulings below supersede their universal
repeat-Base prohibition and their provisional “still unproven” implementation list.

## Public contract and example

`validate`, `conflicts`, and `install` accept `--phase all|base|remaining`, default
`all`. Selection/profile behavior is unchanged. Equality between frontends starts
with identical selected guides: CLI reset/filter/name/GUID/profile selection and
GUI additive selection are still distinct. Both adapters call `phase::prepare`,
then the existing engine operations. No public completed-ID list, coordinator,
receipt reader/editor, recovery format, or second installer was added.

Run with an unchanged guide selecting Base and Wide, where Base Moves base.txt and
Wide has top-level `WidescreenOnly=true`, depends on Base, and Copies wide.txt:

```bash
modsync validate --input guide.json --game-dir game --source-dir mods --use-file-selection --phase all --validation-report --full --dry-run
modsync install --input guide.json --game-dir game --source-dir mods --use-file-selection --phase base --json
modsync validate --input guide.json --game-dir game --source-dir mods --use-file-selection --phase remaining --validation-report
modsync conflicts --input guide.json --game-dir game --source-dir mods --use-file-selection --phase remaining
modsync install --input guide.json --game-dir game --source-dir mods --use-file-selection --phase remaining --json
```

The executable test trace uses temporary absolute roots and the loader's existing
selected flags (without needing `--use-file-selection`); its exact commands are in
[task-2-focused.log](cli-phases-evidence/task-2-focused.log). All five operations
exit 0 with empty stderr: All reports two components; Base emits only Base plus
ordinary snapshot events and consumes its source; Remaining report and conflicts
analyze Wide only and leave game/mod/snapshot/receipt bytes unchanged; Remaining
install emits only Wide, preserves Base bytes, records ordered `[base, wide]`, and
clears pending. Reports retain the existing JSON document format, `--json` install
emits JSONL events, conflicts emits its JSON report, and ordinary progress retains
the existing human-readable format. Remaining report always runs full validation
plus dry run; redundant `--full`/`--dry-run` are allowed, `--dry-run-only` rejects.
Base/Remaining install and conflicts reject `--validation-report` and its shaping
flags. Existing All compatibility, including its report flag behavior, is retained.

| Failure boundary | stdout | stderr / exit |
|---|---|---|
| Validation report setup/preparation/prefix error | One ErrorReport JSON document | Empty / 1 |
| Validation report receipt or pipeline failure | One Report JSON document with `success=false` and `inputPath` | Empty / 1 |
| Normal or JSONL operation preflight rejection | No success event; tested preflight failures have empty stdout | Diagnostic / nonzero |
| CLI parser invalid flag/value | Existing parser diagnostics | Existing parser failure status; not the report dispatcher contract |

Default/explicit All defer order/empty-selection failures to the engine, preserving
failed pipeline stages, component count, inputPath and empty stderr. The initial
Task 2 review caught eager preparation changing these into ErrorReport. The
[fix review](cli-phases-evidence/task-2-fix1-review.md) and red/green logs establish
the correction. Runtime install failures may have earlier progress events; they
do not invent a successful completion.

## Completion, choices, repeats and recovery

Base deselects top-level widescreen components, requires nonempty work, and applies
external choices. Filtering/checking precedes choices application; the current
choices API changes metadata without changing top-level selection or ordering, so
this is behaviorally equivalent to the plan’s provisional order. Remaining requires
a nonempty, unique, selected exact ordered prefix and a nonempty suffix. Remaining conflict analysis now verifies
completion roots, exact serialized completed definitions and completed game hashes
in the shared engine, as validation/install already do. Missing/corrupt receipts,
wrong roots, changed definitions and tampered output reject without further work.

GUI Remaining uses retained effective Base definitions, promoted into the resident
guide only after guarded successful completion with matching guide/root context.
Failure or stale completion clears pending definitions and dispatch context without
promoting them. Empty-widescreen GUI navigation skips dispatch; it does not weaken
shared no-work rejection. CLI Remaining reconstructs Base from the supplied selected
guide and stable choices, then the unchanged engine receipt verifier judges it.

When external choices affected Base, provide the same definition-equivalent
`--fomod-choices choices.json` on both invocations. Metadata embeds the entire
external payload in each unfinished component: even a suffix-only file edit can
change reconstructed Base metadata and reject with
`Completed component definitions differ from receipt`. Missing/corrupt supplied
files fail preparation; omission of choices needed by Base fails receipt comparison.
GUI retained context can accept suffix edits because it already owns the historical
prefix. This is an explicit separate-process CLI limitation, not frontend parity.
Per-component projection, including nested/wildcard/unregistered sources and receipt
compatibility, remains outside this slice.

An explicitly requested Base rerun follows ordinary install semantics: consumed
Move inputs reject before snapshot/write; the Copy/Overwrite fixture successfully
runs Base twice. There is no universal Base-repeat prohibition or automatic replay.
A second Remaining after successful suffix rejects the completion mismatch and
replays nothing. Pending recovery blocks remaining validate/conflicts/install.
For the failed-suffix fixture, a temporary patcher exits 7 after Wide copy; explicit
`modsync restore --game-dir game --source-dir mods` restores the verified Base
snapshot/receipt and clears pending, after which corrected suffix work succeeds in
order. No late backfill or continuation past failure is introduced.

Only explicit `--direct-markdown` transformation is rejected for Base/Remaining.
Ordinary Markdown ingestion remains allowed under the existing definition verifier.
Stable phased Direct Markdown, restart coordination, full K1/K2 builds, native
FOMOD acceptance after these fixes, accessibility and other platforms remain open.

## Source-bound evidence and reviews

[Evidence manifest](cli-phases-evidence/manifest.json) records the implementation
source hashes, archive SHA256 values, and per-task commit bindings. Archived logs
normalize trailing whitespace/EOF blank lines only; original raw logs and exact
task diffs remain in scratch through review. See the [archive README](cli-phases-evidence/README.md). Reports/reviews
retain their historical status wording; this integration record records the later
independent PASS decisions. No suites were rerun for documentation.

| Scope | Commit / evidence | Result |
|---|---|---|
| Shared preparation + remaining conflict verifier | `4010aec`, task-1 review/logs | Independent PASS; focused 50 passed, 1 ignored |
| CLI phase dispatch and regressions | `9714ab1`, task-2 review | Initial P2 All report timing finding |
| Shared All compatibility fix | `312ee16`, task-2-fix1 review/logs | Independent PASS; focused 11 passed |
| Desktop shared preparation and retained history | `3d106ef`, task-3 review/logs | Independent PASS; focused 30 passed |
| Final implementation all targets | task-3-all-targets.log | 229 passed, 0 failed, 1 intentional ignored |
| Final implementation no default features | task-3-no-default.log | 199 passed, 0 failed, 1 intentional ignored |
| Final strict Clippy / format / diff check | task-3-clippy.log and task-3-report.md | Passed; format/diff success recorded in report |

The ignored test is the intentional 513 MiB fixture. Detailed executable trace
predates only Task 2's final All-warning condition and subsequent All-report fix;
final suites include all those tests. Archived trace retains exact commands, exits,
stdout/stderr, event JSONL, Base/final path-to-SHA256 manifests, snapshot and on-disk
receipt hashes. Tests assert byte-backed before/after equality for rejects and
no-write operations. The temporary fixture directories no longer exist; this is
retained log/assertion evidence, not archived original receipt files. On-disk receipt
hashes contain run-specific roots; the trace's separate “phase proof” receipt hash
is JSON reserialization, not the on-disk byte hash.

Guide SHA256: `7d566414266c1486512afc8eeee9a128a02d6c08f87ba4129e553143b0a95bad`.
Stable choices: `5906940f66b9d3fc9e31586f539a9c3511a0b4127b90ad02d48a6201f83e9abf`.
Base bytes: `210a9a455deff70fd7382d26b3d8417400573738a4cff04743b2becf3caf567d`.
Wide bytes: `11c1ff87f61267fc5ee6d4c4127ac313b9a9570f8f49e4dda32ffc630c9d2ec2`.
Base on-disk receipt: `fd1c0cb7e332131199a28180aa431629c7cd01b100b4dd7de2b38e3aaec3cbdc`.
Final on-disk receipt: `9cf3b756e5d7eaf896ad463a7bfe025ef8c53d16c8f60f53d27724be87403077`.

Final review accepts Task 1’s original duplicate-ID test isolation as deferred
nonblocking P3: that fixture also reaches exhausted-suffix rejection and was not
changed. Task 3 independently covers the specific shared duplicate guard with three
selected components across Validate/Conflicts/Install. There is no outstanding
package-level guard-coverage or product safety issue. The reviewer’s minor Base
preparation-order prose correction is incorporated above; no product fix was needed.
Existing [wizard integration](wizard-integration.md) and
[wizard native](wizard-native/README.md) receipts retain their original source and
pre-fix scope. No new native session is implied by automated adapter regressions.

## Current ledger rulings, verbatim

- Task 2 fix round 1 START BASE `9714ab1a59d9633280c79418287a52692bd17073`: independent review P2 reproduced by a new executable regression (exit 101, missing inputPath due to early ErrorReport; `task-2-fix1-before.log`). Ownership expanded by parent to `src/phase.rs` and `tests/phase_preparation.rs` for the shared API correction. Ruling: preserving existing default/explicit All pipeline reports overrides uniform eager preparation validation; defer All/None ordering/empty checks to the engine while preserving choices application and strict Base/Remaining checks. Cost if wrong: delayed error timing in callers. No frontend bypass or duplicated policy.

- Markdown ruling clarified: only explicit `--direct-markdown` transformation is excluded from phases. Ordinary Markdown ingestion is allowed, subject to existing receipt definition verification. Task 4 documentation must use this precise boundary.

- Task 2 ruling (parent accepted): an explicitly requested fresh Base uses ordinary engine install semantics. There is no new universal repeat prohibition; the consumed-Move fixture rejects repeated Base before snapshot/write, and repeated Remaining rejects completion mismatch. A Copy/Overwrite Base was executed twice successfully in the executable regression. Pending recovery still blocks operations. No automatic replay was introduced. Cost if policy changes later: define a shared explicit restart policy; do not implement a CLI receipt reader.

- Ruling: library preparation output carries completed IDs for existing engine calls; no public CLI completed-ID list is introduced. Cost if wrong: keep this internal to adapters.

- Ruling: dispatched remaining preparation rejects exhausted selections. Existing `desktop_empty_widescreen_phase_retains_all_completion_pages_without_replay` is preserved: `advance_wizard` skips worker dispatch entirely when no widescreen is selected. It is navigation, not another engine policy.

- Ruling: one shared phase-preparation function; cost: explicit adapter inputs and context validation.

- Ruling: add shared-engine receipt verification for remaining conflicts first; cost: one bounded engine change and adversarial tests.

- Ruling: CLI reconstruction requires definition-equivalent guide/choices inputs; cost: suffix-only external edits may reject under whole-payload metadata.

- Ruling: preserve `0a4f9dd` GUI retained lifecycle and do not add another completed-definition store.

- Ruling: no coordinator, receipt reader, public completed IDs, or receipt weakening.

Prior wizard rulings remain verbatim in [wizard-integration.md](wizard-integration.md).
