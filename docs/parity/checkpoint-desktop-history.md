# Native checkpoint history and explicit installation mode

The desktop uses the shared checkpoint store and engine coordinator. The title bar opens **Checkpoint history** without changing the current installation mode. Whole-run recovery remains the default; the window exposes explicit component-checkpoint and managed-deployment controls. Managed profile resolution and explicit staging/manifest resolution happen inside the worker.

## Shared operations

Session listing and bounded history browsing show session status, HEAD generation, component boundaries, completed-prefix counts and stored byte totals. Select a boundary to inspect it, verify objects, choose a diff origin, inspect path provenance, restore it, repair its objects or delete its session. Recovery can select one session or ask the library to find an unambiguous interrupted operation. Garbage collection uses the shared reachability implementation. Incomplete repair reports show their unresolved objects alongside the repaired count.

All store operations use the desktop's existing worker receiver, cancellation token and close lifecycle. Reads never create a store. Mutations wait for the completed-sharing barrier before acquiring the library's recovery lock. The UI binds queued actions and results to captured inputs; changing roots or guide settings invalidates their readiness. History status alone does not authorize mutation: pending operations are read separately, and the library revalidates every actual action. Recovery errors remain visible.

A successful mutation report is retained even when the subsequent readiness read fails. The result names its actual captured folders. A stale result cannot update the current project's readiness. Restore/recovery clear prior validation, completion receipts and completed-prefix UI state before dispatch, including when a later error or cancellation leaves recovery necessary.

## Resume and completion

Resume passes the full currently selected guide, including widescreen components, through the shared phase/FOMOD preparation and engine resume API. The UI supplies no completed-prefix authority. The engine validates the immutable prefix, participant bindings and current trees under lock. Accepting changed source files remains an explicit option and requires a successful restore first.

The ordinary UI validation prerequisite is bypassed only for explicit checkpoint resume because legacy validation cannot clear an unfinished checkpoint session. The shared engine resume preflight still runs. Resume never silently falls back to an ordinary install.

Install workers return the actual typed `InstallReport`. Completion pages retain its checkpoint identity and managed-deployment report, expose a copy action, and show retained-history recovery wording when applicable. Newly installed component counts use the checkpoint session's initial prefix, rather than a stale desktop phase count. Failure to read that summary is displayed as unavailable and does not recast a committed install as failed.

## Evidence boundary

This is production source implementation and static review. No tests, custom harnesses, native probes, filesystem fixtures, real installations, or restores were run for this slice. Native keyboard/focus behavior, cancellation timing, restart behavior and reconstructed bytes remain acceptance work. The underlying store rejects unsupported writer platforms explicitly. Component checkpoints have not become the default.
