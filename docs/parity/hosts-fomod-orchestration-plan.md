# F13 host completion and F15 post-download FOMOD orchestration plan

Status: plan only, 2026-09-27. Nothing here is implemented. Rust citations are pinned to `6466faf6ae4f772b3296d671ed2737d5970bf93f`; C# citations are pinned to `d31ccbd3bfcddb4705917eafa6483715c166de3b`. The governing source audit is [hosts-fomod-orchestration-audit.md](hosts-fomod-orchestration-audit.md). This plan advances F13 and the remaining post-download part of F15; it cannot close either ledger item without the acceptance evidence named below (`docs/parity/ACCEPTANCE.md:48-50`).

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md` and every unrelated edit. This documentation task does not authorize source edits (`AGENTS.md:1-25`; `docs/parity/ACCEPTANCE.md:26-30`).
- Do not create, modify, adapt, compile, or run Rust tests or Rust harnesses before full C# parity. Verification for this slice uses production code paths, the real CLI/native GUI, disposable directories, static review, and direct non-Rust fixtures (`docs/parity/ACCEPTANCE.md:26`; `docs/parity/install-progress-plan.md:5-14`).
- Do not build in the planning task. Later implementation builds run only through the host with a lane-owned target directory and `-j 2`; this is an execution constraint supplied by the user, not evidence that any gate passed.
- Keep installation behavior in `src/engine.rs`; host acquisition and FOMOD follow-up stay shared-library behavior used by both CLI and desktop (`AGENTS.md:1-25`; `docs/parity/install-progress-plan.md:10`).
- Validation remains non-mutating and never launches an external program. FOMOD recovery discovery may inspect archive headers/XML but may not rewrite a guide or archive until an explicitly chosen configure action (`AGENTS.md:1-25`; `src/engine.rs:400-409`).
- Login, CAPTCHA/challenge solving, JavaScript countdowns, Nexus free-user waits, payment/premium acquisition, and browser credential entry remain Manual. A manual gate is terminal for Origin and may not silently fall through to Peer (`docs/parity/download-orchestration-plan.md:270-275`, `docs/parity/download-orchestration-plan.md:410-411`).
- Preserve strict guide order and fail-stop installation. A failed required FOMOD follow-up cannot be hidden and cannot be backfilled after later components (`AGENTS.md:1-25`; `docs/parity/ACCEPTANCE.md:75`).
- No new networking crate is permitted without a written need, maintenance/licence review, and review approval. Existing `reqwest` is sufficient for Direct, Nexus API, GameFront manual classification, and the bounded DeadlyStream HTTP path (`Cargo.toml:13-41`; `src/download.rs:33-40`; `src/updates.rs:88-94`). MEGA is the only identified case where provider-side decryption may justify a specialized dependency (`docs/parity/download-orchestration-plan.md:457`).
- Server-supplied filenames become exactly one normal path component before any join, and every transfer publishes through a temporary file without clobbering (`docs/parity/download-orchestration-plan.md:62`; `src/download.rs:35-75`).
- Preserve the existing acquisition plan’s settled boundaries: selected-only Rust CLI, typed Manual state, truthful nonzero failures, no TTY guessing by default, explicit write-back, and no unreviewed persistent global resource index (`docs/parity/download-orchestration-plan.md:328-350`, `docs/parity/download-orchestration-plan.md:455-467`).

## Problem frame

Rust currently recognizes the named hosts, has a safe direct fetcher and Nexus API/update support, and can configure a FOMOD archive when a frontend chooses one (`src/download_hosts.rs:4-30`; `src/download.rs:26-75`; `src/updates.rs:126-443`; `src/fomod.rs:845-909`). It does not yet provide one component-bound acquisition result across all host paths or one shared post-download coordinator that enumerates archives, invokes CLI/GUI hosts, merges configuration, reports failures, and resumes after restart (`docs/parity/hosts-fomod-orchestration-audit.md:15-31`). The result is split behavior: desktop direct downloads may prompt, desktop nxm only records, and CLI nxm does neither (`src/ui.rs:108-124`; `src/main.rs:633-653`).

The desired outcome is one shared boundary:

`selected component + registered resource -> acquisition outcome -> recorded safe archive provenance -> FOMOD follow-up -> atomic guide mutation -> explicit persistence/dirty state`

The acquisition lane decides how bytes arrive; the FOMOD lane decides what to do with component-bound archives after arrival. Frontends only supply interaction and render typed results (`docs/parity/download-orchestration-plan.md:35-45`; `src/ModSync.Core/Services/Fomod/IFomodPostDownloadHost.cs:12-32`).

## Scope boundaries

### In scope

- F13 host behavior for DeadlyStream, Nexus, MEGA, and GameFront, including credentials/premium/manual decisions and Nexus update metadata reuse (`docs/parity/ACCEPTANCE.md:48`).
- F15 post-download enumeration, host callbacks, choices/configuration merge, prompt-state persistence, typed failure results, and restart recovery for selected component-bound archives (`docs/parity/ACCEPTANCE.md:50`; `docs/parity/FOMOD.md:5-13`).
- All component-bound acquisition entry points: batch fetch, nxm handoff/CLI, AlreadyPresent results, and explicit recovery. The free-form direct-download utility remains unbound unless the user explicitly selects a component/resource (`src/ui.rs:108-119`; `src/ui.rs:1809-1875`).
- CLI and native desktop adapters after the shared API is approved (`docs/parity/download-orchestration-plan.md:361-366`).

### Out of scope

- CAPTCHA solving, login automation, Nexus slow-download countdown automation, premium purchase, embedded browser engines, or GameFront JavaScript execution (`src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:69-83`; `docs/parity/download-orchestration-plan.md:270-275`).
- Changing install order, recovery snapshots, deployment, profiles, selection, merge, telemetry, markdown parsing, or unrelated FOMOD XML/selection semantics; those lanes are separately owned (`docs/parity/ACCEPTANCE.md:39-64`).
- Claiming live-host, full-corpus, full-build, game, or F29 equivalence from loopback/static evidence (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:64`).
- Copying C# defects such as shared credential headers, unsafe filenames, challenge detection after status rejection, swallowed cancellation, or success despite download failures (`docs/parity/download-orchestration-csharp-audit.md:304-359`).

### Deferred to follow-up work

- Automated MEGA stays deferred unless the dependency decision in Open Question 1 passes review (`docs/parity/download-orchestration-plan.md:457`).
- Authorized live-host evidence is a separate consented acceptance activity; local implementation work does not contact the four real hosts (`docs/parity/download-orchestration-plan.md:32`; `docs/parity/ACCEPTANCE.md:48`).
- Full FOMOD corpus, UI accessibility/platform matrix, and K1/K2 full-install equivalence remain broader F15/F29 work (`docs/parity/FOMOD.md:29-35`; `docs/parity/ACCEPTANCE.md:50`, `docs/parity/ACCEPTANCE.md:64`).

## Requirements

- **R1 — One host policy.** Task 1c is the only owner of Provider-to-Origin/Manual classification; no frontend or FOMOD code reclassifies a URL (`docs/parity/download-orchestration-plan.md:361-364`).
- **R2 — Human gates.** Login, CAPTCHA/challenge, HTML landing, and Nexus free-user paths return a gate `ManualReason` and perform no bypass, wait automation, or Peer fallback (`docs/parity/download-orchestration-plan.md:20-23`, `:270-275`, `:410-411`). GameFront's countdown is never automated, but GameFront is `Manual(Unsupported)`, not a gate, so it may use Peer when share-eligible (`docs/parity/download-orchestration-plan.md:22`, `:273`).
- **R3 — Safe host output.** Every external filename and mirror is validated before join/publish, every successful archive is verified, and cancellation/failure leaves no published partial (`src/download.rs:33-75`; `docs/parity/download-orchestration-plan.md:62`).
- **R4 — Nexus reuse.** Premium validation, rate-limit tracking, nxm link resolution, and update metadata use `updates::NexusClient`/`updates::check`; no second API client or metadata schema is introduced (`src/updates.rs:126-320`, `src/updates.rs:351-443`).
- **R5 — Provenance-bound enumeration.** Normal follow-up consumes the Task 1 lane's `fomod_followups` result, which is built from Completed/AlreadyPresent acquisition records. That function is the one FOMOD eligibility authority (`/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1394-1451`). Each follow-up must carry component GUID, registry URL, and safe archive path; today it lacks the registry URL and dedups by path only, so change request CR-1 (U1) extends it. It never infers “downloaded this run” from a directory scan (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:33-102`).
- **R6 — Explicit recovery enumeration.** Restart recovery walks only selected, unfinished components’ registered archive entries, rejects unsafe names/symlinks/non-files, verifies existence/archive/FOMOD identity, and labels the origin Recovery rather than CurrentRun (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:68-102`; `src/engine.rs:680-769`).
- **R7 — Shared host callback.** Shared Core owns enumeration, probe, extraction, configuration application, prompt-state transitions, and result aggregation. CLI/GUI hosts own ask/configure/report presentation only (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:38-100`; `src/ModSync.Core/Services/Fomod/IFomodPostDownloadHost.cs:12-32`).
- **R8 — Atomic semantic merge.** Applying one archive’s choices replaces only that archive’s prior saved configuration, preserves other archives and completed components, validates the effective configured instruction set, and commits the guide mutation only after the whole merge succeeds (`src/fomod.rs:845-909`; `src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:21-69`).
- **R9 — Honest prompt state.** Configured and warned suppress future prompts; dismissed and failed prompt again. State is written only to the matching component resource and persists only through an explicit CLI record/output or desktop dirty-guide save (`src/fomod.rs:648-712`; `src/ModSync.Core/Services/Fomod/FomodDownloadPromptState.cs:38-49`).
- **R10 — Failure is typed and blocking.** Every archive ends as NotFomod, AlreadyHandled, Dismissed, Warned, Configured, ManualRequired, Failed, or Cancelled with component/resource/archive identity. A required Failed/Cancelled result prevents install continuation and yields nonzero CLI status (`docs/parity/FOMOD.md:7-13`; `docs/parity/download-orchestration-plan.md:329-339`).
- **R11 — All component-bound paths converge.** Batch fetch, nxm GUI/CLI, AlreadyPresent, and recovery call the same follow-up API. The unbound utility cannot mutate a guide until component/resource binding is explicit (`src/ui.rs:108-124`; `src/main.rs:633-653`; `docs/parity/download-orchestration-plan.md:340-350`).
- **R12 — Cancellation and restart.** Cancellation stops before the next archive mutation, preserves already committed per-archive state, and leaves unhandled archives discoverable by recovery. Recovery is idempotent for configured/warned archives (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:38-55`; `src/fomod.rs:667-712`).

## Key technical decisions

### KTD1. Extend the approved acquisition report; do not create a second downloader

Task 1 adds component/resource/archive provenance to the approved `RunReport`/target outcomes and owns `fomod_followups`; Task 1c provides host execution. FOMOD consumes `fomod_followups` after publish and does not re-derive eligibility in a new module (`docs/parity/download-orchestration-plan.md:192-246`, `docs/parity/download-orchestration-plan.md:355-368`). This prevents the current split where the UI invents eligibility from a filename (`src/ui.rs:2165-2180`). Governs R1, R3, R5, R11.

### KTD2. Use a two-mode enumerator

`CurrentRun` takes the `FomodFollowups` value returned by the lane's `fomod_followups` as its only input. It does not re-filter selection, re-apply `should_prompt`, or re-inspect archives; those decisions stay in Task 1 (`/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1394-1451`). A missing field is fixed by a change request to the Task 1 owner, not locally. `Recovery` reconstructs candidates from selected unfinished components’ registry entries and existing safe files (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:33-102`). The mode is part of each result so evidence cannot confuse a newly downloaded archive with a pre-existing one. Governs R5, R6, R12.

### KTD3. Keep interaction out of the coordinator

The coordinator invokes a small host interface analogous to C# ask/configure/report callbacks, but returns typed results even when a host callback fails (`src/ModSync.Core/Services/Fomod/IFomodPostDownloadHost.cs:12-32`). The GUI schedules its host on the UI thread; CLI modes are deterministic and do not guess TTY state (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOptionsResolver.cs:20-92`; `docs/parity/download-orchestration-plan.md:333-350`). Governs R7, R10.

### KTD4. Preserve Rust’s safe choices representation and prove semantic C# merge equivalence

Rust retains archive-scoped `FomodChoices` and the engine’s current-archive mapping rather than eagerly duplicating executable instructions (`src/fomod.rs:845-909`; `src/engine.rs:1368-1410`). The follow-up merge must remove/replace the same logical archive configuration that C# removes by archive-source prefix and option GUID, and direct probes compare effective selected copies/options, not serialized byte shape (`src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:41-68`, `src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:123-149`). Governs R8.

### KTD5. Prompt state is per component/resource/archive

No recursive “set every matching filename in the guide” operation is used by the shared coordinator. The follow-up record supplies the owning component and registry URL, avoiding collisions when two components register the same basename (`src/fomod.rs:675-712`; `src/fomod.rs:961-985`). This holds only after CR-1 lands: today `FomodFollowup` has no registry URL and the lane dedups by path, so the second component is dropped (`/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1370-1375`, `:1419-1426`). The existing broad frontend helper may remain for the standalone editor, but acquisition follow-up uses the precise API. Governs R5, R9.

### KTD6. Manual is a result, not an error string

Host classifiers return one of the lane's existing `ManualReason` values before retries or Peer fallback: `LoginRequired`, `Captcha`, `HtmlLanding`, `FreeUserWait`, or `Unsupported` (`/home/brunner56/.cache/modsync-wt-dl/src/acquisition.rs:28-42`; `docs/parity/download-orchestration-plan.md:270-275`). This plan adds no reason. GameFront, MEGA, and pre-1c DeadlyStream are `Unsupported`, which is the only non-gate reason, so each may use Peer only when share-eligible (`docs/parity/download-orchestration-plan.md:22`, `:273-275`). Presentation can show the URL/open-link action without parsing prose. Governs R2, R10.

### KTD7. No dependency expansion for the first landing

DeadlyStream uses the already approved recorded-fixture gate and existing HTTPS stack; Nexus reuses `reqwest`; GameFront stays Manual; MEGA stays Manual by default (`Cargo.toml:36`; `docs/parity/download-orchestration-plan.md:457-458`). A specialized MEGA client is a separate reviewed unit only if Open Question 1 is explicitly resolved in favor. Governs R1-R4.

## High-level technical design

```text
selection::download_candidates
        |
        v
acquisition::plan -> Task 1c host policy -> acquisition::run
                                              |
                              Completed / AlreadyPresent records
                                              |
                                              v
                    download_orchestrator::fomod_followups   (Task 1, sole eligibility authority)
                                              |
                                              v
                         fomod_post_download::enumerate(CurrentRun)   (consumes, never re-derives)
                                              |
                         safe registry/component/archive binding
                                              |
                                              v
                      host.ask -> host.configure -> atomic merge
                                              |
                       typed per-archive result + prompt state
                                              |
                       record/dirty-guide + install gate/report

restart -> enumerate(Recovery) -> same probe/host/merge/result path
```

The arrows describe ownership and state flow. `src/engine.rs` consumes the resulting guide but does not acquire files or present prompts (`src/engine.rs:680-769`, `src/engine.rs:1368-1410`).

## Shared-library API boundary

The exact Rust names are implementation-time choices; review must preserve these facts:

- An acquisition archive record identifies run, top-level component GUID, exact resource URL/key, safe relative filename, verified full path, and Completed versus AlreadyPresent provenance (`docs/parity/download-orchestration-plan.md:192-246`).
- A follow-up request identifies CurrentRun versus Recovery, completed-prefix exclusions, cancellation, archive options/budgets, and interaction mode (`src/fomod.rs:720-725`; `src/engine.rs:718-769`).
- The host can answer Configure, Dismiss, AlreadyHandled/Warn, or Cancel and can return configured archive choices; it does not perform downloads, path joins, prompt-state writes, or guide serialization (`src/ModSync.Core/Services/Fomod/IFomodPostDownloadHost.cs:12-32`).
- A follow-up report contains one ordered result per candidate and an aggregate decision: Continue, ManualRequired, Failed, or Cancelled. Frontends format it; they do not recalculate it (`docs/parity/download-orchestration-plan.md:35-45`).
- Guide mutation happens on a clone and commits archive-by-archive only after probe/configure/merge succeeds; a failed archive does not mark configured (`src/fomod.rs:905-909`; `src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:76-99`).

## Host policy matrix

| Provider/path | Planned Origin | Manual boundary | Dependency decision | Evidence before status advances |
|---|---|---|---|---|
| Nexus nxm with key/expiry or validated premium API key | Existing `NexusClient` link/API path (`src/updates.rs:165-218`, `src/updates.rs:247-275`) | Missing/invalid key or free account -> Manual, no automated wait (`docs/parity/download-orchestration-plan.md:270-275`) | No new crate (`Cargo.toml:36`) | Fake HTTPS API plus separately authorized live premium/nxm download (`docs/parity/download-orchestration-plan.md:390-406`) |
| DeadlyStream anonymous, challenge-free fixture | Task 1c cookie/CSRF path after recorded-fixture review (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:827-872`) | Login HTML/challenge -> Manual, no challenge retries around (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:875-944`) | No new networking crate; an HTML parser dependency requires a parser-only rationale | Recorded challenge-free fixture, challenge/login fixtures, then authorized live sample (`docs/parity/download-orchestration-plan.md:457-459`) |
| MEGA single-file | None by default (`src/download_hosts.rs:20-24`) | Manual(Unsupported) (`docs/parity/download-orchestration-plan.md:457`) | Specialized decrypting client only after maintenance/licence/security review | Local deterministic encrypted fixture plus authorized live file; until then remain partial |
| GameFront | None (`src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:47-84`) | `Manual(Unsupported)`; Peer only when share-eligible (`docs/parity/download-orchestration-plan.md:22`, `:273`) | No crate/browser engine | Static classifier/open-link evidence; C# itself is manual (`src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:69-83`) |

## Serialized file ownership and delivery sequence

The named active lanes own `src/ui.rs`, `src/main.rs`, `src/acquisition.rs`, `src/download_orchestrator.rs`, `src/updates.rs`, `src/p2p.rs`, `src/merge.rs`, `src/telemetry.rs`, `src/profiles.rs`, `src/markdown.rs`, `src/deployment.rs`, `src/selection.rs`, and `src/recovery.rs`. No unit below starts until its owned files are released and its base commit is recorded. Workers never edit outside their row without an ownership handoff (`docs/parity/download-orchestration-plan.md:355-368`).

### U1. Land and review the acquisition provenance contract

**Goal:** Make Task 1’s report sufficient for precise FOMOD follow-up without adding FOMOD behavior to acquisition.

**Requirements:** R1, R3, R5, R11.

**Dependencies:** Approved delivery of download-orchestration Task 1 and selection Task 0 (`docs/parity/download-orchestration-plan.md:355-364`).

**Files:** `src/acquisition.rs` (active owner first), `src/download_orchestrator.rs` (active owner first), `src/lib.rs` (serialized handoff), `docs/parity/download-orchestration-plan.md` only if its owner approves a contract correction.

**Approach:** Review rather than independently edit the active lane. Require every successful/present component-bound archive result to carry run/component/resource/safe-filename/provenance fields. Do not add prompt state or UI callbacks to acquisition (`docs/parity/download-orchestration-plan.md:192-246`). If the active lane omits a required field, return a bounded change request to that owner; do not patch around it in FOMOD.

**Change request CR-1 (to the Task 1 owner, lane-download-orchestration).** Named defect in the lane's `fomod_followups`:

- It dedups on the archive path alone (`seen.insert(path.clone())`, `/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1419-1426`). Two selected components that register the same basename resolve to one path, so the second component's follow-up is silently dropped.
- `FomodFollowup` carries only `component_guid` and `archive` (`:1370-1375`), although the outcome it reads already has `registry_url` (`:445-457`). U3 cannot write prompt state to the exact resource without re-deriving it.

Requested change: add `registry_url` to `FomodFollowup` (and to `FomodInspectionError`), and key the `seen` set on component GUID + registry URL + path. Positive inspection may still be cached per path, so one archive is read once. U3 starts from the commit that lands CR-1. **Check:** probe 3 below; it fails today and must pass on the CR-1 commit.

**Direct probe scenarios:**

1. Dry-run selected/unselected planning emits component/resource identity without filesystem mutation (`docs/parity/download-orchestration-plan.md:374-381`).
2. Completed and verified AlreadyPresent archives retain the same component/resource identity; failed/manual targets do not produce follow-up candidates (`docs/parity/download-orchestration-plan.md:382-397`).
3. Two components with the same basename remain distinct by component GUID and registry URL: the production CLI's follow-up JSON lists two entries, each naming its own registry URL (`src/fomod.rs:626-646`; CR-1). This is the CR-1 acceptance check.

**Verification:** Static API review plus JSON output from the real production CLI; no Rust test target or harness.

### U2. Complete Task 1c host policy and transports

**Goal:** Implement the reviewed F13 host decisions in the existing acquisition lane.

**Requirements:** R1-R4.

**Dependencies:** U1; approved recorded fixtures and the settled defaults in the host policy matrix. U2 also waits for `src/updates.rs` to be released by all three current writers: Task 1, which owns it (`docs/parity/download-orchestration-plan.md:361`); the nxm-traversal-fix worker, which fixes the unfiltered Nexus `file_name` there (`docs/parity/download-orchestration-plan.md:24-26`); the integration/wave1 branch, which also touches it; and the completion lane, whose worktree also changes it. The worktree scan found all four (`modsync-wt-dl`, `-nxmfix`, `-integ`, `-completion`; see the audit's Current Rust surface table).

**Files:** `src/download_hosts.rs` and optional `src/download_hosts/*.rs`, which are Task 1c ownership (`docs/parity/download-orchestration-plan.md:363`). `src/updates.rs` is **not** a Task 1c file; it belongs to Task 1 (`docs/parity/download-orchestration-plan.md:361`). U2 edits it only through an explicit handoff after the release above, and only for existing client reuse. `Cargo.toml`/`Cargo.lock` only if Open Question 1 authorizes a reviewed MEGA client, and then through the manifest owner's sequence (download Task 2a reserves them, `docs/parity/download-orchestration-plan.md:364`).

**Approach:**

1. Nexus uses one premium validation per run, typed free-user Manual, safe filename/mirror handling, and existing update metadata (`src/updates.rs:165-320`, `src/updates.rs:351-443`).
2. GameFront remains `Manual(Unsupported)` with an open-link action, and Peer only when share-eligible (`src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:69-83`; `docs/parity/download-orchestration-plan.md:22`, `:273`).
3. DeadlyStream remains Manual until the challenge-free fixture is approved; then implement only the anonymous cookie/CSRF route, classifying challenge/login before status-based retry (`src/ModSync.Core/Services/Download/DeadlyStreamDownloadHandler.cs:827-944`).
4. MEGA remains Manual unless its separate dependency review lands (`docs/parity/download-orchestration-plan.md:457`).

**Direct probe scenarios:** Reuse approved probes 3, 4, 8b, and 8c rather than inventing a second harness (`docs/parity/download-orchestration-plan.md:383-411`). Add no real-host access to local gates (`docs/parity/download-orchestration-plan.md:32`).

**Verification:** Spec review checks manual gates before retry/Peer, exact-one-component filenames, no shared credentials, cancellation/temp cleanup, and no unapproved dependency (`docs/parity/download-orchestration-plan.md:427-449`).

### U3. Add the shared post-download coordinator

**Goal:** Provide CurrentRun/Recovery enumeration, host callbacks, atomic semantic merge, prompt-state transitions, and typed reports.

**Requirements:** R5-R10, R12.

**Dependencies:** U1, including the landed CR-1 commit. U2 is not required for local FOMOD fixtures because the coordinator consumes acquisition outcomes, not host implementations.

**Files:** `src/fomod_post_download.rs` (new, dedicated owner), `src/fomod.rs` (dedicated owner after current ownership is confirmed), `src/lib.rs` (serialized after U1 releases it). `src/engine.rs` is read-only unless review finds a concrete install-gate defect; any expansion requires an explicit ownership handoff because install behavior stays there (`AGENTS.md:1-25`). The new module holds the host callbacks, configure/merge, prompt-state writes, Recovery discovery, and the report. It holds no CurrentRun eligibility logic.

**Approach:**

1. CurrentRun input is the `FomodFollowups` value from the lane's `fomod_followups` (`/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1394-1451`). U3 does not re-filter selection, re-run `should_prompt`, re-inspect for FOMOD, or re-dedup. If a needed field or rule is missing, U3 stops and files a change request to the Task 1 owner, as CR-1 does. Its `inspection_errors` become Failed results with their identity.
2. Validate component/resource binding and safe path before configuring an archive (`src/engine.rs:718-769`). Deduplication by component GUID + registry URL + path is the lane's job under CR-1 (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:73-99`).
3. Recovery mode is the only place this module decides eligibility, because Recovery has no acquisition run. It reuses the same `fomod::should_prompt` and inspection primitives the lane uses, so both modes apply one policy (`src/fomod.rs:648-712`). Configured/warned are skipped; dismissed/failed stay promptable.
4. Invoke the host, configure using existing parser/session/choices primitives, merge on a guide clone, and write status only to the exact resource (`src/fomod.rs:845-909`).
5. Return ordered typed results and aggregate gate; never swallow callback/extract/parse/merge failures (`docs/parity/FOMOD.md:7-13`).

**Direct probe scenarios:**

1. One selected Completed archive produces one Configure request and a Configured result; repeated CurrentRun input deduplicates (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:38-100`).
2. Non-FOMOD, unselected, completed-prefix, configured, and warned cases do not ask; dismissed asks again (`src/fomod.rs:667-712`).
3. Same basename in two components invokes two correctly bound requests and mutates only the chosen resource each time (`src/fomod.rs:675-712`).
4. Corrupt archive, extraction limit, unsafe registry name, missing file, ambiguous FOMOD root, invalid choices, host callback error, and merge error return component/archive-bound Failed without configured state (`src/fomod.rs:469-589`; `src/engine.rs:718-769`).
5. Cancellation between archives preserves earlier committed configuration and leaves later archives discoverable by Recovery (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:38-44`).
6. Recovery discovers registered existing FOMOD archives only; it ignores unrelated directory files, symlinks, unsafe names, and completed-prefix components (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:68-102`; `src/fomod.rs:720-725`).
7. Effective selected copies/options for a reconfigured archive match the C# oracle output of acceptance probe 7 (a new C# probe; none exists today) while preserving other archive choices (`src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:41-68`; `src/fomod.rs:845-909`).

**Verification:** U3's own gate is **spec review only** (Gate 2): API shape, consumption of `fomod_followups` without re-derivation, identity, atomicity, and failure typing, read against the source. U3 has no production entry point of its own, and this plan forbids Rust tests/harnesses, so U3 produces no behavioral receipt. Its behavioral receipt comes at U4: the U4 production CLI drives scenarios 1-7 on disposable fixtures and records input/output guide diffs, archive hashes, stdout/stderr/exit, and before/after manifests. U3 is not marked verified until those U4 receipts exist.

### U4. Add deterministic CLI presentation and recovery entry points

**Goal:** Route batch fetch, nxm, and explicit recovery through the shared follow-up API with deterministic modes and honest persistence.

**Requirements:** R2, R7-R12.

**Dependencies:** U1, U3; `src/main.rs` active owner has delivered and released download-orchestration Task 2a (`docs/parity/download-orchestration-plan.md:364-365`).

**Files:** `src/main.rs` only after ownership release; no Rust test files.

**Approach:** Reuse the approved `--fomod-skip`, `--fomod-choices`, `--interactive`, warn-continue, `--record`, and progress/report contracts (`docs/parity/download-orchestration-plan.md:328-340`). Add an explicit recovery mode/action that loads a guide and source directory, performs read-only discovery first, then configures only after the selected host mode permits it. Without `--record` or an output path, report that changes are process-local and do not claim restart persistence (`src/main.rs:288-364`; `src/model.rs:264-289`).

**Direct probe scenarios:**

1. Batch fetch and nxm for the same component yield the same follow-up/report/guide diff (`docs/parity/download-orchestration-plan.md:397-407`).
2. Skip, choices, warn-continue, interactive dismiss/configure/cancel, and recovery produce distinct typed outcomes; no TTY guessing occurs (`docs/parity/download-orchestration-plan.md:333-339`).
3. `--record` changes only the matched resource/file, archive choices, and prompt state; without it the source guide hash is unchanged (`docs/parity/download-orchestration-plan.md:397`).
4. Required Failed/Cancelled yields nonzero and no install continuation; ManualRequired has its distinct exit (`docs/parity/download-orchestration-plan.md:329-339`).

**Verification:** Real release CLI, disposable roots, byte diffs/manifests, argv/exit/stdout/stderr hashes, and copied binary hash (`docs/parity/download-orchestration-plan.md:370-425`). These receipts also carry U3 scenarios 1-7, which are U3's first behavioral evidence.

### U5. Wire the native desktop host after active UI ownership releases

**Goal:** Give batch/nxm component-bound paths the same run-bound follow-up behavior and restart recovery in the native GUI.

**Requirements:** R7-R12.

**Dependencies:** U1, U3, U4 contract approval; install-progress and download-orchestration UI owners release `src/ui.rs` (`docs/parity/download-orchestration-plan.md:365`).

**Files:** `src/ui.rs`, `src/ui/downloads_panel.rs`, and `src/fomod_ui.rs` only after explicit ownership handoff. No inline test edits (`docs/parity/ACCEPTANCE.md:26`).

**Approach:** The worker sends run-bound acquisition outcomes; main-thread GUI code invokes the shared coordinator/host against the exact guide snapshot, launches existing `fomod_ui`, applies results to the bound loaded guide, marks it dirty, and ignores stale run messages (`docs/parity/download-orchestration-plan.md:342-350`). Nxm must no longer stop after filename recording (`src/ui.rs:120-124`). On project load or an explicit recovery action, show registered pending archives without silently configuring them (`src/ui.rs:1780-1806`; `src/fomod.rs:667-712`).

**Direct native probe scenarios:**

1. Batch and nxm each launch one FOMOD editor for a fixture archive and commit the same guide mutation (`docs/parity/download-orchestration-plan.md:398-407`).
2. Dismiss prompts again on the next explicit fetch/recovery; configured and warned do not (`src/ui.rs:2871-2889`; `src/fomod.rs:667-712`).
3. Stale run completion after guide reload is ignored; it cannot mutate the new guide (`docs/parity/download-orchestration-plan.md:419-423`).
4. Callback/extract/merge failure remains visible, blocks install, retains the original guide, and offers recovery (`docs/parity/FOMOD.md:7-13`).
5. 1280x800 and 600x400 captures show Manual rows, FOMOD editor, failure/recovery state, and completion without claiming live-host evidence (`docs/parity/download-orchestration-plan.md:412-425`).

**Verification:** Native production binary, disposable guide/source/settings roots, screenshots plus transcript and before/after manifests; no source-only UI acceptance.

### U6. Produce evidence, compatibility disclosures, and acceptance updates

**Goal:** Bind claims to the integrated commit and retain every external/manual boundary honestly.

**Requirements:** R1-R12.

**Dependencies:** U2-U5 and all reviews.

**Files:** `docs/parity/hosts-fomod-orchestration-native/` (new), `docs/parity/ACCEPTANCE.md`, `docs/parity/FOMOD.md`, `docs/parity/GUI-CLI.md`, `docs/parity/GUI-INVENTORY.md`, `COMPATIBILITY.md`. Documentation ownership is serialized with any active evidence writer (`docs/parity/download-orchestration-plan.md:366-368`).

**Approach:** Record commit/binary/source hashes, commands, exits, stdout/stderr, manifests, guide diffs, screenshots, authorized-live status, and negative claims. Mark MEGA/DeadlyStream Manual gaps as parity regressions while they remain, and leave F13/F15 partial until their exact acceptance rows are satisfied (`docs/parity/ACCEPTANCE.md:48-50`; `docs/parity/download-orchestration-plan.md:455-469`).

**Verification:** Independent whole-package reviewer reads combined diff and receipts; no status advances from source presence or a build alone (`docs/parity/download-orchestration-plan.md:449-453`; `docs/parity/ACCEPTANCE.md:28-32`).

## Acceptance probes without Rust tests or harnesses

All probes use the real production CLI/native app and disposable roots outside repo/game/mod baselines. Fixture servers are ordinary external processes/scripts already approved by the acquisition plan, not Rust binaries or test targets (`docs/parity/download-orchestration-plan.md:370-425`).

1. **Static host matrix:** provider classification, planned Origin/Manual reason, credential source, retry class, Peer eligibility, and dependency inventory for all four hosts (`src/download_hosts.rs:14-30`; `docs/parity/download-orchestration-plan.md:270-275`).
2. **Local HTTPS acquisition:** reuse the acquisition plan’s transfer, challenge/login, retry, timeout, filename traversal, cancellation, Nexus premium/free, and redirect guards (`docs/parity/download-orchestration-plan.md:383-411`).
3. **CurrentRun enumeration:** two selected components, same basename, one unselected component, one non-FOMOD, configured/warned/dismissed states, and Completed versus AlreadyPresent provenance (`src/fomod.rs:626-712`).
4. **All-path convergence:** batch CLI, nxm CLI, batch GUI, and nxm GUI produce identical per-archive follow-up facts and effective guide mutation when given the same bound component/archive (`src/ui.rs:108-124`; `src/main.rs:633-653`).
5. **Failure matrix:** corrupt/unsafe/missing archive, archive budget, invalid XML/choices, extraction failure, callback failure, merge failure, record failure, cancellation, and stale GUI run. Each receipt proves no game writes and no false configured state (`src/fomod.rs:469-589`; `src/engine.rs:718-769`).
6. **Restart recovery:** terminate after verified archive publish but before configuration, restart, discover only the registered pending archive, configure it, persist, restart again, and observe no prompt. Repeat with dismissed and failed states, which must prompt again (`src/fomod.rs:667-712`).
7. **Semantic C# comparison (new C# probe).** No configured-component merger oracle exists. `docs/parity/FOMOD.md:15-31` measures group selection only, and the existing `fomod-selection-oracle` never calls the merger (`docs/parity/fomod-selection-oracle/Program.cs:1-30`). Add a new non-Rust probe at `docs/parity/fomod-merger-oracle/` on the pattern of `docs/parity/fomod-selection-oracle/`:
   - `probe.csproj`: a net9.0 console exe that references the prebuilt pinned `ModSync.Core.dll` by `HintPath` with `Private=false`, as `fomod-selection-oracle/probe.csproj` does. It never builds or edits the C# tree.
   - `Program.cs`: builds a component from inline ModuleConfig XML through `FomodParser.ParseModuleConfigXml` and `FomodToComponentMapper.Map`. It gives the target prior instructions for two archives, `a.zip` and `b.zip`. It calls `FomodConfiguredComponentMerger.MergeInto(target, configured, "a.zip")` twice, with different selections, and writes `result.json`: selected option GUIDs/names plus each instruction's action, source, and destination.
   - `receipt.json` with the C# commit `d31ccbd3`, the DLL hash, and source hashes; `verify.py` to compare, as in the selection oracle.
   - Rust side: the same inputs through the production CLI from U4. Compare selected options and effective archive-relative copies, not serialized shape. The `b.zip` instructions must survive both merges.
   This is a C# program, not a Rust test or harness, so the test freeze does not block it. Its `dotnet build` of the probe project is a separate authorized action at probe time.
8. **Authorized live hosts:** only after explicit authorization, one bounded Nexus premium/nxm sample and any approved DeadlyStream/MEGA sample. GameFront confirms the manual browser boundary rather than automating it (`docs/parity/ACCEPTANCE.md:48`; `src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:69-83`).
9. **Native matrix:** 1280x800 and 600x400 GUI captures for Manual, editor, dismiss/retry, failure/recovery, stale-run rejection, and configured completion (`docs/parity/download-orchestration-plan.md:412-425`).

Every receipt states: no Rust tests/harnesses created, changed, compiled, or run; no game/mod baseline mutated; real-host access yes/no; exact commit and binary hash; and which claims remain unverified (`docs/parity/ACCEPTANCE.md:26-32`).

## Review gates

### Gate 1 — Acquisition/host contract review

- Confirms U1 provenance exists once and Task 1c is the only host-policy owner (`docs/parity/download-orchestration-plan.md:355-368`).
- Confirms Manual is decided before retry/Peer and no credential/CAPTCHA/countdown automation exists (`docs/parity/download-orchestration-plan.md:427-449`).
- Confirms Nexus reuses `updates.rs`, filenames are one safe component, and no dependency was added without its written reason (`src/updates.rs:126-320`; `Cargo.toml:13-41`).

### Gate 2 — Shared FOMOD specification review

- Confirms CurrentRun cannot scan arbitrary files and Recovery cannot claim an archive was downloaded in the current run (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:33-102`).
- Confirms CurrentRun consumes the lane's `fomod_followups` and contains no second selection/prompt/inspection filter, and that CR-1 landed first (`/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1394-1451`). This review is U3's whole gate; behavior is proven by the U4 receipts.
- Confirms precise component/resource/archive identity, completed-prefix exclusion, deterministic order, cancellation boundaries, and idempotent statuses (`src/fomod.rs:648-712`; `src/fomod.rs:720-725`).
- Confirms merge is atomic, archive-scoped, preserves other archives, and proves effective instruction equivalence (`src/fomod.rs:845-909`; `src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:41-68`).
- Confirms all failure results retain identity and do not mark configured (`src/ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:76-99`).

### Gate 3 — Adapter review

- CLI schemas/exits/write-back are explicit; no TTY guessing or source-guide mutation without record/output (`docs/parity/download-orchestration-plan.md:328-340`).
- GUI callbacks run on the UI thread against the exact run-bound guide, stale messages are ignored, and nxm/batch converge (`src/ModSync.GUI/Services/DownloadOrchestrationService.cs:327-342`; `docs/parity/download-orchestration-plan.md:342-350`).
- Install remains blocked on required Failed/Cancelled/ManualRequired follow-up; adapters do not parse strings for business facts (`docs/parity/FOMOD.md:7-13`).

### Gate 4 — Evidence and whole-package review

- Direct CLI/native receipts bind source and binary hashes to the reviewed commit and include negative filesystem/network evidence (`docs/parity/download-orchestration-plan.md:370-425`).
- An independent reviewer checks the combined host + acquisition + FOMOD + adapters diff and all retained findings before ledger edits (`docs/parity/download-orchestration-plan.md:449-453`).
- F13/F15 remain partial for any missing live-host, corpus, restart, failure, or interface row (`docs/parity/ACCEPTANCE.md:48-50`).

## Risks and mitigations

| Risk | Referent | Mitigation |
|---|---|---|
| Active lanes overwrite or duplicate ownership | `docs/parity/download-orchestration-plan.md:355-368` | Record base/delivery commits and require explicit handoff before U1/U4/U5 edits. |
| Same filename mutates the wrong component | `src/fomod.rs:675-712`; `src/fomod.rs:961-985` | Use component GUID + registry URL + filename; do not use the broad recursive helper in acquisition follow-up. |
| Lane follow-ups drop a same-basename component | `/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1370-1375`, `:1419-1426` | CR-1 to the Task 1 owner; U1 probe 3 is its check; U3 waits for it. |
| Two FOMOD eligibility authorities drift | `/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1394-1451` | U3 consumes `fomod_followups`; Gate 2 rejects any second CurrentRun filter. |
| `src/updates.rs` edited by several lanes at once | `docs/parity/download-orchestration-plan.md:24-26`, `:361` | Task 1 owns it; U2 waits for Task 1, nxm-traversal-fix, integration/wave1, and the completion lane to release it. |
| Restart scan mislabels stale files as current downloads | `src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:33-102` | Separate CurrentRun provenance from Recovery discovery in API and receipts. |
| C# instruction merge and Rust saved choices diverge | `src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:21-69`; `src/fomod.rs:845-909` | Build the new C# merger probe (acceptance probe 7) and compare effective selected options/copies, including reconfiguration. |
| Manual boundary falls through to Peer | `docs/parity/download-orchestration-plan.md:410-411` | Typed gate precedes fallback; spec and socket probes prove Peer never starts. |
| Guide mutation is mistaken for persistence | `src/model.rs:264-289`; `docs/parity/download-orchestration-csharp-audit.md:229-233` | Report dirty/process-local state; require record/output or user save for restart claim. |
| A build is mistaken for parity | `docs/parity/ACCEPTANCE.md:28-32` | Ledger updates enumerate missing live/corpus/native/full-build evidence. |

## Open questions with defaults

1. **MEGA dependency.** Default: stay `Manual(Unsupported)`. A specialized decrypting client is allowed only after a written reason (MEGA content is encrypted and cannot use the plain HTTPS fetcher), maintenance/licence/security review, deterministic local fixture plan, and approval (`docs/parity/download-orchestration-plan.md:457`).
2. **DeadlyStream anonymous automation.** Default: remain Manual until Task 1c reviews an authorized, challenge-free recorded fixture; then automate only that bounded path and classify login/challenge Manual before retry (`docs/parity/download-orchestration-plan.md:458`).
3. **Recovery entry point.** Default: both an explicit CLI recovery mode and a GUI “Review downloaded FOMODs” action. Do not run configuration silently at startup; recovery discovery itself stays read-only (`AGENTS.md:1-25`; `src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:33-102`).
4. **Warn-continue persistence.** Default: mark warned in the in-memory guide and persist only via `--record`/output or desktop save. This matches the prompt policy without silently rewriting an input guide (`src/fomod.rs:667-712`; `docs/parity/download-orchestration-plan.md:328-337`).
5. **Standalone direct-download utility.** Default: remain unbound and do not mutate guide state. Add an explicit component/resource selector later only if the UI needs it; filename inference is insufficient (`src/ui.rs:108-119`, `src/ui.rs:2165-2180`).
6. **Failure aggregation.** Default: finish reporting already discovered candidates without configuring any later candidate after a required failure; return aggregate Failed and block install. This preserves useful diagnostics without backfilling work after failure (`AGENTS.md:1-25`; `docs/parity/FOMOD.md:7-13`).

None of these defaults blocks U1/U3 planning. U2’s automated MEGA/DeadlyStream branches remain conditional; their default Manual behavior is implementable and must be disclosed as partial parity (`docs/parity/download-orchestration-plan.md:455-469`).

## Definition of done

- Task 1/1c has one reviewed host policy and run report with safe component/resource/archive provenance (`docs/parity/download-orchestration-plan.md:355-368`).
- Every component-bound batch/nxm/AlreadyPresent/recovery path uses the shared post-download coordinator; the free-form utility is explicitly unbound (`src/ui.rs:108-124`; `src/main.rs:633-653`).
- CurrentRun and Recovery are distinguishable, registry-bound, safe, deterministic, and restart-proven (`src/ModSync.Core/Services/Fomod/FomodDownloadedArchivePaths.cs:33-102`).
- CLI/GUI host callbacks configure/dismiss/warn/cancel through one library contract; failures remain typed and block install when required (`src/ModSync.Core/Services/Fomod/IFomodPostDownloadHost.cs:12-32`; `docs/parity/FOMOD.md:7-13`).
- Reconfiguration is atomic and semantically matches the pinned C# configured-component merge, as measured by the new `docs/parity/fomod-merger-oracle/` probe (`src/ModSync.Core/Services/Fomod/FomodConfiguredComponentMerger.cs:21-69`; `src/fomod.rs:845-909`).
- Nexus premium/free behavior and update metadata are proven through the existing client/model, GameFront remains Manual, and any remaining MEGA/DeadlyStream Manual gap is disclosed (`src/updates.rs:247-320`, `src/updates.rs:351-443`; `docs/parity/download-orchestration-plan.md:455-466`).
- Direct CLI/native receipts, independent review, and authorized-live status are recorded; no Rust test/harness was created or run (`docs/parity/download-orchestration-plan.md:370-453`; `docs/parity/ACCEPTANCE.md:26-32`).
- F13/F15 ledger status advances only to the level actually demonstrated; F29 remains untouched without isolated full K1/K2/game evidence (`docs/parity/ACCEPTANCE.md:48-50`, `docs/parity/ACCEPTANCE.md:64`).

## Claims

claim: This plan does not duplicate the approved download orchestrator or Task 1c host lane.
referent: `docs/parity/download-orchestration-plan.md:355-368`; U1-U2 ownership above
checked: compared file ownership and responsibilities line by line
verdict: holds
confidence: high; an ownership change in the approved plan would require reconciliation

claim: The proposed coordinator covers every component-bound Rust download path identified in the pinned snapshot.
referent: `src/ui.rs:108-124`; `src/ui.rs:1809-1875`; `src/main.rs:633-653`; `docs/parity/download-orchestration-plan.md:340-350`
checked: `rg -n "Downloaded|NxmDownloaded|NxmDownload|download::fetch" src/ui.rs src/main.rs`
verdict: holds
confidence: medium; active acquisition/UI work can add entry points before implementation, so U1/U5 must repeat the call-site inventory

claim: The plan preserves Manual login, CAPTCHA/challenge, free-user wait, and GameFront countdown boundaries.
referent: R2; `docs/parity/download-orchestration-plan.md:270-275`; `src/ModSync.Core/Services/Download/GameFrontDownloadHandler.cs:69-83`
checked: host matrix, KTD6, U2, and acceptance probes all use typed Manual outcomes
verdict: holds
confidence: high; review Gate 1 rejects any fallback/bypass

claim: CurrentRun FOMOD eligibility has one authority, the Task 1 lane's `fomod_followups`, extended by CR-1 rather than re-derived.
referent: `/home/brunner56/.cache/modsync-wt-dl/src/download_orchestrator.rs:1370-1451`; KTD1, KTD2, U1 CR-1, U3 approach step 1
checked: read the lane function; confirmed U3 has no CurrentRun filter and names CR-1 as its dependency
verdict: holds
confidence: high; a Task 1 owner rejecting CR-1 would reopen it

claim: The plan contains no Rust test or Rust harness work.
referent: Binding constraints; U1-U6 file lists; `docs/parity/ACCEPTANCE.md:26`
checked: every unit and probe section reviewed for Rust test/harness paths
verdict: holds
confidence: high; implementation review must enforce the same restriction

claim: The plan does not authorize a new networking crate.
referent: KTD7; Open Question 1; `Cargo.toml:13-41`
checked: U2 permits Cargo edits only after a separately approved MEGA dependency decision with stated reason
verdict: holds
confidence: high; default delivery adds none
