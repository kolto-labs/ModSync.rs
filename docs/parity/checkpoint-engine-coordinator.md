# T3 checkpoint coordinator and finalization

This note describes production source contracts. No crash, restore, native platform,
managed deployment, or actual installation acceptance is implied by source review.

## Additive integration API

`Store::begin_session_with_context` accepts the held recovery lock and an
`InstallContext`: full ordered definitions, an initial completed definition prefix,
and named typed auxiliary bindings. Existing `begin_session` retains its signature
and supplies an empty inherited prefix and participant set. The engine retains its
default whole-run behavior; opt-in integration must use these shared primitives.

The initial prefix must equal the corresponding full-definition prefix. The
checkpoint service additionally compares it and the captured game-file manifest
against the actual bounded, safely opened legacy completion receipt. A matching
caller-supplied GUID list is insufficient. Baseline sequence zero records the
existing prefix's state; it does not claim vanilla state. Each newly completed
component advances sequence and generation once, and grows the exact definition
prefix by one. Nested options remain inside their top-level component boundary.

The engine takes the root lock once, validates, creates legacy whole-run recovery,
and then begins the checkpoint session before any install or auxiliary mutation.
Every component enters durable Materializing state before its effects. The engine
must emit component completion only after `commit_component` succeeds.

## Auxiliary ownership

Participants are at most 32 named absolute normalized bindings of kind File or
Directory. Captured snapshots distinguish absence, ordinary files (bytes, size,
hash, mode), and directory trees (including empty directories). Install roots,
recovery storage, aliases, and overlapping participant roots are rejected. Files
are copied into the same immutable CAS and included in inventory totals and
verification. Roots and auxiliary state are inventoried before capture and checked
again before publication. Immutable session bindings, not later caller paths,
authorize each component capture.

The deployment resolver supplies exact owned staging/deployment directories and
the individual game-owner record. A deployment directory includes its own manifests,
backups, binding marker and append-pending state. Foreign profile siblings and the
whole game-parent directory are outside the participant set. Resolver preflight
must reject unresolved append state and foreign ownership before admission.

Fresh artifacts can have missing parent directories. The coordinator records up
to 64 initially missing ancestor directory shells separately. A shell records only
absence or directory mode; it grants no ownership of child contents or siblings.
Future restore must remove an originally absent shell only after owned children
are restored and only when the shell is empty. Foreign contents must block cleanup,
never authorize recursive deletion. This slice captures shell state; it does not
implement that later restore operation.

`inspect` includes typed snapshots. `verify` checks their object closure. `diff`
includes changed auxiliary snapshots and changed ancestor shells alongside the
existing game/mod changes. Existing v1 documents with empty auxiliary state remain
readable through default-empty additive fields. The old `auxiliary` tree field is
retained and remains empty; it is not silently reinterpreted as absent/file state.

## Finalize receipt transaction

After the entire immutable prefix is committed, `finalize_session` verifies the
object closure, current roots/auxiliary state, and the legacy snapshot's binding to
the session baseline. It builds the existing completion-receipt schema from the
full immutable definitions and final game manifest. The expected receipt bytes are
stored in CAS and bound by hash/size to a durable Finalize operation.

The transition is:

1. Persist the expected receipt and Materializing Finalize intent.
2. Atomically write and sync the completion receipt; verify its exact bytes.
3. Remove the legacy pending marker and sync its parent directory.
4. Durably publish `FinalizeCleanupRequired` HEAD, retaining the receipt binding.
5. Remove and sync the operation journal.
6. Durably publish `Complete` at the same checkpoint and generation.

Finalization creates no fictional component or sequence. Receipt reconciliation
runs to a durable transition once started even if cancellation arrives; failures
remain errors and retain a journal or cleanup HEAD fence. Readers reconstruct a
missing Finalize journal from the cleanup HEAD and verify the binding against the
immutable completed checkpoint. A final Complete-publication failure occurs only
after receipt and cleanup durability, and still does not report success.

## Session lifetime and legacy callers

An unfinished install session fences legacy installation/restore even between
components, when no component journal exists. Otherwise a whole-run restore could
replace game/mod roots while leaving managed artifacts and HEAD at a later state.
`ensure_no_unfinished_install_sessions` supplies this check. Complete sessions do
not fence subsequent work.

Standalone deployment has no source-root argument, so the read-only
`ensure_game_checkpoint_clean` derives the game-keyed store and validates its saved
source binding, operations and session lifetime. Malformed state or unavailable
bound roots fail closed. It creates no directories or locks. Standalone callers
must check before constructor mutations and under the deployment lock. The managed
engine uses its matching held-lock capability for its own admitted session.

Recovery/resume, restoration of auxiliary state, Finalize recovery after failure,
branching, retention and garbage collection remain subsequent checkpoint slices.
No pending state is silently repaired by a reader. No tests, harnesses, runtime
probes, backend executions, or Cargo commands were run for this coordinator slice.

The report-aware finalization sibling also persists the typed `InstallReport` inside
the expected receipt and HEAD/intent receipt binding. Its checkpoint IDs/generation
and managed-participant presence are validated. Checkpoint inspection exposes the
receipt hash and report payload. T4 library behavior is described in
[the restore/resume note](checkpoint-restore-resume.md).
