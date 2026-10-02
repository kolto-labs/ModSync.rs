# F09 patcher parity plan

## Status

This is an implementation and proof plan, not an implementation claim. It covers F09 external execution and patcher orchestration plus the F28 packaging slice needed to make a backend available in a produced artifact (`docs/parity/ACCEPTANCE.md:44`, `docs/parity/ACCEPTANCE.md:63`, `docs/parity/ACCEPTANCE.md:316-324`). The audited oracle is pinned at `d31ccbd3bfcddb4705917eafa6483715c166de3b`; the findings and source hashes are in `docs/parity/patcher-csharp-audit.md`.

F09 remains `partial` until the final gates below pass. Existing argument and log checks do not prove a packaged native backend or patch-byte equivalence (`COMPATIBILITY.md:115-119`, `COMPATIBILITY.md:149-152`; `docs/parity/ACCEPTANCE.md:121`).

## Binding constraints

1. Keep all CLI and desktop installation behavior in `src/engine.rs`; `src/patcher.rs` may own backend identity, discovery, and command preparation, but presentation adapters may not implement an alternate install path (`AGENTS.md:5`; `src/lib.rs:16-22`).
2. Never use a user's game, baseline, or mod tree. Every external-execution probe starts from a new isolated copy, and every retry restores the verified snapshot rather than backfilling an earlier guide step (`AGENTS.md:7`, `AGENTS.md:9`).
3. Validation remains read-only and must not launch a backend. Backend execution is allowed only in an explicitly authorized install or acceptance probe (`AGENTS.md:8`; `src/engine.rs:864-873`).
4. Preserve guide and source order, stop on the first failure, and do not claim success after a partial sequence (`AGENTS.md:9`; C# oracle: `src/ModSync.Core/Instruction.cs:1298-1306`, `src/ModSync.Core/Instruction.cs:1469-1515`).
5. External execution remains denied unless the caller explicitly enables it (`src/engine.rs:1550-1554`; `COMPATIBILITY.md:128-129`). Packaged availability must not weaken that gate.
6. Reject unsafe paths, unsupported instructions, missing logs, and unavailable backends. Preserved metadata, a virtual install, a zero exit code, or matching argv is not full parity (`AGENTS.md:10`; `docs/parity/ACCEPTANCE.md:121`; C# oracle: `src/ModSync.Core/Instruction.cs:1427-1448`).
7. Do not create, modify, or run Rust tests or test harnesses during this parity phase (`docs/parity/ACCEPTANCE.md:26-27`). Acceptance uses the production CLI, production desktop application, real backend executables, and direct native probes.
8. Future Rust build and lint commands run on the host with an isolated target directory and `-j 2`; no Cargo command runs in the saturated container. This plan does not run a build.
9. Update `COMPATIBILITY.md` and `docs/parity/ACCEPTANCE.md` only after the corresponding proof boundary changes and receipts exist (`AGENTS.md:11`; `docs/parity/ACCEPTANCE.md:28-32`).
10. Do not publish an artifact, release, or pull request as part of this work (`AGENTS.md:12`).

`AGENTS.md` line numbers in this plan refer to the committed 12-line file (`git show HEAD:AGENTS.md`), not to any modified working copy.

## Prerequisite: durable C# reference build

The prebuilt C# oracle used by P07 currently sits on tmpfs at `/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync/debug_net9.0` (`docs/parity/validation-reference/README.md:72-73`). `/tmp` is `tmpfs` on this host and does not survive a reboot. Before any P-series probe that runs C#, the Phase 0 owner must:

1. Copy that `debug_net9.0` tree, unchanged, to `/home/brunner56/.cache/modsync-csharp-ref/d31ccbd3/debug_net9.0/` on the NVMe home disk.
2. Write `/home/brunner56/.cache/modsync-csharp-ref/d31ccbd3/SHA256SUMS` covering every regular file in the copy.
3. Check that the copied `ModSync.dll`, `ModSync.Core.dll`, `Resources/holopatcher` and runtime host match `docs/parity/validation-reference/receipt.json:13-18`. Any mismatch stops the plan.
4. Record the copy path, the `SHA256SUMS` hash and the source commit `d31ccbd3bfcddb4705917eafa6483715c166de3b` in the Phase 0 receipt.

Every later C# run uses the durable copy, never the `/tmp` path. This plan does not perform the copy.

## Lead decisions

### Shared library owns behavior

`src/patcher.rs` will expose a shared backend contract: backend identity, configured-path validation, discovery result and provenance, packaged-asset metadata, and a structured command specification. `src/engine.rs` will remain the owner of install sequencing, temporary staging, process lifecycle, cancellation, log verification, rescan, and error mapping. This keeps one implementation behind CLI and desktop surfaces (`AGENTS.md:5`; current boundaries: `src/patcher.rs:8-79`, `src/engine.rs:1549-1733`, `src/main.rs:924-947`, `src/ui.rs:1454-1495`).

The minimum directional library API is:

```rust
PatcherBackend::resolve(&PatcherSettings, &ArtifactContext)
    -> Result<ResolvedPatcher, PatcherResolutionError>

ResolvedPatcher::command_for(&PreparedPatchSource, &Path, NamespaceSelection)
    -> Result<PatcherCommand, PatcherCommandError>

Engine::process(...)
    -> existing engine result and event stream
```

The names are directional, not frozen signatures. The important contract is that UI/CLI code supplies settings and renders shared results; it never reconstructs discovery order, argv, or log semantics (`src/main.rs:321-333`, `src/main.rs:382-398`, `src/ui.rs:1454-1495`; C# oracle separation: `src/ModSync.Core/Services/InstallationService.cs:617-790`).

### Stronger Rust safety rules remain deliberate differences

An explicitly configured but missing backend path will continue to fail instead of silently selecting another executable. The current Rust resolver canonicalizes that path and errors immediately (`src/patcher.rs:20-30`), whereas C# warns and continues for KPatcher and OdyPatcher (`src/ModSync.Core/Services/InstallationService.cs:617-745`). Fail-closed behavior is more predictable for an external-execution trust boundary.

Validation will not copy C#'s backend-launch checks (`src/ModSync.Core/Services/InstallationService.cs:816-923`). It will report source-derived availability and defer executability to an explicitly authorized probe (`AGENTS.md:8`; `src/engine.rs:864-873`).

Rust will continue to cap captured process output and support cancellation (`src/engine.rs:1653-1684`). Those safeguards are allowed differences from the C# wait path (`src/ModSync.Core/Utility/PlatformAgnosticMethods.cs:434-517`) as long as truncation is surfaced and does not hide the authoritative patcher log.

### Source staging replaces oracle mutation

Each patcher source will be copied into an isolated staging directory before control-key changes, named-INI substitution, log deletion, or backend launch. The staged copy must include every directory P11 shows a backend writes its install log to; if a backend writes the log to the parent of the patch data, the stage root is that parent. The original mod tree remains byte-identical. C# rewrites its selected INI and deletes old logs in place (`src/ModSync.Core/Instruction.cs:1328-1342`); the staged copy preserves the resulting backend inputs without violating the Rust isolation rules (`AGENTS.md:7-8`).

Numeric instruction arguments remain the C# namespace-selection contract (`src/ModSync.Core/Instruction.cs:1369-1373`). Safe named `.ini` selection remains a documented Rust extension using staged `changes.ini` replacement (`src/engine.rs:1593-1638`). It is a deliberate deviation from C#, which forwards every nonblank value as a namespace index. It gets its own receipt, P12, stored apart from exact-C# parity receipts.

### Packaged native backend is the release baseline

A release artifact must contain a pinned native backend for each supported artifact target, plus a manifest containing backend name, version/source identity, SHA-256, license reference, target triple, relative installed path, and expected probe behavior. Resolution will consume that manifest before generic application-resource candidates. Application/resource discovery already exists, but Cargo currently supplies no artifact-staging contract (`src/patcher.rs:68-79`; `Cargo.toml:13-47`).

The manifest and staged files are packaging inputs, not evidence by themselves. Acceptance must unpack the final artifact on a clean host, find the backend with no configured path and a sanitized `PATH`, run its explicit probe, and retain the executable hash and output. The C# copy rules demonstrate why source and build configuration alone are insufficient (`src/ModSync.GUI/ModSync.csproj:174-301`; `scripts/agents/README.md:134-148`).

### Python fallback must be offline or remain unsupported

The C# route can provision Python and install missing dependencies with pip at runtime (`src/ModSync.Core/Services/InstallationService.cs:89-246`). Rust must not reproduce that network mutation. A Python fallback counts only if the interpreter, HoloPatcher/PyKotor source, dependencies, licenses, and hashes are included in or deterministically staged into the artifact, and the fallback passes the same patch-byte differential as the native backend (`src/ModSync.Core/Services/InstallationService.cs:345-566`; `src/ModSync.GUI/ModSync.csproj:218-231`; `AGENTS.md:7-10`).

Until that offline bundle exists and is proven, native HoloPatcher is required and the Python fallback is reported as unsupported. The fallback may not be inferred from preserved Python metadata or an importable source tree (`docs/parity/ACCEPTANCE.md:121`).

## Implementation sequence and serialized ownership

Only one owner may write a listed file at a time. The current owners of `src/ui.rs`, `src/main.rs`, `src/acquisition.rs`, `src/p2p.rs`, `src/merge.rs`, `src/telemetry.rs`, `src/profiles.rs`, `src/markdown.rs`, `src/deployment.rs`, `src/selection.rs`, and `src/recovery.rs` retain those lanes. Patcher work must wait for an explicit handoff before touching `src/ui.rs`, `src/main.rs`, or `src/profiles.rs`; it must use reviewed APIs rather than edit any of the other occupied lanes.

### `src/engine.rs` serialized order

`src/engine.rs` has one writer at a time. This order is shared with `docs/parity/engine-actions-archives-plan.md` and is binding for both plans:

- **E0: install-progress Task 1.** Already landed; `origin/main` is at `cf64f53`.
- **E1: patcher Phase 1.** Owns `process()` (about `src/engine.rs:1549-1733`), including `Execute`/`Run` placeholder expansion and ordered multi-source execution.
- **E2: deployment staging hook.** Adds `Settings.managed`, the `put`/`delete`/`actual` redirects (about `src/engine.rs:1238-1313`) and the barrier around `process()`.
- **E3: engine U5.** One action group per commit. Each commit is probed in classic and managed modes.
- **E4: patcher Phase 6.** Writes `src/deployment.rs` and `src/recovery.rs` only; no `src/engine.rs` write.

A phase starts only after the previous owner releases the file and the integration owner records the new base SHA.

| Phase | Exclusive owner and writable files | Work | Exit gate |
| --- | --- | --- | --- |
| 0. Freeze and handoff | Integration owner; no production writes | Make the durable C# reference copy and its `SHA256SUMS` (prerequisite section). Record Rust and C# SHAs, dirty paths, backend asset hashes, available disk, and lane owners. Obtain handoff for any occupied adapter file before scheduling that phase. | Receipt names exact bases and proves no overlapping writer. |
| 1. Shared contract (E1) | Patcher owner: `src/patcher.rs`; then, as E1, the engine owner for `process()` in `src/engine.rs` | Add typed resolution provenance and command preparation. Then, in `process()`: ordered multi-source execution for Patcher, `Execute` and `Run`; `Execute`/`Run` placeholder expansion; isolated source staging; fresh-log enforcement; cancellation/error mapping; and rescan. Engine U5 does not touch `process()`. | Direct production-CLI probes cover argv, order, failure, source immutability, log semantics, and cancellation; no Rust test is added or run. |
| 2. Artifact packaging | Packaging owner: `Cargo.toml`, new packaging manifest/assets/scripts, and the packaging workflow only | Pin legal native assets by target, stage them next to the application, emit an artifact manifest, and fail packaging on a missing/hash-mismatched backend. Do not fetch `latest` at runtime. | Each unpacked artifact passes sanitized-environment discovery and the backend's direct native probe; receipts include artifact and backend hashes. |
| 3. Python fallback | Packaging owner, serialized after phase 2; shared patcher/engine owners only for reviewed API changes | If licensing and size are approved, add an entirely offline Python/HoloPatcher bundle and a shared fallback route. Never invoke pip or a network client during install. | Native backend removed from a copied artifact, network disabled, fallback discovers and produces the same C# differential result. Otherwise record the route as unsupported and leave F09 partial for this item. |
| 4. CLI/profile adapter | Existing `src/main.rs` and `src/profiles.rs` owners after explicit handoff | Consume shared availability/provenance errors and persist all backend paths without reconstructing engine behavior. Preserve direct-Markdown's OdyPatcher requirement. | CLI settings round-trip and direct install probes use the shared route (`src/main.rs:924-947`, `src/main.rs:1023-1027`, `src/profiles.rs:352-394`). |
| 5. Desktop adapter | Existing `src/ui.rs` owner after explicit handoff | Render backend, configured path, resolved source/provenance, and actionable failure. Invoke the same engine route as CLI. | Native desktop save/reload/install probe uses the shared engine and displays missing backend, nonzero exit, missing log, and log error distinctly (`src/ui.rs:1454-1495`; shared behavior boundary `AGENTS.md:5`). |
| 6. Recovery integration (E4) | Existing deployment/recovery owners; `src/deployment.rs` and `src/recovery.rs` only, after their handoff and after E2 lands | Build on the deployment lane's patcher barrier; do not design a second one. The contract is the deployment audit (`git -C /home/brunner56/.cache/modsync-wt-deploy show HEAD:docs/parity/deployment-csharp-audit.md`, branch `lane-deployment-f18` at `fa3eab3`): §4 rule 5 (line 119) and the §6 engine call order (lines 215-225), which is `deploy_append` → `capture_index` → process → `record_live` → `scan`. E2 puts those calls around `process()`; Phase 6 only finishes the deployment/recovery side, such as the rule 8 recovery journal (line 134). Do not create a patcher-only recovery path. | In managed mode, a staged Copy followed by a patcher step shows the patcher reading the deployed bytes. `record_live` records the patcher-changed files. A forced failure after a patch restores the verified snapshot before retry and never backfills a skipped source (`AGENTS.md:9`; C# provenance reference `src/ModSync.Core/Services/Installation/ManagedInstallSession.cs:202-290`). |
| 7. Documentation and ledger | Documentation owner after all receipts are immutable: `COMPATIBILITY.md`, `docs/parity/ACCEPTANCE.md`, and the evidence index | Link exact receipts, record supported targets and deliberate differences, and change F09/F28 status only to the strength actually proven. | Every checked ledger statement resolves to artifact, command, log, manifest, and byte-diff receipts (`docs/parity/ACCEPTANCE.md:28-32`). |

Phases 1 and 2 may be researched in parallel, but their writes and acceptance runs are serialized because both affect executable resolution and artifact layout. Phases 4-6 wait for current lane owners. Patch-byte runs are always serialized against one fresh baseline; parallel workers are permitted only for immutable manifest comparison after all runs finish (`AGENTS.md:7-9`).

## Phase 1 details: shared execution semantics

### Ordered sources and generic executables

Replace the current exactly-one-source requirement (`src/engine.rs:1555-1558`) with an ordered loop over resolved sources. Patcher, `Execute`, and `Run` stop at the first source that cannot be prepared, launched, or verified. The next guide instruction is not attempted after failure, matching the oracle's source loop and project retry rule (`src/ModSync.Core/Instruction.cs:1298-1306`, `src/ModSync.Core/Instruction.cs:1469-1515`; `AGENTS.md:9`).

For `Execute`/`Run`, expand the same documented placeholders for each resolved source before tokenizing arguments. Continue to pass an executable and structured argument vector to `Command`; do not regress to shell evaluation (`src/engine.rs:1642-1645`; C# placeholder behavior: `src/ModSync.Core/Instruction.cs:1497-1515`).

### Patcher preparation

For each source:

1. Resolve a directory or its parent using the current source rule (`src/engine.rs:1579-1587`).
2. Copy only the complete patcher root needed for relative reads into an isolated temporary directory; reject traversal, links that escape the root, special files, and oversized policy violations (`AGENTS.md:7-10`).
3. Delete staged `installlog.rtf` and `installlog.txt` in every directory P11 shows the backend writes to, never source logs (`src/engine.rs:1647-1651`; C# behavior: `src/ModSync.Core/Instruction.cs:1328-1338`, which cleans only the patch directory).
4. Apply C# control keys to the staged selected INI so the backend uses the provided game path, emits plaintext output, and does not prompt (`src/ModSync.Core/Instruction.cs:1340-1342`).
5. Apply either the common numeric namespace choice or the separately labeled named-INI extension (`src/engine.rs:1589-1638`; C# contract: `src/ModSync.Core/Instruction.cs:1369-1373`).
6. Launch the resolved backend with structured arguments generated from one shared command specification, in the C# order: KPatcher's non-Windows `--console` first, then `--install --game-dir <game> --tslpatchdata <data>`, then OdyPatcher's `--cli -y`, then `--namespace-option-index <n>` (`src/engine.rs:1700-1715`, `src/engine.rs:1589-1591`; C# order: `src/ModSync.Core/Instruction.cs:1354-1373`; KPatcher prefix: `src/ModSync.Core/Services/InstallationService.cs:771-787`).

### Completion and logs

Remove the current success path that accepts no log (`src/engine.rs:1718-1733`). A patcher source succeeds only when the process exits zero, a new readable `installlog.rtf` or `installlog.txt` exists in the staged directory P11 measured for that backend, and neither C# error marker appears (`src/ModSync.Core/Instruction.cs:1427-1448`, `src/ModSync.Core/Instruction.cs:1556-1603`). The result records which log was authoritative and its hash.

The freshness rule is a **deliberate deviation from C#**, not parity. C# deletes logs in the patch directory (`src/ModSync.Core/Instruction.cs:1328-1338`) but `VerifyInstall()` reads the parent of every source after each source (`:1435`, `:1563-1566`, `:1579-1591`). For a directory source, a stale parent log can therefore satisfy C#. Rust today reads only the directory it cleaned (`src/engine.rs:1722`). The receipt for this rule is P05 plus P11, filed as a deviation.

If captured stdout/stderr reaches the 1 MiB limit, emit a truncation event while still retaining the authoritative full patcher log (`src/engine.rs:1672-1684`). Cancellation kills and waits for the child, reports cancellation, and enters the normal restore path before any retry (`src/engine.rs:1653-1671`; `AGENTS.md:9`).

## Phase 2 details: packaging and discovery

The packaging manifest must be target-specific and fail closed. At minimum it contains:

- application version and Rust commit;
- backend kind and backend version/source commit;
- artifact target and relative executable path;
- backend SHA-256 and size;
- license identifier and bundled license path;
- whether the executable needs an auxiliary resource directory;
- the exact explicit probe and accepted exit-status contract.

This metadata closes gaps left by location-only discovery (`src/patcher.rs:31-79`) and by platform copy clauses whose coverage does not equal the declared C# runtime list (`src/ModSync.GUI/ModSync.csproj:48-60`, `src/ModSync.GUI/ModSync.csproj:233-301`).

Discovery order will be: explicit configured path; verified manifest-backed packaged backend; legacy application/resource candidates; PATH only for backends whose policy allows it. The result exposes both path and provenance. HoloPatcher PATH behavior must not silently change because the current resolver deliberately disables it for the HoloPatcher candidate list (`src/patcher.rs:45-57`, `src/patcher.rs:59-67`).

The artifact build fails if a manifest entry is missing, points outside the package, has a mismatched hash, lacks its license, or is not executable on the target. Runtime also verifies the manifest before first execution; a mismatch is a backend-availability error, not permission to fall through to an unpinned executable (`AGENTS.md:10`; current configured-path fail-closed precedent: `src/patcher.rs:20-30`).

## Acceptance probes and durable receipts

No item in this section is a Rust test or harness. Each probe drives the production CLI, production desktop application, or packaged backend directly. All filesystem mutations occur under a newly created isolated root (`AGENTS.md:7-8`; `docs/parity/ACCEPTANCE.md:26-27`).

Receipts live at:

```text
docs/parity/evidence/patcher/<rust-commit>/<probe-id>/
```

Every probe directory contains `command.txt`, `environment.txt` with secrets removed, input and executable hashes, stdout/stderr, exit status, patcher log, pre/post manifests where relevant, and a short `verdict.md`. Artifact probes also retain the artifact SHA-256 and unpacked backend manifest. This is a proposed evidence path; no receipt exists until the command has run.

### P01 — discovery and provenance matrix

For each backend, run the production CLI with: a valid explicit path; an absent explicit path; a valid manifest-backed packaged path under a sanitized `PATH`; a hash-mismatched packaged file; a legacy resource candidate; and an allowed PATH candidate. Verify the exact selected path and provenance or the exact failure. This exercises current resolution branches (`src/patcher.rs:20-79`) and the new manifest branch.

### P02 — argv matrix

Use an isolated executable that records its direct argv and emits a valid fresh log. Drive a production install for all three backends, with and without a numeric namespace. Assert the full argv as an ordered list, not as a set:

- HoloPatcher: `--install --game-dir <game> --tslpatchdata <data> [--namespace-option-index <n>]`
- KPatcher, non-Windows: `--console --install --game-dir <game> --tslpatchdata <data> [--namespace-option-index <n>]`
- OdyPatcher: `--install --game-dir <game> --tslpatchdata <data> --cli -y [--namespace-option-index <n>]`

A present-but-misplaced argument fails the probe. Rust order: `src/engine.rs:1700-1715`, `src/engine.rs:1589-1591`. C# order: `src/ModSync.Core/Instruction.cs:1354-1373`; KPatcher prefix `src/ModSync.Core/Services/InstallationService.cs:771-787`. The OdyPatcher runner at `InstallationService.cs:753-769` only trims. This proves orchestration only, not patch semantics.

### P03 — ordered multi-source and placeholder execution

Provide two isolated external sources that append their canonical identity and expanded arguments to a receipt. Confirm source order and per-source placeholder values. Make the first source fail and prove the second source and next guide instruction did not run. This closes the current one-source gap (`src/engine.rs:1555-1558`) against the C# loops (`src/ModSync.Core/Instruction.cs:1298-1306`, `src/ModSync.Core/Instruction.cs:1469-1515`).

### P04 — trust, path, and staging safety

Prove external execution is rejected by default; traversal, escaping links, unsafe named INIs, and unsupported source forms are rejected; and successful plus failed runs leave the source mod tree's SHA-256 manifest unchanged (`src/engine.rs:1550-1564`, `src/engine.rs:1593-1614`; `AGENTS.md:7-10`).

### P05 — exit and log truth table

Drive the production path through: nonzero exit with a clean log; zero exit with no log; zero exit with each C# error marker; zero exit with a newly created clean RTF log; zero exit with a clean text log; a stale preexisting source log but no staged log; and, for a directory source, a stale clean log in the source's parent, which C# `VerifyInstall()` reads (`src/ModSync.Core/Instruction.cs:1579-1591`). Only the two clean, fresh-log cases may succeed; the parent-log case is where the deliberate freshness deviation shows (`src/engine.rs:1685-1693`, `src/engine.rs:1718-1733`; C# oracle: `src/ModSync.Core/Instruction.cs:1427-1448`, `src/ModSync.Core/Instruction.cs:1556-1603`).

### P06 — cancellation and restore

Cancel a backend after it changes one isolated game file but before completion. Prove the child was reaped, no later source ran, and the verified snapshot was restored before retry. Retry from the original snapshot and compare the successful result with a fresh control install (`src/engine.rs:1653-1671`; `AGENTS.md:9`).

The production CLI cannot drive this today. `install` builds `Settings::new` and never sets `settings.cancelled` (`src/main.rs:1061-1071`), and `src/main.rs` installs no signal handler (no `ctrl_c`, `ctrlc` or `signal` match in the file). Route P06 through the production desktop app, whose run shares `settings.cancelled` and whose "Stop install" button sets it (`src/ui.rs:2369`, `src/ui.rs:3149`). A CLI variant of P06 is optional and depends on a `src/main.rs` cancel hook with its own owner. Download-orchestration Task 2a adds Ctrl+C wiring (`docs/parity/download-orchestration-plan.md:332`, `:364`), but it leaves existing commands unchanged, so extending it to `install` needs a separate handoff from the `src/main.rs` owner.

### P07 — same-backend C# versus Rust patch-byte differential

For each accepted backend and each K1/K2 fixture:

1. Verify and record one pristine baseline manifest.
2. Restore two independent game copies from that baseline.
3. Give each run its **own** copy of the mod tree, made from one frozen source and hashed before the run. C# rewrites the selected INI and deletes logs in place (`src/ModSync.Core/Instruction.cs:1328-1342`), so a shared mod tree would let one run change the other's input. Retain both pre-run and post-run mod-tree manifests.
4. Give each C# run an isolated `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` and settings directory, newly created under the probe root. The C# runs never read the user's profile or settings.
5. Run the pinned C# oracle, from the durable copy in the prerequisite section, on game copy A with mod copy A. Run the candidate Rust production CLI on game copy B with mod copy B. Use the same guide instruction, backend executable, backend hash and namespace choice.
6. Record argv, stdout/stderr, install logs, and post-run manifests.
7. Compare sorted relative paths, file types, sizes, and SHA-256 values. For every mismatch retain both files and a machine-readable diff index.

Pass requires identical post-run manifests and zero differing bytes for the same backend. A cross-backend comparison may be retained as diagnostic evidence but cannot replace this same-backend oracle comparison because backend implementations or versions can legitimately differ (`docs/parity/ACCEPTANCE.md:121`, `docs/parity/ACCEPTANCE.md:316-324`; C# execution route: `src/ModSync.Core/Instruction.cs:1289-1457`; Rust route: `src/engine.rs:1549-1733`).

The fixture must exercise more than file copy: at least one structured binary patch, one table/text patch, one script or compiled-resource output where the backend supports it, and one namespace selection. Fixture inputs and expected changed paths are frozen and hashed before either run. A successful process or matching generated-path list alone is insufficient (`docs/parity/ACCEPTANCE.md:121`; `COMPATIBILITY.md:115-119`).

### P08 — unpacked artifact availability

On every supported artifact target, unpack the final artifact into a clean directory, clear configured backend paths, sanitize `PATH`, and use the production application to resolve the packaged backend. Then run the manifest-declared direct native probe and one production patch. Retain artifact, manifest, executable, and output hashes. This is the proof missing from location-only source inspection (`src/patcher.rs:68-79`; C# packaging caution: `src/ModSync.GUI/ModSync.csproj:174-301`).

### P09 — offline Python fallback

If the fallback is implemented, copy an unpacked artifact, remove only its native HoloPatcher, disable network access, and run discovery plus the P07 differential through the bundled Python route. Prove no cache or user Python installation was consulted. If this probe is absent, the ledger must say Python fallback is unsupported (`src/ModSync.Core/Services/InstallationService.cs:89-246`, `src/ModSync.Core/Services/InstallationService.cs:345-566`; `AGENTS.md:10`).

### P10 — native desktop path

In the production desktop application, select every backend, save and reload its configured path, run an isolated install, and exercise missing-backend, nonzero-exit, missing-log, and log-error states. Confirm the resulting bytes and events match the CLI shared-engine run. A screenshot proves presentation only; the receipt also requires logs and manifests (`src/ui.rs:1454-1495`; shared installation boundary: `AGENTS.md:5`).

### P11 — patcher log location

For each real backend (HoloPatcher, KPatcher, OdyPatcher), run one clean patch from a fresh isolated copy twice: once with the source as the patch directory and once with the source as an `.exe` inside it. Before each run, record a manifest of the patch directory, its parent, the `tslpatchdata` subfolder, the child's working directory and the game root. After the run, record which of those directories gained `installlog.rtf` or `installlog.txt`, with hashes. Run the same matrix through the C# oracle and record which log `VerifyInstall()` read (`src/ModSync.Core/Instruction.cs:1579-1591`). The result sets the stage root and the log path Rust checks. Until P11 has run, the log location is unverified.

### P12 — named-INI extension (deviation receipt)

Named-INI selection has no C# counterpart; C# forwards any nonblank value as `--namespace-option-index` (`src/ModSync.Core/Instruction.cs:1369-1373`). Receipts live apart from parity receipts, at `docs/parity/evidence/patcher/<rust-commit>/deviations/named-ini/`. Drive a production install with a safe named INI. Prove that the staged `changes.ini` equals the named file, that `namespaces.ini` was removed from the stage only, and that the source tree is unchanged. Prove that traversal, a non-`.ini` name and a missing INI are rejected (`src/engine.rs:1593-1638`). The verdict file states "deliberate deviation", not "parity".

## Review gates

### Gate A — contract and source review

- Reconcile every command shape and discovery branch with `docs/parity/patcher-csharp-audit.md`.
- Confirm CLI and desktop contain no alternate execution logic (`AGENTS.md:5`; `src/main.rs:924-947`; `src/ui.rs:1454-1495`).
- Confirm validation remains non-executing (`AGENTS.md:8`; `src/engine.rs:864-873`).
- Confirm named INI is labeled as an extension, not C# parity (`src/engine.rs:1593-1638`; C# oracle: `src/ModSync.Core/Instruction.cs:1369-1373`).

### Gate B — safety and recovery review

- Review trust gating, canonicalization, link handling, staging limits, child termination, output truncation, log freshness, source immutability, and snapshot restoration (`src/engine.rs:1550-1697`; `AGENTS.md:7-10`).
- Force each P04-P06 failure before any real patch corpus run.
- Reject any retry that reuses a partially changed baseline (`AGENTS.md:9`).

### Gate C — packaging review

- Review licenses and redistribution authority for every bundled interpreter, backend, and dependency.
- Compare manifest targets with the actual release matrix; declarations without produced artifacts do not pass (`src/ModSync.GUI/ModSync.csproj:48-60`, `src/ModSync.GUI/ModSync.csproj:233-301`).
- Inspect the unpacked artifact rather than a build directory, and verify all hashes before execution (`scripts/agents/README.md:134-148`).

### Gate D — parity proof review

- Independently recompute P07 manifests and mismatch counts from retained files.
- Verify each C# and Rust pair began from the same game baseline and used the same backend bytes, that each run had its own hashed mod-tree copy, and that every C# run had an isolated `HOME`/XDG/settings root.
- Verify P08 ran against the actual packaged artifact with no ambient backend.
- Treat a missing receipt, missing log, changed input tree, or mismatched backend hash as a failed gate (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:121`).

### Gate E — local quality checks after parity implementation

Only after the no-test freeze is explicitly lifted, run the repository's required host checks with a unique target directory and `-j 2`: formatting check, clippy with warnings denied, all-target tests, and no-default-feature tests (`AGENTS.md:6`; `docs/parity/ACCEPTANCE.md:26-27`). Until then, use formatting/diff inspection and P01-P12 only. Passing Cargo checks will remain code-quality evidence, not packaged-backend or patch-byte evidence.

## Open questions and defaults

1. **Which artifact targets are supported?** Default: the acceptance matrix is the set the release actually publishes, and F28 remains partial for every declared-but-unproduced target. The C# declaration covers Windows x86/x64/arm64, Linux x64/arm64, and macOS x64/arm64, while its copy clauses do not prove all of them (`src/ModSync.GUI/ModSync.csproj:48-60`, `src/ModSync.GUI/ModSync.csproj:233-301`).
2. **Which backend versions are authoritative?** Default: pin the exact backend bytes used by the C# differential and record source/version, SHA-256, license, and target in the manifest. Never resolve `latest` during build or runtime; same-backend bytes are required by P07 (`docs/parity/ACCEPTANCE.md:121`).
3. **Must Rust ship the Python HoloPatcher fallback?** Default: yes for complete behavioral parity, but only as an offline, licensed, hashed bundle. Until P09 passes, report native HoloPatcher support separately and keep the Python-fallback portion partial (`src/ModSync.Core/Services/InstallationService.cs:89-246`, `src/ModSync.Core/Services/InstallationService.cs:345-566`).
4. **What does a missing patcher log mean?** Default: failure, matching C# (`src/ModSync.Core/Instruction.cs:1433-1448`, `src/ModSync.Core/Instruction.cs:1556-1603`) and closing the current Rust acceptance gap (`src/engine.rs:1718-1733`). Requiring the log to be fresh is a deliberate deviation, because C# reads the parent of each source (`:1579-1591`); P11 sets the location.
5. **Should invalid configured paths fall through?** Default: no. Keep the Rust fail-closed behavior (`src/patcher.rs:20-30`) and document it as a safety difference from C# warning-and-fallback discovery (`src/ModSync.Core/Services/InstallationService.cs:617-745`).
6. **Is named-INI selection part of C# parity?** Default: no. Preserve it as a safe Rust extension, file it as a deliberate deviation, and prove it with its own receipt, P12; C# treats the argument as a namespace index (`src/engine.rs:1593-1638`; `src/ModSync.Core/Instruction.cs:1369-1373`).
7. **Should validation probe a backend executable?** Default: no. Keep validation non-executing and place executable probes behind explicit acceptance/install authorization (`AGENTS.md:8`; `src/engine.rs:864-873`; C# behavior not copied: `src/ModSync.Core/Services/InstallationService.cs:816-923`).
8. **May packaging download dependencies?** Default: a controlled packaging job may fetch immutable, checksum-pinned inputs and retain provenance, but the installed application may not fetch or pip-install anything. If redistribution or reproducibility cannot be established, the asset is not bundled (`src/ModSync.Core/Services/InstallationService.cs:129-246`; `AGENTS.md:10`).

## Completion boundary

F09 can move from `partial` to `full` only when all of the following hold:

- ordered patcher and generic external sources, placeholder arguments, backend routing, cancellation, fresh-log verification, and failure stopping have direct production-path receipts (`src/ModSync.Core/Instruction.cs:1289-1603`; Rust implementation area `src/engine.rs:1549-1733`);
- original mod trees remain unchanged and every failed run restores its isolated game snapshot before retry (`AGENTS.md:7-9`);
- each supported final artifact resolves and executes its verified packaged backend without configured paths or ambient PATH entries (`src/patcher.rs:20-79`; `docs/parity/ACCEPTANCE.md:316-324`);
- P07 reports zero path, size, hash, and byte mismatches against the pinned C# oracle for each accepted backend and K1/K2 fixture (`docs/parity/ACCEPTANCE.md:121`);
- the native desktop path is exercised through the same engine and has non-screenshot receipts (`AGENTS.md:5`; `src/ui.rs:1454-1495`);
- P11 has fixed the log location for each backend, and the fresh-log rule and named-INI extension (P12) are recorded as deliberate deviations with their own receipts;
- the Python fallback either passes P09 or is explicitly recorded as an unsupported remaining parity item (`src/ModSync.Core/Services/InstallationService.cs:345-566`; `AGENTS.md:10`);
- `COMPATIBILITY.md` and `docs/parity/ACCEPTANCE.md` link the exact immutable receipts and describe every deliberate difference (`AGENTS.md:11`; `docs/parity/ACCEPTANCE.md:28-32`).

Anything less is a narrower verified slice: source-audited, argv-verified, native-backend-verified, artifact-verified, or byte-differential-verified. It is not full C# F09 parity.
