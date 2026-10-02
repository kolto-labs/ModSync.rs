# Windows checkpoint library source implementation

The checkpoint library now has a separate Windows process-interruption writer
profile. This is source implementation, not native acceptance. No Windows
programs, checkpoint mutations, tests, harnesses, or CI were run for this slice.

## Capability and admission

Existing Unix sessions retain `origin: rust` and directory-fsync ordering. New
Windows sessions/checkpoints use `rust-windows-process-interruption-v1`. Readers
recognize only these two origins and require checkpoint/session and branch-parent
agreement. Mutations reject every incompatible origin in retained, incomplete,
orphan-checkpoint, and trash metadata before touching the store. The qualification
inventory excludes CAS object trees because those bytes contain no location or
capability authority; existing object validation remains mandatory in its normal
capture/verify/repair/GC paths.

`Store::prepare_write` is read-only. Capture and CLI/desktop mutation adapters call
it before acquiring root locks; the common recovery lock also calls it for an
existing checkpoint store before creating state or lock files. Each checkpoint
mutation repeats qualification under the matching lock. Newly supplied auxiliary
participants on Windows require `Store::acquire_lock_with_context`; that method
qualifies the exact canonical participant map before lock creation and records a
private proof in the lock, then repeats location qualification under the lock.
A plain lock cannot authorize a new Windows auxiliary context.

The Windows qualifier requires absolute local fixed-drive NTFS paths, rejects
UNC/device/remote/other filesystem locations, checks existing creation ancestors,
and refuses reparse points. Participant and ancestor-shell paths must exactly match
the canonical existing ancestor plus any missing suffix; drive-prefix and short-name
aliases cannot evade overlap checks or lock-context proof binding. Legacy snapshot
roots/subroots and receipt leaves are also qualified before lock creation. Qualification does not modify permissions or request
administrator, backup, restore, or volume privileges. Every destination uses a
temporary sibling; publication cannot copy across volumes.

`Store::capability` projects origin and the explicit durability description without
changing existing history structs. CLI and desktop history JSON include this
projection. `writerProfileMatchesPlatform` describes origin compatibility only;
it does not assert that location, lock, pending-operation, or integrity gates pass.

## Persistence and recovery

Writable file handles are flushed before close and publication. Windows namespace
changes use `MoveFileExW` with `WRITE_THROUGH`, and `REPLACE_EXISTING` only for
explicit replacement. Immutable documents/objects use no-replacement publication.
Windows namespace boundaries validate an ordinary directory handle; they do not
pretend to fsync a directory. The Windows guarantee excludes OS crashes, power
loss, disconnected storage, device caches, ACL restoration, alternate streams,
ownership, timestamps, compression/encryption attributes, and hardlink topology.

Materialization clears only the read-only bit of operation-owned files when
necessary, flushes new files while writable, closes them before replacement, then
applies captured read-only state. Snapshot reconciliation uses the writable copy
handle rather than reopening a read-only file to flush. Existing cancellation,
root binding, completed-prefix, CAS verification and Component/Transition/Finalize
cleanup HEAD fences remain authoritative. Publication failures leave existing
journal or cleanup fences for explicit recovery.

Engine per-component, resume, and managed execution on Windows remain explicitly
rejected before lock acquisition or mutation. A local NTFS writer alone does not
establish restart-visible quiescence of an earlier external patcher job. Removing
that engine rejection requires separately implemented and reviewed job fencing.

## Acceptance still required

Native Windows acceptance must cover interruption points, sharing violations,
read-only files, long/non-ASCII paths, reparse rejection, same-volume rename
collisions, multiple NTFS volumes, cancellation, each cleanup HEAD reconstruction,
restore/resume prefix integrity and ordinary-user operation. This profile does not
claim native or power-loss success from compilation or source review.

## External-process fence integration

Ordinary checkpoint mutations, standalone game-scoped guards, and legacy
install/restore guards reject a present external process record before proceeding,
including when checkpoint session storage is absent. These presence guards do not
open or signal jobs. Capability preflight and lock acquisition remain separate so
explicit recovery can acquire the same root locks while a process fence exists.

`Store::external_process_status` is a read-only typed projection; on Windows its
reader validates the record's identity, roots and current component operation.
`Store::recover_with_process_quiescence` checks the selected session and operation
against that status before any signal, settles the recorded job through the
reviewed lease service, and only then calls ordinary checkpoint recovery. Its
additive report retains the quiescence outcome if checkpoint recovery subsequently
fails: `complete: false` carries `recoveryError`, without claiming restoration or
component success. Existing `recover_pending` remains fenced and cannot silently
skip process settlement. The engine execution path has now been integrated and source reviewed, and its temporary source gate removed. Native Windows acceptance remains outstanding.
