# Authoritative per-component immutable checkpoints, restore/resume, and browser plan

Status: plan only, 2026-09-27. Nothing in this document is implemented. Rust citations are pinned to `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`; the cited checkpoint-related Rust files were clean at pin time. The pinned C# oracle is `d31ccbd3bfcddb4705917eafa6483715c166de3b`, audited in [checkpoints-csharp-audit.md](checkpoints-csharp-audit.md). This slice advances F10, F11, F24, and F26 but cannot close F11 or full C# parity until crash/restart, CLI, native browser, cross-platform, and full-build evidence is complete (`docs/parity/ACCEPTANCE.md:45-46`, `docs/parity/ACCEPTANCE.md:59-64`).

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md` and every unrelated worktree change. The checkout already contains concurrent source and documentation work; ownership below is serialized and expansion requires an explicit handoff (`docs/parity/install-progress-plan.md:189-200`, `docs/parity/download-orchestration-plan.md:355-368`).
- The current owners of `src/recovery.rs`, `src/ui.rs`, and `src/main.rs` retain those files. Checkpoint integration touching a currently owned file waits for its owner to deliver and release it; no worker rebases, resets, copies over, or edits around another owner's pending work (`docs/parity/download-orchestration-plan.md:28-31`, `docs/parity/download-orchestration-plan.md:355-368`).
- Do not create, modify, port, hide, compile as a custom harness, or run Rust tests before full C# parity. Existing test and inline `#[cfg(test)]` source stays byte-for-byte unchanged. This slice uses production library/CLI/native paths with disposable fixtures and static review (`docs/parity/ACCEPTANCE.md:20-26`, `docs/parity/install-progress-integration.md:23-29`).
- Do not build during this documentation task. Future authorized production builds run on the host because the container has no `cc`, use a dedicated `CARGO_TARGET_DIR`, and limit Cargo to `-j 2`; those environment facts do not prove any feature (`docs/parity/install-progress-integration.md:41-43`).
- Keep desktop and CLI install behavior in the shared engine. CLI and GUI may format checkpoint facts and hold presentation state; neither may derive the completed prefix, checkpoint validity, boundary delta, restore set, resume eligibility, or success independently (`src/engine.rs:786-931`, `docs/parity/install-progress-plan.md:5-14`).
- Preserve strict guide order and fail-stop behavior. A failed checkpoint commit is an install failure with recovery pending; an earlier component is never backfilled after a later one, and resume accepts only an exact verified prefix (`src/engine.rs:910-931`, `src/recovery.rs:208-241`, `docs/parity/ACCEPTANCE.md:45-46`).
- Preserve the existing whole-run recovery snapshot as the coarse safety net. Per-component checkpoints supplement it; they do not weaken pending-marker blocking, lock ownership, snapshot verification, restore-before-retry, or terminal success after receipt/finalization (`src/recovery.rs:35-81`, `src/recovery.rs:208-293`, `src/engine.rs:793-838`).
- Every claimed checkpoint is a process-crash-committed immutable manifest whose referenced objects verify. In this plan, “committed” guarantees recovery after abrupt process termination on a supported local filesystem; it does **not** claim power-loss durability until directory-sync semantics and power-cut evidence exist. A copied directory, component state, completion receipt, log message, progress row, or partially written manifest is not a checkpoint (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:46`).
- A component is not complete until its checkpoint commit is authoritative. `ComponentCompleted` moves after checkpoint publication and prefix-receipt reconciliation; no checkpoint exception is downgraded to a warning (`src/engine.rs:978-992`, `src/ModSync.Core/Services/InstallationService.cs:1252-1284` as audited in `docs/parity/checkpoints-csharp-audit.md`).
- Validation remains read-only and never invokes external programs. Checkpoint inspection/verification may hash files and objects but cannot repair, garbage-collect, restore, resume, initialize a store, or change session/head metadata (`src/engine.rs:864-907`, `docs/parity/ACCEPTANCE.md:43`).
- Restore and resume use both mod and game roots because the engine can mutate either and current recovery protects both (`src/recovery.rs:134-145`, `src/recovery.rs:217-241`, `src/recovery.rs:266-285`).
- Legacy `Event { component, step, message }` and legacy `install --json` remain unchanged. Checkpoint facts extend only the typed install-progress/checkpoint contracts (`docs/parity/install-progress-plan.md:10-14`, `docs/parity/install-progress-integration.md:23-32`).
- Do not label the Rust store “Git” or advertise Git interoperability. The default is one native immutable content-addressed service; exact Git repository compatibility remains an explicit open question because the C# Git and immutable implementations are disconnected and both have source-proven defects (`docs/parity/checkpoints-csharp-audit.md:7-16`, `docs/parity/checkpoints-csharp-audit.md:28-76`).
- Update `COMPATIBILITY.md`, `docs/parity/ACCEPTANCE.md`, `docs/parity/GUI-CLI.md`, and `docs/parity/GUI-INVENTORY.md` when behavior and proof boundaries change; no passing narrow fixture closes F11 or F29 (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:46`, `docs/parity/ACCEPTANCE.md:64`).

## Source findings that govern the design

Rust's current recovery layer has three contracts worth preserving: an exclusive lock over canonical mod/game roots, a hash-verified whole-run snapshot with a `pending` blocker, and a completion receipt binding roots, ordered definitions, and the game manifest (`src/recovery.rs:35-81`, `src/recovery.rs:147-205`, `src/recovery.rs:208-293`). Its atomic writer fsyncs a temporary file before persistence, but does not fsync the parent directory; the checkpoint implementation must document and review the exact crash-durability boundary rather than call it power-loss safe by assumption (`src/recovery.rs:295-302`).

`Engine::install_observed` performs silent preflight, creates the whole-run snapshot, executes the remaining ordered suffix, records the completion receipt, removes `pending`, and emits `Completed` (`src/engine.rs:793-838`). `install_remaining_observed` sets an exact prefix and delegates to that same path (`src/engine.rs:841-862`, `src/engine.rs:910-931`). The new checkpoint service must integrate inside `run_ordered`, not create a second installer or have the UI replay components (`src/engine.rs:942-995`).

The existing progress contract distinguishes top-level components from nested `Choose` owners and calls `ComponentCompleted` a real engine fact (`src/install_progress.rs:9-53`, `docs/parity/install-progress-plan.md:68-77`, `docs/parity/install-progress-plan.md:100-116`). Checkpoint identity belongs to the top-level component boundary. Reached nested options remain within that boundary and never create extra checkpoints or increment the component/checkpoint total; this structural containment does not prove which writer caused each observed file change (`docs/parity/install-progress-plan.md:70-75`).

The current completion receipt has no schema version, mod-tree manifest, checkpoint head, or durable per-component history (`src/recovery.rs:147-178`). Prefix verification already proves the right policy: roots, definitions, and current bytes must match before suffix execution (`src/recovery.rs:180-205`, `src/engine.rs:910-931`). The checkpoint receipt extension must preserve those checks and add a verifiable checkpoint head rather than replace them with saved state.

The C# active installer demonstrates the desired after-component placement, but it allows checkpoint failure to warn-and-continue and checkpoints only the game tree (`docs/parity/checkpoints-csharp-audit.md:18-26`, `docs/parity/checkpoints-csharp-audit.md:28-44`). Rust rejects both defects because they conflict with its fail-stop and source-restoration contracts (`src/recovery.rs:208-293`, `docs/parity/ACCEPTANCE.md:45-46`).

## Lead technical rulings

### R1: one authoritative immutable backend

Implement one Rust-owned immutable, SHA-256 content-addressed checkpoint backend. Do not implement a Git mirror in the first parity slice and do not expose a fake Git repository. The observable parity target is real per-component history, content identity, truthful boundary deltas, browse/diff/verify, restore-to-any-boundary, and safe resume (`docs/parity/ACCEPTANCE.md:46`, `docs/parity/checkpoints-csharp-audit.md:88-110`).

The C# Git backend misses live deletions, reuses HEAD for empty commits, covers only the game tree, and does not reconcile session state on browser restore (`docs/parity/checkpoints-csharp-audit.md:28-44`, `docs/parity/checkpoints-csharp-audit.md:78-86`). The C# immutable backend is disconnected from active installation and has incomplete baseline CAS publication and non-atomic metadata writes (`docs/parity/checkpoints-csharp-audit.md:54-76`). Porting either storage format byte-for-byte would preserve defects without establishing safer behavior.

### R2: whole-run recovery remains independent

`recovery::begin` remains the first mutating install boundary and protects the complete phase entry. A checkpoint session is created only after that snapshot succeeds. If checkpoint initialization fails, no component executes and the existing snapshot remains pending (`src/engine.rs:807-828`, `src/recovery.rs:217-241`).

Each component checkpoint becomes a finer resume/restore boundary. It does not replace the phase-entry snapshot until a user explicitly resumes from a checkpoint and a new phase-entry snapshot is established there (`src/recovery.rs:208-293`, `docs/parity/ACCEPTANCE.md:264-270`).

### R3: checkpoint commit is the component commit point

After a top-level component returns success, the engine captures its post-component roots, publishes required CAS objects and an immutable checkpoint manifest, advances the session head, reconciles the compatible completion-prefix receipt, and only then emits `ComponentCompleted` (`src/engine.rs:955-995`, `src/install_progress.rs:31-53`). Any failure before the head/receipt convergence returns an error, emits no `ComponentCompleted`, runs no later component, and leaves whole-run recovery pending (`src/recovery.rs:208-241`, `docs/parity/install-progress-plan.md:104-116`).

Components with no file changes still receive a unique checkpoint manifest. The manifest's parent, sequence, component identity, and definition digest make the boundary distinct even when both root manifests equal the parent; Rust does not reproduce C#'s empty-commit reuse (`docs/parity/checkpoints-csharp-audit.md:30-36`, `docs/parity/ACCEPTANCE.md:46`).

### R4: component-boundary deltas only unless stronger evidence exists

The service records every changed path's typed root, normalized relative path, change kind, and before/after SHA-256 and size. The manifest also names the top-level component GUID/name/definition digest whose completion boundary triggered the scan, but does not call that component the causal owner of every changed byte. It records instruction or nested-owner attribution only if the engine supplies a complete write journal for every action; a post-component directory diff cannot distinguish the installer from an unrelated writer (`src/install_progress.rs:21-53`, `docs/parity/checkpoints-csharp-audit.md:56-62`, `docs/parity/checkpoints-csharp-audit.md:88-100`).

External patcher output is reported only as a change observed across the top-level component boundary, not as causally attributed to that component, a guessed patcher instruction, or a process. The browser labels it “changes observed at this boundary”; causal file provenance remains an open F11 gap until an independently reviewed write journal exists (`src/engine.rs:1019-1025`, `docs/parity/ACCEPTANCE.md:44`, `docs/parity/ACCEPTANCE.md:46`).

### R5: resume restores before it skips

An interrupted live tree may contain part of the next component. Resume never trusts it merely because a session head exists. It verifies the target checkpoint graph and objects, restores both roots to the selected committed head, verifies the restored manifests, reconciles the completion receipt, closes the old pending phase, starts a new whole-run recovery phase at that restored head, and executes only the exact suffix (`src/recovery.rs:249-293`, `src/engine.rs:841-931`).

The default resume target is the latest valid committed checkpoint. CLI and browser may allow an earlier target, but that action is semantically restore-to-checkpoint followed by resume from its exact prefix; later checkpoints remain history and are not treated as completed (`docs/parity/ACCEPTANCE.md:46`).

## On-disk contract

Keep checkpoint state under the existing hashed recovery directory so one root identity, lock, pending marker, and receipt namespace governs the installation (`src/recovery.rs:23-33`, `src/recovery.rs:35-81`). The proposed versioned layout is:

```text
<state-dir>/
  pending
  snapshot/
  snapshot.json
  completion.json
  checkpoints/
    format.json
    objects/sha256/aa/<62-hex>
    manifests/<checkpoint-id>.json
    guards/<guard-id>.json
    sessions/<session-id>.json
    active.json
    transaction.json
    restore.json
```

`format.json` identifies a Rust checkpoint-store schema and hash algorithm. It is created only by install/checkpoint initialization, never by validation or simply opening the browser (`src/recovery.rs:217-241`, `docs/parity/checkpoints-csharp-audit.md:78-86`). Unknown major versions fail closed; no code silently upgrades or deletes an unknown store (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:46`).

Objects contain complete file bytes and are named by lowercase SHA-256. Version 1 deliberately avoids binary deltas: complete objects make restore verification, garbage-collection reachability, and cross-checking simpler, while deduplication still reuses unchanged content (`src/recovery.rs:83-87`, `src/ModSync.Core/Services/ImmutableCheckpoint/BinaryDiffService.cs:54-121` as audited in `docs/parity/checkpoints-csharp-audit.md`). Delta compression remains deferred until a production corpus receipt proves it is needed.

Object publication uses a temporary file in the final object's directory, streamed hashing, `sync_all`, no-clobber persistence, a post-publish size/hash verification, and an atomic-publish helper shared by every identity-bearing file. That helper syncs the published file and containing directory after create/rename/delete on platforms/filesystems where the operation is supported, and its capability/error behavior is reviewed per supported OS. An existing object is accepted only after it rehashes correctly; a corrupt collision path fails closed and is never overwritten (`src/recovery.rs:295-327`, `docs/parity/checkpoints-csharp-audit.md:56-62`).

Task 1 owns one versioned canonical encoder for manifests, guards, transactions, session heads, and receipt-digest inputs. All hashes derive from its exact retained bytes; recovery, CLI, and GUI consume typed decoded records and never independently serialize identity-bearing content. The encoding contract fixes field order, UTF-8 escaping, integer representation, absent-versus-null rules, normalized separators, and schema version; review artifacts include canonical input/byte/hash vectors even though Rust tests remain frozen (`src/recovery.rs:163-177`, `docs/parity/ACCEPTANCE.md:20-26`).

### Checkpoint manifest

Each immutable manifest owns these facts, with exact field spelling settled in Task 1 review (`docs/parity/ACCEPTANCE.md:46`):

- schema version, session ID, checkpoint ID, parent checkpoint ID, one-based session-wide checkpoint ordinal, completed-prefix length, and creation timestamp;
- canonical mod/game roots and root identity digest;
- selected ordered guide digest plus the exact serialized definition/digest of the newly completed prefix;
- top-level component GUID, case-preserving name, definition digest, and component index/total;
- complete sorted manifests for both roots, mapping normalized relative paths to SHA-256 and size;
- a sorted component-boundary delta from the parent: root, relative path, `added|modified|deleted`, prior hash/size, new hash/size, explicitly defined as changes observed between the parent capture and this boundary rather than causal ownership;
- store-format/hash algorithm and an explicit statement that causal writer attribution is unavailable when it was not observed.

The checkpoint ID is the SHA-256 of canonical manifest content excluding the ID field itself. Canonicalization uses sorted maps, stable UTF-8 JSON, normalized root tags, and no platform-dependent path separators; exact canonical bytes must be retained in receipts (`src/recovery.rs:113-145`, `src/recovery.rs:163-177`).

Paths are accepted only from a traversal that starts at the already validated roots. Symlinks and special files remain rejected, and the `.modsync-*` state/lock paths are outside the captured roots by construction (`src/recovery.rs:46-79`, `src/recovery.rs:90-110`). Case-colliding relative paths fail on every platform rather than producing platform-dependent restore results (`docs/parity/ACCEPTANCE.md:45-46`).

### Session head and publication transaction

The session record binds session ID, roots, guide digest, ordered component GUIDs/definition digests, baseline checkpoint, authoritative head, active branch generation, committed prefix length, next session-wide checkpoint ordinal, status, and timestamps. `active.json` points to at most one active session for those locked roots (`src/recovery.rs:35-81`, `src/engine.rs:910-931`). The ordinal is unique and monotonically allocated across the session's DAG, including branches; completed-prefix length identifies the guide boundary represented by a checkpoint; branch generation plus authoritative head identifies current history; install-progress `component_index/component_total` retains its separate meaning as the current run's remaining suffix position/size (`src/install_progress.rs:34-53`, `docs/parity/install-progress-plan.md:68-75`).

Checkpoint publication is process-crash recoverable rather than falsely described as one-filesystem atomic across several files:

1. Capture both roots by streaming every file into verified CAS objects, build the candidate manifests, then immediately rescan both roots. A changed path/size/hash discards the candidate and retries once; a second unstable scan fails with whole-run recovery pending. The resulting delta is named a **component-boundary delta**, not proof that an unrelated process could not have written those paths (`src/recovery.rs:83-145`, `src/recovery.rs:208-241`).
2. Atomically publish the immutable checkpoint manifest (`src/recovery.rs:295-302`).
3. Atomically publish `transaction.json` naming previous head, candidate head, prefix length, and candidate receipt digest (`src/recovery.rs:295-302`).
4. Atomically advance the session head and prefix to the candidate (`docs/parity/ACCEPTANCE.md:46`).
5. Atomically write the compatible `completion.json` projection for that same head (`src/recovery.rs:163-205`).
6. Re-read and verify session head, checkpoint manifest, receipt agreement, and a final complete live-root scan equal to the candidate manifest; then remove `transaction.json` and emit the committed progress fact. Any live drift fails before `ComponentCompleted` and leaves recovery pending (`src/engine.rs:978-992`, `src/recovery.rs:113-145`).

On restart, read-only inspection never guesses forward from a partial manifest or changes bytes. It returns `NeedsReconciliation` with the previous head, candidate head, and exact agreement state. A separate explicit mutating `reconcile` operation runs under the root lock before restore/resume/new installation: if session and receipt agree, it removes a stale transaction after verification; if only the session advanced, it regenerates the receipt from the verified immutable manifest; if only the receipt advanced, it verifies and advances session metadata only when the transaction and immutable candidate match; otherwise it restores the previous agreed **metadata head and receipt**, never live roots. Live-root restoration remains an explicit restore/resume action. Any ambiguous or corrupt combination blocks reconciliation, install, restore, and resume and remains visible to read-only `checkpoint verify` (`src/recovery.rs:35-81`, `src/recovery.rs:180-205`, `docs/parity/ACCEPTANCE.md:46`).

### Completion receipt v2 compatibility

Extend `completion.json` to version 2 while retaining the existing `mods`, `game`, `components`, and `files` fields read by current code (`src/recovery.rs:147-205`). Add checkpoint session/head, completed-prefix length, both-root manifests or their canonical digests, store format, and receipt digest. Old version-1 receipts remain readable only for the existing suffix contract; they do not imply per-component history or browser restore (`docs/parity/ACCEPTANCE.md:268-270`).

`verify_completion` continues to compare canonical roots, exact ordered component definitions, and live game bytes. Version 2 additionally verifies the requested prefix equals the checkpoint head, the head manifest and reachable CAS objects validate, and live mod/game manifests equal that head when used for resume (`src/recovery.rs:180-205`, `src/engine.rs:910-931`). Ordinary completed-install inspection may report source-tree drift separately because users can legitimately alter/download source archives after completion; resume never ignores source drift and restores from the checkpoint first (`src/recovery.rs:217-293`, `docs/parity/ACCEPTANCE.md:268-270`).

The whole-run snapshot schema advances with v2 receipts and captures the checkpoint-control state that existed before the phase: the exact canonical bytes and digest of `active.json` or explicit absence; the prior active session ID, head, branch generation, completed prefix, and exact canonical session-record bytes/digest or explicit absence; and the prior completion receipt already captured today. `recovery::begin` refuses an unresolved checkpoint or restore transaction, writes this snapshot before creating a child session for the new phase, and never mutates the prior session record in place (`src/recovery.rs:208-241`, `src/engine.rs:807-822`).

Whole-run snapshot restore is a journaled convergence operation over both roots, completion receipt, and checkpoint-control metadata. It marks the failed child session `Aborted` and inactive while retaining its immutable manifests, restores the prior session record and `active.json` bytes exactly (or removes `active.json` when the snapshot recorded absence), and verifies roots, receipt, active session, head, branch generation, and completed prefix before clearing `pending`. A failed phase therefore remains inspectable without becoming the active history. Suffix work is blocked until this complete prior state is restored and its referenced manifest/objects verify (`src/recovery.rs:227-241`, `src/recovery.rs:286-293`, `docs/parity/ACCEPTANCE.md:46`).

## Shared library API

Create `src/checkpoints.rs` plus focused submodules under `src/checkpoints/` if the implementation cannot remain reviewable in one file. Expose the module from `src/lib.rs`. Exact Rust identifiers may change during Task 1 review for a concrete ownership or type-safety reason; the semantics below are binding (`src/lib.rs`, `docs/parity/ACCEPTANCE.md:46`).

The shared service needs these capability groups:

- `open_read_only(roots)`: inspect format, sessions, manifests, transactions, receipt agreement, and corruption without creating directories or changing bytes;
- `begin_session(roots, guide, ordered_suffix)`: under the existing root lock and after `recovery::begin`, create a child session for this phase, publish and verify every baseline file object for both roots, the immutable baseline manifest, and only then the active session pointer; never rewrite the parent session as the new phase;
- `commit_component(component, component_index, component_total)`: scan both roots, publish objects/manifest/boundary-delta/head/receipt, and return a typed committed checkpoint fact;
- `list`, `show`, and `diff`: return typed immutable data for CLI/GUI without parsing log text;
- `verify(store|session|checkpoint, verify_live)`: hash metadata and all reachable objects, optionally compare live roots, and never repair;
- `reconcile(session)`: explicit locked metadata/receipt transaction recovery after read-only inspection reports `NeedsReconciliation`; it never changes live roots;
- `prepare_restore(checkpoint)`, `restore(checkpoint)`: preflight all objects and materialize verified candidates before live mutation, then journal and verify the live result;
- `prepare_resume(checkpoint|latest)`, `resume`: require an exact guide prefix, restore the head, reconcile receipt/recovery state, then enter the shared engine suffix path;
- `delete_session(session)`: explicit locked deletion of a non-active, non-pending session after typed impact/confirmation data; it leaves shared objects for later reachability GC;
- `garbage_collect`: explicit mutating maintenance, locked, reachability-based, unavailable during active/pending install or restore.

All paths, checkpoint IDs, component IDs, counts, definitions, and boundary deltas returned to frontends are typed facts. Frontends do not open JSON files directly, run Git, infer state from filenames, or count progress messages (`docs/parity/install-progress-plan.md:118-139`, `docs/parity/checkpoints-csharp-audit.md:78-100`).

## Engine and recovery lifecycle

### Fresh install

1. Acquire the existing mod/game lock; require no unresolved pending recovery/checkpoint transaction (`src/engine.rs:793-803`, `src/recovery.rs:46-79`, `src/recovery.rs:208-215`).
2. Run current scan, completion verification, and non-mutating preflight silently; no checkpoint session exists yet (`src/engine.rs:799-807`, `docs/parity/install-progress-plan.md:102-105`).
3. Create and verify the whole-run snapshot including prior checkpoint-control state, then begin a new child immutable checkpoint session at the same live bytes; a fresh install has an explicit no-prior-session value (`src/engine.rs:807-822`, `src/recovery.rs:217-241`).
4. Before each component, emit the existing `InstallingComponent` fact. Execute that top-level component and all reached nested owners in current order (`src/engine.rs:969-985`, `src/install_progress.rs:21-53`).
5. After component execution, emit a new factual `RecordingCheckpoint` phase; publish the real immutable checkpoint and v2 prefix receipt (`src/engine.rs:985-992`, `docs/parity/ACCEPTANCE.md:46`).
6. Emit `ComponentCompleted` only with the committed checkpoint ID/sequence attached. The installed count advances at this point, not at instruction return (`docs/parity/install-progress-plan.md:104-114`).
7. After the final component checkpoint, `RecordingCompletion` verifies that session head, prefix receipt, and live roots agree. `recovery::finish` then clears the whole-run pending marker and `Completed` is emitted (`src/engine.rs:829-838`).

If a component changes no bytes, steps 5-6 still publish a unique manifest and advance the prefix. If scanning, object publication, manifest publication, receipt reconciliation, or final verification fails, no later component runs and whole-run recovery remains pending (`src/recovery.rs:208-241`, `docs/parity/ACCEPTANCE.md:45-46`).

### Crash discovery and resume

Read-only startup inspection classifies exact states: no session; clean completed session; pending before any component checkpoint; pending with the authoritative active-branch head; `NeedsReconciliation`; corrupt/ambiguous state; restore in progress (`src/recovery.rs:208-293`, `docs/parity/ACCEPTANCE.md:46`). It never silently resumes, repairs, removes a transaction, or rewrites metadata. A mutating action remains blocked until the user explicitly runs reconciliation or approves it as the first named step of restore/resume (`src/recovery.rs:180-215`, `docs/parity/ACCEPTANCE.md:46`).

`resume` is an explicit engine operation, not a frontend loop:

1. Acquire the existing root lock and verify guide/root/session identity, ordered definitions, checkpoint chain, referenced objects, active branch/generation, and target prefix (`src/recovery.rs:46-79`, `src/engine.rs:910-931`). `--latest` means only the verified authoritative head of that active branch; selecting retained non-current history requires an explicit checkpoint ID.
2. Materialize both target roots into owned temporary directories on their respective filesystems, verify full manifests, and write `restore.json` before live mutation (`src/recovery.rs:249-285`).
3. Replace live contents while preserving locked root directory inodes, verify every restored file, and retain a recoverable journal until both roots match (`src/recovery.rs:266-285`).
4. Reconcile v2 receipt to the selected prefix. Close the old pending phase only after the selected checkpoint is live and verified (`src/recovery.rs:286-293`).
5. Enter a new shared `install_remaining_observed`-equivalent path whose first mutation is a new whole-run snapshot at that checkpoint. Execute only the suffix; do not backfill or reorder (`src/engine.rs:841-862`, `src/engine.rs:910-931`).

If the target is the baseline checkpoint, the completed prefix is empty and the next run is a normal full install from verified baseline bytes. If the target is already final, `resume` reports no remaining components and does not fabricate install progress (`src/engine.rs:913-920`, `docs/parity/install-progress-plan.md:187`).

### Restore without resume

Restore accepts the baseline or any committed checkpoint in the selected session. It uses the same verified materialization/journal path as resume, restores both roots, creates a new active branch generation rooted at the selected checkpoint, updates the session's authoritative head and v2 completion prefix, and retains later immutable history as non-current branches (`docs/parity/ACCEPTANCE.md:46`). It does not immediately install the suffix. `--latest` thereafter resolves to this active-branch head, never a larger ordinal or newer timestamp on retained history.

Before live mutation, the service captures every current file from both roots into verified CAS objects and publishes an immutable restore-guard manifest under `guards/`. The guard binds both complete root manifests, canonical roots/root identity, current session/branch/head, exact completion-receipt bytes and digest, and any pre-existing whole-run pending/snapshot identity. The service verifies that every guard object is reachable before publishing `restore.json`; only then may live replacement begin (`src/recovery.rs:113-145`, `src/recovery.rs:208-293`).

`restore.json` is a versioned state machine with target checkpoint, guard ID, and phases `Prepared`, `ReplacingMods`, `ReplacingGame`, `VerifyingTarget`, `UpdatingMetadata`, and `Committed`. Each phase transition uses the shared publish-and-directory-sync helper. Replacement is idempotent: rerunning a root phase deletes/copies within the locked root inode from an already verified materialization and verifies the full target manifest afterward (`src/recovery.rs:249-293`).

After interruption, read-only inspection reports the exact journal phase and offers two explicit mutating recoveries: roll forward to the target or roll back to the guard. Both first revalidate all target/guard objects, may repeat any root-replacement phase, verify both final root manifests, restore the matching receipt/session/branch metadata, and remove `restore.json` only after live roots and metadata converge. Install, ordinary restore, resume, GC, and session deletion remain blocked while the journal exists (`src/recovery.rs:208-293`, `docs/parity/ACCEPTANCE.md:46`).

Cancellation is honored during object verification and temporary materialization. Once live-tree replacement begins, cancellation becomes a requested stop after the current journaled recovery step; it cannot abandon an unclassified partial tree (`src/recovery.rs:249-293`, `docs/parity/install-progress-plan.md:182-185`).

## Install-progress compatibility

Extend `install_progress::Phase` with `RecordingCheckpoint`. Keep every existing phase name and meaning **except `ComponentCompleted`**, whose contract is deliberately replaced: it now implies both component execution and mandatory checkpoint commit. Existing adapters must be reviewed against that semantic change; it removes no already-implemented F11 checkpoint fact because F11 remains partial (`src/install_progress.rs:9-19`, `src/engine.rs:978-992`, `docs/parity/ACCEPTANCE.md:46`).

Add an optional typed checkpoint projection to `Progress`: session ID, branch generation, checkpoint ID, parent ID, session-wide checkpoint ordinal, completed-prefix length, component GUID, and changed-file counts. It is populated only for committed checkpoint/component facts. The existing `component_index/component_total` remains the current run's suffix position/size; it is not reused as DAG history position. `snapshot_pending` keeps its existing whole-run meaning and is not renamed to checkpoint state (`src/install_progress.rs:31-53`, `docs/parity/install-progress-plan.md:68-77`).

Typed `install --install-progress-json` receives an explicit schema version before this expansion is shipped. Legacy `--json` remains byte-identical. Review must decide whether typed schema version 1 remains selectable or whether the still-partial typed stream makes a documented additive version 2 sufficient; no adapter silently changes legacy `Event` output (`docs/parity/install-progress-plan.md:141-153`, `docs/parity/ACCEPTANCE.md:9`).

The Installing page replaces the current one-snapshot-only checkpoint row only after real facts exist. It may show `Recording checkpoint N/M`, the committed short ID, and `N checkpoints committed`; it never increments on component start, log text, manifest creation before head/receipt convergence, or C# callback behavior (`docs/parity/install-progress-plan.md:169-185`, `docs/parity/checkpoints-csharp-audit.md:18-26`).

## CLI adapters

`src/main.rs` adds an explicit `checkpoint` command family after its current owner releases the file. Naming may be adjusted for Clap consistency during Task 3 review; required semantics are (`docs/parity/ACCEPTANCE.md:46`, `docs/parity/ACCEPTANCE.md:61`):

- `checkpoint list`: read-only JSON/text listing of sessions and checkpoints, including baseline/current/validity state;
- `checkpoint show <id>`: read-only manifest summary and changes observed across that component boundary;
- `checkpoint diff <from> [<to>]`: read-only added/modified/deleted paths and hashes for both roots;
- `checkpoint verify [<id>] [--live]`: read-only metadata/object verification, with nonzero exit on corruption;
- `checkpoint reconcile <session>`: explicit mutating metadata/receipt recovery; prints the proposed previous/candidate resolution before applying unless a reviewed noninteractive confirmation flag is supplied;
- `checkpoint restore <id>`: explicit mutating restore after preflight, with progress on stderr or an explicit NDJSON mode;
- `checkpoint resume [<id>|--latest] <guide>`: explicit mutating restore/reconcile plus shared ordered suffix install;
- `checkpoint delete-session <session>`: explicit confirmed deletion of a non-active session; refuses pending/install/restore state and does not implicitly run GC;
- `checkpoint gc`: explicit locked maintenance, refused while any install/restore transaction is active.

Machine output has one schema per stdout stream. Errors and exit codes distinguish invalid arguments, corrupt checkpoint state, root/guide mismatch, manual restore required, and install failure. No command reports success until post-operation verification passes (`docs/parity/ACCEPTANCE.md:61`, `src/recovery.rs:249-302`).

Direct CLI probes use copied production binaries, disposable roots, exact argv/exit/stdout/stderr capture, source/binary hashes, and before/after manifests. They do not use a Rust test binary, inline test, temporary Rust program, or user game/mod tree (`docs/parity/install-progress-plan.md:202-247`, `docs/parity/ACCEPTANCE.md:20-26`).

## Desktop checkpoint browser

Create `src/ui/checkpoint_browser.rs`; `src/ui.rs` owns only route wiring, worker messages, and run IDs after its current owners release it. The browser calls the same inspect/list/diff/verify/reconcile/restore/resume/delete-session library APIs as the CLI and never reads the store directly. GC remains an explicit CLI maintenance operation rather than a browser action (`docs/parity/checkpoints-csharp-audit.md:78-100`, `docs/parity/ACCEPTANCE.md:46`).

Match the useful C# browser behavior:

- show session and current checkpoint, ordered checkpoint list, component name/GUID, timestamp, sequence/total, full/short ID, and validity (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:57-89,356-372` in the pinned oracle audit);
- show selected-to-current diff grouped by added/modified/deleted, for both mod and game roots, with truthful counts and bounded initial rows (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:115-255`);
- disable restoring the current checkpoint, require confirmation naming data loss and the exact target, expose refresh/close/busy/error states, and reload after success (`src/ModSync.GUI/Dialogs/GitCheckpointBrowserDialog.axaml.cs:269-352`);
- provide `Verify`, `Restore`, and, only for an interrupted compatible session, `Resume from here`; show a separate explicit `Reconcile metadata` action for `NeedsReconciliation`, and a confirmed `Delete session` action only for inactive history; each action uses a typed preflight result and exact current run/root identity (`docs/parity/ACCEPTANCE.md:46`).

Strengthen the reference where its facts are unsafe:

- opening or refreshing the browser never creates a baseline or mutates the store (`docs/parity/checkpoints-csharp-audit.md:78-86`);
- renamed or case-conflicting paths cannot disappear from the diff; unsupported states render explicit errors (`docs/parity/checkpoints-csharp-audit.md:78-86`);
- restore updates the authoritative session/prefix receipt through the library instead of only copying game files (`docs/parity/checkpoints-csharp-audit.md:78-86`, `docs/parity/checkpoints-csharp-audit.md:88-100`);
- the main menu and completion page open the same Rust browser/backend, not two dialogs with conflicting claims (`docs/parity/checkpoints-csharp-audit.md:78-86`).

The UI may keep history after a normal completed install and surface checkpoint storage size, but session deletion requires a separate confirmation and is not bundled into restore. Garbage collection remains the separate explicit CLI command. Browser presentation never calls an invalid session resumable merely because JSON parses (`docs/parity/checkpoints-csharp-audit.md:64-76`, `docs/parity/checkpoints-csharp-audit.md:102-110`, `docs/parity/ACCEPTANCE.md:46`).

## File ownership and serialized task sequence

No two tasks below edit the same file concurrently. Each task begins from the approved commit of its dependencies and stops if an active owner has not released a required file (`docs/parity/install-progress-plan.md:189-200`, `docs/parity/download-orchestration-plan.md:355-368`).

| Task | Required ownership | Responsibility |
|---|---|---|
| 0: active-lane handoff | no edits | Wait for current owners of `src/recovery.rs`, `src/main.rs`, and `src/ui.rs`; re-pin line references and reconcile their delivered APIs before checkpoint source work (`docs/parity/checkpoints-plan.md:5-26`) |
| 1: immutable core | `src/checkpoints.rs` (new), optional `src/checkpoints/*.rs` (new), `src/lib.rs` | Versioned CAS, canonical manifests, session/transaction records, read-only inspect/list/diff/verify, typed boundary deltas, no frontend or engine behavior (`docs/parity/ACCEPTANCE.md:46`) |
| 2: engine/recovery integration | `src/engine.rs`, `src/recovery.rs`, `src/install_progress.rs`, checkpoint modules from Task 1 | Mandatory baseline/per-component commit, receipt v2, reconciliation, restore/resume, progress facts, preserve whole-run recovery and old wrappers (`src/engine.rs:786-995`, `src/recovery.rs:147-302`) |
| 3: CLI | `src/main.rs` only after its acquisition/selection owners release it | Checkpoint list/show/diff/verify/reconcile/restore/resume/delete-session/gc adapters and explicit machine schemas; no business-rule duplication (`docs/parity/ACCEPTANCE.md:61`) |
| 4: desktop | `src/ui.rs` only after current UI owners release it; `src/ui/checkpoint_browser.rs` (new); `src/ui/installing_page.rs` only if the approved progress renderer cannot consume the additive fact | Shared browser, run-bound worker messages, verify/reconcile/restore/resume/delete-session actions and confirmations, factual Installing-page checkpoint state; no desktop GC (`docs/parity/install-progress-plan.md:155-185`) |
| 5: evidence and docs | `docs/parity/checkpoints-native/` (new), `docs/parity/ACCEPTANCE.md`, `docs/parity/GUI-CLI.md`, `docs/parity/GUI-INVENTORY.md`, `COMPATIBILITY.md` | Direct production probes, source/binary/fixture binding, limitations, C# differences, final status; no source edits (`docs/parity/ACCEPTANCE.md:28-32`) |

`Cargo.toml` and `Cargo.lock` are not owned by default. The design uses existing SHA-256, serde, tempfile, filesystem, and locking capabilities already present in recovery (`src/recovery.rs:1-12`, `src/recovery.rs:295-327`). Any new dependency requires a separate ownership expansion and review showing why the standard library/existing crates cannot meet the contract.

Task 2 cannot begin until Task 1's on-disk contract and corruption behavior pass spec review. Tasks 3 and 4 begin only after Task 2 source review approves the shared API. Task 5 starts only after CLI and desktop owners have delivered their exact commits; one final reviewer reads the combined diff and evidence (`docs/parity/download-orchestration-plan.md:427-453`).

## Direct acceptance probes without Rust tests

All probes use newly created disposable mod/game/state/config roots outside the repository and user trees. Each records the pinned source SHA, production binary SHA-256, fixture hashes, platform/filesystem, exact argv, exit status, stdout/stderr, checkpoint metadata, and before/after tree manifests under `docs/parity/checkpoints-native/` (`docs/parity/ACCEPTANCE.md:46`, `docs/parity/install-progress-plan.md:202-247`).

1. **Read-only boundary.** Run list/show/diff/verify and desktop browser open/refresh against an absent store and an existing store. Hash all roots/store files before and after. The absent store remains absent; existing bytes remain identical (`docs/parity/checkpoints-csharp-audit.md:78-86`).
2. **Two real component commits.** Install two ordered Copy/Move/Delete components that change both roots. Verify a unique baseline plus two component manifests, exact parent chain, component boundary metadata, object hashes, before/after boundary deltas, prefix receipt/head agreement, and `ComponentCompleted` only after each committed checkpoint; do not promote the deltas into causal writer attribution (`src/engine.rs:955-995`, `src/recovery.rs:147-205`).
3. **No-change component.** Run a condition-skipped/no-byte-change component with at least one executable authored instruction. Verify a unique checkpoint/head and prefix advance with zero changed paths; it must not reuse its parent ID (`src/engine.rs:969-992`, `docs/parity/checkpoints-csharp-audit.md:30-36`).
4. **Restore every boundary.** From final state, restore baseline, checkpoint 1, and checkpoint 2 in separate fresh copies. Verify exact mod/game manifests, receipt prefix, selected head, no stray files, and later history still listable (`src/recovery.rs:249-293`, `docs/parity/ACCEPTANCE.md:46`).
5. **Resume after committed boundary.** Observe the typed committed checkpoint for component 1 in an owned production CLI process, terminate only that owned process, restart, verify discovery, resume latest, and prove component 1 is not replayed while component 2 runs in guide order. Bind termination timing to captured NDJSON and store/receipt bytes; a process exit alone is not boundary proof (`src/install_progress.rs:31-53`, `src/engine.rs:841-931`). Repeat after restoring an earlier checkpoint while later history remains: `--latest` must select the authoritative active-branch head, while the abandoned later checkpoint requires its explicit ID.
6. **Crash during next component.** Make component 2 perform a bounded large streamed copy in disposable roots, terminate the owned process after live file growth begins, then resume. Verify the partial component-2 bytes are discarded by restoring checkpoint 1 before suffix execution; component 1 remains exact and component 2 runs once from its authored start (`src/recovery.rs:249-293`, `src/engine.rs:955-995`).
7. **Publication reconciliation.** Exercise each naturally observable transaction state produced by process termination around object/manifest/head/receipt publication. If no production-observable timing can hit a state without adding hooks, mark that state unproven; do not add Rust test hooks or claim crash recovery from source review (`docs/parity/ACCEPTANCE.md:20-26`, `docs/parity/ACCEPTANCE.md:46`). Power-loss durability remains explicitly unproven without supported-filesystem directory-sync review and VM/filesystem power-cut evidence.
8. **Corruption matrix.** In separate copied stores, corrupt one object, manifest, session head, transaction, and completion receipt. Read-only verify and both frontends must identify the exact class, refuse restore/resume, and leave every byte unchanged (`src/recovery.rs:180-205`, `docs/parity/ACCEPTANCE.md:46`).
9. **Restore interruption.** Use a large disposable fixture and terminate only the owned restore process once in every observable `restore.json` phase after guard publication. Restart must report the exact phase, offer only roll-forward or guard rollback, block other mutations, and reach either exact pre-restore roots/receipt/head or exact target roots/receipt/head with no mixed success claim (`src/recovery.rs:208-293`).
10. **Definition/order/root mismatch.** Change a completed component definition, reorder IDs, point at a copied different root, duplicate an ID, and request a non-prefix resume. Every case fails before install writes and preserves the trees/store (`src/recovery.rs:180-205`, `src/engine.rs:910-931`).
11. **CAS deduplication, deletion, and GC.** Two checkpoints with identical content reuse one verified object. Session deletion refuses active/pending state and leaves objects intact. GC while active/pending is refused. After an explicitly deleted disposable session, a separate GC removes only unreachable objects and remaining checkpoints still verify/restore (`docs/parity/ACCEPTANCE.md:46`).
12. **CLI schema.** Capture text and JSON for list/show/diff/verify plus progress/exit behavior for restore/resume. Prove legacy install `--json` output is byte-identical for the same fixture and typed install progress uses its documented version (`docs/parity/install-progress-plan.md:141-153`, `docs/parity/ACCEPTANCE.md:61`).
13. **Native browser matrix.** At 1280×800 and 600×400, capture no-store, valid history, selected details/diff, invalid checkpoint, restore confirmation/cancel/success, interrupted-session resume, busy/close/refresh, current-target disabled, and post-action bytes. Exercise keyboard focus and scroll; record untested accessibility/platform states explicitly (`docs/parity/checkpoints-csharp-audit.md:78-86`, `docs/parity/ACCEPTANCE.md:59-60`).
14. **C# comparison.** Using the pinned existing reference binary only if its hashes and disposable configuration are re-established, capture a small Git checkpoint list/diff/restore flow and the main-menu disabled immutable restore behavior. Compare user-visible fields and record deliberate Rust safety/storage differences; do not rebuild C# or use a user game (`docs/parity/checkpoints-csharp-audit.md:78-86`).
15. **Completion compatibility and phase rollback.** Execute the existing Base/Remaining production CLI and native phase flows against v2 receipts. For both a prior active session and no prior session, fail a child phase after at least one checkpoint, invoke whole-run restore, and prove exact prior mod/game manifests, receipt bytes, `active.json` presence/bytes, session status, head, branch generation, and completed prefix; the failed child remains retained as `Aborted` and inactive. Also verify suffix no-replay, terminal `Completed` remains after final receipt and `recovery::finish`, and Finish/reload does not delete checkpoint history (`src/engine.rs:793-862`, `src/recovery.rs:208-293`, `docs/parity/ACCEPTANCE.md:268-280`).

Probe scripts, if retained, are shell/native interaction drivers only. They never compile or invoke Rust test targets, link to Rust internals, or write fixture-only behavior into production source (`docs/parity/ACCEPTANCE.md:20-26`).

## Production gates after implementation

During the test freeze, source gates are limited to formatting plus production targets. Run `cargo fmt --all -- --check`; run Clippy/build on the host with a dedicated target directory, `-j 2`, and only production library/binary/desktop targets. Do not use `--all-targets`, because frozen test source is not part of this slice's proof (`docs/parity/ACCEPTANCE.md:20-26`, `docs/parity/install-progress-integration.md:41-43`).

The project-wide `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, and `cargo test --no-default-features` gates remain deferred until the user lifts the full-parity test freeze. A production build, format check, direct probe, source presence, or receipt cannot be promoted into those missing gates (`docs/parity/ACCEPTANCE.md:28-32`).

No build or gate is run while host disk saturation remains unresolved. Before a future build, the owner records free space, target-directory ownership, and active heavy I/O; it does not delete another worker's target or use the repository's shared target directory (`docs/parity/install-progress-integration.md:41-43`).

## Review gates

### Task 1: storage and API specification

Review canonical bytes, object no-clobber publication, symlink/special/case-collision refusal, both-root coverage, unique no-change checkpoints, parent-chain validation, component-boundary-delta wording, read-only open behavior, schema-version refusal, and GC reachability (`src/recovery.rs:90-145`, `src/recovery.rs:295-327`, `docs/parity/ACCEPTANCE.md:46`).

Quality review checks bounded memory, streaming hash/copy, directory traversal cost, no string-parsed business facts, no direct JSON access from frontends, and no new dependency without an approved ownership expansion (`src/recovery.rs:83-145`, `src/recovery.rs:295-327`).

### Task 2: engine, receipt, restore, and resume

Review that `recovery::begin` still precedes all install writes and captures prior checkpoint-control bytes; every phase uses a child session; checkpoint failure stops the run; `ComponentCompleted` follows process-crash-committed convergence; exact prefix/order/definitions/roots are verified; resume restores before skipping; both roots are restored; whole-run rollback restores roots, receipt, active pointer, parent session/head/generation/prefix and retains the failed child as inactive; old pending state is closed only after target verification; and `Completed` remains after final receipt plus `recovery::finish` (`src/engine.rs:793-838`, `src/engine.rs:910-995`, `src/recovery.rs:180-293`).

A reliability reviewer traces every interruption between object publish, manifest, transaction, session head, receipt, restore journal, and pending-marker changes. Any state without a deterministic recovery classification remains a blocker, not a documentation caveat (`docs/parity/ACCEPTANCE.md:46`).

### Task 3: CLI contract

Review list/show/diff/verify read-only behavior; reconcile/restore/resume/delete-session/GC mutability; JSON schema isolation; stdout/stderr; exit codes; noninteractive confirmations/force rules; lock conflicts; root/guide binding; GC reachability; and legacy install JSON stability (`docs/parity/ACCEPTANCE.md:61`, `docs/parity/install-progress-plan.md:141-153`).

### Task 4: desktop contract

Review one shared browser/backend, no mutation on open/refresh, run-bound messages, stale-message rejection, explicit reconciliation, current-target disablement, restore/resume/delete-session confirmations, deletion refusal for active/pending state, absence of desktop GC, responsive scrolling, accessible labels/focus, factual validity/resume state, and no UI-derived prefix or causal attribution (`docs/parity/checkpoints-csharp-audit.md:78-100`, `docs/parity/ACCEPTANCE.md:59-60`).

### Task 5: whole-package evidence

One reviewer who did not implement Tasks 1-4 reads the combined diff, all direct-probe receipts, source/binary bindings, and documentation. The reviewer explicitly separates source review, production build, direct CLI proof, native interaction, crash/restart proof, C# comparison, and full-build/game proof (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:64`).

Frozen existing Rust tests may fail to compile after enum/struct expansion. Record exact drift without editing, hiding, or compiling custom replacement tests; full project test adaptation waits for the user's parity boundary (`docs/parity/ACCEPTANCE.md:20-26`).

## Open questions with defaults

1. **Exact Git interoperability.** Must Rust expose a LibGit2-compatible repository in addition to immutable parity behavior? Default: no. Ship the authoritative immutable backend and call it immutable checkpoints; keep exact Git import/export missing in `COMPATIBILITY.md` until explicitly required (`docs/parity/checkpoints-csharp-audit.md:28-76`, `docs/parity/checkpoints-csharp-audit.md:88-100`).
2. **C# store import.** Should Rust browse/restore existing `.modsync/checkpoints` Git or immutable C# history? Default: read-only discovery reports “unsupported foreign store” and never mutates it. A separately planned importer requires executable C# fixtures for both formats (`docs/parity/checkpoints-csharp-audit.md:5-28`).
3. **Complete objects versus binary deltas.** Default: complete SHA-256 objects in v1. Add deltas only after real corpus measurement shows storage pressure and restore/corruption probes cover both directions (`src/ModSync.Core/Services/ImmutableCheckpoint/BinaryDiffService.cs:54-121` as audited in `docs/parity/checkpoints-csharp-audit.md`).
4. **Source-tree drift after a completed install.** Default: browser/list/verify reports drift without invalidating historical game restore; resume always restores both roots to the selected checkpoint before suffix execution (`src/recovery.rs:180-205`, `src/recovery.rs:249-293`).
5. **Restore guard retention.** Default: retain the pre-restore guard until the next verified install/checkpoint operation, and expose its storage cost. Never delete it in the same commit that first proves target restore; there is no standalone guard-cleanup action in this slice (`src/recovery.rs:249-293`).
6. **History branching after restore.** Default: immutable manifests remain a DAG, but a resumed installation advances a new active branch from the selected checkpoint. The browser shows later old checkpoints as non-current history; GC treats both retained branches as reachable (`docs/parity/ACCEPTANCE.md:46`).
7. **Timestamp authority.** Default: timestamps are display metadata, never checkpoint identity or `--latest` authority. Parent links and branch generation establish ancestry/current history; the session's authoritative active-branch head defines `--latest`; the canonical manifest hash establishes identity (`docs/parity/checkpoints-csharp-audit.md:30-36`, `docs/parity/checkpoints-csharp-audit.md:56-62`).
8. **Typed install-progress version.** Default: add an explicit version-2 typed envelope and document version 1 as superseded; legacy `--json` remains unchanged. If review finds a real external v1 consumer, retain a selectable v1 adapter (`docs/parity/install-progress-plan.md:141-153`).
9. **Automatic resume at desktop startup.** Default: detect and present choices, never auto-resume. The user chooses whole-run restore, resume latest, browse earlier checkpoints, or leave blocked (`src/recovery.rs:208-293`).
10. **Automatic GC.** Default: none. GC is explicit after session deletion and refuses active/pending/restore states; storage pressure is visible but never silently trades away restore points (`docs/parity/ACCEPTANCE.md:46`).
11. **Causal file provenance.** Default: unresolved and therefore an open F11 gap until the shared engine has complete write attribution for built-in actions, external patchers, and concurrent writers. The first slice ships only before/after changes observed across a named component boundary, never a claim that the component caused each change (`src/install_progress.rs:21-53`, `docs/parity/checkpoints-csharp-audit.md:56-62`, `docs/parity/checkpoints-csharp-audit.md:88-100`).
12. **Retention after successful completion.** Default: retain baseline and all component checkpoints until explicit user deletion. `Finish` clears presentation state only and does not remove recovery/checkpoint history (`docs/parity/ACCEPTANCE.md:280`).

## Completion boundary

This plan is complete only when all of the following are evidenced; no smaller subset closes F11 (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:46`):

- every successful top-level component has a unique, verified immutable checkpoint covering both roots and truthful component-boundary deltas; causal file provenance remains explicitly unproven until a write journal supplies stronger evidence;
- component success cannot outrun checkpoint/head/receipt convergence, and any failure stops later guide work with recovery pending;
- baseline and every component checkpoint restore exact mod/game bytes through shared library, CLI, and desktop paths;
- restart discovery and resume restore a verified checkpoint before executing only its exact ordered suffix;
- completion receipt v2 preserves existing Base/Remaining no-replay, and whole-run rollback restores exact roots, receipt, active pointer, parent session/head/generation/prefix while retaining the failed child as inactive history;
- list/show/diff/verify are demonstrably read-only, and validation never repairs, restores, initializes state, or runs external programs;
- corruption and interrupted publication/restore states fail closed with deterministic recovery choices;
- the native browser provides truthful current/history/diff/verify/restore/resume state at normal and minimum window sizes;
- production source/build/static gates and direct CLI/native/crash probes are bound to exact commits, binaries, fixtures, and manifests;
- `COMPATIBILITY.md`, `docs/parity/ACCEPTANCE.md`, `docs/parity/GUI-CLI.md`, and `docs/parity/GUI-INVENTORY.md` state exact proven scope and remaining C# Git/import, platform, accessibility, full-build, and game gaps.

F29 remains unverified until isolated K1/K2 reference and Rust installs with identical inputs/selections establish per-step/final bytes, patch semantics, GUI/CLI equivalence, and game smoke evidence (`docs/parity/ACCEPTANCE.md:64`).

## Plan claim blocks

claim: The plan preserves Rust's existing whole-run snapshot while making a per-component immutable checkpoint mandatory before component completion.
referent: `src/engine.rs:793-838,955-995`; `src/recovery.rs:208-293`; this plan's R2 and R3.
checked: source-derived design against Rust `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`.
verdict: holds
confidence: high; implementation review could change the API shape but not this required ordering.

claim: Restore/resume must cover both mod and game roots.
referent: `src/recovery.rs:134-145,217-293`; `docs/parity/ACCEPTANCE.md:46`; this plan's R5.
checked: current recovery coverage and F11 behavior trace.
verdict: holds
confidence: high; proving that no install action can mutate the mod tree would be required to narrow it.

claim: A native immutable CAS backend can satisfy the observable F11 checkpoint contract without claiming Git interoperability.
referent: `docs/parity/ACCEPTANCE.md:46`; `docs/parity/checkpoints-csharp-audit.md:88-110`; this plan's R1 and on-disk contract.
checked: requirements-to-design trace; not runtime-checkable in this documentation task.
verdict: holds
confidence: medium; an explicit acceptance requirement for byte-compatible C# Git stores would change the default and is retained as open question 1.

claim: Existing completion-prefix verification is the compatibility spine for checkpoint resume.
referent: `src/recovery.rs:180-205`; `src/engine.rs:841-931`; this plan's completion receipt v2 and resume lifecycle.
checked: direct source trace.
verdict: holds
confidence: high; implementation must preserve definition/root/live-byte checks and add head/object verification.

claim: No Rust test or harness execution is required to implement and directly prove this slice under the current freeze.
referent: `docs/parity/ACCEPTANCE.md:20-26`; this plan's direct acceptance probes and production gates.
checked: user constraint and existing house-style direct-probe contract.
verdict: holds
confidence: high for the current session constraint; it changes only when the user lifts the freeze.
