# Pinned C# checkpoint, restore, resume, provenance, and browser audit

2026-09-27 source audit. The oracle checkout is `/home/brunner56/.codex/worktrees/5813/ModSync` at `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Rust planning is pinned to `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`. No C# or Rust build, test, harness, native launch, install, restore, or source change was performed. Source observations are not runtime acceptance (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:46`).

All C# paths below are relative to the pinned oracle checkout. The active reference contains three checkpoint mechanisms with different callers and guarantees. They must be audited separately; their shared names do not establish one coherent implementation (`src/ModSync.Core/Installation/InstallCoordinator.cs:27-33`, `src/ModSync.Core/Services/GitCheckpointService.cs:18-38`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:19-52`).

## Executive findings

1. The active bulk installer creates a full ZIP recovery snapshot and a Git working copy before installation, then attempts a Git commit after each successful component (`src/ModSync.Core/Installation/InstallCoordinator.cs:35-88`, `src/ModSync.Core/Services/InstallationService.cs:1209-1276`). Git checkpoint failure is caught and downgraded to a warning, after which installation continues and the full snapshot is promoted (`src/ModSync.Core/Services/InstallationService.cs:1273-1284`). A successful C# component therefore does not imply a durable per-component checkpoint (`src/ModSync.Core/Services/InstallationService.cs:1252-1284`).
2. `GitCheckpointService` stores a non-bare repository under the game tree's `.modsync/checkpoints` directory, copies the game tree into that repository, and commits staged changes (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:21-48`, `src/ModSync.Core/Services/GitCheckpointService.cs:35-69`, `src/ModSync.Core/Services/GitCheckpointService.cs:350-378`). It checkpoints the game tree only; the mod/source tree is not part of its repository or restore path (`src/ModSync.Core/Services/GitCheckpointService.cs:26-38`, `src/ModSync.Core/Services/GitCheckpointService.cs:380-440`).
3. The separate immutable implementation has SHA-256-addressed objects, bidirectional deltas, checkpoint/session metadata, validation, garbage collection, and forward/backward restore code (`src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:21-30`, `src/ModSync.Core/Services/ImmutableCheckpoint/BinaryDiffService.cs:24-121`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:95-214`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:217-359`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:714-843`). The active bulk installer does not construct that service; it constructs `GitCheckpointService` (`src/ModSync.Core/Installation/InstallCoordinator.cs:71-88`).
4. The completion-page browser uses `GitCheckpointService`, while the main-window “Manage Checkpoints” route constructs the immutable `CheckpointManagementDialog` (`src/ModSync.GUI/Dialogs/WizardPages/BaseInstallCompletePage.axaml.cs:205`, `src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:24-49`, `src/ModSync.GUI/MainWindow.axaml.cs:5977-6003`, `src/ModSync.GUI/Dialogs/CheckpointManagementDialog.axaml.cs:26-51`). The latter calls a restore facade whose inspected implementation reports checkpoint restore disabled (`src/ModSync.GUI/Dialogs/CheckpointManagementDialog.axaml.cs:250-276`, `src/ModSync.Core/Services/InstallationCoordinatorService.cs:187-203`).
5. The C# session file persists component order, state, baseline commit, and a component-to-commit map; loading the file applies saved component states (`src/ModSync.Core/Installation/InstallSessionState.cs:13-50`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:78-108`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:280-340`). The inspected resume path does not validate that the saved commit exists, that component definitions are unchanged, or that current game bytes equal that commit before completed components are skipped (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:93-108`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:280-296`, `src/ModSync.Core/Installation/InstallCoordinator.cs:71-88`).
6. Rust currently has a stronger whole-run rollback and completion-prefix guard but no per-component checkpoint: it snapshots both mod and game trees, hashes the snapshot, blocks retry while `pending` exists, verifies restore bytes, restores the previous completion receipt, and verifies the completed prefix against definitions and current game bytes (`src/recovery.rs:14-33`, `src/recovery.rs:113-145`, `src/recovery.rs:147-205`, `src/recovery.rs:208-293`). The engine emits `ComponentCompleted` before its one terminal receipt is recorded and exposes no checkpoint identity (`src/engine.rs:786-838`, `src/engine.rs:955-995`, `src/install_progress.rs:9-53`).

These findings support a Rust design that preserves the existing whole-run safety net while adding a real immutable checkpoint commit after each successful top-level component. They do not support copying C# labels, counts, or resume state without durable artifacts (`docs/parity/install-progress-plan.md:5-14`, `docs/parity/ACCEPTANCE.md:268-270`).

## Active installer call graph

1. `InstallationService.InstallAllSelectedComponentsAsync` validates FOMOD configuration, constructs `InstallCoordinator`, and calls `InitializeAsync` before iterating selected ordered components (`src/ModSync.Core/Services/InstallationService.cs:1190-1221`).
2. `InstallCoordinator.InitializeAsync` loads or creates `install_session.json`; unless `--no-checkpoint` is active, it creates or reuses `last_good_backup.zip`, opens `.modsync/checkpoints` as a Git repository, and records its current HEAD as the baseline (`src/ModSync.Core/Installation/InstallCoordinator.cs:35-88`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:58-76`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:193-225`).
3. The installer skips entries whose loaded state is already `Completed`, `Skipped`, or `Blocked` and saves that state without checking checkpoint contents at that branch (`src/ModSync.Core/Services/InstallationService.cs:1226-1245`).
4. After `component.InstallAsync` returns success, the installer asks `GitCheckpointService` for a commit, records the returned commit ID in `ComponentCheckpoints`, and logs success (`src/ModSync.Core/Services/InstallationService.cs:1248-1271`). Checkpoint exceptions are warnings rather than component failures (`src/ModSync.Core/Services/InstallationService.cs:1273-1276`).
5. The installer then replaces the full ZIP snapshot with the new game state and later saves session state (`src/ModSync.Core/Services/InstallationService.cs:1279-1284`, `src/ModSync.Core/Services/InstallationService.cs:1331-1346`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:211-225`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:342-365`).

`--no-checkpoint` disables both the full ZIP snapshot and Git service, but the coordinator still initializes state bookkeeping and returns an ordered list (`src/ModSync.Core/Installation/InstallCoordinator.cs:41-68`). Rust's parity plan must keep its existing recovery mandatory because its documented fail-stop/restore-before-retry behavior is a safety contract, not a presentation choice (`src/recovery.rs:208-241`, `docs/parity/install-progress-integration.md:23-40`).

## Git checkpoint contract

### Initialization and storage

`GitCheckpointService` maps the game directory to `.modsync/checkpoints`, initializes a non-bare LibGit2 repository there, configures compression and a synthetic local identity, and creates an initial commit only when HEAD is empty (`src/ModSync.Core/Services/GitCheckpointService.cs:24-69`, `src/ModSync.Core/Services/GitCheckpointService.cs:337-369`). When an existing repository has a HEAD, initialization returns that HEAD without checking it against the live game tree (`src/ModSync.Core/Services/GitCheckpointService.cs:350-357`).

Before a commit, the service recursively enumerates the game tree, excludes paths whose prefix is `.modsync`, copies every remaining file into the Git work tree, and stages `*` (`src/ModSync.Core/Services/GitCheckpointService.cs:371-400`). The sync loop does not remove a Git-work-tree file when its corresponding live game file was deleted; this source shape cannot stage that live deletion because the stale copied file remains (`src/ModSync.Core/Services/GitCheckpointService.cs:380-400`). This is a source-derived defect; no native deletion probe was run.

Each non-empty commit stores component name, GUID, author, one-based index/total, and local timestamp in its message (`src/ModSync.Core/Services/GitCheckpointService.cs:83-114`, `src/ModSync.Core/Services/GitCheckpointService.cs:443-446`). An empty commit is not created: the method returns the existing HEAD but labels it with the current component in the returned in-memory `CheckpointInfo` (`src/ModSync.Core/Services/GitCheckpointService.cs:115-136`). After restart, parsing that HEAD reconstructs the earlier commit's component from its message, so an unchanged component does not retain a distinct durable checkpoint identity (`src/ModSync.Core/Services/GitCheckpointService.cs:448-496`).

### Listing, diff, and restore

Listing parses every commit message into `CheckpointInfo`; parsing failures collapse index/total or GUID to zero/empty rather than rejecting the record (`src/ModSync.Core/Services/GitCheckpointService.cs:221-248`, `src/ModSync.Core/Services/GitCheckpointService.cs:448-496`). `GetDiffAsync` compares the selected tree to another commit or the working directory and converts LibGit2 change kinds to a five-value status (`src/ModSync.Core/Services/GitCheckpointService.cs:290-335`, `src/ModSync.Core/Services/GitCheckpointService.cs:499-513`).

Restore force-checks out the selected commit into the checkpoint working tree, deletes live game files missing from that work tree, and copies work-tree files back to the game directory (`src/ModSync.Core/Services/GitCheckpointService.cs:164-215`, `src/ModSync.Core/Services/GitCheckpointService.cs:403-440`). Its work-tree enumeration excludes any path containing the substring `.git`, not only the repository metadata directory (`src/ModSync.Core/Services/GitCheckpointService.cs:403-410`). It does not restore the mod/source tree, update `install_session.json`, repair `ComponentCheckpoints`, or write an equivalent of Rust's completion receipt (`src/ModSync.Core/Services/GitCheckpointService.cs:166-215`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:111-131`).

Restore mutates the live game tree in place after checkout and has no separate restore journal in this service (`src/ModSync.Core/Services/GitCheckpointService.cs:191-209`, `src/ModSync.Core/Services/GitCheckpointService.cs:403-440`). Cancellation is checked while copying game files back, but not in the deletion loop (`src/ModSync.Core/Services/GitCheckpointService.cs:412-438`). A failed restore can therefore leave a partially changed live tree; this is a source-derived risk, not a reproduced failure.

## Full ZIP snapshot and session state

`CheckpointManager` owns `.modsync/install_session.json` and `.modsync/last_good_backup.zip`; its save uses a temporary file followed by overwrite-copy and deletion (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:58-76`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:111-131`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:328-336`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:406-410`). The session schema records a destination path, component order and state, backup path, revision, baseline commit, and per-component commit IDs (`src/ModSync.Core/Installation/InstallSessionState.cs:13-50`).

When loading an existing session, validation checks only that the state, component order, and component dictionary are non-null; it then copies saved state and timestamps onto current component objects (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:93-108`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:280-340`). It does not compare the saved destination string to the requested destination, bind current component definitions to the saved order, or verify baseline/component commit objects at that point (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:93-108`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:314-340`).

The ZIP restore deletes every top-level destination entry except `.modsync`, extracts the backup to a temporary directory, then copies it into the existing destination (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:236-277`). It does not hash the ZIP contents before deletion or verify restored files afterward (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:243-277`). Rust already verifies its snapshot manifest before mutation and each restored file afterward (`src/recovery.rs:249-293`).

## Immutable content-addressed implementation

### Data model and object store

The immutable model has a session, checkpoint, file state, and file delta. Checkpoints carry session/component identity, sequence, parent/anchor links, file states, added/modified/deleted collections, and sizes; file deltas carry source/target content hashes, source/target CAS IDs, forward/reverse delta CAS IDs, sizes, and method (`src/ModSync.Core/Services/ImmutableCheckpoint/FileProvenance.cs:11-108`). Despite the filename, the model does not carry an instruction owner, option-owner path, patcher process, acquisition URL, or other fine-grained origin; its durable attribution is component-level checkpoint membership (`src/ModSync.Core/Services/ImmutableCheckpoint/FileProvenance.cs:24-85`).

The CAS computes SHA-256, maps it to `objects/<first-two>/<remainder>`, avoids a copy when the path already exists, and can enumerate or garbage-collect unreferenced object names (`src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:32-47`, `src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:69-108`, `src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:246-288`, `src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:344-370`). `StoreFileAsync` hashes before copying but publishes with direct `File.Copy` rather than a temporary-file/fsync/rename sequence, and it does not rehash the published object (`src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:69-107`). `StoreStreamAsync` uses create-new and removes a partial object on a caught copy failure, but does not fsync or rehash before treating the hash path as complete (`src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:110-197`).

Files below 100 KiB use complete source and target CAS objects. Larger files additionally get forward and reverse Octodiff objects while retaining complete source and target objects (`src/ModSync.Core/Services/ImmutableCheckpoint/BinaryDiffService.cs:14-21`, `src/ModSync.Core/Services/ImmutableCheckpoint/BinaryDiffService.cs:54-121`). Restore applies full copies or deltas directly to the output path and does not verify the result against `SourceHash` or `TargetHash` in the inspected method (`src/ModSync.Core/Services/ImmutableCheckpoint/BinaryDiffService.cs:124-172`, `src/ModSync.Core/Services/ImmutableCheckpoint/BinaryDiffService.cs:206-231`).

### Session and checkpoint lifecycle

Starting a session scans the game tree, stores hashes/size/mtime in an in-memory baseline, and writes `session.json` plus `baseline.json` (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:54-93`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:500-532`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:585-622`). The baseline scan does not store baseline file bytes in CAS or populate `FileState.CASHash` (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:78-88`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:520-528`).

On the first modification of a baseline file, checkpoint creation reads the previous `CASHash` and asks the CAS to retrieve it before creating a delta (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:151-174`). Because baseline `CASHash` is unset by the baseline scan, this path cannot retrieve an ordinary first-modified baseline file as written (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:78-88`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:151-174`, `src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:199-215`). This is a source-derived defect; no runtime probe was run.

Every tenth checkpoint is marked as an anchor and replaces its sparse `Files` map with the current scan (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:95-137`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:190-205`). That scan also lacks CAS IDs for unchanged files, so the anchor metadata alone is not a complete content-restorable snapshot (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:190-205`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:500-532`).

Checkpoint metadata and session metadata are written directly with `StreamWriter`; there is no temporary publish, fsync, or multi-file commit point (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:600-648`). Checkpoint JSON is written before the checkpoint ID is appended to session JSON, so a crash can leave an orphan manifest; the reverse ordering is not used (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:197-207`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:624-632`).

Restore walks checkpoint deltas backward or forward relative to the in-memory `_checkpointCounter`, then rescans the game directory (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:217-258`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:260-359`). It requires `_currentSession` to resolve metadata, but `CheckpointService` has no inspected method that opens an existing session into `_currentSession`; `ListSessionsAsync` only returns deserialized values (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:29-32`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:388-427`, `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:650-665`). A new process therefore cannot infer that listability equals resumability from this source.

Validation checks only that named CAS objects exist; it does not hash the objects or apply and verify the restore graph (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:714-765`). Repair can recreate a missing complete object only when the current game file still hashes to the expected CAS ID (`src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:786-843`).

## Browser and management surfaces

`GitCheckpointBrowserDialog` creates a Git service for the supplied game directory and asynchronously lists commits plus HEAD (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:24-64`). Because `ListCheckpointsAsync` initializes the service when needed, opening the browser can initialize a repository and create a baseline if one is absent (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:57-64`, `src/ModSync.Core/Services/GitCheckpointService.cs:221-226`, `src/ModSync.Core/Services/GitCheckpointService.cs:44-69`).

The browser shows checkpoint count/current checkpoint, selected component, timestamp, full commit ID, component progress, and a selected-to-current diff grouped into added/modified/deleted files (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:57-89`, `src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:115-161`). It displays at most ten paths per group followed by an omitted-count row (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:162-255`). Renamed changes are not rendered in any of those three groups even though the service can return `Renamed` (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:158-255`, `src/ModSync.Core/Services/GitCheckpointService.cs:499-513`).

The browser disables restore for the current commit, confirms that current game changes will be lost, calls `RestoreCheckpointAsync`, reports success/failure, and reloads the list (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:115-130`, `src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:269-313`). It has busy state, refresh, close, and cancellation-on-close behavior (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:316-352`). It does not update the saved component-state prefix after a restore (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:269-313`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:170-181`).

The completion page opens this Git browser (`src/ModSync.GUI/Dialogs/WizardPages/BaseInstallCompletePage.axaml.cs:205`). The main-window checkpoint menu instead opens `CheckpointManagementDialog`, which constructs the immutable service, lists immutable sessions/checkpoints, and routes restore through `InstallationCoordinatorService.RestoreToCheckpointAsync` (`src/ModSync.GUI/MainWindow.axaml.cs:5977-6003`, `src/ModSync.GUI/Dialogs/CheckpointManagementDialog.axaml.cs:48-51`, `src/ModSync.GUI/Dialogs/CheckpointManagementDialog.axaml.cs:79-98`, `src/ModSync.GUI/Dialogs/CheckpointManagementDialog.axaml.cs:167-198`, `src/ModSync.GUI/Dialogs/CheckpointManagementDialog.axaml.cs:250-276`). The inspected facade logs that restore is unavailable and returns without restoring (`src/ModSync.Core/Services/InstallationCoordinatorService.cs:194-203`). These are two different user-visible claims, not interchangeable evidence.

## C# behavior to preserve, strengthen, or reject

| Area | Source-established reference behavior | Rust parity ruling |
|---|---|---|
| Per-component boundary | Git checkpoint is attempted after successful component execution (`src/ModSync.Core/Services/InstallationService.cs:1248-1271`) | Preserve the boundary, but make durable checkpoint commit mandatory before `ComponentCompleted` (`src/engine.rs:978-992`, `docs/parity/ACCEPTANCE.md:46`) |
| Empty component delta | Returns prior HEAD under a transient current-component label (`src/ModSync.Core/Services/GitCheckpointService.cs:115-136`) | Reject; every completed component gets a unique immutable manifest/head even when its tree equals its parent (`docs/parity/ACCEPTANCE.md:46`) |
| Checkpoint failure | Warn and continue (`src/ModSync.Core/Services/InstallationService.cs:1273-1284`) | Reject; stop with whole-run recovery pending because completion without a required checkpoint would fabricate resumability (`src/recovery.rs:208-241`, `docs/parity/ACCEPTANCE.md:45-46`) |
| Data coverage | Git stores game files only (`src/ModSync.Core/Services/GitCheckpointService.cs:380-440`) | Strengthen to both mod and game roots because Rust `Move`/`Delete` operations mutate sources and current rollback covers both roots (`src/recovery.rs:134-145`, `src/recovery.rs:217-241`, `src/recovery.rs:266-285`) |
| Content identity | Immutable service names objects by SHA-256 (`src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:32-47`, `src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:257-267`) | Preserve SHA-256 identity, add atomic no-clobber publication and verification (`src/recovery.rs:295-302`) |
| Provenance | Durable checkpoint attribution is component name/GUID plus file deltas (`src/ModSync.Core/Services/ImmutableCheckpoint/FileProvenance.cs:24-85`) | Preserve component attribution and add guide-definition digest, ordered prefix, root/path, before/after hashes, and change kind; do not claim instruction-level origin without engine evidence (`src/install_progress.rs:21-53`) |
| Restore | Git browser restores any commit's game state (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:269-303`) | Preserve restore-to-any-checkpoint, but restore both roots, verify all inputs before mutation, update the prefix receipt, and leave a recoverable journal on interruption (`src/recovery.rs:249-302`, `docs/parity/ACCEPTANCE.md:46`) |
| Resume | Saved completed states are reapplied (`src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:93-108`, `src/ModSync.Core/Services/Checkpoints/CheckpointPaths.cs:280-296`) | Preserve restart discovery, but resume only an exact ordered prefix whose roots, definitions, checkpoint graph, CAS objects, and live/restored bytes verify (`src/recovery.rs:180-205`, `src/engine.rs:910-931`) |
| Browser | List/current/details/diff/confirm/restore/refresh/busy are present (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:57-161`, `src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:269-352`) | Preserve those actions through one shared Rust library; add explicit verify/resume state and never create a baseline merely by opening the browser (`docs/parity/ACCEPTANCE.md:46`) |

## Rust starting point and proof boundary

Rust's recovery directory is adjacent to the game directory and keyed by the game path; it holds one `snapshot`, `snapshot.json`, `pending`, and `completion.json` contract (`src/recovery.rs:14-33`, `src/recovery.rs:155-178`, `src/recovery.rs:217-247`). The snapshot covers both roots and rejects symlinks/special files (`src/recovery.rs:90-110`, `src/recovery.rs:217-241`). Restore validates its manifest before deleting live entries and validates restored hashes afterward (`src/recovery.rs:249-293`).

The completion receipt binds canonical root paths, ordered serialized component definitions, and a game-tree hash manifest; prefix operations reject missing/corrupt evidence, changed definitions, or changed game bytes (`src/recovery.rs:147-205`, `src/engine.rs:841-931`). It does not record mod-tree bytes, a per-component head, a checkpoint graph, per-file provenance, or crash-reconcilable publication state (`src/recovery.rs:147-178`).

The engine creates its single recovery snapshot after silent preflight, runs every remaining component in order, records one completion receipt, clears `pending`, and only then emits terminal success (`src/engine.rs:793-838`). It emits `ComponentCompleted` immediately after component execution and before terminal receipt persistence (`src/engine.rs:955-995`). The typed progress model has recovery-snapshot state but no checkpoint ID, sequence, or checkpoint-recording phase (`src/install_progress.rs:9-53`).

The acceptance ledger therefore correctly calls F11 partial and keeps per-component checkpoints, provenance, crash durability, GUI transitions, and full resume semantics open (`docs/parity/ACCEPTANCE.md:46`, `docs/parity/ACCEPTANCE.md:268-270`). No source audit, schema, log line, or existing one-snapshot receipt closes those requirements (`docs/parity/ACCEPTANCE.md:28-32`).

## Audit claim blocks

claim: The active C# installer attempts one Git commit after each successful component but does not require that commit to succeed.
referent: `src/ModSync.Core/Services/InstallationService.cs:1248-1284` in oracle `d31ccbd3bfcddb4705917eafa6483715c166de3b`.
checked: direct pinned-source read; no runtime probe.
verdict: holds
confidence: high; only a different runtime path bypassing this method would narrow the claim.

claim: C#'s immutable content-addressed implementation is not the checkpoint service constructed by the active bulk installer.
referent: `src/ModSync.Core/Installation/InstallCoordinator.cs:71-88`; `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:19-52`.
checked: direct pinned-source constructor/call-site trace.
verdict: holds
confidence: high; a separate caller does not change the active bulk-install call graph.

claim: C# Git checkpoints cover the game tree but not the mod/source tree.
referent: `src/ModSync.Core/Services/GitCheckpointService.cs:26-38,380-440`.
checked: direct pinned-source storage and restore trace.
verdict: holds
confidence: high; runtime cannot add an unreferenced second root to these methods.

claim: C# Git synchronization cannot record a live-file deletion because it never removes the stale work-tree copy before staging.
referent: `src/ModSync.Core/Services/GitCheckpointService.cs:371-400`.
checked: source-derived control/data-flow review; no deletion probe.
verdict: holds
confidence: medium; a LibGit2 side effect outside this method would change it, and a disposable native deletion probe would raise confidence.

claim: C# immutable checkpoint creation cannot retrieve an ordinary first-modified baseline file because baseline states have no CAS hash.
referent: `src/ModSync.Core/Services/ImmutableCheckpoint/CheckpointService.cs:78-88,151-174,500-528`; `src/ModSync.Core/Services/ImmutableCheckpoint/ContentAddressableStore.cs:199-215`.
checked: source-derived field-flow review; no runtime probe.
verdict: holds
confidence: high; a hidden serializer/default that populates `CASHash` before line 159 would change it.

claim: The two C# checkpoint dialogs use different backends and the main-window immutable restore route reaches a disabled facade.
referent: `src/ModSync.GUI/Dialogs/WizardPages/BaseInstallCompletePage.axaml.cs:205`; `src/ModSync.GUI/MainWindow.axaml.cs:5977-6003`; `src/ModSync.GUI/Dialogs/CheckpointManagementDialog.axaml.cs:250-276`; `src/ModSync.Core/Services/InstallationCoordinatorService.cs:187-203`.
checked: direct pinned-source UI-to-service trace.
verdict: holds
confidence: high; native UI evidence would show presentation, not change the called methods.

claim: Rust has verified whole-run recovery and prefix receipts, but no real per-component checkpoint or restore-to-arbitrary-boundary API.
referent: `src/recovery.rs:147-293`; `src/engine.rs:786-931`; `src/install_progress.rs:9-53`; `docs/parity/ACCEPTANCE.md:268-270` at Rust `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`.
checked: direct source and ledger read; checkpoint-related Rust files were clean when pinned.
verdict: holds
confidence: high; a later checkpoint implementation would make this snapshot stale and requires a fresh audit.
