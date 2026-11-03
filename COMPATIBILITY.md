# Compatibility and proof boundaries

## Bulk selection and option identity — 2026-11-03

The egui installer bulk selection buttons (Select all, Select through tier,
and Select category) now call the same `selection::apply` actions as the CLI.
This shares nested-option dependency closure and game enforcement, while logging
automatic changes and warnings. Existing base/widescreen scope and filters remain.
This is not proof of archive availability, native GUI interaction, complete
selection parity, or a Qt frontend. Deselection, individual and group controls, common input
identity, unavailable-archive reasons and cross-route receipts remain to audit.
Production build and clippy pass with and without desktop features on Rust 1.95.0,
and formatting passes. Eighteen isolated production CLI cases cover referenced
options, authored selections, nested closure and the legacy deduplication negative
control. Rust tests remain frozen by AGENTS.md; their source regions are unchanged.

Instruction-only option deduplication now preserves referenced options (including
Choose targets and nested references), selected options, options with children,
and options declaring dependencies, restrictions or ordering. Previously an
identical empty instruction list could erase a required option GUID before
selection ran. Unreferenced, unselected leaf options without relations retain
legacy deduplication. This is an intentional correction to the C# loader rule;
it does not establish archive unselectability or complete selection parity.

## Desktop acquisition, step 2 — 2026-09-28

Step 1 was subsequently reviewed and merged in PR #5 (`d438528`), including the
fix that reports cancelled targets when Stop occurs during planning. The older
step-1 snapshot below retains its historical evidence and limitations.

The step-2 source adds preserving peer-settings controls and disclosure, a serial
FOMOD queue bound to download run/guide generation/roots/completed-base context,
and validation/install readiness guards. Saved-choice or choice-application errors
remain blocked behind explicit Retry/Dismiss actions. Settings read/save workers
keep filesystem I/O off the paint path, and Fetch waits for a successful settings
snapshot. Unknown settings and secrets survive explicit peer-setting edits.

Backward navigation retains download status. Narrow windows place Downloads in
scrollable main content, and FOMOD dialogs size to the viewport. Source/spec and
quality review are recorded in [the step-2 review](docs/parity/downloads-gui-step2-review.md).
[Native evidence](docs/parity/downloads-gui-step2/README.md) passes 46/46 assertions
across five isolated scenarios on the final Linux aarch64 binary, including actual
600x400 choices/navigation and preserving an authored low upload cap without an
implicit save. All five production gates pass; 53 frozen files/regions are unchanged.
The retained controller verdict accepts this bounded step-2 slice. Persistent seeding, provider
automation, cross-platform native acceptance and F29 remain open.

## Desktop acquisition, step 1 — 2026-09-28 (unreviewed)

Step 1 of the [downloads GUI plan](docs/parity/downloads-gui-plan.md) is commit
`6ac257f` plus an uncommitted review follow-up (restored test-only
`record_nxm_download`, proxy secrets redacted even after a context change, Stop
during planning skips the transfer, alias-resolved default-directory check,
per-frame context derived from the input stamp). It changes `src/ui.rs`, adds
`src/ui/downloads_panel.rs`, and adds hidden GUI arguments in `src/main.rs`. It has
not had SPEC or QUALITY review, and nothing in it is accepted.

- The desktop **Fetch Downloads** (Downloads panel, from DownloadsExplain onward and
  in the utility view) and the **nxm handoff** now use the shared orchestrator
  exactly as the CLI does. Fetch uses `plan` over the `Scope::All` candidates,
  `run_with_loopback_ca` with workers 4 and retries 1, a per-run `PeerAcquire`, and
  `record`. The nxm handoff uses `nxm_target`, `AcquisitionPlan::single` (Peer Off),
  run and record, re-resolved against the guide bound at dispatch. The old NexusClient
  bypass is removed.
- Every run has a monotonic identity bound to the guide revision, workspace, game
  folder and completed-base context, and that binding is checked every frame and at
  the terminal. On a match, `record` runs on a clone that replaces the guide only on
  success, and the guide file is never saved automatically. A changed context
  records nothing and says where the files are.
- Rows show real sources, phase, measured bytes and peers, the orchestrator's own
  Superseded, and manual reasons with an explicit-click link. Stop shows Stopping
  until the terminal arrives. Keys, nxm credentials, proxy endpoints and URL
  queries/fragments are redacted from the UI and the Activity log.
- The settings directory and any probe key are fixed before any store, telemetry,
  guide or protocol effect. This also fixes an ordinary `--settings-dir` launch,
  which previously read the key and telemetry from the default directory. The hidden
  probe options `--nexus-mods-api-key`, `--nexus-api-base` and `--extra-ca-file`
  have these rules:
  - they require an existing non-default settings directory that is not nested with
    the default;
  - they never claim or forward the instance socket;
  - they never read the stored key or persist the probe key.

[Native proof](docs/parity/downloads-gui/README.md) runs the real binary on
private Xvfb in an isolated network namespace with loopback fixtures. All 46
checks pass, with screenshots. It includes a comparison with the CLI on the same
fixture: same requests, same bytes, same statuses and the same record count.
Regression probes still pass: S2b 254, S2a 224, S1 339, inherited 41, H1a 138 and
fetch-shared-url 381.

Found, not changed: Origin progress reports `total: 0` in both frontends because
`src/download.rs:453` reads the length after the sniff at `:382`.

Still open:
- the FOMOD follow-up queue (listed only), Peer settings controls and disclosure,
  and install gating on unresolved FOMOD work (steps 2–3);
- a GUI guide record diff (the portal save dialog), Peer races, cancel mid-race,
  close-during-work, and stale-message runtime cases.
- Known limitation: planning is not interruptible, so Stop during Fetch preparation
  takes effect when planning returns; the run then transfers nothing.

## Selected-guide acquisition CLI — 2026-09-28

Source `efefba1`, corrected by `2889ec5`, adds `fetch-downloads` using the shared
plan/run/record/FOMOD-followup library and concrete proxy-only Peer adapter.
[Contract](docs/parity/fetch-downloads-cli-plan.md),
[spec review](docs/parity/fetch-downloads-spec-review.md),
[quality review](docs/parity/fetch-downloads-quality-review.md), and
[production evidence](docs/parity/fetch-downloads/RECEIPT.md) define this bounded slice.

- Effective persisted Peer mode participates in planning; explicit Off bypasses
  unnecessary peer settings. Sharing disabled takes precedence. API overrides
  require an explicit nonblank key and cannot inherit stored/environment keys.
- Default selection, file selection and explicit selectors use shared selection
  rules. Recording preserves authored selection flags, including all-false guides.
  An additive preserving loader leaves existing public loader defaults unchanged.
- Acquisition progress uses `{"type":"acquisition","event":...}` NDJSON followed
  by one summary envelope. Ordinary output is a pretty JSON summary. Ordinary
  dry-run is the pretty raw effective plan; dry-run with progress JSON is exactly
  one compact `{"type":"plan","plan":...}` line and performs no acquisition.
- Nexus free accounts remain manual; authorization errors are terminal failures.
  Neither starts Peer. Race startup grants are delayed until Origin eligibility
  and revoked if Origin wins before Peer starts. Isolated HTTPS Origin runtime
  is now exercised by the additive loopback slice described below.
- An nxm handoff with any one-time key is keyed. Without an API key, a missing
  key or a key with no valid `expires` fails before any Nexus request. Expiry
  itself is Nexus's answer, not the local clock's: Nexus refusing the key (HTTP
  401, 403 or 410) is a terminal failure that says to click Mod Manager Download
  again. C# sends the key and reports only the refusal. With an API key, the key
  is still forwarded and Nexus decides. A free API key with a key that has no
  `expires` used to stop at the premium gate as a manual download; it now asks
  Nexus and ends as that refusal. Source only; not built or exercised.
- Fetch (CLI and desktop, not dry-run) discovers files for an unnamed Nexus mod
  page with a Nexus origin and no `file_id`, as C# does, once the key validates
  as premium; a free, invalid or unvalidated key leaves the page as its one
  manual or failed row, with no file-list request. It keeps MAIN, UPDATE
  and MISCELLANEOUS files, then applies the component's Extract patterns and
  resolution filter the way named files are filtered. Each selected file
  becomes its own named target and desktop row, fetched through its `file_id`;
  a present valid copy is skipped. A page none of whose eligible files the
  Extract patterns need gets no target and a warning. The page stays unnamed and fails at run,
  with the reason in the plan warnings (`planWarnings` in the CLI summary),
  when the file list fails, when no file is eligible, when two eligible files
  share a name, or when an authored `ContentHashSHA256` would have to cover
  several files. Several files never use Peer. A named Nexus file listed more
  than once resolves to its one MAIN, UPDATE or MISCELLANEOUS copy, and fails
  instead of taking the first when that is still ambiguous. Recording
  writes `ContentHashSHA256` only for a resource's single archive (no production
  caller records hashes today). Per-file hashes need a registry schema decision
  and are not stored. A Nexus rate limit during discovery waits up to 60
  seconds and does not see Stop until it ends. Source only; not built or
  exercised.
- DeadlyStream is no longer a fixed manual host (its source order changed from Peer
  then manual to Origin then Peer, so Peer racing now applies to it). Fetch (CLI and desktop, not
  dry-run) reads the resource page over one anonymous cookie session, takes the
  CSRF key from a hidden input, a script value or a link, requests the
  `?do=download&csrfKey=` selection page, and names each archive from its
  Content-Disposition or link, following up to two levels of selection pages. A
  topic's `?do=download` attachments are read from the page itself. Relative and
  HTML-escaped links resolve. An unnamed DeadlyStream target becomes one named
  target per archive the component's `Files` flags and Extract patterns select,
  as for Nexus; a named target picks its archive from the page at run time. The
  original page URL stays the `registry_url`. Only exact DeadlyStream hosts are
  accepted (C# accepts any URL containing the text). Every request and redirect
  stays on the page's site: `deadlystream.com` and `www.deadlystream.com` (https,
  default port) count as one site, any other host is refused. The one host-only
  cookie jar is sent to that pair and never elsewhere, and the C# bearer header is
  not sent. Archive transfer errors drop the URL, so a `csrfKey` is not logged. A login, challenge or countdown page, or an
  unexplained page where a file was promised, is a manual boundary with no Peer;
  a missing CSRF key, missing link, unsafe or duplicate name, or a wrong name
  under `Files` is a visible failure, never a guessed link. As in C#, one link
  that fails plainly is skipped while the page's other files stay usable; only a
  page with no usable file fails. Exceeding a bound (200 requests, 2 MiB per page,
  two selection levels, 32 links) fails the whole page rather than returning a
  partial list, and is not retried. Peer is granted only
  after these gates clear; a rate limit or server error retries. The archive then
  goes through the direct downloader's HTML guard, hashes, verification and
  no-clobber publish. Each named target reads the page again, so a multi-file
  page costs one session per archive. Cross-host (CDN) archive redirects are
  still refused, including any host other than the apex/www pair. The hidden `fetch-downloads --deadlystream-base <https-loopback-url>`
  option (needs `--extra-ca-file`) sends every page, selection and archive
  request to that fixture with proxies off, reads no stored Nexus key and
  persists nothing and starts no peers; it does not make localhost a DeadlyStream host. Source only;
  not built or exercised.
- MEGA public single-file links are decrypted anonymously (their source order changed
  from Peer then manual to Origin, then Peer, then a final manual). The `mega` crate cannot use this
  project's TLS, so the anonymous protocol runs in `src/download_hosts/mega/transfer.rs`
  over the shared rustls client; the crate supplies only `compute_condensed_mac`.
  Supported link forms: `https://mega.nz/file/<handle>#<key>`, `/embed/<handle>#<key>`,
  `/embed#!<handle>!<key>` and legacy `/#!<handle>!<key>`, on `mega.nz` or `mega.co.nz`
  (also `www.` or another subdomain), http or https, a missing scheme, percent-encoded
  fragments, and trailing `/`, `?` or `&`. Rejected with their own messages: folder links
  (`/folder/`, `#F!`; folder traversal is unsupported), password-protected links (`#P!`),
  unrecognized paths, and a missing or malformed handle or 43 character key. One API
  request (`g` with `p=<handle>`, `ssl=2`) returns the size, encrypted attributes and
  storage address; the link key folds to the AES key, CTR nonce and meta MAC, the
  attributes must decrypt to `MEGA{...}` with a safe single-component `n` name, the bytes
  stream through AES-128-CTR into staging, must equal the API size (a zero-byte file is
  valid; a longer body is an error, never truncated), and only publish
  through the shared no-clobber path after the condensed MAC equals the link's MAC. A wrong
  key, corrupted attributes, altered bytes, a short transfer, a deleted node (-9), a blocked
  one (-16) and a bad API reply are separate failures; an API entry with a negative `e`
  is that error even when it also carries `g`; a busy API (-1, -3, -4, -6, -18, -19)
  and a short, stalled or reset transfer retry. Quota (-17, HTTP 509) and access denied
  (-11) are manual boundaries at the guide's authored link with no Peer and no later
  manual step. Any other failure (folder, password, bad key or handle, permanent origin
  error) falls to Peer and then ends ManualRequired at the authored link. When the guide
  supplies the file name, a bad node name does not fail the download; otherwise it does.
  A stall between bytes is bounded by the shared idle timeout, not the whole transfer. Production talks only to
  `g.api.mega.co.nz` and `*.userstorage.mega.co.nz` or `*.userstorage.mega.nz` over
  https; redirects stay on the origin that returned them. The key is not put in errors
  or `Debug`. The manual URL is the guide's `registry_url` (the authored link, which
  holds the key); it is shown in plans and reports as authored, subject to display
  redaction. Node metadata beyond name and size
  is not exposed yet (H4). The hidden `fetch-downloads --mega-api-base <https-loopback-url>`
  option (needs `--extra-ca-file`) requires the API and every returned download address
  to be loopback https, with proxies off, no stored Nexus key read, nothing persisted and
  no peers. Source only; not built or exercised.
- Real isolated Peer transfers cover known/unknown names, verification, budget/hash
  refusal and SIGINT cleanup. Unsupported-provider chains can end ManualRequired
  after a Peer failure; this is exit 3, not success. Other exits are 0 complete,
  1 failed/inspection error, and 130 cancelled.

Corrected evidence `14783d2` binds 312 assertions and passing CLI/default production
builds and strict Clippy to binary
`1ba0f02f7577114030d559ea66dd5bcca8f9c3c9c835f6119af77738da947e74`.
All 53 frozen test files/regions remain unchanged. No Rust tests were run.
Persistent sharing, shared desktop acquisition, advanced FOMOD, complete host
automation, full-build/live-game and cross-platform proof remain open. No whole
F01–F29 domain is promoted by this slice.

## Isolated HTTPS acquisition transport — 2026-09-28

Source `6c08656` adds per-invocation loopback certificate trust through additive
`LoopbackCa::from_pem_file`, `fetch_to_with_loopback_ca`, and
`run_with_loopback_ca`; existing fetch/run APIs and RunSettings remain unchanged.
Hidden `fetch-downloads --extra-ca-file` applies only to archive HTTPS requests.
It accepts exact localhost or literal loopback on the initial URL and every bounded
redirect, pins localhost to loopback, and disables proxies only for that client.
Ordinary trust/proxy/error behavior is preserved. No global or persisted CA change.
Unreadable/invalid PEM/DER fails visibly even during dry-run; parsing reads the
explicit file without network or transfer/cache writes.

[Retained proof](docs/parity/origin-loopback-proof/README.md), source-bound to
binary `e40e218304a3b1070fffa516ba60d74dd3128abb03798485316976e18732d82f`,
contains 53 production cases: reachable TEST-NET/hostname controls, separate HTTP
CONNECT control, redirect restrictions, all gate reasons in Race/Fallback with
zero Peer work, both real race winners, retries, integrity/budget refusal, explicit
API-key isolation and cancellation cleanup. Five production gates pass;94 unowned
source/test/config files are unchanged. Independent source SPEC and QUALITY reviews approved the bounded slice. These are isolated synthetic-host results, not live-provider acceptance.

Cancellation during retry backoff retains the last target failure while overall
report.cancelled is true and CLI exit is130. Body/race cancellation marks the
target cancelled. This pre-existing distinction remains explicit. GUI downloads,
persistent sharing, complete providers and F29 remain open.

## Stored instruction Source normalization — 2026-09-28

Source `62b6f70`, evidence `0f48868`, passed independent SPEC and QUALITY review.
The loader now normalizes stored non-Choose Sources with the same helper used for
equality, before deduplication. It preserves placeholder casing, recursively handles
Options, filters blank non-Choose array strings, preserves bare blank scalars for
validation, and retains malformed-value errors. Choose is exempt case-insensitively.
Destination, arguments, selection defaults, Markdown parsing and path enforcement
are unchanged. Bare-relative Copy now executes under the mod root.

[Production evidence](docs/parity/source-normalization-proof/README.md) covers four
structured formats, authored scalars, embedded YAML/TOML, dedup/Choose, idempotence,
nonmutating validation and actual Copy. Identical authored guide bytes produce
identical payload bytes in Rust/C#; C# used --skip-validation because full validation
fails its missing HoloPatcher prerequisite. Absolute-looking input uses the inside
mod root while an outside sentinel is preserved; unsafe paths remain rejected.

C# differences remain explicit: deeper options, blank Choose entries, typed malformed
values and the examined XML empty-array case do not have identical models. Uppercase
placeholders survive conversion but still fail Rust execution. This does not close
F01/F05, full validation or complete format/action parity. Five production gates
pass and53 frozen entries remain unchanged; no Rust tests were run.

## Typed install progress and Installing page — 2026-09-27

Commits `065ffee` (shared contract), `57f198f`, `cd8f0b0` and `33de7f6` (CLI adapter and help text), `dcd1373`..`ac9cf3a`, `a24e96b` and `9df6c3b` (desktop page). Rulings, commit map and review findings are in [install-progress-integration.md](docs/parity/install-progress-integration.md). Evidence is in [install-progress-native/](docs/parity/install-progress-native/README.md).

**Library API (additive).** `Engine::install_observed(guide, emit, progress)` and `Engine::install_remaining_observed(guide, completed, emit, progress)` take a second observer, `FnMut(install_progress::Progress)`. `Engine::install` and `Engine::install_remaining` still exist and pass a no-op observer. The observer cannot return an error. A dropped receiver does not change execution. The legacy `Event { component, step, message }` callback and its serialization are unchanged.

**Progress schema.** `Progress` serializes in camelCase: `phase`, `topLevelComponentName`, `topLevelComponentGuid`, `componentIndex`, `componentTotal`, `ownerName`, `ownerGuid`, `ownerPath` (array of `OwnerFrame {name, guid, reachedFromInstruction}`), `instructionIndex`, `instructionTotal`, `operation`, `message`, `snapshotPending`. `phase` is one of `preparing`, `creatingSnapshot`, `installingComponent`, `runningInstruction`, `componentCompleted`, `recordingCompletion`, `completed`. Indices are one-based when an item is active and 0 otherwise. `componentTotal` counts only the ordered top-level components still to install; nested Choose work never changes it. `instructionTotal` is owner-local. There is no `rollingBack` phase and no error phase. `Progress` carries no schema version field.

**CLI flag.** `install --install-progress-json` writes one JSON object per line to stdout:

```json
{"type":"installProgress","progress":{...Progress...}}
```

- Only `install` accepts it. `validate` and `conflicts` reject it with `--install-progress-json is only supported by install`.
- Clap rejects it together with `--json` or `--validation-report` (exit 2, usage error). One stdout stream has one schema.
- Legacy `--json` is byte-identical to the pre-slice output. The Task 1 fix1 and Task 2a receipts both compare the same 639-byte, 10-line All stream (SHA256 `9ea418cf…`) with keys `component`, `message`, `step`.
- In typed mode, legacy Event text (`component / step: message`, which includes external-program output) goes to **stderr**, from `cd8f0b0`. Stdout stays pure NDJSON.
- Stdout write errors in typed mode are ignored on purpose (`let _ = …write_all`). A closed pipe means the consumer left, and the engine keeps its own safe cancellation and recovery contract. The install still runs to its normal end and exit code. The Task 2a `drop*` probes cover this.

**Success and failure contract (review finding #5).**

- Success: exit 0, and the last stdout line has `"phase":"completed"` with `snapshotPending:false`. `completed` is emitted only after the completion receipt is recorded and `recovery::finish` succeeds.
- Failure or cancellation after the stream started: nonzero exit, no `completed` line, and the real error text on stderr (`Error: …`). The last line is the last fact that really happened; it may be `componentCompleted` for an earlier component. No failure envelope is emitted.
- Preflight failure (lock, receipt check, scan, non-mutating preflight): zero stdout lines, nonzero exit, error on stderr, and no recovery snapshot.
- `33de7f6` updated the flag's `--help` text (`src/main.rs`) to state this contract. That is a source-only change; the `ac9cf3a` probe's `help.stdout` predates it.

**Desktop.** Base and Retained Remaining installs render one Installing page from these facts (`src/ui/installing_page.rs`). The percentage moves only on `componentCompleted`, stays at 99% or less until `completed`, and uses round-half-to-even like .NET `Math.Round`. The page shows one recovery snapshot row instead of C# checkpoint counts. Issue counts read "Not counted". Stop Install asks for confirmation and then cancels at the next safe boundary. Restore and Retry act on the roots captured when the run started. The deliberate differences from C# are listed in `docs/parity/GUI-INVENTORY.md`.

**Limits.** This does not close F10, F24 or F26, and it adds no F29 full-build proof. There is still one whole-run recovery snapshot, no per-component checkpoint and no automatic rollback. Review findings #2, #9 and #10 (`src/ui.rs` restore routing, stale Remaining dead end, Retry after a `Settings::new` failure) are fixed by `a24e96b` (#2, #10) and `9df6c3b` (#9); the scoped frontend-races re-review approved them. Frozen inline tests in `src/ui.rs` will hang on `finish_worker`; see the integration doc. No Rust tests were created, changed or run.

## Shared selection actions (CLI half) — 2026-09-27

Branch `selection-actions` at `384cca5` (`92bd0ac`..`384cca5`) adds `selection::apply` with `Action` and `Outcome` (`src/selection.rs:834-877`, `:1405`). The CLI routes every selection flag through it.

**Status:**
- CLI: landed and probed. Evidence is in [docs/parity/mod-selection-probes/](docs/parity/mod-selection-probes/README.md), with receipt sha256 `a601124f…a014`. 172 copied outputs re-hash equal to the receipt.
- GUI: planned only. `src/ui.rs` is unchanged on this branch. It still runs `enforce_selectable` per draw (`ui.rs:916`) and the old `select_tier` (`ui.rs:1015`). The GUI halves of probes 6 (download candidates) and 7 (mutual restriction) stay unverified until mod-selection Task 3.

The full rules, C# `file:line` references and CLI JSON shape are in [GUI-CLI.md, Mod selection policy](docs/parity/GUI-CLI.md#mod-selection-policy). In short:

- **Tier.** Select by Tier is cumulative over normalized tiers, with a bare `1`–`4` fallback. This is a decided deviation from the C# wizard, which selects the exact tier for every full-label tier (`ModSelectionPage.axaml.cs:1231-1247`). All 920 of 920 corpus tiers take that exact branch. C#'s own prompt (`:1173`) and tooltip (`ModSelectionPage.axaml:49`) promise the cumulative rule, and so does the C# CLI (`ModBuildConverter.cs:1424-1431`).
- **Aspyr.** `EnforceGame` runs once after every selection source whenever the game is known. This is a deviation from the C# wizard and CLI. Only the C# editor gates Aspyr-only mods (`src/ModSync.GUI/MainWindow.axaml.cs:5061-5075`).
  - Since `384cca5`, removing a mod also clears its options and the mods that depend on them (probe p19).
  - An unknown game only warns.
- **Bulk actions.** Bulk and reset actions add the dependency closure only. Restriction conflicts become warnings. This is a Rust policy with no C# source.
- **Single items.** Single-item actions use the full editor cascade. This is a Rust rule: the live Q9 oracle shows the C# wizard cascades only when editor-sidebar items were realized first ([docs/parity/mod-selection-q9-oracle/](docs/parity/mod-selection-q9-oracle/README.md)).
- **Auto lists.** `autoSelected` and `autoDeselected` can name GUIDs that are in neither `selected` nor `deselected` (probe p2, `outcomes[0]`).

**CLI behaviour changes** against `c28f4fc` (probe 8, `out-legacy/compare.txt`). 9 of 12 legacy runs are byte-identical. The other three changed:

- `validate --select "Beta Addon"` (l12) and `conflicts --select "Beta Addon"` (l7):
  - Before, they failed with `Beta Addon requires selected component …0001`.
  - Now `--select` adds the dependency closure and prints `Selection: auto-selected Alpha Core (…0001)`.
  - On the executable fixture (`out-legacy-exec`), both now succeed and print their normal stdout.
- `validate --json` on K1 (l8): stdout is unchanged. stderr gains two `Selection: auto-deselected` lines, because default-all now drops the Aspyr-only mod and its dependent on a known non-Aspyr game. The C# CLI keeps them (`ModBuildConverter.cs:1379-1385`).

**What this supersedes:**
- the wizard tier rule in the Scope paragraph below ("Wizard tier selection is additive and cumulative only for `1` through `4` and `Optional`");
- the "Tier/category selection checkpoint" statement that unknown tiers select exact matches;
- the Selection row's "explicit CLI components/options" (the CLI now resolves them).

These apply to the CLI now and to the GUI once Task 3 lands. No Rust tests were created, changed or run.
## Completion pages — 2026-09-27

Commit `12c5c68` (exact Base/Widescreen/Finished completion pages). Source and page contracts are in [completion-pages-plan.md](docs/parity/completion-pages-plan.md); the source-cited composition is in [completion-pages-csharp-audit.md](docs/parity/completion-pages-csharp-audit.md). Native review evidence is in [completion-pages-native/](docs/parity/completion-pages-native/RECEIPT.txt), reproduced from a full native matrix run at `/tmp/modsync-cpages-native`.

**Open-folder mechanism (recorded difference, follow-up only).** `Open Game Folder` and `Open Mod Folder` on the completion pages (`src/ui/completion_pages.rs:564`) call `ctx.open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())))`, which goes through the `webbrowser` crate — a browser-first API. C# uses `Process.Start(UseShellExecute=true)` (`BaseInstallCompletePage.axaml.cs:158,181`), which asks the desktop shell to open the folder directly. Native review confirmed the Rust path opens real Firefox windows on the test display instead of a file manager, and the path is not percent-encoded before being placed after `file://`. The feedback text is already honest about what happened (`"Asked the system to open the {label}: {path}"`, `src/ui/completion_pages.rs:565`), so no copy change was needed. This task does not change the mechanism; switching to a native "open in file manager" call (e.g. `xdg-open`/`open`/`explorer` dispatch, matching `Process.Start(UseShellExecute=true)` behavior) is a follow-up.

**Stat-card wrap at small window sizes (Q7 resolution, recorded with native evidence).** The Base Installation Complete stat row (`src/ui/completion_pages.rs:245-258`) wraps from 4 columns to 2×2 to a single column as available width shrinks, rather than staying at an exact 4-column layout that could clip. This is the chosen resolution to the plan's Q7 (default was "exact 4-column first, switch to 2×2 only if native capture shows clipping"); native capture at 600×400 showed the shared wizard chrome (title block, Downloads panel, footer nav, Activity dock) leaves only ~25-40px of vertical viewport for the page body on this page, matching the plan's recorded C# D12 difference ("expected to clip") — a shared-layout constraint, not specific to this module. Evidence: [completion-pages-native/RECEIPT.txt](docs/parity/completion-pages-native/RECEIPT.txt) (`probe-base2`, 600×400 section) and screenshots `base-install-complete-1280x800.png`, `base-install-complete-600x400-top.png`, `base-install-complete-600x400-stats.png` in the same directory, with sha256 values in `sha256sums.txt` there.

**Managed-deployment summary stays hidden.** C# shows a managed-deployment summary on the completion pages only when `ManagedDeploymentEnabled` is true, which is false by default. Rust has no managed install session in the engine install path — `src/deployment.rs` is used only by the CLI (`src/main.rs:452`) — so the completion pages never render one. This matches C#'s default-hidden behavior. Managed installation and its completion summary remain unimplemented in the Rust engine path; F18 remains partial.

**Copy Log line limit.** `Copy Log` copies the run-scoped installing-page log, which is capped at `LOG_LIMIT = 2000` lines (`src/ui/installing_page.rs:11,229`). Very long installs will have earlier log lines silently dropped from what gets copied. Recorded as a known limit, not fixed in this task.

**Recovery copy reuse (trivial fix, this task).** The Finished page's second "Next Steps" bullet (`src/ui/completion_pages.rs:502`) previously paraphrased the truthful recovery sentence instead of quoting it. It now reuses the `RECOVERY_TEXT` constant (`src/ui/completion_pages.rs:15`) verbatim, so the Recovery modal, the Base page's Recovery card and the Finished page all state the exact same sentence.

**Task 2b CLI surface integrated.** `completion-summary --game-dir PATH --source-dir PATH [--json]` now reads the same `recovery::CompletionSummary` as the GUI. The standalone query was approved at `cd2af69` and is included in this integration; its six-scenario CLI evidence is in `docs/parity/completion-summary-probe.log`. The earlier completion-page lane inspection predated that merge.

## Final archive diagnostic correction — 2026-09-27

Whole-package review found one P2 combined-path defect: known component archive errors lacked a severity marker, so the shared presentation displayed only an aggregate card and first-issue navigation could select the final generic failure. Scoped specification/quality review approves `099834c` and closes F1. That commit fixes the shared failed-outcome mapping with an idempotent `ERROR:` prefix, preserving component identity/detail. **Raw Report message values now include that prefix; serialized schema, stages, counters and warning behavior are unchanged.** This qualifies earlier transport-compatibility statements: schema stability does not imply unchanged diagnostic text.

Separate [focused archive-fix evidence](docs/parity/validation-archive-diagnostic-fix/README.md) records missing Extract **1/1/4**, corrupt Extract **3/0/3**, unchanged missing Copy **0/1/5**, native actionable cards, automatic/manual first-issue targeting and actual clipboard readback. Strict installs reject before snapshots/pending/game/mod content writes; expected lock files and empty state directories outside content roots are recorded, not hidden. The earlier47c3e67 native package retains its original binding. No Rust tests were created/adapted/run, and no full-parity or all-target compilation claim follows.

## Validation shared pipeline and dedicated page — 2026-09-27

The shared library now exposes pure report presentation and genuine typed stage progress without changing raw Report JSON or ordinary Event contracts. Shared full-report Conflicts checks declared restrictions/dependencies; ComponentValidation checks archive/destination availability; typed ordinary missing Copy/Move/Extract sources become component-scoped dry-run skip warnings. Missing Copy now measures0 errors/1 warning/5 passed, matching pinned native C#, while strict installation, ordinary validate, standalone file conflicts, FOMOD known-state failures and remaining receipt verification retain their guards. Shared corrections are approved through88b2dd9. Registry auto-extraction, Move remapping/fallback and broader resolver parity remain incomplete.

Desktop `src/ui/validation_page.rs` consumes these shared contracts through run/input-bound workers. Dedicated log/results, copy, first-issue, reset/Stop and theme controls are implemented through47c3e67; utilities remain reachable. The bounded final native matrix and source-bound package are now [archived](docs/parity/validation-native/README.md); the subsequent whole-package review finding is closed by the approved099834c correction above. Latest600×400 Stop/cancel proof is confirmed for47c3e67; older captures remain tied to556b165 or5548736. Exact C# GUI/clipboard/typography/accessibility parity is not established.

[Integration contracts and evidence boundaries](docs/parity/validation-page-integration.md) record truthful warning footer wording, raw JSON versus pure solution fallback, proactive Next gating and monochrome-font deviations. They also preserve the correction from historically inferred full2/0/4 to actually measured pre-fix full3/0/3 (partial2/0/1). The user has deferred all new/expanded/ported Rust tests, including obsolete inline worker fixture adaptations, until full parity. Existing tests remain; production-only build evidence is not an all-target/test compilation claim.

CLI `validate`, `conflicts`, and `install` now expose `--phase all|base|remaining`
(default `all`) through the same preparation API used by desktop workers. Remaining
conflicts verify completion roots, serialized definitions and game hashes in the
shared engine. All report compatibility is preserved. CLI Remaining requires stable
definition-equivalent guide/choices inputs; whole-payload external choices metadata
means even suffix-only edits may reject, while GUI retained history can preserve
the completed prefix. Explicit Base reruns use ordinary installer semantics, so a
Copy/Overwrite Base can repeat; consumed Move inputs reject. Explicit phased
`--direct-markdown` is unsupported; ordinary Markdown ingestion remains allowed.
Commands, output/error contracts, reviews and source-bound evidence are in
[cli-phases-integration.md](docs/parity/cli-phases-integration.md). Tasks 1–3 passed
review; whole-package final review passed with nonblocking P3 notes. This adds no
native GUI or full-build proof; the existing pre-fix native evidence retains its original scope.

Remaining-phase FOMOD configuration now uses shared choices application/merge and
prompt-status helpers that preserve completed component definitions and nested
options. The desktop editor and external choices file both feed the same remaining
operation; receipt verification is unchanged. Successful base workers retain their
exact effective definitions in the resident guide, including external choices
supplied before installation. Completion is rejected after a project reset, guide
change, or directory change; historical definitions are never reconstructed from a
subsequently edited choices file. Regressions configure an initially
unselected widescreen archive after a base Move consumes its source, then validate
and install only the remaining component. The native wizard receipt below predates
this fix and is evidence for its recorded source hashes only; the new FOMOD boundary
has automated worker/shared tests, not a newly driven native FOMOD session.

The desktop now uses the active C# embedded-host route: separate landing load,
Welcome, conditional notices, directory/selection/download/validation/review pages,
worker-driven base and remaining phases, and Finish returning to the loaded guide.
Shared full validation includes selected widescreen inputs; shared remaining
validation/install preserve the completed prefix without replaying consumed Move
sources. Failed installs block progression; input changes during a validation worker
discard its stale result. Native route evidence and deliberate safety/copy differences
are recorded in `docs/parity/wizard-native/README.md` and `GUI-INVENTORY.md`.
Exact rendering, the remaining downloads UI (plan steps 2–3), rich markup, accessibility, and
the metadata-free JSON/TOML default-preamble distinction remain open. FOMOD input
stamps avoid per-frame payload reads; non-Unix timestamp-preserving edits remain
an invalidation limitation. This route slice does not establish full C# GUI parity.

GUI and CLI selection rules now live in the shared selection module, with their reference-specific tier behavior kept distinct. CLI filters reset selection, OR values within each group, AND category/tier groups, and normalize named/full tier labels through priority bands; numeric CLI tiers remain custom exact values. Wizard tier selection is additive and cumulative only for `1` through `4` and `Optional`; every other label is an exact case-insensitive match. Exact component names/GUIDs take precedence over selector syntax, including names such as `category:Foo`. Executable CLI regressions cover colon names, collision precedence, and parseable JSON stdout when malformed-selector warnings go to stderr. `docs/parity/GUI-INVENTORY.md` records the open exact-GUI requirements from source comparison; it does not establish native equivalence.

Remaining-phase desktop validation now returns the same structured full report as base validation. Each stage retains the verified completed-prefix context, and FOMOD inspection skips completed components. The consumed-base-source regression covers report success without replaying a destructive Move or mutating either tree or recovery snapshot. Native remaining-phase presentation and exact C# report semantics remain open.

GUI/CLI convergence and exact C# GUI acceptance are tracked in `docs/parity/GUI-CLI.md`. The desktop base validation worker now requests full validation plus dry run, matching the C# WizardFull preset through the same engine used by CLI. A worker regression compares complete reports and verifies no game writes; native visual and interaction equivalence remains open.

Dry-run aggregate counters now count individual errors (including Critical) or warnings, matching C# pipeline precedence: when a dry run contains errors, its warnings remain visible in the issues but do not increment the aggregate warning counter. Regression cases cover mixed severities, multiple warnings, and a clean run. This corrects counters only; diagnostic content equivalence remains incomplete.

The live validation oracle in `docs/parity/validation-pipeline-oracle/` compares both CLIs on one successful Copy fixture with read-only game/mod trees. Parsed JSON, exit status, and empty stderr match. Rust structured CLI reports now require `--full` for Environment/Conflicts/InstallOrder and support `--dry-run-only` to skip ComponentValidation. Desktop reports retain their full-stage default. Full-stage semantics, failing reports, warning counts, and the complete C# component archive validator remain partial; matching the successful fixture does not close those gaps.

Reference: the local ModSync C# source at `d31ccbd3bfcddb4705917eafa6483715c166de3b`. This is a Rust implementation of the instruction workflow, not a source-to-source conversion of the entire application.

| Area | Implemented behavior | Limits |
|---|---|---|
| Desktop | Native egui wizard, file/folder pickers, selection, options, search, validation, install, recovery, logs, report export | No visual or accessibility equivalence claim to Avalonia |
| CLI | List, convert (including stdin/stdout), validate, install, restore, download; JSON progress; explicit Linux `modsync://` desktop-handler registration | No MCP server, `nxm://` registration, or native Windows/macOS registration execution |
| Formats | Existing `thisMod` TOML; JSON arrays; YAML document streams; XML; format aliases/autodetection; fenced structured Markdown; default prose fields and YAML/TOML component comments | Prose export, legacy bullet metadata, custom regex profiles and complete parser error-recovery parity remain open |
| Metadata | Preserves additional component fields; TOML/YAML retain build metadata | Preserving a field does not implement its feature; XML typing of arbitrary metadata remains unverified |
| Selection | File selections, select-all when all components are unselected; explicit CLI components/options; desktop dependency/restriction and deselection cascades across components and options | Native interaction and full Unicode GUID-comparison parity remain open |
| Ordering | C# install order (breadth-first over Dependencies and InstallBefore/After from the resolver's order; Markdown by Dependencies only; `--direct-markdown` in source order), restrictions, instruction conditions; validate and install refuse a bad InstallBefore/After reference anywhere in a non-Markdown guide | Missing references and cycles fail; phased installs keep widescreen-only components after the base build |
| Options | Choose resolves selected option GUIDs in declared option order; an entry names an option only as its hyphenated GUID (any case), as in C# | No automatic choice inference; an entry that names no option is refused, where C# ignores it |
| File operations | Copy, Move, Rename, Delete, Extract, DelDuplicate, CleanList | Copy/Move flatten matched files into the target, as the C# instruction engine does; a Copy/Move whose target is the source itself is skipped with a warning when `Overwrite = false` and refused otherwise; Delete with `Overwrite = true` fails on a missing file, as C# does; a Rename Destination is one file name in the source's folder, as in C# (a placeholder path, a folder part or a name that is not a valid Windows file name is refused); a Delete with no Source is a no-op unless `Overwrite = true`; Extract always extracts into a folder named after the archive, next to it, whatever its Destination, as the C# CLI and GUI do (their validation clears an Extract Destination), and replaces existing files whatever `Overwrite` says, as C# does (configured FOMOD extraction with `Overwrite = false` is refused instead; C# keeps an existing directory or read-only file at an entry path and succeeds, where Rust fails on the directory and replaces the read-only file) |
| Archives | Streaming ZIP, 7z, RAR, TAR, GZip and 7z SFX; integrity checks; isolated extraction root; `[`/`]` in member names are ordinary characters, as in C# | No encrypted/multipart archives; SFX support is limited to embedded 7z payloads; empty directories are not materialized; a member named with `*` or `?` fails the archive (fail-closed) |
| CleanList | C# comma-separated rows, mandatory prefix, substring and token matching; a listed name no file can have is skipped, `[`/`]` literal | `..`, absolute, drive-qualified, trailing-dot and device names refused (fail-closed); final file sets still need full-build differential acceptance |
| Texture cleanup | Removes requested extension when another compatible extension shares its basename | This is not content-hash deduplication |
| Paths | Mod/game placeholders, legacy aliases, case-insensitive matching, wildcard sources | Rejects symlinks, device names (including `COM¹`-`COM³`, `LPT¹`-`LPT³`, `CONIN$`, `CONOUT$`, `COM0`, `LPT0`), traversal, case collisions, overlapping roots, special files, and paths into a Peer download staging folder (`<<modDirectory>>/.peer-<nonce>.part/`) |
| Validation | In-memory file index tracks changes across instruction boundaries; generated drafts require explicit review | Extracted content uses private temporary files outside game/mod trees; external program effects cannot be simulated |
| Patching | OdyPatcher subprocess, namespace index or named INI, exit-status checking, output capture | Resolves configured path, PATH, and application Resources candidates; no embedded patcher, Wine, or patch-byte equivalence claim |
| Recovery | Snapshot both roots, stop on first failure, block retry until verified restore | Full copies; restart whole guide after restore; not per-component resume or a power-loss atomic filesystem transaction |
| Downloads | HTTPS and BitTorrent magnet transfers, streamed integrity checks, SHA-256 content identity, no-clobber publish; source-ordered DeadlyStream/MEGA/Nexus/GameFront provider routing | No host scraping beyond the anonymous DeadlyStream page flow (source only; not built or exercised), account management, CAPTCHA interaction, MEGA folder or password links (single public files are decrypted; source only, not built or exercised), or automatic torrent discovery |
| Profiles | Persistent profile store and CLI CRUD/capture/apply/activation; profile-backed CLI validation/install | Desktop manager, default storage discovery and full schema/platform parity remain incomplete |
| FOMOD | Archive discovery, XML/choices parsing, configured archive extraction and selected-copy installation; CLI and desktop selection flows | Full corpus, restart/recovery, and complete native interface parity remain open |
| Managed deployment | Staged-tree manifest deploy/uninstall/purge CLI with hardlink-or-copy materialization, hash-guarded restoration, preflight and rollback, profile-scoped roots, profile switch with optional purge, and patcher provenance snapshot/record | Normal engine actions are not yet redirected into per-component staging (design in `docs/parity/deployment-csharp-audit.md`); no GUI management integration |
| Merge | `merge EXISTING INCOMING OUTPUT` matches GUIDs, then normalized name/author values; it supports existing-order, field-preference and exclusion switches | No C# URL validation, domain fallback, complete per-field/resource conflict rules, full option merge, or GUI merge dialogs |
| Protocol links | `protocol-parse` parses `nxm://` and `modsync://` links into typed JSON; `protocol-fetch` resolves hosts, rejects local/private targets and every unsafe redirect hop, caps instruction downloads at 5 MiB, and atomically publishes a new destination; `protocol-open` fetches, loads and emits a `modsync://` guide from a temporary file; `protocol-resolve` resolves an `nxm://` game/mod handoff to exactly one top-level component registry entry; `nxm-download` resolves authorized Nexus mirrors and streams an archive through the shared HTTPS/integrity path; the desktop retains an `nxm://` handoff and, when idle, runs it through the shared `nxm_target`/single-plan/run/`record` path bound to the guide at dispatch (desktop step 1, unreviewed), while a confirmed `modsync://` link loads through that same worker/status flow; Unix secondary launches forward either one bounded URL or an activation request to the primary UI through a per-user socket; `protocol-register-linux` writes and registers its desktop handler | No cancellation during a ModSync instruction fetch, Windows named-pipe handoff, Windows/macOS registration execution, or native desktop acceptance |
| Updates | `update-apply` records supplied versions; `update-check` reads the existing settings/API-key override sources, fetches Nexus versions with API headers, one 429 retry and rate-budget tracking, then atomically saves guide and result summary; the desktop can save/clear a masked API key, runs the shared check asynchronously and shows per-component update indicators; the shared client also parses file lists, MD5 matches and key validation; every Nexus API client (default and override base) follows only same-origin redirects, at most 10 hops, so the `apikey` header never leaves its base origin, nxm download keys are URL-query encoded and never appear in client error text, and an override base without a trailing slash keeps its last path segment ([loopback proof](docs/parity/nexus-h1a-proof/README.md); https-to-http downgrade and the default base are covered by source only) | No desktop file/MD5/key-validation surface, no cancellation during a request, and no live-Nexus acceptance run |
| Widescreen, Aspyr, game-specific notices | Automatic KOTOR 1/KOTOR 2 Legacy/KOTOR 2 Aspyr detection, Aspyr `steamassets` content-root resolution, Aspyr-only selection gating, and phased widescreen desktop flow | No native-platform or full-build acceptance of the game-specific flows |
| Editor | Native guide editor for component metadata, dependencies/restrictions, component and instruction add/remove/reorder, executable instruction fields, and atomic multi-format save | No option editor, raw text replacement, URL/regex import, file browser, or full C# editor corpus acceptance |
| Telemetry | Default-off consent gate, nonserialized signing-secret HMAC helper, and authenticated structured-event transport with `telemetry-auth` request headers | No OTLP protobuf traces/metrics, diagnostics UI, persisted consent, or automatic UI/install event wiring |

Unknown instruction fields and actions fail rather than being ignored. Components with no executable instructions cannot be installed. External program permission is off by default.

Archive contents and copies are streamed. Set `--scratch-dir` to an existing folder outside both install trees and optionally set `--max-expanded-bytes` per archive; the desktop exposes the same settings. Extraction also checks available scratch space. A 513 MiB regression passed locally. RAR entry attributes are read as Unix modes only when the header host OS is Unix, so Windows attribute bits never trigger the link/special-file refusal; RAR cancellation is checked between entries; other formats poll during streamed output. FOMOD archives are rejected before their content is published to the install tree because choice handling is still missing.

The full feature inventory and acceptance requirements are tracked in [docs/parity/ACCEPTANCE.md](docs/parity/ACCEPTANCE.md). Full C# parity remains incomplete.

## Validation evidence

The local compatibility probe loaded all eight existing TOML build files without parse errors:

| Guide | Components |
|---|---:|
| KOTOR1_Full | 189 |
| KOTOR1_Spoiler_Free | 109 |
| KOTOR1_Mobile_Full | 114 |
| KOTOR1_Mobile_Spoiler_Free | 63 |
| KOTOR2_Full | 145 |
| KOTOR2_Spoiler_Free | 99 |
| KOTOR2_Mobile_Full | 105 |
| KOTOR2_Mobile_Spoiler_Free | 99 |

Those counts establish parsing only. No full K1/K2 build, comparison against a manual reference install, game launch, or gameplay validation has been performed for this port.

Local Linux checks also exercised the native GUI in an isolated X11 session: load the sample guide, choose mods, validate, confirm installation, and verify the resulting TGA and removed TPC. Separate native OdyPatcher invocations through the Rust CLI installed `hello.txt` into isolated game folders using both a namespace index and a named INI, with zero patcher errors. The named-INI run preserved the original `changes.ini`. The test suite separately checks failure recovery using a controlled subprocess.

A direct HTTPS download of the RAR test fixture matched its expected SHA-256 and bytes. A second request with an incorrect hash failed without publishing the destination file.

### Streaming milestone

Local checks: 21 regular integration tests pass; the explicit 513 MiB GZip extraction and atomic-copy regression passes separately (18.23 seconds test time; 110,384 KiB maximum resident set for the measured cargo command). The temporary filesystem was tmpfs, so this measures process memory, not physical disk throughput or total system memory. New checks cover TAR/GZip CRC failures and links, expansion budgets, cleanup on cancellation, nested archive validation without install-tree writes, 7z SFX signatures crossing buffer boundaries, and C# CleanList matching. These fixtures do not establish full archive-corpus or full-build equivalence.

### Structured-format evidence

The five format tests exercise XML legacy roots/entities/collections, YAML documents and metadata, aliases/autodetection, CLI stdin/named paths, and saved C# exports. `tools/check_formats.py` also verified live C#↔Rust conversion for TOML/JSON/YAML/XML, including options and ordering constraints; see `docs/parity/format-check.json` for its exact scope. K1/K2 full guide component models survived JSON/YAML/XML round trips. These checks do not prove natural-language ingestion, complete resource migration, unknown metadata typing, or complete C# CLI parity. JSON/XML follow the C# formats' omission of root build metadata.

### Markdown import checkpoint

The shared loader imports `###` component sections, plain/bold fields and Docusaurus admonitions, linked names, categories/tiers, directions and warnings, plus explicit YAML/TOML `<!--<<ModSync>> ... -->` instructions/options. `## Mod List` limits component selection to that section, following the pinned reference. Exact master names resolve to GUIDs; unresolved master text is retained as `UnresolvedMasters` and blocks installation. Unannotated components use stable UUIDv8 identifiers derived from their section and position; C# generates random identifiers.

Two new regressions compare a captured C# Markdown conversion, execute its explicit instructions in temporary trees, and reject both instruction-free prose and unresolved dependencies before game/mod mutation. This checkpoint initially imported prose without drafting; the draft-service checkpoint below supersedes that limitation. Author-qualified master aliases, spoiler variants, special widescreen/Aspyr extraction, paste UI and full corpus field equivalence remain open.

The Rust parser loads the desktop K1 full/spoiler-free prose guides (186/180 components) and K2 full/spoiler-free guides (145/139). A live comparison with the pinned C# K1 full parser found the same 186 components; 15 of the 16 compared prose fields matched across all components. Nine `Screenshots` values differ because the reference pattern also matches that word within ordinary prose. This difference remains unresolved, not accepted as parity. The first C# attempt timed out at 120 seconds; the successful bounded retry took 198.76 seconds. See `docs/parity/markdown-check.json` for input/source hashes and mismatches. No corpus install or natural-language instruction generation was performed at that checkpoint.

### Draft-service checkpoint

Preparatory method-context slice (2026-10-15): `draft::generate` now retains
instruction-free components with a nonblank `InstallationMethod` even when they
have no explicit directions. Their `DraftReview.MethodContext` records a bounded
method label and classification, unresolved archive binding, explicit-direction
precedence, and guide-default hints. Only the exact canonical preamble clauses
(ignoring emphasis, ASCII case and whitespace) identify the patcher game-root
default, loose-file Override default, and prohibition on Override/Modules
subfolders. Preambles over 64 KiB are not interpreted; absent or unrecognized
clauses supply no destination. Labels over 256 characters are unsupported and their
stored preview is limited to 256 characters.

Every method-context draft carries an unresolved archive/review diagnostic and
`RequiresReview=true`, including when explicit prose also produces draft actions.
Unsupported/mixed methods and multi-run patchers add explicit diagnostics.
Original directions and method metadata are preserved; the existing explicit
prose parser still runs. Existing authored instructions and prior review records
remain untouched. Method labels and guide defaults generate no install actions,
perform no archive observation, and cannot be accepted through `mark_reviewed`
while their diagnostics remain unresolved. Archive-bound generation and merging
are follow-up work; this slice does not establish installation-method support or
full-build parity. No Rust tests or harnesses were created or run for this slice.

Archive-bound method-only loose files (2026-10-15): resolved-filename generation
now uses the current `InstallationMethod` and current preamble default, rather
than saved method hints. Only `Loose-File`/`Loose-File Mod` with empty directions,
instructions and options, no additional installation/download guidance, and one
resolved local ZIP/7z/RAR archive qualify. Every file must be a recognized
Override resource in one flat payload or one shared wrapper directory, apart
from narrowly recognized documentation. Documentation basenames must be exactly
`readme`, `read me`, `read_me`, `read-me`, `license`, or `licence`, with `.txt`,
`.md`, `.rtf`, or `.pdf` extensions (ASCII case-insensitive), located at the
archive root or shared payload wrapper. Every excluded document is recorded in
`ArchiveMethodBinding.ExcludedDocumentation` and covered by the archive hash and
review identity. Other documents, screenshots, unknown files,
destination-sensitive resources, additional directories and variants block the
entire binding; nothing is silently omitted.
Other methods and partial prose retain their components and unresolved reports.

Observation uses a private archive snapshot (at most 256 MiB compressed, 1 GiB
expanded or the caller's smaller limit, 4096 entries). Successful drafts contain
exactly `Extract` followed by `Move` with explicit observed member sources and the
proven Override destination. `ArchiveMethodBinding` stores filename, SHA-256,
byte count, members, actual method and current defaults outside `DraftReview`,
so explicit review identity includes the binding. Prior acceptance is invalidated;
`RequiresReview` remains true. Only the exact generic method-unresolved gap is
removed after successful observation; all other diagnostics remain. This records
the observed archive identity; it does not add an engine-time archive hash check.
Repeated generation leaves existing actions unchanged. Patcher, mixed-method,
partial-prose and general archive integration remain unsupported by this slice.
No Rust tests or harnesses were created or run.

The production CLI now accepts an offline, explicit binding file with
`convert --resolved-archives <JSON> --source-path <archive-directory>`. The JSON
is a bounded array using the `ResolvedFiles` shape, for example:

```json
[{"componentGuid":"<exact guide component GUID>","filenames":["example.zip"]}]
```

Each GUID must exactly identify one component in the intact input guide; component
bindings must be unique and archive names must be safe literal ZIP/7z/RAR
basenames. The file is limited to 8 MiB, 4096 component bindings, 4096 filenames
and 1 MiB of filename text. Explicit bindings bypass provider filename discovery
without filtering guide components. Every unbound component with an
`InstallationMethod` and no authored instructions retains a method review gap.
This option conflicts with remote `--auto`, local auto-generation and download;
it does not merge an older TOML or approve review. Real archive binding still
requires a separate production invocation and remains subject to the same
archive-member and `RequiresReview` rules above. The editor's single-archive path
still lacks the full guide preamble context.

`src/draft.rs` ports the reference's 56 ordered instruction patterns, sentence/conditional splitting, source/destination inference, exclusions, cleanlist handling, download recommendations and sandbox normalization. Existing authored instructions remain untouched. Desktop file-open and CLI install/validate automatically draft Markdown, while `convert --parse-directions` opts in for conversion. Draft diagnostics and the reference review warning persist in the component; the desktop displays steps and notes, and `convert --review-draft <GUID>` or the desktop review button records explicit acceptance. Gaps, conditional clauses or rejected drafts prevent acceptance. The engine still checks real paths and external-program permissions after review.

Rust-only extension: `convert --review-verified-drafts --source-path <dir>` clears review for a component (and its option subtree) only when its `DraftReview` came from `resolved-filenames` inference of exactly one hash-bound local archive, the diagnostics are gap-free, the archive still hashes to the recorded `Derivation`, and a fresh replay of archive inference equals the stored instructions and options (GUIDs ignored). Prose-drafted, bare-filename, multi-archive, `Execute`/`DelDuplicate` and edited drafts stay fenced, as does every method-bound draft except the narrow case below. Success writes `DraftReview.Verification` (`modsync-ci-tool`, `ArchiveDerived`) and folds it into the reviewed identity, so any later edit re-fences. The result is reported as `{cleared, skipped}` on stderr; this proves archive-derivation reproducibility, not install behavior.

Method-bound exception (2026-10-18): a draft whose `DraftReview.MethodContext.ArchiveBinding` is `Bound` clears only if it is exactly what `bind_loose_method` produces: `Generator` `resolved-filenames`, `ArchiveMethodBinding` and `Derivation` present and agreeing, no gaps or conditions, no review-history or resolution records, and a component that is still method-only `Loose-File` (no directions, extra installation/download guidance, or choices). The verifier re-hashes the single local archive against `Derivation`, replays `bind_loose_method` on it, and requires the replayed `ArchiveMethodBinding`, `MethodContext`, `Derivation`, instructions and options to equal the stored draft (GUIDs ignored). Any edit, hash drift, missing archive, prose draft, patcher or other method, `Execute`/`DelDuplicate`, or multi-archive group stays fenced. The Override default is read from the binding receipt's recorded `GuideDefaults`, because the verifier sees no guide preamble; it unlocks only the fixed `<<kotorDirectory>>\Override` destination, and the receipt is covered by the draft identity, so editing it re-fences. This proves the binding is reproducible from the archive and the recorded default, not that the guide's preamble still says so or that the install behaves correctly.
Patcher method binding (2026-10-18): `bind_patcher_method` binds a method-only `TSLPatcher`/`HoloPatcher` (or `... Mod`) component to exactly one hash-bound ZIP/7z/RAR as `Extract` plus `Patcher <<modDirectory>>\<archive>\<exe> -> <<gameDirectory>>`, the same shape `generate_from_archive` emits. Gates, all required: the guide preamble establishes the patcher game-folder default; no directions, extra installation/download guidance, instructions or choices; one archive in one resolved group; exactly one `tslpatchdata/changes.ini` at the archive root, no `namespaces.ini`, no payload outside `tslpatchdata` (root-level readme/license files are recorded as excluded documentation), and exactly one root-level `.exe`. Anything else, including multi-run patchers and wrapper directories, stays fenced with the existing unresolved-method diagnostics. `verify_archive_derived` re-derives the binding through `replay_method_binding` (`MethodContext.Kind` `Patcher`) with the same strictness as the loose-file path. This proves reproducibility from the archive and the recorded default, not that the patcher run or its `changes.ini` outcome is correct.

Guide preamble from structured input: `draft::guide_defaults` now also reads `metadata.preambleContent` / `metadata.PreambleContent`, where TOML (`[metadata]`) and YAML (`# Metadata` document) carry it, so `--resolved-archives` can bind loose files from a TOML guide. JSON component arrays and XML carry no guide metadata, so a JSON/XML input still has no Override default and still cannot bind. A `GuideDefaults` receipt with an absent default holds a null, which the TOML writer cannot encode; keep both default sentences in the preamble when drafting method components to TOML.

`tools/check_drafts.py` compares the pinned C# service's raw objects rather than relying on lossy C# JSON export. It assigns the input's prose fields directly because the legacy JSON loader drops some installation warnings. Across 19 focused cases and four desktop guide component sets (669 component records), instructions, option names/descriptions/selections, warnings, gaps and conditional notes matched with zero differences. The corpus inputs are parsed component JSON, so this proves the draft stage, not complete C# Markdown ingestion or installation. Source/input hashes and scope are in `docs/parity/draft-check.json`. Since 2026-10-04 a drafted Rename deliberately differs (see "Drafted Renames" below).

Some reference drafts are demonstrably incomplete interpretations: generic patterns can yield one-letter folder names, and numeric ranges can yield root placeholders. Matching them does not certify a correct install. Complete local archive-based generation, full direct-Markdown preflight rules (including 4GB/widescreen exclusions), editing unresolved drafts and full-build acceptance remain open. The Rust review gate enforces the draft service's never-auto-trust contract; C# direct-Markdown preflight currently counts flagged drafts as executable without that gate. This workflow difference must be reconciled before claiming full parity.

All 31 regular local integration tests pass with and without desktop features, alongside formatting and Clippy. Native X11 acceptance exercised automatic drafting, rejection before review, visible instructions/diagnostics, acceptance, validation, and an isolated install whose resulting bytes and source removal were verified. The preceding Markdown commit's Windows CI failure was an LF-only malformed-input test transform against a CRLF checkout; the test now normalizes line endings before mutation.

### Local archive-generation checkpoint

`convert --auto-generate-local --source-path <directory>` now uses a shared Rust service to infer instructions for fresh components with one resource link. It matches top-level ZIP/RAR/7z/EXE names using the reference's normalized-name scores and newest-file preference. Valid archives produce Extract, loose-file Move/Choose, changes.ini Patcher, namespaces.ini options, hybrid installation-method labels, and the special Remove Duplicate TGA/TPC instruction. Namespaces retain insertion order, including skipped indices. Generation stages content in scratch and leaves input archives untouched; failed generation leaves the component unchanged.

`tools/check_archive_generation.py` builds 34 fixture cases (ZIP archives, a few `.exe` payloads, and authored options) and captures the pinned C# generator twice. The CLI capture runs on Sources as the C# JSON loader stores them and is compared with Rust `convert --auto-generate-local` on its 28 non-direct-API cases: instruction fields, ordered option details, success and installation-method labels, with Choose identifiers normalized to names. The integration-test capture assigns the original authored Source strings after C# loading, for all 34 cases; it must equal `tests/fixtures/csharp/archive-generation.json` byte for byte, which `tests/archive_generation.rs` compares with components built without the Rust loader. Both captures pass each archive path directly, so neither depends on the C# loader's resource URL canonicalization. Neither is full JSON-ingestion proof. See `docs/parity/archive-generation-check.json`. Separate Rust tests exercise exact/newest archive selection, an actual selected-folder install, validation without source/game writes, authored-data preservation, corruption, unsafe entry names, cancellation and expansion budgets.

Re-run 2026-10-04 on `f5a2d00`: 27 of 28 CLI cases match. The remaining difference is the annotated `.exe` case (`archive-installer`), the same one the record listed before. The integration-test capture equals the committed fixture. The tool needs `bwrap`, a .NET runtime that can load a `net9.0` probe, and a 7-Zip CLI on `PATH` that can list ZIP: the first of `7z`, `7za` or `7zr` found, as the reference searches, and the tool checks it by listing a ZIP (`7zr` cannot). It passes `-f json` to `convert`, sets `DOTNET_ROLL_FORWARD=Major` unless it is already set, and mounts the reference DLL directory read-only into its sandbox. A DLL under `/tmp` therefore works, but not one directly in `/tmp`, since binding `/tmp` would expose the host's. The receipt records the 7-Zip path and version, the .NET runtime, the roll-forward setting and the DLL's informational version. Its DLL hash comes from a local `d31ccbd3` build (`2.0.0-alpha1+d31ccbd3…`). Sibling receipts record another build (`8f3b1e3f…`), because the bytes vary with build path and SDK. Before these changes, a run gave 22 matching and 6 differences:

- `archive-installer` and `irrelevant-installer-archive`: C# side, environment. SharpCompress opens a ZIP renamed to `.exe` but cannot list it, and the reference falls back to the 7-Zip CLI (`AutoInstructionGenerator.cs:2235-2269`). Without that CLI it throws and emits `Execute` / `Executable Installer`. The Rust output for both cases is unchanged since before `62b6f70`.
- `reuse`, `overlap`, `wildcard-option`, `overwrite-option`: intended, introduced by `62b6f70` (bisected from `1cb3f4b`). Rust `convert` now stores authored non-Choose Sources the way the C# loader does (`PathHelper.FixPathFormatting` and the placeholder prefix), so `\` becomes `/` before generation (C# on Linux gives the same; Rust uses `/` on every platform). The probe overwrote C#'s loaded Sources with the raw authored strings for both purposes. The CLI capture now keeps the loaded Sources. The raw assignment remains only in the integration-test capture.

This remains partial. Authored-option merging, insertion-first resource selection and EXE fallback are covered by the following checkpoints. URL/download-based generation, arbitrary existing-instruction deduplication, namespace encoding variants and desktop archive-generation controls remain open. Unlike C#, inference does not delete corrupt downloads. The matched reference output can omit a wrapper's extraction-directory prefix or produce an Extract-only patch-data component; matching those objects does not establish a correct patcher or full-build install. Only the isolated loose-file choice has end-to-end install proof in this checkpoint.

All 35 regular integration tests pass with and without desktop features, alongside formatting and Clippy. The 513 MiB regression was not rerun locally for this inference-only change; its latest completed native evidence is the preceding commit's three-platform CI run 35054774351.

### Authored archive options

Archive generation now reuses equivalent authored options and consolidates options with overlapping instructions. Matching follows the reference's action-specific destination/argument/overwrite checks, source wildcard comparisons and asymmetric source coverage. Consolidation preserves the first option's metadata and selection, appends missing instructions, and replaces removed option IDs in Choose instructions. Source filename fallback follows host path-separator rules, as the C# helper does; this is not a platform-independent path-matching contract.

The expanded live comparison matches all 20 fixtures, including authored identity reuse, partial-overlap consolidation, broad wildcard reuse and distinct overwrite policies. The probe assigns authored Source strings directly after C# deserialization to isolate merging from its JSON loader's separator rewriting. A new isolated install verifies that the original selected option and its GUID survive generation and install the expected texture. C# can emit repeated references to one broad option in a newly created Choose; Rust preserves that output, but this does not certify that such a guide installs correctly. All matching and generation edge cases, existing top-level instruction augmentation and full-build behavior remain unproven.

All 36 regular integration tests pass with and without desktop features, alongside formatting and Clippy. The preceding archive-generation commit passed native CI run 35055903581; that native evidence predates authored-option merging.

### Resource order and local matching

ResourceRegistry now retains insertion order instead of sorting URL keys. TOML parsing/serialization retains map order as well, and Markdown links are collected in their written order. JSON, TOML, YAML and XML round-trip tests check URL order explicitly for both components and nested options; map equality alone would miss this behavior.

Local generation now accepts multiple resource links and selects the first, following C#. The live reference comparison covers 22 fixtures, including the first-link preference and the absence of fallback when only a later link matches a downloaded archive. Those two cases invoke C#'s actual local-processing entry point with an isolated SourcePath. Full resource migration, host-specific URL normalization, cache behavior and all matching edge cases remain open. Native CI run 35056506242 passed for preceding commit `5791c7c9a43d52c0d4ea66bd081cf498c2670e78`; it predates this resource-order change.

Live C#↔Rust format checks also preserve top-level resource order in both directions across TOML/JSON/YAML/XML. The stronger check exposed C# YAML preprocessing flattening ResourceRegistry into an unreadable list; Rust now emits the same derived ModLinkFilenames compatibility map as the reference exporter. It preserves file-selection flags and retains unrelated legacy URLs. Loading discards redundant legacy entries for URLs already in the richer registry, following C# precedence, so Rust round trips do not accumulate duplicate metadata.

`resource-order-check.json` records both input and C# control URL lists: C# drops registries on options before the cross-read comparison, so nested-option resource preservation is proven only within Rust. Metadata values, legacy-resource migration and complete C# option serialization remain unproven; ordered keys are not proof of those behaviors.

All 38 regular integration tests pass with and without desktop features, alongside formatting and Clippy. New regressions explicitly compare resource order and check the YAML compatibility map's nullable selections and round-trip behavior.

### Executable inference checkpoint

An EXE that cannot be read as an archive now generates Execute with the reference's Executable Installer label. Cancellation remains an error, and the Remove Duplicate TGA/TPC special case still takes precedence. Readable EXE archives use normal archive analysis; a readable archive with no supported content does not become Execute. Inference never launches or deletes the input. The shared engine retains its external-program trust requirement, and validation does not execute the program even when trust is configured.

The expanded differential corpus has 28 cases: 27 match C# and one remains explicitly unresolved. For a ZIP renamed to `.exe`, C# forces its 7z reader, falls back to CLI listing, then emits only Extract because its subsequent root-file check returns false. Rust's signature-based reader emits Extract plus Move. `archive-generation-check.json` retains both complete outputs. The fixture annotation permits only that exact additional instruction; any other difference still fails the checker. This is recorded evidence of a parity gap, not an accepted equivalence. Real Windows installers, SFX variants and full installs remain unproven.

The preceding resource-order commit passed native CI run 35057519588; that evidence predates executable inference.

All 39 regular integration tests pass with and without desktop features, alongside formatting and Clippy. The added engine test proves rejection without process trust, validation without execution even when trust is configured, unchanged input bytes and cancellation without a generated step. These checks do not prove real installer execution or resolve the renamed-ZIP difference.

## Direct archive API augmentation checkpoint

The public `generate_from_archive` API now augments authored instructions, reusing equivalent steps and honoring existing Copy/Move folder coverage. Parent-folder coverage respects path boundaries; similarly named sibling folders remain separate. Existing Choose references and option identities are retained. The automatic local CLI path still skips components with top-level instructions, matching C#.

The corpus now contains 34 cases: 27 live CLI comparisons match, six direct-API C# outputs are checked by Rust integration tests, and the renamed-ZIP EXE difference above remains unresolved. An isolated installation test verifies repeated augmentation is idempotent and preserves an authored Copy destination in Modules, including source retention. This does not prove all merging paths or full-build parity. Native CI run 35058007498 passed for preceding commit `967f6c81cbc858cb30c3985160b7a3f2a4657758`.

Local validation for this checkpoint passed formatting, clippy with warnings denied, all-target tests and no-default-feature tests. Both test configurations passed 40 regular integration tests; the explicit 513 MiB regression was not rerun locally for this instruction-merging change.

## Legacy resource migration checkpoint

Structured guide loading migrates legacy ModLink and ModLinkFilenames sources into the ordered ResourceRegistry. Modern entries take precedence, followed by ModLink, then filename maps; URL equality is case-insensitive. Empty filename maps retain download links. Migrated resources carry normalized-URL SHA-1 ContentKey/MetadataHash values, nullable file selections, empty handler metadata and FileSize zero. Original URLs remain the download keys, including their fragments. SHA-1 here reproduces reference identity metadata; it is not a download integrity guarantee.

The live JSON-loader comparison covers 15 cases (12 URL normalization variants plus precedence, string-form links and lowercase legacy-field handling). Fourteen outputs match exactly. The remaining case matches all legacy migration fields but records a pre-existing modern Files-map difference: pinned C# drops that map, while Rust retains it. This exact gap is retained in `docs/parity/legacy-resources-check.json` and regression assertions. JSON legacy filenames follow the reference's first-character capitalization; TOML/YAML/XML retain their input filename casing. String-form ModLink follows the reference's comma/semicolon conversion rather than treating embedded newlines as separate links.

Rust round-trips migrated data through JSON/TOML/YAML/XML. An isolated flow verifies an empty legacy filename table reaches local archive inference, non-mutating validation and installation through the shared engine. Full URI behavior, modern resource deserialization, host download/cache integration and option-loader parity remain open. Native CI run 35058728745 passed for preceding commit `78774c295fc00be4be5a4ddc6b62fbf9fdba042b`.

This checkpoint passed formatting, clippy with warnings denied, and 42 regular integration tests in both all-target and no-default-feature configurations. The explicit 513 MiB test was not rerun locally for this loader change.

## Duplicate legacy URL checkpoint

Case-insensitive duplicate ModLinkFilenames URLs now use the last filename map while retaining the first URL spelling and insertion position, matching C# dictionary materialization. Existing registry entries and ModLink entries still win over that consolidated map. Three additional live-loader fixtures cover duplicate URL maps, duplicate ModLink precedence and comma/semicolon string splitting. The corpus totals 18 cases; the previously documented modern Files-map difference remains unresolved.

The 18-case comparison has 17 exact matches and the one previously recorded modern Files-map difference. Formatting, clippy and 42 regular integration tests passed in both configurations. Preceding commit `234e7cd3d3c07c0049ad0ce7bf415f985e015283` passed native CI run 35059298226.

## PrimaryUrl resource migration checkpoint

Old content-hash registry keys now migrate to their nonempty PrimaryUrl before legacy links are merged. Resource metadata is retained; the obsolete PrimaryUrl field is removed. Case-insensitive collisions retain the first URL spelling/order and the last metadata entry. Empty PrimaryUrl uses the original registry key. This also consolidates duplicate modern URL keys. Three live C# loader fixtures cover hash-key migration, collisions and the empty fallback; the corpus now has 21 cases. The existing modern Files-map difference remains open. The isolated archive-inference/install flow now also starts from an old hash-key registry.

This checkpoint passed formatting, clippy with warnings denied, and 42 regular integration tests in both configurations. The reference corpus has 20 exact matches and the existing modern Files-map difference. Preceding commit `914a14a01e87b8071aaf609677ea12f1c5990a10` passed native CI run 35059525742.

## Modern resource metadata investigation

The four-format inspection in `docs/parity/modern-resource-formats.md` broadens the known reference-loader difference: pinned C# JSON/TOML/XML drop both Files and HandlerMetadata; YAML's legacy fallback preserves file flags but regenerates identity hashes and loses size/timestamps. Rust preserves the fixture in JSON/TOML/YAML, but XML changes a nested numeric handler value into text. A regression records that exact XML gap. This is evidence of incomplete metadata fidelity, not a parity pass; earlier field-projected format checks do not certify these fields. No installer behavior changed in this investigation.

## Download filename planning checkpoint

The shared `download_plan` module and read-only `download-plan` CLI implement the reference's URL-enabled decision and filename filtering. Explicit false excludes a known file; true/null allow it. Unknown candidates and empty registries use unsatisfied Extract patterns from top-level and immediate option instructions, including unselected options. Known registry filenames and files found recursively on disk count as available, regardless of registry selection flags. Explicit filtering retains candidate duplicates; auto-discovery deduplicates case-insensitively when Extract instructions exist. Wildcard matching shares the reference helper already used by archive-option inference.

Twelve direct C# consumer cases match the Rust CLI (including the missing-source error); the C# probe restores file maps and Source strings after loading to isolate consumer behavior from known loader losses. An integration test verifies the command leaves the guide and source files unchanged. This does not implement host acquisition, cache persistence, queue integration, desktop controls or full install-readiness validation. Planning rejects source-tree symlinks consistently with Rust's install-tree policy; this safety boundary is not claimed as C# filesystem parity.

This checkpoint passed formatting, clippy with warnings denied, and 45 regular integration tests in both configurations. Preceding commit `a8d00928c41b793c12108e847a513e7a6199ce34` passed native CI run 35060165735.

## Profile persistence and CLI checkpoint

The shared profile store implements C#'s camel-case disk schema, per-profile JSON files, name sanitization using host filename rules, sorted listing with corrupt-file skipping, load/save, create, deep clone, rename, delete, capture and selection application. Capture retains an existing creation timestamp; apply leaves absent components untouched and replaces immediate option selections for present components. `profile --action ... --settings-dir ...` exposes these operations plus activation through `activeProfileName`. List/show JSON use the reference's PascalCase CLI shape. Other commands also emit JSON, unlike the reference's text messages; complete CLI output parity is not claimed.

Seven CLI operations per implementation, including duplicate-create and missing-delete failures, matched in isolated storage. Both implementations read each other's written profile files; activation preserved unrelated nested settings. `profile-check.json` records this live comparison. Local tests cover clone independence, rename conflicts, capture/apply, concurrent create serialization and corrupt settings protection. Rust refuses to replace malformed settings on activation; the reference resets malformed settings to defaults. Writes use atomic replacement and cooperating Rust writers share a lock; cross-process C# writers do not participate in that lock. Multi-file crash atomicity is unproven.

F16 is now partial, with desktop profile controls, platform-default storage, automatic active-profile install integration, directory activation through the engine, malformed GUID/date/null-schema parity and all Unicode/platform name rules still open. CLI apply exports selection flags but does not change the loader's existing all-unselected fallback on reload. No profile-controlled full installation is claimed by this checkpoint.

This checkpoint passed formatting, clippy with warnings denied, and 50 regular integration tests in both configurations. The explicit large-archive test was not rerun locally for profile persistence. Preceding commit `86675ce9650244891a4f8098f9671ac564a6423c` passed native CI run 35060722127.

## Profile-backed installation checkpoint

Validate/install now accept `--profile NAME` or `--active-profile` with an explicit settings directory. Stored paths fill missing guide/game/mod arguments; supplied arguments take precedence. Profile component and immediate option selections apply after guide loading, before explicit selection overrides. An all-unselected profile fails preflight without an installation snapshot or game-file mutation. This bypasses the exported-guide all-unselected fallback for direct profile-backed runs. These commands do not change profile/settings files or last-used timestamps; explicit profile activation remains a separate action.

Isolated CLI tests verify validation without game writes, actual selected option bytes, disabled components with missing inputs, explicit game-directory/option/component overrides, unchanged profile/settings/input files, missing defaults, and the all-unselected failure boundary. This is Rust integration evidence, not a full C# installer differential or desktop activation proof. Default settings discovery, automatic desktop profile application and remaining schema/platform cases keep F16 partial.

This checkpoint passed formatting, clippy with warnings denied, the desktop build, and 52 regular integration tests in both default and no-default-features configurations. The explicit 513 MiB archive regression was not rerun locally for this integration. Preceding commit `39a9775cb637c2aed72247ae3f70ea4e184d166c` passed native CI run 35061995790.

## Desktop profile activation checkpoint

The desktop's Install profiles panel can capture current selections/directories or activate a named profile from an explicit settings directory. Activation applies selections to the already-loaded guide, matching the reference ProfileManagerDialog flow; it does not automatically load the profile's stored guide. The shared store persists activeProfileName and last-used time. Successful activation clears validation and any pending install confirmation. Failed settings persistence leaves displayed selections and paths unchanged. Capture records the actually loaded guide path even if the guide text field has since been edited.

A desktop-state test covers capture, all-unselected activation, directory restoration, validation/confirmation reset, settings activation and malformed-settings failure. Formatting, clippy, 52 regular integration tests in both configurations and the additional desktop-state test passed. This is state-level evidence, not live native UI interaction proof. Profile listing, clone/rename/delete controls, default settings discovery and automatic startup activation remain open; F16 stays partial.

## Desktop profile management checkpoint

The Install profiles panel now lists stored profile names on explicit refresh, selects a stored profile, clones, renames, and confirms deletion. Clone/rename share the CLI store's collision and filename handling. Deletion confirmation is bound to both the settings directory and profile name. Desktop deletion clears a matching activeProfileName while preserving unrelated settings and installed files, following the reference dialog's intent. Plain CLI delete retains its existing reference behavior. Malformed settings prevent desktop deletion before profile removal. Settings are cleared before file deletion; a later deletion failure can leave an inactive profile. This is not multi-file crash atomicity.

State tests exercise list/clone/rename/delete and stale confirmation rejection. A shared-store test checks active versus inactive deletion, preservation of nested settings and malformed-settings refusal. Native widget interaction, default storage/startup behavior and the remaining schema/platform cases are still unverified or incomplete. F16 remains partial.

This checkpoint passed formatting, clippy with warnings denied, 53 regular integration tests in both configurations and two desktop-state tests. The explicit large-archive regression was not rerun locally. Commit `702afbb4ad0375911cdc394a9e08ac5de7563edb` passed native CI run 35062531335; the immediately preceding desktop commit's CI was still running when checked.

## Default profile storage checkpoint

Profile CLI commands, profile-backed validate/install, and the desktop settings-directory field now default to the platform configuration directory plus `ModSync`. Explicit settings directories still override it. The `dirs` platform helper uses the native Windows roaming-app-data folder, macOS Application Support, and Linux XDG configuration rules; see https://docs.rs/dirs/6.0.0/dirs/fn.config_dir.html. This avoids implementing native folder APIs separately.

A live Linux comparison with the pinned C# CLI created `ModSync/profiles/Default.json` beneath an existing absolute XDG_CONFIG_HOME in both implementations (`default-profile-directory-check.json`). A subprocess test proves default create, explicit override and named/active profile lookup. Local formatting, clippy, 54 integration tests in both configurations and two desktop-state tests passed. Native macOS/Windows default-folder behavior is not yet differentially verified. Rust reports unavailable config lookup instead of falling back to the working directory; unlike .NET GetFolderPath's default verification, the helper can return a base that does not yet exist. Missing/unreadable-base parity remains open, as does legacy KOTORModSync settings fallback.

Correction to earlier checkpoint notes: pinned-source caller inspection found ProfileService.ApplyProfile invoked by ProfileManagerDialog, not automatically at startup. Automatic startup reapplication is therefore not established as a C# parity requirement; the saved active name and explicit activation behavior remain supported. Broader settings restoration is still F17 work.

Native CI for `97b473bd280e138fdbdebdd0f191574d27ae481e` passed (35062691840). Run 35062899570 for `9e586aec491170a6b597bb4ae4950dfe0ed5100f` failed before all three jobs started; GitHub's annotation reports failed account payments or an insufficient spending limit. This is an infrastructure block, not a test result; further native proof remains outstanding.

## Legacy profile settings fallback checkpoint

Profile settings reads now fall back to the platform configuration directory's `KOTORModSync/settings.json` only when the requested current settings file is absent, including when storage was explicitly supplied. The profile CLI imports its two managed-deployment fields, writes subsequent activation to the current store, and leaves the legacy file unchanged. Current settings take precedence. Corrupt current or legacy JSON is rejected, retaining the documented difference from C#'s reset-to-default error behavior.

A live pinned C#/Rust CLI comparison matched resulting JSON for legacy activation and current-file precedence, with unchanged legacy bytes (`legacy-settings-check.json`). Formatting, clippy, 55 regular integration tests in both configurations and two desktop-state tests passed. General GUI settings migration, schema/casing edge cases and native UI interaction remain open; F16/F17 are not complete. This change does not migrate profile files from a legacy directory.

## Desktop directory preferences checkpoint

Desktop startup now restores C# sourcePath/destinationPath settings when corresponding launch paths were not supplied. Field names are read case-insensitively; non-string/non-null values reject the load before changing either displayed directory. The Load guide page can save both directories through atomic, locked settings writes that retain unrelated fields (including active profile and unknown nested values). The shared settings reader supports the legacy fallback while retaining its full object for directory writes. `gui --settings-dir` chooses a custom store for startup restoration and later profile/settings operations.

C# source inspection grounds the field names and nullable/empty behavior; this checkpoint has Rust store and desktop-state tests, not a live C# GUI comparison. Rust preserves unknown settings instead of serializing only the C# typed schema, and refuses corrupt JSON rather than replacing it. F17 is partial: theme, remaining settings, automatic save lifecycle, native interaction, schema edge cases and secret redaction remain outstanding.

Formatting, clippy with warnings denied, 56 regular integration tests in both configurations and three desktop-state tests passed for this checkpoint. The large-archive regression was not rerun locally; hosted native CI remains unverified under the recorded account billing limitation.

## Desktop theme preference checkpoint

The desktop Theme selector now offers the pinned C# ThemeService choices (Light, KOTOR, KOTOR 2) and persists their canonical `/Styles/*Style.axaml` paths. Startup reads the saved theme, including legacy settings fallback; absent/null uses Light. Reads are case-insensitive for the settings key. Writes atomically preserve unrelated settings and normalize that key. A save failure leaves the displayed theme unchanged. Shared string-field decoding is reused by directory preferences.

Rust uses egui light/dark visuals, with blue `#3AAAFF` and green `#18AE88` interaction/link accents drawn from the reference game themes. This is not a reproduction of the Avalonia styles, backgrounds, typography or images. C# also derives MainConfig.TargetGame from the theme; propagation into Rust's game-dependent consumers remains unimplemented. State tests verify persistence, light/dark switching and failed-save behavior; native visual/interaction proof remains open. F17/F24 stay partial.

This checkpoint passed formatting, clippy with warnings denied, 57 regular integration tests in both configurations and four desktop-state tests. The explicit large-archive test was not rerun locally; no new hosted native CI proof is claimed.

## Native profile/settings acceptance checkpoint

The native binary from `13f50111ec0eb6950ef73a21f29491fe5a6278a4` was driven with actual X11 mouse/keyboard events in an isolated Xvfb session. Screenshots and its binary hash are recorded under `docs/parity/native-profiles/`. The flow covered seeded-directory restoration, KOTOR 2 theme selection, profile capture, clone, rename, activation, stored-name listing and confirmed active-profile deletion. Disk checks verified the retained original profile, removed renamed clone, cleared active name and preserved nested settings.

The same native session ran validation and a confirmed snapshot/install for a single Copy instruction. The game directory was empty after validation; the installed file contained exactly `selected bytes`, with the source unchanged. A new process restored the saved theme and directories. This replaces the earlier blanket lack of Linux native interaction proof for these exercised paths. It does not prove all profile error states, visual equivalence, accessibility, Windows/macOS behavior or full K1/K2 guide execution. No source behavior changed in this acceptance checkpoint; the desktop binary was rebuilt, and the already-passing unit/integration suite was not repeated for documentation/screenshots.

## Tier/category selection checkpoint

The desktop Choose mods page now has exact tier/category filters and additive selection buttons backed by a shared `selection` module. Selecting a known tier includes priorities 1 through that tier; Optional includes priorities 1–4 and Optional. Unknown tiers select exact case-insensitive matches. Category selection adds matching components without clearing other selections. Both bulk operations skip WidescreenOnly components, as the pinned reference wizard does, and clear validation/install confirmation.

Source grounding: ModSelectionPage.SelectByTier and SelectByCategories. A regression covers cumulative known tiers, custom tiers, case-insensitive categories, preservation of existing selections and widescreen exclusion. Full .NET Unicode ordinal-ignore-case equivalence is not claimed. Rust still shows widescreen components in its existing single selection page, lacks the reference's separate widescreen phase and multi-category dialog, and retains its earlier text-search behavior. The new filter controls have not yet had native interaction verification. F04/F24 stay partial.

Investigation note: UseCopyForMoveActions exists in C# configuration/persistence but no execution reader was found in the pinned source. Rust already implements Move as copy then delete. No ineffective switch or alternative move behavior was added based on that setting name.

Formatting, clippy with warnings denied, 58 regular integration tests in both configurations and four desktop-state tests passed. The new selection regression is grounded in inspected reference source, not a live C# wizard differential. The explicit large-archive regression was not rerun locally.

## Spoiler-aware selection text checkpoint

The selection page now offers spoiler-free names/descriptions for components and nested options. Authored NameSpoilerFree/DescriptionSpoilerFree values take precedence; whitespace-only alternatives fall back to the reference's acronym/metadata name and simplified category description. Search now matches displayed name or simplified description, rather than the prior concatenated name/tier/category string. Normal descriptions use the reference's 200 UTF-16-unit threshold and 197-unit prefix plus ellipsis. Directions are hidden while the toggle is enabled. Component data and executable instructions are unchanged.

The acronym implementation follows the pinned converter body, including its differences from comments: JC's Weapons yields JsW, and K1 Restoration yields KR. Tests cover authored overrides, acronym limits/filler handling, hidden-text search exclusion and truncation. These are source-grounded Rust regressions, not a live C# converter differential. Unicode letter/casing behavior and truncation through an unpaired UTF-16 surrogate are not fully equivalent (Rust replaces an unpaired surrogate). The toggle is not yet persisted; links, raw draft review and activity logs can still contain original text. It is a name/description display mode, not comprehensive spoiler redaction. Native interaction and full tooltip/detail parity remain open.

Formatting, clippy with warnings denied, 60 regular integration tests in both configurations and four desktop-state tests passed for this checkpoint. No new native interaction or large-archive execution proof is claimed.

## Spoiler preference persistence and description tooltips

The selection toggle now persists the C# `spoilerFreeMode` boolean using the shared settings store and restores it at desktop startup. Missing defaults to false; case-insensitive field lookup accepts booleans and rejects null/string values. Writes preserve unrelated data and malformed JSON is not replaced. A failed toggle save leaves its previous display mode in effect. Component/option description tooltips now retain the full normal description while spoiler-mode tooltips use the authored/generated safe description, avoiding disclosure of the hidden normal description on hover.

Store and projection tests cover round trips, key casing, defaults, type rejection, corrupt-file preservation and long-description tooltip visibility. Native toggle/restart/hover proof remains outstanding. This does not redact URLs, raw draft review or activity logs and does not complete the reference's comprehensive metadata tooltip. C# settings-load failures fall back to defaults; Rust reports errors and retains files, as documented in earlier checkpoints.

Formatting, clippy with warnings denied, 62 regular integration tests in both configurations and four desktop-state tests passed. No new native interaction, hosted CI or large-archive proof is claimed for this checkpoint.

## Multiple-category chooser checkpoint

The desktop now opens a scrollable category chooser with independent checkboxes, Select categories and Cancel. Its category inventory excludes widescreen-only components and deduplicates/sorts names. Applying calls the existing shared additive selection rule with all checked categories, preserves prior choices and invalidates validation plus install confirmation. No selection changes occur while checking boxes or cancelling; Select is disabled for an empty choice. Loading a new guide clears the pending chooser.

A desktop-state regression applies two categories together, retains an unrelated selected component, excludes widescreen and verifies validation/confirmation reset. This is not native modal-dialog parity: the egui window is not an Avalonia modal window, and Unicode ordering/casing and live interaction remain unverified. The separate widescreen phase still has not been ported.

Formatting, clippy with warnings denied, 62 regular integration tests in both configurations and five desktop-state tests passed. The existing large-archive test was not rerun locally; no new native or hosted CI proof is claimed.

### Native modal input verification

The category chooser and install confirmation now use the installed egui Modal container. Background input is blocked; Escape and backdrop clicks cancel. This supersedes the earlier nonmodal-window limitation for these two dialogs. Native Linux input checks verified blocked background Deselect all and trust-checkbox clicks, category application, install cancellation without game writes or snapshots, and an explicitly confirmed two-component Copy install with exact byte checks. Evidence and source/binary hashes: `docs/parity/native-modals/receipt.json` and adjacent screenshots. Required format, lint and both test configurations passed. Windows/macOS, exhaustive focus/accessibility behavior, reference GUI comparison and the separate widescreen phase remain unverified or incomplete. F04/F24 stay partial.

### Option description visibility

Option descriptions now test the displayed text for whitespace, matching the reference ModSelectionPage option-card condition. An empty raw Description no longer suppresses authored DescriptionSpoilerFree or the generated safe fallback. The shared recursive option renderer applies the same rule to nested options. An egui rendering regression inspects emitted text shapes for both authored and fallback descriptions with an empty raw field. This is rendering-path evidence, not native accessibility or full option-card parity.

### Selection tooltip content

Component names, option checkboxes and description labels now expose a shared comprehensive tooltip following pinned ModSelectionPage.CreateModTooltip: displayed name, author, case-sensitive category descriptions, normalized tier description, full ordinary or safe description, instruction/option counts, installation method, language list and resource count. Category definitions are ported from CategoryTierDefinitions, including custom fallbacks. Numeric-only tiers retain the reference custom-tier description. A regression checks the complete multi-section output, omitted empty sections, casing, tier names and spoiler isolation. Language metadata currently requires a JSON array of strings; malformed or alternate representations do not establish schema parity. Native hover timing, positioning, accessibility and cross-platform rendering remain unverified.

### Filtered bulk selection

Select all now applies the same search/tier/category predicate as visible rows, retaining existing selections outside the filter and excluding widescreen-only components. Deselect all clears every base component regardless of filters and preserves widescreen selections, following SelectAllVisibleMods and DeselectAllMods in the reference. Clear filters resets search, tier and category without changing selection. Regression coverage combines all filters, spoiler display search, additive selection and widescreen preservation. The Rust single-page list still exposes widescreen rows; a separate widescreen phase and native button verification remain open.

### Selection and filter summaries

The selection page reports base-build selected/total counts and the reference none/all/partial selection detail, independently of active filters. A separate summary reports matching base components and active search/category/tier/spoiler filters in reference order. Both exclude widescreen-only components. Labels explicitly identify base-build scope because widescreen rows remain on the single Rust selection page. Regression coverage includes hidden selections, widescreen exclusion, no filters, spoiler mode and empty guides. Reference required/optional badges, separate widescreen flow and native summary layout remain open.

### Required and optional selection badges

The base-selection page now displays the reference required/optional counts. Required counts selected non-widescreen components with at least one declared dependency; optional counts the remaining selected base components. This follows UpdateSelectionCount exactly for supported dependency arrays, rather than inferring counts from tiers, dependency targets or dependency-list length. Explanatory tooltips state the rule. A regression distinguishes each case and excludes unselected/widescreen entries. This supersedes the earlier missing-badges note; native layout and accessibility remain unverified.

### Native selection verification

Linux native input at commit `253ba18301c0b150ca9b5542122dda63430437fe` verified search-filtered Select all, Clear filters, preserved widescreen selection, base counts, required/optional counts, safe component/option hover content and authored safe option description with an empty raw field. Screenshots, corrected fixture and binary hash are in `docs/parity/native-selection/receipt.json`. This supersedes the native-unverified notes for those exercised paths only. Tier/category dropdown interaction, accessibility, other platforms and separate widescreen flow remain open. Some emoji glyphs displayed fallback boxes; visual parity is not established.

### No-result selection state

The desktop now shows an explicit no-results message and recovery hint when no rows match. The details pane uses the shared row-filter predicate, so hidden components no longer retain visible editable options or links. Clearing filters restores details without changing selection. Native Linux screenshots and source/binary hashes are recorded in `docs/parity/native-empty-selection/receipt.json`; required Rust checks passed. This closes the exercised empty-state gap, not full grouped-card/widescreen/accessibility parity.

### Category grouping and expansion

Filtered selection rows now group by the first category (Other when absent), using ordinal UTF-16 category and raw-name ordering while retaining original guide indices for installation. Groups start collapsed and expose selected counts, select/deselect-group actions and global expand/collapse controls. Individual expansion uses egui retained state. Regression covers first-category grouping, secondary-category filtering, raw-name ordering in safe mode, empty matches and unchanged source order. Native group interaction and precise expansion-state lifecycle parity remain unverified. The existing single-page widescreen exposure also remains; group actions currently operate on all displayed members.

### Native group controls

Linux native input at commit `8ba677c` verified initial collapse, expand/collapse all, counts, filtered group deselection, restored selection and retained expansion through the exercised search/clear cycle. Evidence and binary hash: `docs/parity/native-groups/receipt.json`. This supersedes native-unverified notes for these paths only. Exact C# state lifecycle, separate widescreen handling, accessibility and other platforms remain unverified or incomplete. Installation order was not exercised in this GUI session; the shared grouping regression remains its current proof.

### Widescreen execution boundary investigation

`docs/parity/WIDESCREEN.md` records a pinned-source inconsistency: the reference base UI counts exclude widescreen, but its bulk execution receives all components and filters only by selection. The later widescreen single-component path lacks the bulk completed-state skip. Double execution is a source-grounded risk, not a live reproduction. A phased Rust port requires the isolated service oracle and recovery checks in that document. No Rust execution behavior changed in this investigation.

The widescreen double-execution risk is now reproduced at service level: bulk install produced `first` and Completed state, then single install overwrote it with `second`; both returned Success. See `docs/parity/widescreen-oracle/receipt.json`. This is not a GUI or checkpoint reproduction. Rust behavior is unchanged.

An isolated cross-phase dependency fixture now reproduces DependencyViolation with the reference wizard's widescreen-only context and Success with full component context after base installation. The initially unselected wide component was not installed by bulk execution. Evidence: `docs/parity/widescreen-dependency-oracle/`. Rust phased execution must retain completed-base dependency knowledge without replaying base instructions.

### Remaining-prefix engine execution

Engine.install_remaining now accepts a caller-verified completed prefix, retains the full selected guide for dependency/condition context, and runs only the remaining suffix through existing preflight, snapshot and stop-on-failure paths. Unknown, duplicate or non-prefix completed IDs are rejected before install writes. A two-stage fixture modifies the base source after installing it, then verifies the dependent widescreen component installs without replaying base bytes or events. This is a library primitive, not persisted resume: callers must verify guide/root/completion identity. CLI and GUI do not expose it yet. Recovery after a remaining-phase failure and durable completion receipts remain unverified/unimplemented.

### Remaining-phase recovery verification

An isolated two-stage regression now cancels the remaining suffix after its first write overwrites a completed-base file. It verifies no later step runs, retry is blocked while recovery is pending, verified restoration recovers the completed base (not the original empty game), source files return to their phase-entry bytes, and retry runs only suffix steps in order. The pending marker clears only after success. This supersedes the earlier unverified cancellation/recovery path for install_remaining; external patcher failure, process crash, durable completion identity and GUI phase wiring remain open.

### Remaining-suffix validation

Engine.validate_remaining shares completed-prefix checks with install_remaining and simulates only the suffix using full selected dependency context. A regression first executes a base Move that consumes its source: ordinary whole-guide validation fails on the consumed source, while suffix validation succeeds using the installed base output. The check verifies no suffix output, source mutation, recovery-manifest replacement or pending marker is produced. Caller-verified completion remains required; no durable resume or wizard wiring is implied.

### Saved completion evidence

Successful engine installs now atomically save completion.json with canonical roots, ordered selected component definitions and a full game-file hash manifest before clearing the pending marker. Suffix validation/install require an exact receipt match for the requested prefix and current game bytes. Missing/corrupt receipts, changed completed definitions and changed game files are rejected. Source files may change or be consumed; they are not part of completion-byte verification. Snapshots include the prior receipt and restore it (or remove it when absent), preserving the completed-base boundary after suffix failure. This supersedes the earlier caller-only completion contract. Regression coverage includes consumed sources, no replay, rollback/retry, missing/corrupt evidence and changed definitions/game bytes. Empty directories, file permissions, external effects, malicious edits to both state and game, power-loss durability and GUI phase transitions are not proven by this receipt. Hashing the entire game adds I/O proportional to its size.

### Completion summary query

`modsync completion-summary --game-dir <dir> --source-dir <dir> [--json]` reads completion.json without hashing the game and prints the components it records, the receipt's game-file count, and the game-file delta since the run started. Without `--json` it prints plain text, `N/A` when there is no delta and `No completion receipt.` when there is no receipt. With `--json` it prints the `CompletionSummary` document, or `null` when there is no receipt. Both exit 0. The delta is null while an install is pending, when snapshot.json is missing or from other roots, and when the snapshot's stored completion equals the current receipt (after a restore). A receipt written for other roots exits 1 with `Completion roots differ` on stderr. A query with no receipt creates no state directory or lock file. This is a read-only view of the receipt; it does not verify current game bytes.

### Desktop base/widescreen phases

The desktop now masks widescreen selection in the base job without changing authored choices, hides widescreen base-list rows, and transitions after base success to widescreen selection on the install page. That job uses receipt-checked suffix validation/install with full dependency context. Base navigation/profile changes are disabled after success. Zero selected widescreen mods can finish; a pending failed install requires restore before that finish action is enabled. Successful widescreen execution reaches a completion view. A desktop-state regression verifies deferred selection and phase transitions; engine receipt/no-replay/recovery regressions cover execution primitives. Native end-to-end phased interaction remains unverified. Optional warning content, automatic restart/resume navigation, guides with no selected base mods, and phase orders where base is not an execution prefix remain open; unsupported ordering fails instead of being reordered or backfilled. Start a fresh app session for another guide after completion.

### Native two-phase installation

Native Linux input at `7918409` verified base validation/confirmation, base-only writes, transition to widescreen selection, and confirmed dependent widescreen installation after the base source was removed. Final bytes, completion IDs and absence of pending recovery state were asserted. Screenshots, fixture, final completion receipt and binary hash: `docs/parity/native-phases/`. This supersedes native-unverified status for the exercised successful two-phase path; native failure/restore/skip, restart, warning content, empty-base guides, patchers and full builds remain unverified or incomplete.

### Optional widescreen notice

After base success, the desktop shows the reference default warning or authored metadata.widescreenWarningContent (also WidescreenWarningContent) before widescreen selection. Explicit blank/null content skips it; Continue dismisses it for this guide session. Text is displayed literally, matching the reference notice control. A desktop regression covers phase gating, default/authored/blank/null content and dismissal. Native notice interaction and Markdown warning extraction remain unverified/incomplete.

### Markdown widescreen guidance

Markdown now extracts the optional widescreen warning from its case-insensitive heading through the first following level-three heading and starts epilogue content at the subsequent level-two section. The desktop consumes this extracted warning. The isolated C# parser fixture in `docs/parity/markdown-warning-oracle/` confirms exact warning/epilogue text and the reference Mod List boundary excluding later widescreen components. Rust regressions cover CRLF normalization, missing first mod, absent post-widescreen section and desktop metadata consumption. This supersedes the missing warning-extraction status above; widescreen component inclusion/classification outside Mod List, native notice interaction and complete Markdown fidelity remain open.

### Markdown widescreen component classification

For prose guides, WidescreenOnly now follows component position within the optional widescreen section, matching the reference override of embedded metadata. An isolated C# fixture without a Mod List heading confirms base/base/widescreen/base classification and warning/epilogue boundaries; the Rust regression compares its saved output directly and covers CRLF, missing header and missing closing section. Evidence: `docs/parity/markdown-phase-oracle/`. This closes the positional classification gap above. The existing Mod List exclusion is unchanged. Full corpus equivalence and native Markdown-to-phased-install proof remain open; classification does not relax the installer requirement that completed base components form an execution prefix.

### Finish returns to setup

The successful completion view now has a Finish action returning to guide setup, retaining the loaded guide, directories and output log while clearing transient phase/confirmation/validation state. Loading a guide reuses the same reset. A desktop regression verifies this boundary; a new installation requires fresh validation. This supersedes the restart-required limitation above. Saved completion/recovery files are untouched; this is not restart/resume support. Native repeated-install interaction remains unverified. The reference FinishedPage contains static next steps, not authored epilogue display; epilogue documentation export is still a separate parity gap.

### Desktop validation invalidation on setup failure

Starting any desktop job now clears prior validation and install confirmation before checking directories or archive settings. Early setup failures can no longer leave the old install-ready state active. A regression covers invalid archive budgets for validate/install/restore/download and an install directory removed before launch, asserting that no worker starts. This repairs desktop state only; engine preflight already independently rejects invalid inputs.

### Wizard content export preservation

TOML and YAML export now place extracted Markdown wizard-content fields in the reference metadata object, supplying fileFormatVersion 2.0 when absent. Existing nested metadata overrides root content, including explicit blanks and PascalCase aliases. A Markdown-to-TOML/YAML regression verifies content retention and stable re-export; JSON/XML remain component-only exports. Live C# re-import of this new export path remains unverified.

### C# re-import of wizard content exports

At Rust be7052a, a fresh CLI build exported the saved Markdown warning fixture to TOML and YAML. The pinned C# deserializer imported both in an isolated process and recovered exact preamble, epilogue and widescreen-warning text plus both components. `docs/parity/content-export-oracle/` contains exports, probe, result, hashes and a runnable verifier. This supersedes the unverified cross-language re-import note above for this fixture only; full corpus and native UI proof remain open.

### YAML metadata comment recognition

YAML documents beginning with the reference case-insensitive `# Metadata` marker now load as metadata even without fileFormatVersion. Document separators must be unindented; separator-like text in block scalars remains data. Regression covers marker-only metadata, CRLF, a leading YAML directive, descriptions containing marker text, re-export and invalid non-mapping metadata. Exotic YAML directive/tag combinations and live C# differential coverage for these cases remain open.

YAML directive regression correction: directives after an explicit document end now stay with the following document. A three-document fixture first proves acceptance by serde_yaml_ng, then verifies guide components and metadata. The test failed under 795420a and passes with this correction; exhaustive YAML grammar compatibility is not claimed.

### FOMOD reference selection investigation

`docs/parity/FOMOD.md` records the full implementation/acceptance boundary. The isolated C# parser/mapper/presenter probe in `fomod-selection-oracle/` verifies five group defaults and selection validation, and reproduces an indirect required-plugin deselection conflict. Rust still refuses FOMOD installation; no FOMOD support is claimed from this evidence.

### FOMOD shared group selection

`fomod::GroupSelection` now implements the five reference group modes, default selection and exact cardinality messages. An executable regression compares defaults and selection transitions against the saved C# oracle. Required-plugin removal is rejected both directly and through exclusive siblings, with no partial mutation; this deliberately corrects the reproduced reference bypass. Conflicting required choices remain validation errors. Parser, visibility, mapping, choices files, CLI/desktop wiring and installation remain unfinished, so the engine still refuses FOMOD archives.

### FOMOD choices-file primitives

The library reads/writes the reference version/archives/selections camelCase schema (missing version defaults to 1), finds archive filenames ignoring ASCII case, and applies named plugin groups atomically through shared cardinality validation. Repeated plugin names collapse to one choice. Unsupported versions, duplicate archive/group entries, missing or ambiguous plugins, omitted required plugins and invalid cardinality are rejected without mutating selections. These are explicit stricter checks than permissive reference behavior. Unicode ordinal case equivalence, whole-session step lookup/visibility, CLI flags and configured installation remain open.

### FOMOD choices across steps

ArchiveChoices.apply_to routes named selections through unconditional StepSelection groups using ASCII-insensitive lookup. It validates the candidate set before committing any changes: late failures, missing/ambiguous names, duplicate choices and invalid untouched defaults leave all original groups unchanged. This closes whole-set atomicity for unconditional steps only. Visibility-dependent validation and session flag propagation remain required before this can configure arbitrary FOMOD packages.

### FOMOD flag-based visibility

Step selections now carry optional nested All/Any/Flag conditions, and plugins carry ordered flag assignments. Visibility follows the reference source: all selected plugins contribute even from hidden steps, later assignments win, flag names ignore ASCII case and values are case-sensitive; empty composites are true. Choices application validates resulting visible steps. Regression covers hidden flags, overwrite order, nested conditions, empty Or, value casing and hidden invalid groups becoming visible. Live reference differential proof, Unicode equivalence, file/game dependencies and package integration remain open.

### FOMOD XML selection reader

The library now reads the selection portion of ModuleConfig.xml into shared steps/groups/plugins, including Required/Recommended defaults, dependencyType defaultType, condition flags and nested flag visibility. It uses namespace-local names, handles escaped attributes/CDATA, rejects DTDs/external entities and malformed roots, and bounds nesting. Unsupported non-flag visibility conditions fail explicitly. This reader intentionally does not produce install instructions or enable FOMOD extraction: files, priorities, descriptions/images, full module metadata and conditional file mapping remain to be added to a complete module parser. Regression checks XML-driven visibility/defaults and rejection boundaries; live differential XML coverage remains open.

### FOMOD plugin details

XML selection state now preserves trimmed plugin descriptions and authored image paths. Nested description text is concatenated in document order, matching XElement.Value semantics, with entity and CDATA decoding. Image paths remain untrusted metadata and are not opened by parsing. Regression covers mixed text, missing fields and an escaping image path retained without filesystem access. Image rendering/path containment and live C# differential proof remain open.

### FOMOD module file data

parse_module_xml now retains module name, required files, plugin files and flag-conditional file patterns alongside selection state. File/folder records preserve authored source/destination and order; signed 32-bit priority parses with whitespace and defaults to zero for missing/invalid/overflow values, following the reference parser. Missing conditional dependencies become an always-true empty conjunction. Parsing does not resolve paths, sort priorities or authorize writes. File/game dependencies, info.xml, instruction mapping, archive containment and installer integration remain open.

### FOMOD selected file plan

ModuleConfig.selected_files builds a read-only ordered plan: required files, selected visible plugin files with their attached conditional files, then unconditional patterns. Supported flat flag patterns attach to the first plugin declaring all required flags, even if another selected plugin declares them too, following the reference mapper. Nested/multi-branch OR patterns and flags not supplied by one plugin are skipped, matching the reference mapper's scoped fallback. A network-disabled C# mapper probe confirms both skip cases leave only plugin files and an unconditional trailing file; source, result, and receipt are in `docs/parity/fomod-conditional-skip-oracle/`. Priorities remain preserved data: the reference mapper does not sort them. Safe path/instruction mapping and real installation remain open.

### FOMOD file-copy instruction mapping

ModuleConfig.copy_instructions maps the ordered selected file plan from an explicitly supplied extracted mod-relative directory to shared Copy instructions. Existing cross-platform path validation rejects traversal, rooted paths, wildcards and reserved names before instructions are returned. Compatible filename destinations become parent directories; requested renames and folder directives fail explicitly until implemented. A temporary-directory integration test proves dry validation creates no game files and installation copies exact bytes without changing the source. Archive extraction/configuration gates remain unchanged; this is a library mapping primitive, not end-to-end FOMOD support.

### Verified FOMOD folder mapping

An isolated C# mapper/install probe returned Success with `textures/*`, installed only top.txt and omitted nested/deep.txt. Evidence: `docs/parity/fomod-folder-oracle/`; the verifier excludes only C# .modsync session bookkeeping from asset comparison. Rust now emits the same immediate-file wildcard and a shared-engine regression verifies identical asset behavior, dry-run non-mutation and unchanged source data. This supersedes folder mapping rejection above. It is deliberately not recursive FOMOD directory semantics; empty folders, recursive expectations, renames, archive configuration and UI/CLI integration remain open.

### FOMOD info.xml metadata

The shared XML reader now parses Name, Author, Version, Website and Description with the reference parser's optional-field defaults, first direct-child lookup, case-insensitive local names and preserved authored text. A regression covers namespaces, duplicate fields, mixed text, malformed XML and DTD rejection. This is source-grounded against FomodParser.ParseInfoXml, not a live C# differential receipt. Website remains untrusted text; archive discovery and UI presentation are still pending.

### FOMOD detector segment boundary

Extraction now uses a shared terminal-segment detector matching FomodDetector.IsModuleConfigPath instead of a raw suffix check. Nested and mixed-case FOMOD paths still require configuration; an unrelated notfomod directory no longer causes false refusal. Isolated ZIP preflight coverage verifies both outcomes without writes. Detection alone does not authorize paths or configured installation.

### FOMOD archive inspection

The shared archive reader can now inspect and parse a nested ModuleConfig.xml plus sibling info.xml, returning their content root without retaining extracted files. It uses existing archive safety, size-budget and cancellation checks and temporary scratch storage. Multiple installers fail explicitly rather than choosing an enumeration-dependent first entry. ZIP regressions cover metadata, unchanged source bytes, cleanup, malformed XML, corrupt archives, unsafe paths and cancellation. This spools the whole archive and is not the C# listing-only probe; configured installation and interface integration remain open.

### FOMOD inspection CLI

`modsync fomod-inspect <archive> [--scratch-dir <directory>] [--max-expanded-bytes <bytes>]` exposes archive inspection as JSON: module path, content root, optional info metadata, steps, current visibility, group modes and plugin defaults. Non-FOMOD archives return JSON null; invalid archives exit with an error. The ZIP integration test invokes the actual CLI and checks its report. Inspection does not configure or authorize installation.

### CLI FOMOD choices validation

`fomod-inspect --choices <json>` applies the matching archive entry through the shared atomic selection logic before reporting selections and visibility. Archive, step, group and plugin matching reuse the existing case-insensitive rules. Missing archive entries, absent installers and invalid selections return errors without a success report. A subprocess regression verifies selection changes, invalid names and unchanged archive bytes. This does not persist configuration or enable installation.

### Archive-relative FOMOD copy plans

ArchiveModule copy mapping now retains the wrapper directory above fomod/ModuleConfig.xml. The inspection CLI can emit the validated shared-engine copy instructions with `--extracted-directory <relative-path>`, after optional choices application. Invalid selections and unsafe roots fail before output. A regression checks wrapper-relative source mapping. The command does not extract into that path or install; automatic execution still requires archive configuration and recovery integration.

### Configured FOMOD extraction execution

A component may now embed the version-1 choices document in its FomodChoices metadata. At each Extract instruction, the shared engine parses the actual archive configuration, requires a matching archive entry, applies and validates choices, and runs mapped copies after extraction. Wrapped paths remain relative to the actual extraction destination, which must be inside the mod directory. This uses ordinary preflight, snapshot, ordered execution and recovery; choices remain serialized with component metadata in completion receipts. A real ZIP regression proves default refusal, non-mutating validation and selected output bytes. Automatic prompting/configuration persistence UI, broader archive fixtures and failure/restore acceptance remain open.

### Configured FOMOD cancellation and restore acceptance

The configured ZIP install regression now cancels after the FOMOD Extract/copy step and before a later Delete. It verifies selected output exists at failure, the later step did not run, retry is refused while recovery is pending, restore removes extracted files and restores exact baseline/archive bytes, and an ordered retry completes with no pending marker. This covers cancellation between guide instructions, not every interruption point within archive extraction or copying.

### CLI configured FOMOD installation

Validate and install accept --fomod-choices with the existing version-1 JSON contract. Explicit CLI choices override embedded choices for the run, including option components, and are retained in completion component definitions without rewriting the input guide. A subprocess test validates without writes, installs a real configured ZIP, checks exact output bytes and verifies choices in the completion receipt. Desktop selection controls and automatic post-download prompts remain open.

### Desktop FOMOD choices-file input

The validation page now accepts an optional choices-file path using the existing file picker. Desktop validation and installation parse that file and use the same recursive guide override as CLI; load errors invalidate validation and prevent job launch. Restore does not depend on the choices file. This is file-based configuration, not the full interactive plugin-selection wizard; native visual acceptance remains pending.

### FOMOD source provenance guard

Configured copies now require every matched source to be supplied by the current archive. Stale extraction files cannot satisfy missing file directives or join folder wildcard copies. Configured extraction with Overwrite=false rejects existing archive paths rather than retaining stale bytes. Regression installs cover these three cases and verify rejection during preflight, unchanged stale source bytes and no pending snapshot or game writes. This intentionally fails closed on contaminated extraction directories instead of silently using their contents.

### Saving FOMOD selections

The inspection CLI accepts --save-choices to atomically write the current selection session, including choices applied with --choices, in the reusable version-1 schema. Capture preserves hidden-step selections and rejects invalid or ambiguous named groups by reapplying the generated document before saving. A round-trip regression verifies nondefault selection persistence and ambiguity rejection. Interactive selection editing remains outstanding.

### Choices export archive preservation

A Unix subprocess regression reproduced source-archive replacement when --save-choices named the archive through a symlinked parent directory. Export now compares canonical existing paths before writing and propagates unexpected resolution errors. The regression also reads back a successful choices export and verifies original archive bytes survive the rejected alias write. Concurrent external path replacement is outside this check's guarantee.

### Interactive console FOMOD selections

fomod-inspect --interactive prompts for numbered plugin selections, reusing group cardinality and required-plugin rules, recomputing visibility before each step and retaining edits only on success. Invalid answers retry; q or EOF cancels. Prompts use stderr so stdout remains JSON, and --save-choices persists the result for installation. A stream-driven regression covers retry, selection and cancellation. Back navigation and the desktop interactive wizard remain open.

### Console FOMOD back navigation

The interactive prompt accepts back at a group to revisit the preceding currently visible step. Revised flags are evaluated before advancing again; the regression revisits an earlier choice and hides the previously displayed following step. Cancellation still discards the whole pending session. Native console interaction and the desktop interactive wizard remain unverified.

### Interactive CLI to installation acceptance

The real CLI subprocess fixture now selects a nondefault FOMOD plugin through piped stdin, saves its choices, parses the stdout JSON, validates without install-tree writes and installs using the exported file. It verifies only alternate asset bytes are installed, prompts stay on stderr, the source archive and guide remain unchanged, and the receipt records the selected plugin. This proves the console protocol end to end, not terminal rendering or desktop interaction.

### Desktop interactive FOMOD configuration

The validation page now offers Configure FOMOD archive. A background worker inspects the selected archive, then a modal displays visible steps and plugin checkboxes through the shared selection rules. Accept captures valid choices and merges them into loaded guide metadata, preserving other archive configurations and invalidating installation readiness. Cancel discards pending edits and requests worker cancellation. A merge regression verifies package preservation. Native rendering/click acceptance, archive images, persistence to a saved guide and automatic post-download prompting remain outstanding.

### Reopening desktop FOMOD selections

The desktop editor now reapplies saved archive choices after inspection, using the selected external choices file or embedded guide configuration. Invalid saved selections stop loading instead of silently reverting to defaults. Conflicting component configurations are reported. A shared resolver regression covers nested components, case-insensitive archive names, missing entries and conflicts; native reopening acceptance remains pending.

### Desktop configured-guide persistence

The existing Save selection action now embeds the selected external FOMOD choices file into its saved guide, as installation does. Accepted editor choices were already embedded in the loaded guide and use the same save path. A desktop regression saves TOML, reloads component selections and named archive choices, verifies the in-memory guide remains unchanged, and proves invalid choices leave an existing output untouched. Native save-dialog acceptance remains pending.

### Desktop archive path entry

An isolated Linux X11 run rendered the validation page, but the system archive picker did not appear on that display. FOMOD configuration now accepts a typed archive path with optional Browse, matching the existing path-field pattern. This removes the picker as a prerequisite for opening the editor; full native editor/install acceptance is still pending.

### Native FOMOD editor/install acceptance

Linux X11 acceptance at commit 3e7cf9f opened the desktop editor from a typed archive path, selected Alternate instead of Default, accepted and reopened the session, validated without changing either install tree, and confirmed a real install. The GUI reached Installation complete; only alternate.txt reached Override with exact expected bytes, the completion receipt records Alternate, and no pending marker remains. Screenshots, fixture ZIP, receipt and binary hash are in docs/parity/native-fomod. Save-dialog/restart persistence, previews, automatic prompts, other platforms and full builds remain outside this proof.

### Desktop post-download FOMOD prompt

Successful verified direct downloads now pass their actual output path and archive limits to the desktop FOMOD editor when a guide is loaded before base installation. Before launching, Rust matches the downloaded filename against resource-registry entries and uses shared prompt state: registered `configured` and `warned` archives stay quiet, while `dismissed` or unregistered archives remain eligible for inspection. Inspection runs in the background, restores existing choices through the same resolver as manual opening, and dismisses ordinary archives without a configuration error. Failed downloads do not dispatch this event; inspection errors remain visible. An egui regression checks ordinary-archive dismissal, status suppression, and visible errors. Automatic post-download editor cancellation persists `dismissed` on matching registered resources only after inspection positively identifies a FOMOD, so a later download re-prompts as C# requires; cancellation during inspection and manual editor cancellation leave state unchanged. Native network-download-to-editor acceptance and multi-host queue integration remain pending.

### Persisted FOMOD prompt state

The Rust guide resource registry now preserves the C# `handlerMetadata.fomodPromptStatus` shape per matching archive filename. It recognizes `dismissed`, `configured`, and `warned`: dismissed archives prompt again, while configured and warned archives do not. Applying or merging valid Rust choices records `configured` when the component already has a resource entry for that archive, preserving existing metadata and its serialized key casing. The shared validation gate now also enumerates archive-like `ResourceRegistry.Files` entries independently of `Extract` instructions, safely resolves their mod-directory paths, and inspects existing archives before requiring explicit choices for detected FOMODs. A focused regression proves a registered nested FOMOD cannot evade the gate simply by omitting an Extract instruction. Registered archives that become missing after a persisted prompt state and registered unreadable archives now fail closed. (Later sections add the missing-archive soft warning without prompt state, dependency-scope expansion and post-download orchestration; the configured-without-merged-instructions warning has only the no-route analogue.) The state by itself does not authorize execution: the Rust validation gate still requires explicit `FomodChoices`.

### Desktop FOMOD inspection lifecycle

A worker-lifecycle regression verifies that replacing an inspection requests cancellation of the old worker, disconnects its result channel, and receives the replacement worker's own error. Dropping the editor requests cancellation of the current worker. This establishes result isolation and cancellation signaling; it does not prove a bound on archive-reader cancellation latency.

### Desktop FOMOD step navigation

The editor now displays one visible step at a time with Back and Next actions, matching the C# dialog flow. Next validates the current groups and recomputes flag-dependent visibility; final acceptance still validates captured choices. Descriptions appear inline and required plugins have disabled checkboxes. A regression covers returning to revise an earlier choice, skipping a newly hidden step, invalid selection blocking advancement, and packages without optional steps. Native acceptance of the new multi-step navigation remains pending; earlier native evidence covers the previous single-panel editor.

### Native FOMOD multi-step acceptance

At d813d01, an isolated Linux X11 run exercised Next to a conditional step, Back to change its controlling selection, and Next directly to the last remaining step. Clearing the exclusive selection blocked advancement with a visible error. Accepting and reopening retained the revised choice and reset navigation to the first visible step. The game stayed empty and the mod directory retained only the original archive. Fixture, screenshots, binary hash and limitations are recorded in docs/parity/native-fomod-steps/receipt.json. This verifies native navigation; installation, process restart and other platforms were not exercised in this run.

### Shared file-conflict analysis and CLI

The shared engine now exposes analyze_conflicts, and the conflicts CLI accepts the same guide/profile/selection/FOMOD arguments as validate. Reports contain case-insensitive game-file write histories, first-observed path casing, ordered component writers with zero-based instruction indices, last writer, component counts and analyzed component count. Repeated writes by one component collapse to its latest instruction; skipped overwrite=false operations and mod-workspace staging do not count. Option writes are attributed to their top-level component and Choose instruction. External program effects are explicitly excluded. Unlike the C# analyzer, simulation errors abort rather than returning a report that silently omits failed instructions. Tests compare the real CLI report to the shared result, prove analysis leaves both trees untouched, and confirm the reported last writer against a real isolated install. Desktop presentation, C# differential execution and broader instruction/option coverage remain pending; F19 is partial.

### Desktop file-conflict report

The validation page can now run shared conflict analysis in its background worker before base installation. The expandable report lists paths and ordered writers with instruction numbers and last-writer labels, exposes external-effect limitations, and copies JSON. Input/readiness invalidation clears the report, and conflict success does not authorize installation. A desktop worker regression checks the report, read-only game state and stale-result clearing; setup-failure tests include the new action. Native rendering/click acceptance and remaining-phase analysis are still pending.

### C# conflict-service differential fixture

The pinned C# FileConflictAnalyzer was executed in a network-disabled, read-only-root sandbox with only an isolated fixture writable. Its report agrees with Rust on destination, ordered writers, latest instruction index per writer, last writer, per-component counts and analyzed count for repeated Copy instructions, overwrite=false and staging exclusions. The Rust regression consumes the captured C# result; probe source, DLL hash, result and boundaries are recorded in docs/parity/conflict-oracle. Both analyzers leave the fixture game empty. This is one service-level fixture, not full conflict parity or native desktop acceptance.

### Native desktop conflict-report acceptance

At commit 4bdd53a, an isolated Linux X11 run started conflict analysis from the validation page, displayed one conflict across two analyzed components, expanded its destination, and showed both writers in install order with the final writer labeled. Changing the component selection removed the stale report. The game tree remained empty and the mod tree retained only its two source files. Screenshots, fixture inputs, the binary hash, and proof boundaries are recorded in `docs/parity/native-conflicts/receipt.json`. Clipboard contents, other platforms, patchers, and broader instruction combinations remain unverified.

### Remaining-phase conflict analysis

Conflict analysis now has the same saved-completion prefix contract as remaining validation and installation. After a verified base phase, the desktop action analyzes only selected remaining components and keeps it available for the widescreen phase. The shared-engine regression installs a base component, enables a dependent tail component, verifies that the tail alone is analyzed without recreating a base conflict, confirms both trees remain unchanged, and rejects a non-prefix completion list. Native rendering of this post-base action remains unverified.

### FOMOD plugin type descriptors

FOMOD parsing now preserves Optional, Required, Recommended, NotUsable, and CouldBeUsable descriptors on every plugin. Required and Recommended retain the C# default-selection behavior; unknown descriptors fall back to Optional. The `fomod-inspect` JSON report exposes `pluginType`, so callers no longer have to infer the authored state from the required and selected booleans. A sandboxed execution of the pinned C# parser agrees on all six type cases, including a dependencyType default and unknown fallback; source, result, and DLL hash are in `docs/parity/fomod-plugin-types/`. DependencyType per-pattern rules, full-corpus behavior, and native UI presentation remain open.

### FOMOD file and game dependencies

The pinned C# mapper logs fileDependency and gameDependency as unmodeled, then treats each as satisfied. Rust now preserves that behavior as an always-satisfied dependency so a file or game leaf does not hide a step or discard a conditional-file pattern. Flag leaves in the same AND group remain enforced. A network-disabled C# parser/mapper probe confirms both dependency kinds evaluate true in visible and conditional contexts; source, output, and receipt are in `docs/parity/fomod-unmodeled-dependencies/`. This does not model game state or file presence, and nested unsupported dependency shapes remain rejected.

### FOMOD file rename mapping

The C# mapper cannot express a rename through its Copy action. For a file destination containing a filename, it logs the requested rename, drops that filename, and copies the source filename into the destination directory. Rust now does the same. An isolated install regression verifies that `textures/a.tga` requested as `Override/b.tga` installs `Override/a.tga`; a network-disabled C# mapper probe records the same generated source and destination in `docs/parity/fomod-rename-oracle/`. Archive extraction, full FOMOD corpus behavior, and native desktop parity remain outside this proof.

### Managed deployment manifests

The shared deployment service stages `staging/<component-guid>` trees into the game directory using hardlinks with a copy fallback, writes C#-shaped per-component JSON manifests (seven-digit `DeployedUtc`, lowercase GUID file names), backs up overwritten files, and exposes status, deploy, uninstall, newest-first purge, snapshot, record and switch through the CLI. Roots come from `--staging-dir`/`--manifest-dir` or from a profile's `profiles/<name>/{staging,deployment}` directory, as in C#. Uninstall removes only files whose current SHA-256 matches the manifest and restores a displaced file only after that removal; user-modified deployed files and their backups remain intact. Deliberate differences from C#: redeploying a component that still has a manifest is refused (C# overwrites the original backup), the staged tree is preflighted and a failed deploy rolls back, symlinked targets and staged symlinks are refused, existing case variants of game paths are reused, an out-of-order uninstall hands its original backup to the later owner, corrupt manifests are skipped with a warning, and patcher provenance records a pre-existing changed file only with a backup of its prior bytes. Real CLI probes and a C# DLL differential (C# writes and Rust uninstalls; mixed C# and Rust manifests purged by Rust) are recorded in `docs/parity/deployment-csharp-audit.md`. Engine staging, GUI controls, multi-platform filesystem behavior and a C#-side purge of Rust manifests remain open.

### Profile-switch purge safeguards

The CLI switch takes the canonical game-directory lock before reading the active
profile, checks target report prerequisites, and preflights every readable current
manifest with the shared uninstall-blocker query before any purge. A modified
file, a path replaced by a directory, or an unsafe path blocks switching without
partially purging other components. After purge, skipped entries or remaining
manifests block activation. Ordinary uninstall/purge hash-mismatch semantics are
unchanged. The 39 source-bound direct CLI assertions, production gates, and spec
and quality approvals are in [the wave-two receipt](docs/parity/wave2-integration/README.md).

This does not make unreadable/corrupt manifests fail closed: the underlying service
still warns and skips them. Plain profile activation uses a separate settings lock,
and no atomicity against uncooperative external filesystem writers is claimed.
Managed engine integration and desktop profile management remain incomplete.

### Concurrent direct-download queue

`download-batch` accepts a JSON array of direct HTTPS jobs and runs a bounded worker queue with retry counts and stable input-order JSON results. The shared queue marks work that has not started after cancellation as cancelled and leaves failed direct-download output unpublished because the existing downloader uses a temporary file. Deterministic executor tests cover retry, terminal failure, result order and cancellation before start. Per-byte progress/speed, live cancellation control, provider-specific handlers, GUI status/restart behavior and controlled HTTPS transport acceptance remain open.

### Interactive dependency selection checkpoint

Desktop component and option toggles now route through a shared selection resolver matching the pinned `ComponentSelectionService` behavior: selecting a component follows transitive component dependencies, resolves component/option restrictions in both directions and selects required options; selecting an option selects its parent and its component/option dependencies; deselection cascades through dependents and clears a parent when its last option is removed. Filtered, tier, category and group bulk selection use the same resolver, while base deselection cascades instead of leaving selected dependents behind. A focused Rust regression covers the reference test suite's component chain, mixed restriction, option dependency, reverse option restriction and deselection cases. The pinned C# test source contains corresponding executable cases, but a fresh local reference run could not progress past NuGet restore and was cancelled after five minutes; this checkpoint therefore has source/test alignment and Rust execution evidence, not a fresh cross-runtime result. Native pointer interaction, nested options beyond the reference's immediate option model and Unicode identifier equivalence remain open.


## Wave 2 integration follow-up — 2026-09-28 UTC

The engine's source-tree scan now excludes `.peer-*.part` directories using the
same predicate as download planning. Unaccepted peer payloads therefore do not
become wildcard install inputs. The game-tree scan is unchanged, and recovery
snapshots still cover the complete source tree and reject symlinks. A direct CLI
install copied an accepted file while leaving the staged peer payload untouched.
This is a source-discovery safeguard, not complete peer acquisition or F29 proof.

## FOMOD S2a reconfiguration and stale choices — 2026-09-28 UTC

`fetch-downloads --fomod-choices <file>` with an entry for an archive now replaces
different or stale saved choices for that archive (`reconfigured`). It keeps a single
entry per archive, keeps other archives' entries unchanged, and makes no edit when the
choices are identical. Only the flag reconfigures or recovers saved choices:
`MODSYNC_FOMOD_CHOICES` keeps its S1 role of configuring pending consumers, and
`--fomod-skip` still wins over the flag. Interactive, warn and skip never touch
configured consumers. When stale saved choices need the flag's file and it cannot be
read or parsed, the file error is reported; when it loads but has no entry for that
archive, that is reported too. An entry that does not fit the current
archive fails the run without changing consumers whose saved choices are valid.
`validate --validation-report` now applies saved choices to the current archive
instead of checking only that an entry exists. Stale or invalid choices, and
C#-configured guides with no `FomodChoices`, fail with the supported import route.
Saved `FomodChoices` that cannot be parsed (for example version 2 or a duplicate
archive) are never overwritten by Fetch, since that could discard other archives'
entries. Fetch, validate and install instead name a repair hint: repair that
component's `FomodChoices` in the guide (removing it discards every archive's saved
choices for that component), then capture and record any missing choices. A
`--fomod-choices` entry still reconfigures the archive's other consumers, and the run
fails for the unparseable one.
Prompt status is never treated as executable choices.

A registered FOMOD archive with valid choices but no selected `Extract` passes with
one warning per archive, naming its registrants: configuration alone installs no
files. Choices carry no byte identity (as in C#). A replacement archive that still
accepts the saved names is mapped fresh from its current bytes. Evidence:
[S2a proof](docs/parity/fomod-s2a-proof/README.md), 217/217 checks. (Superseded by
S2b: the run-scoped `validate`/`install --fomod-choices` once replaced all embedded
archive choices for that run; it now merges per archive. It is still not a recovery
route.) Unicode matching, desktop reconfiguration and the S3 corpora remain open, and
F15 stays partial.

## FOMOD S2b run choices, Unicode names, gate and fallbacks — 2026-09-28 UTC

`validate`/`install --fomod-choices` is a Rust extension: C# validate and install take
no choices file (only Fetch's post-download hook does). It now merges per archive. A
file entry replaces that archive's embedded choices for the run, and other archives
keep theirs. The guide is never written, and with `Retained` completion context
completed components are not touched. `--phase remaining` still applies the file to
the reconstructed completed definitions, and the receipt refuses a different file
before any write. Both phases must therefore use the same binary: a base recorded by
a build before `37cd4cc`, by `37cd4cc` without its review follow-up, or by `c28dee0`
for a guide with folder or glob character-class Extract sources, reconstructs
different completed definitions and must be re-run. A file entry is applied only to
components (and options) that register the archive or may extract it through their
own Extract source, so an unrelated component's unparseable `FomodChoices` do not fail
the run. A source may extract the archive when its last component matches the archive
name with the engine's glob rules (including `[...]` classes) or is not an archive file
name, since the engine also extracts every archive below a folder source; this may
include a component that turns out not to extract it. A folder source whose own name ends in an archive extension (for example a directory
named `Mod.zip`) is compared as a file name, not treated as a folder, so its component
fails for lack of choices rather than installing unconfigured; a component included by
this rule whose own `FomodChoices` cannot be parsed fails with the repair hint.
Unparseable embedded `FomodChoices` on a component the file applies to fail with the
repair hint instead of being replaced. This supersedes the earlier whole-file replacement noted
under "CLI configured FOMOD installation" and in the S2a section.

FOMOD archive, step, group, plugin and flag names, and prompt-status archive keys,
now match like C# `OrdinalIgnoreCase` (`Ä` matches `ä`; `ß` does not match `SS`;
Greek iota-subscript letters such as `ᾀ`/`ᾈ` and `ᾳ`/`ᾼ` use their simple uppercase
mapping), replacing the earlier ASCII-only matching. Recording a status updates an
existing key of any case in place. .NET casing-table exceptions are not replicated.

Plain `validate` and `install` now run the registered-archive FOMOD gate, as C# does
on every validate path and at install. Missing, stale or configured-only choices and
a missing archive with recorded state fail before any simulation or write. The
no-route warning becomes a `WARNING:` event. This includes desktop installation
through the shared engine. The gate inspects each archive path once per run, however
many components register or extract it, and spools an archive to scratch only when
its verified listing names `fomod/ModuleConfig.xml`.

(Superseded for `validate`/`install` since `f68dcfc`; see
[visible FOMOD fallbacks](docs/parity/fomod-visible-fallbacks.md): those verbs now
report each fallback once as a FomodConfiguration warning prefixed
`FOMOD archive '<name>': `, counted in `warningCount`, and no longer log the C# line.
Fetch configuration and `fomod-inspect` still log the C# wording below. The
2026-10-03 FOMOD S2b proof rerun checks the new contract.)
C#'s mapper fallback warnings are logged with its exact `[FomodToComponentMapper]`
wording and no archive prefix:
unevaluated file/game dependencies, skipped conditional patterns and dropped renames.
They are logged by the gate, by Fetch configuration and by `fomod-inspect`. These
verbs mirror the log to stderr, and the `--validation-report` JSON is unchanged.
Plain `validate`/`install --json` stdout gains one `WARNING:` event line per no-route
archive from the gate. C# has no warning for group defaults or auto-selection, so Rust
adds none. Evidence: [S2b proof](docs/parity/fomod-s2b-proof/README.md), 254/254 checks
at `c28dee0` plus its scope follow-up (70 of the original 200 fail on `b6d1d88`; 19
of 232 fail on `37cd4cc`; 10 of 254 fail on `c28dee0`). Ownership propagation, gate
dependency scope and the missing-archive soft warning are settled in the next section;
desktop reconfiguration and the S3 corpora remain open, and F15 stays partial.

## FOMOD gate scope and residuals — 2026-10-03 UTC

The FOMOD gate (plain `validate`/`install`, the report's FomodConfiguration stage and
the desktop install through the shared engine) now covers the selected components and
their hard dependencies, transitively, selected or not, like C#
`FomodConfigurationGate.ExpandWithHardDependencies`. Unknown dependency GUIDs and
completed components are skipped. A dependency outside the selection has only its
registered archives gated, as in C#. The gate reports FOMOD findings first, so an
unconfigured or stale FOMOD archive of an unselected dependency is named instead of
being masked. Rust's stricter readiness checks then still apply: an unselected
dependency, a restriction conflict, an invalid order or unreviewed drafts fail the gate
(C# only warns for a missing dependency). No run that failed before now passes. The
CLI selects every component unless `--use-file-selection`, `--select` or a profile
narrows the selection.

A registered archive that is missing without prompt state now gets C#'s soft warning,
once per registering component: `<component>: Registered archive '<file>' is not on
disk under the mod directory. Re-download the archive (Fetch Downloads), then configure
the FOMOD installer before validate/install.` It is a counted report warning and a
`WARNING:` event; it never blocks. With recorded prompt state a missing archive is still
a hard error.

Already in source and now proved at runtime: run choices reach only the components and
options that register an archive or may extract it; a directory whose name ends in an
archive extension (for example `Pack.zip/`) is a folder that owns only the archives it
contains, so a run entry for `inner.zip` configures it and an unrelated suffixed folder's
unparseable choices do not fail the run; and a run entry replaces only its own archive's
embedded choices. The `--fomod-choices` help on `install`/`validate` now says it merges per
archive. Evidence: [gate-scope proof](docs/parity/fomod-gate-scope-proof/README.md),
74/74 checks (12 fail on `15b11cb`); contract in
[fomod-gate-scope.md](docs/parity/fomod-gate-scope.md). F15 stays partial.

A registered FOMOD archive with prompt status `configured` and valid saved choices whose
installer maps no files for any choice now produces C#'s soft warning, once per registering
component. The installer maps nothing when it has no required files, no plugin files, and
no conditional pattern that the reference mapper can place. The warning reads
`<component>: FOMOD archive '<file>' is marked configured but has no matching
archive-scoped instructions. Its installer maps no files for any choice, so this
configuration installs nothing from the archive.` It is a counted report warning and a
`WARNING:` event; it never blocks. As in C#, there is no warning when only an unchosen
plugin installs files. Evidence: [archive-scoped proof](docs/parity/fomod-archive-scoped-proof/README.md),
37/37 checks (7 fail on `223ed92`), with a live reference oracle; contract in
[fomod-archive-scoped.md](docs/parity/fomod-archive-scoped.md).

These soft gate warnings, and the Rust-only no-route warning, now survive a later hard
failure of the same gate, such as an unselected dependency, a restriction conflict or a
FOMOD issue on a later component. As in C#, the failed FOMOD report stage lists the
warnings first and then the error. Unlike C#, which leaves `hasWarnings` false, adds
nothing to `warningCount` and counts every message of a failed stage as an error, Rust
sets `hasWarnings`, counts the warnings in `warningCount` and the error once in
`errorCount`. Plain `validate` and `install` emit the `WARNING:` events before failing.
The error text and exit code are unchanged. "Go to first issue" still points at the error.
Evidence: [gate-warnings proof](docs/parity/fomod-gate-warnings-proof/README.md),
55/55 checks (17 fail on `438bb73`); contract in
[fomod-gate-warnings.md](docs/parity/fomod-gate-warnings.md).

The gate no longer stops at its first FOMOD issue. As C# `FomodConfigurationGate` does,
it walks the whole scope and collects every warning and every issue. The issues are a
registered archive that is missing after prompt state was recorded, an archive that
cannot be inspected, and missing, unparseable or stale saved choices. Cancellation,
unsafe or invalid paths, guide errors and Extract source resolution still abort the gate.
An abort is listed after the issues collected before it. Issues with identical text are
listed once. Any issue fails the gate before Rust's stricter order and selection checks,
as before.
- **Report.** The failed FOMOD stage lists all warnings, then each issue as its own
  message in gate order. `errorCount` counts each issue.
- **Plain `validate` and `install`.** They emit every `WARNING:` event. A single issue
  keeps its unchanged error text. Several issues are numbered in one message:
  `FOMOD configuration check failed: <n> issues: [1] <first>; [2] <second>; ...`.
  Install prefixes it with `Installation blocked before any write: `.

The issue texts keep Rust's wording, without C#'s `ERROR: <component>:` prefix. Evidence:
[gate-collect proof](docs/parity/fomod-gate-collect-proof/README.md), 67/67 checks (22
fail on `d78aea2`); contract in [fomod-gate-collect.md](docs/parity/fomod-gate-collect.md).

## Delete, Copy, Move and Extract with an empty Source list — 2026-10-04 UTC

An instruction whose `Source` list is empty after loading (`Source = []`, only blank
entries such as `Source = [""]`, or no `Source` key) now behaves as in C#. Before
this change every such step failed with the untyped `Instruction has no Source`.

- **Delete, `Overwrite` false or omitted** (false for Delete in both loaders): a
  no-op, as C# `DeleteFile` returns `Success` with nothing to delete. Install logs
  `Delete has no Source; nothing to delete` and runs the later steps. Per CLI verb:
  `validate --output json --dry-run`, plain `validate`, `install` and `conflicts` all
  accept it, where they all rejected it before.
- **Delete, `Overwrite = true`**: the typed missing-source cause `Delete has no Source
  and Overwrite is true`, as C# returns `FileNotFoundPost` (`MissingSourceFiles`).
- **Copy, Move and Extract, any `Overwrite`**: the typed missing-source cause
  `<Action> has no Source`, as C# source resolution fails with `FileNotFoundPre`.
  Recovery (auto-extraction, subfolder variant, redundant Move) is not tried, and
  archive validation and the FOMOD gate skip an Extract that names no archive (until
  2026-10-05; see "An Extract with no Source in the validation reports" below).

For the two failing forms: `validate --output json --dry-run` now reports `Skipped
'<component>' during dry-run: source archives or files are not available.`
(`DryRunSkipped`) and passes, as C# does; before, the report had an Error. Plain
`validate` still exits non-zero and `install` is still refused before any write (C#
fails mid-install); `conflicts` still exits 1. The desktop runs the same engine.
Rename with no Source is still rejected everywhere, as in C#.
Guide authors: a Delete with no Source and `Overwrite = true`, including one inserted
by `guide-edit` without `Overwrite` (the model default is `true`), fails `validate`
and `install`. No guide that passed before can now fail; only empty non-strict
Deletes change from failing to passing. Evidence:
[empty-source proof](docs/parity/empty-source-proof/README.md), 51/51 checks against a
live reference oracle (26 fail on `fba4eb1`; 49 match / 2 known since
extract-destination; 86 match / 3 known of 89, with the live C# CLI, since
empty-extract-report); contract in
[empty-source-contract.md](docs/parity/empty-source-contract.md).

Exceptions: a scalar empty Source in XML or Markdown metadata is not an empty list (see
the next section). The report without `--dry-run` (also run by the desktop) now passes
an empty Extract, as C# component validation does. See the contract. (Superseded
2026-10-04: the CLI report without `--dry-run` now also simulates the steps, so it
refuses an empty Extract as plain `validate` does; the desktop report, which always
includes a dry run, is unchanged and still passes it; see "Step simulation in the validation report without a dry
run" below. Superseded 2026-10-05 for a component with no archive that exists: every
report, the desktop's included, fails an empty Extract there with `Missing Required
Archives: []`; see "An Extract with no Source in the validation reports" below.)

## Scalar empty Source in XML and Markdown — 2026-10-04 UTC

A scalar empty or whitespace Source, `<Source></Source>` in XML or `Source = ""` /
`Source: ''` in a Markdown `<!--<<ModSync>>` metadata block, is now kept as `[""]`
and refused, as TOML, JSON and YAML guides already were. Before, the XML reader and the
second Markdown metadata pass turned it into an empty list, so it took the empty-list
behaviour of the previous section. C# `Serializer.PrefixPath` throws on the empty path:
the XML guide fails to load, and the Markdown parser falls back to its legacy reader,
which drops the whole metadata block.

- Per CLI verb: `validate --output json --dry-run` reports an Error (`Path must start
  with <<modDirectory>> or <<kotorDirectory>>:`), plain `validate` exits non-zero and
  `install` is refused before any write. The desktop runs the same engine. `convert`
  and `list` still load the guide, as for TOML.
- Saving keeps the scalar: a single blank Source is written as `Source = ""`
  (TOML), `"Source": ""` (JSON), `Source: ''` (YAML, Markdown) or `<Source></Source>`
  (XML), so a non-Choose guide converted to any format is still refused. Before, the converted
  guide read back as an empty list and a non-strict Delete then installed as a no-op.
- A self-closing `<Source />` is null in C# and loads as an empty list; Rust now reads
  it the same way, so it keeps the empty-list behaviour. `[]`, `[""]`, a missing key
  and a stray component-level `<Source></Source>` are unchanged.
- Choose is exempt, as in C#, which does not path-normalize a Choose Source (see the
  next section): a Choose with a scalar empty Source is an empty no-op Choose in every
  format.
- A scalar-blank Source inside an option that is not selected loads and validates in
  Rust, since only selected instructions are validated; C# refuses the whole XML guide.
- Difference: for Markdown, C# loads the guide with the component's instructions (and
  Guid) silently dropped; Rust refuses the guide, as it refuses other metadata the
  reference's typed parser rejects.

Guide authors: an XML guide with an empty `<Source></Source>`, or a Markdown guide whose
metadata has `Source = ""` or `Source: ''`, used to pass in Rust. A non-strict Delete
written that way installed as a no-op, and a Copy, Move or Extract passed the dry-run
report with a `DryRunSkipped` warning. These guides are now refused. Replace the value
with a list (`Source = []`, or a self-closing `<Source />` in XML) for a deliberate
empty Source, or give the real path. Converting such a guide with `convert` does not
clear the problem: the output keeps the empty scalar and is refused too (C# `convert`
fails on the input). Evidence:
[scalar-empty-source proof](docs/parity/scalar-empty-source-proof/README.md), 99/99
checks against a live reference oracle (30 fail on `3911e74`); contract in
[scalar-empty-source-contract.md](docs/parity/scalar-empty-source-contract.md).

## Choose with a scalar Source — 2026-10-04 UTC

C# decides Choose on the raw `Action` text (OrdinalIgnoreCase), skips `PrefixPath` for
it, and reads a scalar Source by splitting it on `;` and `,`, trimming each part and
dropping blank ones. Rust now does the same in the shared loader for TOML, JSON, YAML,
XML and Markdown metadata:

- A Choose with `Source = ""`, `"  "`, `" , "` or `" ;, ; "` loads as an empty no-op
  Choose in every format. Before, a standalone TOML, JSON or YAML guide (and any
  format for `" , "`) kept a one-entry Source and was refused (`FOMOD configuration
  check failed: Unknown option`).
- A Choose scalar naming several options, such as `"<guid1>, <guid2>"` or
  `"<guid1>;<guid2>"`, loads as those options. Before, Rust kept one entry with the
  delimiter in it.
- Only the raw text counts: `Action = 9` (or `"9"`) with a blank Source is refused in
  every format, as a non-Choose blank scalar is. Before, Markdown read it as an empty
  Choose. C# refuses the TOML, JSON and XML guide and drops the Markdown metadata block
  (the documented Markdown difference above).
- Saving writes an empty Choose as an empty list (`Source = []`, `"Source": []`,
  `Source: []`, or no Source in XML and Markdown), which reads back as an empty Choose.
  Saving (via `convert`, merge, editor or desktop save) a guide whose action reads as
  Choose without being the raw text "Choose" (`9`, `"Choose "`, `"Choose, Unset"`)
  writes the canonical `Choose`, so the saved guide is an empty Choose; C# reads that
  saved form the same way.

Evidence: the [scalar-empty-source proof](docs/parity/scalar-empty-source-proof/README.md)
now has 45 cases and 190 checks (235 with the live reference loads), all matching; 73
fail on `1b3faeb`. Contract:
[scalar-empty-source-contract.md](docs/parity/scalar-empty-source-contract.md).

## Choose option matching — 2026-10-04 UTC

C# runs a selected option when its `Guid.ToString()`, the lowercase hyphenated form,
is an entry of the Choose Source, ignoring case (`Instruction.GetChosenOptions`). Install
and the dry run both use this rule. Rust compared entries as parsed GUID values, so a
braced `{guid}`, `(guid)`, a 32-digit form without dashes, or a padded `" guid "`
entry ran the option, which C# skips. Rust now uses the C# text rule in install,
`validate` and the dry-run report:

- An entry names an option only when it is the option's GUID with dashes and no braces,
  in any case. The option's own `Guid` may still be written in any GUID form.
- An entry that names no option is refused before any write (`FOMOD configuration
  check failed: Unknown option ...`), as an unknown GUID already was. C# ignores such
  an entry and installs the rest. This fail-closed difference is deliberate and
  unchanged. When the entry is another spelling of a real option, the message gives
  the spelling to use.
- Legacy Markdown bullets split a Choose Source only on `,`, in both implementations,
  so `<g1>;<g2>` is a single entry that names no option and is refused. No change.

- `convert --auto-generate-local` and the other archive generators write each option
  into a Choose as its hyphenated lowercase GUID, as C# does, even when the authored
  option `Guid` is braced, upper case or without dashes. The desktop editor checks
  Choose references by the same rule as install.

Guide authors: a hand-written Choose entry written `{guid}`, `(guid)`, without dashes
or with surrounding spaces used to install that option in Rust. C# never installs it.
Such guides are now refused with `Unknown option {guid}: a Choose entry names an
option only as guid`. Write the entry as the plain hyphenated GUID (any case) to
install the option. The option's own `Guid` may keep any GUID spelling. Guides
produced by `convert --auto-generate-local` already use the plain form. In a legacy
Markdown bullet, separate several options with `,`, not `;`. In a guide without
FOMOD, the refusal is still reported under the `FOMOD configuration check failed:`
prefix of the shared pre-install check. Evidence:
[choose-option-match proof](docs/parity/choose-option-match-proof/README.md), 69/69
checks (92/92 with the live reference runs); 21 fail on `a09ca4a`, and 6 on `5eb80d5`,
whose generator wrote the authored spelling. Contract:
[choose-option-match-contract.md](docs/parity/choose-option-match-contract.md).

## Delete of a missing file with Overwrite=true — 2026-10-04 UTC

A `Delete` instruction with `Overwrite = true` now fails when a literal source is
missing, or when its sources match nothing, as C# `Instruction.DeleteFile` does. That
method returns `FileNotFoundPost`, which fails the component with `MissingSourceFiles`
on install and becomes a `DryRunSkipped` warning in C# dry-run validation. The engine
deletes the present matches, then raises the same typed missing-source cause a missing
Copy/Move/Extract source raises (`Source matches no files: <source>`). Per CLI verb:
`validate --output json --dry-run` reports `Skipped '<component>' during dry-run: source
archives or files are not available.` and passes, as C# does; plain `validate` exits
non-zero, as for any missing source; `install` is refused by its validation before any
write (C# fails mid-install after deleting the present paths); `conflicts` exits 1
because simulation errors abort its analysis. The desktop runs the same engine. Before
this change, Rust ignored `Overwrite` on Delete and passed all of them, and install ran
the later steps. `Overwrite = false` and an omitted `Overwrite` (false for Delete in
both loaders) still skip missing files, and a wildcard miss beside a resolved path is
still not an error. Evidence: [delete-strict proof](docs/parity/delete-strict-proof/README.md),
39/39 checks against a live reference oracle (15 fail on `8fad43c`); contract in
[delete-strict-contract.md](docs/parity/delete-strict-contract.md).


Guide authors: a cleanup `Delete` with `Overwrite = true` on an optional file that may be
absent now fails, as in C#. Omit `Overwrite` (the loaders default it to `false` for Delete)
or set it to `false`. `guide-edit` inserts default `Overwrite` to `true`.

## Rename onto an existing file — 2026-10-04 UTC

A `Rename` instruction with `Overwrite = false` whose target file already exists now
fails, as C# `Instruction.RenameFile` does. That method returns `RenameTargetAlreadyExists`,
and the component fails with `UnknownError` on install and in dry-run validation. The
engine skips each blocked source, renames the remaining sources, and then fails with
`Rename target already exists and overwrite is false: <target>`. Plain `validate` exits
non-zero, the report's `DryRun` stage carries the error, and `conflicts` exits 1 because
simulation errors abort its analysis. `install` is refused by its
validation before any write; C# instead fails mid-install after the earlier steps. Before
this change, Rust warned, skipped and passed, leaving the source un-renamed for later
steps. `Overwrite = true` (the default) and free targets are unchanged. `Copy` and `Move`
with `Overwrite = false` still warn and skip, as in C#. Evidence:
[rename-overwrite proof](docs/parity/rename-overwrite-proof/README.md), 18/18 checks
against a live reference oracle (8 fail on `5643276`); contract in
[rename-overwrite-contract.md](docs/parity/rename-overwrite-contract.md).

## Extract onto existing files with Overwrite=false — 2026-10-04 UTC

An `Extract` instruction now replaces files that already exist at its entry paths,
whatever its `Overwrite` value, as C# does. C# `Instruction.ExtractFileAsync` never
reads `Overwrite`, and its real file-system provider writes every entry with
`FileMode.Create`, so an existing file is replaced and, when two archives supply one
path, the last one wins. Before this change Rust honoured `Overwrite = false` on
Extract and silently kept the existing file: stale output from an earlier install, a
game file in `Override`, or the first archive's copy. Install, validate, the dry-run
report and `conflicts` run the same engine, so later simulated steps see the replaced
content. Unchanged: a configured FOMOD archive with `Overwrite = false` is still
refused when any of its paths exists (the provenance guard above, a deliberate
fail-closed choice); Copy and Move still warn and skip, and Rename still fails, with
`Overwrite = false`. Evidence: [extract-overwrite proof](docs/parity/extract-overwrite-proof/README.md),
20/20 checks against a live reference oracle (5 fail on `5905259`; since
extract-destination 4 match / 16 known, the cases with a Destination compared with the
live C# CLI instead, its `stale_*` cases refused since loose-source; 14 match / 16 known
with five Destination-free `same_stem_*` cases since source-order); contract in
[extract-overwrite-contract.md](docs/parity/extract-overwrite-contract.md). The
reference dry run nests every extraction under an archive-named folder, unlike its own
install; that inconsistency is recorded there and not copied.

Guide authors: `Overwrite = false` on an `Extract` step does not protect existing
files, in C# or in Rust. To keep a file, extract into a separate folder and `Copy` it
with `Overwrite = false`, or order the steps so the wanted file is written last.

Known differences: when an entry path holds an existing directory, C# skips that
entry with a warning and succeeds, while Rust fails the step. When it holds an
existing read-only file, C# keeps it, while Rust replaces it. Both were already
Rust's behaviour with `Overwrite = true`. See the contract.

## Copy/Move onto itself — 2026-10-04 UTC

A `Copy` or `Move` whose target `<Destination>/<source filename>` is the source file
itself (same path under the path key: case-folded by default, exact when
`case_insensitive_pathing` is off) is now handled as C# handles
it when `Overwrite = false`: the step records the usual `Destination file already
exists and overwrite is false` warning, skips that file, handles the other sources and
goes on. C# `Instruction.CopyFileAsync`/`MoveFileAsync` find the target present, log
the skip and return `Success`. Before this change Rust refused the step with `Source
equals destination`, so plain `validate` exited non-zero and `install` was refused, for
guides C# installs. Typical triggers are a wildcard copied back into its own folder
(`Override/*.tga -> Override`) or a Source list that names a file already in the
destination. Unchanged: with `Overwrite = true` or omitted (the default is `true`),
Rust still refuses before any write; C# also fails, but only after deleting the file.
Rename onto its own name still fails, as in C#. A symlink in an install tree is still
refused, where C# skips it. Evidence: [copy-move-self proof](docs/parity/copy-move-self-proof/README.md),
30 checks match and 2 are known symlink differences, against a live reference oracle
(20 fail on `354055b`); contract in
[copy-move-self-contract.md](docs/parity/copy-move-self-contract.md).

Follow-up (2026-10-04): the same rule now applies when another Source of the step is
recovered from a `ResourceRegistry` archive; recovery qualification refused the
self-target before. And `install` now emits one ordinary event per file that a
Copy/Move skips with `Overwrite = false` (self-target or any other existing target),
`WARNING: Destination file already exists and overwrite is false: <path>`, as C#
logs one warning per skipped file. It is a stdout line in plain and `--json`
output, a stderr line with `--install-progress-json` (typed stdout unchanged), and a
line in the desktop Activity log. Exit codes, results and files are unchanged;
validate, dry run and `conflicts` keep their report warning and emit no event.
Evidence: the extended proof, 51 checks match and 4 are known differences (the
symlink case and the recovered trees, where Rust also extracts the other source)
against a live reference oracle; 17 fail on `6b8baff`.

Guide authors: copying or moving a file onto itself does nothing useful. With
`Overwrite = false` it is skipped, in C# and in Rust. With `Overwrite = true` or no
`Overwrite`, C# deletes the file and fails the mod; Rust refuses the guide. Point
`Destination` at another folder, or narrow the Source so it does not match files
already there.

## An install root as Destination — 2026-10-04 UTC

A `Copy`, `Move`, `CleanList` or `DelDuplicate` step may name an install root itself
as its Destination: `<<kotorDirectory>>`, `<<gameDirectory>>` or `<<modDirectory>>`,
with or without a trailing `/` or `\`. C# trims the trailing separator and uses the
root. Rust resolved such a Destination with a trailing separator and did not find the
root folder. So the archive/destination stage of validation (the desktop Validate
step and `validate --output json`) failed with `Destination cannot be found:
<<kotorDirectory>>` for every such step, and a `DelDuplicate` into a root failed the
dry run, plain `validate` and `install`. Copy, Move and CleanList installs were
already correct. Rust now resolves a bare root to the root folder, so these pass and
DelDuplicate deletes the duplicate in the root, as in C#. An `Extract` of an archive
at the mod root with Destination `<<modDirectory>>` now extracts into
`mods/<archive name>/`, as C# does; before, Rust extracted straight into the mod root.
A Patcher with Destination `<<gameDirectory>>` now passes Rust's game-directory check.

A bare root as a Source (`Extract [<<modDirectory>>]`, `Copy [<<kotorDirectory>>]`,
`Delete [<<modDirectory>>]`) still names no file and reaches nothing beneath the root,
as in C#: Extract, Copy and Move fail with missing source files, Delete deletes nothing.

A Destination with a `.` part, such as `<<kotorDirectory>>/Override/.`, now fails the
archive/destination stage, as C# `PathValidator.IsValidPath` does; before, Rust passed
it. Copy and Move into it installed in both (until the destination-path-parts change
below). DelDuplicate and CleanList into it
now fail the dry run, plain `validate` and `install`, as in C#; before, Rust ran them
(a CleanList into `<<kotorDirectory>>/.` deleted the listed game-root files). Since
the destination-path-parts change below, plain `validate` and `install` also refuse a
Copy or Move into it, as the C# CLI `install` (which validates first) does; this
proof's install reference, `ModComponent.InstallAsync` alone, still installs it.
Unchanged: a `Rename` whose Destination
is a bare placeholder (`Rename <<kotorDirectory>>/r.txt -> <<kotorDirectory>>`) is
refused in Rust; C# renames the file to one literally named `<<kotorDirectory>>`
(fail-closed, deliberate). Evidence: [root-destination
proof](docs/parity/root-destination-proof/README.md), 119 checks match and 4 are
known differences against a live reference oracle (45 fail on `8c79761`; 113 / 10
since destination-path-parts, 114 / 9 since extract-destination); contract in
[root-destination-contract.md](docs/parity/root-destination-contract.md).

Guide authors: `<<kotorDirectory>>` on its own is a valid Destination for Copy, Move,
CleanList and DelDuplicate, for example to copy `dialog.tlk` into the game folder. A
bare root is never a valid Source (Patcher's direct binding still accepts one; see the contract); name the files (`<<modDirectory>>/*.zip`). Do not
use a `.` part in a Destination; validation refuses it, and DelDuplicate and CleanList
fail. A Rename Destination is a file name, never a placeholder.

## CleanList rows and listed names — 2026-10-04 UTC

A CleanList row that lists a name no file can have now skips that name, as C# does:
C# deletes `Path.Combine(target, name)` only when `File.Exists` is true, and that is
false for a quoted field (`"a.tga"`), a wildcard (`*.tga`, `?.tga`) or a name with
`<`, `>`, `|` or `:`. Rust parsed every listed name as a guide path and failed the
step, so the Validate step reported an error and install refused. CSV quoting is the
common trigger: `"Mod, by Author",a.tga` splits into `"Mod` and ` by Author"`, and
the second field ended the install. `[` and `]` in a listed name are now literal
(`c[1].tga` deletes that file, as in C#). Row grammar, row matching, comments and
blank rows already matched.

Unchanged and refused before anything is deleted, for a name that is not already skipped as unnameable: `..` parts and absolute names (C#
deletes outside the target), drive-qualified names (`a:b.tga`, `C:\x`), trailing dots
and Windows device names (their meaning depends on the platform). A backslash is a
separator and lookups ignore case on every host, as C# on Windows; C# on Linux skips
`sub\x.tga` and `A.TGA` (platform accidents, not copied). Evidence:
[cleanlist-rows proof](docs/parity/cleanlist-rows-proof/README.md), 121 checks match
and 20 are known differences against a live reference oracle (30 fail on `f9eab15`);
contract in [cleanlist-rows-contract.md](docs/parity/cleanlist-rows-contract.md).

Guide authors: a CleanList list is plain comma-separated text. Quotes are part of a
field, so a mod name or file name cannot contain a comma; a quoted file name is never
deleted. List files relative to the Destination, with `/`, and without `..`, drive
letters, absolute paths or trailing dots.

## Archive members with brackets in their names — 2026-10-04 UTC

An archive whose member names contain `[` or `]` (`Override/x[1].tga`, `readme
[v1.2].txt`, a folder `[HD] textures/`) now installs, as in C#. Rust admitted member
names with the same rule as literal guide paths and refused `[`/`]` there, so one such
member failed the whole archive: plain `validate` and `install` stopped at the FOMOD
configuration check, the Validate step's archive/destination stage reported `Cannot
inspect archive`, and archive verification after a download or peer transfer refused it
(by code reading; not probed).
C# extraction (`PathHelper.TryGetZipSafeArchiveEntryExtractPath`) keeps every
character except rooted names and `..` parts. Rust now takes member names literally:
`[` and `]` are ordinary characters, and later Sources already treat them as literal,
so `Copy <<modDirectory>>/a/Override/x[1].tga` reaches that file and not `x1.tga`.

Component checkpoints (`install --component-checkpoints` and the desktop option) now
also accept a game or mod file whose name holds `[` or `]`; before, checkpoint capture
refused such a tree even with no archive involved.

A managed install (`--managed`) whose later component runs a program now captures a
game tree holding a bracketed file; on the first fix (`4a09589`) that step failed after
the earlier component had deployed it. A FOMOD archive whose own name, wrapper folder
or ModuleConfig `source` holds `[` or `]` now maps and installs, as the reference
mapper does; before, the gate reported the saved choices as stale.

Unchanged: a member name with `*` or `?` fails the archive in Rust on every host
(no Windows file can have it; C# on Linux writes it), and the other unsafe member
rules stay fail-closed. Destinations (including a FOMOD ModuleConfig `destination`,
whose error now names it), Rename Destinations and maintenance transactions still
refuse `[`/`]`. A Patcher Source no longer does; see the next entry.
Evidence: [archive member brackets
proof](docs/parity/archive-member-brackets-proof/README.md), 83 checks match and 8 are
known differences against a live reference oracle and mapper (9 fail on `4a09589`; 45
of the first 68 on `31e98ec`); contract in
[archive-member-brackets-contract.md](docs/parity/archive-member-brackets-contract.md).

Guide authors: archive member names may contain `[` and `]`. Name such a file in a
Source exactly as it is spelled; brackets never form a character class. A FOMOD's
archive name, wrapper folder and ModuleConfig `source` may contain them; a
ModuleConfig `destination` may not. Archives with `*` or `?` in a member name are
refused.

## Patcher Sources with brackets in their path — 2026-10-16 UTC

A Patcher whose Source path holds `[` or `]` (an archive folder such as `[Example]
Texture Pack A/`) now binds and runs. A Patcher Source names one literal file or
folder, but Rust resolved it with the guide Destination rule, which refuses `[`/`]`
(they are not a pattern here, only characters that rule did not admit). Install
stopped with `Patcher source must bind one literal file or directory`, and local
archive generation skipped such a component rather than write a step install could not
run. The Source now resolves with the literal-name rule archive members use, so
`<<modDirectory>>/pack/[Example] Texture Pack A` reaches that folder and never a
sibling such as `E Texture Pack A`; generation emits the step.

Unchanged: a Patcher Source with `*` or `?` is refused (it must bind exactly one
path), and Destinations, Rename Destinations and maintenance transactions still refuse
`[`/`]`. Covered by `tests/installer.rs` and `tests/archive_generation.rs`; the full
installer suite still has 11 failures that also fail without this change.

## Execute and Run Arguments — 2026-10-04 UTC

An `Execute` or `Run` step's `Arguments` text is now split into the program's
arguments with the Windows command-line rules on every host, as in C#: .NET splits
`ProcessStartInfo.Arguments` with those rules on Linux and macOS too, and a Windows
program's C runtime (UCRT and post-2008 CRTs) uses the same rules. Only spaces and tabs separate arguments,
double quotes group, `""` inside quotes is a literal quote, and a backslash is literal
unless it comes before a `"`. Before, Rust used POSIX shell rules on Linux and macOS,
so the same guide ran its program with other arguments there:
`<<kotorDirectory>>\Override` arrived as `<game>Override`, `sub\file.txt` as
`subfile.txt`, `'a b'` as one argument, `x #y z` as `x` alone, and a newline separated
arguments. Validation did not show this; only the program's behaviour differed. No
shell runs, before or after.

Unchanged and documented: an unclosed double quote (including `"...\Override\"`,
where `\"` escapes the quote) and an unknown `<<name>>` are refused before any
program runs, where C# runs the program; placeholders match case-insensitively (C# is
case-sensitive); and a root folder with spaces stays one argument (C# substitutes
before splitting and passes two). The Unix standalone HoloPatcher `--args` text keeps
shell-style splitting.
Evidence: [execute arguments proof](docs/parity/execute-arguments-proof/README.md),
89 checks match and 21 are known differences against a live reference oracle (21 fail
on `0cc50a7`); contract in
[execute-arguments-contract.md](docs/parity/execute-arguments-contract.md).

Guide authors: write Arguments as on a Windows command line; they mean the same on
every host. Keep backslashes as they are, group with double quotes, and treat single
quotes and `#` as ordinary characters. Do not end a quoted argument with one backslash
(`"<<kotorDirectory>>\Override\"`); drop the trailing backslash or double it. Spell
placeholders exactly as `<<modDirectory>>`, `<<kotorDirectory>>` or `<<gameDirectory>>`.

## Patcher INI names with brackets — 2026-10-04 UTC

The virtual Patcher simulation (used by validation, the dry-run report, `conflicts` and
install preflight) now treats `[` and `]` in a patcher's own INI names as ordinary
characters: the `[InstallList]` folders, the `File*`/`Replace*` file names, and the
namespaces.ini `IniName` and `DataPath` of the selected namespace. This matches C#,
whose simulation joins these names with `Path.Combine`. Before, Rust read them as
wildcards and refused the guide with `Wildcards are not allowed here: [HD]tex.tga`, so
a guide whose mod ships a file such as `[HD]tex.tga` or a namespace folder such as
`[A] Option` could not be validated, analysed for conflicts or installed. Conflict
reports now list such paths (`<<kotorDirectory>>/Override/[HD]tex.tga`) with the same
writers and winner as C#.

Unchanged and documented: INI names with `*` or `?` are still refused (no Windows file
can hold them; C# on Linux simulates them), as are `..`, rooted names and the other
unsafe names. A named-INI `Arguments` value may still not contain brackets. Evidence:
[patcher INI brackets proof](docs/parity/patcher-ini-brackets-proof/README.md), 39
checks match and 6 are known differences against a live reference oracle (33 fail on
`c2225e3`); contract in
[patcher-ini-brackets-contract.md](docs/parity/patcher-ini-brackets-contract.md).

Users: a guide that failed with `Wildcards are not allowed here` naming a file or
folder from a patcher's changes.ini or namespaces.ini now validates and installs; no
guide change is needed.

## Install order and reference checks — 2026-10-04 UTC

`install` now runs the selected components in the order the C# CLI does
(`InstallCoordinator.GetOrderedInstallList`): starting from the order the C# loader
gives a guide, every component, selected or not, is sorted breadth-first by
`Dependencies`, `InstallAfter` and `InstallBefore`, so a component waits for its whole
layer. For `A`, `B` (InstallAfter `A`), `C`, both now install `A, C, B`; Rust used to
install `A, B, C`, taking the first ready component in file order and ignoring
unselected components. Widescreen-only components take their breadth-first place too,
as in the C# CLI. A Markdown guide (by extension, or by content for `.txt`, other
extensions and no extension) orders by `Dependencies` alone from file order, because
the C# Markdown reader drops `InstallBefore` and `InstallAfter`; those fields neither
order nor stop a Markdown install. `--direct-markdown` keeps the guide's source order,
as C# does. The validate dry run, checkpoints and the desktop use the same order.

`validate` (with or without roots), `install` and `conflicts` now also run C#'s
reference check when they load a non-Markdown guide: a missing, self or circular
`InstallBefore`/`InstallAfter` reference on any component, selected or not, stops the
run before the report or any game write with `Dependency resolution failed with N
errors. Fix these InstallBefore/InstallAfter references in the guide (validate and
install do not take --ignore-errors):` and one line per error (`error` in `--output
json`), exit 1. Otherwise the report lists the components in the resolver's order, as
C#'s does. Before, Rust checked references only on selected components, inside the
report.

New for phased installs: `--phase base` and `--phase remaining` keep selected
widescreen-only components after the base build, because Remaining continues the base
prefix. When a selected base component must follow a selected widescreen-only one
(directly or through unselected components), `--phase base` now refuses before any
write with `The base build cannot be installed first: a base component must install
after widescreen-only <W> or a later widescreen-only component. Install with --phase
all.` (`<W>` is the first selected widescreen-only component in the install order);
before, Base installed and Remaining then refused.

Unchanged and documented: `--ignore-errors` stays refused on validate and install. A
selected component whose Dependencies are not selected, a Restrictions conflict and a
cycle through Dependencies are still refused; C# installs those components (checked
live). Rust's dry run simulates the install order; C# simulates the list order. With
phases, the widescreen-only components install in breadth-first order; the C# wizard's
widescreen page uses list order. Evidence: [install order
proof](docs/parity/install-order-proof/README.md), 161 checks match and 1 is a known
difference against the live C# CLI (85 fail on `e4c0b4d`, 23 on the first version of
this change, `174482a`); contract in
[install-order-contract.md](docs/parity/install-order-contract.md).

Users: when one mod's `InstallAfter`, `InstallBefore` or `Dependencies` holds it back,
mods with no such link that come later in the guide may now install before it, as in
C#, so a file both write can end up from a different mod than with the previous
version. In a Markdown guide only `Dependencies` order the install. If validate or
install now stops with `Dependency resolution failed`, fix or remove the named
reference, even on a component you have not selected. If `--phase base` now refuses,
install with `--phase all`. A phased or checkpointed install started with an older
version may refuse to resume because the order changed; restore and start it again.

## Component order in convert — 2026-10-04 UTC

`convert` now writes a guide's components in the order the C# converter does: each
component after every component it names in `InstallAfter` and before every component
that names it in `InstallBefore`, otherwise in guide order, using C#'s depth-first
ordering (`DependencyResolverService`). Every top-level component takes part, selected
or not; options keep their place. A guide whose `InstallBefore` or `InstallAfter`
names a missing component, the component itself, or forms a cycle is refused with
`Dependency resolution failed with N errors. Use --ignore-errors flag to attempt
loading in best possible order.` and one line per error (exit 1, nothing written), as
in C#. Before, Rust wrote the components in file order and converted such guides
without a message. `convert --ignore-errors` is now accepted: it orders around missing
references, keeps them in the output and prints C#'s warning with the errors to
stderr. A Markdown input (by extension, or by content when the extension names no
format or with `--stdin`) keeps its file order and is never refused, as in C#, whose
Markdown reader ignores `InstallBefore` and `InstallAfter`; Rust still writes the
references it read.

Unchanged and documented: with `--ignore-errors`, a cycle is still refused; C# writes
a guide with no components there. `merge` keeps its own order and does not take
`--ignore-errors`. (Validate, install and `conflicts` now run the same check; see
"Install order and reference checks" above.) References keep their authored
spelling. A reference that is not a GUID (or is empty) is refused as missing; C# drops that component's whole list and
converts. Evidence: [convert
dependency order proof](docs/parity/convert-dependency-order-proof/README.md), 126
checks match and 14 are known differences against the live C# CLI (57 fail on
`24897e6`); contract in
[convert-dependency-order-contract.md](docs/parity/convert-dependency-order-contract.md).

Users: a converted TOML, JSON, YAML or XML guide that uses `InstallBefore` or
`InstallAfter` now lists its components in install order, as the C# converter writes
it; a Markdown guide converts in its own order, as before. If `convert` now stops with
`Dependency resolution failed`, fix the named reference, or pass `--ignore-errors` to
convert anyway when the only problem is a missing component.

## File selection when the file selects nothing — 2026-10-04 UTC

`validate` and `install` with `--use-file-selection` now treat a guide that marks no
component `IsSelected` (or has no `IsSelected` keys) as C# does: every component is
selected, because the C# file loader selects them all when the file selects none.
Before, Rust refused: validate printed `No components are marked IsSelected in the
instruction file.` (exit 1) and install stopped with `FOMOD configuration check failed:
Select at least one component`. This covers every non-Markdown guide (TOML, JSON and
YAML were compared; XML takes the same loader path) and the report without roots;
`conflicts` follows validate. A Markdown guide keeps its flags,
as in C#, so an all-unselected Markdown guide is still refused. Runs with `--profile` or
`--active-profile` are unchanged, and so are runs without the flag, which already
selected every component. Evidence: [file selection fallback
proof](docs/parity/file-selection-fallback-proof/README.md), 58 checks match the live
C# CLI (30 fail on `a0df4b7`); contract in
[file-selection-fallback-contract.md](docs/parity/file-selection-fallback-contract.md).

Users: `--use-file-selection` on a guide saved with nothing selected now validates or
installs the whole guide, as the C# CLI does, instead of stopping. Select at least one
component in the file to limit the run to it. Additive flags such as `--option` or
`--add-category` on such a guide now add to the whole guide, as they do without
`--use-file-selection`.

## Missing destination folders in the validation report — 2026-10-04 UTC

The validation report's archive stage (`ComponentValidation` in `validate --output
json`, the mod-archives card of the desktop Validate step) now reports a `Copy`,
`Move`, `CleanList` or `DelDuplicate` step whose Destination folder does not exist as
C# does: `ERROR: <component>: Destination cannot be found! Got '<full path>'`, one
error per such step, each counted in `errorCount` and in the stage summary. The path
is the folder's full path, with the folders that exist in their on-disk spelling; an
empty Destination gives `Got ''`. Before, Rust stopped at the first such step of a
component and printed `archive/destination validation: Destination cannot be found:
<Destination as written>`, so `errorCount` was lower than in C#. The report's decision
is unchanged, and plain `validate`, `install` and the dry run are unaffected.

Unchanged and documented: C# creates each missing folder while validating, so it names
a folder once and a second run passes; Rust validation never changes the game or mod
folders, so it names every step into a missing folder, every time. A Destination that
Rust refuses for another reason (a Rename Destination, a `..` part) still ends the
component's checks with one error; a Patcher Destination is reported per step since
2026-10-05 (see "A Patcher step's Destination"). Evidence: [missing destination
proof](docs/parity/missing-destination-proof/README.md), 75 checks match and 12 are
known differences against the live C# CLI (32 fail on `1e2d133`; 77 / 10 since the
Patcher Destination section, whose `missing_then_bad_patcher` case now matches);
contract in [missing-destination-contract.md](docs/parity/missing-destination-contract.md).

Users: the Validate step now lists each step whose folder is missing, with the full
path it looked for, so you can create the folders or correct the game or mod
directory in one pass.

## Missing archives in the validation report — 2026-10-04 UTC

The validation report's archive stage (`ComponentValidation` in `validate --output
json`, the mod-archives card of the desktop Validate step) now reports a missing
download as C# does: one error per Extract step, `ERROR: <component>: Missing
Required Archives: [<that step's Sources, joined by ",">]`, each counted in
`errorCount` and in the stage summary, and the component's other archive checks are
skipped. When a component has no archive at all, only its own Extract steps are named,
not its options'. Before, Rust stopped at the first missing archive of a component
and printed `archive/destination validation: Source matches no files: <Source>`, so a
component with two missing downloads showed one, and `errorCount` was lower than in
C#. The report's decision is unchanged, and plain `validate`, `install` and the dry
run are unaffected.

Unchanged and documented: Rust still reports a missing archive when the component's
other archives exist, or when the Extract is in an option; C# passes the stage then,
and its install fails at that Extract. Rust does not report an Extract that does not
run (an unmet instruction dependency); C# does when the component has no archive.
(An Extract with no Source is reported as in C# since 2026-10-05; see "An Extract
with no Source in the validation reports" below.) Other archive-stage failures keep
their Rust wording. Evidence: [missing archives
proof](docs/parity/missing-archives-proof/README.md), 47 checks match and 22 are known
differences against the live C# CLI (27 fail on `3024e4d`; 50 match / 19 known since
2026-10-05); contract in
[missing-archives-contract.md](docs/parity/missing-archives-contract.md).

Users: the Validate step now lists every missing download of a component, each named
by its guide Source, so one Fetch Downloads pass can cover them all.

## A failing chosen option in the validation dry run — 2026-10-04 UTC

When an option that a `Choose` step runs is missing a source (a Copy, Move, Extract,
Rename or CleanList file, the file of a Delete with `Overwrite = true`, or the program
of an Execute/Run whose Sources are all wildcards), the
validation dry run (`validate --dry-run`, the desktop Validate step) now fails that
component, as C# does: the `DryRun` stage reports `ERROR: [EngineSimulation]
<component> / step <n> (Choose): <option> / step <m> (<action>): <cause>`, it is
counted in `errorCount`, and `validate` exits 1. C# maps any failure of a chosen
option to `OptionalInstallFailed` and then `UserCancelledInstall`, which its dry run
reports as `Dry-run failed for '<component>' with exit code UserCancelledInstall.`
Before, Rust reported the component as `DryRunSkipped` (a warning) and the report
passed, although its install refused the same selection. The component's earlier
warnings now stay in the report, as C# keeps its issues for a failed component; a
Delete with `Overwrite = true` that matches nothing no longer adds an `Attempting to
delete non-existent file` warning before it fails, which C# does not report either.

Unchanged: a missing source in a component's own steps is still the `DryRunSkipped`
warning in both; plain `validate` and `install` refused these guides already and are
unaffected; options that are not selected or not named by a `Choose` are not run. The
message keeps the Rust wording. An option's own `Dependencies` and `Restrictions` are
still checked (C# ignores them when a `Choose` runs the option), and an option flagged
selected under an unselected component still counts as unselected for instruction
conditions (C# counts it). Evidence: [option failure
proof](docs/parity/option-failure-proof/README.md), 67 checks match and 1 is a known difference against the live C# CLI (37 fail on
`b89484d`); contract in
[option-failure-contract.md](docs/parity/option-failure-contract.md).

Users: if Validate now fails with `<component> / step <n> (Choose)`, a file of an
option you selected is missing, often a separate download. Fetch or place it, or
deselect that option, then validate again.

## A missing patcher in the validation dry run — 2026-10-04 UTC

When a component's own `Patcher` step names a patcher that is not in the mod folder
(the patcher folder, its `tslpatchdata` folder or its executable), the validation dry
run (`validate --dry-run`, the desktop Validate step) now reports `WARNING:
[DryRunSkipped] Skipped '<component>' during dry-run: source archives or files are not
available.` and passes, as C# does and as Rust already did for a missing Copy, Move or
Extract source. C# returns `FileNotFoundPre` for the miss, which its dry run reports
as a skipped component. Before, Rust failed the report with `ERROR: [EngineSimulation]
<component> / step <n> (Patcher): Missing external program/mod: <Source>` and
`validate --dry-run` exited 1. The component's earlier warnings are dropped with the
skip, as for the other skips.

Unchanged: `install`, the desktop install and plain text-mode `validate` still refuse
with `Missing external program/mod: <Source>` (the JSON report without `--dry-run`
never simulates Patcher steps and reports success, as before; superseded 2026-10-04,
it now refuses too, see the next section); a Patcher miss in a chosen option still fails
its component; a present patcher is simulated as before. A Patcher Source that is a
wildcard or an empty list is still refused (C# skips the component), and a Source
naming a present patcher folder is still bound and simulated (C# finds no file there
and skips); Rust binds a patcher to one literal file or folder before anything runs.
Evidence: [patcher missing source
proof](docs/parity/patcher-missing-source-proof/README.md), 58 checks match and 11 are
known differences against the live C# CLI (32 fail on `0f71baa`); contract in
[patcher-missing-source-contract.md](docs/parity/patcher-missing-source-contract.md).

Users: Validate now warns instead of failing when a patcher mod has not been
downloaded yet. The install still needs it: fetch or place the patcher, then validate
again before installing.

## Step simulation in the validation report without a dry run — 2026-10-04 UTC

`validate` with both roots and a report flag but no `--dry-run` (`--output json`,
`--output text`, `--validation-report`, `--errors-only`, `--full`) now decides as plain
`validate` does. Once its stages pass, it simulates every step in install order on a
virtual copy, as plain `validate` does, and the first failure fails the
`ComponentValidation` stage: that component's `OK:` line becomes `ERROR: <component> /
step <n> (<Action>): <message>`, the wording plain `validate` prints, with one more
error, `success: false` and exit 1. Before, the report ran only its archive and FOMOD
stages, so it passed (exit 0) a guide whose Copy, Move, Patcher, strict Delete,
Rename or CleanList source was missing, whose Execute or Run step lacked
`--allow-processes`, whose chosen option missed a source, or that had a component
without instructions, while plain `validate` and `install` refused it. The pinned C#
CLI never passes these guides either: without a dry run or `--full` it never sets the
mod folder, so its FOMOD stage fails every guide, and a Patcher guide stops earlier
with `'path' cannot be null or whitespace`.

Unchanged: a report that already has an error is not simulated, so archive,
destination, FOMOD and `--full` stage errors are reported and counted as before. The
dry-run report (`--dry-run`, `--dry-run-only`, the desktop Validate step,
`--phase remaining`) still reports a missing source as `DryRunSkipped`. Validation
without both roots, plain `validate`, `install` and `conflicts` are unchanged. Rust
passes a guide whose steps all simulate, where the C# basic report fails it for want of
a mod folder (a C# accident); with `--full` and a packaged HoloPatcher C# would pass a
missing loose source, which Rust refuses (fail-closed). Evidence: [plain report
simulation proof](docs/parity/plain-report-simulation-proof/README.md), 63 checks match
and 6 are known differences against the live C# CLI (48 fail on `fd9ec46`); contract in
[plain-report-simulation-contract.md](docs/parity/plain-report-simulation-contract.md).

Users: if `validate --output json` (or `--output text`, `--validation-report`,
`--full`) now fails with `ERROR: <component> / step <n> ...`, plain `validate` refuses
the guide for the same reason and `install` would stop there: fetch or place the
missing file (or pass `--allow-processes` for a trusted Execute step), then validate
again. Use `--dry-run` to see every component with a missing download at once.

## Missing destination folders in the full validation pipeline — 2026-10-04 UTC

The full validation pipeline (`validate --full`, `validate --full --dry-run`, the
desktop Validate step, the `--phase remaining` report) no longer fails a `Copy`, `Move`
or `CleanList` step whose Destination folder does not exist yet but can be created (no
`.` part, no file in the way). C# runs the same checks in that pipeline's Environment
stage first, with AttemptFixes on by default, and creates each such folder there, so
its archive stage prints `OK: <component>` and its `install`, which runs this pipeline
unless `--skip-validation` is given, installs the files. Rust creates nothing while
validating: its dry run and step simulation create the folder virtually, and install
creates it. Before, Rust's full report failed with `Destination cannot be found!
Got '<path>'` while Rust's plain `validate` and `install` passed the same guide, and
the desktop app would not install it.

Unchanged: the basic report (`validate --output json`, with or without `--dry-run`, no
`--full`) still reports every missing folder, as C#'s basic report does. A
`DelDuplicate` into a missing folder is still refused everywhere (deliberate; C#
creates the folder and passes). An empty Destination, a `.` part, a file in the way
and a missing archive still fail (in C# they fail its Environment stage instead). Plain
`validate` and `install` are unchanged; they already passed these guides, as C#
`install` does. Evidence: [full pipeline destination
proof](docs/parity/full-pipeline-destination-proof/README.md), 120 checks match and
18 are known differences against the live C# CLI with a HoloPatcher stub (78 fail on
`38c6ca9`; 122 / 16 since destination-path-parts, whose `dot_part` install and plain
`validate` checks now match); contract in
[full-pipeline-destination-contract.md](docs/parity/full-pipeline-destination-contract.md).

Users: the desktop Validate step and `validate --full` now pass a guide that copies
into a folder your game does not have yet (for example a missing `Override`), and
install creates the folder, as the C# wizard does. Unlike C#, validating does not
create the folder, so it appears only once you install.

## Windows device names and Peer staging folders in instruction paths — 2026-10-04 UTC

Two path rules now fail closed. First, the reserved-name rule, which every guide path,
archive member, new checkpoint entry, FOMOD path and download filename passes, also refuses
the Windows device names it missed: `COM¹`, `COM²`, `COM³`, `LPT¹`, `LPT²`, `LPT³`
(Windows reads the superscripts as device numbers), `CONIN$`, `CONOUT$`, `COM0` and
`LPT0`, with or without an extension and in any case, and a stem with spaces before
the extension (`CON .txt`), on every host and with the existing `Reserved filename`
wording. Before, plain `validate`, every report and `install` passed them, and install
created `game/COM¹/...` on Linux (on Windows the step would open a device). C#
`PathValidator` refuses `COM0` and `LPT0` too but accepts the others. Second, an
instruction path under `<<modDirectory>>` that names a Peer download staging folder
(`.peer-<nonce>.part`, any case, at any depth, including a wildcard part of that shape)
is refused with `Path is inside a Peer download staging folder`, in plain `validate`,
the install preflight (before any write), the dry run, every report and the desktop
Validate step. The engine scan hides these folders, so before, a Destination inside one
passed plain `validate` and the full report and `install` wrote into it unseen. Peer
and download code does not resolve instruction paths and is unchanged, as are game
folder paths. Longer names stay ordinary (`COM10`, `com1x.txt`, `COM¹¹`, `CLOCK$`). A
file whose name has the staging shape (`.peer-notes.part`) is refused too (fail closed).

Checkpoints: a new checkpoint session (`checkpoint capture`, `install
--component-checkpoints`, `--managed`) refuses a game or mod folder that holds a
file or folder with a device name anywhere, with `Checkpoint cannot record <path>:
Windows device name`, before any game or mod write (as it already did for `COM1`); a
maintenance run refuses such a name inside its subtree with `Reserved filename`. Stores, history and maintenance journals written before keep the rule they
were written with, so they stay listable, restorable and recoverable even when they
recorded such a file. Evidence: [reserved staging paths
proof](docs/parity/reserved-staging-paths-proof/README.md), 246 checks match and 42 are
known differences (Rust refuses what C# installs) against the live C# CLI with a
HoloPatcher stub (172 fail on `e756730`; 10, the stored-checkpoint and capture checks,
on the first version of this change, `81003bd`); contract in
[reserved-staging-paths-contract.md](docs/parity/reserved-staging-paths-contract.md).

Users: if `validate` or `install` now stops with `Reserved filename: ...`, a guide path
or an archive member uses a Windows device name such as `COM¹` or `CONOUT$`, which no
Windows install can hold; ask the guide author to rename it. `Path is inside a Peer
download staging folder` means a step points into a `.peer-...part` folder of your mod
directory, which holds an unfinished Peer download; point the step at the accepted
archive or its extracted folder instead. If `checkpoint capture`, `install
--component-checkpoints` or `--managed` stops with `Checkpoint cannot record <path>:
Windows device name`, rename or remove that file in your game or mod folder (no
Windows install can hold it), or install without checkpoints; after a refused install,
run `modsync restore` first to clear its recovery snapshot. Checkpoints you made
earlier stay usable.

## Destination path parts C# validation refuses — 2026-10-04 UTC

A `Copy`, `Move`, `CleanList` or `DelDuplicate` Destination with a part C#
`PathValidator.IsValidPath` refuses is now refused by plain `validate` and by
`install` before any write, as C# `install` refuses it (it validates first): a `.`
part (`<<kotorDirectory>>/Override/.`, `<<kotorDirectory>>/./Override`), `..` inside a
longer name (`a..b`, `..x`) or a control character such as a tab. The message is
`<component> / step <n> (<action>): Destination is not a valid path (a `.` part, `..`
inside a name or a control character): <Destination>`, after `Installation blocked
before any write:` for `install`. The check covers each selected component's own steps
and its immediate options' steps, selected or not, as the archive stage does. Before,
Rust resolved `Nope/.` to `Nope` and installed there although its own report (and the
desktop Validate step) refused it, and it installed into `a..b`, `..x` or a folder
with a tab with every report passing. The basic and full reports, `--phase remaining`
and the desktop Validate step now also fail `..` inside a name and control characters
(`Destination cannot be found! Got '<folder>'`, as for a `.` part), and DelDuplicate and
CleanList refuse them when they run, as C# does.

Unchanged: Copy and Move into such a folder still pass the dry run itself, as in C#;
a bare `..` stays `Unsafe path`; names such as `.x` or `a.b` are ordinary. C# fails
most of these in its full pipeline's Environment stage, which ends its report there,
while Rust fails the `ComponentValidation` stage (the decision matches). C# `install
--skip-validation` installs the Copy and Move cases; Rust has no such flag. A step
whose instruction conditions exclude it is not checked in Rust (it never runs); C#
refuses the guide. Evidence: [destination path parts
proof](docs/parity/destination-path-parts-proof/README.md), 138 checks match and 62
are known differences against the live C# CLI with a HoloPatcher stub (79 fail on
`a5b2053`); contract in
[destination-path-parts-contract.md](docs/parity/destination-path-parts-contract.md).
The root-destination proof's `.` Copy and Move cases, compared there with C#
`ModComponent.InstallAsync` alone, now count Rust's refusal as a known difference.

Users: if `validate` or `install` now stops with `Destination is not a valid path`,
remove the `.` part from the step's Destination (`<<kotorDirectory>>/Override`, not
`<<kotorDirectory>>/Override/.`), or rename a folder whose name has `..` or a control
character; no Windows install can use such a folder name, and the C# installer refuses
the guide too.

## Dependencies that name an option — 2026-10-04 UTC

A component whose `Dependencies` name an immediate option of another component
(`[[thisMod.Options]]`) instead of a component now validates and installs when that
option and its component are selected, as in C#: `GetConflictingComponents` looks a
GUID up among each component's immediate options too (`FindComponentFromGuid`). The
selection flags decide, not whether the option ran: an option no Choose step names
installs no files yet satisfies the Dependency, as in C#. Before, Rust
looked only at top-level components, so every report (its `InstallOrder`,
`FomodConfiguration` and `DryRun` stages), plain `validate` and `install` refused the
guide with `<component> requires selected component <option GUID>`, although its own
selection layer already follows such a Dependency. A Dependency on an option adds no
ordering edge, as in C#.

Unchanged and documented: when the option is not selected, or its component is not,
Rust still refuses before any write, as for any unmet Dependency; C# installs the
component anyway (and reads an option's own `IsSelected` even under an unselected
component). An option nested below another option does not count in either; C#
installs the other components and fails at the dependent one, Rust refuses before any
write. The desktop's auto-selection selects the option but not its component, as the
C# GUI does, so its Validate step refuses until that component is selected, where C#
installs. Restrictions and `InstallBefore`/`InstallAfter` are unchanged. Evidence:
[option dependencies proof](docs/parity/option-dependencies-proof/README.md), 49 checks
match and 17 are known differences against the live C# CLI with a HoloPatcher stub (42
fail on `0bc0efa`); contract in
[option-dependencies-contract.md](docs/parity/option-dependencies-contract.md).

Users: a guide that stopped with `requires selected component` naming an option's GUID
now installs when you select that option and its mod (in the desktop, tick the mod
yourself). Without them, the mod that depends
on the option is still refused, as before.

## Restrictions and step conditions that name a nested option — 2026-10-04 UTC

A GUID that names an option nested below another option (`[[thisMod.Options.Options]]`)
now counts as not selected wherever Rust checks selection, as in C#, whose loader keeps
no option below an option, so `FindComponentFromGuid` finds nothing: a component's
Restriction naming such an option is no conflict, and a step whose Dependencies name it
is skipped while a step whose Restrictions name it runs (a component's own steps and a
chosen option's steps alike). Before, Rust counted a selected nested option as
selected, so a Restriction naming it failed every report's `Conflicts`,
`FomodConfiguration` and `DryRun` stages, plain `validate` and `install` (`B conflicts
with <GUID>`), and step conditions naming it ran the steps C# skips and skipped the
ones C# runs. The report's `Conflicts` stage now also warns about a component
Dependency on a nested option, as C# does; `Guide::ordered` already refused that
Dependency and still does.

Unchanged and documented: a Restriction naming a selected immediate option is still
refused before any write (C#'s CLI loader deselects that option with AttemptFixes and
installs); an option selected under an unselected component still counts as not
selected; a Choose step of an option still runs the nested options it names (a Rust
extension: C# loads none, so it installs none of their files). Evidence: [nested option
references proof](docs/parity/nested-option-references-proof/README.md), 62 checks
match and 8 are known differences against the live C# CLI with a HoloPatcher stub (11
fail on `c386d33`); contract in
[nested-option-references-contract.md](docs/parity/nested-option-references-contract.md).

Users: a guide whose Restrictions name a nested option no longer stops with `conflicts
with`, and steps whose conditions name a nested option now run or are skipped as in
the C# installer, so a file can be added or left out compared with the previous
version. To make a step depend on a choice, name an option directly under the mod.

## Rename to a placeholder path — 2026-10-04 UTC

A `Rename` step whose Destination is a placeholder path, such as
`<<kotorDirectory>>/Override/z.txt`, now fails before any write, with `Rename
Destination must be a file name, not a placeholder path: <Destination>`. C#
`RenameFile` takes the Destination as the new file name in the source's folder and
joins it unresolved, so its install fails such a step (there is no folder named
`<<kotorDirectory>>` there); with `\` separators on Linux C# instead renames the
source to a file literally named after the whole Destination and succeeds, which Rust
also refuses. Before, Rust resolved the placeholder and moved the file
to that path, even from the mod folder into the game folder, so it installed guides C#
refuses. The dry run fails the step, so the full and basic reports (`DryRun` stage),
plain `validate` and the install preflight refuse it. A plain file name renames as
before, and a step that does not run (its conditions exclude it, or its option is not
selected) passes.

Unchanged and documented: C#'s dry run passes such a step, so its reports pass and its
install fails only when it reaches the step, keeping the earlier steps' writes; Rust's
reports already fail it (fail-closed). A Destination with a folder part (`sub/z.txt`,
`../z.txt`) is still refused everywhere; C# renames into or out of the source's folder.
Evidence: [rename destination
proof](docs/parity/rename-destination-proof/README.md), 25 checks match and 29 are
known differences against the live C# CLI with a HoloPatcher stub (24 fail on
`044fca5`); contract in
[rename-destination-contract.md](docs/parity/rename-destination-contract.md).

Users: a guide whose Rename Destination is a path stops with `Rename Destination must
be a file name` where it used to install; the C# installer fails it too. Write only the
new file name (`z.txt`); to move the file elsewhere, add a `Move` step.

## Drafted Renames — 2026-10-04 UTC

A `Rename` drafted from guide prose such as "Rename the file old.tga to new.tga." now
has the new name as its Destination (`new.tga`) and the named file as its only Source
(`<<modDirectory>>\old.tga`). Before, the drafter gave every drafted Rename a
`<<kotorDirectory>>\Override` Destination and, after "to", the new name as a second
Source, as the pinned C# drafter does; Rust refuses such a Destination (see the
section above), so a reviewed drafted Rename never installed. C# `RenameFile` reads the
Destination as the new file name in the source's folder, and the newer C# parser keeps
the bare name too. This applies wherever drafts are made: Markdown loaded for
`validate` or `install`, `convert --parse-directions` and the desktop's "Draft
instructions from prose" button.

Drafts stay drafts: the component still carries the review warning and must be
reviewed (`convert --review-draft <GUID>`, the desktop review button) before anything
installs, and after review the engine checks the step like any authored Rename. A new
name without an extension or with a folder part keeps the Override Destination and is
still refused after review; prose with a placeholder or without a new name drafts no
Rename and its unparsed text blocks the review. "Make a copy of X and rename it to Y"
is unchanged. `tools/check_drafts.py` now lists this Rename shape as an intentional
difference from the pinned C# drafter instead of failing (and binds the reference and
inputs into its sandbox and asks `convert` for JSON); the K1/K2 corpus drafts are
unchanged. Evidence: [drafted Rename proof](docs/parity/drafted-rename-proof/README.md),
57 checks match and 4 are known differences (24 fail on `b5df010`); contract in
[drafted-rename-contract.md](docs/parity/drafted-rename-contract.md). The frozen Rust
test `tests/drafts.rs` still expects the pinned drafter's Rename (its instruction
check at `tests/drafts.rs:13-18` fails on fixture component 12) and was not run; the
drift is recorded in the tracker for the post-freeze fixture repair.

Users: a reviewed drafted Rename now renames the file in its own folder instead of
stopping with `Rename Destination must be a file name`. Check its Source when reviewing:
the drafter names the file at the top of the mod folder, not inside an extracted
archive folder.

## Rename to a name that is not a valid file name — 2026-10-04 UTC

A `Rename` Destination must now be one file name that is valid on Windows, written as
it is used. A name with `..` inside it (`z..txt`, `..z.txt`), a control character
(a tab), a `./` part or a trailing separator (`z.txt/`) is refused with `Rename
Destination is not a valid file name` or `Rename Destination must be one file name`;
Rust renamed to it (or to `z.txt` for `./z.txt` and `z.txt/`) and every report passed.
It is refused where names Rust already refused (a trailing dot or space, a device name
such as `con.txt`, `[` or `]`) are: the ComponentValidation and DryRun stages of every
report, plain `validate` and `install`, before any write. The `..` and
control-character rule is the one C# applies to a Copy or Move Destination; C# never checks a Rename Destination, so its reports
pass these and its Linux install renames to the name as written (`z.txt/` fails), and
on Windows the result depends on the name. Rust is stricter here on purpose: a guide
should not rename a mod file to a name Windows cannot hold or reads differently.

The drafter keeps a drafted new name as a bare Destination only when the engine
accepts it, so prose such as "Rename the file old.tga as con.tga." keeps the
`<<kotorDirectory>>\Override` Destination the pinned C# drafter gives, and is refused
after review like a name without an extension. `tools/check_drafts.py` also lists a
Rename with that Destination and one Source (C# adds the new name as a second Source
after "to") as an intentional difference. Evidence:
[Rename file name proof](docs/parity/rename-file-name-proof/README.md), 37 checks match
and 63 are known differences against the live C# CLI (50 fail on `c2c9002`); contract
in [rename-file-name-contract.md](docs/parity/rename-file-name-contract.md).

Users: a guide whose Rename gives a new name such as `name..tga`, a name with a control
character, `./name.tga` or `name.tga/` now stops before installing anything. Write the
new name as one plain file name (`name.tga`).

## Extract with a Destination — 2026-10-05 UTC

An `Extract` step now always extracts into a folder named after the archive, next to
it (`mods/m/` for `<<modDirectory>>/m.zip`), whatever its `Destination`, as the C#
CLI and GUI do: C# validation (`ComponentValidation`, with AttemptFixes, on by
default) clears an Extract Destination before the dry run and the install. Rust
extracted into the Destination, so `Extract m.zip -> <<kotorDirectory>>/Override`
wrote the archive's files, readme included, into `Override`, and a later step written
for C#'s layout (`Move <<modDirectory>>/m/Override/*`) failed validation and install.
The basic report (`validate --output json --dry-run` without `--full`) now fails an
Extract, Choose or Delete step with a Destination (`'Destination' key cannot be used
with this action`), one error per step, as C#'s does; the full report and the desktop
Validate step pass it, as C#'s Environment stage has already cleared it. A
Destination Rust refuses as a path (outside the roots, `..`) is still refused before
any write, though it is not used; C# clears it and installs (fail closed, kept). The
install and dry run log `Extract Destination '...' is not used`. Evidence: [Extract
Destination proof](docs/parity/extract-destination-proof/README.md), 82 checks match
and 9 are known differences against the live C# CLI (41 fail on `c0e9cef`); contract
in [extract-destination-contract.md](docs/parity/extract-destination-contract.md).
The root-destination, delete-directory, empty-source, extract-overwrite and FOMOD
S2b proofs were updated for it (see the contract).

Users: an `Extract` step with a `Destination` no longer extracts there; its files are
in the folder named after the archive, next to it, and later steps must name them
there, as in C#. Remove the Destination from Extract steps; the basic validation
report fails it.

## A loose Source next to an Extract — 2026-10-05 UTC

Plain `validate` and `install` now refuse, before any write, a step whose Source in
the mod folder none of its component's archives holds, once the component extracts
an archive that exists (`Failed to find '<Source>' in any archives`), as the C# CLI
`install` and the desktop Validate step do: C# validation
(`ComponentValidation.VerifyExtractPaths`) runs before installing and refuses it even
when the file exists. Rust's reports already ran the check, so plain `validate` passed
and `install` copied the loose file (a stale file left by an earlier install, a file
another component extracts, a file an earlier Rename made). The check covers every
step of the component and of its options, chosen or not, whose conditions are met;
game Sources, Extract and Choose steps and a DelDuplicate's extensions are not
checked, and a component with no Extract never is, so guides for mods the user
unpacks by hand are unaffected (205 of the 210 components the mod-builds corpus drafts
with `--parse-directions`; the other five name a missing archive). A component named
`Improved AI` is not checked, as in C#; Rust's reports refused it before. Archives are
searched in step order, each listed (without extracting it) only when a Source needs
it, stopping at the first that holds the Source, as C# opens them; an archive Rust
cannot list (a link or a case-duplicate entry) in an option that is not chosen no
longer refuses the component, in the reports either, and a missing archive there is
left out, as in C#. Known differences: C#
matches the Source case-sensitively and reads DelDuplicate extensions as paths, so it
refuses both where Rust installs; C# rewrites a Source to a doubled archive folder
(`d/d/...`) where Rust refuses it (fail closed, unchanged). Evidence: [Loose Source
proof](docs/parity/loose-source-proof/README.md), 98 checks match and 49 are known
differences against the live C# CLI (33 fail on `6ca4d35`; 98 match / 56 known since
the fail-open case below was probed, 2026-10-05); contract in
[loose-source-contract.md](docs/parity/loose-source-contract.md). The
extract-overwrite proof's `stale_*` cases are now refused as by the C# CLI, and the
FOMOD S2b and gate-scope proofs no longer copy a loose note in a component that
extracts (counts unchanged).

Users: in a component that extracts an archive, name only files that archive holds,
under the folder named after it (`<<modDirectory>>/m/...` for `m.zip`). Move a step
that copies a loose file or another component's file into a component without an
Extract step, or into the component that extracts that file.

Fail-open, now probed (2026-10-05): when no archive Rust can list holds a loose
Source and an option that is not chosen extracts an archive Rust cannot list (a
case-duplicate entry, a link), Rust passes the Source and installs it; C# opens that
archive, does not find the Source and refuses (`unchosen_dup_loose` in the proof, 7
known checks).

## An Extract with no Source in the validation reports — 2026-10-05 UTC

An `Extract` step with no Source (`Source = []`, `Source = [""]` or no `Source` key)
in a component none of whose Extract steps names an archive that exists now fails the
validation reports' archive stage with `Missing Required Archives: []`, one error per
such step, as C# `ComponentValidation.VerifyExtractPaths` does. The basic report
(`validate --output json --dry-run`), the full report (`--full`, with or without
`--dry-run`), `--phase remaining` and the desktop Validate step now fail it; before,
the reports with a dry run passed it with a `DryRunSkipped` warning, while the C# CLI
fails it (its full report in the Environment stage) and Rust's own plain `validate`
and `install` refused it. Unchanged: plain `validate` and `install` still refuse it
before any write, as the C# CLI `install` does; an empty Extract beside an archive
that exists still passes the reports (as in C#) and fails plain `validate` and
`install`; an empty Extract in an option is not reported, as in C#. Evidence: the
[empty-source proof](docs/parity/empty-source-proof/README.md), now also compared with
the live C# CLI, 86 checks match and 3 are known differences (7 fail on `b8b4d57`),
and the [missing archives proof](docs/parity/missing-archives-proof/README.md), 50
match and 19 known (3 fail on `b8b4d57`); contract in
[empty-extract-report-contract.md](docs/parity/empty-extract-report-contract.md).

Users: the Validate step now fails a component whose Extract step has no Source, as
the install already did. Give the step the archive's path
(`<<modDirectory>>/name.zip`), or remove it.

## The order of a step's Sources — 2026-10-05 UTC

A Copy, Move or Extract step now handles the files its Sources name in the order the
Sources are written, as C# does (`Instruction.SetRealPaths`), so when two of them
write the same file the later Source's file wins (with `Overwrite = false`, the first
is kept). Before, Rust handled them in path order: `Copy [<<modDirectory>>/b/x.txt,
<<modDirectory>>/a/x.txt] -> Override` installed `b`'s `x.txt` where C# installs
`a`'s, and `Extract [pack.zip, pack.7z]` (both extracting to `pack/`) left
`pack.zip`'s files where C# leaves `pack.7z`'s. Every check passed on both sides; only
the installed files differed. A file two Sources name keeps its first place. The
order within one wildcard stays path order (C# uses the directory listing order,
which .NET does not define on Unix). Evidence: [source order
proof](docs/parity/source-order-proof/README.md), 18/18 checks against the live C# CLI
(5 fail on `b8b4d57`), and the five `same_stem_*` cases of the [extract-overwrite
proof](docs/parity/extract-overwrite-proof/README.md) (2 fail on `b8b4d57`); contract in
[source-order-contract.md](docs/parity/source-order-contract.md).

Users: when two Sources of one step write the same file, the later one now wins, as in
C#. Put the file you want to keep last in the Source list, or use two steps.

## A wildcard Source that matches nothing beside one that does — 2026-10-05 UTC

A `Copy`, `Move` or `Rename` step whose Sources include a wildcard that matches no
file now handles the files its other Sources name, as C# does
(`Instruction.SetRealPaths` gathers every Source's files and fails only when none
matched): `Move [<<modDirectory>>/m/*.tga, <<modDirectory>>/m/*.tpc] -> Override` with
only TGAs in `m/` moves the TGAs. Before, Rust required every Source to match, so
plain `validate` and `install` refused such a step before any write (`Source matches
no files: <<modDirectory>>/m/*.tpc`), the dry-run reports and the desktop Validate
step skipped a Copy or Move component with a `DryRunSkipped` warning (failed a Rename
or a step in a chosen option), and `conflicts` stopped, where the C# CLI validates
and installs. Unchanged: a step whose Sources all match nothing still fails, as in
C#; a wildcard in the mod folder next to the component's own Extract must still be
in one of its archives (`Failed to find ... in any archives`, both refuse). Kept
stricter, documented: a literal Copy or Move Source that names no file still fails
the step, where C# drops it and installs the rest (a named file that is missing
usually means a wrong or incomplete download); Extract still needs every Source to
match (missing-archives contract); CleanList, Execute and Run are unchanged. A partly
matching list no longer enters recovery, so a wildcard whose files only a registered
archive holds (auto-extracted before) is now dropped, as in C#; a missing literal
that a registered archive holds is still recovered and installed, where C# drops it.
Evidence: [wildcard miss proof](docs/parity/wildcard-miss-proof/README.md), 71 checks match and
13 are known differences against the live C# CLI with a HoloPatcher stub (40 fail on
`f6d2ec5`); the [delete-directory proof](docs/parity/delete-directory-proof/README.md)
now matches its `rename_present_and_wildcard_nomatch` case (170 / 10); contract in
[wildcard-miss-contract.md](docs/parity/wildcard-miss-contract.md).

Users: a guide step that lists several wildcards (`*.tga`, `*.tpc`, `*.txi`) now
installs when some of them match nothing, as in C#. A step whose wildcards all match
nothing, or that names a missing file without a wildcard, still stops the install
before any write: fetch the download again, or fix the Source.

## A Patcher step's Destination — 2026-10-05 UTC

A `Patcher` step now always runs the patcher on the game folder, whatever its
Destination, as C# does (`ExecuteTSLPatcherAsync` passes the game folder as
`--game-dir`, and its validation sets any other Destination to `<<gameDirectory>>`).
Before, Rust ran it on the Destination: `<<kotorDirectory>>/Override` gave `--game-dir
<game>/Override`, so the patch landed in `Override/Override` (and a later step that
needed the patched file was refused), and `<<modDirectory>>` patched the mod folder.
The dry run's InstallList simulation, `conflicts` and the reports follow the same
folder. The validation reports no longer fail a Patcher step without a Destination
or with another one: the basic report (`validate --output json` without `--full`)
gives C#'s warning (`Destination must be <<gameDirectory>> with 'Patcher' action,
setting it now automatically.`) or error (`'Destination' key must be either null or
string literal '<<gameDirectory>>' (legacy: '<<kotorDirectory>>') for this action.
Got '<Destination>'`), counted as C# counts them; the full report and the desktop
Validate step pass it, as C#'s Environment stage has fixed it. Before, both reports
failed it, so the desktop app would not install a Patcher step with no Destination.
Unchanged and documented: a Destination Rust cannot resolve (outside the roots, `..`,
an unknown or miscased placeholder) is still refused before any write, where C#
ignores it; the rooted reports still leave out C#'s other ComponentValidation
warnings. Evidence: [patcher destination
proof](docs/parity/patcher-destination-proof/README.md), 63 checks match and 9 are
known differences against the live C# CLI with a HoloPatcher stub (30 fail on
`d4f30d1`); contract in
[patcher-destination-contract.md](docs/parity/patcher-destination-contract.md).

Users: a Patcher step needs no Destination; the patcher always patches the game
folder you chose. If a guide gave one such as `<<kotorDirectory>>/Override`, an
earlier Rust install may have left patched files in `Override/Override` (or an
`Override` folder in the mod folder); remove them and install again.

## Component checks in validate --full --dry-run-only — 2026-10-05 UTC

`validate --full --dry-run-only` now fails a guide whose selected component has an
archive/destination error, as the C# CLI does: its full pipeline skips the
per-component archive stage under `--dry-run-only`, but its Environment stage still
runs those checks on every selected component and fails with `Some components failed
to validate. Check the output/console window for details.` (and a second line about
antivirus or missing mods), counting one error and ending the report. Before, Rust's
Environment stage checked only the recovery state, so the report passed (exit 0,
one `DryRunSkipped` warning) a guide whose archive was missing or whose loose Source
no archive holds; an empty Destination or one below a file failed only later, in the
dry run. Rust now gives C#'s message and then each component's errors (`ERROR: A:
Missing Required Archives: [<<modDirectory>>/m1.zip]`), which C# writes only to its
log. Documented: a component with some, but not all, of its archives missing (in
one Extract such as `[m1.zip, m2.zip]` with `m2.zip` absent, or across several
Extract steps) now fails this stage in Rust (fail closed, new: it passed before),
where C# checks archives only when none of the component's archives exists and
passes; an option's missing archive fails this stage in Rust where C# fails it in
its dry run. `--dry-run-only` without `--full`, the other reports and the desktop
Validate step are unchanged. Evidence: [dry-run-only environment
proof](docs/parity/dry-run-only-environment-proof/README.md), 101 checks
match and 11 are known differences against the live C# CLI with a HoloPatcher stub
(42 fail on `72ed369`); contract in
[dry-run-only-environment-contract.md](docs/parity/dry-run-only-environment-contract.md).

Users: `validate --full --dry-run-only` no longer reports `Validation passed` while a
selected mod's archive is missing; download it (or deselect the mod) and validate
again. To check the steps without the downloads, drop `--full`.

## Case duplicates in the game folder in the full report, 2026-10-05 UTC

`validate --full` (also with `--dry-run` or `--dry-run-only`, and `--phase
remaining`) now reports a game folder that holds two names differing only in case
(`Override/a.txt` and `Override/A.txt`, or `Override` and `override`, at any depth) as
the C# CLI does: its Environment stage fails with `You have duplicate files/folders in
your installation directory in a case-insensitive environment.Please resolve these
before continuing. Check the output window for the specific files to resolve.`,
counting one error and ending the report; that message replaces the stage's other
messages (a missing archive, a read-only folder). Rust lists the name it found after
it (`ERROR: <path> has a duplicate, please resolve before attempting an install.`).
Before, Rust refused the folder before any report: the text output was the bare
`Error: Case-colliding files: <path>` and the JSON report had `errorCount` 0,
`componentCount` 0 and no stages. Both failed; only the report differed. The desktop
Validate step shows the same failed Environment stage. Unchanged and documented:
`install`, plain `validate`, `conflicts` and the basic report (`--dry-run` without
`--full`) still refuse with `Case-colliding files: <path>` before any write (C#'s basic
report passes, as only its Environment stage checks; fail closed); a case duplicate
only in the mod folder is still refused before any report, where C# passes (it checks
only the game folder). Evidence: [case duplicates
proof](docs/parity/case-duplicates-proof/README.md), 95 checks match and 12 are known
differences against the live C# CLI with a HoloPatcher stub (42 fail on `0f41c1f`);
contract in [case-duplicates-contract.md](docs/parity/case-duplicates-contract.md).

Users: if `validate --full` fails with `You have duplicate files/folders in your
installation directory`, keep one of the two names it lists (merge the folders, or
delete the file you do not want) and validate again. Nothing was installed.

## Restrictions and Dependencies in the full report, 2026-10-05 UTC

`validate --full` (also with `--dry-run` or `--dry-run-only`, and `--phase remaining`)
now fails its Environment stage when a selected component restricts another selected
component (or a selected immediate option), or depends on one that is not selected,
as the C# CLI does: the stage fails with `Some components failed to validate. Check
the output/console window for details.` (and C#'s second line about antivirus or
missing mods), counting one error and ending the report; that message replaces a
read-only folder message, as in C#. Rust then lists each finding in C#'s log wording
(`ERROR: Cannot install 'A' due to 'C' being selected for install.`, or `... not being
selected for install.`), which C# writes only to its log. Before, Rust's Environment
stage passed and its Conflicts, InstallOrder and FomodConfiguration stages each failed
the guide (three errors, four with `--dry-run-only`). Both failed; only the report
differed. A Restriction or Dependency naming no component or immediate option is
skipped here, as in C#. The desktop Validate step shows the same failed Environment
stage. Unchanged and documented: `install`, plain `validate`, `conflicts` and the basic
report (no `--full`) still refuse a restriction conflict or an unselected Dependency
before any write (C#'s basic report passes a restriction conflict; fail closed);
C#'s loader selects every Dependency and deselects what a selected component
restricts (AttemptFixes) and Rust does not, so with `--use-file-selection` (or a
restricted selected option) C# passes where Rust's Environment stage now fails with
C#'s message; Rust's `--select` adds the dependency closure where C#'s does not, so C#
fails there and Rust passes. Without `--dry-run-only` a component's archive error
still fails Rust's ComponentValidation stage, not its Environment stage. Evidence:
[environment relations proof](docs/parity/environment-relations-proof/README.md), 85
checks match and 33 are known differences against the live C# CLI with a HoloPatcher
stub (44 fail on `1ab3d27`); contract in
[environment-relations-contract.md](docs/parity/environment-relations-contract.md).

Users: if `validate --full` fails with `Some components failed to validate` and a
`Cannot install 'A' due to 'C' being selected for install.` line, two selected mods
cannot be installed together: deselect one of them (or select the missing mod named
in a `not being selected` line) and validate again. Nothing was installed.

## A read-only game or mod folder, 2026-10-05 UTC

`validate --full` (also with `--dry-run` or `--dry-run-only`) now fails when Rust
cannot write to the game folder or the mod folder, as the C# CLI does: its
Environment stage tries to create a file in each and fails with `The Install
directory is not writable! Please ensure administrative privileges or reinstall
KOTOR to a directory with write access.` or, when the mod folder fails, `The Mod
directory is not writable! Please ensure administrative privileges or choose a new
mod directory.`, counting one error and ending the report. `install` refuses the same
folders before any write into them or any snapshot (`Installation blocked before any
write: <message>`), as C#'s install refuses after its validation pipeline fails; if
the folder's parent is read-only too, the recovery lock fails first with the
operating system's error. Before, Rust's full report
passed (exit 0), and `install` took its recovery snapshot and stopped at its first
write (`Read-only file system`), leaving an install to restore; with only the mod
folder read-only, a guide that reads from it installed. The desktop Validate step and
`--phase remaining` use the same check. Rust asks the operating system (Unix
`access(2)`, which also sees a read-only mount) and writes no probe file. Unchanged
and documented: the basic report (no `--full`) and plain `validate` do not check, as
C# does not; under `--full` without `--dry-run-only` a component error is reported
later, in ComponentValidation, so the Environment stage gives the folder message where
C# gives its component message; Rust does not offer to fix the permissions (C#'s
`install` asks and then tries chmod, chown and sudo); Windows is not checked.
Evidence: [writable roots proof](docs/parity/writable-roots-proof/README.md), 58 checks
match and 1 is a known difference against the live C# CLI with a HoloPatcher stub
(31 fail on `2de5a47`); contract in
[writable-roots-contract.md](docs/parity/writable-roots-contract.md).

Users: if `validate --full` or `install` now stops with `... directory is not
writable!`, make the game folder (or the mod folder) writable for your user, or move
the game out of a protected folder, then validate again. Nothing was installed.

## install --no-managed — 2026-10-05 UTC

`install` now accepts the C# CLI's `--no-managed`: `install -i guide.toml -g game -s
mods --no-managed` validates and installs without managed deployment, as C# does for
that run even when settings.json turns managed deployment on. Before, Rust stopped
with a usage error (`unexpected argument '--no-managed'`, exit 2) and nothing was
validated or installed. The Rust CLI already installs this way unless `--managed` is
given, so the flag changes nothing else: the same validation, recovery snapshot and
stop on failure. `--managed --no-managed` stops with C#'s `Cannot combine --managed
and --no-managed.` (exit 1, nothing written; Rust refuses before validating, C# after).
`validate` and `conflicts` refuse `--no-managed` (`--no-managed is only supported by
install`, exit 1), as the C# `validate` does not take it.

Unchanged and documented: the Rust CLI does not read settings.json's
`managedDeploymentEnabled`, so without either flag it installs classic where C# would
start a managed install for the active profile. A component that fails validation
still stops the install before any write; C# installs the components before it.
Evidence: [no-managed proof](docs/parity/no-managed-proof/README.md), 14 checks match
and 3 are known differences against the live C# CLI (7 fail on `bef5735`); contract
in [no-managed-contract.md](docs/parity/no-managed-contract.md).

Users: a C# command line with `install --no-managed` now runs in Rust and installs as
it did without the flag. Leave out `--managed` when you pass it. For a managed install
in Rust, pass `--managed` with `--component-checkpoints`; the settings.json toggle
applies to the desktop only.

## Several values after one --select — 2026-10-05 UTC

`validate`, `install` and `conflicts` (and the Rust-only `selection` and
`fetch-downloads`) now read every value after `--select` up to the next option, as
the C# CLI does (its `--select` is a list option, `IEnumerable<string> Select`):
`install -i guide.toml -g game --select category:Bugfix category:Immersion` installs
both categories. Before, Rust took one value per flag, so that command line stopped
with a usage error (exit 2) and nothing was validated or installed. The selection rules are
unchanged: categories are ORed, tiers are ORed, the two groups are ANDed, and an exact
component name or GUID still wins over selector syntax (a Rust extension; C# ignores
a value without `:`). Still accepted: a repeated `--select`, which the C# CLI refuses
(`defined multiple times`, exit 1). Still refused: `--select=a b` (C# reads `b` as a
second value; Rust stops with a usage error before anything runs) and `--select`
with no value, as in C#. `convert` and `merge` keep one value per `-s`/`--select`,
since their positional input and output paths would otherwise be read as selections.
Evidence: [select values proof](docs/parity/select-values-proof/README.md), 34
checks match and 8 are known differences against the live C# CLI (23 fail on
`b97c090`); contract in
[select-values-contract.md](docs/parity/select-values-contract.md).

Users: `--select category:Bugfix category:Immersion` now works as in the C# CLI, and
`--select category:Bugfix --select category:Immersion` still works. Quote a value
with spaces (`--select "tier:1 - Essential"`).

## A missing guide for convert or merge, 2026-10-05 UTC

`convert` and `merge` (also `convert --merge`) now name a guide that does not exist,
or is a folder, before reading anything, as the C# CLI does
(`CLI/ModBuildConverter.cs:1966-1970`, `:2473-2483`): `Input file not found: <path>`
for `convert`, `Existing file not found: <path>` then `Incoming file not found:
<path>` for `merge`, the path as typed. Rust said `Read <path>: No such file or
directory (os error 2)` (or `Read <path>: Archive source must be a regular file, ...`
for a folder). Exit codes (1) and the absence of any write are unchanged; `--stdin`
is unchanged; a symlinked guide is still refused by the read, as before. C# logs the
message on stdout with its log prefix, Rust prints `Error: <message>` on stderr (as
for every error). The other guide readers (`merge-preview`, `merge-apply`, `list`,
`download-plan`, `selection`, `fetch-downloads`; no C# verbs) keep `Read <path>:
...`. Evidence: [missing guides proof](docs/parity/missing-guides-proof/README.md), 45
checks match against the live C# CLI (13 fail on `5796295`); contract in
[missing-guides-contract.md](docs/parity/missing-guides-contract.md).

Users: when `convert` or `merge` stops with `Input file not found`, `Existing file not
found` or `Incoming file not found`, the named guide does not exist (or is a folder);
fix that path and run the command again. Nothing was written.

## A missing guide, game folder or mod folder, 2026-10-05 UTC

`validate`, `install` and `conflicts` now name a guide, game folder or mod folder that
does not exist, as the C# CLI does (`CLI/ModBuildConverter.cs:3030-3040`,
`:3056-3078`, `:3318-3328`, `:3349-3353`): `Input file not found: <path>`, `Game
directory not found: <path>` or `Source directory not found: <path>`, the path as
typed, checked in that order; with `--output json` it is the report's `error` field.
Rust said `No such file or directory (os error 2)` for either folder (or `Choose
existing mod and game directories` for a file in place of one) and `Read <path>: No
such file or directory (os error 2)` for the guide. Exit codes (1) and the absence of
any write are unchanged. The folder message comes from the shared root check, so
`restore`, `completion-summary`, checkpoint and maintenance commands and the desktop
Validate and Install steps show it too. Unchanged and documented: C#'s plain `validate`
ignores `-s`/`-g` and fails its FOMOD stage, where Rust names the missing folder;
`convert`, `merge` and the other guide readers keep `Read <path>: ...` (`convert` and
`merge` since changed, see the section above). Evidence:
[missing paths proof](docs/parity/missing-paths-proof/README.md), 62 checks match and 1
is a known difference against the live C# CLI with a HoloPatcher stub (19 fail on
`b0483d5`); contract in [missing-paths-contract.md](docs/parity/missing-paths-contract.md).

Users: when a command stops with `Game directory not found`, `Source directory not
found` or `Input file not found`, the named path does not exist (or is the wrong kind:
a file where a folder is needed, or the reverse); fix that `-g`, `-s` or `-i` argument
and run it again. Nothing was written.

## A patcher without changes.ini in the validation dry run — 2026-10-04 UTC

When a component's `Patcher` step finds its patcher executable but no `changes.ini`
in the patcher folder or its `tslpatchdata` folder (and no named INI or
`namespaces.ini`), the validation dry run (`validate --dry-run`, the desktop Validate
step) now reports nothing for it, as C# does: its virtual simulation
(`TslPatcherVfsSimulator.SimulateInstallAsync`) only logs `No changes.ini found` and
the step succeeds. Before, Rust added `WARNING: [PatcherSimulation] No changes.ini
found; no InstallList effects inferred` per such step, so `warningCount` and the
Validate step's warning cards differed from C#. The decision was already the same. The
step's progress event now says `No changes.ini found under <patcher folder>; skipped
InstallList simulation`.

Unchanged: a present `changes.ini` is simulated as before (an empty one, or one without
`[InstallList]`, has no effect and no warning, as in C#). An InstallList file missing
from `tslpatchdata` still warns that its content is unknown (deliberate: C# writes an
empty file in its place). A missing named INI or `namespaces.ini` entry, or a
namespace index above `0` without `namespaces.ini`, still fails the step; a missing
executable or patcher folder is still `DryRunSkipped`; install still hands the folder
to the patcher backend, which needs that configuration. Evidence: [patcher missing
configuration proof](docs/parity/patcher-missing-configuration-proof/README.md), 69
checks match and 3 are known differences against the live C# CLI (30 fail on `c22836a`); contract in
[patcher-missing-configuration-contract.md](docs/parity/patcher-missing-configuration-contract.md).

Users: Validate no longer warns about a patcher without `changes.ini`, as C# does not.
Validate does not check that a patcher has its configuration: if a patcher mod's
folder has no `tslpatchdata/changes.ini`, re-extract or re-download it before
installing, or the patcher will fail during install.

## CleanList list-file choice — 2026-10-04 UTC

A `CleanList` reads one list file. When its Source list names more than one existing
file, Rust now reads the first authored Source's file, as C# does
(`ExecuteCleanListAsync` reads `RealSourcePaths[0]`). Before, Rust read the match with
the smallest sorted path: `CleanList [<<modDirectory>>/l/b.csv, <<modDirectory>>/l/a.csv]`
applied `a.csv` where C# applies `b.csv`, so a different set of game files was deleted.
Validate and the dry-run report passed in both.

Unchanged: every Source must still match, so a folder or missing Source beside a
resolving one is refused where C# reads the resolving list (fail-closed). When the
first Source is a wildcard with several matches, Rust reads the first in sorted order;
C# uses directory listing order, which .NET does not define on Unix. Evidence:
[cleanlist-list-choice proof](docs/parity/cleanlist-list-choice-proof/README.md), 39
checks match and 3 are known differences against a live reference oracle (9 fail on
`0997b0f`); contract in
[cleanlist-list-choice-contract.md](docs/parity/cleanlist-list-choice-contract.md).

Guide authors: give a CleanList exactly one list file. Only the first Source's list is
read; the other Sources are ignored, in C# and now in Rust. Do not rely on a wildcard
that matches several lists.

## Directory Sources and wildcard rules for file actions — 2026-10-04 UTC

A `Delete`, `Copy`, `Move`, `Rename`, `CleanList`, `Execute` or `Run` Source that names
a directory (`<<kotorDirectory>>/Override`, `<<kotorDirectory>>/Override/textures`, a
mod folder) no longer reaches the files beneath it, as in C#. The reference binds these Sources
with `PathHelper.EnumerateFilesWithWildcards`, which returns files only, and
`Instruction.DeleteFile` deletes a path only when `File.Exists` holds. So a folder
Source is a missing file:

- `Delete`: nothing is deleted with `Overwrite` false or omitted; with `Overwrite =
  true` the step fails with `MissingSourceFiles`, as before.
- `Copy`, `Move`, `Rename`, `CleanList`: the step fails with `MissingSourceFiles`
  (a folder is a missing literal Source) whatever `Overwrite` is. Plain `validate`
  and `install` refuse before any write; the `validate --dry-run` report gives the
  `DryRunSkipped` warning.
- `Execute`, `Run`: the step fails before any program starts (with
  `--allow-processes`, Rust ran every file under the folder before), and the dry-run
  report fails, as C# reports `UnknownError` ("Program file does not exist").

Missing Sources are now classified as in C# in the dry-run report. C# keeps every
literal Source, so its expansion is empty only when every Source is a wildcard that
matched nothing, and the actions differ:

- Rename and CleanList: a missing literal Source (a folder included) is
  `MissingSourceFiles` (`DryRunSkipped`); when only wildcard Sources matched nothing,
  C# throws on the empty expansion, so the report fails. A missing CleanList list was
  an error before.
- Execute and Run: a missing literal program fails the report (it was typed as a
  skip); when only wildcard Sources matched nothing (`<<modDirectory>>/x/*.nope`, or
  `<<modDirectory>>/*` where `*` matches only folders), C# reports `MissingSourceFiles`,
  so the report gives `DryRunSkipped`.

Plain validate and install refuse all of them, as before.

Before this change Rust took these Sources from the engine's shared source matching,
which also matches every file under a folder Source. `Delete <<kotorDirectory>>/Override`
emptied `Override`; `Move <<kotorDirectory>>/Override/sub -> <<modDirectory>>/d` moved the
whole subtree, flattened, out of the game; `Rename <<kotorDirectory>>/Override/sub -> c.txt`
renamed every file under it onto one name, overwriting as it went; `CleanList
[<<modDirectory>>/d]` read the first file under `d` as its list. None of them warned.

Wildcards in every action's Source now follow C# `PathHelper.WildcardPathMatch`: each
path level is matched on its own, `*` and `**` match any run of characters within one
level (`**` is not recursive), `?` matches one character, and `[` and `]` are literal
characters, not a class. Before, `Delete <<kotorDirectory>>/Override/**` deleted every
file in every subfolder of `Override` (C# deletes only the files directly in it), and
`Delete <<kotorDirectory>>/Override/[ab].tga` deleted `a.tga` (C# deletes only a file
literally named `[ab].tga`). The FOMOD choice scoping uses the same rule, and an
archive literally named `[e]xample.zip` is extracted and scoped when the Source names
it. Literal non-Source paths (destinations, a Patcher Source) still may not contain `[`
or `]`; archive member names may since the archive member brackets slice (below).

Unchanged: Extract, Patcher, the archive preflight and the FOMOD gate keep the folder
rule, which FOMOD root-folder Extract flows rely on. Known differences:

- Rust accepts an Extract of a folder that holds archives (`Extract
  <<modDirectory>>/n` with `n/deep.zip` extracts `deep.zip`); C# refuses it with
  `MissingSourceFiles`. When the folder holds no archives both refuse install, but
  Rust's dry-run report has an error where C# only skips.
- Rust requires every literal Source to match: `Copy [<<modDirectory>>/d,
  <<modDirectory>>/d/m.txt]` is refused, where C# drops the missing Source and goes
  on. A wildcard that matches nothing beside a matching Source is dropped in a Copy,
  Move or Rename since 2026-10-05 (see "A wildcard Source that matches nothing beside
  one that does").
- When no Delete Source matched, `validate --dry-run` adds one `DeleteFile` warning
  per Source (`Delete Source is a directory; Delete removes files only: <path>` for a
  folder, `Attempting to delete non-existent file: <path>` otherwise); C# adds none.
- Rust matching stays case-insensitive where C# wildcard matching is ordinal.

The FOMOD S2b proof's `scope_char_class_*` checks now expect `[e]xample.zip` to be a
missing source, and its new `scope_bracket_literal_*` checks install an archive
literally so named (265 checks). Evidence:
[delete-directory proof](docs/parity/delete-directory-proof/README.md), 169 checks
match and 11 are known differences against a live reference oracle (4 fail on
`ab6b389`, 13 on `df67229`; 167 / 13 since extract-destination, 170 / 10 since
wildcard-miss); contract in
[delete-directory-contract.md](docs/parity/delete-directory-contract.md).

Guide authors: `Delete`, `Copy`, `Move`, `Rename`, `CleanList`, `Execute` and `Run` act on
files, not folders. A Source that names a folder is a missing file: Delete deletes
nothing (and fails with `Overwrite = true`); the others fail the mod, in C# and now in
Rust. Use `folder/*`. `Extract` of a folder works in Rust but fails in C#; name each
archive.
A wildcard reaches only its own folder level: `Override/**` is the same as
`Override/*`, so name each subfolder level you need (`textures/*`, `textures/hd/*`).
`[` and `]` in a Source are ordinary characters.

## Fetch shared URL and destination — 2026-09-28 UTC

`fetch-downloads` no longer lets two selected components race for one archive file.
Two targets may publish one file when their `Files` names are equal (ASCII
case-insensitive), or when they have the same registry URL and a name is revealed
only by the transfer. The plan groups targets transitively by that relation: any two
targets that may publish one file are in one group, even when they are linked only
through a third target (a name match on one side, a URL match on the other). Each
group is owned by its first target in guide order; every other member gets
`sharedDestination` `{"kind":"sameUrl"|"differentUrl","targetIndex":N}`, where `N`
is the owner and the kind only compares URLs. A group runs in one worker: owner
first, then its linked targets in guide order, never concurrently. A linked target
takes, with no request, a file an earlier member published under its name (for an
unnamed target, from its own URL). This is the rule by which a rerun finds the file
present by name (`AlreadyPresent`). Its own `ContentHashSHA256` must still match.
Otherwise it takes the unfinished outcome of an earlier member with an identical
source list (same URL and fallbacks) that asked for the same name, or, like the
target, for no name, since a repeat would make the same request again. An unnamed
target therefore never inherits the failure of a member that asked for a name, which
may be specific to that name. Otherwise it runs its own sources, for example a
different URL after the owner failed. A target that took another
member's result keeps its own `targetIndex`, component and registry URL, gains
`sharedWith: <member>`, and gets one event with its final phase. Every component is
therefore reported, recorded under `--record` and bound as a FOMOD consumer of the
one archive, in guide order.

Before, the later component failed on the existing destination and was neither
recorded nor bound, and Fetch exited 1. With several workers, both components had
requested the file and the loser got `File exists`. With `--workers 1` there was one
request, and the second target failed before requesting with `Download destination
already exists`.

Consequences:

- With identical sources, the first member's outcome governs: if the owner's
  authored hash rejects the bytes, a later component with the same URL fails too,
  even without a hash.
- A component whose `Files` name another URL has just published takes that file, as
  C# and a Rust rerun do, subject to its own hash.
- On a case-sensitive filesystem, names that differ only in case now share one
  destination: `Example.zip` and `example.zip` give one file, `Example.zip`.
- For a page URL such as a Nexus mod page, an unnamed component takes the file an
  earlier component named.
- A component without a `Files` name that runs its own source and whose transfer
  reveals a name already present (in a group, after an unlinked component that ran
  first, or on a rerun) adopts the present file without replacing it: under
  its own `ContentHashSHA256` when it authors one, else only when the present file
  holds exactly the downloaded bytes. Otherwise it fails with an error that names
  the present file and says it was not replaced; C# would overwrite it. The
  transfer's temp file is removed either way. A refusal ends the target only when a
  later source would reveal the same name (a repeat of the same Origin URL);
  otherwise the target falls through to its next source, such as Peer in Fallback
  mode. A run cancelled before or during the adoption ends `cancelled` and adopts
  nothing. An unnamed transfer that finds its name absent but loses the no-clobber
  publish to a concurrent transfer adopts the same way instead of failing with
  `File exists`. A Peer copy whose torrent names a present file follows the same rule
  under its authored hash. In a race that adoption runs before Origin is stopped: a
  refusal is a Peer failure and Origin continues, and an adopted file reports source
  Peer even when it is Origin's own copy not yet reported (Peer paths source only).

C# deduplicates nothing: each component downloads on its own, and a later publish
overwrites the file or finds it present and uses it. A component with a `Files`
name follows the same adoption rule when its name is taken after planning by a target
it is not grouped with: before its transfer starts (with no authored hash it still
transfers, to compare bytes; with one, the present file is adopted or refused at once,
with no request) or by a concurrent publish that wins the no-clobber move. An unnamed
Nexus target whose name the API reveals adopts or refuses a present file the same way
([download residuals proof](docs/parity/download-residuals-proof/README.md);
the MEGA equivalent is source only). A Peer copy for a known name that is present or
loses its move to an ungrouped publisher follows the same typed rule (source only). The plan and report JSON changes are
additive; rows with no link are unchanged. Evidence: commit `4bf5c54`, its review
follow-up `339b3d6`, a second follow-up `3f0caf4` (transitive groups, name-matched
failure inheritance), a third follow-up `d4ccbc1` (present-file adoption for unnamed
targets, no named failure for an unnamed target) and a polish follow-up (adoption
before Origin stops in a race, fall-through after a refusal, no adoption after a
cancel, adoption after a lost unnamed publish),
[shared-URL proof](docs/parity/fetch-shared-url-proof/README.md), 381/381 checks
(10 fail on `d4ccbc1`; 38 of the earlier 354 fail on `45b3c64`).
This is Fetch/nxm acquisition only. Desktop Fetch and nxm use this orchestrator from
desktop step 1 (top of this file, unreviewed).

## Provider metadata observations (H4 implementation, acceptance pending)

The shared `download_orchestrator::collect_metadata` operation now supplies metadata
for successful origin Fetch downloads to both CLI and desktop completion paths. CLI
`fetch --record` persists the observations; without `--record`, its JSON
`providerMetadata` report is read-only. Desktop Fetch records into the loaded guide
and marks it unsaved. The NXM completion path uses the same operation. Planning and
`fetch --dry-run` return before lookup. A metadata failure retains existing registry
values and reports an advisory warning. Cancellation discards pending observations.
No metadata lookup follows a failed/gated download or a peer success. GameFront
produces provider-only metadata without website requests.

Nexus metadata uses the URL path's game/mod IDs and explicit `file_id`, with a separate
optional wire type that does not change the existing discovery type. An unqualified
multi-file Nexus page is provider-only: one archive's metadata is never attributed to
all of its siblings. DeadlyStream metadata uses the bounded anonymous page session,
redirect rules, gate detection and cancellation checks already used by acquisition;
size and ISO updated date are parsed only if present. Nexus metadata bodies are
limited to 1 MiB and poll cancellation between reads; its MD5 is lowercased.
Direct identity URLs reuse the legacy URL normalization function. Unlike the C# incidental defect,
an authored changelog query is retained. MEGA metadata uses decrypted public node
attributes (safe name, size, sparse fingerprint and optional modification timestamp).
The fingerprint is never replaced with the fragment key, MAC or archive SHA-256.
Direct metadata uses HEAD with a headers-only Range fallback when HEAD is unsupported,
checks response success, uses Content-Range total for 206, validates server filenames,
and confines extra CA trust to loopback HTTPS and same-origin redirects. Direct
requests have a 20-second timeout; provider clients retain their existing bounded
request timeouts, with cancellation checked between page/API reads and operations.

Recording preserves existing key casing, nonempty authored metadata, FileSize,
MetadataHash, registry Files flags and unrelated FOMOD state. Missing values are
filled. The exact legacy migration placeholder is promoted only when ContentKey and
MetadataHash both equal SHA1(normalized resource URL) and no provider is recorded;
in that shape only, FileSize=0 is filled too. An authored copy of that exact placeholder
is indistinguishable from migration output. Other authored zero sizes and hashes are
preserved. Missing provider
fields remain absent, a deliberate difference from C# defaults of zero/empty string.
Metadata hashes are separate from verified archive content hashes. Fractional custom
metadata outside the documented provider whitelist is not part of this hash contract.
The existing CLI report/plan URL fields can contain authored MEGA fragments; do not
publish unsanitized output. New advisory metadata warnings omit provider URLs/secrets.

The MEGA dependency's reqwest feature enables its required Tokio dependency and also
compiles its native-TLS adapter. Because that feature also turns on reqwest's
`default-tls`, reqwest then selects native TLS as the builder default. Every ModSync
client builder therefore pins `use_rustls_tls()` explicitly (2026-10-03 correction:
between `f68dcfc` and that fix, production clients silently used OpenSSL with the
system trust store). MEGA requests use the shared rustls client and confined provider
transport; no native-TLS adapter is used by the provider. Production compilation/lint alone does not establish metadata protocol,
persistence, native UI or live-host parity. H4 controlled production acceptance and
F13/F22 acceptance remain open; no tests, harnesses or CI were run for this slice.

## Virtual patcher InstallList effects (E2 implementation, acceptance pending)

The shared engine now simulates bounded InstallList presence during validation,
conflict analysis and installation preflight, using existing or previously extracted
INI content, selected namespace/INI and configured destination. It writes only the
VFS, retains external-effects warnings, and does not model binary patches. Missing
payloads have an explicit unknown-content marker: virtual path operations can use
presence, while operations requiring bytes fail visibly. No unknown content can be
published by real installation. The source bounds, intentional reference differences
and unexecuted acceptance matrix are in
[`docs/parity/patcher-install-list-simulation.md`](docs/parity/patcher-install-list-simulation.md).
No patcher/native/full-install parity promotion follows from this implementation.

### Current Markdown and acquisition source changes (2026-10-02)

Markdown export now emits the reference prose layout and embedded executable
metadata through the public render/convert route. Rust DraftReview records are
retained recursively; raw structured loaders reject malformed records before
normalization. Export rejects executable metadata markers in prose and structured
guide fences that would be reinterpreted on import. Legacy bullet metadata now
has a strict parser using the ordinary contained Source normalization. These
changes have source review; complete reference text/model/corpus and execution
acceptance is still outstanding.

The acquisition race verifies Origin-revealed existing files before stopping
Peer; refused adoption leaves eligible Peer work available. Existing-file hash
reads and planning now accept cancellation. Provider-revealed filenames and
concurrent no-clobber publication conflicts retain a typed existing-file result
for safe verification/adoption. Runtime acceptance for known-name and Nexus-revealed
collisions, planning hash cancellation (Ctrl+C during planning now exits 130, like one
during the run) and Origin `transferring.total` is in the
[download residuals proof](docs/parity/download-residuals-proof/README.md). That proof
also fixed Origin totals: `total` was read after the 64 KiB sniff prefix, so it was the
remaining length (`0` for a small file), and now it is the response's Content-Length
(`null` when the server sends none). Review follow-ups: a hashed target whose name is
already present is settled with no transfer; a Peer known-name collision is typed
(`DestinationExists`, then adopt-or-refuse) instead of "Could not publish the peer
copy ... File exists" (source only); and Ctrl+C during execution-time discovery exits
130 instead of 1. The Peer race refusal path remains source only.

### Checkpoint history, desktop sharing and patcher discovery (2026-10-02)

Checkpoint capture/list/verify now have inspect/diff/provenance readers and
lock-bound component journal primitives. These are additive APIs; the default
installer still uses whole-run recovery. New pending checkpoint operations fence
legacy installation and restore. Component commits do not claim final completion
before receipt reconciliation. Managed auxiliary state, restore/resume, lifecycle
and native history remain required; see `docs/parity/checkpoint-plan.md` and the
current history/journal implementation note for crash-state details.

The desktop now owns completed-sharing startup, admission and shutdown workers.
Mutation barriers precede guide recording and install/restore access; delayed
download terminals re-evaluate the current context before acceptance. Stop/Off,
saved provenance, and controller shutdown errors are explicit state transitions.
Static review corrections and implementation boundaries are recorded in
`docs/parity/persistent-sharing-desktop.md`. Runtime transport, fairness, budget,
restart and native interaction acceptance remains deferred.

The patcher resolver has typed native/Python invocation with native-first resource
discovery. Python fallback requires installed resources, interpreter and packages;
it performs no runtime provisioning. Missing implicit resources do not block
pure simulation or unrelated installation. See
`docs/parity/patcher-backend-implementation.md` for the exact package contract and
the remaining delivery/platform limitations. The production build and strict
Clippy without desktop features passed after the initial T2/E6 integration;
later review changes still require their own production checks. These checks
do not establish package execution or full feature acceptance.

### Opt-in managed checkpoints and archive times (2026-10-02)

`install --component-checkpoints` now connects the engine to durable component
boundaries and final receipt reconciliation. `--managed` requires that mode;
explicit staging/manifest roots or the selected profile supply its auxiliary
participants. Managed reinstallation validates instructions inside the journaled
component boundary after restoring prior originals. Standalone virtual validation
does not forecast that reinstall state. Default installs retain whole-run recovery;
`--no-checkpoint` explicitly selects that compatibility behavior. Continuing after
failure (`--best-effort` or `--continue-on-error`) is rejected with ordered-recovery
guidance. The reference install flags `--skip-validation`,
`--continue-on-missing-sources`, `--continue-on-mod-failure`, `--ignore-errors` and
`-y`/`--yes` are refused by name for install, validate and conflicts. They stay hidden
from help. `set-nexus-api-key --skip-validation` is a separate option and is unaffected. Checkpoint restore/recover/resume, deletion/GC/repair and desktop history are now implemented and production-gated; native and crash/replay acceptance remains open.

Archive entries now retain decoder modification times through extraction and
engine copies; managed deployment copy fallbacks preserve source times too.
ZIP extended/NTFS times take precedence over DOS local time, 7z uses Windows
ticks, TAR/GZip use Unix seconds, and RAR uses the decoder-produced timestamp.
These source rules and timezone/precision limits are documented in
[`archive-timestamp-implementation.md`](docs/parity/archive-timestamp-implementation.md).
Cross-format/native timestamp acceptance remains deferred; checkpoint inventory
identity still consists of bytes and supported modes, not timestamp equivalence.

Desktop FOMOD prompts now retain exact captured top-level consumer GUIDs through
the editor. Accepting choices and dismissing prompts affect only those bindings;
valid existing configurations are preserved during automatic follow-up. Manual
reconfiguration targets the displayed selected component. Bound prefill is
normalized against the current archive, with an external sidecar imported as
the separately selected run-wide input. Source review passed; native acceptance
remains pending. See `docs/parity/fomod-desktop-consumer-binding.md`.

### Desktop tools, profiles and platform continuation (2026-10-02)

The desktop editor now uses shared transactions for component/option/instruction
edits, configurable regex/document previews, resource browsing and explicit safe
saves. Conflict and update windows consume shared presentations and timestamped
snapshots. Cancelled update results remain in memory; fresh metadata application
requires matching captured inputs and guide identity. Both production feature
builds and strict Clippy passed at the source boundary recorded in
`docs/parity/production-gates-2026-10-02.md`; native and provider acceptance is open.

Profiles have bounded compatibility decoding, visible legacy opaque-identity
warnings and final serialized-size checks. Typed desktop settings preserve the
24 reference keys and unknown extensions; the validated-key CLI defaults to a
cancellable provider validation before saving. Explicit settings directories never
consult the global legacy root. Full legacy date/locale, settings consumer and
platform migration acceptance remains open.

Single-instance delivery now uses bounded authenticated Unix sockets and Windows
named pipes, identified acknowledgements, bounded queues and recent request-ID
deduplication. Linux compilation is recorded; native cross-platform delivery is
unverified. Owned registration and macOS URL-event integration are being completed.

Subsequent telemetry lifecycle, CLI cleanup, macOS URL-event, merge-preview and
packaging changes are working source and require their own gates. Telemetry
configuration is bounded, diagnostics redact sensitive URL components, and signed
HTTP exports reject redirects. The package builder requires explicit source/build
receipts and reviewed patcher provenance; no native artifact, trusted patcher
payload, release publication or runtime acceptance is implied by these files.

### Further desktop, review and platform source (2026-10-02)

Owned protocol registration is now implemented for Linux, Windows and macOS,
including package/channel identity and compare-before-cleanup behavior. The
legacy Linux registration entry point delegates to those ownership checks. macOS
URL events feed the bounded shared admission route. These platform changes have
source review only; native registration, delivery and package launch remain open.

The current editor supports explicit diagnostic-by-diagnostic draft resolution
with retained original provenance and normalized authoring identities. Ordinary
edits invalidate reviews, including generated descendants in component
replacement. The CLI can prepare an unapproved resolution template; desktop
review uses the same transaction. Native usability and durable cross-format
execution are not established by this source. See
`docs/parity/draft-resolution-implementation.md`.

Private profile/settings persistence now checks ordinary files, ancestors and
current-user ownership and creates private replacement files. Telemetry config
and key loading reuse that storage boundary. Implicit default-directory migration
can read legacy configuration and signing keys without modifying them; explicit
roots remain isolated. Panic logging and telemetry use nonblocking lock attempts
and queue admission rather than reentering locks or waiting for export. Automatic
HTTP, MEGA and proxied peer download records describe attempted transfers and
maximum observed payload progress, not wire traffic or unique installed bytes.

Case-sensitive matching is available on Unix unmanaged execution, selected before
the first engine scan. Exact-mode managed/checkpoint runs and conflict reports
are refused because their persisted contracts remain folded. Physical spelling
checks reject native case aliases before a write. Virtual validation cannot prove
how a case-insensitive volume will alias two newly created names; such a later
physical failure retains the recovery requirement. Whole-run recovery is still
the default; component checkpoints remain opt-in.

Deployment management uses bounded manifests and a reviewed inventory identity
under the shared game lock. Desktop uninstall/purge require explicit confirmation
and display complete, partial, failed or cancelled receipts. Partial operations
do not become successful installation state. Native ownership/interoperability
acceptance remains open.

Packaging includes a source for manual artifact production and an offline
producer that records exact clean source, toolchain, compiler artifacts and output
hashes. It has not been run. Native ICO/ICNS assets are now retained unchanged
from the pinned reference with source and license provenance. The retained license
explicitly excludes trademark/logo rights; no separate artwork permission is
claimed. Exact backend source/license/build bindings and native package delivery
remain open. No CI run or release
was requested by adding these files. All changes in this section remain beyond
the last recorded production gate boundary.


### CLI composition source additions

Legacy GUI preload arguments now normalize into the existing GUI command without
changing modern command parsing. Convert and merge can apply a bounded spoiler
metadata overlay and an explicit download/FOMOD prephase before output. Merge
accepts named existing/incoming/output options and can render to stdout. Conversion
keeps cancellation authority through final publication. Install can default its
source directory to the input guide parent and acquire resources before engine
construction; remaining-phase acquisition excludes completed components and
verifies their definitions remain unchanged. Checkpoint resume still requires
acquisition to be completed separately before reviewing its boundary.

Validation accepts reference text/JSON output selectors, error-only text filtering,
and standalone full/dry-run selectors. Missing roots produce a structured setup
failure for JSON output; root-free validation success is not claimed. These CLI
changes have source review, with production and behavioral gates still pending.


Shared selection now uses simple Unicode case matching for component names,
custom categories/tiers and displayed search text, consistent with the existing
FOMOD name policy. Numeric tier order and GUID rules are unchanged. Unicode
runtime-version and native filtering acceptance remains open; see
`docs/parity/selection-unicode-source.md`.


FOMOD mapper fallback warnings are bounded and now appear in structured engine
validation and the choices editor before Finish. The fallback mapping rules
remain the same. Desktop managed execution uses the persisted active profile
identity independently of the profile-manager selection. Explicit guide game
metadata controls presentation theme and conditional notice routing, with the
stored theme used only as a fallback. These are reviewed source changes;
native behavior and platform acceptance remain open.

Installation acquisition adds the reference key alias/environment precedence,
sequential default with explicit concurrency, and an owned 48-hour default deadline.
The publication check enforces elapsed time independently of timer scheduling.
Convert/merge selection, local inference, legacy convert-merge spelling, and
FOMOD mode controls are wired through shared adapters. Unresolved FOMOD consumers
still prevent composed processing. The reference Python.Included bootstrap verb
returns an explicit unsupported error because this Rust application has no such
runtime; no dependency installation or no-op success is implied.

## Current source: process ownership, provider cancellation and desktop binding

Engine Execute/Run/Patcher and standalone HoloPatcher now share owned process
execution. Linux/macOS retain the unreaped group leader through cooperative group
cleanup; Windows retains the suspended-launch Job Object until owned processes
terminate. Cancellation and execution timeouts do not abandon cleanup or release
the engine recovery lock while membership remains uncertain. Cleanup may wait
indefinitely on OS failure. Captured streams are bounded; engine patcher-log and
success-only managed reconciliation rules remain. See
`docs/parity/engine-owned-processes.md` and
`docs/parity/standalone-patcher-desktop-source.md`.

Nexus acquisition/discovery now use bounded cancellable response intake, including
per-race cancellation and cancellable shared-client waits. Deferred rate limits
from optional metadata propagate to the retry owner. Origin diagnostics strip
attached provider URLs; authored registry/manual-action fields retain their data
contract. See `docs/parity/download-transport-diagnostics.md`.

Desktop settings apply the selected backend's own stored executable from a paired
snapshot. Preference saves validate and return the exact persisted pair under the
settings lock. Desktop File/Open uses the shared installation guide loader. The
Windows recovery lock rejects reparse-point handles. These changes have source
review; production gates and native/runtime acceptance are separate. Windows checkpoint persistence was subsequently implemented under the limited
process-interruption contract described below; Unix durability receipts do not
establish Windows support.

### Patcher INI preparation (2026-10-02 source boundary)

Every Patcher installation now uses an owned, bounded, cancellable payload copy. Normal, numeric namespace and named-INI routes preserve original configuration bytes. Reference logging/game-folder/confirmation replacements apply only inside staging; the VFS path remains read-only. Named selection preserves actual directory spelling and removes both supported namespace-file locations. Strict invalid-Unicode rejection and explicit size/reparse limits are deliberate safety constraints. See `docs/parity/patcher-ini-preparation.md`; native backend acceptance remains deferred.

### Windows checkpoint execution (2026-10-02 integrated source boundary)

The local fixed-NTFS process-interruption capability now includes persistent named-job ownership for checkpointed external processes. LaunchIntent precedes JOB_LIST process creation; normal completion requires the owned job to drain. Independent process fences block ordinary checkpoint/deployment/legacy mutations and are settled only through explicit, identity-validated recovery. CLI/desktop expose read-only status and preserve partial recovery outcomes. The former blanket Windows engine block was removed after separate ownership/security and combined integration reviews. The guarantee is limited to same-user, same-logon/session process restart while the OS/filesystem remain running; OS/power-loss and native acceptance are unproven. See `docs/parity/windows-checkpoint-job-quiescence-plan.md`.

### Desktop presentation completion (2026-10-02 source boundary)

Aspyr and widescreen warnings display literal text while Preamble retains Markdown. Help/About/Other Projects restore the pinned reference destinations. The registered-resource editor now has bounded hierarchy and multiple source selection with explicit variable-path projection and draft confirmation. Saved update results rehydrate read-only after settings load, and provider checks retain implicit/explicit key provenance. The log surface exposes typed severity/time rows through nonblocking snapshots with exact-snapshot save and selected copy; raw activity remains separate. Native interaction/provider acceptance remains open. See `docs/parity/editor-resource-browser.md` and `docs/parity/structured-log-and-update-rehydration.md`.

### Windows archive dependency build correction

The local production-only `unrar_sys` 0.5.8 patch selects source units and system libraries using the Cargo target instead of the build host. It includes upstream `isnt.cpp` for Windows and corrects Windows header casing on case-sensitive build hosts. Original decoder/FFI behavior is retained; tests and disabled self-tests are excluded. Provenance and binding/UnRAR license boundaries are documented in `vendor/unrar_sys/README.md`. Cross-linking and native archive acceptance are separately recorded; a source review alone does not establish either.

### CLI settings provenance (2026-10-02)

Install, validation and conflicts load stored backend preferences while preserving
explicit engine/path overrides; a stored executable is used only for its matching
engine. Nexus acquisition consistently resolves explicit key, then stored key,
then supported environment fallback. Implicit default settings retain legacy
fallback; explicit settings directories remain isolated. Unrelated host work does
not load an implicit Nexus key. Production compilation and static review cover
these paths; invocation/provider acceptance remains deferred.

### Resource observation and package receipt corrections (2026-10-02)

Observer pauses, changed contexts and fresh baselines clear desktop validation,
conflict results and install confirmation. Read-only update work retains
observation. In-flight validation uses the observation epoch to reject stale
results; install/download identity is preserved across expected observer pauses.
The finished-worker event slot is drained after joining. Metadata observation
remains advisory; it does not establish file-content integrity.

Package assembly now requires a successful producing receipt with matching clean
source boundaries, lockfile and artifact hashes. Windows resource receipts bind
the current RC template and rendered source, and package/resource versions must
agree. Packages retain the vendored UnRAR source-provenance and license notices.
These are reviewed production-source corrections; no package was assembled and
no native acceptance is implied.

### Desktop NXM preference and confirmed registration

The desktop consumes the stored NXM preference on Linux/Windows through owned
background registration work. Already-owned registration is a startup no-op;
foreign handlers are preserved. Explicit replacement binds the displayed
scheme/state/handler to a fresh check under the shared platform mutation lock.
The lock coordinates ModSync instances, not external OS tools, and the displayed
handler string is not a file-content fingerprint. macOS removal requires explicit
whole-bundle confirmation. Preference-only disable does not touch native handlers.
Successful native changes remain reported if the separate settings merge fails.
The merge preserves original settings-directory provenance and unrelated fields.
See `docs/parity/desktop-protocol-preference.md`. Native acceptance is deferred.

### Profile date and desktop/CLI action compatibility

The profile reader additionally accepts confirmed Newtonsoft.Json 13.0.4 ISO
forms: hour-only/compact numeric offsets, lowercase UTC marker, up to seven
fractional digits and zero-only midnight24 rollover. Valid dates and the year9999
boundary remain checked. Authored strings retain their existing representation;
broader invariant-culture DateTime parsing and local-zone normalization are not
closed. See `docs/parity/profile-date-compatibility.md`.

Component/option removal from keyboard and button routes now requires a dialog
bound to current guide/component identity, with Cancel initially focused. Selected
application-log records have an independent highlight toggle via button or focused
H shortcut. Validation output selectors accept case-insensitive text/json values.
These are source corrections, with native and reference-invocation acceptance
still open.

### Selection saving, raw IDs and validation setup errors

Legacy Save Selection uses the owned background worker and bounded FOMOD sidecar
loading, preserving completed component definitions. A stale successful save
reports its output without clearing current unsaved records or promoting current
sharing provenance. The raw document UUID helper generates a separate value for
copying and never rewrites component IDs. Drop confirmation supports Cancel,
Escape/backdrop dismissal and prior focus restoration; Help includes explicit
issue-form and sponsor links.

Ordinary all-phase validation rejects a zero effective top-level selection before
pipeline construction. JSON output therefore uses the setup-error schema and
exit status1; install, conflict and phased-validation paths retain their own
selection handling. These changes have source review; native/CLI behavior
acceptance remains deferred.

### Current-culture settings numbers

Production CLI settings setters now obtain native numeric symbols before taking
the settings lock. Linux/macOS use system ICU; macOS includes its native locale
fallback. Windows uses NLS user-locale data with user overrides. The bounded
parser preserves integer-first precedence, native decimal/group/sign syntax,
floating negative zero, and explicit rejection of nonfinite JSON values.
Whitespace-value removal requires no native locale provider. The previous public
en-US helper remains intact for compatibility callers.

Windows NLS data is not claimed identical to every .NET ICU/NLS configuration;
app-local ICU, custom managed cultures and native locale acceptance remain open.
See `docs/parity/settings-numeric-culture.md` for source contracts and bounds.

### Editor section controls and archive-generation preview

Five editor sections provide Expand All/Collapse All and begin collapsed. Local
archive instruction inference now has a worker-based preview for components and
nested options. Authors explicitly choose append or instructions-only replacement,
review the candidate/diagnostics, and adopt into the draft before the existing
Apply transaction. Shared inference preserves prior review provenance and leaves
generated instructions requiring review. No archive is copied to the mod directory
and no installation or guide save occurs during generation. Provider filename
discovery is also exposed through explicit preview: incomplete discovery rejects
the candidate and archive bytes are never downloaded by this action. Native
behavior acceptance remains open.

Provider generation is unavailable in isolated probe launches, with a worker-side
check before discovery as well as the UI gate. Editor and shell worker handles
remain owned through completion and close; a queued terminal result survives a
subsequent thread failure. Picker cancellation still waits for dialog dismissal.

### CLI directory arguments

Install now reaches its existing input-guide-parent fallback when --source-dir
is omitted; -g and -s are accepted for game/source directories. Profile path
precedence and mode-specific dispatch checks remain intact.

### Basic validation, conversion defaults and legacy settings continuation

Ordinary validation with either effective root absent now runs a dedicated
read-only component report followed by the reference's failed FOMOD stage. It
does not initialize the engine or invent roots. Literal paths receive bounded
ordinary-file checks; archive membership retains spelling and distinguishes
explicit directory entries from inferred file parents. Cancellation aborts the
report. CLI setup errors retain the JSON report boundary, and both rooted and
rootless validation preserve authored all-false selection until explicit
selection/default policy is applied. Empty effective selection remains an error.
(Superseded 2026-10-04: without a profile both now load as the C# file loader does,
which selects every component of a non-Markdown guide that selects none; see "File
selection when the file selects nothing".)

The reference's guide/directory autofixes are deliberately not reproduced during
validation. The model also cannot recover its null-versus-empty Source distinction.
Complete-root validation retains the existing Rust engine extension. See
[the basic report contract](docs/parity/basic-validation-without-roots.md) for
bounds, exact scope and remaining differences. This is source implementation and
review; invocation/runtime and full F08/F26 acceptance remain deferred.

Convert and merge now default to TOML regardless of the output filename, matching
the pinned CLI's explicit format default. Explicit output formats are unchanged.
The shared Nexus key getter imports a bounded private legacy nexusmods.config
only when its own effective settings key is blank, under the settings lock.
Desktop settings loading calls the same getter. Malformed or unsafe keys remain
errors without exposing their contents. See
[the migration contract](docs/parity/legacy-nexus-key-import.md).

The profile decoder additionally accepts the pinned Microsoft-date offset
grammar, retaining authored date strings. Host-local canonicalization and the
broader invariant-culture DateTime fallback remain separate source work; see
[the date contract](docs/parity/profile-date-compatibility.md).

### Explicit maintenance and rooted FOMOD choice scope

The case and permission commands now share explicit preview/apply/status/recover
with the maintenance library. Linux source refuses unsupported object/security
state, binds a reviewed digest to actual roots/content/identity, uses native
exclusive rename and owner-only permission additions, and journals inverse
operations before changes. Interrupted cleanup retains recoverable terminal
state. Ordinary install/checkpoint/deployment writers are fenced while maintenance
is pending. CLI outcomes and the desktop's owned worker retain recovery status;
sharing needs explicit resume after mutation. The next source slice adds a
Darwin profile with pinned native ACL/flag checks and exclusive rename, retaining
stable capability admission and old Linux intent compatibility. Windows mutation,
Unicode mapping and native interruption/full F24/F25 acceptance remain open.
See [maintenance authority and limits](docs/parity/maintenance-contract.md).

Rooted phase preparation and Save Selection use actual archive members to scope
external choices when a directory itself has an archive suffix. Ordinary
folder/glob sources retain lexical ownership, including archives produced by
earlier VFS instructions. Registry ownership, completed-prefix exclusion,
cancellation and atomic overlay publication are shared. A registered missing
archive without recorded prompt state now has a bounded soft warning; recorded
prompt state retains its hard error. Pure rootless APIs retain their prior
contract. Corpus/native acceptance remains open; see
[the rooted-choice contract](docs/parity/fomod-root-aware-choices.md). Dependency
expansion and runtime proof are in "FOMOD gate scope and residuals".

### Durable maintenance and acquisition coordination

Maintenance publishes a bounded sibling mod-root fence before tree effects.
Terminal records bind the final/original inventory and optional outcome
disposition. Cleanup clears the selected target's fence last, preserving
game-only or mod-only admission after interruption. Explicit terminal recovery
verifies that tree and removes metadata; it does not reverse a completed apply.
Fence-only mod cleanup contains no content-write authority. The owned sibling
namespace restriction and native acceptance limits are described in the
[maintenance contract](docs/parity/maintenance-contract.md).

Orchestrated transfers retain the same root lock through workers and revalidate
planning-time existing files against current guide/resource/hash/integrity
authority. Standalone transfers can bind nested outputs with `--source-dir`;
otherwise they guard only immediate output parents. Worker/retry limits,
cancellable keyed execution grouping and direct torrent metadata cancellation
are implemented. These are cooperating-writer locks and path preflight, with
external namespace races and native behavior still unverified. See
[acquisition coordination](docs/parity/acquisition-root-coordination.md).

The backend producer now has source for an explicit locked offline OdyPatcher
CLI build receipt, exact root-package/compiler-executable binding and retained
source/license/artifact digests. It is nonhermetic, has not been invoked and
does not admit a bundled payload. The provenance allowlist is still empty;
dependency/source obligations, actual producing review and installed acceptance
remain open. See [package construction](packaging/README.md).

### Native maintenance, versioned casing, date serialization and acquisition continuation

Windows maintenance has a separate local fixed NTFS source profile. It retains
full 128-bit file identity, current effective ownership, owner/group/DACL hashes
and admitted attributes. Named data streams are refused. Readonly repair and
exclusive rename use pinned handles; new metadata has a protected owner-only
DACL at creation. Partial-tail recovery requests truncation rights and every
append verifies EOF. Native execution/interruption remain unverified; see
[the Windows contract](docs/parity/maintenance-windows-contract.md).

Maintenance now records an optional invariant simple lowercase profile with an
exact Unicode 15.0 source/table digest. Preview and persisted recovery derive
the same operations from that mapper. Absent metadata retains legacy ASCII-only
admission, and unknown profiles or unassigned scalars are refused. The exact
scope and native ICU/NLS limitation are in the
[Unicode maintenance contract](docs/parity/maintenance-unicode-contract.md).

Shared FOMOD/selection/authoring name keys use the pinned simple-uppercase
table and .NET 9 ICU ordinal dotless-i/long-s exceptions, replacing Rust full
uppercase heuristics. No normalization or expansion is applied. Newer datasets
and every Windows NLS version remain separate acceptance boundaries; see
[ordinal matching](docs/parity/ordinal-name-matching.md).

Profile UTC and timezone-free dates now canonicalize with reference-style
100-nanosecond precision, trailing fractional-zero trimming and Kind-specific Z
suffixes. Fast parser dispatch uses the original string. A bounded immutable
Unix timezone snapshot supports historical and admitted future conversions,
retaining DST folds and reselecting the serialization offset. Unsupported rules
retain authored text with warnings. Windows local conversion, adjacent-year
stitching, complex boundary dates and broader framework fallback remain open; see
[date canonicalization](docs/parity/profile-date-canonicalization.md).

Planning, discovery and execution now share the same bounded cancellable keyed
destination grouping. Stale peer cleanup removes only canonically named empty
old ordinary directories; nonempty contents are retained for inspection, and
all Windows reparse points are excluded. These changes passed independent source
review. No runtime, corpus or native acceptance is claimed by this section.

### Telemetry endpoint migration and first-run consent continuation

The shared OTLP endpoint projection accepts a configured base or a literal final
traces/metrics signal URL, preserving prefixes and building both signals from
the same admitted base. Ambiguous encoded/case-varied terminal spellings,
credentials, query/fragment and oversized endpoints are explicit refusals with
URL-free diagnostics. The desktop now has an owned first-run consent choice
that persists allow/decline through matching settings and lifecycle receipts.
Load-error defaults are not first-run authority; failure leaves the prompt
retryable and event admission disabled. Readback checks the saved choice, and
logging failure is separate from configuration failure. See the current section
of [the telemetry audit](docs/parity/telemetry-csharp-audit.md). Source review,
production compilation and native acceptance remain separate gates.

### Profile local-zone source continuation

Unix local normalization now uses shared checked calendar primitives, effective
historical/future rule selection, neighboring-year marker seasons, daylight
day transfer and special endpoint lookup. The Windows provider captures bounded
read-only native/registry data twice, then applies pure ordinary annual rules
with fixed/floating transitions and retained folds. Both providers reselect the
serialization offset and keep checked shifts separate from endpoint clamping.

Windows marker stitching, shifted cross-year windows, extreme daylight rebasing,
adjusted endpoint rules and fallback zone discovery remain explicit authored-text
refusals with bounded warnings. Broader invariant parsing/rounding is incomplete.
See the [general date contract](docs/parity/profile-date-canonicalization.md) and
[Windows date contract](docs/parity/profile-date-windows-contract.md). Independent
source review accepted the implemented subset after correcting local negative
daylight ambiguity intervals; native behavior is unverified.

### Shared merge casing source continuation

Merge name/author, instruction and option keys now use the pinned Unicode 15
invariant simple-lower operation. Destination/argument preservation and Language
union use the separate ordinal-ignore-case mapping, with streaming equality
and retained first spelling. Maintenance and FOMOD reference the same compiled
tables through policy wrappers; maintenance descriptor admission and generated
bytes are unchanged. Later Unicode/NLS, URI/IDN parsing and full merge corpus
remain separate boundaries. See [the merge casing contract](docs/parity/merge-unicode-casing.md).
This implementation passed independent source review; behavioral acceptance is
unverified.

### UTF-16 merge punctuation and deterministic date syntax continuation

Merge punctuation now uses .NET 9's pinned Unicode 15 General_Category over
BMP code units, preserving supplementary punctuation as the reference char
iteration does. A retained production generator/table/provenance replaces the
regex category dependency; existing casing descriptors and table bytes remain
unchanged. The exact HTTP(S) loopback alias projects to localhost; full Uri.Host,
IDN, port and custom/file-scheme behavior remains a separate source boundary.
See the merge casing contract for the dataset and reference anchors.

The profile fallback adds explicit weekday and AM/PM handling plus qualified
hash/NUL envelopes, retaining separate specialized and ISO routes. The Unix
future parser now admits standard components outside fourteen hours when its
validated effective daylight total and normalized delta qualify. Refusal of
out-of-range effective totals remains stricter than the reference's normalization
and is documented. These are source changes with independent review; the
production receipt records the final combined boundary and remaining limits.

The same reviewed date extension includes terminal GMT, lowercase strict ISO
`t`, homogeneous numeric dash dates and one terminal weekday. Windows discovery
now retains failed direct-load observations, native first-match registry order,
the reference's disabled-flag distinction, native enabled/disabled retries and
qualified UTC fallback. Two complete bounded decision observations and repeated
native state must agree. Operational, unsupported and unstable intake retains
authored text with warnings. Alternative IANA-ID mapping remains separate. See
the Windows contract for types, budgets and exclusions.

### Effective date offsets and qualified numeric merge domains

Unix historical and future rule construction now share initial one-day daylight
transfer and delta validation. An effective total outside fourteen hours shifts
the standard component into range and reconstructs the rule, applying the
reference's second single-day transfer only when clamping occurred. Fixed
future offsets use the same normalization. Invalid initial deltas remain
unsupported before clamping; broad malformed TZif intake remains separate.
See the profile date contract for source anchors and arithmetic bounds.

Merge now canonicalizes qualified HTTP(S) IPv4 forms into four decimal bytes
and standard bracketed IPv6 through the pinned compression/embedded-address
decisions. The IP adapter checks its bounded port grammar and userinfo separator
count. Invalid or unsupported inputs retain the existing DNS/manual fallback;
this correction does not establish full Uri.Host admission equivalence. Scope,
prefix, Unicode IRI/IDN and custom/file schemes remain separate. See the merge
contract for exact radix, word and source policies. These increments received
source review separately from parent production gates and behavioral acceptance.

### Format-aware intake and alternative Windows timezone IDs

Known guide properties now follow explicit TOML, JSON, YAML and XML access
policies, preserving raw review fences before canonical projection. Ignored
executable aliases cannot acquire meaning through conversion. Instruction
Source filtering and literal-property preprocessing remain distinct. See
[the property contract](docs/parity/format-property-access.md) for duplicate,
option, resource and scalar boundaries.

Windows alternative timezone-ID lookup uses a bounded, qualified combined
System32 ICU module with repeated configuration/data/mapping witnesses. Semantic
no-map continues native fallback; operational or unsupported observations retain
authored text with a warning. App-local/split ICU and unobserved AppContext
configuration remain outside this profile. Both changes have independent source
review and the production gates recorded in the current receipt; native and
behavioral acceptance remain open.

### Structured Action, profile GUIDs and guide input

Instruction intake now implements the pinned signed Action enum grammar,
including numeric values, composite names and raw-Choose/parsed-Delete ordering.
Unknown instruction properties are discarded. Its bounded getter profile and
culture-sensitive collection exclusions are recorded in
[the Action contract](docs/parity/instruction-action-intake.md).

Profile GUID normalization now shares the pinned .NET character parser, including
ordinary forms and qualified D/X compatibility cases. Guide IDs and checkpoint
proofs are not rewritten. See
[the GUID contract](docs/parity/guid-parsing-contract.md).

Public guide strings are bounded before BOM handling or autodetection. Terminal
XML/JSON errors retain their causes, and qualified UTF-8 stdin retains decoding
replacement characters. File ingestion keeps its separate underscore policy.
See [the input contract](docs/parity/guide-input-contract.md).

Qualified HTTP(S) Unicode DNS host comparison now validates original UTF-16
label weights and retained Unicode 15 assignment, strips the reference Bidi set,
normalizes NFC and applies pinned lowercase. Empty resulting hosts do not earn a
domain-match score. Unsupported authorities retain the existing manual fallback;
full URI admission and native/corpus acceptance remain separate. See
[the merge contract](docs/parity/merge-unicode-casing.md).

### Boolean intake, JSON shapes, GUID merge values and CLI handoff

IsSelected and surviving literal Overwrite fields now use the pinned optional
Boolean getter. XML distinguishes attribute-free selfclosing Boolean leaves
from explicit empty text in actual consumed model containers, restoring candidate
values in opaque metadata. Selection defaults and action-dependent Overwrite
presence remain separate. See
[the Boolean contract](docs/parity/instruction-boolean-intake.md).

JSON ignores nonobject top-level component entries after raw review checks, with
one aggregate advisory, while retaining object errors and empty-guide refusal.
Merge compares known GUID values and performs target-first set unions without
rewriting emitted spellings or persisted identities. Empty source fields remain
untouched. See [guide input](docs/parity/guide-input-contract.md) and
[merge GUIDs](docs/parity/merge-guid-contract.md).

Install/conflicts reject all six validation-only flags before preparation. Bare
protocol URLs reach the existing GUI/handoff path through shared bounded pure
qualification; extra arguments and invalid qualified requests receive redacted
errors. Explicit inspection remains available. See
[the CLI contract](docs/parity/cli-validation-handoff-contract.md). These changes
have independent source review; native/behavioral acceptance remains separate.


### Cancellable protocol guide loading

Protocol guide loading now has an additive cancellation-aware library API and
uses real CLI stop flags. Guide-opening entry points share the 8 KiB handoff
bound; direct HTTP fetch retains its existing URL intake. CLI cancellation errors
return 130, with completed publication/ready success retaining existing priority.
The desktop owns and joins its guide worker, discards cancelled or stale results,
and retains run/input/token identity through sharing acknowledgement and close.
See [the protocol loading contract](docs/parity/protocol-load-cancellation.md).
Independent source specification and quality reviews accepted these scopes;
runtime/native and FIFO/backpressure acceptance remain separate.


### Bounded component text conversion

Component Name/Author/Description/Directions/Tier and option Name/Description
now use the qualified scalar getter; Category/Language separate scalar trimming
from array-item preservation and bounded collection cleaning. Negative collection
integers use one lazy captured native sign. The represented JSON UInt64 range
above Int64 defaults optional Boolean/text conversion as Newtonsoft BigInteger.
See [the text intake contract](docs/parity/component-text-intake.md) for generated
text/work bounds, floating/type-name limits and the remaining YAML/JSON parser
origin boundaries. Independent source review accepted this slice; broader
conversion and behavioral acceptance remain open.

### Bounded protocol inbox and FOMOD metadata inspection

Pending desktop Modsync links now use a FIFO instead of one replaceable slot.
The combined Modsync/NXM pending capacity is 64; transport intake stops before
receiving when full and processes at most 64 messages per frame. macOS alternates
transport preference. Ordering is per transport and protocol queue, without a
claimed total order across sources. Closing stops intake and warns that pending
links will be discarded. The queue is not durable. See
[the inbox contract](docs/parity/protocol-inbox.md).

Module/info XML inspection now reads verified spools in cancellable 64 KiB chunks
with an 8 MiB per-document ceiling and strict UTF-8. XML event processing retains
the depth rule and adds 100,000 node/attribute ceilings and a 32 MiB cumulative
logical string budget, including ancestor text copies. Semantic mapping checks
cancellation at its boundaries. Engine extraction passes its actual cancellation
flag. These are explicit Rust admission limits; metadata parsing does not grant
installation permission. See [metadata bounds](docs/parity/fomod-metadata-bounds.md).
Production and behavioral evidence are recorded separately in the gate receipt.

### Bounded cleanup and filename inventory

Cleanup now retains original selected names, uses pinned invariant token casing
and ordinal comparisons, and recognizes all five BOM profiles with replacement
semantics independent of guide intake. It decodes and qualifies all selected
paths before deleting files. DelDuplicate counts compatible extensions but
examines every immediate candidate, including extensionless names when Arguments
is empty. Its count is not decremented while planning. Rust stops on deletion
failures and retains recovery instead of continuing a guide after a failure.
The bounded policy and primary sources are in
[the cleanup contract](docs/parity/engine-cleanup-contract.md).

Filename selection now uses one lazy, read-only inventory per planning or
discovery invocation, with cumulative work/storage limits and cancellation.
Registry names remain component-specific; there is no GUID memoization. Shared
ancestor qualification rejects unsafe paths and Windows reparse points. Local
qualification failures propagate rather than authorizing unnamed provider
fallback. Inventory names are advisory and do not prove archive integrity or
publication eligibility. Explicit candidate duplicates and inferred first-spelling
ordinal uniqueness are retained. Existing wildcard/URL lookup policies are
unchanged. See [inventory bounds](docs/parity/download-filename-inventory.md).

### YAML default-object scalar and alias intake

Structured and embedded Markdown YAML now retain decoded style/tag/key context
before projection. Untagged default object scalars remain strings under the
pinned YamlDotNet reader; plain nonkey null spellings and explicit null tags keep
their separate rules. Explicit Boolean, Int32 and finite Double tags use qualified
reference conversion; nonfinite values, timestamps, null keys and complex map keys
have explicit representation refusals. Valid typed Boolean/integer/float keys load
as text keys, matching the reference's acceptance (2026-10-03). Ordered exact-key replacement and
anchor rebinding preserve their recorded reader policy. Forward/cyclic aliases
fail; acyclic expansion is bounded with budgets shared across guide documents
and Markdown blocks. No merge-key expansion is introduced. This replaces plain
scalar inference but does not close all native YAML type contracts. See
[the YAML intake contract](docs/parity/yaml-scalar-intake.md).

### Guide lastModified metadata

TOML and YAML saves now write guide `lastModified` metadata the way the
reference does. The reference reads it with invariant `DateTime.TryParse` and
`DateTimeStyles.None`. YAML gets the `O` pattern with the host offset for zoned
input. TOML gets a native local date-time truncated to milliseconds.

- Values the reference cannot parse are omitted. These include empty,
  digit-free and lone-integer text, invalid weekdays and offsets over 14 hours.
- Forms outside the shared bounded date model keep their authored value with a
  warning. Examples are time-only values, impossible dates, `24:00` and
  unsupported zone rules.
- Key choice follows the reference. TOML reads only `lastModified`, so a
  TOML `LastModified` key is omitted with a warning. YAML reads
  `LastModified` first, with no fallback when it fails to parse.
- Native TOML date-times now round-trip as date-times in TOML and are
  written as date-time text in YAML.
- JSON, XML and Markdown do not carry the field in either implementation.
- The saved bytes depend on the host. Zoned values are converted to the host
  time zone, including its DST rule on that date, as the reference does.

Proof: a convert differential against the live reference across three time
zones. See [the lastModified contract](docs/parity/lastmodified-contract.md).

### Ordinal path identity across engine, deployment and conflicts

Case-insensitive path keys in the engine virtual filesystem, managed process
capture, the deployment service (manifest admission, ownership and foreign
indexes, patcher provenance diffs, existing case variants on disk) and conflict
write histories now use the shared pinned .NET `OrdinalIgnoreCase` mapping
instead of Rust Unicode lowercase. This is the comparer the reference uses for
its VFS, `DeploymentService` and `FileConflictAnalyzer`. For example final
sigma `ς` and `σ` are now one path, and a dotted capital `İ` no longer equals
`i` followed by a combining dot. Conflict reports sort destination paths by OrdinalIgnoreCase UTF-16
order, so `A_B` follows `AAB` as in the reference. Out-of-order uninstall
backup lookup now matches paths by the same identity rather than ASCII-only
case folding.

Persisted manifests and indexes keep their stored spelling; only the in-memory
key changes, so no migration is written. A manifest whose paths were distinct
under lowercase but equal under ordinal casing is now refused on load as a
duplicate deployment path, rather than tracking two names one Windows volume
would treat as a single file. Checkpoint trees use the same ordinal key for
their case-alias check, matching `CheckpointService`'s OrdinalIgnoreCase file
map: names that differ only by Unicode normalization (composed vs decomposed)
are no longer aliases, and ordinal-only pairs such as `ς`/`σ` now are.
Auxiliary participant bindings, a Rust-only overlap guard with no reference
counterpart, keep the stricter NFC lowercase rule. Glob matching keeps the glob
crate's case folding. Source only: production fmt/build/Clippy passed; no tests, runtime or
CI ran, and behavioral acceptance stays open.

### Runtime revalidation, 2026-10-03

Re-running the retained production CLI proofs against `main` `73e8819` found five
regressions introduced by later source work; all are corrected. Evidence and the
fresh C#/Rust K1/K2 differential are in
[runtime revalidation](docs/parity/runtime-revalidation-2026-10-03.md).

- **TLS backend.** The MEGA crate's `reqwest` feature turned on reqwest's
  `default-tls`, so every ModSync HTTPS client silently used OpenSSL and the system
  trust store instead of rustls. All client builders now pin `use_rustls_tls()`.
- **FOMOD prompt status key.** A fresh status was written under camelCase
  `handlerMetadata`, which the next load canonicalized to `HandlerMetadata`, so a
  no-op or failed `--record` run still changed guide bytes. New entries use the
  canonical spelling; an authored camelCase-only entry keeps its spelling.
- **FOMOD choices errors.** Read and parse failures again say `Reading FOMOD choices`
  or `Parsing FOMOD choices` with the path, for Fetch, validate and install.
- **Draft review reload.** Rust's own YAML and XML exports of drafted guides failed to
  reload (`DraftReview.RequiresReview must be a boolean`) because both formats carry
  scalars as text and XML omits empty lists. The review record's Rust-written typed
  spellings are restored before validation; anything else still fails closed.
- **YAML typed fields.** Under the pinned YamlDotNet string profile, YAML
  `WidescreenOnly`/`AspyrExclusive`, resource `FileSize`/`ContentLength`/
  `SchemaVersion` and `UpdateAvailable` arrived as text, so desktop/install checks
  read `WidescreenOnly: true` as false. They are now typed as XML already did
  (`UpdateAvailable` also for XML). Invalid Boolean text is an error.

One adjacent correction: H4 metadata collection skipped nothing for a different-URL
follower, so `--record` could store the headers of a URL whose bytes were never
downloaded. Such followers now get no metadata lookup.

## Easy-mode authoring continuation (2026-10-09)

The `easy` CLI exposes durable authoring sessions: `create`, `status`, `edit`,
`undo`, `redo`, `export`, and `predict`. Accepted edit transactions automatically save an
immutable guide revision and advance the saved cursor. Undo and redo affect the
guide only. Separate installed-action undo/redo restores filesystem snapshots.
Sessions retain their
exclusive lock while open. The desktop editor has a separate easy-mode surface
for these sessions, a predicted folder browser, and an action recorder. Disk
work runs in the existing editor worker. Patcher prediction still marks patched
content as unknown; it does not simulate actual 2DA/GFF/TLK edits.

`easy diff` hashes two ordinary trees and compares exact paths, file bytes, and
directory presence. It refuses links and special files, detects files changing
while hashed, and reports every explicit relative exclusion; there are no
implicit exclusions. Compare quiescent lanes after each mod. This is comparison
infrastructure, not proof of K1/TSL parity.

New engine settings, unconfigured profiles, and new desktop instances default
to OdyPatcher. Explicitly saved backend preferences are retained. The default
execution path uses the pinned library; explicit executable settings retain the
subprocess path. Read-only
inspection uses the pinned OdyPatcher library at `9b9ee8d1` to show descriptions,
namespace choices, and configuration summaries. Inspection of a predicted
authoring point projects known configuration bytes into private scratch storage;
it refuses unknown bytes and bounds document sizes. The production CLI inspected
the actual Character Start Up Changes payload and reported its three 2DA changes
without a configuration blocker. This does not establish patch-byte parity.
The `easy apply` command and desktop action button execute the next recorded
action in guide order through a single-action component journal, with a legacy
recovery snapshot and before/after content-addressed checkpoints. `disk-seek`
restores saved bytes without replaying actions; original checkpoints retain redo
ancestry. Every restore saves current files first. The authoring journal records
pending actions and restores durably; `disk-recover` settles only its owned
single-action checkpoint. Actions interrupted before their checkpoint identity
was saved require recovery of the configured roots before journal reconciliation.
The reconciliation path holds the root lock and refuses legacy recovery fences,
pending checkpoint operations, and unfinished install sessions; it never
executes the pending action. The reconciliation change passes the required production build gates. These new
paths have source/build evidence; actual installation and undo/redo evidence
remains pending. The browser now implements Explorer details and a folder
sidebar, Dolphin icon tiles, and Finder ancestor columns. All three rendered
against the real K1 predicted inventory; Finder traversed the extracted folder
without writing it. Dolphin's missing icon glyphs were replaced with drawn
folder/file icons; the replacement has build evidence, with rendering pending.
Easy mode exposes an autosaved mod
selection checkbox; importing an unselected guide does not silently select it.
Production lib/bin builds and strict Clippy pass with and without default
features. Formatting and whitespace checks pass. The isolated desktop
renders easy mode, reopens the K1 authoring session, and records/autosaves its
first archive extraction action through the panel. No disk install is claimed.
Its first prediction
rejects case collisions in the existing archive cache's extracted directories;
new runs need clean source folders. An isolated AT-SPI bus with its own registry
now exposes the application and its visible controls; invoking the easy-mode
button through its accessibility action succeeded. Full screen-reader behavior
remains unverified. All three prototype K1 lanes match the historical reference
by exact paths and bytes, but that reference is not a verified stock baseline.
An audit against the cached Steam-client depot 32371 manifest
4122010938027300909 found 16 mismatched stock WAV files, user saves and extra
paths, and a saves-directory case difference. Preserve that reference; these
copies establish transport equality only. Fresh manual and automated installs
must wait for verified stock files and an NVMe workspace with enough capacity.
The HDD-backed ext4 prototype workspace does not satisfy the active NVMe
constraint. The first recorded extraction was cancelled during checkpoint
capture. The desktop reports cancellation, no extracted folder exists, and
no action or checkpoint session was committed. Its durable authoring journal
still requires recovery because no checkpoint identity was published.
Authoring progress
updates now also reach the status line displayed while the worker is busy.
The debug profile now optimizes the SHA-256 dependency used by full-tree
comparison and checkpoint hashing. The application remains unoptimized for
debugging. Build verification and measured runtime improvement for this profile
change initially remained pending; the profile now passes production lib/bin
builds and strict Clippy with and without default features. Measured runtime
improvement remains unproven; the cancelled extraction used the prior binary.
Authoring baseline capture and post-action capture now forward checkpoint file
and byte counts to the desktop/CLI progress observer. That change passes all
production gates with and without default features. Easy-mode directions now
open by default for each mod, and prediction, patcher inspection, and disk
actions honor the GUI's scratch folder and expanded-archive budget. Those GUI
changes pass the default build, strict Clippy, formatting and whitespace checks;
the CLI sources are unchanged. Runtime verification remains pending; the cancelled action's recovery journal
must be reconciled before further disk actions.

Authoring prediction now has a read-only checkpoint inventory source. After a
session records its first disk action, browser prediction and patcher inspection
start from that action's verified before-checkpoint, retaining logical install
paths while reading stored object bytes. Pending action/restoration journals
fence prediction. This avoids replaying applied moves/deletions against mutated
roots. The checkpoint is selected from validated committed ancestry and must
belong to the saved action with no auxiliary participants. No HEAD is changed
and no files are restored. The change passes production builds and strict Clippy with and without desktop
features; actual checkpoint-backed prediction remains runtime unverified.
Registered FOMOD inspection now checks the virtual inventory and opens its
content bytes, matching the existing Extract inspection path. The authoring
point before a mod's first action excludes that mod's effects. Supporting
archives introduced after the initial captured baseline remains unresolved;
full future-state behavior and patch-byte prediction are not proven. These
changes pass the required production builds and strict Clippy; runtime
verification remains pending.

Private archive import is now implemented in authoring sessions and exposed as
`easy import-archive` and a desktop import control. Imported bytes are stored
under the session, verified against their saved SHA-256 before use, and supplied
as logical mod-root archives to prediction and recorded disk actions. Imports
are autosaved guide revisions; undo/redo removes/restores input availability
without deleting bytes needed by prior revisions. Source paths are not written
into the exported metadata. Case aliases, conflicting live/checkpoint bytes,
pending recovery, cancellation, byte limits, and insufficient free space refuse
imports. Archive inputs survive the engine's preflight rescans. Existing inputs
are never overwritten. This new source still needs production gates and actual
archive-import runtime evidence; no full-build parity claim is made.

The private-input CLI implementation passes the no-default production lib/bin
build and strict Clippy. The fresh NVMe K1 manual authoring session imported the
actual Dialogue Fixes archive (1,068,958 bytes, SHA-256
37798965bc153548eccf9da2f5250fd5cc6bb5b9f635b6d137d6de4d3ae53f6f).
The production CLI undid and redid this import; the declaration disappeared and
returned while the stored object retained the same verified bytes. No game
files changed. Desktop gates and imported-input extraction remain unverified.
Direct moves/deletions/renames and process execution of private imported
archives still need defined consumption/materialization behavior; rescanning
must not resurrect a consumed input. Binding prior applied actions to their
input digests also remains unfinished. These gaps prevent a full authoring or
installation-parity claim.
The reloaded desktop reopened the prior prototype authoring revision, rendered
its directions by default, fenced prediction, and refused recovery while the
legacy pending marker existed. Its authoring journal hash was unchanged. This
proves the refusal path, not successful recovery or installation undo/redo.

Before another recorded disk action starts, the session now loads every applied
cursor-prefix action's saved authoring revision and checks that its declared
archive inputs still have the same name, digest and byte count. New archive
inputs may be added. Removing/replacing a bound input requires restoring the
filesystem boundary first. A missing saved revision refuses execution rather
than inferring its inputs from the current guide. This uses retained immutable
revision files without changing the disk-history schema. The new binding guard
still needs production gates and actual applied-action evidence.

The current private-input implementation and applied-input binding guard pass
all required production lib/bin builds and strict Clippy with and without
default features, plus formatting and whitespace checks. No Rust tests or
custom harnesses were run. Imported-input consumption and process execution,
full checkpoint-backed prediction, fresh baseline readiness, real installation,
and six-lane per-mod parity remain unproven.

Desktop restart support now includes `gui --easy-session <directory>`. The
request uses a separate instance, opens the editor, waits for existing desktop,
worker and sharing gates, and reopens the saved session through the editor
worker. It refuses combination with a protocol URL. A separate private view
record retains the mod GUID, instruction point, browser style and folder; it
never changes recipe or disk-history cursors. View files are bounded, session
bound, and kept outside install roots. Malformed/stale views are logged and do
not replace the recipe. The pending journal and recovery fences still apply.
This is restart support, not an in-process Rust code hot swap. Required production
builds and strict Clippy pass with and without default features; formatting
passes. The rebuilt desktop reopened the real NVMe authoring session at revision
2 after the existing sharing pause gate. A second owned-process restart restored
the saved mod, instruction point and Dolphin style. The existing unfinished-root
prediction refusal remained visible throughout. No disk action or game write
was performed during this restart check. Folder navigation restoration and
in-process code reload remain unproven.

Reusing a private archive object now verifies its regular-file type, length,
SHA-256 and stable length/mtime before recording another import revision. The
same bounded, cancellable verifier checks attached inputs. This guards object
reuse before autosave; it does not establish immutable bytes against arbitrary
external writers after verification, or resolve input consumption semantics.
This follow-up passes all required production builds, strict Clippy in both
feature configurations, formatting and whitespace checks. No corrupt-input
fixture or Rust test was created; the corrupt-object refusal remains without
a runtime corruption exercise.

Easy-mode real disk actions now carry a private-input consumption delta in the
pending/applied action journal. Dry-run computes it; the callback saves it before
legacy or checkpoint baseline capture, and later binds the checkpoint identity.
Execution restores the initial input availability, then must produce the same
remaining input set before committing. Moves, deletes and overwrites remove the
original import availability without deleting its immutable private object.
Only an absent logical file backed by that private object skips physical unlink;
ordinary missing-file failures still propagate. A matching real/checkpoint file
keeps its scanned spelling and backing instead of being replaced by the import.

Live input availability derives from deltas before the disk cursor. Undo/redo
and recovered committed actions therefore select the appropriate availability;
prediction continues replaying from the initial checkpoint with all declared
imports. Older journal entries decode with a missing plan. If their saved recipe
contained imported inputs, further execution refuses until disk undo returns to
the initial boundary; it does not infer historical consumption. Each action
stores only its delta rather than duplicating the cumulative archive list.
Root recovery and unfinished-checkpoint fences run before a new apply journal
or engine scan. External execution/materialization of private executables and
patcher-byte future prediction remain incomplete. These source changes pass the required locked lib/bin builds and strict Clippy
with and without default features, plus formatting and whitespace checks. Actual
guide-action/disk-undo/recovery evidence remains pending. No Rust tests or
synthetic filesystem fixtures have been created or run.

Explicit future-state prediction now projects known game and selected package bytes
into private scratch and invokes native OdyPatcher there. Output is validated
before publishing the predicted VFS; failures retain the prior state. The workspace
has an independent 16 GiB byte cap, 100,000-entry cap and 2 GiB free-space reserve.
Configured roots are not patch targets. Bundled external NSS compilers are disabled;
the pinned in-process NSS compiler is now enabled explicitly, without enabling
OdyPatcher's GUI. Compiler or patch errors fail rather than publish partial
prediction. Required Linux host source checks pass with and without desktop
features; actual native output, confinement and guide parity are unproven.

Built-in native patch installation no longer requires the external-program trust
switch. Execute/Run and configured patcher child processes still require it.
This changes process admission only; baseline/recovery and real-disk action
checks remain in force. Required Linux host source checks pass; actual native
install proof remains pending.

Built-in real installation now constructs the selected native installer after
Session discovery/namespace selection, validates paths, disables bundled external
NSS compilers and preserves Session's install-log handling. Prediction uses the
same compiler policy. This is required for process admission to remain accurate
on Windows as well as Linux; actual platform/runtime behavior remains unverified.

The shared process-admission predicate now covers whole-guide preflight,
easy-mode recorded-action preflight, and action execution. Default OdyPatcher
selection already resolves to the linked library when no executable is supplied.
Current-source Linux host production checks pass in both feature configurations;
runtime proof remains pending.

Execute/Run of a private imported installer now materializes its logical source
name through the normal write path during real execution. Dry-run records the
same import-consumption transition without writing; immutable input bytes remain
private. A physical file appearing after preflight is refused. External trust,
recovery and checkpoint handling still apply. Required Linux host builds, strict
Clippy in both feature configurations and formatting pass.
Actual supplied-installer execution, undo/recovery and Linux Wine runtime support
remain unproven or incomplete; this does not establish provided-patcher parity.

Explicit authoring prediction refuses unobserved Execute/Run effects instead of
presenting an unchanged game tree as the exact future. A matching executed
prefix can seed prediction from its verified after-checkpoint, with consumed
imports filtered and only remaining instructions replayed. Reuse requires an
identical saved guide and selected component context, matching action definitions,
roots and input identities; recipe changes fall back to replay from the initial boundary. Only the current
disk cursor's executed prefix is reused. Ordinary validation remains read-only
and can inspect external program inputs. Locked production lib/bin builds,
strict Clippy in both feature configurations and formatting pass, using private
OpenSSL build aliases for the host's stale development symlinks. Actual external
installer/checkpoint prediction proof remains pending for this change.

Authoring previews preserve the full recipe's selected component context while
clearing only future instruction effects. Conditions, dependencies, CleanList
names and checkpoint matching therefore use the same selections as real-disk
actions, including previews before the first action. An unobserved configured
external patcher refuses exact prediction rather than using native OdyPatcher
bytes as proof for a different backend. Matching committed checkpoints remain
available for observed prefixes. Formatting and required locked production builds
and strict Clippy pass with and without desktop features. The actual first K1
archive preview at zero actions and after extraction succeeded using empty NVMe
preview roots: 3 then 16 entries, no unknown file content, roots still empty, and
the saved authoring head unchanged. This is read-only authoring proof, not stock
baseline, external-installer checkpoint, or six-lane per-mod parity proof.

Desktop toolkit replacement is in progress in the standalone `desktop/qt`
production crate. Qt Quick/Quick Controls with CXX-Qt 0.10.0 uses the parent
Rust engine without egui, shared custom controls and embedded Noto Sans fonts.
Initial bridges cover existing saved sessions, read-only predictions, archive
import, durable revision seek and recorded actions. Full workflow migration,
rich model-based explorer, platform accessibility/runtime checks and default
GUI cutover remain incomplete. Locked Qt production builds, strict Rust Clippy and formatting passed using a
matching private Qt 6.11.2 SDK after the installed 6.11.1 development tools proved
incompatible with the runtime. The real K1 authoring session opened in the owned
Linux Xvfb software-rendered Qt window. Actual imported-archive prediction and
undo/redo restored the saved head; preview roots remained empty. This establishes
local authoring evidence, not full migration, GPU, accessibility or cross-platform
proof. Qt/GCC C++ header warnings remain despite strict Rust Clippy passing.
Explorer details, Dolphin icons and Finder columns use the predicted inventory;
full explorer interactions and lazy model scaling remain incomplete.
Qt toolkit mobile/WebAssembly support does not establish ModSync installer
support on those platforms. Root installation behavior and prior proof boundaries
are unchanged. See `docs/plans/desktop-toolkit-migration.md` for the comparison
and packaging/license obligations.

Qt action recording now emits canonical PascalCase instruction fields and rejects
unknown fields or unsupported action kinds before a revision is saved. Shared
modifier/range selection and action menus populate the durable recipe form.
In the real K1 session, the recommended PC Response Moderation dialog.tlk Move
was authored through the Qt context menu after Extract, with main-root destination
and overwrite enabled. The saved record uses mod/game root tokens. Prediction
shows the file removed from its source and present at the virtual game root; both
real preview directories remain empty. Undo/redo 2 → 1 → 2 restored the identical
saved head and prior immutable authoring files stayed unchanged. This is authored
recipe/virtual-state evidence, not a manual install, stock baseline or mod parity.

Qt saved actions now support identity-preserving edits and confirmed removal via
shared authoring transactions. Forms carry the saved revision and action GUID;
stale revision or identity mismatches refuse the change. Byte-equivalent edits
skip a new revision. Real-session UI verification preserved all saved JSON hashes
for an unchanged Move; a temporary overwrite edit changed that field only and
undo restored the original guide. Removal changed only Move and undo restored
the complete guide. Immutable and abandoned revision files remain intact and
real roots stayed empty. Builds, strict Rust Clippy and formatting passed;
separate-client stale-request refusal remains source-only evidence.

Qt built-in patcher inspection now reads the chosen recipe boundary through the
shared engine and pinned OdyPatcher dependency. Actual Character Startup Changes
data, description and changes.ini were observed after virtual extraction; no
namespace options were present in this supplied installer. Inspection preserved
saved-session hashes and both empty real roots. Both supplied archives were
hash-verified/imported through Qt, and Extract + Patcher were authored for the
second mod after the first mod’s two actions. Native future patch execution
refuses the empty preview root as an invalid KOTOR installation. This is real
inspection/authoring evidence, not stock-baseline, manual-install, patched-byte
or six-lane parity proof. The mandatory loose-file patch is imported but its
follow-up actions remain unauthored. Multiple namespaces, provided external
installer runtime and full literal guide-warning presentation remain unverified.
The Qt guide panel now includes imported download instructions, installation and
usage warnings, compatibility warnings, known bugs and Steam notes alongside
installation directions. Missing metadata is not reconstructed from historical
recipes; display completeness still depends on the imported guide. The actual
Character Startup Changes patch warning was visible in the owned Linux Xvfb
window after reopening its saved session, with revision hashes unchanged. This
is local software-rendered display proof, not broad-platform or full-guide proof.
Production build, strict Rust Clippy and Qt/root formatting passed.

Qt browser delegates now implement arrow-key focus/selection, Home/End and
Page Up/Down in list, grid and column presentations. Shift extends the existing
range selection; Control/Command moves focus without replacing selection. Grid
vertical motion follows the visible column count, and focused entries receive a
visible outline. Actual imported K1 archive previews demonstrated list, grid
and column range selection, focus-only movement and page navigation on Linux
Xvfb. Folder navigation now schedules first-entry focus after view reconstruction,
and Finder Left returns to the parent. Repeated Right-arrow descent into the
actual Character Startup Changes installer, End selection of its final data file,
and Left-arrow return demonstrated the focus handoff in the owned Linux GUI.
Saved revision hashes remained unchanged and the real game root stayed empty.
Large off-screen inventories and empty-directory keyboard behavior remain unproven.
These receipts do not prove screen-reader support or cross-platform keyboard parity.

Qt snapshots now include the durable disk journal cursor, retained checkpoint
count, pending/restoring flags, configured-root identity match and next action
from the shared guide order. A compact footer exposes applied disk progress and
recovery state without loading full checkpoint inventories into QML. Guide-order
errors are preserved. This status is read-only; explicit Qt disk controls are described below.
Successful disk execution/restoration and immediate-action mode remain unverified. The
actual saved K1 session displayed zero applied actions and correctly identified
KOTOR Dialogue Fixes action 1 even while Character Startup Changes was selected.
No disk journal was created, saved revision hashes remained unchanged and the
real game root remained empty. Pending/recovery/root-mismatch UI states have
source implementation only, without live proof.

The Qt browser heading now stacks its action controls in narrow panes, with
explicit title sizing derived from the enclosing pane rather than its own
content-dependent layout width. Minimum-width viewport controls are no longer
forced off-screen by the heading. The owned Linux software-rendered
window demonstrated 1000x660 compact controls and 1320x860 expanded controls,
including narrow-to-wide restoration and a long mod heading. This does not
prove all viewport sizes, split-pane combinations, GPU rendering or platforms.

Qt now exposes explicit disk apply, checkpoint seek and pending-action recovery
through the existing Session/shared engine. Apply captures revision, action GUID
and root identities; restore/recovery also capture disk cursor. Changed requests
are rejected before execution, and the shared journal enforces order and fences.
Progress is queued to the UI with bounded event capture and cancellation; recipe
undo/redo remains separate. Source compilation and confirmation/cancel UI proof
do not establish successful installs, restore/recovery or immediate-action mode.

The owned Linux Qt window opened the disk action dialog, showed disabled disk
undo/redo for a session without installed history, displayed the next ordered
action and exact folders in confirmation, and cancelled back to the dialog.
Saved revision hashes, disk-journal absence and empty real roots were unchanged.
Production build, strict Rust Clippy and root/Qt formatting passed. Actual apply,
restore, recovery, in-flight cancellation and stale-request refusals remain
unproven at runtime; source guard checks are not installation or parity proof.

Qt's Session dialog now creates authoring sessions from guide documents through
the shared bounded editor reader and Session::create. Authored selections are
preserved. Session directories require an existing parent, must be outside the
configured game/mod roots, and cannot replace existing sessions. Failure leaves
the current session available. The actual K1 full.md created a separate 186-mod
session in the owned Linux window; repeating creation reported "Authoring session
already exists" and retained the open session. Production compilation and strict
Rust Clippy passed. Other formats, platform behavior, failed-write recovery and
full-guide authoring integration remain unproven. The partial K1 recipe's required
startup patch now has Extract followed by Copy of feat.2da and featgain.2da into
Override; prior immutable revisions and empty real roots were preserved. These
are durable authoring changes, not successful manual installs or patcher parity.

Guide-state correction: the existing K1 authoring session has always contained
186 parsed components; the four visible sidebar rows were not its total count.
Its saved head and immutable revisions remain unchanged. No expansion or action
merge is required. TSL's real full.md created a 145-component authoring session
through Qt, but the shared Markdown reader keeps pre-Mod-List 3C-FD Patcher and
Water Restoration in preambleContent rather than component rows. This is an
unresolved authoring coverage gap, not permission to skip these required steps.
Both TSL preview roots are empty and no manual TSL install has been performed.

The preamble coverage gap is now addressed for easy-mode authoring through a
separate shared editor reader; the reference Markdown parser remains unchanged.
Named setup components retain their source descriptions/warnings, main-list GUIDs
stay stable, and selections are preserved rather than automatically enabled.
Qt and CLI session creation use this reader. Qt can repair older sessions as one
durable revision after checking the source main-list identities/order, introduction
and captured revision. The actual TSL session gained 3C-FD, Water Restoration and
Stutter Fix and Force Cage Update before its original 145 components. All three
remain unselected; the original body, document metadata and immutable revisions
are unchanged. GUI Undo/Redo returned to the original revision and restored the
148-entry state. Production builds and strict Clippy passed with/without default
features, plus Qt build/Clippy and both formatting checks. Other guide formats,
stale-request refusal, fresh-create setup inclusion and platform behavior do not
have separate runtime receipts. Supplied executable compatibility, clean baselines,
manual installation and per-mod parity remain unproven.

Qt now exposes saved guide-wide introductory, widescreen-warning and epilogue
content through a read-only Markdown notes dialog. The owned Linux window showed
the actual TSL version requirements and Enhanced Widescreen Support tail, an
explicit empty-section message, section switching back to the introduction and
1000x660 layout. Fresh document instances avoid blank views after section changes
while retaining the Rust session. Custom admonition syntax remains literal and
links are displayed without automatic external navigation. The Rust bridge passed
production build/strict Clippy; subsequent QML corrections passed production
builds and actual rendering checks. Named tail mods still lack authoring rows;
guide text visibility is not executable coverage, installer compatibility or parity.

Easy-mode authoring now recovers explicit named tail mods from original Markdown,
including K1's optional-widescreen section which reference import skips without
storing its mod text in the epilogue. Reference import is unchanged. The reader
retains main-list GUIDs, marks tail rows as conditional widescreen content and
carries section requirements. Qt's missing-only repair checks source identities,
order, notes and revision, preserving previously authored components exactly.
Actual GUI repair added 11 K1 tail rows (197 total) and six TSL tail rows (154 total).
Every original Name field matches a saved row in exact source order. All new tail
selections are off; prior components, metadata and immutable files are unchanged.
GUI Undo/Redo returned to each prior revision and restored each repaired head.
K1 tail rows and UniWS requirements were displayed. Production builds and strict
Clippy passed with/without default features, plus Qt build/Clippy and root/Qt fmt.
The K1 robe-icons master still has an unresolved cross-group reference; named-row
coverage is not complete dependency semantics. Clean baselines, supplied installer
execution, full instruction authoring and per-mod parity remain unproven.
Authoring master reconciliation now uses the reference importer's shared name,
heading and author-possessive resolver across the complete authoring guide. Fresh
reads resolve known setup/body/tail relationships; existing Qt sessions offer an
explicit revision-guarded, undoable repair. Unknown and conditional master text
is retained. Operations reject self references and validate dependency cycles
before saving. Resolved and unresolved requirements are displayed in mod notes.
All eight production build, strict Rust Clippy and formatting checks passed.
In the owned Linux Qt window, the actual K1 repair changed only robe-icons
dependency metadata. The other two unresolved labels, all authored instructions,
document metadata and prior immutable revisions were preserved. A second call
left all saved hashes unchanged; GUI Undo restored the original revision and
Redo restored the repaired state exactly. No disk journal exists and preview
roots remain empty. This does not establish complete dependency semantics, an
installed baseline or per-mod parity.

Qt now offers immediate execution when recording new actions, disabled at startup.
Execution folders are captured and validated with the recipe revision. Before
saving, the candidate must be the next ordered selected instruction, with matching
disk roots, no pending/restoring action and no unapplied disk redo. The durable
recipe edit precedes shared Session disk application; failure retains the recipe
and returns checkpoint guidance through the worker's current-state fallback.
Progress/cancellation use existing shared engine plumbing; editing saved actions
continues to affect only the recipe. The corrected bridge passed production Qt
build, strict Rust Clippy and formatting. A subsequent QML scroll correction
passed production build and formatting. In the owned Linux software-rendered
window, immediate mode displayed exact roots and rejected an out-of-order new
action before saving. Every saved JSON hash was unchanged and no disk journal
was created. At 1000x660, the form scrolled to show the execution notice and
overwrite control while Save/Cancel stayed visible. Actual immediate application,
failure recovery, cancellation and platform behavior remain unproven. No game
baseline or manual/automated per-mod parity is established by these checks.

Debug Qt builds now accept a bounded local `MODSYNC_QML_DEV_SOURCE` and watch
actual QML edits, including atomic editor saves. The Rust backend is an engine-owned
QML singleton; replacement windows initialize from its current snapshot. Reload
defers while busy, clears only the component cache and retains the prior window
when candidate creation fails. Mod/folder/presentation/sorting are carried forward;
unsaved dialogs, selections, history and action-point position reset. Immediate
mode resets off. Release-path source ignores the development setting and loads
embedded QML; release packaging has not been separately verified.

The bridge/main changes passed production Qt build, strict Rust Clippy and fmt.
Native repaint corrections passed production compilation; final Qt fmt passed.
In the owned Linux software-rendered window, atomic edits to actual UI headings
replaced the window in the same process while retaining K1 and the selected mod.
After the final scene-graph resource reset, the complete UI repainted without
manual resizing. Saved JSON hashes were unchanged. GUI Undo after reload restored
the prior revision, and Redo restored the exact original hashes. No disk journal
was created. Busy deferral, invalid-candidate retention, GPU/other platform behavior
and packaged release operation remain unproven at runtime. These development
checks do not establish any installed baseline, manual install or per-mod parity.

Reload references: https://doc.qt.io/qt-6/qqmlengine.html and
https://doc.qt.io/qt-6.11/qquickwindow.html.

The actual K1 authoring session now records Thematic KOTOR Companions, the third
full.md entry, as Extract then Patcher. The author's supplied v1.0.1 archive was
downloaded from its guide-linked page, verified as a ZIP and pinned through Qt
archive import (357556 bytes; SHA256
fe4fc60fcaed76e123b2563b934030ed9bb4a1c1b97481ab401ab34aba79e3af).
It retains INSTALL.exe, changes.ini, nine UTC templates and original documentation.
The archive's new-game requirement remains part of that pinned documentation.
The saved extraction uses a distinct thematic-companions working directory, then
the patcher targets the configured K1 root. No namespaces/installer choices are
present. Only the third component's selection/instructions and the new archive
reference changed; all prior components, document metadata, archive pins and
immutable revisions were preserved. Actual GUI Undo restored extraction-only
authoring and Redo restored exact saved hashes. Both preview roots remain empty
and no disk journal exists. No supplied installer, OdyPatcher execution, clean
baseline, manual install or automated per-mod comparison is proven by this work.

The fourth K1 full.md entry, JC's Minor Fixes, is now authored through the owned
Qt app using embedded QML. Its supplied v1.1 ZIP was obtained from the guide-linked
page and pinned through the GUI (3634303 bytes; SHA256
8bd3edee5af3af210405fa7871bc7580659d24e50fa56a7af0f550966d6cbae5).
The saved recipe extracts into a distinct minor-fixes directory, then moves 32
explicit paths into Override with overwrite. Sources follow Straight Fixes,
Resolution Fixes, Aesthetic Improvements and the allowed Things What Bother Me
files in guide order. The six Sith-uniform model files and the Bug Fixes script
are absent from the Move action; documentation files also stay out of Override.
The exclusion is an installation instruction, not a comparison exclusion.
Saved source paths were checked against the actual ZIP and literal guide. Only
the fourth component's selection/instructions and the new archive pin changed;
other components, document metadata, prior pins and immutable revisions are exact.
GUI Undo returned to extraction-only and Redo restored exact saved JSON hashes.
After the previous owned app/display were found absent, only a new owned Xvfb :1
and Qt process were launched; unrelated displays remained untouched. This also
demonstrated opening the saved session without a development QML source setting.
The real preview roots remain empty and no disk journal exists. This is recipe
authoring and GUI evidence, not a manual install or six-lane per-mod parity proof.


The fifth K1 full.md entry, Ajunta Pall Unique Appearance, is now authored in Qt.
The required guide-linked patch ZIP is pinned (9470582 bytes; SHA256
09dcdc47fe920ea8a9f1f3de422d77ad935aea6ebcb20e1ff42bd7025083ba5e),
as is the main v1.1 RAR (1654204 bytes; SHA256
6f3abbfdbf18719905cac6042685541b2bb4b17dd77ad042266bd99efa2466cd).
The recipe extracts and runs the required patch first, then extracts the main
RAR and moves only the two Transparent Skins textures to Override with overwrite.
Transparent/standard eyes is the recorded default pending an optional user
preference. Main-root files and the optional Sith Eyes replacement are absent
from the Move sources, as required by the guide. The actual RAR inventory was
read with unrar because this installed 7-Zip build did not support it. Required
patch download passed MEGA file-MAC verification. Actual GUI Undo/Redo restored
the exact saved revision, with other components, document metadata and old pins
unchanged. No archive extraction onto the game/source roots or disk install ran.

The sixth entry, KOTOR Community Patch, is now authored with six Qt-recorded
actions: extract the base v1.10.0 ZIP, Patcher from its root, extract the official
v1.10.1 starmap hotfix, move its MDL/MDX to Override, extract the guide-required
follow-up RAR, then move its n_fatcomm MDL/MDX to Override. Both moves overwrite.
The mod page supplies the starmap hotfix separately and describes it as fixing
Linux/Steamdeck/Wine crashes; it remains within this mod's step before the guide
follow-up. The supplied INSTALL.exe and original tslpatchdata/changes.ini remain
in the pinned base archive; no namespace choices are present.
Base ZIP: 86802053 bytes; SHA256
cdee80d28f9f87ffaa0b39fdfb46a2a3139858d87b3695d5161c96c02df14068.
Official hotfix ZIP: 87265 bytes; SHA256
70d271d9fe24ca6fad270c32ddf0f0ee2d9cdff3a732ce08118ce622f8eb7c94.
Guide follow-up RAR: 128612 bytes; SHA256
8564c745f3cd545cc356fc9932b5ede331adbd0b7bdd365da3bcd6f4b613665c.
An empty MEGA-helper output was rejected before import; a bounded direct SSL
transfer subsequently matched explicit API length and passed the file MAC.
GUI Undo restored the five-action state; Redo restored the exact six-action
revision hash. Prior components, guide metadata and input pins are exact.
K1 now has six authored recipes at cursor 36, with 197 components in guide order.
Preview game/source roots remain empty and no disk journal exists. Supplied
installer execution, OdyPatcher output, clean baselines, manual installs and
six-lane per-mod byte/path/directory parity remain unverified.
Authoritative mod-page source:
https://deadlystream.com/files/file/1258-kotor-1-community-patch/.


The seventh K1 full.md entry, Droid Claw Fix, is authored through the Qt GUI as
Extract to droid-claw, then Patcher from that root to the configured K1 root.
Its guide-linked supplied ZIP is pinned (341986 bytes; SHA256
73fc19bba379797163e41b6e7080e5db5cfba4812e0a18231959dd2970de6976).
The archive retains TSLPatcher.exe and the original changes.ini/baseitems.2da.
No namespaces or installer choices are present; the supplied configuration edits
baseitems row 81 droidorhuman to 0. Other guide components, document metadata and
prior input pins were checked exact. GUI Undo restored extraction-only; Redo
restored the exact saved revision hash. K1 cursor is now 40 with seven authored
recipes and 197 guide components. Real preview roots remain empty and no disk
journal exists; manual installation and six-lane per-mod parity are unverified.


The eighth K1 full.md entry, K1 Ported Alien VO Replacements, is authored in Qt
as Extract, Patcher namespace 0, then Patcher namespace 1. Its supplied v1.3.2
7z is pinned (11170530 bytes; SHA256
588aaed44a7386c9c453c15a373195d0c5e54708b5d169c6490ec106c3474f2b).
The original namespace document identifies 0 as Main Mod Installation and 1 as
KOTOR 1 Community Patch Compatibility Patch. Both actions target the configured
K1 root from the same extracted alien-vo folder. Installer.exe, namespace data,
per-option changes documents and original readme remain in the pinned archive.
K1R and NPC Overhaul compatibility options are not selected. This matches the
guide's main-then-K1CP instructions with Community Patch already selected.
GUI Undo removed only the second patcher pass; Redo restored the exact saved
revision hash. Other components, document metadata and previous pins are exact.
K1 cursor is 45, with eight authored recipes and 197 guide-order components.
Preview source/game roots remain empty, no disk journal exists, and supplied
installer execution, OdyPatcher output and six-lane per-mod parity are unverified.


The ninth K1 full.md entry, Ultimate Korriban High Resolution, is authored in Qt
using the preserved supplied TPC v1.2 RAR (716440044 bytes; SHA256
9c587b682f8469c22093e9b718d1110a209c0666ece65dd007fceb9b002a11ee).
It matches the guide-required TPC variant and the current Nexus files listing.
The recipe extracts to ultimate-korriban and moves 110 explicit Override texture
paths: the supplied archive contains 96 TPC and 14 TGA files. The TGA files are
part of that TPC download and are retained. It then extracts the guide-linked
Korriban Patch.7z (155 bytes; SHA256
de54c6d0c89b8152b4df6125b45d54bce876b9cb566da4297e6472df90223f27)
and moves its m36_shrub.txi to Override. Both moves overwrite. The public patch
transfer matched expected length and passed MEGA file-MAC verification.
The original archive/input collection was read without modifying it. Its full
source list was checked against the saved action. GUI Undo removed the TXI move;
Redo restored the exact saved revision hash. Other components, metadata and old
pins are exact. K1 cursor is 52 with nine authored recipes and 197 components.
Real preview roots remain empty and no disk journal exists; real installation,
clean-baseline proof and six-lane per-mod parity remain unverified.


The tenth K1 full.md entry, Ultimate Kashyyyk High Resolution, is authored in Qt
using the preserved supplied TPC v1.2 RAR (316045534 bytes; SHA256
3d53dc0c905f28043c500c74eaa814ffb8447ed4c7aeed38f8367d9b3115752b).
The recipe extracts to ultimate-kashyyyk and moves 64 explicit Override paths
with overwrite: 55 TPC, seven TGA and two TXI files, retaining the complete
supplied TPC-variant payload. Sources were checked exact against the actual RAR
inventory. The original preserved input was read without modification.
GUI Undo restored extraction-only; Redo restored the exact saved revision hash.
Other components, document metadata and prior pins are exact. K1 cursor is 56,
with ten authored recipes and 197 components. Preview source/game roots remain
empty and no disk journal exists. Clean baselines, actual installs and six-lane
per-mod parity remain unverified; available NVMe capacity is still insufficient.


Qt snapshot refresh now preserves the guide sidebar viewport and selected mod
by stable GUID instead of allowing a replacement component model to jump back
to earlier guide rows. A generation-guarded deferred layout restores the clamped
scroll offset after model replacement. Opening a session explicitly requests a
fresh viewport. This affects presentation only, not authored guide order.
The normal embedded-QML production app was rebuilt and launched on the owned
Xvfb :1. With the sidebar scrolled beyond the first ten entries and Ultimate
Tatooine selected, actual GUI Undo to K1 cursor 55 and Redo to 56 kept the same
visible guide rows and selected component. Redo restored the original saved
revision SHA256; no authoring revisions were added. Game/source roots were not
mutated. Qt locked production build, strict Rust Clippy and fmt check passed;
existing native Qt/GCC diagnostics remain. New-session reset is implemented but
was not separately exercised across two sessions in this receipt.


The eleventh K1 full.md entry, Ultimate Tatooine High Resolution, is authored
through the Qt GUI using the preserved supplied TPC v1.2 RAR (466430635 bytes;
SHA256 3ec658c09cebcd9a8cc2ccf12c53faf2bc4dea0d32765cb1459f0fc45e46bd41).
The recipe extracts to ultimate-tatooine, then moves 136 explicit Override paths
with overwrite: 130 TPC and six TGA files supplied in that TPC-variant archive.
Sources were checked exact against its inventory; the preserved original input
was not modified. Other guide components, document metadata and prior pins are
exact. GUI Undo restored extraction-only; Redo restored the exact saved revision
hash. The corrected guide sidebar retained the later-guide viewport throughout
selection, archive import, recording and Undo/Redo. K1 cursor is now 60 with
11 authored recipes and 197 guide components. Real preview source/game roots
remain empty and no disk journal exists. Actual installations, clean baselines
and six-lane per-mod parity remain unverified.


The twelfth K1 full.md entry, Ultimate Dantooine High Resolution, is authored
through Qt using the preserved K1 mod1368 TPC v1.2 RAR (494070294 bytes; SHA256
bf40814a47ce6856ec4c198e9b75e01e6b00ad91a2d9ca4486844c055e1bde8a).
The similarly named TSL mod1103 archive was not used. The recipe extracts to
ultimate-dantooine and moves 96 explicit Override paths with overwrite: 76 TPC,
19 TGA and one TXI supplied in the K1 TPC variant. Saved paths were checked exact
against the RAR inventory. The original preserved input was not modified.
GUI Undo restored extraction-only; Redo restored the exact saved revision hash.
Other components, document metadata and previous pins are exact. K1 cursor is
64 with 12 authored recipes and 197 guide-order components. Real preview roots
remain empty and no disk journal exists. Actual installation, clean-baseline
proof and six-lane per-mod parity remain unverified.


Qt action recording/editing now puts source paths in a bounded ScrollView and
displays the nonblank path count using the same line interpretation as Save.
The TextArea has mouse selection and an accessibility description with the count
and review instruction. The normal embedded-QML app opened the actual saved
96-path K1 Dantooine Move action: count 96 was visible, Ctrl+End reached its final
waterfall/waves/Window01 paths, and Save/Cancel remained accessible. Cancel kept
the exact saved revision SHA256 and K1 cursor64. Production Qt locked build,
strict Rust Clippy and fmt check passed; existing native compiler diagnostics
remain. Screen-reader behavior was not separately proven. No test fixtures or
game/source-root mutations were used.
Pinned input objects remain regular private session files: their shared verifier
rejects symlinks, so no archive-object symlink migration was performed and that
integrity boundary was not weakened. Storage capacity remains unresolved.
Qt reference: https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html.

- K1 easy-builder authoring now covers the thirteenth guide entry, Ultimate Endar Spire/Star Forge/Yavin Station: preserved supplied TPC archive pinned, Extract followed by Move of all 72 explicit TPC paths into Override. Actual Qt GUI Undo returned to Extract-only and Redo restored the exact saved revision SHA; all other components and guide text remained unchanged. Preview roots stayed empty with no disk journal. This is recipe-authoring evidence, not installation or per-mod lane parity evidence.

- K1 easy-builder authoring covers guide entry fourteen, Ultimate Manaan High Resolution: supplied TPC archive pinned by SHA and size; Extract then Move of 54 explicit TPC files into Override (two directory inventory entries are not file payloads). Actual Qt GUI Undo/Redo restored the exact saved revision; other components and prior archive pins remained unchanged. Preview roots remained empty and no disk journal exists. Installation and six-lane parity remain unproven.

- K1 easy-builder authoring covers guide entry fifteen, Ultimate Taris High Resolution: supplied TPC-variant archive pinned; Extract, Delete LSI_win01.tpc and LSI_box01.tpc from extracted sources as directed, then Move the remaining 284 explicit files into Override (272 TPC, 11 TGA, 1 TXI). These are literal installation directions, not comparison exclusions. Actual Qt GUI Undo/Redo restored the exact saved revision. Other components and prior pins were unchanged; preview roots stayed empty, with no disk journal. The guide requirement to use later Quanon Taris remains preserved. Installation and six-lane parity are unproven.

### Explorer patcher-data file inspection

The easy-mode patcher inspector accepts a file directly inside `tslpatchdata` as well as its folder or the top-level installer. All three resolve to the same mod root; selecting a named INI uses that name for a lone-namespace installer. A real namespace list retains the OdyPatcher library's namespace selection rules. Existing root containment, ordinary-file checks and document size bounds remain enforced. The owned source was formatted; production compilation and headless explorer interaction acceptance remain pending. This is not six-lane installation parity proof.

### Archive namespace installer identity

Archive instruction generation now binds the first discovered namespace document to its owning `tslpatchdata` tree even if later compatibility trees contain another namespace document. The first namespace tree outranks an earlier plain `changes.ini` tree. Namespace options preserve their zero-based indices and share an actual eligible installer executable beside that same data tree; compiler, helper and uninstall executables are excluded using the C# generator's list. Nested installer paths include the archive extraction directory. Missing sibling installers fail generation without publishing the staged component; no guessed namespace executable or `TSLPatcher.exe` is emitted. Source grounding: C# `AutoInstructionGenerator.AnalyzeArchive` and `ResolvePatcherExecutable` / `AddNamespacesChooseInstructions`. Owned sources were formatted, but production compilation, archive corpus differential acceptance and six-lane parity remain pending.

### Explicit supplied-installer runner (source checkpoint)

`Settings.provided_installer` explicitly selects the supplied Source executable instead of substituting a configured patcher. The Linux adapter creates and owns a fresh Xvfb server and passes its allocated display to native/Wine execution; Wine additionally needs a dedicated prefix beneath private request storage. Actual execution requires external-process trust and an active durable checkpoint operation. The request retains executable SHA-256, target, namespace/INI intent, checkpoint operation, owned display/PID, process diagnostics and install-log content/hash. Original installer executables receive no invented patcher CLI switches; a headless controller must operate their real dialogs. Exact provided-backend prediction refuses unknown effects. Windows/macOS supplied-GUI adapters are unsupported by this source slice. Compilation, live supplied-installer interaction, Wine descendant quiescence and per-mod six-lane output parity remain unverified; the request's process/log verification is not checkpoint commit or lane parity acceptance.


### Revision-fenced authoring CLI

The production `easy edit DIRECTORY - --label LABEL --expected-revision ID` path accepts a UTF-8 JSON operation list on standard input, bounded to 8 MiB. `easy undo` and `easy redo` also accept `--expected-revision`. The revision check runs while the authoring session holds its exclusive OS lock; a mismatch refuses the transaction. Status output includes `currentRevision`. Existing invocations remain compatible when the optional fence is omitted. Source integration and review are complete; the updated binary and writable TypeScript bridge still require production compilation and acceptance. No application tests or fixtures were added.

Dedicated Wine daemon quiescence is now part of the source contract: `WindowsProcessRunner.wineserver` is required, shared runner leases fence both supplied patchers and configured generic Execute, and prefix-scoped `-k` followed by `-w` must succeed before the writer marker can be removed. Cleanup ignores application cancellation and is bounded. Failed quiescence leaves the marker and pending checkpoint; it does not authorize rollback. A configured runner routes native Execute into its owned Xvfb too. Runtime descendant/process-failure acceptance remains unverified.

The supplied-runner writer fence is additionally persisted under the existing game recovery state, bound to canonical roots, private request/prefix storage and the active checkpoint operation. Both normal checkpoint write/recovery qualification and the external-process fence reject its presence, including when authoring recovery reconstructs default settings. The active runner clears its own exact ownership record only after verified process/prefix quiescence. Restart-time explicit runner quiescence recovery remains unimplemented; interrupted/uncertain ownership fails closed and must not be waived or treated as permission to roll back. Source coverage is not crash/restart runtime acceptance.

Native cancellation no longer conflates a failed action with unresolved owned-group writers: the command receipt now carries a separate settlement flag, false by default and set on Unix only after the existing owned-child group settlement actually succeeds. A cancelled/failed action with proven settlement can release its writer fence while retaining its failed/pending checkpoint for explicit recovery. This flag does not prove escaped daemon quiescence; Wine retains its separate prefix-scoped `-k`/`-w` requirement. Runtime cancellation acceptance remains pending.

Easy Predict accepts optional `--expected-revision`; the saved revision is checked under the session lock before prediction. Runtime acceptance remains pending.

Prelaunch failures now carry a separate `no_child_started` receipt proof. It is set only by validation/cancellation before dispatch or by the Unix launch preparation/spawn failure path; ordinary postlaunch errors do not imply it. Runner sessions additionally track whether external execution was submitted, so failed request publication or command preparation can release a native writer fence when no installer was launched. Submitted ambiguous failures keep the fence; Wine still requires its separate daemon quiescence proof. The action remains failed/pending even when a no-child fence is released. Source inspection and formatting are not runtime cancellation/spawn-error acceptance.

Easy Apply accepts runtime-only `--provided-request-dir` and `--provided-xvfb`, with optional all-or-none `--provided-wine`, `--provided-wineserver` and `--provided-wine-prefix`. Supplied execution requires `--allow-processes` and rejects simultaneous `--patcher`; it uses the shared engine without invented installer arguments. Apply `--expected-revision` is checked under the session lock before execution settings. Owned headless children remove `SESSION_MANAGER`; installer Qt/OpenGL rendering is forced to software. Source integration is not live installer or lane parity acceptance.

### 2026-10-10 headless continuation: directory authoring and recursive cleanup

`CreateDirectory` is a Rust/TypeScript authoring extension at numeric action index 12; the twelve legacy indices retain their values. It requires empty Source and Arguments and a rooted child destination with an existing logical parent. Virtual prediction and the ordinary checkpointed disk engine support directory creation; managed deployment rejects it until directory ownership is implemented. The legacy basic validator cannot establish root-aware directory safety and reports that limitation explicitly. This extension is not claimed as a C# interchange action.

Archive validation follows installation dependency order and projects selected filesystem branches. Effects of external Patcher/Execute/Run remain unknown: source syntax and root checks continue, while downstream existence checks are deferred to actual execution. Recursive DelDuplicate now groups candidates by parent directory and normalized stem. The separate final guide duplicate sweep remains unfinished.

Core formatting, default library/CLI build, and strict default Clippy passed. Qt source review passed; Qt rebuild, no-default checks, and actual new-directory prediction/rollback acceptance remain pending. The first K1 mod was installed through the isolated manual GUI; its complete game tree matches the Ody lane with zero exclusions. The supplied lane has not completed the first-mod comparison.

### Authoring checkpoint import recovery (2026-10-10)

Imported authoring snapshots use install scope, including empty component prefixes, so normal strict metadata reading and finalization agree. A retained same-target authoring restore can quarantine an exact interrupted importer clone under paired root locks. Durable intent and retained original metadata make the quarantine replayable; ambiguous candidates, references, external writers, maintenance and other unfinished operations fail closed. Ordinary checkpoint mutations refuse an active repair intent. Existing source checkpoint heads and authoring redo history are preserved. Source review passed; production compilation and actual GUI Resume/Undo/Redo acceptance for this correction remain pending.

The first K1 loose-file mod now has complete three-lane game-tree equality with no exclusions. This does not establish supplied executable installer parity. Actual GUI CreateDirectory Apply committed disk action three with the game inventory unchanged; Undo exposed the import defect above and remains retained at restore target two until runtime recovery succeeds.

### Unchanged-file restore optimization (source reviewed)

On Unix, restore can keep an existing ordinary file when its full SHA-256, size, supported permission mode and stable open-handle/path identity match the saved entry. It still flushes that file and its parent namespace. Selected CAS verification, root locking, temporary cleanup, directory ordering and final complete live-tree verification remain required. Mismatches use the ordinary replacement path; Windows retains that path. This avoids copying and renaming already-correct files without using timestamps as content proof. Source review passed; production compilation and runtime acceptance of this optimization are pending. The in-flight headless restore continues using the prior binary.


## 2026-10-14 continuation

Direct downloads now check cancellation before network admission, after response
headers and prefix reads, and before publication after archive verification and
fsync. Observed cancellation discards staging and returns the typed nonretryable
Cancelled outcome. Blocking network reads still obey their existing timeouts;
the final cancellation check does not eliminate the check-to-publication race.

Patcher InstallList simulation now uses the retained .NET ordinal Unicode comparer
for section names, keys and duplicate replacement. Profiles use the same comparer
for sorting, active-name presentation and desktop deactivation. Stored spelling
and native filename lookup remain unchanged. `merge` and legacy `convert --merge`
accept the reference's inert `--ignore-errors` flag; it does not relax install or
validation safety gates. These changes passed independent source review.

The [synthetic runner proof](tools/SYNTHETIC_FULL_BUILD.md) reads the pinned public
`kolto-labs/mod-builds` K1/TSL `full.md` corpus, records parser/review gaps and
projects supported action families onto original synthetic payloads. CLI and
native GUI have separate manual workflows for Linux, Windows and macOS. Source
corpus parsing, derived synthetic installs, and actual game/backend parity are
distinct evidence boundaries. Screenshots, command logs, omitted actions, ordered
completion receipts and final file hashes are retained as workflow artifacts.
No copyrighted game files or downloaded mod assets are used or required.

### Easy-builder execution boundaries and revision ordering (2026-10-15)

Previews retain the complete selection/dependency graph and cut effects after the
target identity in `Guide::ordered()`, not after its raw array position. Qt's raw
component-index API is unchanged; excluded preview targets fail explicitly.
New immutable authoring revisions use a private version-1 envelope containing the
guide and all three ordering flags. Every revision load (including cursor undo/redo)
restores and validates it; prediction checkpoint matching and disk-prefix acceptance
include ordering. Public guide serialization/export remains unchanged.

Existing bare-guide revisions are never rewritten. Their explicit legacy policy is
`Ordering::default()` (all flags false), matching their previous reload behavior;
the original import policy is unknown and is not inferred. New edits persist that
effective policy. Legacy checkpoints cannot prove the original import ordering;
restore the initial disk boundary or create a new session from an explicitly loaded
guide when that provenance matters. Unknown envelope versions, missing/invalid
ordering flags and unknown envelope fields fail closed, without legacy fallback.
The `authoringRevisionFormat` root key is reserved; a legacy guide using it as
extension metadata is rejected rather than ambiguously interpreted. Source review
and scoped formatting are the validation boundary here; the parent performs the
final production build. No tests, harnesses or install-tree operations were run.

### Validation: capsule `[InstallList]` folders (2026-10-18)

In the validation dry run, an `[InstallList]` folder whose last component ends in
`.mod`, `.erf` or `.rim` (any case) is now treated as a capsule file, as the pinned
OdyPatcher `is_capsule_file` does, instead of as a directory under which each file was
placed. Before, a game tree that held the capsule as a file failed with `File blocks
destination directory`. The files inside the capsule are not modelled.

- Capsule present: left unchanged, no warning.
- Absent `.mod`: a `PatcherSimulation` warning, because the real patcher builds it
  from the module's `.rim` files (and `_dlg.erf` on K2); whether those sources exist is
  not checked.
- Absent `.erf` or `.rim`: the step fails, because the real patcher cannot create them
  and logs an error for that file.
- The capsule path must stay inside the install roots. Other folders (for example
  `Override`) keep the literal directory behaviour.

Install is unchanged and still runs the patcher backend. Proof is targeted tests in
`tests/validate_capsule_install_folders.rs` plus source comparison against the pinned
patcher; no real patcher run.

### Built-in patcher failure log (2026-10-18)

Built-in patcher failures are now diagnosable. When the linked patcher errors,
reports errors or failed operations, is cancelled after running, or leaves no
install log, the engine emits the log's error and warning lines and a tail
bounded to 200 lines (each line capped at 400 characters) to the progress
channel before returning, and appends the first three error lines to the
returned error text. The staged log is otherwise deleted with its staging
directory. Successful runs emit exactly what they did before. Covered by a
generic stand-in patch with one failing operation; external-patcher failure
output is unchanged and runtime behavior on other platforms is unverified.

### Desktop patcher defaults and backend-specific saves (2026-11-03)

Typed desktop defaults and the invalid-settings session fallback now select
OdyPatcher, matching the engine and raw patcher preference decoder. Valid saved
backend choices remain authoritative; loading an invalid document does not rewrite
it. The dedicated desktop patcher save now updates only the selected backend and
its own executable key under the settings lock, returning the persisted desktop
and patcher projections together. HoloPatcher and OdyPatcher paths no longer land
in the KPatcher field. A blank executable clears only that backend's override;
other backend paths and unrelated settings survive. External executables and the
pinned OdyPatcher Git dependency are unchanged. Formatting, locked production
builds and strict Clippy passed in both feature modes; ten isolated CLI settings
inspections passed. Native UI save interaction remains unverified; Rust tests
remain frozen under AGENTS.md.

### Native archive-aware installer selection (source implementation)

The installer desktop's selection controls and complete-root ordinary CLI
selection now call the same engine selection entry point. Local archive failures
and unavailable required dependencies prevent new selections, with explicit
reasons, while saved choices remain intact. Desktop archive work is cancellable,
owned, and bound to the current input snapshot. The selection inspection command
accepts `--source-dir` alongside `--game-dir` and emits availability reasons in
JSON. No CI-generated GUID prefilter or TypeScript change is involved. See
[selection availability](docs/parity/selection-availability/README.md) for the
acquisition/direct-Markdown/remaining-phase boundaries and native GUI acceptance
limits. This does not establish full installation or cross-route parity.

### Enforce confirmed archive unavailability on saved selections

Successful native selection scans now clear confirmed unavailable saved items
and transitive required dependents, including during desktop refresh and CLI
`--use-file-selection`. Healthy option choices remain intact; the source guide is
not automatically rewritten. Incomplete/cancelled scans retain existing choices
and do not publish partial corrections. The scanner checks directory snapshots
and archive identity/size/timestamps, and distinguishes permission/read failures
from stable archive-format failures. Desktop scan errors also invalidate prior
validation readiness. See the selection-availability proof notes for fixtures
and native-platform/runtime limitations. This supersedes the earlier behavior
that retained unavailable saved checkmarks until validation rejected them.

### Nested required dependencies after explicit deselection

The shared CLI/installer selection graph now removes effective dependents when
an individual, option subtree, category group, or base-selection action unchecks
one of their required components/options, including nested option requirements.
The cascade reaches a fixed point and reports dependency reasons. Dormant option
flags remain saved preferences; selection receipts list only nodes whose full
ancestor chain is selected. Production CLI fixtures cover parent/subtree
removal, retained preferences on re-selection, and missing nested archives.
Group/base GUI actions share the source handler but actual native button proof
remains outstanding. No TypeScript comparison or full parity is claimed.

### Comparable native selection receipts

Archive eligibility now carries stable snake_case exclusion categories alongside
human reasons. `selection --json --receipt` and the installer desktop's Prepare /
Copy selection receipt controls share an engine export: schema version, normalized
guide-definition SHA-256 (selection flags cleared), relative-path mod-file sizes
and SHA-256s, inventory SHA-256, detected game/path policy/archive limit, effective
selected IDs, and all excluded IDs with categories/reasons. Export is explicit
because it hashes the scanned mod inventory twice around a fresh inspection.
It fails on uncertain I/O or observed input changes; ordinary selection remains
free of full-inventory hashing. This is not a raw-guide-byte or game-tree identity.

Linux actual Select All / Prepare / Copy clicks matched the CLI receipt exactly
for an unchanged full six-component fixture with missing/corrupt archives and
transitive dependents. Actual Deselect All invalidated the copied receipt. See
[replay and receipts](docs/parity/selection-availability/README.md). This bounded
native proof does not establish all category/option/wizard flows, Windows/macOS,
or TypeScript parity; issue #68 remains open.

### Native group and nested-option acceptance follow-up

A category group's toggle now offers Deselect group when every currently eligible
entry is selected, even if missing archives leave other entries unchecked. It
continues to offer Select group while any eligible entry remains unselected.
The eligibility decision uses the current availability snapshot and game filter.

Actual Linux desktop evidence now additionally covers group select/deselect,
filtered Select All, parent deselection through nested required dependencies,
option-subtree clearing/restoration with saved choices, disabled missing-option
clicks, and leaf deselection. All ten recorded selection states match the
corresponding CLI receipts. Proof retains the raw fixture guide and initial input
hashes as well as the normalized receipts and action history. These are bounded
native cases; wizard-page variants, native Windows/macOS and TypeScript remain
outside this evidence. See the selection-availability replay notes.

### Receipt inventory stability across a complete hash pass

A production CLI race fixture exposed successful receipt export with a stale hash
when an earlier file changed while a later file was being hashed on the final
inventory pass. Receipt hashing now retains each file's metadata stamp and
rechecks every file after the whole pass, in addition to immediate per-file and
directory checks. A detected change aborts export without partial JSON.

Linux synchronized production CLI evidence covers that exact race, an unreadable
receipt-only inventory file, and a repaired stable retry with the current content
hash. This closes the previously unverified CLI export-race case, not all possible
concurrent mutations or the corresponding native GUI failure presentation.
