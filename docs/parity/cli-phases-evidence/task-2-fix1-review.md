# Task 2 fix round 1 independent review

Scope: BASE `9714ab1a59d9633280c79418287a52692bd17073` through HEAD `312ee167a4732f4095f8c966595fd3b2185396d9`. Read the complete fix diff, appended task report, current shared preparation implementation, and recorded red/green/check logs. This is a scoped re-review of the prior P2 and potential new breakage from its fix. No product edits, subagents, probes, or passing-suite reruns.

## Specification verdict: PASS — prior P2 addressed

The early All ordering error is removed at its source. `phase::prepare` keeps the valid context matrix and shared choices application, but All/None no longer calls `Guide::ordered()` before returning the prepared guide. Existing engine operations therefore again own ordering/empty-selection validation and the CLI report pipeline retains its original failure staging. No CLI bypass or synthesized report was added.

The correction retains eager validation for Base immediately after widescreen projection and for Remaining before prefix/suffix acceptance. Reconstruction still obtains its candidate IDs through ordering, retained IDs still receive strict prefix checks, and choices still skip completed retained definitions. Receipt verification, output schemas, selection behavior, and recovery semantics are untouched.

The earlier uniform empty-All rejection expectation was deliberately replaced in accordance with the parent ruling: existing All behavior takes priority. Invalid All preparation now preserves the invalid guide for engine reporting rather than treating it as an accepted installation.

## Quality verdict: PASS

No new actionable findings in this fix.

The production edit is minimal and directly addresses the cause. The new executable regression tests both omitted and explicit All for missing dependencies and empty selection, asserting exit 1, empty stderr, Report/inputPath/componentCount, failed Conflicts/InstallOrder/ComponentValidation stages, no ErrorReport scalar error, and unchanged temporary-tree manifests. The empty-selection fixture uses the established category filter so the loader's all-selected fallback does not invalidate the test setup.

The updated shared test additionally checks exact external-choice application on invalid All, confirms ordering remains invalid for later engine validation, and retains invalid-Base rejection. This guards against accidentally bypassing choice preparation while restoring report compatibility.

## Evidence and disposition

Read recorded results: the new regression failed before the fix (0 passed / 1 failed), then focused coverage passed (11 passed). Recorded complete suites total 226 passed / 0 failed / 1 ignored and 199 passed / 0 failed / 1 ignored without default features; clippy recorded successful completion. Format/diff-check success is recorded in the report and was not rerun.

Task 2's sole blocking P2 is resolved and Task 2 is acceptable under the previously documented repeat-Base and explicit Direct Markdown rulings. The unrelated Task 1 P3 duplicate-case test-isolation note remains deferred. Task 4 documentation and later GUI integration/proof responsibilities remain open; this re-review makes no full GUI or full-build completion claim.
