# Standalone HoloPatcher desktop adapter

The pinned reference `MenuBuilderService.cs:737–740` exposes Run HoloPatcher
through its Tools callback. The Rust Tools action now opens a dedicated dialog
using the shared `patcher::standalone::run` implementation. This is source-only
work; no backend launch, native UI operation, tests or build were performed.

## Request and ownership

The dialog captures an optional backend path (empty uses shared discovery),
quoted arguments, optional working directory, and a 1–86400 second timeout.
Arguments are bounded to 16384 Unicode scalars by the UI and 64 KiB by the
runner. A per-run trust checkbox and Run button provide explicit authorization;
editing request fields clears the checkbox. No shell is executed.

Dispatch waits for the existing sharing mutation barrier and rejects a changed
captured application context. A queued request can be cancelled before launch.
The app's ordinary worker receiver excludes other mutations. A dedicated result
channel and retained JoinHandle remain owned until the worker finishes; polling
joins only a finished worker. Cancel and application close use the same token,
and normal close waits for cleanup and joining. An abnormal State drop requests
cancellation and joins as a final ownership fallback.

When game/mod paths are configured, both must resolve to valid distinct roots.
The worker holds their recovery lock and rejects unfinished legacy/checkpoint
recovery or install sessions before launch. With both paths empty, the tool runs
without a captured-root lock. Explicit arguments can target unrelated paths;
the UI states that those paths are outside the captured lock's protection.
No rollback snapshot is created for standalone execution.

## Result boundaries

Dispatch invalidates existing validation and installation completion presentation
because an external program may change files even on failure or cancellation.
It does not edit the guide or generate an installation receipt. The dedicated
result shows the actual exit code and bounded stdout/stderr, with selectable,
copyable read-only output. Failure/cancellation remains an error outcome.
Captured results from changed contexts are identified as such and never grant
readiness. Output is not copied automatically into telemetry or persistent logs.
Sharing stays paused until the user explicitly resumes it after inspecting edits.

Platform process containment and backend behavior remain the shared runner's
responsibility. Native backend/process-tree and desktop acceptance remain open;
this adapter does not establish them.
