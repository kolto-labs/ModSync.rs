# Widescreen phase investigation

Scope remains full C# parity. This investigation changes the next execution task: establish a live phase oracle before splitting the Rust engine/UI. The pinned reference has conflicting UI and execution boundaries. Source hashes and commit are recorded in `widescreen-source-trace.json`.

## Confirmed source behavior

- `InstallWizardDialog.axaml.cs:135`: detects all WidescreenOnly components. After BaseInstallCompletePage, inserts optional warning, widescreen selection, widescreen installing and completion before FinishedPage. The original list is retained by the constructor.
- `InstallingPage.axaml.cs:202`: base progress counts exclude widescreen. At line 291 the install service receives `_allComponents`, not the filtered local list.
- `InstallationService.cs:1180`: selection is by IsSelected alone. The coordinator receives all components; returned components are again filtered only by IsSelected. Completed entries are skipped in this bulk loop.
- `Installation/InstallCoordinator.cs:35`: initialization returns either the input list or the dependency-ordered list. Neither branch removes widescreen components.
- `WidescreenModSelectionPage.axaml.cs`: preserves the supplied selections, permits zero selection, and disallows back/cancel navigation. The summary updates on page entry.
- `WidescreenInstallingPage.axaml.cs:87`: iterates selected widescreen components in supplied order and calls InstallSingleComponentAsync with the widescreen list as dependency context. A non-success result blocks completion. Zero selected components completes without installation.
- `InstallationService.cs:1102`: single-component execution validates, checks FOMOD configuration, then calls component.InstallAsync without a completed-state guard.
- `ModComponent.cs:841`: InstallAsync resets state to Running and executes instructions. It does not skip a previously Completed component.

## Inference, not runtime proof

A widescreen component already selected before base installation can reach both execution paths. Assuming validation permits both invocations, it can run twice. The second invocation also receives only widescreen components as dependency context. Base dependencies therefore need explicit investigation. Neither duplicate execution nor dependency failure has been reproduced live in this investigation.

Do not claim that the C# base engine already excludes widescreen just because its counter and selection page do. Do not add a Rust second pass that blindly replays components already installed in the first pass. The user requires ordered installation and no late backfill; existing Rust behavior remains unchanged during this investigation.

## Next executable oracle and implementation requirements

1. Use isolated source/game directories and a tiny guide containing one base Copy and one initially selected widescreen action with an observable second-execution effect. Keep telemetry/network disabled or isolated as in existing reference fixtures.
2. Invoke the same bulk and single-component service entry points used by the wizard, capture invocation counts, resulting bytes, InstallState and exit codes. Include initially unselected widescreen, zero selected widescreen, base-dependency and failure cases.
3. Preserve the observed reference discrepancy separately from intended phase behavior. Any correction to double execution must be documented rather than presented as literal observed parity.
4. Implement base completion, optional notice, widescreen selection/skip and widescreen completion as explicit transitions. Preserve full dependency knowledge while scoping execution, preserve guide order within the authorized phases, and prevent base replay.
5. Bind recovery to the phase boundary: a widescreen failure must restore the verified pre-widescreen snapshot, retain the completed base install, and prohibit continuing past the failed step. No full parity claim until isolated failure/retry and native navigation are verified.

The Rust single selection page and engine still execute a single guide. Existing grouping/count/filter checks do not prove phased installation, resume or widescreen correctness.

## Live service reproduction

`widescreen-oracle/` contains the probe, project, output, reference log and DLL-bound receipt. The bulk service installed the initially selected widescreen Copy and marked it Completed. After changing only the source bytes from `first` to `second`, the single-component service installed it again and returned Success. Default bulk ordering was used. NoCheckpoint was enabled to avoid unrelated backup machinery. Network and host writes were isolated with bubblewrap.

This upgrades the double-execution finding from source inference to service-level reproduction. It does not prove GUI navigation, checkpoint behavior, initially unselected/zero-selection behavior, or cross-phase dependencies. The net9 attempt failed on a DiagnosticSource version conflict; the successful probe ran under installed .NET 10.0.11 against the same reference DLL.

The Rust phase implementation must prevent this observed duplicate execution, explicitly documenting it as a reference defect correction under the user's guide-order/no-backfill constraint. Next work remains the untested dependency and failure cases, then phase-scoped execution with completed-base context and verified recovery boundaries.

## Live dependency-context reproduction

`widescreen-dependency-oracle/` adds a wide component initially unselected and dependent on the base GUID. Bulk installation succeeded without creating the wide file; its state stayed Pending. Selecting it and invoking the single service with only the widescreen list returned DependencyViolation and created no file. Passing the full component list to the same service then returned Success and wrote the expected bytes. The runtime/isolation/DLL binding is recorded in the receipt, with a result assertion in verify.py.

This confirms that execution membership and dependency context must remain distinct in the Rust phase implementation. The base install must not be replayed to satisfy a wide dependency. Initial deselection is also observed to defer wide execution in this fixture. Zero-selection completion, failures and checkpoint recovery remain unverified.
