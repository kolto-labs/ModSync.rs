# T4 restore, recovery and resume source contract

These are production-library semantics, pending independent source/build review and
later native filesystem/crash acceptance. The default installation mode is unchanged.
Windows and other non-Unix checkpoint writers remain rejected before mutation by
the shared durability gate; this note does not claim their restore support.

## Public entry points

- `restore_checkpoint(lock, expected_head, target_checkpoint_id, cancelled, progress)`
  restores a committed boundary in the selected session's ancestry.
- `recover_pending(lock, optional_session, cancelled, progress)` resumes the one
  durable operation or recovers an unambiguous unfinished session. Unrelated
  unfinished sessions remain blockers; they are never detached as collateral cleanup.
- `resume_plan(request, full_definitions, participant_bindings, cancelled)` is read-only.
  It returns the verified completed prefix, remaining suffix, source changes and a
  private observed-inventory binding.
- `branch_for_resume(lock, plan, cancelled, progress)` revalidates the exact private
  plan after engine preflight and creates the next install session. It replaces the
  normal legacy-snapshot/new-session setup in the resume path.

Every mutator requires the matching held game/source lock. Readers and plans create
no directories or locks. CLI and desktop adapters consume the same library; no
frontend constructs a completed prefix by splicing GUIDs.

## Restore authority and materialization

The complete target metadata, object closure, root binding, auxiliary ownership and
supported modes are validated before destructive work. Both install roots must
still be directories. Selected rollback first captures and verifies an independent
safety session; its ID is retained in the new immutable session and result. Recovery
of a failed operation retains that operation's protection instead of pretending the
partially changed tree is another successful component.

A durable transition fixes the original HEAD identity, target checkpoint, new branch
session/baseline IDs, action and safety reference. Cancellation after publication
leaves that same target recoverable. Materialization preserves game/source root
inodes, creates verified file copies through operation-specific temporary siblings,
flushes data and parent directories, removes absent paths and restores modes. Owned
directories become temporarily writable for traversal and replacement; target modes
are applied after their contents. Symlinks, reparse points and special files reject.

Directory participants grant recursive ownership only inside their exact bound
root. File participants grant one leaf: a live directory at a File binding rejects
before any root mutation. Ancestor shells grant no sibling ownership. Restoring an
absent shell rejects foreign descendants up front and uses only nonrecursive
`remove_dir`; a foreign file appearing during work causes failure and leaves the
intent. No recursive game-parent or profile-sibling deletion is authorized.

After exact game/source/auxiliary verification, the transaction reconstructs the
legacy completion receipt for the restored prefix (removing it for prefix zero),
then rebuilds and flushes the legacy compatibility snapshot under the same intent.
Snapshot copying is cancellable. The snapshot manifest must match the restored
baseline. No successful result is returned for partial materialization.

## Branch commit and interrupted cleanup

Restore creates a new lineage-linked baseline at the target prefix. The old ancestry
is preserved; moving backward never lowers an existing HEAD generation or rewrites
old immutable checkpoints. The new `TransitionCleanupRequired` HEAD embeds the
Verified transition operation. Only after that durable HEAD and receipt/snapshot
reconciliation does the source HEAD become `Detached`. Journal deletion is synced
before the branch becomes `Restored` (rollback) or `Ready` (resume/rebase).

A missing journal does not remove the fence: the embedded operation reconstructs it.
Readers validate the same schema, roots, UUIDs, definition digest and branch binding
for file-backed and reconstructed operations. The branch cleanup HEAD must identify
its own session/checkpoint. A durable replacement transition explicitly supersedes
the old failed component/finalize journal while retaining the old HEAD until commit.

Completed-component cleanup recovery verifies the advanced checkpoint and exact live
state before accepting it. If it cannot accept that state, recovery restores the
previous boundary into a branch and returns `restoredPreviousBoundary`, identifying
the advanced source and restored target separately. It never leaves the advanced
checkpoint executable as success. Uncommitted components restore their prior
boundary. Finalize recovery verifies the full final state and receipt CAS, then
reconciles the receipt and pending marker idempotently; altered live state is restored
to the final committed boundary instead of being silently finalized.

## Resume and source repair

Resume requires an unchanged full-definition completed prefix, including selected
option payloads and order, and exact game/auxiliary state. The remaining definitions
may change to repair the failing suffix. Participant identifiers, kinds and paths
must equal the original session; neither a different artifact root nor an omitted
managed participant can be substituted.

Without explicit source-change acceptance, the source tree must also match exactly.
Changed source files are accepted only after a successful restore (`Restored` HEAD).
The request reports changed paths, before/after hashes and root-mode changes and
captures a `SourceRebase` branch. It never edits the prior checkpoint's expected
hashes. The resume plan privately binds the exact observed inventories; further
changes between planning, engine preflight and branch capture reject even when
source-change acceptance was requested.

Branch lineage is validated with a 128-session depth bound and a shared 256 MiB
checkpoint-metadata budget. Safety and parent session links remain retained
references for lifecycle operations. Detached source sessions preserve history but
no longer act as unfinished-install fences.

## Remaining proof

No tests, custom harnesses, runtime probes, actual restores, installs or Cargo
commands were run by this implementation worker. Production builds are owned by the
parent acceptance operator. Native interruption boundaries, disk-full behavior,
process termination, all supported filesystem modes and end-to-end guide replay
still need their planned production acceptance. Hostile concurrent ancestor swaps
remain outside the existing filesystem threat model; captured/live changes are
checked, and detected changes stop the transaction.
