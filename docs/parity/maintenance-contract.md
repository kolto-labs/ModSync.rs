# Explicit directory maintenance transactions

This document describes production source behavior. Rust tests, custom harnesses, runtime probes and CI runs remain frozen. The implementation has received independent source review; native platform, interruption, power loss and external process acceptance remain unverified.

Maintenance is separate from the installation engine and checkpoint component history. It never creates a fabricated guide component, updates a completed install prefix, installs archives, or resumes a forward operation during recovery.

## API and review authority

`modsync::maintenance` exposes:

```rust
Request {
    roots: paths::Roots,
    target: TreeRoot::Game | TreeRoot::Mods,
    subtree: String,
    action: Action::LowercaseNames | Action::OwnerWritable,
}

preview(&Request, &AtomicBool) -> anyhow::Result<Preview>
apply(&Request, expected_digest: &str, &AtomicBool) -> anyhow::Result<Outcome>
status(&Roots) -> anyhow::Result<Status>
recover(&Roots, expected_digest: &str, &AtomicBool) -> anyhow::Result<Outcome>
```

Request, action, root selector and operation types support serialization. Request and preview are cloneable. `TreeRoot` reuses the checkpoint selector enum, without reusing checkpoint install authority.

Preview is read only: it creates no state or lock files. It reports `digest`, the complete `operations` list, `entry_count`, `operation_count`, and an explicit `capability`. Apply recomputes the plan under the shared game/mod root locks and requires the reviewed digest to match. The digest binds canonical root spelling, both root objects' device/inode/owner/group identities, target and subtree, action, exact inventory paths, file contents and identities, full Unix mode bits, deterministic operations, and platform capability.

Status reports `pending`, `digest`, `operation_count`, and `phase`. A malformed or foreign artifact is an error, never an idle result. An unsupported platform can report an idle state without claiming mutation capability. Existing foreign-platform intents remain fenced.

Outcome reports the digest, completed-operation count, whether explicit recovery was performed, a bounded optional failure, and one of `Applied`, `CancelledAndRolledBack`, `FailedAndRolledBack`, or `RecoveryRequired`. Successful explicit recovery uses `Applied` with `recovered = true` and zero completed forward operations: the recovery transaction has restored the original state. A post-intent error does not silently escape as an unclassified failure. Errors before publication and before any tree operation may return `Err`.

The caller supplies the reviewed digest to apply and the selected durable operation digest to recover. It cannot supply journal contents as authority. On load the reader rejects unknown fields, duplicate inventory keys, malformed paths and hashes, unknown capabilities, invalid phases/cursors, and operations that do not derive exactly from the immutable request/inventory.

## Operations and platform admission

| Behavior | Qualified source behavior |
| --- | --- |
| Lowercase names | ASCII descendant file and directory names only; the selected subtree's own name is excluded. Sibling destinations are checked before publication. Files and directories are never merged or overwritten. |
| Owner writable | Selected directory and descendants retain all existing `07777` bits; files receive `mode \| 0600`, directories receive `mode \| 0700`. No ownership takeover or chmod 777. |
| Linux rename | Native `renameat2(RENAME_NOREPLACE)` using an opened, identity-checked parent directory. Every rename goes through its exclusive intermediate name, including case-only changes. |
| Linux permission change | Ordinary owned object opened with no-follow; identity checked on the handle, permissions changed through the handle, and exact resulting bits verified. |
| Other platforms | Windows ACL/identity-preserving readonly attribute operations and other Unix rename/ACL profiles are unsupported. No generic `set_readonly(false)` substitute is used. |

Objects must already be readable and directories searchable to produce a trustworthy read-only preview. An initially unreadable `000` tree is refused; preview never repairs permissions to inspect it.

The implementation rejects symlinks and special files at every existing ancestor and leaf, file hardlinks, foreign ownership, extended POSIX ACLs, Samba security ACLs, and file `security.capability` metadata. SGID objects require membership in the preserved group. These restrictions prevent permission changes from discarding security state the transaction could not restore. Full ACL, arbitrary xattr, ownership or .NET Unicode mapping equivalence is not claimed.

Roots must be absolute, canonical, ordinary directories, must not be filesystem roots, and must not overlap each other or recovery storage. Inventory paths must round-trip through portable path admission without changing the actual relative filesystem path. Literal Unix backslashes, reserved Windows names and unsafe aliases are refused. The same rules constrain operations loaded from disk.

Operations run by descending path depth. Rollback reverses this order, restoring ancestor spelling before descendant rollback. The projected expected inventory updates only a renamed key and its prefix range; unrelated entries are not rescanned for every rename. Projection observes cancellation and work bounds.

## Durable intent and recovery

The dedicated `maintenance-v1` directory is under the existing game recovery state, outside either tree. It holds `plan.json` and `journal.jsonl`. The immutable plan body is separate from the append-only progress stream. Both roots use the existing recovery lock names, so cooperative installers/checkpoint writers cannot interleave with maintenance.

The initial intent is written and flushed in a private temporary file outside the pending directory, then published atomically with native no-replace rename. Both source and destination namespaces are flushed. The published bytes are checked before a tree operation begins. A crash before publication exposes no partial intent and authorizes no tree mutation. Private scratch artifacts outside the pending directory are not recovery authority; normal scope cleanup removes the caller's own temporary artifact, and orphan private files from an abrupt process death are not used to authorize mutations.

Each rename leg and each permission change receives a flushed progress intent before mutation. Rename endpoints are exclusive. Parent/object identities, file hash/size and modes are checked; file and namespace changes receive the relevant durability boundaries. Recovery reconciles a single interrupted intent from exact endpoint spelling, object identity and modes, then verifies the complete projected subtree before normalizing progress. An incomplete append suffix is discarded only after this object-state proof.

Before cleanup, a terminal cursor is atomically published in the intent header and flushed. Removing progress therefore leaves a recoverable original/final cursor. If recovery must recreate progress after a terminal cleanup interruption, a durable progress origin preserves that cursor before new rollback records are written. Forward recovery is never implicit: explicit recovery restores the verified original tree.

Cancellation triggers automatic reverse rollback using a separate bounded cleanup budget that ignores the triggering cancellation. A whole-tree original-state proof is required before claiming rollback succeeded. Foreign edits, lost objects, mode/content changes, exhausted cleanup budgets and persistence failures retain an explicit recovery outcome.

If the final intent unlink cannot be flushed, cleanup attempts to republish its terminal intent. If the filesystem also refuses republication, status and ordinary mutation guards require a successful flush of the existing empty maintenance namespace before treating it as idle. No failed flush is reported as confirmed durable cleanup.

## Scope of mutation fences

Pending or foreign maintenance metadata fences `recovery::ensure_clean`, ordinary `Lock::acquire` before and after root lock acquisition, the game-scoped checkpoint/deployment guard, and checkpoint `Store::prepare_write`. Capture, restore, resume and lifecycle mutation paths that require that checkpoint capability check inherit the fence. Maintenance recovery uses a private lock path that qualifies checkpoint capabilities without waiving its own intent; checkpoint/external-process and no-follow legacy pending checks repeat under the held locks.

These fences cover the cooperating install, checkpoint and deployment writers that participate in those guards. Cross-process acquisition, downloads, watcher publication, peer publication and arbitrary external editing are not claimed to participate in this transaction. Callers must quiesce their owned writers; concurrent outside-app acquisition/editing is unsupported. Final state comparison and per-operation object checks refuse detected interference, without claiming filesystem-wide isolation from hostile namespace races.

## Bounds and evidence

The target inventory is limited to 100,000 objects, depth 64 and 16 MiB of path bytes. Serialized documents use a bounded writer rather than allocating an unlimited document and checking afterward. The combined plan/progress budget is 64 MiB, and preview reserves recovery progress space. This can impose a lower operation count than the object inventory limit.

Each inventory/transaction work budget permits at most 128 GiB of aggregate hashed bytes, two million work/namespace visits and ten minutes. Hash loops and projection observe cancellation. Automatic cleanup has its own budget and cannot become an unbounded retry loop. Exceeding a recovery bound preserves the pending state instead of declaring success.

Source validation consists of formatting, whitespace checks, inspection of native and journaling paths, and independent review of crash windows, projection complexity and mutation fences. These are source checks. They do not establish native Windows/macOS support, runtime permission behavior, interruption recovery, power-loss durability or live-tree acceptance.
