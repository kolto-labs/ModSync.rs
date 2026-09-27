# Whole validation package review

2026-09-27. Reviewer: correctness; fresh whole-package specification and quality review. Base `915ec69a21d90fac0395932d54e1622f8c049a18`; integrated head `63c8bc63efd517b48d207ccd6cb82867aad8c1fe`, with production source `47c3e675b93abcbd8660ad84a0b1fdd74b4aad95`. Verdict: **changes requested**, one P2 combined-path finding. No P0/P1 issue identified.

## Finding F1 — preserve actionable archive errors in cards and first-issue navigation

**P2, confidence 100**, `src/validation.rs:229-233`, consumed by `src/validation/presentation.rs:125-144` and `src/ui/validation_page.rs:109-115`.

The new per-component archive outcomes still emit their error strings without a severity prefix. The shared presentation recognizes individual diagnostic cards only from `ERROR:`/`WARNING:`. For the real packaged missing-Extract case (`docs/parity/validation-missing-source-fix/extract-missing.stdout`), the component error is `Fixture: archive/destination validation: Source matches no files: <<modDirectory>>/package.zip`; all other hard stages pass and dry-run is now a `DryRunSkipped` warning. The projected result therefore has only the non-actionable error card `❌ Archive Validation` / `1 component error(s)`. The actual missing archive path is absent from result cards.

The detail **is retained in the log and copied Log section**. However, that log line has no error marker. Completion appends `❌ Validation failed`; error-first lookup consequently selects that final status line, not the real archive diagnostic. Automatic/manual Go to first issue thus targets the final generic failure while highlighting an aggregate card with no affected archive. This execution trace follows mechanically from the archived actual CLI output and current mapper/state code; no new runtime probe was run by this reviewer.

Pinned C# `InstallationValidationPipeline.cs:398-403` prefixes every archive error with `ERROR: {component.Name}:`, so its mapper retains both the actionable detail and a meaningful first-error target. Earlier Task 1 accepted an aggregate fallback for unprefixed legacy engine strings and forbade guessed identities. The new decisive evidence is Task 1b's per-component outcomes with known ownership, plus its missing-source downgrade leaving no other diagnostic error log marker. This is an integration correction, not a request to guess arbitrary engine identities or reopen all deferred backend parity.

Correct once in shared report construction or shared presentation, preserving raw Report JSON shape and GUI/CLI policy ownership. Validate a real missing-Extract report and native/card/clipboard/first-issue result, plus a corrupt archive case and existing missing-Copy warning. No Rust tests or test harnesses may be created/modified under the user's freeze.

## Reviewed and accepted scope

- Read the complete production diff across engine, validation, presentation, UI lifecycle/renderer and bundled font configuration. Traced actual All/Base/Remaining preparation, retained completed definitions, completed-prefix receipt verification, strict install preflight, typed source misses, reached Choose/FOMOD scope, selection conditions and raw report serialization.
- Traced run-ID and input-stamp checks, cancellation-before-terminal rejection, setup failures, stopped-report clearing, rerun/reset paths, worker panic conversion and utility reachability. No second installer or GUI-specific diagnostic policy was introduced.
- Read pinned C# pipeline archive and missing-source behavior, task reviews/rulings and current integration/compatibility/acceptance documentation. Inspected final native warning capture and real copied reports. The warning footer, proactive Next gating, monochrome symbols and platform/font/layout gaps are disclosed, not exact-parity claims.
- Independently hashed all artifacts named in `validation-native/47c3e67/receipt.json`: zero mismatches. Current `ui.rs`, renderer and both bundled font files match receipt hashes. Earlier native captures retain their separate commit bindings. This checks artifact consistency, not a new native execution.
- Flagged ignored `.log` evidence omission to the integrator. Documentation commit `63c8bc6` explicitly includes the owned app log and all six referenced production build/lint/format logs; verified with committed tree inventory. Integrator reports all 66 native archive entries tracked and byte-checked. This packaging concern is resolved, not an additional production finding.

No compiler, Rust tests, new runtime probes, native operations, product edits, commits, pushes or subagents were run by this reviewer. Only this scratch review is written. Existing obsolete Rust test fixtures remain intentionally frozen; production-only builds are not claimed to establish all-target/test compilation.

## Residual risks and proof gaps

The package is a bounded validation slice. Full C# feature parity and exact GUI equivalence remain open. Existing documented limitations include registry auto-extraction, Move fallback/remapping, broader nested-option semantics, final native retained-remaining/effective-FOMOD coverage, greater-than-five mixed diagnostics, comprehensive final splitter/independent-scroll/reset matrix, C# clipboard readback, DPI, keyboard/screen-reader parity, other platforms and full/live installs. These are not new blockers manufactured from absent tests.

Memory was used only to orient checkout and proof boundaries; live source and artifacts were checked. Relevant registry references: `MEMORY.md:707-716`, `MEMORY.md:734-740`, rollout `01a0a7d5-e1a7-71e1-9c2e-77a0cacd63d5`.
