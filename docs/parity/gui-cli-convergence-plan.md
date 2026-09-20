# GUI/CLI convergence work package

Spec: user requires both frontends to share code-behind logic and the GUI to match C# exactly. This package advances that objective; it does not define full parity as complete.

## Global Constraints

- Keep installation and validation behavior in shared Rust library modules.
- C# reference is `/home/brunner56/.codex/worktrees/5813/ModSync` at `d31ccbd3bfcddb4705917eafa6483715c166de3b`.
- Preserve user-owned AGENTS.md. Do not alter game installations. Tests use temporary directories.
- Never claim native visual equivalence from state tests or source inspection.
- Do not introduce alternate business rules in either frontend.
- Work in the existing authorized `codex/rust-port` checkout. Parent owns pushes.

## Task 1: Share selection filter semantics

Inspect C# ApplySelectionFilters and existing Rust selection helpers, GUI selection callbacks, and CLI selection preparation. Implement C# category/tier CLI selectors through the existing shared selection module, preserving current explicit name/GUID CLI support and documented GUI additive behavior. Use one common category/tier matching primitive for both frontends where their predicates should agree; do not make CLI selection additive if C# resets selection. Handle malformed or unmatched selectors explicitly following reference behavior. Own `src/selection.rs`, relevant selection preparation in `src/main.rs`, and `tests/selection.rs`; modify `src/ui.rs` only if needed to replace duplicated matching, and coordinate before doing so. Add executable tests for filter matching, multiple filters, selection reset/additive distinctions, case handling, and GUI/CLI use of shared predicates. Record remaining differences honestly. Run focused tests and required checks. Commit only owned files; no push.

## Task 2: Inventory exact GUI gaps

Read-only audit comparing Rust wizard and C# wizard page/control/layout/interaction definitions. Produce `docs/parity/GUI-INVENTORY.md` with exact source pointers, current mismatches and prioritized implementation groups. Include onboarding, conditional pages, selection, validation presentation, progress, final pages, keyboard/accessibility, dimensions/style. Do not implement UI changes or claim rendering proof. This inventory is independent of Task 1's shared selection implementation.

## Task 3: Review and integrate

Review Task 1 implementation for spec compliance and quality, review inventory fidelity, resolve findings through the implementer, then conduct a whole-package review. Verify tests and commit documentation. Keep all unresolved GUI requirements open.

## Task 4: Correct remaining report scope

Independent review of the preceding integration found that remaining-phase reports count and label skipped base components as freshly validated. Filter presentation count and successful component messages to the actual remaining suffix; retain the complete selected guide for dependency and receipt checks. Extend the consumed-source regression with exact component count/messages. Own engine, validation report formatting, and installer tests only. Execute after Task 1 implementation and before final review.
