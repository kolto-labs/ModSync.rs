# Windows checkpoint process-interruption capability

Status: source-reviewed implementation contract, with the mandatory refinements below. The production non-Unix writer rejection remains in place until the full library slice is reviewed. No native probes, builds, tests, or checkpoint mutations were performed to produce this proposal.

## Decision and scope

Add a Windows capability for recovery after termination of the ModSync process while Windows and the filesystem remain running. Retain the existing Unix file-and-directory-fsync protocol unchanged. The Windows capability does not cover OS crashes, power loss, storage disconnects, volatile device caches, or universal filesystem durability. A successful file flush or rename must never be presented as proof of those stronger properties.

Initially qualify ordinary local NTFS volumes only. Reject remote/UNC, network mapped volumes, FAT/exFAT, ReFS, unknown filesystem types, and unsupported providers before beginning a new transaction. Qualify the checkpoint store, game root, mod root, exact auxiliary participants, and their existing creation ancestors. Roots may reside on separate qualified volumes: each publication must have its own temporary sibling on the destination volume. There is no cross-volume atomic transaction; the existing intent/recovery protocol coordinates sequential mutations.

Ordinary-user operation must not obtain administrator privileges, open a raw volume, enable backup/restore privileges, or silently downgrade after a filesystem API failure. ACL denial and sharing violations remain explicit operation failures with recovery fences retained.

## Persistent capability without frozen struct changes

Existing metadata can express this capability without changing public struct fields or frozen helpers:

- `Session.origin` and every `Checkpoint.origin`: retain `rust` for existing Unix captures; use `rust-windows-process-interruption-v1` for new Windows captures.
- `CaptureReport.durability`: retain the existing Unix string; use `windows-process-interruption-v1; flushed-file-data and same-volume namespace operations; no OS-crash or power-loss guarantee; native acceptance pending` on Windows.
- Do not repurpose HEAD status, component definitions, branch kind, or install-report metadata. They already have distinct protocol meanings.
- Centralize origin decoding. `read_session`, `read_checkpoint`, checked ancestry, lifecycle incomplete-session retention, and orphan-checkpoint parsing currently contain strict `origin == "rust"` checks. Update every such consumer to recognize the finite capability set.
- Enforce checkpoint/session origin agreement across each session, including newly committed component checkpoints and restore/resume branch baselines. Cross-capability mutation/rebase is unsupported; read-only inspection can explain an unsupported writer profile.
- Existing `rust` metadata is not silently relabeled as Windows-capable. Windows mutation refuses it. Unix mutation refuses the Windows profile. Existing old readers reject the new discriminator, which is the required compatibility behavior.
- Keep schema version and serialized field shapes unchanged. Expose an additive capability query/projection for CLI/UI history so capability remains visible after the original capture report is gone; compact existing summary structs remain unchanged.

This extends the accepted values of the origin discriminator. It does not assert that earlier consumers understand Windows sessions.

## Production platform interface

Introduce a private checkpoint persistence module with explicit operations:

1. `writer_capability` / `require_supported_locations`: return the finite platform capability and validate all participating locations before intent publication or other new-store writes.
2. `flush_file`: synchronize the original writable file handle. Do not reopen read-only and call `sync_all`; Windows FlushFileBuffers requires write access.
3. `publish_new`: create a private sibling with create-new semantics, write/flush/close it, then publish without replacement using a same-volume rename. Existing destination is an error or the existing explicit immutable-object verification path; never overwrite an immutable document.
4. `replace_file`: write/flush/close a private sibling, then replace the destination through an explicit same-volume namespace operation. Never implement replacement as delete-target followed by rename. Never enable COPY_ALLOWED, delayed reboot, or cross-volume fallback.
5. `rename_directory`: checked same-volume move for lifecycle trash transitions; preserve the bound deletion marker until payload cleanup completes.
6. `namespace_boundary`: on Unix perform the existing directory fsync. On Windows validate the directory through a no-reparse directory handle and classify completion as process-visible namespace ordering only. This is an explicit weaker operation, not a fake successful fsync. Call sites and receipts must use the capability-aware name rather than claiming a directory flush occurred.

MoveFileExW with WRITE_THROUGH (and REPLACE_EXISTING only for replacement) is the proposed Windows rename primitive, using UTF-16/verbatim canonical paths, checking every return value. Its same-volume process-interruption behavior remains a native acceptance gate. WRITE_THROUGH is additional best effort under this profile and must not expand its guarantee. Existing tempfile Windows persistence currently omits this flag; centralize the implementation rather than assuming tempfile provides the new contract.

Do not substitute ReplaceFileW casually: its WRITE_THROUGH option is unsupported, it merges destination metadata/streams, and some documented failure states alter filenames. The proposed rename primitive avoids introducing that extra recovery state machine. File data is flushed through the writer before publication; handles are closed before namespace changes to avoid self-inflicted sharing failures.

Existing dependencies expose CreateFileW, GetFileInformationByHandle, GetVolumeInformationW, GetDriveTypeW, MoveFileExW, FlushFileBuffers, and file attribute APIs. Confirm exact local windows-sys signatures and enabled feature coverage during implementation; no new dependency is presently justified.

## Preserve the journal protocol

The weaker storage capability does not weaken logical recovery gates:

- Publish and reread a bound InstallComponent/Restore/Finalize operation before authorized destructive changes.
- Retain Materializing until capture and hash verification succeed.
- Publish committed HEAD as CleanupRequired or FinalizeCleanupRequired before removing its journal.
- Remove the operation only after the committed HEAD and referenced closure have been reread successfully. Ready/Complete publication remains the final namespace transition.
- A missing operation plus a cleanup HEAD must still synthesize the same HEAD-bound pending intent. Every library, engine, standalone deployment, and legacy recovery entry point continues to fence unresolved work.
- Rename, delete, attribute, flush, validation, or capability errors never emit component completion. Leave the existing intent/cleanup HEAD available to recovery. A partially completed OS operation requires reread/reconciliation, not blind retries or success inference from an error code.
- Cancellation is honored at existing safe boundaries; once cleanup publication crosses its commit boundary, finish reconciliation or report its retained fence. Never cancel by deleting an intent.
- Preserve exact generation/root bindings, install-root identities, auxiliary authority, and full completed-prefix verification. There is no append-at-end recovery path.

## Required materializer and legacy adapter changes

- `restore.rs`: before deleting/replacing an owned read-only file, clear only its read-only attribute after the operation is Materializing. This applies to controlled temporary leftovers too. Do not change ACLs or unrelated paths. Flush and close the new file while writable; apply captured read-only state after publication and include it in final verification. An attribute failure leaves the operation pending.
- Preserve directory identity and bounded child deletion. Windows directory read-only is not Unix permission-bit protection; do not claim ACL restoration or broaden deletion authority to make an operation work.
- `recovery.rs` snapshot reconciliation: flush the writable copy before restoring permissions; remove the read-only reopen-and-sync assumption. Reconciliation must call the same capability-specific namespace boundary.
- Integrate the parent's separately reviewed recovery lock opener. Require matching root locks and no-reparse leaf/handle checks; do not duplicate or replace that owner's implementation.
- Route store, journal, finalize, transition, restore, lifecycle delete/GC/repair, and legacy receipt/snapshot publication through the platform interface. Cover directory renames, removals, and restored external file participants as well as HEAD writes.
- Current schema captures ordinary file bytes, directory membership, and read-only state on Windows. ACLs, alternate data streams, ownership, compression/encryption semantics, hardlink topology, and timestamps are not proven restored. Preserve those documented limits; do not claim byte-for-byte Windows filesystem reconstruction.

## Process ownership prerequisite

The capability assumes no concurrent writer continues changing a captured root after its owner is interrupted. Library-only checkpoint operations satisfy this through root locks and their own synchronous mutations under the existing non-hostile-path assumption. External patcher descendants require additional care: an owned kill-on-close Windows job terminates descendants when the parent exits, but job termination is not proof that descendants have already drained when another ModSync process acquires a released root lock.

Before promoting engine-wide abrupt-process recovery, audit the existing job lifetime and restart protocol. Either establish a restart-visible quiescence check for the exact previous owned process job, or explicitly limit acceptance to interruption points after external process drain. Never claim arbitrary mid-patcher crash recovery merely because the checkpoint writer works on Windows. A new process must not restore while an earlier descendant is still writing.

## Review and acceptance gates

Implementation may be source-reviewed without executing Windows programs. The library writer gate may be released after the complete library capability and consumers are reviewed. Independently retain the Windows engine checkpoint/resume/managed execution source gate until persistent external-process fencing is implemented and reviewed. Enabling either source branch does not itself establish native acceptance.

Separately authorized native acceptance must cover each publication/interruption boundary, same-volume rename collisions/replacement, open destination handles, cancellation, read-only files, long/non-ASCII paths, reparse points, active/cleanup missing-journal reconstruction, participant roots on multiple local NTFS volumes, and full restore/resume prefix fidelity. Unsupported volumes must reject before new transaction mutation. Native crash tests must distinguish ModSync process termination from OS/power failure, which remains outside this capability.

## Official source basis

- [Directory handles](https://learn.microsoft.com/en-us/windows/win32/fileio/obtaining-a-handle-to-a-directory): BACKUP_SEMANTICS is required for an existing-directory handle; this is not a documented directory-fsync guarantee.
- [FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers): requires GENERIC_WRITE; the documented volume-wide route requires administrative privileges.
- [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw): defines replacement, same-drive directory moves, and write-through/copy behavior. No cross-volume copy fallback is accepted here.
- [ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew): write-through is unsupported; documented partial failures and metadata merging require care.

These API contracts support a bounded implementation proposal, not a claim of native Windows or power-loss acceptance.

## Mandatory independent-review refinements

- Volume/location qualification must precede `recovery::Lock::acquire`, which
  creates state storage; repeat qualification under the acquired lock.
- Readable origin is separate from mutation capability. Every write path, including
  GC, deletion, repair, incomplete-session and trash catalogs, must reject a
  foreign-capability origin before mutation.
- Preserve `TransitionCleanupRequired` and embedded transition intent alongside
  component and finalization cleanup states.
- Windows engine checkpoints/managed execution remain source-rejected before
  mutation until restart-visible external-process quiescence exists. A narrower
  acceptance claim alone cannot guard an actual restart from live descendants.
  The initial enabled slice is the complete library checkpoint lifecycle.

## Current source reconciliation

The persistence and process-fencing protocols above are now implemented and separately reviewed, including central mutation guards, selected-session recovery and engine/CLI/desktop integration. The temporary Windows engine source gate has been removed. Native Windows acceptance remains open; see `windows-checkpoint-job-quiescence-plan.md` and production compilation receipts for the exact boundaries.
