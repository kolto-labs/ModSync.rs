# E7 owned external process execution

Engine Execute, Run, and Patcher actions now call the same owned command runner
as standalone HoloPatcher. There is one Linux/macOS process-group owner and one
Windows suspended-launch/Job Object owner, shared by both entry points.
The existing explicit process-trust and backend preflight gates remain.

The crate-private `patcher::standalone::run_command` accepts a prepared Command,
a timeout, and cancellation. Current callers construct inherited environments
with explicit overrides; none clears the environment. The internal command
budget is 512 arguments and 128 KiB including invocation prefixes. Standalone
user-argument admission keeps its stricter existing public limits. The engine
uses a fixed 24-hour execution deadline without adding public Settings fields.

Admission and spawn failures return errors. After launch, a terminal receipt
retains available stdout/stderr, exit status when established, cancellation,
and a failure diagnostic. Each stream is limited to a one MiB polled disk-size
ceiling and bounded final capture; excess output is truncated and fails the
operation. Cancellation, timeout, wait/I/O errors, and failed descendant cleanup
cannot become a successful engine action.

For every launched outcome, Unix retains the unreaped leader while repeatedly
terminating its process group and checking live membership. The reserved leader
prevents normal group-ID reuse. Linux inspects matching process task states under
`/proc`, including siblings of a zombie main thread; macOS uses libproc process
inventory and BSD state. Zombies cannot write and do not block settlement.
Disappearing entries are normal races; inaccessible or malformed membership
information retains ownership and retries. The leader is reaped only after no
live members remain. External reaping is an ownership violation: no further
signals are sent after detected identity loss. Deliberate setsid escape by
explicitly trusted code remains outside cooperative group containment.

Windows retains the private Job Object until its active process count reaches
zero and the leader wait completes. These cleanup paths apply to success,
cancellation, timeout, and failure. Cleanup has no cancellation/deadline escape:
uncertain membership or OS termination failure retains the runner and therefore
the engine recovery lock. This can wait indefinitely rather than release roots
while an owned writer may still be active.

The engine publishes the fresh patcher log result and captured streams before
checking terminal failure/cancellation/exit status. Existing patcher log error
checks remain mandatory. Managed process reconciliation and the fresh VFS scan
run only after successful process, cleanup, and patcher-log verification.
Interrupted actions retain the existing pending recovery state.

The execution deadline is cooperative. OS cleanup/reaping waits can outlast it;
this is not a hard real-time timeout guarantee. No process launches, tests,
filesystem probes, native builds, or CI were performed for this source slice.

Expected patcher logs use the existing no-follow regular-file opener and an
8 MiB descriptor/read limit. Oversized or invalid UTF-8 logs fail verification;
no truncated log can prove success. RTF retains precedence over TXT, including
when the preferred log is unreadable. The bounded read remains available after
cancellation so terminal failure diagnostics are not discarded. Fresh-log
cleanup and invocation-root selection are unchanged.
