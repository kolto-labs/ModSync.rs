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

## Completed repository state

The sole registered checkout is `/run/media/brunner56/MyBook/Workspaces/ModSync.rs`, on `main`, with upstream `origin/main`. `/home/brunner56/Workspaces/ModSync.rs` is a symlink to that same checkout. All 18 additional registered checkout directories were moved intact into archive storage, then unregistered. Five standalone clones and the orphaned checkout were also moved intact into archive storage. Historical branch/archive refs remain recoverable; no branches were deleted or force-pushed.

The implementation/reconciliation source boundary is `174a331975e1218b44dd66bd618b7252718760e9`, successfully pushed to GitHub. Production checks on that source passed:

- `cargo fmt --all -- --check`
- `cargo build --locked --lib --bin modsync -j2` (1m31s)
- `cargo clippy --locked --lib --bin modsync -j2 -- -D warnings` (22s)
- `cargo build --locked --lib --bin modsync --no-default-features -j2` (27s)
- `cargo clippy --locked --lib --bin modsync --no-default-features -j2 -- -D warnings` (19s)
- Windows GNU no-default production build/link (2m06s), without executing the executable.
- `git diff --check`

Six tracked inline test-module suffixes were compared byte for byte with `eed8089` and remained unchanged. This was a source comparison; no test was run or compiled. CI was not dispatched or polled.

Recovery inventory and full Git bundles: `/home/brunner56/.cache/modsync-consolidation-20261002`. Home cache checkouts: its `retired-checkouts` and `retired-standalone-clones` directories. Former nested USB checkouts: `/run/media/brunner56/MyBook/Workspaces/.modsync-rs-retired-20261002`. `retired-checkouts.json` maps original paths to preserved paths. The obsolete original-checkout AGENTS edit is also retained in a named Git stash and the initial patch archive; the active AGENTS retains the Rust workflow and current restrictions.
