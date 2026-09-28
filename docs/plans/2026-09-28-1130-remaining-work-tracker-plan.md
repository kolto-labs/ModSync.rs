# ModSync remaining-work tracker

Updated: 2026-09-28. Baseline: main `254b8d1`, downloads GUI step 2 merged in PR #6.

Purpose: one checklist of everything still needed for full Rust parity and the related session commitments. This is a tracker, not an implementation guide. Unchecked means implementation **or acceptance evidence** remains; it does not mean nothing exists. Check an item only when its scope is reviewed, verified and integrated.

## Current execution policy

User instruction on 2026-09-28: implement everything before testing or waiting for CI; do not run, edit, or create filesystem tests. Source implementation and independent static review continue. Runtime probes, test work, CI dispatch/polling, and acceptance runs are deferred. Earlier receipts apply only to their recorded source and do not validate later changes. Implementation may advance after static review while the acceptance checkbox remains open.

The user authorized the ModOrchestration workflow correction, which reached default-branch head `66e468a7` before this deferral. Its latest observed CI run reached real installs but failed on fontconfig timestamp stderr; subsequent CI work is deferred. The K2 schema question is prepared only, per the user's contact preference; no author message is authorized.

## Next sequence

- [ ] Finish downloads GUI step 3: peer races/cancellation, window close, completion-page status, Save/record diffs, stale guide/root outcomes, direct Install blocking, remaining sizes/platforms.
- [ ] Implement and verify persistent proxy-only peer sharing, including lifecycle, budgets, restart, Stop/exit and install handoff.
- [ ] Finish host-provider H1–H4: Nexus residuals, DeadlyStream, MEGA, metadata/GameFront contracts and authorized live proof.
- [ ] Finish Markdown M1–M3: export, legacy metadata and real full-guide ingestion/drafts.
- [ ] Implement and verify per-component checkpoints, crash recovery and ordered resume/replay.
- [ ] Finish engine/archive/patcher E1–E7.
- [ ] Close the remaining domain checklist below, then complete F29 and the final acceptance audit.

## Known acquisition and FOMOD follow-ups

- [ ] Fix Origin-present race adoption so refusal can still fall back to Peer.
- [ ] Make existing-file hash/adoption cancellation responsive.
- [ ] Handle an unnamed Nexus target whose API-revealed filename already exists.
- [ ] Handle concurrent known-name publication outside a serialized group.
- [ ] Report actual Origin download totals where available.
- [ ] Reconcile residual FOMOD ownership, hard-dependency scope, missing-archive warnings and folder sources named like archives.
- [ ] Verify per-archive embedded/run-choice preservation; reconcile the older replacement warning against merged S2b.
- [ ] Finish desktop FOMOD reconfiguration and S3 semantic/corpus acceptance.

## Full parity checklist

- [ ] **F01 — Formats:** complete format/error corpus, GUIDs, stdin/autodetection and bilateral round trips.
- [ ] **F02 — Markdown:** full/spoiler-free K1/K2 fidelity, metadata/IDs and native paste/import.
- [ ] **F03 — Drafts:** complete direction coverage, archive-generated instructions, ambiguous-draft review and direct-Markdown execution.
- [ ] **F04 — Selection:** remaining desktop actions, nested/Unicode models, dependencies, ordering and shared CLI/GUI selections.
- [ ] **F05 — Actions:** complete real/VFS action, path, wildcard, condition, overwrite and missing-source behavior.
- [ ] **F06 — Archives:** supported formats/SFX, empty directories, large/nested/corrupt inputs, containment and resource measurements.
- [ ] **F07 — Cleanup:** full CleanList matching and texture-priority final-file comparisons.
- [ ] **F08 — Validation:** remaining backend classes, exact reports and native navigation/copy across clean/warning/failure cases.
- [ ] **F09 — Patchers:** packaged discovery, arguments/logs/failures and real patched-resource equivalence across backends/platforms.
- [ ] **F10 — Coordinator:** component boundaries, cancellation, nested Choose, crash durability and best-effort contract reconciliation.
- [ ] **F11 — Checkpoints:** durable storage/provenance, browse/restore, restart/resume and exact ordered replay through CLI/GUI.
- [ ] **F12 — Resources:** stale/cache behavior, complete filename resolution and shared fetch/record metadata.
- [ ] **F13 — Hosts:** complete provider discovery/auth/transfer/update behavior and live acceptance.
- [ ] **F14 — Download queue:** retries, races, progress, cancellation, restart and stale-event acceptance.
- [ ] **F15 — FOMOD:** remaining conditions/navigation/priorities, consumer scope, persistence/recovery and corpus equivalence.
- [ ] **F16 — Profiles:** full schema/text contracts, native CRUD/apply/activate and active-profile/platform integration.
- [ ] **F17 — Settings:** complete typed desktop settings/defaults, theme/game/locale mapping, API-key validation and safe persistence.
- [ ] **F18 — Deployment:** engine/session integration, GUI management, ownership/reinstall/purge and C#/platform interoperability.
- [ ] **F19 — Conflicts:** option/order corpus, attribution/winner reports and remaining-phase desktop preview.
- [ ] **F20 — Merge:** URL/domain heuristics, field/resource/option decisions, dialogs and saved-guide fidelity.
- [ ] **F21 — Editor:** complete option/raw/import/regex/file-browser/edit actions and save/reload equivalence.
- [ ] **F22 — Updates:** file/hash/key UI, cancellation, persisted results/badges and provider acceptance.
- [ ] **F23 — Protocols:** NXM registration, second-instance handoff, cancellation and URI security on supported platforms.
- [ ] **F24 — Wizard:** every remaining onboarding/review/conditional/completion flow, real folder opening and native page acceptance.
- [ ] **F25 — Main window:** legacy controls/menus, rich text/spoilers, keyboard/focus, accessibility, themes and DPI.
- [ ] **F26 — CLI:** complete verb/alias/option/preload/pipeline/stdout/stderr/schema/exit contract comparison.
- [ ] **F27 — Diagnostics:** persisted consent, automatic events, diagnostics UI, authenticated transport and secret redaction.
- [ ] **F28 — Packaging:** native artifacts/resources/patchers, migration, installation/launch and supported-platform release validation.
- [ ] **F29 — Full builds:** K1 and K2 Rust/C#/manual lanes with identical guides, archives and selections; per-step and final-content comparisons; CLI/GUI equivalence and live-game proof.

## Integration and external blockers

- [ ] Reconcile historical status prose and newly merged/uncommitted audits against current source; derive bounded tasks where a domain still lacks a current plan.
- [ ] Resolve GitHub Actions billing/spending-limit failure and obtain actual CI results, including the unrun Windows cross-check.
- [ ] Resolve the frozen-test/CI contract at the authorized post-parity point; then repair obsolete fixtures and run the deferred test/all-target checks.
- [ ] Refresh/preserve source-bound evidence and reference inputs before final acceptance; do not reuse old counts as current-source proof.
- [ ] Complete independent specification/quality review and integration for each remaining slice.
- [ ] Resolve the ModOrchestration CI-change permission blocker and obtain reference CI evidence.
- [ ] Complete ModOrchestration duplicate-sweep dry-run parity before restoring its wiring.
- [ ] Resolve the mod-builds K2 schema question about the required `main` section with its author.
- [ ] Reconcile and close every acceptance-ledger row and documented compatibility difference.

## Completion rules

Preserve the dirty primary checkout and AGENTS.md. Keep the Rust-test freeze until its stated release point. Use isolated production/native evidence meanwhile. Full installs follow literal full.md order: stop, restore before the failed step, fix, then replay. Preserve existing archives and live baselines. Authentication, CAPTCHA and permission blockers remain explicit. Release publication remains a separate manual action.

Done means every applicable item above has evidence and all F01–F29 acceptance requirements are met; compilation or a narrow successful slice is insufficient. Newly discovered gaps join this checklist.

## Detailed sources

- [Acceptance ledger](../parity/ACCEPTANCE.md), [progress](../parity/PROGRESS.md), [remaining-scope map](../parity/remaining-scope-map.md), [latest handoff](../handoffs/rust-parity-cloud-2.md).
- [Downloads](../parity/downloads-gui-plan.md), [step-2 evidence and residuals](../parity/downloads-gui-step2/README.md), [persistent sharing](../parity/persistent-sharing-plan.md), [providers](../parity/host-providers-plan.md), [FOMOD](../parity/fomod-remaining-plan.md).
- [Markdown](../parity/markdown-remaining-plan.md), [checkpoints](../parity/checkpoint-plan.md), [engine/archive/patchers](../parity/engine-archive-patcher-remaining-plan.md).
- [Selection](../parity/mod-selection-plan.md), [GUI tools](../parity/gui-tools-plan.md), [wizard/main window](../parity/wizard-mainwindow-plan.md), [platform/protocol/packaging](../parity/platform-protocol-packaging-plan.md).
