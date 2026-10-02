# F13 host handlers and F15 post-download FOMOD audit

Status: source audit only, 2026-09-27. No implementation, build, Rust test, harness, network request, or live-host action was performed. Rust observations are pinned to `6466faf6ae4f772b3296d671ed2737d5970bf93f`; the C# oracle is pinned to `d31ccbd3bfcddb4705917eafa6483715c166de3b`. F13 and F15 are still partial in the acceptance ledger (`docs/parity/ACCEPTANCE.md:48-50`).

## Scope and evidence boundary

- This audit covers DeadlyStream, Nexus Mods, MEGA, and GameFront dispatch, authentication/manual boundaries, Nexus update metadata, and the C# `FomodPostDownloadOrchestrator` path (`src/ModSync.Core/Services/Download/DownloadHandlerFactory.cs:51-64`; `src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:15-102`).
- It coordinates with the approved acquisition plan rather than replacing it: Task 1 owns the shared run/report contract, Task 1c owns host classifiers/transports, and Task 2 owns frontend adapters (`docs/parity/download-orchestration-plan.md:355-368`).
- Login, CAPTCHA, JavaScript countdowns, and Nexus free-user waits are human gates. The C# handlers report those situations as failures or manual text because their result type has no manual-required state (`docs/parity/download-orchestration-csharp-audit.md:209-220`); Rust must preserve the boundary with a typed Manual result (`docs/parity/download-orchestration-plan.md:270-275`).
- F13 acceptance still requires authorized live-host evidence, while ordinary implementation probes remain local/disposable (`docs/parity/ACCEPTANCE.md:48`; `docs/parity/download-orchestration-plan.md:370-425`).

## Current Rust surface

| Concern | Present Rust behavior | Gap relevant to this slice |
|---|---|---|
| Host recognition | `download_hosts::provider` recognizes the four named hosts and Direct from an HTTP(S) URL (`src/download_hosts.rs:4-30`). | Recognition is not acquisition: it has no credentials, premium decision, manual gate, filename resolution, or host transport (`src/download_hosts.rs:14-30`). |
| Direct transfer | `download::fetch_with_options` enforces HTTPS, rejects an existing destination, rejects HTML, streams through a temp file, verifies the archive/hash, and publishes without clobbering (`src/download.rs:26-75`). | It has no host login/challenge classification and accepts a caller-chosen destination rather than resolving host filenames (`src/download.rs:33-50`). |
| Queue | `download_queue::fetch_all` provides worker-count validation, cancellation, retry count, stable result ordering, and Completed/Failed/Cancelled outcomes (`src/download_queue.rs:37-52`, `src/download_queue.rs:55-139`). | It has no ManualRequired outcome or component/resource/archive provenance (`src/download_queue.rs:20-35`). |
| Nexus API | Rust resolves nxm download links, tracks rate-limit headers, validates API keys/premium status, and updates resource metadata (`src/updates.rs:165-218`, `src/updates.rs:247-320`, `src/updates.rs:351-443`). | `with_base_url` accepts any scheme and its client is not HTTPS-only (`src/updates.rs:131-140`); this is already assigned to acquisition Task 1 (`docs/parity/download-orchestration-plan.md:361-364`). `src/updates.rs` is a Task 1 file, not a Task 1c file (`docs/parity/download-orchestration-plan.md:361` lists it; `:363` does not). The separate nxm-traversal-fix worker also edits it for the Nexus `file_name` defect (`docs/parity/download-orchestration-plan.md:24-26`), and the integration/wave1 branch carries changes to it. A diff scan of the lane worktrees found `src/updates.rs` changed in `modsync-wt-dl` (Task 1), `modsync-wt-nxmfix`, `modsync-wt-integ`, and also `modsync-wt-completion` (`git diff --name-only <merge-base with 6466faf6>` plus `git status --porcelain` in each `~/.cache/modsync-wt-*`, 2026-09-27). Any F13 change to it waits for every one of them to release it. |
| FOMOD state | Rust reads and writes C#-compatible per-archive prompt state; configured and warned suppress prompts, dismissed does not (`src/fomod.rs:600-712`). | There is no library coordinator that enumerates a run’s component-bound archives, invokes a host, aggregates failures, or resumes pending follow-ups (`src/fomod.rs:600-712`). |
| FOMOD merge | `ChoicesFile::merge_into_remaining` atomically replaces the edited archive’s stored choices while retaining other archives and avoids completed components (`src/fomod.rs:845-909`). | Effective instruction equivalence to the C# configured-component merger is not reported as a post-download result, and restart/failure orchestration is absent (`docs/parity/FOMOD.md:11-13`). |
| Desktop direct download | A completed utility download may launch the editor based on the current guide and filename (`src/ui.rs:105-119`, `src/ui.rs:2145-2180`). | The message carries no component/resource identity; an unregistered archive is deliberately offered, so it cannot be the shared component-bound orchestration contract (`src/ui.rs:108-119`, `src/ui.rs:2165-2180`). |
| Desktop nxm | The desktop resolves one component, downloads, then records the filename (`src/ui.rs:1809-1857`, `src/ui.rs:1860-1875`). | It does not run the same FOMOD follow-up as `Message::Downloaded`; this is one of the all-download-path gaps (`src/ui.rs:108-124`). |
| CLI nxm | `nxm-download` resolves and downloads to a caller path, but does not bind a guide component or record/follow up (`src/main.rs:633-653`). | The approved plan assigns convergence to `nxm_target` plus shared run/record/followups (`docs/parity/download-orchestration-plan.md:340-350`). |

The leading comment in `src/fomod.rs` says archive configuration and installation are not wired (`src/fomod.rs:1`), but later code and the engine now do wire saved choices into validation/install (`src/fomod.rs:845-909`; `src/engine.rs:680-769`, `src/engine.rs:1368-1410`). Treat that comment as stale, not as implementation evidence.

## C# host dispatch contract

The C# factory orders DeadlyStream, MEGA, Nexus, GameFront, then Direct, with the generic handler last (`src/ModSync.Core/Services/Download/DownloadHandlerFactory.cs:51-64`). `IDownloadHandler` exposes recognition, filename resolution, download, metadata, and provider identity (`src/ModSync.Core/Services/Download/IDownloadHandler.cs:14-34`). The first matching handler wins; there is no cross-handler fallback (`docs/parity/download-orchestration-csharp-audit.md:209-213`).

### DeadlyStream

- Recognition is a case-insensitive `deadlystream.com` substring test (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:85-89`). The handler upgrades HTTP to HTTPS during resolution (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:135-146`).
- The anonymous path establishes cookies, extracts `csrfKey` from HTML/input/script/link forms, and follows download links (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:287-335`, `src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:827-872`).
- Challenge text is detected, retried up to three times, and delayed between tries (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:875-944`). However, non-success responses are rejected before challenge-body inspection, so 403/503 challenge detection is defective and must not be copied (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:906-916`).
- An HTML response where a file is expected is treated as authentication/invalid-link failure (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:698-745`). Missing links produce manual-download guidance and write debug HTML into the destination, another behavior Rust must not copy (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:510-542`).
- Content-Disposition and URL-derived names are joined without a one-component safety check (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:749-768`, `src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:1117-1157`). Rust must sanitize before every join per the approved plan (`docs/parity/download-orchestration-plan.md:62`).
- The handler accepts `targetFilenames` but downloads the discovered set and recursively follows multi-file pages, returning only the last file from a nested page (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:339-345`, `src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:706-736`). This is not a parity target (`docs/parity/download-orchestration-csharp-audit.md:336-337`).

### Nexus Mods

- Nexus recognizes both `nxm://` and `nexusmods.com` forms (`src/ModSync.Core/Services/Download/NexusModsDownloadHandler.cs:94-99`). The Rust parser already separates nxm identity/key/expiry from ordinary web URLs (`src/updates.rs:165-183`; `src/protocol.rs:1-121`).
- C# chooses the supplied key, configured key, then environment fallbacks; without a key it returns manual-interaction failure for web URLs, and nxm without either API credentials or key/expiry fails (`src/ModSync.Core/Services/Download/NexusModsDownloadHandler.cs:473-545`).
- Rust already exposes an explicit validate result with `is_valid` and `is_premium` and reads rate-limit headers (`src/updates.rs:68-74`, `src/updates.rs:247-275`, `src/updates.rs:300-320`). The approved contract validates premium once per run: a free or absent key becomes `Manual(FreeUserWait)`, not an automated slow-download wait (`docs/parity/download-orchestration-plan.md:270-275`, `docs/parity/download-orchestration-plan.md:465-466`).
- Nexus API filenames and mirror URLs are external input (`src/updates.rs:184-217`). The host lane must retain the one-normal-component filename rule and HTTPS mirror rule before publish (`src/updates.rs:198-213`; `docs/parity/download-orchestration-plan.md:62`).

### MEGA

- C# recognizes `mega.nz`, logs in anonymously through `MegaApiClient`, serializes access with a semaphore, and uses the provider node name (`src/ModSync.Core/Services/Download/MegaDownloadHandler.cs:17-57`, `src/ModSync.Core/Services/Download/MegaDownloadHandler.cs:220-288`).
- C# converts old file/folder link shapes, but the pinned executable evidence covers URL conversion rather than successful folder download (`src/ModSync.Core/Services/Download/MegaDownloadHandler.cs:463-516`; `src/ModSync.Tests/MegaDownloadHandlerTests.cs:52-124`).
- Rust has no MEGA decrypting client (`Cargo.toml:13-41`; `src/download_hosts.rs:20-24`). The approved default is `Manual(Unsupported)` until a maintained/licence-compatible decrypting client is reviewed; adding one would require an explicit dependency reason because HTTPS alone cannot decrypt MEGA payloads (`docs/parity/download-orchestration-plan.md:457-458`).

### GameFront

- C# recognizes `gamefront.com`, cannot resolve filenames, and always returns manual-interaction failure because of JavaScript countdown/anti-bot behavior (`src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:41-84`).
- Therefore Rust classifies GameFront as `Manual(Unsupported)`, exactly as the approved plan says (`docs/parity/download-orchestration-plan.md:273`). Peer fallback is allowed only for share-eligible resources, with an authored SHA-256 plus a magnet (`docs/parity/download-orchestration-plan.md:22`). Automating a countdown, browser challenge, or CAPTCHA would exceed the oracle and the user constraint (`src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:69-83`).
- There is no `BrowserRequired` reason, and none may be added for GameFront. The lane's `ManualReason` has exactly `LoginRequired`, `Captcha`, `HtmlLanding`, `FreeUserWait`, and `Unsupported`, and `is_gate()` treats every reason except `Unsupported` as a gate (`/home/brunner56/.cache/modsync-wt-dl/src/acquisition.rs:28-42`). A new GameFront reason would therefore silently turn GameFront into a gate and forbid its share-eligible Peer fallback, contradicting `download-orchestration-plan.md:22,273`.

## Nexus update metadata

C# checks only registry URLs matching a Nexus game/mod URL, caches each unique game/mod lookup per run, stops when the rate-limit budget is exhausted, and records errors per key (`src/ModSync.Core/Services/ModUpdateCheckService.cs:50-143`). A successful version writes `LastUpdateCheck` and `LatestKnownVersion`; the first check adopts a missing `ModVersion`, while later version inequality sets `UpdateAvailable` and appends a summary item (`src/ModSync.Core/Services/ModUpdateCheckService.cs:163-203`). Rust already implements the same four metadata effects and per-key cache/rate-limit stop (`src/updates.rs:351-443`). F13 work should reuse this service and restrict itself to transport/auth/probe gaps; it must not create a second update model (`src/updates.rs:327-443`).

## C# post-download FOMOD orchestration

### Enumeration and prompt policy

`FomodPostDownloadOrchestrator.ProcessAsync` validates its inputs, filters to selected components, then walks each component’s registered on-disk archive paths (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:17-44`). `FomodDownloadedArchivePaths` deduplicates registered names case-insensitively, keeps only archive-like entries, joins them under the mod directory, and returns existing files for prompting (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:33-48`, `src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:50-102`).

For each detected FOMOD, the orchestrator skips configured/warned state, asks the host, records dismissals, extracts, runs the wizard, reports extraction/configuration failure, merges a non-null configured component, marks configured, and reports success (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:46-100`). Configured and warned suppress future prompts; dismissed intentionally prompts again (`src/ModSync.Core/Services/Fomod/FomodDownloadPromptState.cs:18-49`). Rust already matches the status policy (`src/fomod.rs:648-712`).

### Host callbacks and configured-component merge

The C# host interface owns four presentation callbacks: ask, run wizard, report failure, and report configured (`src/ModSync.Core/Services/Fomod/IFomodPostDownloadHost.cs:12-32`). This keeps extraction/enumeration/merge policy in Core while GUI and CLI choose interaction (`src/ModSync.GUI/Services/FomodPostDownloadPromptService.cs:18-27`; `src/ModSync.Core/Services/Fomod/FomodPostDownloadOptionsResolver.cs:94-115`).

The merger removes prior archive-scoped instructions and `Choose` instructions associated with the new configured options, updates existing option selection/instructions by GUID, appends new options, and appends configured component instructions (`src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:21-69`, `src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:123-149`). Rust stores archive-scoped `FomodChoices` atomically and derives safe current-archive copies in the engine (`src/fomod.rs:845-909`; `src/engine.rs:1368-1410`). Acceptance therefore needs semantic comparison of the effective configured instruction set, not a claim based only on metadata presence (`docs/parity/FOMOD.md:11-13`).

No C# oracle for this merger exists yet. The measured reference behavior in `docs/parity/FOMOD.md:15-31` covers group selection modes and the required-plugin conflict only, and the existing `fomod-selection-oracle` probe calls `FomodInstallerPresenter`/`FomodToComponentMapper`, never `FomodConfiguredComponentMerger.MergeInto` (`docs/parity/fomod-selection-oracle/Program.cs:1-30`). The oracle receipts list `FomodConfiguredComponentMerger.cs` only as a hashed source file (`docs/parity/fomod-selection-oracle/receipt.json`, `docs/parity/fomod-folder-oracle/receipt.json`). A merger comparison needs a new non-Rust C# probe; the plan specifies it.

### Invocation coverage and holes

- GUI batch download runs the FOMOD pass once after the batch, only if the session was not cancelled, on the UI dispatcher (`src/ModSync.GUI/Services/DownloadOrchestrationService.cs:287-342`). Per-component download exceptions are logged and do not prevent the batch-level pass (`src/ModSync.GUI/Services/DownloadOrchestrationService.cs:296-318`).
- CLI convert, merge, and install invoke the hook after their download batch (`src/ModSync.Core/CLI/ModBuildConverter.cs:2230-2237`, `src/ModSync.Core/CLI/ModBuildConverter.cs:2671-2678`, `src/ModSync.Core/CLI/ModBuildConverter.cs:3514-3535`). The resolver selects skip, choices file, interactive, or warn-continue (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOptionsResolver.cs:20-92`).
- C# nxm handoff downloads and records a file but does not invoke the FOMOD pass (`docs/parity/download-orchestration-csharp-audit.md:265-275`). C# per-mod download buttons also use a separate path (`docs/parity/download-orchestration-csharp-audit.md:300-302`). These omissions are defects/gaps to close through the shared Rust coordinator, not behaviors to preserve.
- Rust desktop direct downloads can prompt, but Rust nxm messages only record the file (`src/ui.rs:108-124`). Rust CLI nxm has neither guide binding nor follow-up (`src/main.rs:633-653`). The approved acquisition plan already requires every component-bound path to converge on `nxm_target`, `run`, `record`, and `fomod_followups` (`docs/parity/download-orchestration-plan.md:340-350`).

### The lane's `fomod_followups` (post-pin, active Task 1 worktree)

Task 1 has already landed a follow-up selector in its worktree, after this audit's Rust pin (`/home/brunner56/.cache/modsync-wt-dl` at `e2396a2`; `src/download_orchestrator.rs:1370-1451`). It is the one FOMOD eligibility authority. It filters to selected components, takes Completed outcomes plus `AlreadyPresent` skips, applies `fomod::should_prompt` through `offer_prompt`, and positively inspects each archive (`src/download_orchestrator.rs:1394-1451`, `:1455-1469`).

It has one defect this slice depends on:

- **Path-only dedup.** `seen.insert(path.clone())` keys on the archive path alone (`src/download_orchestrator.rs:1419-1426`). Two components that register the same basename resolve to the same path under the mod directory, so the second component's follow-up is dropped.
- **No registry URL.** `FomodFollowup` carries only `component_guid` and `archive` (`src/download_orchestrator.rs:1370-1375`), although `TargetOutcome` already has `registry_url` (`src/download_orchestrator.rs:445-457`). A consumer cannot write prompt state to the exact resource without re-deriving it.

The fix belongs to the Task 1 owner as a change request, not to a second FOMOD enumerator. The plan names it CR-1.

## Restart and failure boundary

- C# prompt state lives inside `ResourceMetadata.HandlerMetadata`, so it can survive only when the mutated guide is serialized (`src/ModSync.Core/Services/Fomod/FomodDownloadPromptState.cs:107-124`; `docs/parity/download-orchestration-csharp-audit.md:229-233`). The pinned audit did not prove that every frontend serializes those mutations (`docs/parity/download-orchestration-csharp-audit.md:231-233`).
- Rust guide serialization preserves arbitrary resource metadata, including `handlerMetadata`, and its FOMOD helpers write the compatible object shape (`src/model.rs:34-58`; `src/fomod.rs:675-712`). This establishes a persistence mechanism, not restart proof.
- A restart-safe Rust design needs two explicit modes: current-run follow-up from typed acquisition provenance, and recovery discovery from selected components’ registered, existing archives. An unrestricted directory scan would lose component ownership and could prompt for unrelated files; C# enumeration is registry-bound (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:68-102`).
- Extraction, parse, choices, merge, serialization, and host callback failures must be returned as archive/component-bound outcomes. The C# interface reports only extraction/configuration failure and continues (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:76-99`); Rust’s acceptance ledger requires safe failure gating before install (`docs/parity/FOMOD.md:7-13`).

## Claims

claim: Task 1c, not this FOMOD slice, owns host transport and host classification.
referent: `docs/parity/download-orchestration-plan.md:355-368`
checked: `nl -ba docs/parity/download-orchestration-plan.md | sed -n '355,368p'`
verdict: holds
confidence: high; only an approved ownership revision would change it

claim: Current Rust has provider recognition and Nexus/update primitives, but no shared F13 host acquisition implementation.
referent: `src/download_hosts.rs:4-30`; `src/updates.rs:126-320`; `src/download.rs:26-75`
checked: `nl -ba` on the cited Rust files at `6466faf6`
verdict: holds
confidence: high; active Task 1 or Task 1c delivery will supersede this snapshot

claim: Current Rust has compatible FOMOD prompt state and atomic choices merge, but no shared post-download coordinator.
referent: `src/fomod.rs:600-712`; `src/fomod.rs:845-909`; `src/lib.rs:42`
checked: `rg -n "followup|PostDownload|orchestrat" src` plus `nl -ba` on cited files
verdict: holds
confidence: high; active acquisition work may add the missing module after this audit pin

claim: C# post-download processing is registry-bound, selected-component-only, host-driven, and merges configuration before marking configured.
referent: `src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:38-100`; `src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:68-102`
checked: direct read in pinned C# worktree at `d31ccbd3`
verdict: holds
confidence: high; source is pinned and internally consistent

claim: Login, CAPTCHA/challenge, JavaScript countdown, and Nexus free-user waits must remain manual.
referent: `src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:69-83`; `src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:875-944`; `docs/parity/download-orchestration-plan.md:270-275`
checked: direct source/approved-plan read
verdict: holds
confidence: high; changing it would contradict both the user constraint and approved acquisition contract

claim: The lane's `fomod_followups` collapses two components that register the same basename, and its result cannot name the registry entry.
referent: `/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1370-1375` (struct fields), `:1419-1426` (path-only `seen` set)
checked: `nl -ba src/download_orchestrator.rs | sed -n '1360,1470p'` in the lane worktree at `e2396a2`
verdict: holds
confidence: high; a later lane commit that keys `seen` on component GUID + registry URL + path and adds `registry_url` to `FomodFollowup` would retire it

claim: GameFront is `Manual(Unsupported)` and no `BrowserRequired` reason exists.
referent: `docs/parity/download-orchestration-plan.md:22,273`; `/home/brunner56/.cache/modsync-wt-dl/src/acquisition.rs:28-42`
checked: direct read of both files
verdict: holds
confidence: high; only an approved change to the Task 1 `ManualReason` enum would change it

claim: No runtime, live-host, build, test, or harness evidence was produced by this audit.
referent: `docs/parity/ACCEPTANCE.md:26-32`; this document's source-only scope
checked: no Cargo, host-spawn, external program, or network command was invoked during the audit
verdict: holds
confidence: high; command history for this task contains read-only source inspection and the two documentation writes only
