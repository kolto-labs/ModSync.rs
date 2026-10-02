# T3 opt-in engine and managed participant integration

Implementation source only; no install, fixture, test, crash injection, restore,
receiving-client or CI acceptance is claimed. The ordinary whole-run engine path
remains the default. This note describes the opt-in path under review.

## Engine boundary

`Engine::with_component_checkpoints(bool)` adds configuration without changing
required Settings fields. `with_managed_deployment(Service)` requires that opt-in
and an exactly matching game root. Installation holds the existing recovery lock
once; it rejects legacy/checkpoint unfinished work and validates managed ownership
before preflight and the legacy recovery snapshot. A checkpoint baseline then binds
full selected ordered definitions, the already verified completed prefix, and exact
auxiliary participants. Baseline sequence zero can represent a previously installed
phase; it is never described as vanilla.

Every remaining top-level component publishes its next-definition intent and durable
Materializing phase before managed initialization, reinstall cleanup or instructions.
Managed source simulation runs just in time after journaled reinstall preparation,
then the engine rescans before actual instructions. This validates restored originals
instead of the old deployed tree; a preparation/preflight failure leaves the transaction
pending for restore. Structural/FOMOD/trust/backend checks still precede the snapshot, with selection context
initialized before conditional applicability checks. Pure managed validate/dry-run does
not forecast reinstall effects; only this journaled install preflight uses restored
originals, so dry-run output is not exact managed reinstall proof.
Nested Choose components run within that owning boundary. After instructions and any
managed deployment flush, the store captures both roots and participants, commits HEAD,
and clears the journal before ComponentCompleted is emitted. Any instruction,
publication, provenance, or cancellation error stops before the next component. Finalize
binds the expected legacy receipt and reconciles the legacy pending marker before the
engine emits overall completion; the opt-in path does not independently write a second
receipt. There is no restore shortcut or automatic continuation through a failure.

## Managed filesystem ownership

`Service::resolve_readonly` resolves explicit staging/deployment roots or a named/active
profile without mkdir or owner adoption. Explicit paths determine deployment artifacts
when supplied together; the existing standalone resolver contract is unchanged. The
session registers the exact staging tree, deployment tree, and individual game-parent
owner file. Existing and missing participant states and any constructor-created directory
shells belong to the checkpoint store's participant contract. Other profiles are checked
for exclusivity but their content is not captured as session-owned data.

The ordinary Service constructors hold a temporary game lock and check the game-scoped checkpoint fence before mkdir; the lock drops before returning the service.
Standalone game-lock acquisition and direct mutation methods also check it. Only an
internal initialization path with a matching held recovery lock enables the engine's
private session capability. Initialization and owner adoption occur after the component
intent. Managed root paths reject symlink/reparse ancestors and overlapping install roots.

## Managed instructions and external programs

Logical VFS keys remain game paths. Extract/Copy/Move/Rename game outputs are written
atomically to the owning top-level component's staging tree. Later sources read staged
bytes. Nested options preserve their top-level deployment owner. A logical deletion removes both any live path and its pending/retained staging source,
preventing an old live file from reappearing on rescan or a later flush from restoring it.
This deliberately preserves ordinary logical Move/Delete semantics instead of retaining
an older live file behind a deleted pending replacement. Unstaged
Delete/DelDuplicate/CleanList effects remain live edits, as described in the C# deployment
audit; the checkpoint records them even where a deployment manifest cannot represent
an explicit deletion. No complete per-file uninstall equivalence is inferred.

Before Patcher/Execute/Run, staged files are deployed through deploy_append and the VFS
is rescanned. A cancellable private byte-copy preimage captures the immediately preceding
game state, verifies every copied hash and a second source inventory, and supplies
record_live's backup provenance. It is not a hardlink to the game or staged files.
A growing preimage file fails bounded hashing. External deletions are reported as
untracked live edits and remove the current component's retained staging counterpart.
Their old deployment manifest entries remain historical ownership records; later
re-adding the same path can still be refused by deployment integrity checks. This is
not complete deletion provenance or unrestricted reinstall equivalence. Successful process output/log verification
precedes provenance recording; skipped provenance is an install failure. The VFS is
rescanned after external mutation. Component end flushes any remaining stage and captures
the resulting transactional state.

## Remaining evidence

Source review and compilation cannot establish power-loss durability, native filesystem
behavior, restore replay, nested patcher equivalence, uninstall behavior after untracked
edits, or real managed installation correctness. The plan's ordered failure/restore/replay
and managed artifact acceptance remain required before changing the default mode.

## Typed completion report

The additive observed-report APIs return InstallReport with finalized checkpoint
session/checkpoint/generation and optional ManagedRunReport. The latter retains the
parity fields ManagedModeUsed, ManifestsWritten (distinct component manifests written
by this run), ActiveProfileName, PatcherComponentNames, PatcherProvenanceRecorded, and
untracked live deletion edits with owning component, action and relative path. Only
successful actual deploy/provenance calls update these counters; simulation does not.
The same typed report is passed into Finalize for durable receipt binding before the
engine returns success. Existing Result<()> installation APIs remain wrappers.

## Checkpoint resume builder

`with_checkpoint_resume(ResumeRequest)` requires component-checkpoint mode. Under the
existing root lock, installation asks the shared store for a read-only ResumePlan
using the full currently selected definitions and exact managed participant bindings.
Its verified completed prefix supplies the engine's completed IDs before source
preflight; a frontend phase prefix may be an earlier subset, never a later boundary.
All selection, phase and profile setup remains in the existing frontends.

The plan is retained across preflight. The coordinator passes that unforgeable plan
to branch_for_resume instead of creating an ordinary legacy snapshot and new unrelated
session. The store rechecks the plan's exact observed root/auxiliary state, including
explicitly accepted changed source files, before capturing a continuation baseline.
Accepted source changes are reported with their typed change details. Normal clean
checks are bypassed only by this validated shared-store path; managed configuration
must match the source session. The original session/history is retained with explicit
lineage. Resume remains opt-in and does not run through a pending or half-restored tree.
