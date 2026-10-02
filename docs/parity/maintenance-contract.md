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
recover_mod_fence(mod_root: &Path, expected_digest: &str, &AtomicBool)
    -> anyhow::Result<Outcome>
```

Request, action, root selector and operation types support serialization. Request and preview are cloneable. `TreeRoot` reuses the checkpoint selector enum, without reusing checkpoint install authority.

Preview is read only: it creates no state or lock files. It reports `digest`, the complete `operations` list, `entry_count`, `operation_count`, and an explicit `capability`. Apply recomputes the plan under the shared game/mod root locks and requires the reviewed digest to match. The digest binds canonical root spelling, both root objects' device/inode/owner/group identities, target and subtree, action, exact inventory paths, file contents and identities, full Unix mode bits, deterministic operations, and platform capability.

Status reports `pending`, `digest`, `operation_count`, and `phase`. A malformed or foreign artifact is an error, never an idle result. An unsupported platform can report an idle state without claiming mutation capability. Existing foreign-platform intents remain fenced.

Outcome reports the digest, completed-operation count, whether explicit recovery was performed, a bounded optional failure, and one of `Applied`, `CancelledAndRolledBack`, `FailedAndRolledBack`, or `RecoveryRequired`. Its additive optional `disposition` distinguishes `AppliedTree` from `RestoredOriginalTree`; an unresolved outcome has no disposition. Successful explicit recovery uses `Applied` with `recovered = true`, meaning the recovery request succeeded. It may restore the original tree or finalize cleanup of an already verified applied tree, as the disposition states. A post-intent error does not silently escape as an unclassified failure. Errors before publication and before any tree operation may return `Err`.

The caller supplies the reviewed digest to apply and the selected durable operation digest to recover. It cannot supply journal contents as authority. On load the reader rejects unknown fields, duplicate inventory keys, malformed paths and hashes, unknown capabilities, invalid phases/cursors, and operations that do not derive exactly from the immutable request/inventory.

## Operations and platform admission

| Behavior | Qualified source behavior |
| --- | --- |
| Lowercase names | New previews record a Unicode 15.0 invariant simple lowercase mapper. Absent recorded mapper means legacy ASCII-only admission. The selected subtree's own name is excluded. Sibling destinations are checked before publication. Files and directories are never merged or overwritten. |
| Owner writable | Unix retains all existing `07777` bits; files receive `mode \| 0600`, directories receive `mode \| 0700`. Windows clears file readonly only and preserves directory readonly after DACL qualification. No ownership takeover or chmod 777. |
| Linux rename | Native `renameat2(RENAME_NOREPLACE)` using an opened, identity-checked parent directory. Every rename goes through its exclusive intermediate name, including case-only changes. |
| Linux permission change | Ordinary owned object opened with no-follow; identity checked on the handle, permissions changed through the handle, and exact resulting bits verified. |
| macOS profile | Native descriptor-relative exclusive rename and preserved owner modes/native flags have a separate source implementation and stable profile ID. Only `UF_NODUMP` and `UF_HIDDEN` flags are admitted. Native platform acceptance remains unverified. |
| Windows profile | Local fixed NTFS, full 128-bit identity, current effective owner and unchanged owner/group/DACL fingerprint; bounded default-stream admission, native exclusive rename and pinned readonly changes. See [the Windows contract](maintenance-windows-contract.md). Native execution remains unverified. |
| Other platforms | Other Unix rename/ACL profiles are unsupported. No generic `set_readonly(false)` substitute is used. |

Objects must already be readable and directories searchable to produce a trustworthy read-only preview. An initially unreadable `000` tree is refused; preview never repairs permissions to inspect it.

The mod root's parent must also qualify as a current-owner ordinary readable, writable and searchable directory, without unsupported ACL/security state. The independent sibling fence needs that namespace for exclusive atomic publication and flushing. Preview checks this restriction before any intent exists; access to a usable mod tree beneath a foreign-owned parent alone is insufficient. The root-only cleanup facade canonicalizes its supplied path through the acquired root lock.

The Unix implementation rejects symlinks and special files at every existing ancestor and leaf, file hardlinks, foreign ownership, extended POSIX ACLs, Samba security ACLs, and file `security.capability` metadata. SGID objects require membership in the preserved group. These restrictions prevent permission changes from discarding security state the transaction could not restore. Full ACL, arbitrary xattr or ownership equivalence is not claimed. The [Unicode contract](maintenance-unicode-contract.md) records the exact mapper, provenance, assigned-scalar admission and separate ICU/NLS proof boundary.

Roots must be absolute, canonical, ordinary directories, must not be filesystem roots, and must not overlap each other or recovery storage. Inventory paths must round-trip through portable path admission without changing the actual relative filesystem path. Literal Unix backslashes, reserved Windows names and unsafe aliases are refused. The same rules constrain operations loaded from disk.

Operations run by descending path depth. Rollback reverses this order, restoring ancestor spelling before descendant rollback. The projected expected inventory updates only a renamed key and its prefix range; unrelated entries are not rescanned for every rename. Projection observes cancellation and work bounds.

## Durable intent and recovery

The dedicated `maintenance-v1` directory is under the existing game recovery state, outside either tree. It holds `plan.json` and `journal.jsonl`. The immutable plan body is separate from the append-only progress stream. Both roots use the existing recovery lock names, so cooperative installers/checkpoint writers cannot interleave with maintenance.

A separate bounded, owned `.modsync-maintenance-{root-hash}.json` sibling record binds the canonical mod root to the game intent's roots, root identities, digest, nonce, target, subtree, action and capability. Its name uses the same canonical root spelling as the shared mod lock. It is outside the selected tree. The initial game intent is published first; the mod fence is published and flushed under both root locks before any tree effect. A crash between these publications has performed no tree operation. Acquisition needs only its actual mod root to detect the fence; it does not fabricate a game root or scan unrelated recovery stores. Any partial, foreign, linked or unknown fence blocks acquisition.

The initial intent is written and flushed in a private temporary file outside the pending directory, then published atomically with native no-replace rename. Both source and destination namespaces are flushed. The published bytes are checked before a tree operation begins. A crash before publication exposes no partial intent and authorizes no tree mutation. Private scratch artifacts outside the pending directory are not recovery authority; normal scope cleanup removes the caller's own temporary artifact, and orphan private files from an abrupt process death are not used to authorize mutations.

Each rename leg and each permission change receives a flushed progress intent before mutation. Rename endpoints are exclusive. Parent/object identities, file hash/size and modes are checked; file and namespace changes receive the relevant durability boundaries. Recovery reconciles a single interrupted intent from exact endpoint spelling, object identity and modes, then verifies the complete projected subtree before normalizing progress. An incomplete append suffix is discarded only after this object-state proof.

After whole-tree verification, cleanup first publishes a terminal mod record containing the disposition, inventory count and SHA-256 proof of the projected complete inventory. This binds every name, identity, file hash and complete mode through the immutable plan. It then atomically publishes the matching terminal cursor in the game intent header. Removing progress therefore leaves a recoverable original/final cursor. The target tree's fence is cleared last: a `Mods` target durably removes the game journal and intent before clearing the mod fence; a `Game` target durably clears the mod fence while the terminal game intent still fences game-only writers, then removes that game intent. This prevents a legitimate writer for the target tree from changing its terminal proof before cleanup completes. Writers for the other tree can resume without changing the target inventory proof.

Nonterminal recovery still reconciles and restores the original tree. Once a matching verified terminal has been published in either owned record, explicit recovery verifies that terminal tree and completes metadata cleanup. It does not roll back a completed apply. Primary and mod terminal records must agree; mixed terminal/nonterminal records are reconciled against the full immutable plan. A disagreement or failed complete tree proof retains the fence. This terminal cleanup behavior supersedes the earlier source checkpoint's behavior of resetting a terminal header to begin rollback.

If only the terminal mod record survives, paired recovery verifies both root bindings and the complete actual inventory proof before removing that record. A fence-only record contains no mutation operations and can authorize no content write. `recover_mod_fence` provides the same metadata-only cleanup under the actual mod root lock when the target is `Mods` and primary artifacts are absent. Game-target cleanup requires both real root locks and paired recovery. Nonterminal orphan records require the primary intent and are never treated as cleanup authority.

Cancellation triggers automatic reverse rollback using a separate bounded cleanup budget that ignores the triggering cancellation. A whole-tree original-state proof is required before claiming rollback succeeded. Foreign edits, lost objects, mode/content changes, exhausted cleanup budgets and persistence failures retain an explicit recovery outcome.

If the final game intent unlink cannot be flushed, cleanup attempts to republish its terminal intent. A `Mods` target also retains its terminal mod fence; a `Game` target retains or republishes its primary game fence. If the mod fence unlink cannot be flushed, cleanup attempts no-clobber republication of that terminal fence. If the filesystem also refuses republication, absence guards require a successful parent namespace flush before admitting acquisition. Status likewise qualifies empty namespaces before reporting idle. No failed flush is reported as confirmed durable cleanup.

## Scope of mutation fences

Pending or foreign maintenance metadata fences `recovery::ensure_clean`, ordinary `Lock::acquire` before and after root lock acquisition, the game-scoped checkpoint/deployment guard, and checkpoint `Store::prepare_write`. Capture, restore, resume and lifecycle mutation paths that require that checkpoint capability check inherit the fence. Maintenance recovery uses a private lock path that qualifies checkpoint capabilities without waiving its own intent; checkpoint/external-process and no-follow legacy pending checks repeat under the held locks.

The shared `ModRootLock` checks the mod fence before and after acquiring its existing sibling lock. Paired ordinary root locks perform the same checks. The acquisition orchestrator holds this lock through its participating transfer mutations, so a restart cannot lose the maintenance admission fence. The deployment, install and checkpoint guards retain their game-side checks as well.

Only writers that participate in these locks and guards are covered. Arbitrary external editors, watcher publication and peer publication are not claimed to be isolated by the transaction. Callers must quiesce their owned writers. Final state comparison and per-operation object checks refuse detected interference, without claiming filesystem-wide isolation from hostile namespace races.

## Bounds and evidence

The target inventory is limited to 100,000 objects, depth 64 and 16 MiB of path bytes. Serialized documents use a bounded writer rather than allocating an unlimited document and checking afterward. The combined plan/progress budget is 64 MiB; new previews and progress writers reserve another 64 KiB within that budget for the mod fence. The fence itself is at most 64 KiB and contains no copied full inventory or plan. Preview reserves recovery progress space. These reservations can impose a lower operation count than the object inventory limit.

Each inventory/transaction work budget permits at most 128 GiB of aggregate hashed bytes, two million work/namespace visits and ten minutes. Hash loops and projection observe cancellation. Automatic cleanup has its own budget and cannot become an unbounded retry loop. Exceeding a recovery bound preserves the pending state instead of declaring success.

Source validation consists of formatting, whitespace checks, inspection of native and journaling paths, and independent review of crash windows, projection complexity and mutation fences. These are source checks. They do not establish native Windows/macOS support, runtime permission behavior, interruption recovery, power-loss durability or live-tree acceptance.
