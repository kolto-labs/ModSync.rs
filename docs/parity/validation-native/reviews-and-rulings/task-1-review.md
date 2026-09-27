# Task 1 independent spec and quality review

> Historical review annotation, 2026-09-27 integration: the original review and verdict below are preserved. Formatter P2 was subsequently corrected and approved at9d77f4b. Its asserted full missing-source2/0/4 was extrapolated, not measured: the actual partial `--dry-run` was2/0/1 and independent source-bound full report at9d77f4b was3/0/3. See `docs/parity/validation-missing-source-evidence/current/receipt.json` and the missing-source audit. Task 1b later corrected shared Copy availability to0/1/5 and is approved through88b2dd9; this annotation does not rewrite the historical reviewed code or attribute later evidence to it. Earlier test-writing requests below predate the user freeze and are not current authorization.

Reviewed 2026-09-27. Rust BASE `915ec69a21d90fac0395932d54e1622f8c049a18` to HEAD `4c1a68dd5ef5805591171bd9d590c653913994cc`; pinned C# HEAD independently confirmed `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Read the brief, implementation ledger, saved review diff, actual five-file diff, binding audit, presenter/mapper/page C# source, and bounded native-reference README. No builds, product edits, C# edits, or subagents. Only this review file is owned.

## Specification verdict: REQUEST_CHANGES

### [P2] Trim the final human report exactly as the reference does

`src/validation/presentation.rs:322` returns the accumulated string without trimming. Pinned C# `src/ModSync.GUI/Dialogs/WizardPages/ValidatePage.axaml.cs:734` returns `report.ToString().TrimEnd()`. This affects every ordinary report because the formatter adds a newline to the counts/results and full page logs normally end with a newline; whitespace-only trailing log content is also retained. Therefore copied text differs even after the permitted timestamp/duration normalization. `tests/validation_presentation.rs:235` and `:267` currently encode the incorrect trailing newline in their golden expectations.

Required correction: trim trailing whitespace at the final formatter boundary and update literal goldens to the reference output. Include a full log ending in mixed whitespace, a cards-without-log report, and a no-cards/no-log report, preserving internal blank lines and whitespace. This is a shared formatter fix, not a GUI-only clipboard workaround. Re-review the delta and focused passing evidence before approval.

The rest of the representable stage mapping matches the reviewed pinned source: ordered headings; case-sensitive prefix recognition and whole stripped detail; Environment duplicate suppression; Conflicts/InstallOrder fallback logs and aggregate severity; archive OK transformation and successful aggregate omission; passed FOMOD checkmark even with warnings; dry-run missing/clean/informational cases, errors before warnings, five-item caps and counts; known solution strings; error-first card/log lookup with warning-count fallback. Rust non-null empty strings remain empty as in C#, and absent component names become Unknown. No invented engine identities were found.

Observed APIs preserve old method signatures by no-op wrappers. Callback calls bracket actual stage work, totals derive from enabled stages, early Environment failure emits only the actual stage pair, and invalid completed-prefix setup returns before any callback. The remaining preset preserves full + dry-run + component validation and completed-prefix context. Ordinary Event transport and Report serialization fields are unchanged in the diff.

## Quality verdict: PASS_WITH_CONCERNS, independent of the spec blocker

No additional correctness, safety, ownership, or maintainability blocker was found in the five changed product/test files. Projection has no filesystem/process/UI dependency. Engine edits add observers around the existing pipeline without moving validation policy into the UI. Stage counter increments happen only on starts. Observer panics propagate before subsequent work; the code neither catches them as success nor emits fabricated completion. Shared helpers centralize solutions and first-issue selection. Unknown raw engine categories/details are retained. Tests use temporary roots; the trusted-patcher marker test and consumed-Move suffix fixture exercise meaningful safety boundaries.

The golden-output mistake above is also a test-quality defect: literal expectations are independent strings, but must be grounded in the entire C# formatter including its final TrimEnd. Broader nonblocking coverage gaps remain: explicit empty optional solution/name/category strings, warning-only first-card selection, Conflicts/InstallOrder warning aggregate literals, and complete single-issue dry-run card/log text. Source review covers these branches; six presentation tests do not constitute exhaustive differential execution. Old-versus-observed comparisons exercise the same new implementation through wrappers, so they prove wrapper equivalence, not independent BASE runtime equivalence; the narrow engine diff and existing CLI regression evidence provide the additional compatibility support.

Repository AGENTS asks for COMPATIBILITY.md updates when behavior/proof boundaries change. This Task 1 commit changes only the assigned five paths; the integrator must record the new shared APIs and unresolved parity boundaries in COMPATIBILITY.md at the integrated documentation gate rather than claim that requirement is already satisfied.

## Evidence and acceptance limits

Inspected existing log summaries: `/tmp/task1-no-default.log` records 210 passed / 1 ignored across its test/doc targets; `/tmp/task1-final-progress.log` and `/tmp/task1-final-no-default-progress.log` each record the corrected 5/5 progress suite; `/tmp/task1-clippy.log` records completed Clippy. The implementation ledger additionally reports presentation 6, validation 15, CLI 7 and formatting success. No test was repeated by this reviewer. Parent subsequently confirmed required all-targets execution against reviewed HEAD completed successfully. Independently counted `/tmp/task1-all-targets.log`: 240 passed / 0 failed / 1 ignored across 24 test targets. This satisfies the current-HEAD all-targets evidence requirement; the spec fix still requires focused validation and delta review before acceptance.

The native C# receipt/README independently records clean 0/0/6, missing-source warning 0/1/5, and missing-patcher Environment 1/0/0, with checkpoint tree manifests. It explicitly lacks successful clipboard readback, complete native stage/report comparison, and broad layout/input/accessibility coverage. It proves neither Rust runtime behavior nor Rust native parity.

Retained deviations are not silently accepted as exact parity:

- Rust missing-source classification remains 2 errors / 0 warnings / 4 passed for the full report, versus C# 0 / 1 / 5. This is a material shared-backend parity gap; keeping actual outcomes visible is correct for this presentation slice, but the broader parity goal remains open.
- Warning details deliberately omit C# automatic-handling promise. This follows the audit safety constraint; exact warning copy remains blocked until shared engine semantics justify the promise.
- Raw absent solution fields remain absent in CLI JSON while the reusable headless presentation helper derives known C# solutions. Parent explicitly resolved the audit wording conflict in favor of transport compatibility plus shared projection. This is an acknowledged raw/presentation contract difference, not literal serialized-solution equality.

Task 2 must not treat this review as Task 1 acceptance until the formatter fix and required test gate are confirmed. Full native page parity remains a later independent obligation.

Memory provenance: MEMORY.md:707-716 used only to orient active Rust/C# checkout context, all relevant revisions verified directly; rollout `01a0a7d5-e1a7-71e1-9c2e-77a0cacd63d5`. No memory was edited.
