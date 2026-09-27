# Task 1 independent review

Reviewed BASE `872b1dec4b5a120661aa604374c38cf043a11943` through HEAD `4010aece72e901203c71ddc5ed6b61a0005f2ca5`, the complete `task-1-review.diff`, task brief/report, shared preparation dependencies, recovery verifier, and existing remaining-conflict regression. Scope is the four changed source/test files. No product edits, test reruns, subprocess agents, installation runs, or publication actions were performed for this review.

## Specification verdict: PASS

No blocking specification finding.

- `src/engine.rs:547` invokes the existing completion verifier before suffix conflict simulation. The verifier remains unchanged: zero prefix is a no-op, and nonzero prefixes compare roots, exact serialized completed definitions, and the current game-tree manifest. The subsequent conflict path still checks pending recovery state before simulation. There is no receipt input to preparation and no snapshot/pending mutation introduced on rejected evidence.
- `src/phase.rs` implements the valid All/None, Base/None, Remaining/Retained, and Remaining/Reconstruct matrix and rejects all other pairs. `Guide::ordered()` supplies existing empty-selection/dependency/order validation. Remaining adds nonempty/unique/ordered-prefix/nonempty-suffix validation without selecting or inserting missing components.
- Base deselects top-level boolean `WidescreenOnly=true` components. Choices use the existing recursive application API. Filtering before applying choices differs from the provisional plan's textual ordering, but is equivalent with the current choices implementation: it updates component metadata recursively without consulting or altering selection, ordering, or the widescreen flag.
- Retained context preserves exact successful resident definitions and nested options through `apply_to_remaining`; reconstruction uses current selected definitions and current choices exclusively. Whole-payload suffix-only choice changes correctly remain reconstruction mismatches.
- The implementation adds no installer, coordinator, CLI completed-ID option, receipt reader/editor, recovery format, replay, backfill, or UI lifecycle changes. Public library completed IDs are adapter state, not a new user-facing command option.
- Phased Direct Markdown rejection remains a caller responsibility because this pure API receives no source-format input. Task 2 must enforce it. Existing desktop empty-widescreen navigation remains an intentional no-dispatch path, distinct from requesting an empty prepared operation.

Actual API versus provisional plan: `phase::Phase`, `phase::CompletionContext`, `phase::PreparedPhase { guide, completed }`, and `phase::prepare(selected, phase, context, choices)`. The plan used `InstallPhase`, `CompletedContext`, `completed_ids`, and `prepare_install_phase(selected, phase, choices, completed)`. These naming/argument-order changes are allowed by the brief and preserve its semantics; callers must use the actual API.

## Quality verdict: PASS with one nonblocking test-strength note

No P0/P1/P2 implementation finding. The change is small, pure at the preparation boundary, and delegates receipt checks, ordering, and choice application to existing code. Early validation and cloning prevent caller-guide mutation on failure. No additional persistent state or unnecessary abstraction was introduced.

- **P3 — duplicate-ID case does not reach the duplicate-specific guard.** `tests/phase_preparation.rs:91` supplies two IDs for a guide containing only two selected components. `src/phase.rs:74` rejects the exhausted suffix before the duplicate check at lines 78–82. This case proves rejection, but cannot detect removal of duplicate validation independently. A future focused improvement can use the three-component fixture with a duplicate two-ID prefix and check the duplicate error. This is nonblocking: production explicitly validates uniqueness and ordered prefix membership, and no unsafe acceptance was identified.

The new byte-backed fixture installs a Move, verifies the output and consumed input, compares retained/reconstructed guides, and verifies changed base/suffix choices against the real completion contract. Conflict rejection cases cover missing/corrupt/root-mismatched receipts, changed definitions, and tampered game bytes. Pre/post manifests include both installation trees and recovery state, and rejected calls assert no pending marker. Existing suffix-conflict coverage remains present.

## Evidence and follow-through

Read the recorded successful clippy completion and test result lines rather than repeating passing suites. They support the reported 219 passed / 1 ignored for all targets and 192 passed / 1 ignored without default features. Format and diff-check success are recorded in the task report; this review did not independently rerun them.

Task 1 is acceptable for caller integration. The required `COMPATIBILITY.md` proof-boundary update remains assigned to Task 4 and must land before the overall feature is declared complete. CLI parsing/dispatch, adapter equality after identical selection, GUI lifecycle integration, and executable failure/restore/retry coverage remain later-task responsibilities. This review provides no exact visual GUI, full-build, live-installation, or original overarching GUI-goal completion claim.
