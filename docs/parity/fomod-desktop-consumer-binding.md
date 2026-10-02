# Desktop FOMOD consumer binding

Working source, 2026-10-02. Runtime/native acceptance remains deferred.

The post-download worker uses an additive `bound_fomod_followups` route. It
inspects each acquired archive once, preserves every selected unfinished
top-level consumer from the captured acquisition report, and validates existing
choices against the current installer. Valid saved sessions are retained.
Malformed or stale sessions produce an inspection error; prompt suppression
does not become an installation-readiness claim.

The desktop groups those records by exact archive path and carries the full
consumer GUID list through its queue, editor and outcome. Choices and dismissal
use `apply_bound_archive` on precisely that list. Run identity, guide/context
stamps and completed-prefix checks still apply. The explicit manual editor
targets the selected unfinished top-level component, whose name is shown beside
the configuration controls. Nested options are not enrolled by basename.

Direct URL downloads capture eligible top-level resource owners when dispatched.
Their `Files` key must name the actual output leaf, rather than an arbitrary
matching basename in another folder. Existing saved choices suppress automatic
replacement; validation still diagnoses missing or invalid configuration.

Prefill reads only bound embedded sessions. The editor worker normalizes their
selections and mapping against the current archive before comparing them. A
matching entry in an explicitly selected external sidecar takes precedence;
when that sidecar lacks the archive, embedded choices remain the fallback.
Sidecar reading uses the shared ordinary-file opener and an 8 MiB bound on the
worker, avoiding a repaint-thread file read. Its existing context stamp rejects
changed files before the outcome is accepted.

An explicitly selected sidecar retains its prior run-wide import meaning:
acceptance imports it into a cloned guide, then applies the editor change only
to its bound consumers. Both commit together before the sidecar path is cleared.
The result log distinguishes this explicit import from the scoped editor edit.

No test source, filesystem fixture or native probe was created or executed.
Source review and production compilation do not establish queue/race/restart or
saved-guide acceptance. Folder Extract candidate semantics and native image
rendering remain separate work.
