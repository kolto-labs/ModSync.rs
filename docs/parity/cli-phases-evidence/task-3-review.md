# Task 3 independent review

Reviewed BASE `312ee167a4732f4095f8c966595fd3b2185396d9` through HEAD `3d106ef0c3cc9687864bcdf0a3c988bbd2c6d7c2`, complete `task-3-review.diff`, task brief/report, worker dispatch/completion context, navigation gates, existing lifecycle regressions, and recorded final-check logs. Only `src/ui.rs` is changed by the task. No product edits, subagents, installation probes, or repeated passing suites were used in this review.

## Specification verdict: PASS

No actionable specification findings.

- `src/ui.rs:1864–1879` is a thin routing adapter to shared `phase::prepare`: pre-base Validate uses All/None, pre-base Conflicts/Install use Base/None, and completed state uses Remaining/Retained. Choices-file reading stays at the frontend boundary; phase projection, prefix validation, and external-choice application stay in the shared API. Selection policy is unchanged, so differential equality properly starts at identical post-selection guides.
- `src/ui.rs:2067–2095` uses the prepared guide/IDs for existing worker dispatch and captures effective ordered Base definitions only after successful preparation. Preparation rejection goes through the existing failure handler without spawning an engine worker or promoting completion. All retains the corrected delayed report validation.
- The existing successful-install guard still compares the loaded-guide serialization and both roots before overlaying exact effective Base definitions. IDs still derive from those successful definitions. The change at `src/ui.rs:148–152` clears pending definitions and dispatch context on every terminal install result, including worker failure and stale-success rejection; it does not promote failed definitions.
- Existing worker messages, full-plus-dry desktop report processing, receipt comparison, recovery checks, and install execution remain unchanged. Retained choices continue preserving historical Base definitions rather than reconstructing them from current inputs or receipt content.
- Empty-widescreen navigation remains the existing no-dispatch skip. Phase preview now uses the shared preparation path; only successfully prepared operations reach worker execution. No installer, automatic coordinator, receipt reader/editor, replay path, or recovery format was added.

## Quality verdict: PASS

No concrete severity/file-line issues to report.

The implementation removes the private projection/application path rather than maintaining parallel business behavior. The small terminal cleanup closes stale pending-definition lifetime without weakening guarded success. Existing settings and choices setup diagnostics remain intact, and preparation errors cannot generate a success promotion or automatic navigation advance.

New tests exercise both adapter mapping and actual shared-engine behavior using temporary roots. They compare All/Base prepared guides and choices after identical selection; install a Base Move that consumes its input; verify retained effective definitions against the actual receipt; compare stable retained/reconstructed reports; prove changed completed and suffix-only choices preserve the GUI prefix while CLI-style reconstruction fails; assert unchanged manifests for rejected reconstruction and nonmutating GUI operations; then install only the suffix with exact final bytes and receipt definitions.

The invalid retained-ID matrix covers empty, duplicate, unknown, and wrong-prefix inputs across Validate/Conflicts/Install, with no worker, no promotion, and unchanged manifests. The three-component duplicate fixture reaches the duplicate-specific diagnostic, strengthening the behavioral coverage noted as a nonblocking Task 1 test concern. Failure cleanup and stale guide/reset/root guards are explicitly checked. The failed-Base test correctly limits unchanged-byte claims to game/mod trees because ordinary lock/state directories may be created.

This evidence does not equate GUI retained behavior with separate-process CLI reconstruction: the whole-payload choices difference is intentional and is tested as such. It also does not claim identical frontend selection semantics or native desktop visual interaction proof.

## Evidence and disposition

Read and summed the final stored results: focused desktop tests 30 passed / 0 failed; all targets 229 passed / 0 failed / 1 ignored; without default features 199 passed / 0 failed / 1 ignored. Clippy recorded successful completion. Format and diff-check success are recorded in the task report and were not rerun. The report distinguishes corrected development failures from these final checks.

Task 3 is acceptable for integration. Task 4 retains compatibility/GUI-CLI/acceptance documentation and durable status obligations. The original exact GUI parity/full-build objective remains open; this review proves neither visual parity nor a real K1/K2 installation.
