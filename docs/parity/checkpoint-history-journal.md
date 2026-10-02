# T2 history and component journal implementation

Status: production source implementation; no filesystem tests, runtime capture,
crash injection, restore, installation, or CI acceptance was performed. Engine and
managed deployment integration remain T3 work. The public component writer primitives
are available to that future integration; the CLI exposes readers only.

## Readers

Inspect, diff, provenance, list, and verify use one ancestry validator. It follows
HEAD to baseline with a cycle/10,001 boundary/256 MiB document budget, validates each
schema, canonical UUID/root binding and manifest, and requires sequence +1 and exactly
one complete component definition appended at every boundary. Full ordered definitions
are recursively serialized with sorted object keys (array order and scalar types
preserved) before digest and prefix checks. Nonempty histories require definition and
prefix digests. Legacy empty T1 baseline documents remain readable. Orphan documents
never become history authority. HEAD is re-read at the end to reject stale observations.

Diff covers both trees, file hash/length/mode, directory entries, creation/deletion,
and root modes. Provenance reports every changed committed state of an exact portable
relative path, associated with the boundary's component identity; baseline is explicit
with no component. This describes committed manifests, not proof of a backend's effect.
Read methods never create a store or acquire root locks. Pending operations are validated
against immutable session definitions, parent sequence and current HEAD operation ID.
List marks the session Pending and inspect exposes the intent; diff and provenance may
browse committed history. Verify refuses pending operations so closure verification
cannot be confused with a recovery-ready live tree.

## Writer primitives

`begin_session` requires an existing unforgeable recovery Lock for exactly these roots;
it captures a baseline and freezes complete ordered definitions. `begin_component`
requires a matching ExpectedHead (session/checkpoint/generation), verifies its object
closure and live root manifests, and publishes a durable intent for the next immutable
definition. `mark_component_running` durably changes Intent to Materializing before
any component mutation. `commit_component` accepts only that exact persisted operation
and expected HEAD, captures consistent resulting trees, publishes the immutable next
checkpoint, re-reads it and verifies all objects, then publishes HEAD with the operation
ID, incremented generation, and status `CleanupRequired`. Only after that HEAD is
durably published is the intent removed and its directory synced. After successful
cleanup sync, HEAD is replaced with `Ready` at the same generation.
Component commits leave HEAD Ready, including the last component; Complete is reserved for the future Finalize intent and legacy-receipt reconciliation. The writer never accepts a caller-supplied
completed prefix or adopts a different operation. It does not run a patcher or decide
whether a component succeeded; the integrating engine must establish that first.

All writes require the held two-root lock without reacquiring it. Any unresolved intent
blocks fresh captures, sessions, or components. A crash after HEAD but before intent
cleanup leaves a recognized pending operation that still fences subsequent writes.
Readers never repair it. Failures before HEAD leave orphan data; failures after HEAD
retain committed history and may retain the intent. The durable `CleanupRequired`
HEAD remains the recovery fence even when unlink succeeded but directory sync failed.
The pending reader scans both bounded operation and session inventories, reconstructs
any missing intent from the validated HEAD ancestry, and checks equality when both
representations exist. Incomplete baseline sessions without HEAD remain nonauthoritative;
readers neither create paths nor remove records. Old T1 `Ready` and `Complete` heads
remain readable without migration. A final Ready-publication failure cannot grant
unsafe progress: checkpoint content and journal removal are already durable before
that publication is attempted, and an error still prevents a success report.
Legacy install and restore also reject these pending operations. Cancellation is checked before
commit, and is not reported as an uncommitted cancellation after HEAD. No repair,
rollback, restoration, pending-intent deletion command, branching, managed auxiliary
capture, retention, or default install-path change is implemented in this slice.
Unix directory durability and the nonhostile ancestor-replacement assumption from the
T1 note still apply. Native durability/crash-boundary acceptance remains outstanding.

T3 additive context, auxiliary inventory and Finalize contracts are described in
[the coordinator note](checkpoint-engine-coordinator.md). Existing empty-prefix,
empty-participant v1 history remains readable.
