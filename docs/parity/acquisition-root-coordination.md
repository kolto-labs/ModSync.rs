# Acquisition root coordination

Production acquisition now shares the mod-root half of the install/recovery lock namespace. `recovery::ModRootLock` opens an existing ordinary root, retains its directory identity, canonicalizes the binding, and takes the existing sibling `.modsync-lock-<root-hash>` file lock without fabricating a game directory. Both paired normal locks and acquisition check the durable maintenance mod fence before and after lock acquisition. `recovery::ensure_clean` also checks this fence. Maintenance recovery has a crate-private acquisition path for validating and clearing its own fence under the same lock.

The shared opener refuses special files, links/reparse points, replaced lock paths and replaced roots. Destination checks repeat root identity and ordinary ancestor checks. Coordination is between cooperating ModSync writers; these locks do not prevent arbitrary external programs from modifying files or replacing paths.

## Orchestrated acquisition

`download_orchestrator::run_with_source_context` takes the current execution guide. The mod guard is held before stale peer-stage cleanup or network acquisition until all scoped workers finish. The execution root is canonicalized once. The plan is limited to 100000 targets plus skips before its execution copy is made. Planning, discovery and run admission share keyed transitive destination grouping (known filename groups and unknown-name URL bridges), with cancellation, a 100000-target bound and a 2000000-edge work limit. Worker configuration requires 1–32 workers and at most 10 retries; the active pool is capped by group-owner count. Component GUID lookup is indexed once (at most 100000 components); authored file/instruction/source matching is cancellation-aware and limited to 2000000 visits.

An `AlreadyPresent` skip is revalidated while holding the guard: unique selected component, exact registry URL ownership, currently enabled resource/filename, contained ordinary file, current authored SHA256 when present, and archive integrity. Filename ownership follows the authored Files/Extract matching policy, without treating a file's newly satisfied Extract pattern as proof that its consumer no longer needs it. Length and modification time are compared across verification and safe containment is repeated. This does not constitute a cryptographic snapshot against noncooperating concurrent writers.

Invalid or missing skips fail before cleanup or acquisition. They are not silently converted into new download targets. Existing `run` and `run_with_loopback_ca` signatures remain available, but refuse plans containing `AlreadyPresent` without source context. Production CLI, desktop and the CLI acquisition prephase use the bound API; include-unselected authoring passes its selected execution projection.

Stale peer staging cleanup is cancellable and scans at most 100000 root entries.
Only an old ordinary directory named `.peer-<32 lowercase hex nonnil UUID>.part`
is eligible, and cleanup uses `remove_dir`, which removes only an empty directory.
Historical nonempty staging is retained for inspection; the program cannot prove
ownership from a name and timestamp. Recursive cleanup remains confined to the
current run's owned stage after its transfer workers finish.

A report constructed for cancellation before entering the runner omits `AlreadyPresent` skips, because those files have not been revalidated under the guard. Other skip diagnostics and cancelled target outcomes remain available. Such a report cannot record an unverified file or authorize its FOMOD followup.

## Standalone adapters

`acquisition_guard::Scope` is the shared adapter for standalone outputs. An explicit source root coordinates nested destinations with that install/maintenance root. Without an explicit source root, only the immediate output parent is locked; the program does not infer a larger mod workspace. Low-level transfer functions remain unlocked so callers can retain one outer guard without recursive locking.

Direct `download`, unbound `nxm-download`, `torrent-download`, and
`download-batch` accept an optional `--source-dir` for this binding. The desktop
single-download adapter supplies its configured mod root. Canonical output paths
are retained throughout execution and in batch results. Existing destinations,
duplicate exact destinations, parent traversal and unsafe ancestors are refused
before transfers. Case aliases on a case-insensitive filesystem still rely on
the transfer's native no-clobber publication; they cannot silently replace a file.

Standalone batch intake is limited to 64 MiB and 10000 jobs. Standalone and
orchestrated execution require 1–32 workers and at most ten extra retries, and
spawn no more workers than available jobs/groups (one for an empty queue).
The batch scope stays owned through all worker joins and retries. These are
cooperating-writer locks and path checks; acquisition does not adopt maintenance's
current-owner qualification or claim descriptor-sealed ancestor protection.

The caller's cancellation flag reaches both torrent proxy modes. Direct magnet
metadata acquisition observes cancellation and a 120-second timeout; tracker
preparation occurs before session creation, and metadata/completion outcomes flow
through explicit session stop. Successful publication wins over a later signal;
cancelled CLI errors and cancelled batch outcomes use exit 130.

## Evidence boundary

This is a production source change. No filesystem tests, custom harnesses, provider requests, native probes or CI were executed for this slice. Cross-process/native acceptance remains separate from source review and parent-owned production compilation.
