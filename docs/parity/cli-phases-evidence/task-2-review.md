# Task 2 independent review

Reviewed BASE `4010aece72e901203c71ddc5ed6b61a0005f2ca5` through HEAD `9714ab1a59d9633280c79418287a52692bd17073`, the full review diff, brief/report, progress-ledger repeat ruling, and relevant shared preparation/report implementation. Scope is `src/main.rs` and `tests/cli_phases.rs`, with dependencies read to evaluate behavior. No source edits, broad suite reruns, or subagents were used.

## Specification verdict: CHANGES REQUIRED

### P2 — preserve the existing All validation-report pipeline for invalid ordering/selection

**Location:** `src/main.rs:1045–1046`; dependency causing the early failure: `src/phase.rs:67`.

The unconditional shared preparation call executes `Guide::ordered()` before creating the normal report pipeline, including for default All. Previously, a parseable guide with a missing selected dependency, restriction/order problem, or empty selection reached `validate_report_with_options`, which returned a structured `Report` containing stage failures and received `inputPath` in main. Now the same input exits preparation and emits `ErrorReport`, with no stages/inputPath and zero componentCount. This breaks the explicit requirement that default/explicit All behavior and existing report contracts remain unchanged. Report consumers lose the stage results they previously received.

Concrete new executable probe, using an isolated temporary game/mod directory and this guide:

```json
{"thisMod":[{"Guid":"base","Name":"Base","IsSelected":true,"Dependencies":["missing"],"Instructions":[]}]}
```

Command shape (the actual probe used `/tmp/modsync-rs-target/debug/modsync` and absolute temporary paths):

```text
modsync validate --input guide.json --source-dir mods --game-dir game --validation-report --full
```

Observed exit 1, empty stderr, and stdout:

```json
{
  "success": false,
  "exitCode": 1,
  "errorCount": 0,
  "warningCount": 0,
  "passedCount": 0,
  "componentCount": 0,
  "error": "Base requires selected component missing"
}
```

The prior behavior is established by BASE dispatch plus the unchanged `Engine::validate_report_with_options`: it performs environment/conflict/order/component/FOMOD stages inside a `Report` and main adds inputPath. I did not rebuild an old executable. The current behavior above was executed directly; no existing passing suite was repeated.

**Smallest correction:** keep the single shared preparation call and external-choice application, but defer All/None empty-selection and ordering validation to the existing Engine operations. Keep strict eager Base/Remaining validation. This is a concrete Task 1 API integration defect to return to its owner, not a reason to duplicate preparation or fabricate pipeline reports in main. Add regression coverage for both omitted `--phase` and explicit `--phase all`, asserting original Report shape/stages/inputPath and empty stderr for invalid ordering and empty selection. Existing Task 1 expectations that eagerly reject empty All need adjustment. Parent explicitly ruled that preserved All pipeline output takes priority over uniform eager checks.

## Quality verdict: CHANGES REQUIRED for the same P2

No additional blocking quality findings. The changed dispatcher otherwise remains a narrow adapter over the existing engine, and the new executable tests provide substantial real-byte/recovery evidence. The missing regression category is failure compatibility for All; the happy All report assertion cannot catch the schema change above.

Reviewed behavior that satisfies the task:

- Shared InstallArgs adds the phase enum/default and retains existing profile/reset/filter/name/GUID/option selection before preparation. CLI choices are parsed once and passed to the shared API; no receipt reader, phase projector, extra installer, or coordinator was added.
- All/Base dispatch to ordinary operations; Remaining uses the corresponding engine methods with only prepared internal IDs.
- Remaining reports use fixed full-plus-dry-run validation, permit redundant full/dry-run, and reject dry-run-only. Phased install/conflicts reject validation-report flags; clap already requires validation-report for its shaping flags. All flag compatibility and All selection-warning emission are preserved apart from the P2 above.
- Phased preparation failures use ErrorReport; receipt/pipeline failures use Report with inputPath; normal/JSONL rejection retains stderr/nonzero behavior. The explicit no-success-event assertions cover preflight failures, while real install failure appropriately retains progress events already emitted.
- Temporary executable tests cover consumed Base Move, exact Base/Wide bytes, suffix-only dispatch, completion definitions/IDs, receipt corruption/root/definition/game-byte rejection, unchanged manifests, stable and changed choices, and pending failure followed by explicit restore and suffix retry.
- Explicit Base repeat follows the accepted ledger ruling: ordinary Copy/Overwrite may succeed again, consumed Move fails, and repeated Remaining fails receipt verification. This is not universal repeat rejection and is not automatic replay.

Markdown boundary confirmed with parent during review: exclusion applies to explicit `--direct-markdown` transformation mode. Ordinary Markdown ingestion through existing `Guide::load_for_install` is not banned by file extension; unchanged receipt definition verification remains the authority for separate invocations. The implemented flag guard matches that scope. Documentation must use that precise boundary.

## Evidence and remaining scope

Read the stored clippy completion and summed the recorded test results: all targets 225 passed / 0 failed / 1 ignored; no default features 198 passed / 0 failed / 1 ignored. Format and diff-check success are recorded in the task report and were not rerun. These existing passing checks do not cover the new P2 failure case.

Task 2 should remain unaccepted until the All report compatibility issue is fixed and focused regressions pass. Task 4 retains the compatibility/integration documentation obligation. No desktop visual, full-build, cross-platform patcher, or original full GUI-goal completion claim follows from this review.
