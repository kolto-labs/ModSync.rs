# Task 1b implementation handoff

Date: 2026-09-27. BASE: `9d77f4ba9955ea505d3eb7969afdf5b5aee73b72`.
Production commit: `1cb3f4b` (`fix: align shared validation source availability stages`), only engine.rs and validation.rs. Complete BASE..HEAD source diff: `task-1b-review.diff`. Evidence remains in the assigned durable directory for integrator publication; no unrelated files were staged.
Status: implemented and directly exercised; pending independent specification and quality approval. Task 2 must wait for that approval. No push/publication.

Production ownership is limited to `src/engine.rs` and `src/validation.rs`. No additional module, installer, CLI flag, adapter, recovery format, UI edit, or test edit was introduced. User-owned AGENTS.md and the other workers' audit/plan/native evidence remain untouched. No Rust test files, inline test modules, disposable Rust harnesses, or cargo test invocations were created or run. The existing test `tests/validation_progress.rs::missing_copy_source_remains_an_engine_error_without_gui_reclassification` intentionally remains unchanged and now has a stale report expectation; retaining that assertion is not a reason to preserve the known product defect.

## Production changes

- The shared full-report Conflicts stage directly inspects declared dependencies/restrictions in selected plus completed-prefix context. Missing dependencies produce component warnings and selected restrictions produce component errors, using C# counter precedence. It no longer calls simulated-write analysis. Standalone `analyze_conflicts`, remaining verification, conflict JSON, and strict no-partial-success behavior are unchanged.
- ComponentValidation performs a read-only archive/destination pass instead of calling the strict whole installer simulation. Applicable Extract instructions, including immediate option instructions, use the existing Roots, matching, archive unpack/safety/budget primitives and actual extraction destination calculation. Real member paths are matched using the same glob rules. Game-root sources bypass membership checks. Copy/Move-only absence does not fail this stage. Copy/Move and directory-targeting action destinations must exist; no AttemptFixes directory creation occurs. Every candidate archive is actually inspected. Temporary decoded bytes remain outside game/mod roots and are discarded.
- The shared matcher returns a private typed `MissingSource`. Only Copy/Move/Extract source resolution wraps this as `MissingSourceFiles`; malformed patterns, unsafe paths, permission failures, corrupt archives, Rename misses, and FOMOD member failures remain hard errors. Source syntax is checked across the source list before ordinary absence classification; readable content is required even for a virtual copy.
- Dry-run executes existing instructions against its evolving VFS. Typed source unavailability replaces that component's provisional warnings with exactly one component-only `Warning / DryRunSkipped`, stops later instructions in that component, retains earlier successful virtual effects, and examines subsequent selected components. Other execution failures remain errors and also stop that component. No initial-tree pre-scan, source fabrication, or string-based severity filtering is used.
- Missing generic Extract sources no longer manufacture a FOMOD inspection failure. The archive stage and dry-run own generic availability. Registered prior prompt state, unreadable/corrupt archives and unconfigured/malformed FOMOD remain separate failures.
- Actual `install` still performs strict simulation before snapshot/pending/live writes. Ordinary `validate`, remaining installation and standalone conflicts continue rejecting missing source. The completion verifier, preparation/prefix definitions and recovery lifecycle have no implementation changes.
- Existing observed stage boundaries, one-based stage steps, stage order, enabled total, environment early exit and remaining wrapper remain in the same locations. The archive stage emits a real archive-check completion Event instead of claiming execution of Copy/etc. Event fields and all public report/issue JSON shapes remain unchanged. Native callback timing/lifecycle was not driven in this task; this boundary claim is source review, not a fabricated observation.

## Evidence and commands

Durable artifacts are in `docs/parity/validation-missing-source-fix/`. `build-receipt.json` binds the initial implementation to its source hashes, before/after HEAD, build logs, and private copied desktop/headless binary hashes. `probe-receipts.json`, `phase-probe-receipts.json`, `present-probe-receipts.json`, and `root-probe-receipt.json` record actual command argv, input guides/hashes, stdout/stderr/exit and manifests. `verification-summary.json` summarizes 80 real CLI invocations, including 55 report invocations. Every recorded report left its before/after manifest unchanged. These are file/mode/hash checkpoints, not syscall traces.

The final self-review hoisted matcher prefix-string calculation out of each candidate-file iteration, preserving baseline allocation behavior. The original 80 receipts retain their original source/binary binding. `final-build-receipt.json` and `final-probe-receipts.json` bind the final source and repeat exact native input plus positive/negative/glob/permission matcher cases. Do not attribute the earlier receipts to the final binary without this distinction.

Both initial and final production check sequences use `CARGO_TARGET_DIR=/tmp/modsync-rs-target`:

```text
cargo fmt --all -- --check
cargo build -j2
cargo clippy --lib --bins -j2 -- -D warnings
cargo build --no-default-features -j2
cargo clippy --no-default-features --lib --bins -j2 -- -D warnings
```

Both initial and final five-command sequences exited 0. The final copied binaries additionally ran 14 direct CLI probes (13 reports), including the exact three-state native input, no-default-feature missing case, strict install rejection, positive/negative glob matching, directory-prefix Move, game-root absence, malformed/unsafe paths, permission denied, and present/missing archive members. Every final report manifest is unchanged. Source/binary hashes match the final build receipt; `prefix-hoist.diff` is independently checked against the recorded initial engine hash and shows the exact allocation-only difference. Across both bound implementations there are 94 CLI invocations and 68 reports, not 94 automated tests. Clippy deliberately targets production lib/bins, not test targets, under the latest user ruling. `git diff --check` is clean. An early development build found a misplaced `mut emit`; it was corrected before the source-bound production checks and direct probes. Compiler ownership was explicitly released after final verification; no compiler process remains active for this task.

Representative real report command, with absolute disposable paths recorded in each receipt:

```text
<private-binary> validate --input <fixture>/guide.json --game-dir <fixture>/game --source-dir <fixture>/mods --use-file-selection --validation-report --full --dry-run
```

Strict probes use the real `install`, ordinary `validate`, and `conflicts` commands with the same selected guide; remaining probes use the existing `--phase base` / `--phase remaining` route. No completed-ID flag or receipt editing was added. The sole external process deliberately executed was a copied `/usr/bin/false` in an owned disposable runtime-failure fixture, with explicit `--allow-processes`, to exercise the existing pending/restore lifecycle. Validation did not launch it.

## Observed results

| Direct case | Errors / warnings / passed | Result |
|---|---|---|
| Exact pinned guide and source present | 0 / 0 / 6 | Exit 0; six clean stages |
| Only sample.tga removed | 0 / 1 / 5 | Exit 0; five clean stages and warning-only DryRun |
| Source restored byte-for-byte | 0 / 0 / 6 | Exit 0; no persistent stale warning |
| Missing Copy or Move, literal or wildcard, each mods/game/kotor root | 0 / 1 / 5 | Warning-only report; corresponding actual install rejects |
| Existing Copy or Move, each mods/game/kotor root | 0 / 0 / 6 | Executes virtually without filesystem writes |
| Preceding virtual Copy supplies later Move source | 0 / 0 / 6 | Resolution uses evolving VFS |
| Missing first source before a source-producing tail instruction | 0 / 2 / 5 | Tail does not run; later component also lacks its required source |
| Successful virtual prefix, then absence, then a Delete tail | 0 / 1 / 5 | Delete tail skipped; later component sees retained prefix bytes |
| Delete-miss warning followed by missing Copy | 0 / 1 / 5 | Provisional warning replaced by one component skip |
| Missing Extract archive | 1 / 1 / 4 | Archive error plus dry-run skip; FOMOD stage passes |
| Required archive member absent | 1 / 1 / 4 | Archive member error retained alongside dry-run skip |
| Valid archive with required member | 0 / 0 / 6 | Actual archive/member inspection passes |
| Corrupt archive or unsafe archive entry | 3 / 0 / 3 | Hard archive/FOMOD/dry-run failures |
| Missing source with absent destination | 1 / 1 / 4 | Destination error survives missing-source warning |
| Unsafe source/destination or malformed glob | 2 / 0 / 4 | Component and dry-run errors; no skip warning |
| Missing Rename source | 1 / 0 / 5 | Hard dry-run error, not Copy/Move availability |
| Unconfigured or malformed FOMOD | 2 / 0 / 4 | FOMOD and execution gates remain hard failures |
| Missing registered archive with recorded configuration prompt state | 1 / 1 / 4 | FOMOD hard failure survives loose-source warning |
| chmod(000) loose source | 1 / 0 / 5 | Permission denied stays a hard dry-run error |
| chmod(000) archive | 3 / 0 / 3 | Hard archive/FOMOD/dry-run errors |
| Consumed base Move source, missing remaining suffix | 0 / 1 / 5 | Completed base is not replayed; componentCount is 1 |
| Remaining roots / completed definitions / game bytes mismatch | 1 / 0 / 0 | Environment-only failure; no later stage |
| Pending recovery | 1 / 0 / 0 | Environment-only failure; actual retry also blocked |

Exact missing-copy issue fields are `severity: Warning`, `category: DryRunSkipped`, `message: Skipped 'Fixture' during dry-run: source archives or files are not available.`, componentName `Fixture`, componentGuid `11111111-1111-1111-1111-111111111111`. There is no affectedPath, instructionIndex, or solution. Guide SHA256 is `3c184ea3922d6454cc3b18dc26e3a8e7250fc164ca9075ab83723ef78e0c873b`; native source SHA256 is `eb3a5fb11d4b50febbbb25d373c55fe6bdcc760640a18ce3c01e3f92ab4ef7be`. Both match the pinned C# receipt. The exact-tree probes have only the synthetic zero-byte swkotor.exe, existing Override directory and the optional source file before execution.

Eighteen initial strict missing-source install probes exit 1, emit no successful instruction/snapshot stdout, preserve game/mod trees, and create no snapshot, pending marker, or completion promotion. First-install lock acquisition creates the existing empty state directory and two root lock files; the receipts report that separately instead of calling the whole parent directory unchanged. The remaining install also preserves the existing completion receipt and all state bytes exactly.

The real disposable late failure wrote an earlier Copy, then failed the owned external program, retained pending recovery, rejected retry, and successfully restored both game/mod trees byte-for-byte. No real user installation, download, GUI process, or pre-existing recovery state was touched.

Declared conflict probes confirm the new stage warning/error distinction. Other existing strict `Guide::ordered` and FOMOD ordering gates still make the whole report fail for an unselected dependency/restriction. This task does not claim full C# install-order/dependency-policy parity.

## Remaining limits and integration notes

- Native C# evidence is reused from `docs/parity/validation-reference/receipt.json`, pinned at `d31ccbd3bfcddb4705917eafa6483715c166de3b`; no duplicate C# build/session ran. This task closes the measured shared report data behavior, not native rendering/clipboard/navigation parity. Task 2/3 still own driven native Rust comparison and callback/lifecycle evidence.
- Cancellation checks remain hard and were inspected at component/instruction/archive boundaries. CLI does not expose this cancellation token, and no Rust harness was authorized, so cancellation was not directly driven here. No cancellation success claim is made.
- Registry auto-extraction and Move subfolder remapping remain absent from Rust source resolution: C# `ModComponent.cs:975` and `:1367`. Move's already-present target-basename fallback without byte comparison remains deliberately unimplemented: `ModComponent.cs:1289`. Mixed-source aggregation (one existing match can satisfy a list containing an absent literal) is still stricter in Rust: C# `PathHelper.cs:633` and `Instruction.cs:349`. These are shared-executor follow-ups, not UI rewrites or hidden lookups.
- Archive validation now inspects every applicable candidate and uses actual extraction destinations; it does not reproduce C#'s no-archive top-level-only scan, archive-name heuristics, fixture-specific exceptions, or mutable AttemptFixes behavior (`ComponentValidation.cs:74`, `:191`, `:331`, `:394`). It retains safe explicit Extract destinations supported by the existing Rust executor. Options are included at the immediate level as the C# archive collector does; general nested-option parity is not established. DelDuplicate's source tokens are extensions in the existing Rust executor, so they are not treated as archive paths. These scoped differences must not be advertised as full archive-stage parity.
- C# missing registered archives without prompt state add a soft FOMOD message that does not increase the pipeline warning counter (`FomodConfigurationGate.cs:117`); Rust's existing no-prior-state gate continues silently. Prior-state hard protection is preserved.
- Task 3 integrator should update COMPATIBILITY.md: structured report conflicts now mean declared dependencies/restrictions; component stage checks archives/destinations; Copy/Move/Extract ordinary availability becomes component-scoped dry-run warning while strict validation/install/conflicts stay failing. Cite the actual 0/0/6 -> 0/1/5 -> 0/0/6 direct receipt and the remaining limits above. COMPATIBILITY.md was intentionally not edited outside assigned ownership.
- The previous full Rust baseline is the measured 3 errors / 0 warnings / 3 passed. The old full 2/0/4 extrapolation is not evidence; it remains superseded by the audit. No full C# feature-parity claim follows from this bounded fix.

Self-review checked typed-cause boundaries, VFS retention/termination, recovery verification order, unchanged strict preflight, JSON shape, actual stage boundaries, immediate option scope, archive scratch cleanup and matcher allocation behavior. Independent specification and quality review remains required before marking Task 1b complete.

## Fix round 1: independently gate executable Choose options

Review finding P1 in `task-1b-review.md` is valid: after replacing strict component simulation, an applicable Choose into a selected option containing an unconfigured FOMOD Extract incorrectly passed the independent FOMOD gate. The exact reviewed fixture without dry-run reported 0/0/5; adding dry-run caught the failure only in execution (1/0/5). Those original reviewer observations remain preserved; they are not relabeled as fixed evidence.

Fix commit `88b2dd9` (`fix: traverse executable options in the FOMOD validation gate`) is based on `1cb3f4be432b9c5435af442517990fedb2a97482` and changes only `src/engine.rs`. The complete fix-only diff is `task-1b-fix1-review.diff`. No test/harness files or inline test modules were created or edited; no tests ran. No UI, CLI, recovery, compatibility or other worker-owned production paths changed.

The fix factors `chosen_options` out of the existing executor's Choose branch and uses that same helper in the FOMOD scope walk. `instruction_applies` controls both Choose traversal and Extract inspection; the existing `check_component` enforces reached component dependencies/restrictions and safety constraints. The scope walk excludes completed top-level components before traversing their options and reads FomodChoices/ResourceRegistry from the actual owning option. It does not treat every selected or unselected option as executable, and it does not simulate installation inside the archive stage. Registered prior-state protection applies to the reached scope. Generic missing Extract files still belong to archive/dry-run availability rather than fabricated FOMOD inspection errors.

New artifacts live under `docs/parity/validation-missing-source-fix/fix1/`: `build-receipt.json`, `probe-receipts.json`, `phase-probe-receipts.json`, `verification-summary.json`, build logs and raw stdout/stderr. The earlier 94 invocation receipts retain their older source/binary bindings. Fix1 records another **39 actual CLI invocations, including 37 reports**; all 37 before/after report manifests match. Current production hashes match the fix build receipt, as do both private executable hashes.

Commands all exited 0, serialized with `CARGO_TARGET_DIR=/tmp/modsync-rs-target`:

```text
cargo fmt --all -- --check
cargo build -j2
cargo clippy --lib --bins -j2 -- -D warnings
cargo build --no-default-features -j2
cargo clippy --no-default-features --lib --bins -j2 -- -D warnings
```

Direct CLI results:

| Fixture / selection | Full report without dry-run | Full report with dry-run |
|---|---|---|
| Exact review guide, selected reachable unconfigured option | FOMOD fails, 1/0/4, exit 1 | FOMOD and DryRun fail, 2/0/4, exit 1 |
| Explicit configuration on owning option | 0/0/5, exit 0 | 0/0/6, exit 0 |
| Configuration only on parent | FOMOD fails, 1/0/4 | FOMOD and DryRun fail, 2/0/4 |
| Unselected or unreachable option | 0/0/5 | 0/0/6 |
| Choose excluded by instruction dependency or restriction | 0/0/5 | 0/0/6 |
| Extract excluded by instruction dependency or restriction | 0/0/5 | 0/0/6 |
| Reached option has unsatisfied component dependency or selected restriction | FOMOD fails, 1/0/4 | FOMOD and DryRun fail, 2/0/4 |
| Reached option has missing registered archive with prior configuration state | FOMOD fails, 1/0/4 | FOMOD fails, 1/0/5 |
| Same prior-state option unselected | 0/0/5 | 0/0/6 |
| Reached generic Extract input absent without recorded prior state | Archive fails, FOMOD passes, 1/0/4 | Archive fails, FOMOD passes, 1/1/4 |

The headless executable reproduces the exact review fixture's independent FOMOD failure, 1/0/4. The exact original missing loose-Copy guide still produces 0/1/5 with the existing precise component-scoped skip issue.

Remaining-phase evidence uses a newly owned real base Choose/Move installation to consume a registered, configured archive. Its completed option retains prior configuration state but its source archive is now absent. Remaining validation correctly excludes that completed subtree and independently detects an unconfigured reachable suffix option. Supplying choices on the suffix option produces 0/0/6 without replaying the completed Move or rejecting its absent registered archive. The suffix Choose also depends on the completed base ID. Strict remaining install rejects unconfigured execution before mutation and leaves the completed receipt and trees unchanged. Completed-definition, game-byte and source-root mismatches each fail Environment only (1/0/0). The existing remaining-report wrapper always enables dry-run, so the remaining command without an explicit `--dry-run` still produces six stages; that established API behavior was preserved and is visible in the raw outputs.

Self-review checked that the extracted Choose helper preserves the old ordering, unknown-option rejection, case-insensitive ID matching and selected-option filtering. There is one shared option-selection rule, with the existing condition and component checks reused by both traversal paths. The independent gate performs no virtual or real installation. No broader FOMOD selection-validation policy or resolver feature was added. COMPATIBILITY.md integrator delta: independent FOMOD report checks now include selected options actually reached through applicable Choose instructions and use owning option configuration, while excluding completed subtrees and unreachable choices.

Compiler ownership released after the fix checks. P1 is implemented and directly exercised, but scoped independent re-review is still required before Task 1b approval or Task 2 dispatch.
