# Managed deployment participants for component checkpoints

Source audit of the current working tree, 2026-10-02. This records implementation
requirements for checkpoint T3; it does not establish runtime rollback proof.

## Existing call paths

`Engine::install_observed` acquires `recovery::Lock`, validates, creates the legacy
two-root snapshot, runs components, and writes the completion receipt. Its current
component loop does not call `deployment::Service`. Both install frontends use
that shared engine. Managed deployment currently has separate CLI operations;
its existence does not establish managed installation or checkpoint integration.

`deployment::Service::lock` uses the same game-scoped lock path as recovery.
Checkpoint-enabled installation must pass its held-lock capability through the
deployment participant, without acquiring that game lock again. Standalone
deployment must also check checkpoint intents before mutating a participating
game or auxiliary tree.

## State requiring explicit ownership

| State | Current source | Required capture boundary |
| --- | --- | --- |
| Component staging trees | `Service::staging_root`, `staged_directory` | Exact session-owned staging paths, including absence and empty directories |
| Component manifests | `manifest_root/manifests` | Owned manifests and original bytes |
| Displaced game files | `manifest_root/backups` | Owned backup trees and metadata |
| Deployment game binding | `manifest_root/game-directory` | Existence and exact bytes |
| Append recovery state | `manifest_root/append-pending` | Detect unresolved state before baseline; never erase it as unrelated scratch |
| Per-game owner record | Game parent `.modsync-owner-<hash>` | Existence and exact bytes; validate the bound manifest root |

Profile settings and other profiles' artifact trees are outside this ownership
set. Peer-profile manifests are read for exclusivity, not captured and overwritten
as if owned by this session. Auxiliary roots must be resolved and validated before
mutation, with aliases, overlapping roots, unsafe paths and foreign ownership
rejected.

## Constructor and preflight mutations

`Service::new` creates `manifests` and `backups`. `Service::for_profile` creates
staging/deployment directories and constructs a profile store. These constructors
cannot serve as read-only participant discovery. Split path resolution and
validation from initialization before registering a checkpoint participant.

`ensure_exclusive_game` can adopt the game owner record. Capture the owner record
before adoption, or include adoption in the journaled transaction. Calling this
method before a baseline without capturing its earlier state loses rollback
information.

## Integration requirements

1. Resolve participant paths without creating files or directories.
2. Acquire the shared recovery lock and validate root/profile/owner bindings.
3. Reject existing legacy, checkpoint or deployment append intents as appropriate.
4. Capture both install roots and the registered auxiliary state before mutation.
5. Journal each top-level component, including its deployment effects and nested
   option effects; advance HEAD before emitting component completion.
6. Restore and verify auxiliary state together with both install roots. A
   game-only restore must not leave stale manifests, backups or owner records.
7. Keep component checkpoints opt-in until the required restore/crash acceptance
   can run under the user's deferred-testing policy.

The baseline and history slices currently capture only the two explicit install
roots. They must not advertise managed-deployment transaction coverage.
