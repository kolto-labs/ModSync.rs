# Pinned C# download orchestration oracle

2026-09-27 source audit. Oracle checkout `/home/brunner56/.codex/worktrees/5813/ModSync`, HEAD `d31ccbd3bfcddb4705917eafa6483715c166de3b`.

- No builds, tests, native launches, network requests or product changes were performed.
- Paths are relative to `src/`. Abbreviations:
  - DCS = `ModSync.Core/Services/DownloadCacheService.cs`
  - DM = `ModSync.Core/Services/Download/DownloadManager.cs`
  - DOS = `ModSync.GUI/Services/DownloadOrchestrationService.cs`
  - MBC = `ModSync.Core/CLI/ModBuildConverter.cs`
- Three read-only sub-audits (core, hosts, GUI/CLI) produced the citations. The lead re-read these lines directly: DCS:709-714, 1128-1140, 1258-1272; `DeadlyStreamDownloadHandler.cs:70-78`; `NexusModsDownloadHandler.cs:714-720, 862-872`; `DownloadHelper.cs:94-104`; DOS:52, 100-330; `DownloadsExplainPage.axaml(.cs)`; `WizardHostControl.axaml.cs:48-70, 163-182, 255-364, 580-664`; `DownloadIndicatorUiHelper.cs:12-48`; `MainWindow.axaml.cs:6922-6947, 7725-7760`; MBC:1118-1160, 1236-1262, 2070-2105, 2625-2662, 3480-3545; `FomodDownloadPromptState.cs:36-50`; `ConsoleProgressDisplay.cs:275-280`; and a grep showing that `UpdateDownloadUI(` has no callers.
- Anything marked *unverified* was inferred from source and not confirmed at runtime.

The plan built on this oracle is [download-orchestration-plan.md](download-orchestration-plan.md). The defects listed in §10 are reference defects. They are not parity targets.

## §1 Component → URL → disk call chain (core)

- Core has **no batch entry point**. Every caller loops over components itself and calls `ResolveOrDownloadAsync(component, destDir, progress, sequential, ct)` once per component (DCS:952).
  - Callers: GUI DOS:427, `SingleModDownloadDialog.axaml.cs:90`, and CLI MBC:1259-1260.
- The only URL source is `component.ResourceRegistry` keys (DCS:142, 996). "ModLinkFilenames" in logs and comments means `ResourceMetadata.Files` (DCS:1107, 2292). Metadata `Link`/`DownloadLink` fields are never acquisition input.
- Order inside `ResolveOrDownloadAsync`:
  1. VFS initialisation (DCS:977).
  2. `AnalyzeDownloadNecessityAsync` matches instruction patterns to decide which URLs are needed. On an exception, or if no targeted match is found, it returns every URL (DCS:980; `ComponentValidationService.cs:458-474`). The `initialSimulationFailed` value it computes is never used (DCS:980).
  3. `resolutionFilter.ShouldDownload(url)` (DCS:985).
  4. A cache/disk check per URL (DCS:1062-1078).
  5. `ShouldDownloadUrl` drops a URL only when every `Files` value is `false` (DCS:1100, 2570).
  6. Target filename selection and the download itself (DCS:1120-1222).
  7. `EnsureInstructionsExist` (DCS:1327).
- Resolution filter:
  - It uses the regex `(\d{3,4})x(\d{3,4})`. A tagged resource must match the screen exactly; untagged resources pass. There is no nearest-match fallback (`ResolutionFilterService.cs:16, 50-70, 163`).
  - A new filter is built on every call (DCS:45-46). On Linux that means running `xrandr` each time (`ResolutionFilterService.cs:220`).
- `ModComponent.ExcludedDownloads` is serialized, but the engine never reads it (`ModComponentSerializationService.cs:1504, 5463` are the only uses).

## §2 Filenames, existing-file detection, resource index

- Expected filenames are taken from the first source that yields any, in this order:
  1. The resource index (`TryGetResourceMetadataByUrl`, DCS:693, 1131). This path uses every cached key as a target. It **ignores `Files=false` and the resolution filter** (DCS:1135-1136, re-read).
  2. A live `handler.ResolveFilenamesAsync` (DCS:768, 1141; DM:179).
  3. The resolution filter, then `GetFilenamesForDownload` (DCS:1144-1146, 2496-2516):
     - A `Files` value of null or true means download; false means skip.
     - Names not in the dictionary are checked against the VFS patterns that Extract needs.
     - Rust `download_plan::select_filenames` ports this consumer.
  4. `FindBestMatchingFilenameAsync` (DCS:1155).
  5. All filtered names (DCS:1164).
- Host metadata (`GetFileMetadataAsync`) is used only in PreResolve, to compute the MetadataHash and size (DCS:327-401).
- **The existing-file skip is `File.Exists` on the name alone**, with no size or hash check (DCS:712, 791; re-read at DCS:709-714).
  - The path comes from `ResolveSourceFilePath`, which prefers `MainConfig.SourcePath` and falls back to destDir (DCS:1611-1642).
  - The `Files` dictionary is `OrdinalIgnoreCase` (DCS:681), but `File.Exists` is case-sensitive on Linux.
  - A URL is skipped only when every expected name exists (DCS:737, 815).
  - The "archive validation" at that point only writes a log line (DCS:743-746, 821-824).
- Persistent index:
  - `resource-index.json` lives under `%LOCALAPPDATA%/ModSync` or `~/.local/share/ModSync` (DCS:1570-1584, 2588).
  - Its schema is `{schemaVersion:1, lastSaved, entries:{MetadataHash → ResourceMetadata}}` (DCS:2648-2653).
  - Companion files are `.lock`, `.tmp` and `.bak` (DCS:2593, 2603-2604).
  - Looking up a URL is an O(n) scan of `HandlerMetadata["PrimaryUrl"]` (DCS:880-891).
  - On load, `Files` is deserialized with the default case-sensitive comparer (DCS:2799).

## §3 Concurrency, retries, timeouts

**Concurrency**

- **GUI:**
  - `StartDownloadSessionAsync(onScanComplete, sequential = true)` is called without the flag (DOS:52; `MainWindow.axaml.cs:6922-6927`), so components run one after another (DOS:287-305). The `Task.WhenAll` branch (DOS:308-318) is unreachable from Fetch.
  - Downloads within a component are strictly sequential whatever the flag says (DCS:1201-1222). `sequential` only affects the cache-check phase (DCS:1062-1078).
  - Net effect: one download at a time.
- **CLI:**
  - Always `SemaphoreSlim(10)` across components (MBC:1245-1247).
  - `--concurrent` only toggles the in-component flag (MBC:1263).
- `DM.DownloadAllWithProgressAsync` starts every task at once. `DownloadSingleWithConcurrencyLimit` has no semaphore at all (DM:318-326, 337).
- There is no per-host limit. `MaxConnectionsPerServer = 128` (`DownloadHandlerFactory.cs:40`).

**Retries**

- There is no generic retry. A failed download is recorded once (DCS:1232-1242).
- DeadlyStream retries only Cloudflare challenge pages: 3 attempts with 2s·n backoff (`DeadlyStreamDownloadHandler.cs:898-940`).
- Nexus waits once on a 429, using `Retry-After` or 60 s (`NexusModsDownloadHandler.cs:743-768`). The retry is broken: see H2.

**Timeouts**

- The HttpClient default is 180 minutes (`DownloadHandlerFactory.cs:29, 46`).
- The GUI passes `DownloadTimeoutMinutes`, which is always the 7-day default. The `TimeoutNumericUpDown` control it reads does not exist in the XAML (`DownloadProgressWindow.axaml.cs:49, 98-103, 113`).
- The CLI's session-wide cancellation is hard-coded to 2 hours for convert and merge (MBC:2097, 2653) and set by `--download-timeout-hours` for install, default 48 (MBC:664-665, 3523-3524).
- MEGA uses a per-step `Task.Delay` timeout of `max(300 s, size/100 KB/s)` (`MegaDownloadHandler.cs:270, 283-289`).

## §4 GUI and CLI entry points

### DownloadsExplainPage

- Title `Download Process`, subtitle `Downloading required mod files` (`DownloadsExplainPage.axaml.cs:33, 35`).
- Copy (`DownloadsExplainPage.axaml`):
  - Heading: `⬇️ Downloading Mod Files` (:13).
  - Intro: "ModSync will now begin downloading the required mod files in the background." (:19)
  - A dynamic `SelectionSummaryText`.
  - A "What's happening:" box with four bullets (:28-32), ending with "Installation will wait for downloads to complete if needed".
  - A tip (:37).
- The dynamic text (`.cs:37-47`) counts `IsSelected && !WidescreenOnly`. It reads "Downloads will be requested for {n} selected mod(s)…" or "No mods are selected yet…".
- The page has no controls. `ValidateAsync` always returns valid (`.cs:52-53`). **The page starts nothing.**
- Two lines of copy are false:
  - Nothing downloads automatically.
  - Install never waits for downloads (see gating below).

### Wizard sidebar (Download Center)

- It is built in code (`WizardHostControl.axaml.cs:219-364`). Contents:
  - Heading `Download Center`.
  - `📥 Fetch Downloads` with the tooltip "Try to download all missing mod archives automatically." (:255-263).
  - Open workspace (:266-274).
  - A status button (:288).
  - An LED and `Downloads idle` (:304-317).
  - `⏹ Stop Downloads` (:340-349).
  - A tip (:354).
- Visibility: on every page from `DownloadsExplainPage` onward, including the completion pages (`_downloadSidebarStartIndex`, :182; :701-709, 757-761). The status bar is `WizardHostControl.axaml:116-139`.
- Text formats come from `DownloadIndicatorUiHelper.cs:15-45`:
  - `Downloaded: {c} / {t} mods`
  - `Downloads in progress` / `Downloads idle`
  - `Preparing downloads…` / `No downloads queued.`
  - `Downloads: {c}/{t} complete`
  - `Downloads running…` / `Downloads ready`
  - Icon `⬇️` / `✅`
  - The counts are components, labelled "mods".
- **The sidebar never reflects real state.** `UpdateDownloadUI` (:652) has no callers anywhere (a grep for `UpdateDownloadUI(` finds only the declaration). It permanently shows the idle strings and keeps Stop hidden (:613). `OnDownloadStateChanged` updates only the Getting Started tab (`MainWindow.axaml.cs:6966-7000`).

### Fetch Downloads (GUI)

- Call chain: sidebar button → `DownloadFetchRequested` → `OnWizardDownloadFetchRequested` → `ScrapeDownloadsButton_Click` (`MainWindow.axaml.cs:7729-7732, 6922-6927`). That handler sets `HasFetchedDownloads = true` and calls `StartDownloadSessionAsync`.
- Guards (DOS:86-123):
  - An existing unfinished window is brought forward instead.
  - A session already in progress gives a warning and returns.
  - A missing mod directory or an empty guide gives a dialog.
- Component set: `allComponents.Where(c => c.IsSelected && c.ResourceRegistry.Count > 0)` (DOS:124-126, re-read).
  - Selected widescreen mods are **included**, so this can differ from the DownloadsExplain count.
  - Option selection and dependencies are not expanded.
- Per session:
  - A new `DownloadProgressWindow` (DOS:141-149).
  - A new `DownloadManager` with the Nexus key and window timeout (DOS:181-186).
  - One pending row per `(component, url)`, using the spoiler-free name when that mode is on (DOS:260-279).
- Per component: `PreResolveUrlsAsync`, then a "Ready to download: {file}" row (DOS:403-413), then `ResolveOrDownloadAsync` (DOS:427). Editor Mode also auto-generates instructions (DOS:436-451). The completed count uses `Interlocked` (DOS:453).
- On completion, only if not cancelled:
  - `progressWindow.MarkCompleted()`
  - `IsDownloadInProgress=false`
  - the FOMOD post-download pass
  - `onScanComplete` → `UpdateDownloadStatus` (DOS:321-342; `MainWindow.axaml.cs:6700-6896`). This only refreshes `IsDownloaded`, the text and the list. It does not re-validate.

**Progress window** (`DownloadProgressWindow.axaml(.cs)`):

- Sections: Active / Pending / Completed, plus Direct and "Network Cache" filters (:585-624).
- Row fields: `DisplayName`, `StatusIcon`, `DownloadSpeed`, `GroupProgressPercentage`/`GroupStatusMessage`, `ErrorMessage`, and a control button (`DownloadProgress.cs:441-592`).
- Aggregate: "Overall Progress: X / N URLs (a downloaded, b cached, c skipped, d failed)". The bar is finished URLs divided by total URLs, not bytes (:632-676). The summary reads "Download complete! …" (:714-716).
- Row control: Stop while in progress, Retry when completed/skipped/failed, otherwise Start (:941-966). Retry and Resume go to `HandleRetryDownloadAsync` (DOS:577-662), which bypasses the cache service.
- Context menu: Open Download Folder (only when the file exists), Copy Download URL, View Details (`.axaml:114-117`; `.cs:241-260`).
- Closing the window while downloads are active or pending hides it instead (:804-823).

**Stop:**

- Sidebar Stop calls `CancelAllDownloads(closeWindow: true)` (`MainWindow.axaml.cs:6936-6947`; DOS:464-517).
- The window's Cancel cancels only its own token (:765-800).
- A per-row Stop also calls `downloadManager.CancelAllAsync()`, which stops everything (DOS:207-212).

**Gating:**

- None. Next/Back depend only on each page's own flags (`WizardHostControl.axaml.cs:772-785`).
- ValidatePage, InstallStartPage and InstallingPage never reference download state.

### CLI `--download`

- `-d/--download` exists on convert (MBC:400), merge (:500) and install (:634-635). `--nexus-api-key` and `--download-timeout-hours` exist on install only (:661-665). Keys also come from `KOTOR_MODSYNC_NEXUS_API_KEY` / `NEXUS_MODS_API_KEY` (MBC:234-235).
- Entry point: `DownloadAllModFilesAsync(components, dest, verbose, sequential, ct)` (MBC:1118).
- **Component set: every component with a non-empty `ResourceRegistry`, whatever its selection** (MBC:1120, 1242, re-read):
  - convert downloads before selection filters are applied (MBC:2101 vs 2232)
  - merge forces every component to `IsSelected = true` (MBC:2639-2642)
  - install passes the full list (MBC:3526) before the selected-count check (MBC:3538-3542)
  - the `--best-effort` Nexus deselect (MBC:3504) therefore has no effect on downloads
- Progress:
  - A `Progress<DownloadProgress>` drives `ConsoleProgressDisplay` with per-file %, `✓ Downloaded`, `✗ Failed` and `⊙ Skipped`, or logs throttled to 10% steps (MBC:1155-1240).
  - At the end it prints `Download results: {n} files available, {m} failed` (MBC:1316).
- Failures: `LogAllErrors` (MBC:879-1094) prints `ERROR AND FAILURE SUMMARY`, then errors grouped by category, then `▼ Download and File Failures` split into with-filename, URL-only and file-only groups, then `TOTAL ERRORS/FAILURES: N`.
- Exit code: download failures do not change it. Convert returns 0 (MBC:2346). Install continues to the install step afterwards.

## §5 Progress, speed, cancellation, temp files

**Progress model**

- `DownloadProgress` is a single `INotifyPropertyChanged` model (`DownloadProgress.cs:13-592`) that mixes facts with presentation.
  - Facts: Status, ProgressPercentage, BytesDownloaded, TotalBytes, FilePath, times, ComponentGuid, TargetFilenames, ChildDownloads.
  - Presentation: `StatusIcon`, `ControlButtonIcon`, `DownloadSpeed`, `SourceIcon`.
- The status enum has five values: Pending, InProgress, Completed, Failed, Skipped (:792-799). There is no manual-required state.
- `DownloadSource` has three values: Direct, Optimized, Hybrid (:801-806). The last two are left over from the removed distributed cache.

**Speed and aggregate**

- Speed is a lifetime average: `BytesDownloaded / (DateTime.Now - StartTime)` (:462-476). There is no ETA anywhere.
- The group aggregate is an unweighted mean of child percentages (:430).
- Reports go through `IProgress<T>`. Chunk updates are throttled to 250 ms and read with 8 KB buffers (`DownloadHelper.cs:14-15, 111-132`). DM forwards the same mutable item (DM:373-445).
- `ThrottledStream` limits DeadlyStream to 7 MB/s using a synchronous `Thread.Sleep` in a 1-second window, with no `ReadAsync` override (`ThrottledStream.cs:51-111`; `DeadlyStreamDownloadHandler.cs:23, 780`).

**Cancellation**

- The caller token is linked to DM's global CTS (DM:67-69, 309-313). `CancelAll` and `ResetCancellation` sit at DM:201-294.
- DM converts cancellation to `Failed("cancelled")`, which DCS records as `DownloadFailed` (DM:452-462; DCS:1240).

**Temp files and publish**

- Temp name is `{name}..{ext}.{8hex}.tmp`, with a double dot (`DownloadHelper.cs:25-31`).
- The partial file is deleted only on `OperationCanceledException` (:158-173).
- Publish on Windows is `File.Replace` with a fixed `.bak`. On POSIX it is Delete then Move, which is not atomic (:46-70).

## §6 Host handlers

Contract: `IDownloadHandler` exposes `CanHandle`, `ResolveFilenamesAsync`, `DownloadAsync(url, destDir, progress, targetFilenames, ct)`, `GetFileMetadataAsync` and `GetProviderKey` (`IDownloadHandler.cs:14-34`).

Dispatch: DeadlyStream, then Mega, then Nexus, then GameFront, then Direct (`DownloadHandlerFactory.cs:57-64`). The first `CanHandle` match wins, and there is no fallback to another handler (DM:38, 168, 353, 447-474).

No handler opens a browser. None has a manual-required result: every login, CAPTCHA or countdown boundary returns `DownloadResult.Failed(text)`. No handler checks for existing files. `ModSync.Core/Services/Download/` contains no `Process.Start`, `OpenUrl` or `FlareSolverr`.

| Host (file) | Match | Auth | Manual-boundary behaviour | Filename source | Retries |
|---|---|---|---|---|---|
| DeadlyStream (`DeadlyStreamDownloadHandler.cs`) | URL contains `deadlystream.com` (:87) | Scraped `csrfKey` (:827-872), hand-parsed cookies (:287-320) | Cloudflare page ("Just a moment…" etc., :875-892): 3 tries, then the challenge HTML is returned and parsed, giving a generic "layout changed / requires login" failure (:526-542, :944). Login: an HTML body for a file link is logged and the file comes back null (:743-745) | Content-Disposition `filename*` then `filename` (:1117-1157), then the URL segment (:753), then `deadlystream_download_{guid}.zip` (:756) | Challenge only; `EnsureSuccessStatusCode` (:911) means 403/503 challenges never reach detection |
| MEGA (`MegaDownloadHandler.cs`) | URL contains `mega.nz` (:26) | Anonymous `MegaApiClient` 1.10.5 login, serialized by a semaphore (:17-57) | None; quota errors fall into generic failures (:348-370, :415-433). Folder links are converted (:463-516) but, per the library, not supported (*unverified*) | `node.Name` (:241) | None |
| Nexus (`NexusModsDownloadHandler.cs`, `NexusApiClient.cs`) | `nxm://` or `nexusmods.com` (:94-99) | `apikey` header from parameter, then `MainConfig`, then the CLI env vars | No key: fetches the mod page, discards it, and returns Failed "require manual interaction" (:473-512). No scraping, and no slow-download automation. nxm without a key and without key/expires: Failed (:530-545) | API `file_name` (:420, :558-583), then URI segment (:647), then `nexus_{g}_{m}_{f}` (:651) | One 429 retry (broken, H2). `NexusApiClient` tracks `X-RL-*` (:267-280) but the handler does not use it |
| GameFront (`GameFrontDownloadHandler.cs`) | `gamefront.com` (:43) | n/a | Always Failed: "requires manual interaction… JavaScript countdown/anti-bot" (:56-84). Resolve returns empty (:47-54) | n/a | None |
| Direct (`DirectDownloadHandler.cs`) | Any http(s) not matching the other hosts (:24-45) | None | None. An HTML page is saved as though it were the archive | Resolve: HEAD with Content-Disposition, then segment, then `"download"` (:47-115). Download: final redirect segment only (:170-184), so it can disagree with Resolve | None; 180-minute HttpClient |

DeadlyStream on a multi-file page downloads **every** link and ignores `targetFilenames` (:343, :961-966). It recurses on nested selection pages with no depth limit and returns only the last file (:706-736).

`NxmUrl.TryParse` requires `nxm://{game}/mods/{id}/files/{id}` and reads `key`, `expires` and `user_id` (`NxmUrl.cs:68-140`).

## §7 After download: write-back and FOMOD

**Registry write-back**

- `ResourceRegistry[url]` gets `FileSize`, `LastVerified` and `Files[name] = true` (DCS:1293-1310). `EnsureInstructionsExist` may then auto-create instructions and roll them back (DCS:1327, 1332, 2139).
- `DownloadManagementService.ProcessDownloadCompletions` handles only `Status == Completed`. It adds `Files = true` and saves the index. In Editor Mode it also inserts an Extract instruction (`DownloadManagementService.cs:132, 158, 183-191`).
- The GUI never saves the guide automatically (DOS contains no save call). CLI convert/merge write their output file afterwards. Whether the serializer carries every change is *unverified*.

**Archive validation**

- It is a stub: `bool isValid = true;` makes the delete branch unreachable (DCS:1261-1277, re-read).

**FOMOD**

- The shared core is `FomodPostDownloadOrchestrator.ProcessAsync` (`ModSync.Core/Services/Fomod/FomodPostDownloadOrchestrator.cs:17-102`). For each selected component (:38), and each of its downloaded archives (:42), it:
  1. detects a FOMOD (:47)
  2. checks `ShouldPrompt` (:52)
  3. asks the host, then marks the archive dismissed or skips it (:58-70)
  4. extracts it (:72)
  5. runs the wizard (:85)
  6. merges the result (:97)
  7. marks it configured (:98)
- Prompt policy (`FomodDownloadPromptState.cs:36-50`, re-read): only `configured` and `warned` suppress the prompt. `dismissed` **prompts again** on the next Fetch. State lives in `ResourceMetadata.HandlerMetadata["fomodPromptStatus"]` (:18-24, 87-124). Rust `fomod::should_prompt` already matches this.
- GUI:
  - Runs once after the whole batch, and only when the session was not cancelled (DOS:327-342).
  - Always uses `FomodGuiPostDownloadHost`: a Yes/No dialog, then `FomodInstallerDialog` (`FomodPostDownloadPromptService.cs:24-27`; `FomodGuiPostDownloadHost.cs:58-98`).
  - Ignores its own saved `fomodPostDownloadMode` setting (`SettingsDialog.cs:844`).
- CLI:
  - `RunFomodPostDownloadHookAsync` (MBC:1097-1116) resolves a mode (`FomodPostDownloadOptionsResolver.cs:157-229`). Precedence:
    1. `--fomod-skip` gives SkipAll.
    2. `--fomod-choices` or `MODSYNC_FOMOD_CHOICES` gives a choices file.
    3. A setting or env value of `skip` gives SkipAll.
    4. `--interactive` gives a console prompt.
    5. `warn-continue`.
    6. Otherwise, a TTY gives Interactive and no TTY gives WarnContinue.
  - Hosts are in `CLI/FomodCliPostDownloadHosts.cs`: warn at :16-48, skip at :50-81, console at :83-133.
  - The hook runs after the batch in convert (MBC:2234-2237), merge (:2675-2678) and install (:3535).

## §8 nxm handoff (GUI)

- Flow: `NxmHandoffQueue.UrlEnqueued` → `ProcessPendingAsync` on the UI thread (`ModSync.GUI/Services/NxmHandoffService.cs:42-81`), which then:
  - parses the URL (:86-93)
  - requires a workspace (:95-103)
  - re-queues the URL if no guide is loaded yet (:105-119)
- `NxmComponentResolver.TryResolve` matches game domain plus mod id against registry URLs of **all** components, whatever their selection. More than one match gives Ambiguous (`NxmComponentResolver.cs:42-93`; service :121-145).
- Download:
  - `DownloadModFromUrlWithProgressUiAsync` (DOS:664-700) downloads into `%TEMP%/ModSync_AutoGen_xxxx` (DOS:747).
  - The file is then copied into the workspace (`NxmHandoffService.cs:166-183`) and the filename is recorded (:185-199).
- Not done: no FOMOD pass, no status rescan, and the temp folder is kept after success (DOS:778-785 removes it on failure only).

## §9 Shared versus duplicated in C#

Shared Core:

- `DownloadCacheService.ResolveOrDownloadAsync`
- `DownloadHandlerFactory.CreateDownloadManager`
- the handlers
- `DownloadProgress`
- `FomodPostDownloadOrchestrator` and `FomodDownloadPromptState`
- `NxmComponentResolver`

Re-implemented per frontend:

| Concern | GUI | CLI / other |
|---|---|---|
| Component filter | DOS:124-126 (selected) | MBC:1242 (all with URLs) |
| Batch loop and concurrency | DOS:287-319 (sequential) | MBC:1247-1305 (10 at a time) |
| Manager setup | DOS:181-186 (timeout + key) | MBC:1148-1151 (no timeout) |
| Progress mapping | DOS:421-425 | MBC:1155-1240 |
| "File arrived" check | `SingleModDownloadDialog.axaml.cs:111-118` | MBC:1266-1272 |
| Filename write-back | DOS:613-638 (retry path) | `DownloadManagementService.cs:158, 201, 226`; `NxmHandoffService.cs:196-207` |
| FOMOD host choice | hard-coded GUI host | resolver |

Apparently dead code (no callers outside DOS, by grep): DOS `DownloadSingleComponentAsync` (:1009-1079), `HandleDownloadControlAsync` (:1087-1166) and `ResolveAndCacheModFilesAsync` (:878-973).

`DownloadModFromUrlAsync` (DOS:721-798) serves the per-mod Download buttons (`DownloadLinksControl.axaml.cs:894`, `ModListItem.axaml.cs:184`, `SummaryTab.axaml.cs:277`).

## §10 C# defects (not parity targets)

### Core (C)

1. **C1.** Data races in parallel PreResolve and in the cache check. They write a shared `Dictionary`/`List` and `component.ResourceRegistry` concurrently (DCS:574-577, 620, 655, 685, 715, 1073-1077; `ModComponent.cs:265`).
2. **C2.** The index save reloads the index while holding its own non-blocking lock. The second `FileStream`/`LOCK_NB` should fail (*unverified at runtime*). Its `Clear()` would drop the just-added entry (DCS:2427, 2628-2637, 2759-2761, 2776, 3080, 3139).
3. **C3.** Index writes are lost under contention. There is no retry on `LOCK_NB`, and every writer uses one shared `.tmp` (DCS:2603, 2726-2728, 3139-3143).
4. **C4.** The index replace is not atomic on POSIX even though the comment says it is: it moves path→bak, then tmp→path (DCS:2689-2695). The lock file is deleted after unlock (DCS:2713-2718).
5. **C5.** One `ResourceMetadata` object is shared between a component and the global index, so flags leak across components that share a URL (DCS:2427).
6. **C6.** Resource-index targets ignore `Files=false` and the resolution filter (DCS:1135-1136).
7. **C7.** The final status depends on callback order. DM never sets Completed itself, and a late InProgress report can land last, so `ProcessDownloadCompletions` skips the item (DM:476-520; `DownloadHelper.cs:141-145`; `DownloadManagementService.cs:132`; *unverified at runtime*).
8. **C8.** Results are paired with URLs by index across a `Dictionary` (DCS:1201, 1227-1230). Only one file is recorded per URL (DCS:1245; `DownloadResult.cs:11`).
9. **C9.** An unset SourcePath throws `InvalidOperationException` out of `ResolveOrDownloadAsync` (DCS:2492-2493, 1146).
10. **C10.** Cached names with a root or `..` escape the existence check (DCS:1620-1622, 1641).
11. **C11.** Archive validation is a stub (DCS:1261-1277).
12. **C12.** A truncated file can be published on cancel. The loop exits on `IsCancellationRequested` without throwing, and the temp file is moved into place (`DownloadHelper.cs:96-102`; `DirectDownloadHandler.cs:218`). Temp files are orphaned on non-cancel errors (`DownloadHelper.cs:158`).
13. **C13.** Failures are keyed "first failure wins" by URL and never cleared (DCS:26, 2206-2209). Cancellation is recorded as `DownloadFailed` (DCS:1240).
14. **C14.** Log capture buffers are never removed (`DownloadLogCaptureManager.cs:58, 92`). `ResetCancellation` disposes a CTS that linked sources may still use (DM:270-285).
15. **C15.** Speed uses the wall clock (`DateTime.Now`). There is no ETA, and the aggregate is an unweighted percent mean (`DownloadProgress.cs:430, 462-476`).

### Hosts (H)

1. **H1.** DeadlyStream sets `Authorization: Bearer KOTOR_MODSYNC_PUBLIC` and `X-ModSync-*` headers on the **shared** HttpClient, so they are sent to every host, including Direct targets and Nexus CDNs (`DeadlyStreamDownloadHandler.cs:70-78`, re-read; `DownloadHandlerFactory.cs:59-63`). The headers are duplicated if handlers are built twice.
2. **H2.** The Nexus 429 retry resends an `HttpRequestMessage` that was already sent, which throws. The first response is never disposed (`NexusModsDownloadHandler.cs:756, 763`).
3. **H3.** Nexus API-key downloads yield no links. `JObject.Parse` plus `["download_links"]` is applied to what the file's own comment says is a JSON array (:867-869 vs :716-717, re-read). The error is swallowed (:890), and the call returns `Succeeded(null, "Downloaded 0 file(s)")` (:470).
4. **H4.** Server-supplied names are used in `Path.Combine` without sanitising: MEGA `node.Name` (:241), DeadlyStream Content-Disposition (:749, 767, 1128-1145), Nexus `file_name` (:400, 420, 673). A rooted name or a `..` escapes the destination.
5. **H5.** A MEGA timeout leaves the download task writing to the final path after the lock is released and the session logged out (`MegaDownloadHandler.cs:276, 286-288, 438, 456`).
6. **H6.** DeadlyStream swallows `OperationCanceledException` (:819-823) and writes `deadlystream_debug.html` into the mods folder (:514-518).
7. **H7.** MEGA reports "timed out after 15 seconds" for any cancel (:399).
8. **H8.** Nexus does not pass a CancellationToken into link resolution (:386, 799, 861). It sets `ProgressPercentage = linkInfos.Count` (:405-406), and metadata uses `MainConfig` instead of the injected key (:1026, 1052).
9. **H9.** Direct buffers the whole body in memory: there is no `ResponseHeadersRead`, and the response is not disposed on exceptions (`DirectDownloadHandler.cs:162, 240`). A user cancel is reported as a timeout (:266).
10. **H10.** Direct does no HTML detection, so a landing page is saved as the archive (§6).
11. **H11.** DeadlyStream ignores `targetFilenames` and downloads every attachment. Its unbounded recursion returns only the last file (:343, 706-736, 961-966).
12. **H12.** Cloudflare detection never sees 403/503 challenges, because `EnsureSuccessStatusCode` throws first (:911).
13. **H13.** `mega.co.nz` is missed by both MEGA's match and Direct's exclusions, so Direct downloads the HTML page (:26; `DirectDownloadHandler.cs:38`).
14. **H14.** Temp names contain a double dot (`DownloadHelper.cs:25, 31`). Cosmetic.

### GUI and CLI (G)

1. **G1.** The wizard sidebar never updates: `UpdateDownloadUI` has no callers (`WizardHostControl.axaml.cs:652`, grep-verified). It always shows idle/ready and hides Stop.
2. **G2.** Fetching again after a completed session opens a second window. The old window's `Closed` handler then disposes the new session's manager (DOS:58-85, 144-149, 157-175).
3. **G3.** After the window's own Cancel, `IsDownloadInProgress` is never cleared. Pending rows keep the window hidden rather than closed, and later Fetch clicks only re-show it (DOS:321-324, 86-97; `DownloadProgressWindow.axaml.cs:807-817`). The `Closed` handler clears the flag without raising `DownloadStateChanged` (DOS:173).
4. **G4.** Retry matches `c.Name == progress.ModName`, which fails in spoiler-free mode (DOS:587 vs 265, 386).
5. **G5.** Pending rows are restarted whenever the summary is complete (`DownloadProgressWindow.axaml.cs:680-690`; DOS:857). The resulting storm is *inferred*.
6. **G6.** A per-row Stop cancels every download (DOS:207-212).
7. **G7.** Single downloads overwrite the batch counters and progress flag (DOS:702-719).
8. **G8.** Per-mod Download buttons save to a temp folder and discard the path, and use only the first URL (DOS:747; the three call sites in §9).
9. **G9.** FOMOD dialogs may be constructed off the UI thread after `ConfigureAwait(false)` (`FomodPostDownloadOrchestrator.cs:59, 74`; `FomodInstallerDialog.axaml.cs:92`; *unverified*).
10. **G10.** The console "ACTIVE DOWNLOADS" block never renders. `AppendFormat` gets a four-placeholder format with one argument, throws `FormatException`, and the exception is swallowed (`ConsoleProgressDisplay.cs:275-279`, re-read).
11. **G11.** The CLI ignores selection for downloads (MBC:1120, 1242; call sites :2101, :2657, :3526). `--best-effort` Nexus deselection is therefore ineffective.
12. **G12.** The CLI exits 0 despite download failures (MBC:2346). Its success count can inspect `MainConfig.SourcePath` instead of the destination (MBC:1266-1270).
13. **G13.** The GUI timeout control is missing, so the timeout is silently 7 days (`DownloadProgressWindow.axaml.cs:49, 113`).
14. **G14.** Download state gates nothing, contrary to `DownloadsExplainPage.axaml:32`. The page's "will now begin downloading" copy (:19) is false.
15. **G15.** Per-component GUI exceptions are only logged (DOS:300-303). Failed rows do not stop the FOMOD pass or the rescan. `ResolveAndCacheModFilesAsync` (:968-972) and `DownloadModFromUrlAsync` (:789-793) return empty or null silently.
16. **G16.** The GUI ignores the `fomodPostDownloadMode` setting (`SettingsDialog.cs:844` vs `FomodPostDownloadPromptService.cs:24-27`).
17. **G17.** The nxm temp folder is kept after success (DOS:778-785).

## §11 Unverified

These need runtime or live checks, which were out of scope:

- Live Nexus behaviour of unauthenticated `files.json` and of free-key `download_link.json`.
- `MegaApiClient` handling of folder links.
- The Progress<T> callback order (C7).
- Lock re-entry per OS (C2).
- FOMOD dialog threading (G9).
- Whether the serializer persists download write-backs.
