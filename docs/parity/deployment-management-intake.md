# F18 bounded deployment metadata and management API

Source-only implementation. No tests, probes, native actions, or builds were run for this slice. Earlier deployment probe reports describe their historical source, not this revision.

Deployment manifests now enter through a 16 MiB bounded regular-file reader using the shared no-follow opener and ancestor path checks. Inventory is limited to 10,000 directory entries and 64 MiB of retained serialized manifest data. Each manifest validates component/filename binding, relative game and backup paths, unique normalized paths, SHA-256 syntax and a 100,000-entry limit. Legacy opaque component IDs, GUID casing, path separators, date parsing and public manifest shapes remain supported. Unsupported or unreadable JSON candidates retain per-file warnings, including symlink/special-file candidates. A direct lookup fails rather than treating a dangling symlink as absent. Writes validate and bound the final serialized document before writing markers or replacing the manifest.

Owner and game-marker reads are bounded to 64 KiB and use the same safe reader. Only missing owner files count as absent; unreadable/special files propagate errors. Ownership inventory errors conservatively mean occupied rather than free. Invalid nonempty game markers reject mutation. These changes do not provide protection against arbitrary concurrent external renames of ancestor directories; cooperating ModSync writers serialize through the game lock.

## Desktop adapter contract

Resolve via `Service::resolve_readonly` first; it does not create managed participant roots. Run these calls on the adapter's owned worker:

- `management_status() -> ManagementStatus`: acquires the game lock, honors the checkpoint fence, returns captured game/manifest/profile roots, manifests, warnings and deterministic review identity. An absent manifest directory is an empty inventory.
- `manage(&status.identity, ManagementAction::Uninstall(id) | ManagementAction::Purge, &AtomicBool) -> ManagementReceipt`: owns the same lock and fence, recomputes the review identity, refuses unreadable inventory, then uninstalls newest-first for purge. It stops after the first skipped/warned operation or failure, retaining prior reports.

`complete` is false for cancellation, warnings, partial outcomes or errors. Cancellation is observed between component operations; an in-progress component is allowed to settle. A failing component can have partial filesystem changes; its error explicitly requires inspection. This API does not claim standalone uninstall crash rollback. The adapter must retain captured context, pause sharing before mutation, reject stale presentation, and display individual deleted/restored/transferred/skipped paths. It must not turn `Ok(receipt)` into success without checking `complete`. Existing standalone APIs and typed reports remain available with unchanged signatures.

The engine's existing checkpoint-enabled managed install remains separate from these desktop management actions. The next source slice is the desktop inventory/badge/uninstall/purge adapter; native filesystem and UI acceptance remain required afterward.
