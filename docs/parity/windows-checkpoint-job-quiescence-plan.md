# Restart-visible Windows checkpoint process quiescence

Status: source implemented and independently reviewed on 2026-10-02. This supplements `windows-checkpoint-implementation-plan.md`. The Windows library capability and persistent process ownership now have integrated engine, central mutation guards, explicit recovery and CLI/desktop status adapters. The temporary engine rejection was removed after the ownership/ACL and combined integration reviews. Production compilation is recorded separately; native Windows interruption, restart and backend acceptance remain outstanding. The design below records the implemented protocol, not runtime proof.

## Current source and gap

`patcher/standalone/windows.rs` creates an unnamed kill-on-close job, launches a suspended process, assigns it, resumes it, and retains handles while querying ActiveProcesses during cleanup. The current owner keeps recovery ownership through ordinary error cleanup. `engine/checkpoint_coordinator.rs` marks the component Materializing before invoking instructions, and `engine.rs` invokes the shared `run_command` while the enclosing install holds recovery locks.

Abrupt parent termination bypasses Rust Drop. The root lock may become available before a restarted process knows that the previous job has drained. The unnamed job cannot be reopened. There is also a launch gap between successful suspended CreateProcessW and AssignProcessToJobObject: parent termination there can leave an unassigned suspended process. Although it has not executed user code, it is an orphan and cannot be found safely from the current checkpoint metadata.

A narrowly scoped named-job record can close these gaps for ordinary app restart in the same Windows logon/session. It is not a sandbox against a hostile process running as the same principal, not cross-logon recovery, and not an OS-crash/power-loss protocol.

## Bound record and authority

Use one bounded, strict-schema private owner record at `recovery::state_dir(roots)/external-job-v1.json`. Engine instructions launch external programs sequentially; do not build a general process registry. Record fields:

- schema version and protocol discriminator;
- canonical game/mod root binding;
- checkpoint session, component operation ID, and from-checkpoint/generation identity;
- fresh random launch UUID and independently generated random job-name nonce;
- exact `Local\\ModSync.Checkpoint.<nonce>` job name;
- creator user SID, logon SID, and Windows session ID;
- state `LaunchIntent` or `Drained`.

No API keys, raw command arguments, environment, stdout, or executable identity are needed in this record. Bound the entire document to 16 KiB and reject unknown fields, malformed IDs/names, aliases, unsafe paths, or mismatched roots. Use the checkpoint platform publication primitives. The directory/file must have a protected DACL limiting write authority to the current owner and SYSTEM; inherited broad write grants must be rejected or removed only on newly created owned artifacts. Owner/DACL verification must be handle-based and bounded.

The record is an independent process fence, not component success. Presence blocks normal install, standalone managed mutation, restore, resume, finalization, lifecycle deletion/GC, and legacy recovery until the explicit quiescence path resolves it. Read-only inventory remains available. Never let malformed/inaccessible records disappear behind a missing-record fallback. This placement is outside checkpoints-v1, avoiding accidental acceptance by its strict inventory; all game-scoped adapters must explicitly consult it.

Minimal additive API shape:

- `prepare_external_process(lock: &recovery::Lock, intent: &OperationIntent) -> ExternalProcessLease<'_>`: validates Materializing and exact lock/roots; creates the job and persists its launch record.
- `run_command_checkpointed(command, timeout, cancelled, lease: &mut ExternalProcessLease<'_>) -> Result<CommandReceipt>`: uses the already-created job, then settles and marks its record Drained before returning.
- `recover_external_process(lock: &recovery::Lock, cancelled) -> Result<QuiescenceReport>`: validates the persisted record and proves quiescence before component recovery/materialization is permitted.

These are private/additive APIs; existing standalone `run_command` signatures remain available without a checkpoint restart guarantee. Thread the borrowed lease/held-lock authority through the engine's private execution context, including nested Choose and every Execute/Run/Patcher instruction. Do not add an implicit global process owner or reacquire the already-held roots lock. A normal cleanup error must retain the lease, root lock, and fence rather than returning success.

## Fresh launch ordering

1. Under the existing game/mod locks, verify the current Materializing intent and absence of another unresolved owner record. Prepare bounded output handles and command data before any process exists.
2. Generate a fresh unguessable name. CreateJobObjectW with an explicit protected security descriptor and non-inheritable handle. Read GetLastError immediately: if ERROR_ALREADY_EXISTS, do not set limits, assign, terminate, or adopt the returned object. Close that handle and fail before launch; never reuse the name. Reject an object-type collision too.
3. On the new job only, set KILL_ON_JOB_CLOSE and forbid both breakaway flags. Query back the limits and security descriptor. Default security descriptors are insufficient for this durable authority record.
4. Publish and reread LaunchIntent containing the exact job name and binding. Only after this succeeds may CreateProcessW run. A crash before record publication leaves an empty job, so no executable writer escapes the record.
5. Use STARTUPINFOEX with **two** attributes: the existing restricted standard HANDLE_LIST and PROC_THREAD_ATTRIBUTE_JOB_LIST containing the job. CreateProcessW still uses CREATE_SUSPENDED. Assignment at process creation replaces the current separate AssignProcessToJobObject launch gap. This requires Windows 10 / Server 2016 or newer; unsupported job-list behavior fails closed without a fallback launch.
6. Establish the existing process/thread RAII ownership immediately after successful creation, verify membership if needed, and resume. Do not inherit the job handle or recovery lock handles into the child. No breakaway or alternate parent process attributes are permitted.
7. On normal completion, cancellation, or error, terminate the owned job as required and poll QueryInformationJobObject BasicAccounting until ActiveProcesses is zero, retaining the job handle. Also wait for the retained leader handle. Existing ordinary cleanup can remain unbounded on kernel uncertainty with a visible warning; it must not release owned writers merely to meet a UI timeout.
8. Publish Drained only after that proof. Remove the owner record through the capability-specific namespace boundary. Only then release the job handle and return the command receipt. If record cleanup fails, keep the fence; later recovery can safely repeat the check. Component commit/finalization remains a separate protocol.

## Restart protocol

Acquire the normal roots locks first. Read and validate the owner record before any snapshot rebase, restore, uninstall, or other game/mod mutation. Require exact user SID, logon SID, Windows session ID, root binding, and supported owner protocol. A different session or logon is an explicit blocked recovery, never evidence that a Local job is absent. This intentionally bounds v1 to same-session application restart.

If a validated Drained record exists, complete its bounded cleanup; it grants no component success. For LaunchIntent:

1. OpenJobObjectW with non-inheritable JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE | READ_CONTROL. Never CreateJobObjectW during recovery and never derive a name from a PID.
2. Only ERROR_FILE_NOT_FOUND for the exact validated same-session name is an absence candidate. Access denied, wrong object type, invalid name, unsupported namespace, or any other error keeps the fence. Microsoft documents job destruction after its handles are closed and associated processes exit; this is the source basis for accepting true absence, subject to native acceptance of the intended error path.
3. For an opened object, verify owner/protected DACL and required job limit policy before any termination. A mismatch keeps the fence and must not signal the object. Record/name/security checks establish ownership only under the existing non-hostile same-user assumption; Windows exposes no persisted arbitrary application nonce on a job that independently authenticates a malicious same-user replacement.
4. Call TerminateJobObject on the authenticated recorded job and poll ActiveProcesses to zero. Opening a recovery handle can postpone KILL_ON_JOB_CLOSE if the old parent's last handle has not closed yet, so passive polling alone is insufficient. Hold the opened handle while checking membership. Do not infer quiescence from WaitForSingleObject(job), a notification, a leader PID, or absence from a process-name listing.
5. After zero/accepted absence, publish Drained and finish owner-record cleanup before invoking ordinary checkpoint recovery. Restore the journal's committed boundary; do not infer the interrupted component succeeded.

Cancellation or a bounded recovery polling deadline may stop this restart operation with an error while retaining the owner record and leaving roots unmodified. Closing its recovery handle retains the job's kill-on-close behavior; the next attempt must prove quiescence again. A stale root lock alone never authorizes removal of this fence.

## Crash windows and limits

| Interruption point | Required restart behavior |
| --- | --- |
| Before job creation or before LaunchIntent publication | No child was created. Empty job can disappear. |
| After LaunchIntent, before CreateProcessW | Record exists; absent/empty authenticated job is safe to drain and recover. |
| During/after process creation before resume | JOB_LIST makes successful creation a member; kill-on-close/reopened job covers the suspended child. |
| During external program or its descendants | Open exact recorded job; terminate and observe zero before touching roots. |
| After zero but before Drained/record removal | Repeat zero/absence proof. |
| After record removal before component commit | Process is already drained; ordinary Materializing recovery restores previous boundary. |
| After job name disappears and a foreign object replaces it | Type/owner/policy mismatch blocks. A malicious same-principal indistinguishable replacement is outside the trust model; never advertise protection against it. |

A protected user-owned record and random name prevent accidental adoption and other-user substitution; they do not establish hostile same-user isolation. Cooperative patchers may create ordinary descendants covered by jobs. Brokered/service/WMI launches that do not belong to the job are outside the process-tree contract and require explicit unsupported classification if a backend uses them. No job policy can retroactively control such an external service.

Full native acceptance still depends on native proof of job-list assignment, exact absent-name errors/lifetime, same-session reopen/security verification, interrupted creation, output completion, and descendant drain under the relevant Windows runner configuration. Cross-logon/boot recovery is deferred and must remain an explicit limitation, not a guessed safe path. The independent Windows library lifecycle may be enabled after its persistence implementation is reviewed. The source rejection was retained until this process-fencing protocol was implemented and reviewed; the integrated boundary below records its removal. Native acceptance remains a separate gate.

## Official API basis

- [CreateJobObjectW](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-createjobobjectw): named creation may return an existing object, reports ERROR_ALREADY_EXISTS, and documents destruction ordering.
- [Job object lifecycle and process membership](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects): child membership, breakaway exceptions, accounting, and kill-on-close lifecycle.
- [OpenJobObjectW](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-openjobobjectw): exact name, access checks, and non-inheritable reopening.
- [Job object security](https://learn.microsoft.com/en-us/windows/win32/procthread/job-object-security-and-access-rights): query/terminate/READ_CONTROL and security descriptor operations.
- [Process creation attributes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute): JOB_LIST assigns jobs during creation and requires Windows 10 or Server 2016; HANDLE_LIST remains separate.

The installed windows-sys declarations expose the required job-list constant, OpenJobObjectW, token SID classes, and GetSecurityInfo under existing features. This is source evidence only.

## Exact integration ownership and mutation fences

The platform persistence owner implements the independent Windows library lifecycle, capability/location preflight, and namespace publication primitives. The process-fence owner consumes those primitives; it must not duplicate platform flush/rename code or delay library enablement until engine execution is ready. The parent-owned Windows recovery lock opener stays authoritative.

The process API belongs in a narrow recovery child module (for example `recovery/external_process.rs`) with a Windows kernel-object leaf. Record inspection must be pure file intake with no job creation or implicit termination. OS cleanup is invoked only by the explicit recovery operation or the live lease owner.

| Current entry point | Required process fence |
| --- | --- |
| `Store::require_lock` in checkpoint/store.rs | After matching-lock/location/capability checks, reject any unresolved external owner record. This centrally covers normal library mutation, without requiring process ownership for independent clean library captures. |
| `Store::capture`, `begin_session`, `begin_session_with_context`, `capture_locked` | Keep platform preflight before lock acquisition; the under-lock require_lock check prevents capturing/replacing state while previous writers remain. |
| `Store::begin_component`, `mark_component_running`, `commit_component` | Existing require_lock calls reject a leftover process fence. Creating a live external lease is allowed only after Materializing; successful lease cleanup removes its fence before commit. |
| `Store::finalize_session[_with_report]`, `recover_finalize` | Do not clear receipts/intents or mark Complete while a process fence exists. |
| `Store::restore_checkpoint`, `recover_pending`, `finish_transition`, `branch_for_resume` | Reject before verification that may lead to root mutation, snapshot rebase, or branch publication. Explicit job recovery runs first as a separate operation. |
| `Store::delete_session`, `gc`, `repair` | Their existing require_lock calls must block before catalog changes, trash renames, object replacement, or reclamation. Repair is not permission to mutate around a live process. |
| `Store::resume_plan`, inspection/list/diff/provenance | May remain read-only, but any readiness projection must include the separate process fence; a computed resume plan does not bypass the later mutation guard. |
| `recovery::ensure_checkpoint_clean`, `ensure_clean`, `begin`, legacy `restore` | Check external owner record before the early return for missing checkpoints-v1. Otherwise an interrupted process record can be silently ignored when session metadata is absent or damaged. |
| `recovery::finish`, checkpoint legacy receipt/snapshot reconciliation | Explicitly assert no process fence before clearing pending or writing completion; lower-level atomic_write/atomic_copy cannot infer game identity and are not appropriate central policy gates. |
| `checkpoint::ensure_game_checkpoint_clean` in guard.rs | Check the game-scoped recovery record before the missing-checkpoints early return and before reconstructing mods roots from sessions. A bound record contains its exact mods/game roots; never fabricate mods authority from game alone. |
| Deployment `Service::new`, `for_profile`, `lock`, ordinary mutation guard | Continue using the game-scoped guard both before constructor mutations and under the game lock. Inventory remains read-only. |
| Deployment `initialize_checkpointed` and private matching-lock bypass | Preserve exemption from the caller's own checkpoint session, but never exempt an unresolved external process fence. Patcher-managed reconciliation happens only after the lease has proved drain and removed the fence. |
| Standalone patcher with configured roots | Retain its current held-lock/ensure_clean guard. It cannot run beside a checkpoint-owned process fence. Unbound standalone calls do not receive checkpoint restart claims. |

Do not encode this record as a fake `OperationIntent` merely to reuse `pending_operations() -> Vec<OperationIntent>`: that would corrupt the existing component/transition schema. Add an independent typed `external_process_status` reader and include it in CLI/UI readiness. Missing malformed metadata must be an error, never an empty pending list.

### Avoid recovery recursion

`recover_external_process` validates the held lock, root binding, platform capability, and its own strict record directly. It must not call normal `Store::require_lock` or `ensure_clean`, which intentionally reject its unresolved record. It also must not call `recover_pending` until cleanup succeeds. The frontend recovery sequence is:

1. platform location preflight;
2. acquire matching roots lock;
3. explicitly recover/settle the external process record, if present;
4. call unchanged checkpoint `recover_pending` under the same lock;
5. report both quiescence and checkpoint outcomes independently.

If process cleanup succeeds and checkpoint recovery fails, retain that distinction; do not report the install recovered. Direct library callers of recover_pending receive a clear process-fence error until they invoke the additive cleanup API. No public bypass boolean or global exemption is needed.

### Proposed narrow implementation ownership

- Process owner: new recovery process-record/kernel-object leaves; additive shared runner lease API; Windows JOB_LIST startup attribute integration; exact record lifecycle and typed reports.
- Checkpoint persistence owner: platform helpers and existing library lifecycle. Coordinate read-only access to finalized helper APIs after that slice freezes; no overlapping edits to platform publication code.
- Engine integration owner: private execution context carrying the borrowed Materializing lease authority through component and nested option execution; remove Windows engine source gate only after the fence integration is complete.
- Parent/frontends: explicit recovery orchestration and readiness reporting; no process manipulation in rendering or read-only status paths.

All three production integrations must be source-reviewed together before Windows engine enablement. The library lifecycle and its separate native acceptance remain independently scoped.

### Persistence-owner API coordination

The library implementer confirmed these new seams during the design audit:

- `Store::prepare_write()` performs read-only qualification of known catalog locations and origins before acquiring a lock; `require_lock` repeats qualification under the lock.
- `recovery::Lock::acquire` calls `checkpoint::prepare_existing` before creating state directories. This is the central pre-lock intake/location seam. It must distinguish a status check from automatic job termination: acquiring a lock for explicit process recovery must remain possible when a valid pending process record exists. Malformed/unsafe record intake can fail before lock-directory creation, while the normal mutation prohibition remains under-lock policy.
- `checkpoint::persistence` exposes crate-private `publish_temp(NamedTempFile, &Path, replace)`, `rename(from, to, replace)`, `flush_file`, and `namespace_boundary`. The process record writer should reuse these finalized primitives.
- Platform writer origin and location qualification remain owned by the persistence module; the process module must not invent another platform capability flag.

These are coordination contracts reported by the active library owner. Implementation review must verify their final frozen signatures and call ordering before integrating the process lease.

## Preparatory source slice (historical boundary; integration recorded below)

`patcher/checkpoint_job.rs` and its private `security.rs` leaf now implement the bounded record and lease preparation/recovery protocol on Windows. `Lease::prepare(&Lock, &Roots, &OperationIntent)` compares the complete supplied intent with the single authoritative `Store::pending_operations()` result and requires Materializing. The lease borrows the roots lock. Its consuming `run` method uses a non-inheritable duplicate of the precreated job and the new runner JOB_LIST path, then verifies no active members before writing Drained and removing the record.

`recover(&Lock, &Roots, &AtomicBool)` revalidates the same stored component authority and exact current user/logon/session, then opens rather than creates the job. `ensure_clear_game(&Path)` only inspects safe namespace/record presence and is the proposed normal mutation guard hook. At this preparatory boundary the crate-private entry points were not yet wired to engine, checkpoint, recovery, deployment or frontend policy. The subsequent integrated source boundary below supersedes that temporary engine gate.

Security is handle-inspected: new record files and jobs use a protected current-user DACL. A pre-existing recovery directory may retain trusted SYSTEM/Administrators permissions and other-principal read-only grants, but foreign mutation grants are rejected. This is a deliberate ordinary-user compatibility refinement to the draft's directory protection wording; the newly published record itself remains protected. Unknown ACE forms fail closed. The module does not alter a pre-existing directory ACL.

The Windows runner keeps the existing public APIs and ordinary path behavior. Only the preparatory lease path supplies JOB_LIST; it allocates two startup attributes and keeps the job array alive through process creation. Original lease and duplicate runner handles are non-inheritable, and the runner still settles all members before returning a receipt. A failed setup leaves a recoverable fence and never marks a component complete.

Targeted formatting, source diff checks, and a byte comparison of the existing patcher inline-test suffix were performed. No tests were compiled or run; no Windows compile, native request, process launch, or runtime acceptance was performed. This paragraph records the preparatory checks; subsequent reviews and integration are recorded below.

Independent source review tightened namespace authority: every existing recovery-path ancestor is now inspected through a no-reparse handle before record absence can be accepted or a record published. Effective grants that let an untrusted principal delete/replace an ancestor or change its ACL/owner/attributes are rejected; existing ACLs are never rewritten. Ancestor owners must be the current user, SYSTEM, or Administrators. Inherit-only ACEs do not authorize changes to the existing object and are skipped for this check. Shared layouts outside these explicit bounds fail closed. This prevents another user from replacing the entire recovery directory to hide an otherwise protected record.

### Engine and frontend call graph audit

The current private engine path is `run_checkpointed -> component -> execute_observed -> process -> process_path -> standalone::run_command`; nested Choose calls `component` recursively. The borrowed Materializing authority must follow this entire chain. Prepare a fresh consuming Lease immediately before each actual external launch, not once per component: one component can launch multiple processes. Keep virtual/preflight calls on the existing non-launch path. After lease.run returns, preserve the existing diagnostic collection, patcher-log validation, managed reconciliation, and VFS scan ordering. A drained process is not a successful patcher install.

Current CLI recover is `checkpoint_cli.rs::Action::Recover`; desktop recover is `ui/checkpoints.rs::Action::Recover`. Both already perform prepare_write, acquire Lock, then recover_pending. The process module is crate-private and cannot be called directly from the binary crates. Add a library recovery facade (or an additive Store method) that validates any selected session against the process-record authority, settles the job, then invokes normal recover_pending under that same lock. Do not terminate a different session's process merely because the requested session is invalid. Return an additive combined report if callers need both outcomes; keep existing report fields/signatures intact.

The normal Store::require_lock guard must reject an unresolved process record, while the explicit facade's quiescence step uses the lease module's separate lock/authority validation. Its prepare_write call must remain read-only capability qualification and must not itself reject every valid pending job, or explicit recovery would deadlock behind its own fence. CLI/UI readiness must combine checkpoint pending state with the additive process-fence status reader; no rendering path opens or terminates jobs.

The inherit-only exception applies only to ancestor authority checks. The actual recovery directory also rejects untrusted inheritable mutation grants, preventing a foreign process from obtaining a writable temporary-file handle between tempfile creation and its protected-DACL installation. Tightening a DACL would not revoke an already-open handle, so that creation window cannot be ignored.

## Integrated source boundary

The engine retains the roots lock through an `Arc` and binds the persisted Materializing intent around actual component execution. Nested Choose inherits this private context; every actual Execute/Run/Patcher launch gets a fresh consuming lease. Participant locations are qualified before Windows lock creation. Existing diagnostic collection and success-only managed reconciliation run after process settlement.

Normal Store mutations, game-only deployment admission and legacy recovery admission check the independent record before missing-checkpoint shortcuts. Capability preparation remains read-only and permits explicit recovery lock acquisition. `Store::external_process_status` validates the record without kernel actions. `recover_with_process_quiescence` checks selected session and operation authority before signals, then invokes normal checkpoint recovery under the same lock. Its additive report retains process settlement when checkpoint recovery fails. CLI prints the report before returning a nonzero status; desktop readiness includes the process fence and cannot resume sharing on incomplete recovery.

Reviews resolved ancestor-directory deletion/replacement authority and inherited foreign-write grants before temporary-file DACL protection. No existing ACL is silently modified. Source review is complete for this boundary; it is not Windows runtime acceptance or a power-loss guarantee.
