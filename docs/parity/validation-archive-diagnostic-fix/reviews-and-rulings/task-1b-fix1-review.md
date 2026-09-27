# Task 1b fix1 independent scoped re-review

Date: 2026-09-27. BASE `1cb3f4be432b9c5435af442517990fedb2a97482`; HEAD `88b2dd9ea8331281c2a14bc6cb1dbaabcdb2a662`.

Specification verdict: **APPROVED for this bounded Task 1b correction**. Quality verdict: **APPROVED**. Prior P1 is **addressed**. No new actionable finding in the fix-only diff. Together with the prior review's unaffected conclusions, this removes my Task 1b review blocker; it does not establish full-project C# parity or native UI acceptance.

## Why the prior finding is resolved

`src/engine.rs:617-627` builds independent FOMOD scope from selected components with completed IDs in condition context and excludes completed top-level subtrees before traversal. `collect_execution_components` at line 967 reuses `check_component` and `instruction_applies`. Applicable Choose instructions use `chosen_options` at line 941, the same helper now called by the actual executor at line 1165. The helper preserves the old behavior: all referenced option IDs must exist, comparison is case-insensitive, selected and referenced options are visited in component option order, and unselected/unreachable options are excluded.

The independent registered-state and Extract checks iterate reached owners at lines 629 and 674. FomodChoices and ResourceRegistry therefore come from the actual option, rather than accidentally inheriting the parent's metadata. Extract conditions use the existing instruction applicability rule. Generic absent Extract sources still use missing-ok matching in the FOMOD stage and retain separate archive/dry-run outcomes. The fix does not restore full installation simulation to ComponentValidation or change strict installation, recovery, report shapes or stage observer boundaries.

## Evidence inspected

I read the fix-only diff and appended handoff, checked current HEAD and final production source hashes against `docs/parity/validation-missing-source-fix/fix1/build-receipt.json`, and independently hashed both copied production executables: both match the receipt. The recorded five production fmt/build/clippy commands all exited zero for default/headless variants. `git diff --check 1cb3f4b HEAD` completed cleanly. I did not rerun builds, tests or CLI probes.

I read both fix1 probe receipts and their report stdout files. There are 39 CLI invocations, 37 of them reports. Every raw report's counters match its receipt, and every report's before/after manifests match. These are unchanged file checkpoints, not syscall evidence. The real disposable base installation is correctly recorded as mutating its owned fixture, while the strict remaining rejection is unchanged.

- The exact prior review fixture, guide SHA256 `20e28f2689dc8ac5e6dc07031bd982cbedb3fd4c7d1c9131f89a42614e0b7792`, now fails the independent FomodConfiguration stage: full without dry-run **1/0/4**, full with dry-run **2/0/4**. The headless variant independently fails **1/0/4**.
- Configuring the owning option passes **0/0/5** without dry-run and **0/0/6** with it. Configuring only the parent still fails.
- Unselected/unreachable options and Choose/Extract instructions excluded by dependencies or restrictions pass their applicable checks. Reached component dependency/restriction failures remain hard.
- A reached option with absent registered archive and prior configuration state fails; the same unselected option is excluded. Generic absent Extract without prior state passes FOMOD but fails archive validation and produces the existing dry-run availability warning.
- The remaining-phase scenario excludes the completed Choose/Move subtree whose registered archive has been consumed, detects the unconfigured suffix option, and passes once the suffix owner is configured. Completed-definition, game-byte and source-root mismatches remain Environment-only failures. Strict remaining installation rejects the unconfigured suffix before mutation.
- The original missing loose-Copy guide remains **0/1/5** with the existing component-scoped skip warning.

The prior original/pre-hoist, final-hoist and fix1 evidence retain separate source/binary bindings. No earlier measurement was attributed to this fix binary.

## Scope and constraints

No Rust test files, inline tests or custom harnesses were added, edited or run. No builds, fresh probes, production edits, subagents or GUI sessions were started in this re-review. Only this review file was written. User-owned AGENTS.md and other dirty files were preserved.

The named registry auto-extraction, Move remapping, target-basename fallback, mixed-source aggregation, archive heuristic/AttemptFixes and general nested-option parity limits remain open. Source traversal support here is not full nested-option parity evidence. Cancellation and observer lifecycle remain source-reviewed where no direct runtime evidence exists. Task 2/3 retain native rendering, clipboard, navigation and exact UI acceptance work. COMPATIBILITY.md integration remains assigned to the integrator as documented in the handoff.
