# Repository consolidation — 2026-10-02

The user paused parity implementation to consolidate GitHub and local ModSync.rs work onto the default `main` branch. GitHub default was verified as `main`; the open-PR inventory returned no PRs.

## Inventory and preservation

Nineteen registered checkouts, five standalone development clones, one orphaned checkout and a preseed Git store were examined. Eight valid checkouts contained dirty work. Registered refs, tracked/index patches, untracked files and standalone histories were preserved under `/home/brunner56/.cache/modsync-consolidation-20261002`. Unique older receipt material is retained under `/home/brunner56/.cache/modsync-reconciliation-preserved-20261002`.

All registered branch histories were already included in fetched `origin/main` (`254b8d1`) except the single 13-commit chain ending at `eed8089`. That chain and the active implementation snapshot (`f68dcfc`) were fast-forwarded into `main`. The original checkout’s audit/plan changes (`58cfa40`) were merged, with its selection-outcome documentation conflict resolved against current production semantics. Every preseed head was already an ancestor of consolidated `main`.

## Source reconciliation

Unique standalone-clone changes were adapted into current production interfaces:

- Wizard navigation requires successful validation for the current input stamp.
- Premium validation retries bounded transient failures and distinguishes valid free accounts from invalid authorization.
- Metadata suppresses only HTTP 404; metadata/link HTTP authorization refusals remain terminal, including unkeyed requests.
- Conflicting/malformed Peer-mode case aliases fail closed; Fallback wins over Race.
- Embedded Markdown metadata preserves InstallBefore, InstallAfter and ExcludedDownloads.
- Sharing enrollment verifies records before batch publication and rolls back newly published pending identities on cancellation or handled write failure. This is not crash-atomic publication of multiple files.

The old standalone sharing commit `d7b60ea` and dirty rp/share implementations are retained as alternate source snapshots. Current implementations supersede their old schema, owner/UI, metadata replacement policy, direct-install buttons and local-clock expiry policy. Their unique required behavior was ported; the older implementations were not substituted wholesale. The local MEGA MAC variant is preserved separately: current code already verifies the same condensed MAC through the pinned crate, so no missing integrity behavior was identified.

Machine-local agent session files, nested retired checkout directories and generated Python bytecode are excluded from published source. Existing test modules and probe harnesses were not edited or executed for reconciliation.

## Validation boundary

Focused source/spec review found no remaining actionable issues after authorization refusal handling was corrected. Production-only formatting, build and Clippy checks are recorded with the completion result. CI, test targets, runtime/native probes and package publication remain deferred. Consolidation does not close F01–F28 acceptance or F29 parity.
