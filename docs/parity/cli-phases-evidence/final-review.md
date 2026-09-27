# CLI phases whole-package final review

Reviewed BASE `872b1dec4b5a120661aa604374c38cf043a11943` through HEAD `e157021af3b32a510d5657ddfcaf6e7d87ebeb73`. Implementation HEAD is `3d106ef0c3cc9687864bcdf0a3c988bbd2c6d7c2`; the final commit records documentation/evidence. Confirmed the supplied complete `integration-review.diff` is byte-identical to `git diff BASE..HEAD`. Read the plan, preflight, progress rulings, all task reviews/reports, every changed production/test implementation, integration/status documentation and relevant unchanged ordering, choices, engine, completion and recovery dependencies.

Review was read-only except this report. No subagents, new tests/builds, live installation, source edits, index/HEAD changes or publication actions. User-owned dirty `AGENTS.md` remains outside the package.

## Strengths

- The shared API is a small pure post-selection boundary. Both adapters use it; preparation clones the guide, validates the phase/context matrix, rejects invalid Remaining prefixes/suffixes and delegates existing choices behavior. It introduces no persistent completed-definition store, automatic coordinator, receipt reader/editor or second installation loop.
- Receipt authority remains in the engine. The remaining-conflict addition invokes the unchanged verifier before analysis; exact roots, serialized completed definitions and full game-tree hashes remain mandatory. Ordinary zero-prefix conflict behavior is unchanged. Pending recovery still prevents suffix operations, and install retains its existing lock, preflight, snapshot, ordered execution and completion sequence.
- CLI preparation reconstructs candidates exclusively from current selected input/choices. GUI retains exact effective Base definitions only after the pre-existing guide/root success guard. Terminal install cleanup now clears stale pending definitions/context. Changed choices do not overwrite retained successful history, while changed reconstructed Base definitions still reject honestly.
- All/None preserves the original pipeline timing, including failed stages, counts and inputPath; the previous Task 2 P2 is resolved at the shared boundary. New phased flags route to the appropriate existing engine operations. Preparation ErrorReport versus pipeline Report, JSONL and human error behavior are coherently tested. Selection policy remains adapter-specific.
- Tests exercise actual temporary bytes, consumed Move inputs, suffix-only execution, full manifests including snapshots/receipts, tampering, choices divergence, guarded promotion and explicit failed-suffix restore/retry. The accepted explicit Base rerun ruling is implemented and documented: ordinary Copy/Overwrite may repeat, consumed Move rejects, and no automatic replay exists.
- Documentation keeps explicit `--direct-markdown` transformation separate from ordinary Markdown ingestion, labels historical plans and accepted superseding rulings, distinguishes normalized logs from raw originals, and avoids claiming new native GUI, full-build, cross-platform or exact C# parity proof.

## Issues

### Critical / Important

None. No P0/P1/P2 production, contract, architecture, security or integration regression identified in this package.

### Minor

**P3 — integration prose reverses the preparation order.** `docs/parity/cli-phases-integration.md:62` says Base deselection happens “after choices application.” Actual `src/phase.rs:61-68` filters and checks Base before choices application at `:93-101`. Task 1 review correctly acknowledges this allowed deviation: current `ChoicesFile::update_guide` changes choice/prompt metadata recursively without consulting or changing top-level selection, dependency ordering or WidescreenOnly, so it is behaviorally equivalent. There is no current correctness defect. For precise implementation documentation, replace the sentence with “Base deselects top-level widescreen components, requires nonempty work, and applies external choices.” This is optional documentation cleanup, not a merge blocker or a reason to rerun suites.

## Deferred P3 disposition

The original Task 1 duplicate-ID case at `tests/phase_preparation.rs:104-110` is unchanged and still also satisfies the exhausted-suffix condition before reaching the duplicate guard. Do not describe that original test as fixed. Task 3 independently closes the package-level guard-coverage gap: `src/ui.rs:3076-3095` constructs three selected components, supplies two case-insensitive duplicate IDs, and asserts the duplicate-specific diagnostic across Validate/Conflicts/Install while checking no worker/promotion/mutation. The shared guard is therefore specifically exercised in final default-feature coverage. Accept the original test-isolation cleanup as deferred nonblocking P3; there is no outstanding product safety issue and no required extra test wave.

## Evidence assessment

Independently recalculated all 8 source hashes and all 25 archived artifact hashes from `docs/parity/cli-phases-evidence/manifest.json`; every hash matches. The current HEAD matches the requested HEAD. Summed final archived result lines: all targets 229 passed / 0 failed / 1 ignored, no default features 199 passed / 0 failed / 1 ignored. Final Clippy log records successful completion; strict command and fmt/diff-check success are recorded in the corresponding report. No repeated suites were needed or run.

The detailed CLI trace predates the final warning-compatibility condition and All-report correction, as explicitly disclosed; later full suites include the corrected source/tests. Archived logs normalize trailing whitespace/EOF and their hashes bind those normalized copies. Temporary roots no longer exist; manifests, event streams, exact command outputs and assertion-backed receipt hashes are retained log evidence, not retained original filesystem snapshots. Receipt hashes containing temporary absolute roots are run-specific. These limits are accurately acknowledged and do not undermine the bounded automated acceptance claim.

## Recommendations

When closing Task 4, record this final PASS and the explicit deferred-P3 disposition in durable status, replacing pending-final-review wording. The optional ordering sentence can be corrected alongside that status-only update without repeating passing product checks. Preserve the original historical task review/report wording and source bindings. Continue treating exact C# GUI acceptance, fresh native interaction after these changes, real K1/K2 full builds, restart coordination, accessibility and cross-platform acceptance as open work.

## Assessment

**Specification: PASS. Quality: PASS with nonblocking P3 notes. Ready to integrate: Yes.**

The whole package fulfills the amended phase plan and accepted rulings through one shared preparation boundary and the existing engine/recovery contract. No combined production fix wave is required. Approval is limited to this shared GUI/CLI phase slice; the user's original exact C# GUI/full-build objective remains incomplete.
