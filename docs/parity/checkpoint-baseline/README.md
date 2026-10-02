# T1 immutable baseline implementation

This implements the additive capture/list/verify library slice from checkpoint-plan.
The producer and readers have not been exercised against filesystem fixtures here;
no Rust tests, harnesses, CI, restore or live game operations were run. Production
build/lint gates are coordinated separately and do not establish crash or native
filesystem acceptance. T2 history/journal source is described in [its implementation note](../checkpoint-history-journal.md); T3–T7 remain open and default installation is unchanged.

`Store::open(&Roots)` validates canonical UTF-8 root bindings and the derived
`checkpoints-v1` path without creating it. `list()` returns an empty report when
absent. Read operations create no directory/lock/selector, do not hash live roots,
and reject unsafe paths, symlinks, special files, foreign schema/unknown fields,
noncanonical UUIDs and unsupported history. The T2 reader extension accepts only
validated committed HEAD ancestry. `verify` accepts a selected committed ancestor, verifies every referenced CAS object independently, and rejects a changed
HEAD after reading. Incomplete session directories without HEAD remain explicitly
incomplete and cannot authorize a restore or resume. An active selector is only
convenience; verification never relies on it.

`capture` acquires the existing two-root recovery lock once and creates an independent
session, with empty completed definitions and no parent lineage. A legacy pending
snapshot does not become completed-component authority and is not modified. Both
roots are inventoried with file SHA256/length, directory entries (including empty
directories), readonly and available Unix permission bits. Modes exclude ownership,
ACLs, xattrs and timestamps. Inputs require portable relative names with no case/NFC
aliases or file-as-directory ancestry. Limits: 100,000 entries per root, 64 MiB per
JSON artifact, 10,000 sessions per listing. Regular-file hashing and copying are
streamed and cancellation-aware; files growing during capture fail.

Mutable live files are copied into private object temporaries; no live-file hardlinks
are made. Existing object identities must verify before reuse. A no-clobber publication
is independently rehashed. Captures compare full before/after root manifests to detect
noncooperating changes. This is consistency detection, not protection against a hostile
writer changing roots between observations. Parent directories and leaf files are
checked; Unix leaf opens refuse symlinks and directory/special-file leaves. Concurrent
adversarial ancestor replacement remains outside the current nonhostile workspace
assumption and requires stronger directory-handle traversal for a hostile threat model.

Objects, checkpoint metadata and session metadata publish immutably before HEAD.
The emitted manifest/session and object closure are read back before HEAD becomes
authoritative. HEAD is the commit point; postcommit cancellation does not pretend the
checkpoint was rolled back. The active selector is then atomically replaced. A failure
before HEAD leaves only unreferenced objects/incomplete sessions; a selector failure
after HEAD is an error with a valid discoverable checkpoint, not automatic rollback.
No garbage collection or orphan deletion occurs in T1.

Writers currently support Unix file/directory fsync and fail explicitly before state
creation on platforms without implemented directory durability. An fsync error is a
failed operation, never silently reported durable. Native Windows/macOS/filesystem
power-loss behavior and process-interruption boundaries remain acceptance work.
Read APIs can decode the schema on other platforms but cannot claim metadata-equivalent
restoration; restoration is not part of T1.

The required next evidence is the plan's real producer capture → list → verify matrix:
deduplication, empty directories/modes, before/after root and legacy-state manifests,
corrupt/missing objects, malformed IDs/root bindings/paths, no-clobber collisions,
cancellation/interruption before and after HEAD, and read-only absence. None of that
acceptance is inferred from source or successful compilation.
