# Shared selected-guide download orchestration plan

Status: plan only, 2026-09-27. Nothing here is implemented. **All Rust citations are pinned to `c28f4fc`** (`codex/rust-port` HEAD at revision time). The audit started at `cd8f0b0`, and every `ui.rs` line below was re-located at `f598129`. `git diff f598129..c28f4fc --stat` touches only `src/ui/installing_page.rs`, which this plan does not cite, so those line numbers hold at `c28f4fc`. They match `733bffb` as well. Pinned C# oracle `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Source evidence is in [download-orchestration-csharp-audit.md](download-orchestration-csharp-audit.md). This slice advances F12, F13, F14, F15 (post-download FOMOD), F23 (nxm handoff) and F26. It cannot close F13 live-host acceptance or full C# GUI parity by itself.

`src/ui.rs` is still being edited by the install-progress worker, so the function names given next to its line numbers are the durable referent.

Sibling agreements recorded here:

- P2P source abstraction agreed with `audit-bittorrent-integration`, which records the same interface in `docs/parity/p2p-acquisition-plan.md`. **The agreed default (P2P off until opt-in) is superseded** by lead ruling R1 (user override, below). The type shapes, race and Superseded semantics still stand. `p2p-acquisition-plan.md` is now owned by `p2p-privacy-feasibility` (coordinator ruling), which applies R1/R2/`PeerEndpoint` together with the tunnel design. It has not been edited from here, and until that lands it still shows the old default.
- Selection input agreed with `audit-mod-selection`.
- Sidebar ownership on completion pages agreed with `audit-completion-pages`.

## Lead rulings (binding, 2026-09-27 review)

- **R1: peer sharing is on by default.** This is a user override.
  - It ships with a discoverable off-setting and a one-line disclosure. There is no opt-in prompt.
  - All peer traffic goes through an IP-protecting tunnel. The tunnel design is still being probed.
  - Every Peer step fails closed when the tunnel is down: no peer sockets are opened, and the Peer source is skipped with `SourceFailed { will_fallback }`.
  - LAN discovery is off, because it broadcasts the IP outside any tunnel.
- **R2: gated targets never fall back to Peer.**
  - `ManualReason::{LoginRequired, Captcha, FreeUserWait}` are terminal with **no** Peer fallback. A peer copy must not get around a host gate.
  - `Unsupported` covers MEGA, pre-1c DeadlyStream and GameFront. It **may** fall back to Peer, but only for share-eligible resources: an authored `ContentHashSHA256` plus a `Magnet`.
  - `HtmlLanding` is treated as gated, because an unexplained HTML page may be a gate.
- **R3: the path-traversal fix ships first, on its own, in a separate worker.**
  - It covers `ui.rs:1811` and the unfiltered Nexus `file_name` at `updates.rs:204-213`.
  - It is not a task in this plan. This plan assumes it is **done before** Task 1, and relies on its one-component filename helper.

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md` and all unrelated worktree state. Never write `src/ui.rs` or `src/ui/installing_page.rs` while the install-progress worker owns them. The GUI task below starts only after that worker's slice is delivered.
- Rust tests stay frozen. Do not create, port, expand, modify, `cfg`-hide or run any Rust test, inline `#[cfg(test)]` block or disguised harness. Verification uses production builds, the real CLI, the real native desktop, disposable fixtures, manifests and independent review.
- No real-host network downloads are used as evidence in this slice. Live DeadlyStream, Nexus, MEGA and GameFront acceptance (F13) needs separate authorization from the user.
- Hosts that need a login, a CAPTCHA, a Cloudflare challenge, a JavaScript countdown or a free-user wait page are **manual boundaries**. Rust reports `ManualRequired` with the URL and a reason, and stops for that source. It never scrapes around, solves, replays cookies for, or headless-browses those gates. That includes FlareSolverr-style proxies. A peer copy never satisfies a gated target either (R2).
- One library owns every business fact:
  - which components qualify
  - which URLs and filenames are wanted
  - what already exists
  - the source order
  - retries
  - cancellation
  - verification
  - guide write-back
  - FOMOD follow-up eligibility

  The CLI and GUI only format events and retain presentation state. No frontend may re-derive selection, URL enablement, filename matching or FOMOD eligibility.
- Existing contracts stay additive and byte-compatible:
  - `download::fetch`/`fetch_with_options` (`src/download.rs:11-76`)
  - `download_queue::fetch_all`/`run_with` and the `download-batch` JSON job/result schema (`src/download_queue.rs:15-139`, `src/main.rs:223-233,864-890`)
  - `download-plan` JSON (`src/main.rs:180-190,794-820`)
  - `download-provider` (`src/main.rs:235,821-827`)
  - `nxm-download` (`src/main.rs:83-90,627-648`)
  - `torrent-download` (`src/main.rs:202-214,843-858`)
- Rust keeps its safety contracts even where C# does not:
  - HTTPS only
  - refuse an existing destination
  - temp file in the destination parent
  - SHA-256 when authored
  - `archive::verify`
  - `persist_noclobber` publish (`src/download.rs:33-74`)

  C# defects listed in the audit are not parity targets.
- Server-supplied filenames (Content-Disposition, Nexus `file_name`, MEGA node names) must reduce to exactly one normal path component before any join. Anything else fails that target.
- P2P follows R1. It is **on by default**, runs tunnel-only and fails closed, and ships with a one-line disclosure and a discoverable off-setting. Guide-authored peer addresses and trackers are ignored. Peers come only from the tunnel's discovery or from entries the user types. LAN discovery is off.

## Source findings that govern the design

**Answer to the key question.** No Rust frontend maps the selected guide to downloads. Each has its own partial, disconnected pieces:

| Concern | GUI today | CLI today | Shared library today |
|---|---|---|---|
| Which components | `ui.rs:503` filters `guide.components` by `is_selected` for a hyperlink list only | none; `download-plan` takes one `--component` GUID (`main.rs:181-190`, `:801-806`) | none |
| Which URLs | `ui.rs:503-521` lists `resource_registry` keys **plus** metadata `Link`/`DownloadLink`. C# uses registry keys only (audit §1) | caller-supplied `--url` (`main.rs:185`) | `download_plan::url_enabled` (`download_plan.rs:16-23`) |
| Which filenames | user types one name (`ui.rs:488`, `:2357`, `:2384-2390`) | caller supplies `--filename` candidates (`main.rs:187`) | `download_plan::select_filenames` (`download_plan.rs:89-141`), a port of C# `GetFilenamesForDownload` |
| Existing-file skip | none (`fetch` errors on an existing destination, `download.rs:35`) | none | none |
| Host handling | none; tells users to download provider pages in a browser (`ui.rs:480`) | `download-provider` classification only (`download_hosts.rs:14-30`) | provider enum only |
| Queue / retry | single job (`ui.rs:2384-2399`, inside `start` at `:2216`) | `download-batch` over hand-authored JSON jobs, 25 ms fixed retry delay (`download_queue.rs:55-139`) | `run_with` |
| Progress | none: "no aggregate download percentage is available" (`ui.rs:523`) | none; final JSON array only | none |
| Cancellation | shared `settings.cancelled` into `fetch` (`ui.rs:2395`) | `AtomicBool::new(false)`, never set (`main.rs:645,837,876`) | per-chunk check (`download.rs:58`) |
| Post-download FOMOD | GUI-only predicate `should_offer_fomod_editor` (`ui.rs:2125-2152`) and `Message::Downloaded` handler (`ui.rs:106-117`) | none | `fomod::should_prompt`/`resource_matches_archive`/`prompt_status` (`fomod.rs:640-676`) |
| nxm handoff | resolves the component, downloads into the mods dir, records `Files[name]=true` (`ui.rs:1769-1836`, `download_pending_nxm`/`record_nxm_download`) | downloads to a caller path, **no** component resolution, **no** record (`main.rs:627-648`) | `protocol::resolve_nxm`, `NexusClient::nxm_download_link`, `updates::record_nxm_download` |

Rust defects found while auditing. Defect 2 belongs to the separate R3 worker. Defects 1 and 3-6 are Task 1. Defect 7 is Task 2a.

1. **Timeout semantics are implicit.** The blocking `timeout(300s)` (`download.rs:36-39`) applies per operation, not per body.
   - It bounds `send` until headers arrive (reqwest-0.12.28 `blocking/client.rs:1438,1453`) and then each `Read::read` call (`blocking/response.rs:439-440`). The inner async client gets no total timeout (`blocking/client.rs:1361-1362`).
   - Large archives therefore do **not** fail at five minutes. The actual gap is that there is no explicit idle-versus-overall policy: a stalled stream hangs up to 300 s per read, and nothing caps a whole transfer.
   - Make the idle timeout explicit and documented. Retry classification treats it as transient.
   - Low severity.
2. **Unsanitized Nexus filename** (fixed **before** this plan by the separate R3 worker; listed here for the record only). The GUI joins the API-supplied `file_name` straight onto the mods dir: `mods.join(&download_link.file_name)` (`ui.rs:1811`, `download_pending_nxm`). The name comes from Nexus metadata unfiltered (`updates.rs:204-213`). A rooted or `..` name escapes the workspace. `persist_noclobber` does not prevent this.
3. **Computed hash discarded.** `fetch_with_options` computes SHA-256 and throws it away when none was expected (`download.rs:55-71`). Seeding and hash-proposal flows need it.
4. **Substring host classification.** `host.contains("mega.nz")` also classifies `evilmega.nz`, and the same holds for every provider (`download_hosts.rs:20-29`). Use host equality or a dot-suffix match. This also covers `mega.co.nz`, which C# misroutes (audit defect H13).
5. **Blind retry.** `download_queue` retries every error after 25 ms, including 4xx responses, HTML landing pages and SHA mismatches (`download_queue.rs:116-119`). Retries must be limited to transient classes.
6. **Challenge detection runs after status classification.** `download.rs:40` calls `send()?.error_for_status()?` before the HTML/content-type check at `:41-49`.
   - A Cloudflare challenge served as 503 becomes a transient 5xx and would be retried.
   - One served as 403 becomes a plain HTTP failure instead of `ManualRequired`.
   - Fix: classify the body first. Inspect the status, content type and a bounded body prefix for challenge, login or countdown markers **before** `error_for_status`. A detected gate yields `ManualRequired`, which is never retried.
   - This mirrors C# defect H12 (`DeadlyStreamDownloadHandler.cs:911`), which is not copied.
7. **CLI cancellation never set.** The flag is `AtomicBool::new(false)` and never set (`main.rs:837,876`). `main.rs` installs no signal handler (a grep for `ctrlc|ctrl_c|signal` in `src/main.rs` finds nothing). By default, SIGINT terminates the process without unwinding, so the `NamedTempFile` in the destination directory is probably left behind. This is reasoned from source; no probe has run it.

## Acquisition-source interface (converged with audit-bittorrent-integration; amended by R1 and R2)

New module `src/acquisition.rs`. Names may change in review only for a concrete compatibility or ownership reason. The semantics are fixed.

```rust
pub enum SourceKind { Existing, Origin(download_hosts::Provider), Peer, Manual }

pub enum ManualReason { LoginRequired, Captcha, HtmlLanding, FreeUserWait, Unsupported }

pub enum Source {
    Existing { path: PathBuf },                                  // verified local file satisfies the target
    Origin   { url: String, provider: download_hosts::Provider, requires: OriginRequirement }, // direct HTTPS or host API
    Peer     { magnet: String, sha256: String, peers: Vec<PeerEndpoint> }, // only with authored sha256+magnet; peers from user settings only
    Manual   { url: String, reason: ManualReason },               // terminal; never automated around
}

// PENDING the tunnel design (R1): the agreed shape used `Vec<SocketAddr>`, which hard-codes IP
// transport and would bypass an IP-protecting tunnel. `PeerEndpoint` is an opaque,
// transport-agnostic handle (e.g. a tunnel-scoped peer id or an overlay address) whose
// concrete variants are fixed only after the tunnel probe lands. No `SocketAddr` crosses this API.
pub struct PeerEndpoint(/* pending tunnel design */);

pub enum OriginRequirement {
    None,            // direct HTTPS, or an nxm link carrying key+expires
    NexusPremium,    // nexusmods.com URL with an API key; `run` validates premium once per run
}

pub struct ArchiveTarget {
    pub component_guid: String,
    pub registry_url: String,
    pub filename: Option<String>,        // None until an origin reveals it; always one normal component (see Destinations)
    pub expected_sha256: Option<String>, // ResourceRegistry[url].ContentHashSHA256
    pub sources: Vec<Source>,            // already in fallback order
}

pub trait Acquire: Send + Sync {
    fn kind(&self) -> SourceKind;
    fn acquire(&self, target: &ArchiveTarget, destination: &Path,
               cancelled: &AtomicBool, on: &mut dyn FnMut(AcquisitionEvent)) -> Result<Acquired>;
}
pub struct Acquired { pub path: PathBuf, pub sha256: String }

pub struct AcquisitionEvent {
    pub target_index: usize,
    pub component_guid: String,
    pub filename: Option<String>,
    pub source: SourceKind,
    pub phase: Phase,
}

pub enum Phase {
    Resolving,
    Transferring { bytes: u64, total: Option<u64>, peers: Option<u32> }, // peers: librqbit fact, None for Origin
    Verifying,
    Completed { path: PathBuf, sha256: String },
    Skipped { reason: SkipReason },               // e.g. AlreadyPresent, UrlDisabled, AllFilesFlaggedFalse
    SourceFailed { error: String, will_fallback: bool },
    Superseded,                                   // decided by the orchestrator, never by error text
    ManualRequired { url: String, reason: ManualReason },
    Cancelled,
}
```

Guide schema: add raw `ResourceRegistry[url]` keys `ContentHashSHA256` and `Magnet`. These are the names C# used before `93c74e57` removed distributed cache. `resource_registry` is a raw `serde_json::Map` (`model.rs:56`), so they round-trip without a struct change. `ResourceRegistry[url].Peers` is **not** honored.

Source order per target:

1. `Existing`, when a matching file is present. A name match is always required. If `ContentHashSHA256` is authored, the hash must match too; otherwise `archive::verify` must pass.
2. If `peer_mode == Race` **and** the tunnel is up **and** the target has an authored sha256 and a magnet: `Origin` (automatable) **races** `Peer`. Otherwise `Origin` runs alone.
   - Each racer gets a child `AtomicBool` linked to the parent cancel, and its own temp in the destination parent.
   - The first `Ok` wins.
   - The orchestrator then sets the loser's flag and **joins** it before returning, and reports it as `Superseded`.
   - Reference: the removed C# race, `97b00c48^:src/KOTORModSync.Core/Services/Download/DownloadCacheOptimizer.cs:78-106`. Its post-win "wait" re-awaits an already-finished `WhenAny` and joins nothing. That defect is not copied.
3. `Peer` alone. It needs an authored sha256 and a magnet, and the tunnel up. Allowed in two cases:
   - `Origin` failed with a non-gate error, such as a network, 5xx, verify or hash failure.
   - The target's only other source is `Manual(Unsupported)`.
4. `Manual`, which is terminal.
   - `LoginRequired`, `Captcha`, `FreeUserWait` and `HtmlLanding` end the target **without** trying Peer (R2). This applies even when `Origin` discovers the gate at run time; `Peer` never starts after a `ManualRequired` with a gated reason.
   - `Unsupported` ends the target only when it is not share-eligible.

Setting `peer_mode: Off | Race | Fallback`. Defaults, stated explicitly per R1:
- `Race` when the tunnel is up. There is no opt-in step.
- Effectively `Off` when the tunnel is down: Peer steps fail closed, with no sockets opened.
- `Off` when the user turns sharing off in settings.

Decision criterion: the user's stated intent (peers in addition to host links; making Nexus rate limits unimportant) and the R1 override. Host download counts are not the criterion.

Row O1 in `p2p-acquisition-plan.md` is this plan's Task 1: the types live in `acquisition.rs`, and the orchestrator lives in `download_orchestrator.rs`. That plan's P2 may parse a guide-authored `Peers` key only to warn that it is ignored.

Ownership: this slice owns `acquisition.rs`, the orchestrator, the Origin/Existing/Manual implementations and the frontend adapters. The BitTorrent slice owns the `Peer` implementation of `Acquire` inside `src/p2p.rs` (wrapping `p2p::fetch`, `p2p.rs:73`) and the magnet/sha256 authoring flow.

## Shared library API

New module `src/download_orchestrator.rs` (exposed from `lib.rs`):

```rust
// Selection input comes only from selection.rs (owned by the mod-selection slice):
//   selection::download_candidates(&guide.components, Scope) -> Vec<DownloadCandidate { index, guid }>

pub struct PlanOptions {
    pub source_dir: PathBuf,           // mod workspace
    pub resolution: Option<(u32, u32)>, // explicit; never shells out per call (C# defect)
    pub peer_mode: PeerMode,            // default Race (R1); effective Off while the tunnel is down
    pub user_peers: Vec<PeerEndpoint>,  // PENDING tunnel design; never SocketAddr (R1)
    pub nexus_key_present: bool,        // presence only; the key itself stays in RunSettings
}
// Nexus premium status is NOT a plan input; `plan` cannot know whether a key is free.
// No key: `plan` emits `Manual(FreeUserWait)`. Any key: `plan` emits non-nxm Nexus URLs as
// `Source::Origin { requires: OriginRequirement::NexusPremium, .. }`. `run` validates the key once per run via
// `NexusClient::validate` (updates.rs:247-275) *inside the library*; if not premium, every
// such target becomes `ManualRequired(FreeUserWait)` without a download request.
// Adapters never call validate. `--dry-run` reports these targets as "premium unknown".

pub struct AcquisitionPlan { pub targets: Vec<ArchiveTarget>, pub skipped: Vec<PlannedSkip> }

pub fn plan(guide: &Guide, candidates: &[selection::DownloadCandidate], options: &PlanOptions)
    -> Result<AcquisitionPlan>;

pub struct RunSettings {
    pub workers: usize,              // default 4
    pub retries: usize,              // transient classes only
    pub archive: archive::Options,
    pub nexus_api_key: Option<String>,
}

pub struct RunReport { pub outcomes: Vec<TargetOutcome> } // one per target, in plan order

pub fn run(plan: &AcquisitionPlan, settings: &RunSettings, cancelled: &AtomicBool,
           observe: impl FnMut(AcquisitionEvent) + Send) -> Result<RunReport>;

pub fn nxm_target(guide: &Guide, link: &protocol::Link) -> Result<ArchiveTarget>;
// Resolves the component with protocol::resolve_nxm (Matched only) and returns one target with
// an Origin source built from the nxm key+expires. Both the GUI handoff and CLI
// `nxm-download --input` wrap it in a one-target AcquisitionPlan and call `run` and `record`.
// There is no second orchestration path; `ui.rs:1769-1836` is replaced, not duplicated.

pub fn record(guide: &mut Guide, report: &RunReport) -> Result<RecordSummary>;
// Sets ResourceRegistry[url].Files[name] = true for Completed/AlreadyPresent (generalizes
// updates::record_nxm_download), and records ContentHashSHA256 only when a user-approved
// flag says so. Never mutates Files=false entries.

pub fn fomod_followups(guide: &Guide, report: &RunReport) -> Vec<FomodFollowup>;
// Lifts ui.rs:2125-2152 (should_offer_fomod_editor) into the library. Mirrors C# FomodPostDownloadOrchestrator.cs:38-70:
// selected components only, archives actually acquired or present in this run, positive FOMOD
// detection, and fomod::should_prompt (configured/warned suppress; dismissed re-prompts).
// Runs once after the batch, and not after a cancelled run (C# DOS:321-342, MBC:3535).
```

Planning rules:

- **Components.** Only `download_candidates(…, Scope::All)` over the selected set, in guide order. This matches the C# GUI (`DownloadOrchestrationService.cs:124-126`), which includes selected widescreen mods. The C# CLI downloads every component with a URL whatever its selection (audit §4); that is recorded as a defect, not a target.
- **URLs.** `resource_registry` keys only, filtered by `download_plan::url_enabled`. Metadata `Link`/`DownloadLink` stay presentation hyperlinks and are never acquisition input.
- **Filenames.** `download_plan::select_filenames` with candidates taken from `Files` keys. Every `Files` key is first passed through the one-component rule from R3's helper (`paths::relative(name, false)` with exactly one component, as the GUI already does for typed names at `ui.rs:2385-2389`). A key that fails is a planned `Skipped{UnsafeFilename}` and is never joined for Existing detection or as a destination. When there are no keys, the target filename is `None` and the Origin reveals it. C# additionally asks the host live (`ResolveFilenamesAsync`). A live resolve is only added per host in Task 1b, never by scraping gated pages.
- **Destinations** (`destination` in `Acquire::acquire`):
  - With a known filename, the destination is `source_dir.join(one_component(filename))`. The check runs in `plan`, before any join.
  - With an unknown filename (`None`), `acquire` receives `source_dir` as a **directory** destination. The source then does three things:
    1. It streams into a temp file in `source_dir`.
    2. It derives the name from the server: Content-Disposition, API `file_name` or the URL segment.
    3. It applies the same one-component rule **inside the library, after the transfer and before publish**. That point is `persist_noclobber` to `source_dir.join(name)`.
  - An unsafe server name fails the target and removes the temp file. Nothing is published.
  - The rule lives in the library, never in an adapter.
- **Existing detection.**
  - Filename matching is ASCII case-insensitive, to match C#'s `OrdinalIgnoreCase` `Files` dictionary and to avoid C#'s Linux `File.Exists` case bug (audit §2).
  - A name match plus a successful `archive::verify` is required. If an authored hash exists, it is verified as well.
  - It is a planned `Skipped { AlreadyPresent }`, never a download.
- **Provider mapping.**

  | Host | Source |
  |---|---|
  | Direct HTTPS | `Origin` |
  | Nexus with an nxm key+expires | `Origin` |
  | Nexus `nexusmods.com` URL with no API key | `Manual(FreeUserWait)` at plan time. Gated, so no Peer fallback (R2) |
  | Nexus `nexusmods.com` URL with any API key | `Origin { requires: NexusPremium }`. `run` validates premium once per run, in the library. A free key yields run-time `ManualRequired(FreeUserWait)` with no download request, and no Peer fallback (R2) |
  | GameFront | `Manual(Unsupported)`, matching C#'s behavior (not its status). Peer fallback is allowed only if share-eligible (R2) |
  | DeadlyStream | `Manual(Unsupported)` (not yet automated) until Task 1c. C# downloads anonymously through a csrf scrape in the common case (audit §6), so no login gate is asserted. After 1c, a *detected* Cloudflare challenge is `ManualRequired(Captcha)` and a login page is `ManualRequired(LoginRequired)`, never retried around |
  | MEGA | `Manual(Unsupported)` until a vetted decrypting client is chosen (open question 1). Peer fallback is allowed only if share-eligible (R2) |

- **Resolution filter.** Parity with C# `ResolutionFilterService` (exact `WxH` match; untagged passes). Evaluated once per plan.

Run rules:

- The bounded worker pool reuses `download_queue::run_with` semantics (ordered results, per-job cancellation check) and is extended additively with an observer. 4 workers is a **choice**, not a parity value: it matches the existing `download-batch` default (`main.rs:225`). C# effectively runs one download at a time in the GUI and 10 components at a time in the CLI (audit §3). Rust keeps 4 unless native evidence argues otherwise.
- Gate classification (defect 6) runs **before** status classification, so a challenge page is never a retryable 5xx.
- Retries only for connect/reset/timeouts and HTTP 408/429/5xx that are not gate pages.
  - 429 honors `Retry-After`, capped.
  - Never retried: 4xx, HTML, a hash mismatch, a verify failure, or `ManualRequired`.
- Cancellation:
  - Flag checked per chunk and between targets.
  - Temp files removed on every error path (C# removes them only on cancel).
  - No partial file is ever published, unlike C# `DownloadHelper.cs:96-102`.
- `Completed` is emitted only after verify and publish succeed. The terminal `Result` stays authoritative for failures. Nothing is inferred from error text.
- The report is per-target and per-URL, keeps every file name for a URL (C# keeps one), and is never keyed "first failure wins".

## Shared facts versus presentation

Shared facts: everything in `AcquisitionEvent`, `AcquisitionPlan`, `RunReport`, `RecordSummary` and `FomodFollowup`.

Presentation only:

- speed and ETA from the monotonic clock; C# uses a lifetime average on `DateTime.Now` and has no ETA
- the aggregate percentage
  - Byte-weighted when every active total is known; otherwise a completed/total count. Never an unweighted percent average (C# `DownloadProgress.cs:430`).
  - The unit must be labeled. C# says "mods" while counting components.
- sidebar LED, status and icons (C# `DownloadIndicatorUiHelper.cs:15-45`)
- per-row buttons, spoiler-free names, window visibility

C# mixes these into one `INotifyPropertyChanged` model (`DownloadProgress.cs:438-592`). Rust must not.

Sidebar visibility stays as it is: from `DownloadsExplain` on, through the completion pages (`ui.rs:445-449` `downloads_visible`, matching C# `WizardHostControl.axaml.cs:182,701`). Only its text source changes, to the shared run snapshot. No seeding status appears on completion pages.

## Frontend adapters

CLI (`src/main.rs`):

- New `fetch-downloads <guide> --source-dir <dir>`.
- Options:
  - `--select`, reusing the existing selector parsing
  - `--workers` and `--retries`
  - `--nexus-mods-api-key`, `--settings-dir` and env, reusing the `nxm-download` resolution order at `main.rs:640-643`
  - `--peer-mode off|race|fallback`. The default is `race`, per R1. It runs tunnel-only and fails closed when the tunnel is down.
  - `--nexus-api-base <https-url>`, hidden and probe-only (open question 10). When it is set:
    - The key is taken **only** from an explicit `--nexus-mods-api-key`. Settings, the profile store and env are never read, so a real key cannot leak to a fake server.
    - `NexusClient::with_base_url` (`updates.rs:131-141`) enforces `https`. Today it accepts any scheme, and its client has no `https_only`.
  - `--extra-ca-file <pem>`, probe-only (open question 3). It is honored **only** when the request host is a loopback address or `localhost`, and rejected with an error for any other host.
    - reqwest follows redirects by default, keeping the same trust store, and `download.rs:36-40` sets no redirect policy. The client built with the extra CA must therefore use `redirect::Policy::custom`, which **re-checks that every hop's host is loopback** and fails the request otherwise.
    - If review prefers it, the client can use `Policy::none()` instead. A redirect then fails the target.
    - The extra CA is never added to the default client.
  - `--max-expanded-bytes` and `--scratch-dir`
  - `--dry-run`: runs `plan` only, prints the `AcquisitionPlan` JSON, and makes no network requests or filesystem writes. Nexus premium-gated targets are reported as `"premium": "unknown"`, because premium is resolved only in `run`.
  - `--progress-json`: NDJSON `{"type":"acquisition","event":{…}}`, the envelope convention from install-progress
  - `--record`: writes the guide back via `record`, atomically, only when asked. It is opt-in because the CLI's guide argument is often a shared or version-controlled build file, and an unrequested rewrite would change a file the user did not ask to modify. The GUI instead edits an in-memory loaded guide that the user saves explicitly.
- Final `RunReport` JSON goes to stdout. Exit is nonzero if any target failed. `ManualRequired` targets get a distinct exit code and a listed summary, so scripts can tell "needs a human" from "broken".
- Ctrl+C sets the shared flag. Existing commands are unchanged.
- Post-download FOMOD in the CLI goes through `fomod_followups` and a mode that mirrors C# `FomodPostDownloadOptionsResolver.cs:157-229`:
  - `--fomod-skip` gives SkipAll and marks archives dismissed
  - a choices file (`--fomod-choices`, the same spelling as install args at `main.rs:295-297`) applies choices
  - `--interactive` prompts on stdin
  - otherwise, warn-continue marks archives `warned`, with no TTY guessing, so scripts stay deterministic. This is a deliberate difference, recorded as such.

  C# CLI exits 0 despite download failures (`ModBuildConverter.cs:2346`). Rust exiting nonzero is a deliberate difference.
- `nxm-download` gains an optional `--input <guide>`, which calls `nxm_target` then `run` then `record`. That is the same library path as the GUI handoff, and it removes the CLI/GUI nxm divergence.

GUI:

- New `src/ui/downloads_panel.rs` owns the Fetch Downloads / Stop / Open folder / Show downloads presentation.
- `src/ui.rs` only wires the worker. It sends `Message::Acquisition(run_id, AcquisitionEvent)` and `Message::AcquisitionDone(run_id, Result<RunReport>)`, then calls `record` and `fomod_followups` on the main thread against the exact guide snapshot bound to `run_id`.
- The existing free-form direct download stays as a utility.
- Late messages from an old run are ignored.
- The sidebar and footer show real run state. C#'s wizard sidebar never updates: `WizardHostControl.UpdateDownloadUI` (`WizardHostControl.axaml.cs:652`) has no callers, so it always reads "Downloads idle" / "Downloads ready". Rust matches C#'s strings (`DownloadIndicatorUiHelper.cs:15-42`) but feeds them from the shared snapshot. Truthful state is a deliberate difference.
- Peer sharing presentation (R1): one line of disclosure in the Downloads panel, for example "Downloads also use other ModSync users through a private tunnel. Turn off in Settings." The final copy goes through a plain-UI-copy pass. The off-setting sits in Settings and is linked from that line. Peer status shows "Peer sharing paused" while the tunnel is down.
- The GUI FOMOD follow-up uses the existing `fomod_ui` editor. C# ignores its own `fomodPostDownloadMode` setting in the GUI (audit G16); Rust does not add such a setting.
- Navigation while downloading keeps the current `wizard_gate` reading rule (`wizard_gate` at `ui.rs:382`, rule at `:388-389`).
- Validate and Install stay blocked by the existing receiver-serialization. C# says "Installation will wait for downloads to complete" (`DownloadsExplainPage.axaml:32`). Rust blocks with a stated reason and does not auto-wait. That is a deliberate, recorded difference.
- `DownloadsExplain` copy is aligned to what Rust actually does (the Fetch button starts downloads). C#'s "ModSync will now begin downloading" (`DownloadsExplainPage.axaml:19`) is not true there either: the page triggers nothing (`DownloadsExplainPage.axaml.cs:37-47`).

## File ownership and task sequence (serialized)

| Task | Owned files | Responsibility |
|---|---|---|
| 0 (other slice) | `src/selection.rs` | `download_candidates` / `Scope`, per audit-mod-selection |
| pre (separate worker, R3) | not this plan | Path-traversal fix plus the one-component filename helper. **Done before Task 1**; Task 1 starts from its approved commit |
| 1 | `src/acquisition.rs` (new), `src/download_orchestrator.rs` (new), `src/lib.rs`, `src/download.rs`, `src/download_queue.rs`, `src/download_hosts.rs`, `src/updates.rs` (`with_base_url` HTTPS enforcement, premium validate reuse, nxm helper reuse) | Types, plan/run/record/followups/`nxm_target`, Rust defects 1 and 3-6, additive observer; old APIs unchanged |
| 1b (other slice) | `src/p2p.rs` | `Peer` `Acquire` impl, per audit-bittorrent-integration |
| 1c | `src/download_hosts.rs` plus optional `src/download_hosts/*.rs` (new) | Per-host Origin/Manual classifiers: Nexus API/nxm. DeadlyStream anonymous path only after review of a challenge-free fixture. MEGA only after the open question is settled |
| 2a | `src/main.rs`, `Cargo.toml` and `Cargo.lock` (add **one** signal crate, `ctrlc` or `signal-hook`, whichever review picks; the existing tokio `signal` feature at `Cargo.toml:40` is async-only) | `fetch-downloads`, `nxm-download --input`, Ctrl+C wiring, probe-only flags with their guards |
| 2b | `src/ui.rs`, `src/ui/downloads_panel.rs` (new), `src/wizard.rs` only for copy, `src/main.rs` only for the `Gui` probe args (after 2a) | Starts only after the install-progress GUI worker's delivery. Run-bound adapter, sidebar/footer text from the snapshot, FOMOD follow-up via the library |
| 3 | `docs/parity/download-orchestration-native/` (new), `ACCEPTANCE.md`, `GUI-CLI.md`, `GUI-INVENTORY.md`, `COMPATIBILITY.md` | Evidence and honest status. `COMPATIBILITY.md` records the MEGA and DeadlyStream manual boundaries as **parity regressions** (open questions 1 and 2), plus the deliberate differences: truthful sidebar, nonzero CLI exit, no install auto-wait, no TTY guessing, selected-only CLI. No source edits |

Workers do not edit outside their ownership without first reporting the exact expansion needed.

## Acceptance probes without Rust tests

All probes use disposable roots outside the repo and user game trees, with manifests before and after each step, a copied release binary and its hash, and full argv/exit/stdout/stderr hashes. None touch a real host.

1. **Plan-only.** `fetch-downloads --dry-run` on a fixture guide covering:
   - selected, unselected and widescreen components
   - `Files` true/false/absent
   - `url_enabled` false
   - case-variant existing files
   - Nexus with and without a key, GameFront, MEGA, DeadlyStream

   Compare the target list, skips and Manual reasons to the C# rules in the audit. Record the C# CLI's select-ignoring behavior as a difference.
2. **Existing skip.** A pre-placed valid archive gives `Skipped{AlreadyPresent}` and zero network. A pre-placed corrupt archive is not skipped.
3. **Transfer, cancel, retry and timeout** against a loopback HTTPS origin:
   - two archives with byte progress; an HTML response (no retry); a 503 then 200 (one retry); a 404 (no retry)
   - a stalled body that trips the explicit idle timeout (defect 1); and a slow but steady body over five minutes that must succeed
   - a traversal Content-Disposition (rejected)
   - Ctrl+C mid-body: no published file, temp removed, exit code
   - a challenge page served as 503 and as 403: `ManualRequired`, zero retries (defect 6)
   - Needs the open-question trust hook (loopback-only `--extra-ca-file`), because `reqwest` uses `rustls-tls` webpki roots only (`Cargo.toml:36`).
4. **Nexus via a local fake API** (`--nexus-api-base`, open question 10). Cases:
   - `download_link.json` in both array and object shapes
   - a 429 with `Retry-After`
   - a traversal `file_name` (a regression check on the R3 fix)
   - a non-premium `validate` response, which must give `FreeUserWait` with zero download-link requests and exactly one validate call per run
   - an `http://` base, which must be refused
   - settings and env holding a key while the base is overridden: the key must not be sent, as shown by a request log on the fake server
5. **Record.** `--record` writes `Files[name]=true` only for completed/present targets. The byte diff of the guide touches nothing else.
6. **nxm parity.** `nxm-download --input` and the GUI handoff must produce identical guide diffs, both through `nxm_target`, `run` and `record`.
   - The GUI side needs the Task 2b probe arguments on `modsync gui`: `--nexus-api-base`, `--nexus-mods-api-key` and `--extra-ca-file`. They carry the same guards as the CLI: hidden, HTTPS-only base, explicit key only, loopback-only CA with the loopback redirect policy, and never persisted to settings.
   - **Single-instance isolation (required).** `single_instance::claim_or_forward` (`single_instance.rs:27-37`, `claim_at` `:40-71`) binds the per-user socket `modsync-rs-{user}.sock` (`:36`). If a GUI is already running, only the protocol string is forwarded (`:44-46`, `forward` `:74-78`).
     - Without a guard, `modsync gui --nexus-api-base … --protocol nxm://…` would drop the probe arguments. The user's real session would then fetch from real Nexus with the stored key.
     - The guard: when any probe argument is present, `main.rs` forces `allow_multiple_instances = true` (`main.rs:44`). That already skips `claim_or_forward` entirely (`ui.rs:3096-3100`), so the process never forwards anything and never binds the socket.
     - It also **requires** an explicit `--settings-dir` that is not the default settings directory. If that is missing or equals the default, it refuses to start with an error.
     - Probe receipts record the throwaway settings dir and confirm that the user's socket received no message.
   - The nxm URL is delivered through the existing `--protocol` argument (`main.rs:39-41`). `accept_protocol` already queues `Link::Nxm` (`ui.rs:1748-1753`).
   - If review rejects the GUI probe arguments, the GUI half of this probe is **live-auth-only**.
7. **FOMOD follow-up.** Downloading a fixture FOMOD archive yields exactly one follow-up. Configured and warned archives yield none. A dismissed archive prompts again, as C# `FomodDownloadPromptState.cs:40-49` and Rust `fomod::should_prompt` (`fomod.rs:667-673`) both require. A non-FOMOD archive and an unselected component yield none. The CLI covers skip, choices, warn-continue and interactive modes.
8. **P2P race** (jointly with the BitTorrent slice, local seeder from `torrent-seed`): the winner is published once, the loser is `Superseded` and joined, and no stray temp files remain. **Pending the tunnel design (R1)**; the probe runs through the tunnel once it exists.
8a. **Fail-closed tunnel (R1).** With the tunnel down and `peer_mode=race` (the default), a share-eligible target opens no peer sockets. Check with a socket listing or `ss` diff of the owned process, and confirm that Origin still runs. LAN discovery sends no broadcast packets. Check with a capture on the loopback or LAN interface of a disposable netns.
8b. **Gates stop Peer (R2).** This probe needs open questions 3 and 10 (loopback origin, fake Nexus). A share-eligible target whose Origin returns a login page, a challenge page or a Nexus free-user result ends as `ManualRequired`, and `Peer` never starts. The tunnel state is irrelevant: the gate decides before any Peer step.
8c. **Unsupported falls back to Peer (R2).** This probe needs open question 11, because with the tunnel down it can only show fail-closed, which is covered by 8a. A share-eligible `Manual(Unsupported)` MEGA target falls back to `Peer` through the tunnel. A non-eligible one does not.
9. **Native GUI matrix** at 1280x800 and 600x400:
   - idle sidebar
   - Fetch starting
   - two concurrent rows with bytes
   - a Manual row with its reason and open-link action
   - Stop confirmation and actual cancel
   - completion pages still showing the sidebar
   - FOMOD editor launched from a follow-up
   - stale-run message ignored after reload
   - peer disclosure line, off-setting, and "Peer sharing paused" with the tunnel down

   The transfer rows (Fetch starting, concurrent rows), the Stop/cancel rows and the FOMOD-follow-up rows need a loopback origin. They run with the 2b `modsync gui` probe arguments, which carry the same guards as the CLI; if review rejects those arguments, the rows are **live-auth-only**. The idle, Manual, completion-page, stale-run and disclosure rows need no network.

Each receipt also states that no Rust tests or harnesses were created, changed or run.

## Review gates

- **Task 1 spec review.** Checks:
  - selection comes only from `download_candidates`
  - no metadata `Link` input
  - Manual is terminal
  - no cookie/CAPTCHA/challenge automation
  - filename sanitization at every join
  - Existing requires verify
  - `Completed` only after publish
  - Superseded is orchestrator-decided
  - old API/JSON unchanged
  - the retry class list
  - gate classification before `error_for_status` (defect 6)
  - no Peer after a gated `ManualRequired` (R2), and Unsupported→Peer only when share-eligible
  - fail-closed Peer with the tunnel down, and no LAN discovery (R1)
  - no `SocketAddr` in public acquisition types (R1)
  - premium resolved once, inside `run`
  - `nxm_target` as the only nxm path
  - one-component rule on `Files` keys and at post-transfer publish
  - the R3 fix present in the base commit

  Quality review: temp cleanup on every path, joins on the race loser, no string-parsed business facts, lock-free event ordering per target.
- **Task 2 spec review.** CLI schema isolation, exit codes, Ctrl+C, probe-flag guards (explicit key only under an overridden base, HTTPS base, loopback-only CA with every redirect hop re-checked or redirects disabled, hidden, never persisted), **GUI probe arguments that force a separate instance with no `claim_or_forward` or forwarding and that require a non-default `--settings-dir`**, the peer disclosure and off-setting, the run-bound GUI channel, record/followups on the bound snapshot, sidebar text, navigation and gating, copy truthfulness. Quality review: stale messages, repaint, small-window layout.
- **Task 3.** One fresh whole-package reviewer that argued neither side reads the combined diff and all receipts before delivery.

Frozen tests may fail to compile if exhaustive `Message` or state constructors change. Prefer additive variants. Record any test-only drift by exact location; do not edit or hide it.

## Open questions (with defaults)

1. **MEGA.** Is there a vetted decrypting client, or does MEGA stay Manual? Default: Manual(Unsupported) until a crate is reviewed for maintenance and licensing, with a share-eligible Peer fallback (R2). This is a **parity regression**: C# automates single-file MEGA links (audit §6). Task 3 records it in `COMPATIBILITY.md`. Folder links stay unsupported in any case (C# does not support them either).
2. **DeadlyStream anonymous path.** Is the C# csrf-scrape download (audit §6) acceptable as an automated Origin when no challenge or login page is detected? Default: Manual(Unsupported) until Task 1c is reviewed against an authorized recorded fixture. Until then this is a **parity regression**: C# automates the common anonymous case. Task 3 records it in `COMPATIBILITY.md`.
3. **Loopback HTTPS trust for probes.** Default: an explicit `--extra-ca-file <pem>`, honored only for loopback or `localhost` hosts. It exists on the download commands and as a hidden `modsync gui` probe argument (2b). It is never read from the environment or settings, never shown in GUI controls, and documented. The alternative is no transfer probes until live authorization.
4. **CLI whole-guide download**, matching the C# CLI. Default: no. Rust uses the selected set, and a separate `--all-components` is not added.
5. **Install wait.** Should Install auto-wait for downloads, as the C# copy promises? Default: no. Block with a reason.
6. **Write-back.** Should the GUI record `Files[name]=true` automatically after a download, as C# does (`DownloadCacheService.cs:1293-1310`)? Default: yes for the GUI (bound snapshot, marks the guide dirty, the user saves). For the CLI it is opt-in `--record`, because the CLI's guide argument is typically a shared or version-controlled build file and an unrequested rewrite changes a file the user did not ask to modify.
7. **Persistent resource index.** Should Rust port the global `resource-index.json`? Default: no, until a race-free design is reviewed. C# has multiple lock/atomicity defects (audit §2, §10).
8. **Worker count.** Default: 4. This is a **choice**, not parity: it reuses the `download-batch` default (`main.rs:225`). C# runs one at a time in the GUI and 10 components at a time in the CLI (audit §3).
9. **Nexus premium gating.** Is Origin for a plain `nexusmods.com` URL limited to a validated premium key? Default: yes, validated once per run inside `run`. Free keys and no key give `Manual(FreeUserWait)`, with no Peer fallback (R2). nxm links with key+expires give Origin. Live free-key behaviour is unverified (audit §11).
10. **Nexus base URL for probes.** Should `NexusClient::with_base_url` (`updates.rs:131`) be exposed? Default: a hidden `--nexus-api-base <https-url>` on the download commands and as a `modsync gui` probe argument. It is documented as probe-only and never read from settings or the environment. When it is set, only an explicit `--nexus-mods-api-key` is used, and `with_base_url` rejects non-HTTPS. Otherwise probe 4 and the GUI half of probe 6 wait.
11. **Tunnel design (R1).** The tunnel design is not yet settled. Default: `PeerEndpoint` stays opaque, and Peer fails closed (with Origin unaffected) until the tunnel probe lands. Probe 8 waits for it.

Probes that do not depend on questions 3, 10 or 11: 1, 2, 5, 7 (CLI), 8a, and the idle, Manual, stale-run, completion-page and disclosure rows of 9. Probes 3, 4, 6, 8b, and the transfer, Stop and FOMOD rows of 9 need questions 3 and 10 settled, plus the 2b GUI probe arguments with single-instance isolation for their GUI parts. Otherwise those GUI parts are live-auth-only. Probes 8 and 8c need question 11.
