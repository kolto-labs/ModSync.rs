# Profiles and settings: C# audit (F16/F17)

Reference: pinned C# worktree at `d31ccbd3` (`/home/brunner56/.codex/worktrees/5813/ModSync`).
Live checks used the prebuilt `ModSync.Core.dll` (sha256 `8f3b1e3f…5768f0`, same hash as the earlier receipts). No C# was compiled.
Paths below are relative to the C# repo root. Rust paths are relative to this repo.

## 1. Settings file location

| Behavior | C# | Rust |
|---|---|---|
| Base directory | `SpecialFolder.ApplicationData` + `ModSync` (`Core/Services/Settings/ModSyncSettings.cs:36`) | `dirs::config_dir()` + `ModSync` (`src/profiles.rs` `settings_directory`) |
| Linux | `$XDG_CONFIG_HOME` if it starts with `/`, else `$HOME/.config` (dotnet/runtime `Environment.GetFolderPathCore.Unix.cs`, `GetXdgConfig`) | `dirs` uses the same rule. Live match recorded in `default-profile-directory-check.json` |
| macOS | `NSApplicationSupportDirectory`, which is `~/Library/Application Support` (same runtime file, `#if TARGET_OSX`, release/9.0 and main) | `dirs::config_dir()` is also `~/Library/Application Support`. Matches from source. Not checked on a Mac |
| Windows | Roaming AppData | `dirs` uses `FOLDERID_RoamingAppData`. Matches from source. Not checked on Windows |
| Legacy `KOTORModSync/settings.json` | Used when the current file is missing: GUI `SettingsManager.LoadSettings` (`GUI/Models/AppSettings.cs:266`), profile CLI `ModSyncSettings.LoadFromDirectory` (`ModSyncSettings.cs:67-82`, even with an explicit dir) | `Store::read_settings` does the same |
| `settings` verb and the legacy file | Never used. `RunSettingsAsync` resolves the directory first (`CLI/ModBuildConverter.cs:3752`), so `SettingsFileStore.ResolveSettingsFilePath` (`SettingsFileStore.cs:43`) always sees a non-empty directory. A live probe (A16) shows `set` creating a fresh `ModSync/settings.json` next to an untouched legacy file | New `SettingsFile::resolve` gives the same result |
| Explicit dir trimming | `ResolveSettingsDirectory` trims, and whitespace means the default (`SettingsFileStore.cs:35`) | `SettingsFile::resolve` trims. The profile CLI passes the value through unchanged |
| File mode | `SaveRoot` and GUI `SaveSettings` set 0600 (`SettingsFileStore.cs:195`, `AppSettings.cs:346`). The profile CLI's `SaveManagedDeploymentFieldsToDirectory` does **not** | Every Rust write goes through `recovery::atomic_write` (a tempfile created 0600). `SettingsFile::save` also sets 0600 explicitly |

## 2. Settings keys (GUI `AppSettings`, `GUI/Models/AppSettings.cs:23-104`)

| Key | Type | C# default | Rust today |
|---|---|---|---|
| `theme` | string | `/Styles/LightStyle.axaml` | read/write (`theme_preference`) |
| `sourcePath` | string? | null | read/write (`directory_preferences`) |
| `destinationPath` | string? | null | read/write |
| `debugLogging` | bool | true | generic `settings` verb only |
| `attemptFixes` | bool | true | generic only |
| `noAdmin` | bool | false | generic only |
| `caseInsensitivePathing` | bool | true | generic only |
| `archiveDeepCheck` | bool | false | generic only |
| `useMultiThreadedIO` | bool | false | generic only |
| `useCopyForMoveActions` | bool | false | generic only |
| `lastOutputDirectory` | string? | null. Applied only if the directory exists (`AppSettings.cs:206`) | generic only |
| `validateAndReplaceInvalidArchives` | bool | true | generic only |
| `filterDownloadsByResolution` | bool | false | generic only |
| `nexusModsApiKey` | string? | null. Sensitive (`SettingsFileStore.cs:29`) | read/write (`nexus_api_key`) |
| `fileEncoding` | string | `utf-8` | generic only |
| `selectedHolopatcherVersion` | string? | null | generic only |
| `patcherEngine` | string | `Holopatcher` | read/write (`patcher_preferences`) |
| `kpatcherExecutablePath` | string? | null | read/write |
| `enableFileWatcher` | bool | true | generic only |
| `spoilerFreeMode` | bool | false | read/write |
| `registerNxmProtocolHandler` | bool | false | generic only |
| `fomodPostDownloadMode` | string | `warn-continue` (blank also maps to it, `AppSettings.cs:148`) | generic only |
| `managedDeploymentEnabled` | bool | false | profile activation |
| `activeProfileName` | string? | null | profile activation, list, delete |

Rust-only keys: `holopatcherExecutablePath` and `odyPatcherExecutablePath`, written by `save_patcher_preferences`. `AppSettings` does not declare them, so a C# GUI save drops them. C# `MainConfig.odyPatcherExecutablePath` is CLI-only (`--ody-patcher-path`) and is never persisted.

Theme mapping: `GetTargetGameFromTheme` (`AppSettings.cs:222`) maps `Kotor2` to TSL, `Kotor` to K1, and Light to none. Rust does not implement it (see ACCEPTANCE F17/F24).

Other writers of `settings.json`:
- **GUI `SettingsManager.SaveSettings`** (`AppSettings.cs:315`). Serializes the whole typed object, so **unknown keys are dropped** and every known key is rewritten. Rust typed setters keep unknown keys. That is a deliberate, safer deviation.
- **GUI `SettingsDialog.cs`**. Only calls `SettingsManager.Load/SaveSettings` (lines 193-846). It writes no keys outside `AppSettings`.
- **C# CLI `set-nexus-api-key`**. Uses `ModBuildConverter.SaveSettings` (`CLI/ModBuildConverter.cs:245`) with a narrower Newtonsoft `SettingsData` (line 308). It drops unknown keys and GUI-only keys, and fills a missing theme with `SukiUI.Light`, which is a different default. Rust has no `set-nexus-api-key` verb. The desktop `save_nexus_api_key` keeps the other keys.
- **Consent.** Telemetry consent does not live in `settings.json`. It is in `telemetry_config.json` (`Core/Services/TelemetryConfiguration.cs:15`: `enabled`, `user_consented`, `consent_date`, …, with its own legacy path). The telemetry lane (F27) owns it. It is only audited here.

Parse failure: `SettingsManager.LoadSettings` falls back to defaults for everything when the file is unreadable or any known key has the wrong type. `ModSyncSettings` behaves the same way for its two fields. Probes A2 and A3 show that `"managedDeploymentEnabled":"true"` or `"activeProfileName":5` resets `managedDeploymentEnabled` to false on activation.

## 3. `settings` CLI verb (C# `ModBuildConverter.cs:726`, `3752-4066`)

- **`list`**. Loads the object and masks `nexusModsApiKey` unless `--reveal-secrets` is set. Text mode prints `Settings (N):` and then `  key=value`, sorted case-insensitively. An empty file prints `No settings found in <file>`. JSON mode prints `{settingsDirectory, settingsFile, values}`.
- **`get`**. Exact-case lookup. A missing key prints `Setting 'k' was not found.` and exits 1. The mask check is case-insensitive. Text mode prints strings raw, booleans in lowercase, null as an empty string, and anything else as compact JSON.
- **`set`**. An omitted or blank value removes the key. `ParseCliValue` (`SettingsFileStore.cs:138`) tries these in order:
  1. `true`, `false`, `null` (any case)
  2. `long`
  3. `double` (culture-sensitive)
  4. `{…}` or `[…]` parsed as JSON
  5. the trimmed text

  The write keeps other keys and sets 0600. The `--json` output masks the value.
- **Unreadable, empty, `null` or non-object file.** `LoadRoot` returns `{}` with a warning, so `set` **replaces the corrupt file** (probe A9).
- **Masking.** `CloneForOutput` masks only the exact-case `nexusModsApiKey`. Probe A12 shows `NexusModsApiKey` printed in clear. A null value stays null.

## 4. Profile schema (`Core/Services/Profiles/Profile.cs`)

| Field | JSON | Type | Notes |
|---|---|---|---|
| Name | `name` (line 38) | string | Also the sanitized filename (`ProfileService.cs:66-96`) |
| KotorDirectory | `kotorDirectory` (43) | string? | Game dir (MainConfig destination) |
| ModDirectory | `modDirectory` (48) | string? | Mod archive dir (MainConfig source) |
| InstructionFilePath | `instructionFilePath` (53) | string? | |
| ComponentSelections | `componentSelections` (58) | `Dictionary<Guid, {isSelected (20), selectedOptionGuids (25): Guid[]}>` | Newtonsoft writes lowercase hyphenated GUIDs |
| CreatedUtc | `createdUtc` (63) | DateTime | Newtonsoft writes 7-digit fractions ending in `Z`. Rust writes milliseconds. Both read each other (profile-check.json) |
| LastUsedUtc | `lastUsedUtc` (67) | DateTime | |

`ProfileService` (`Core/Services/Profiles/ProfileService.cs`):
- **Storage.** `{dir}/profiles/{name}.json`, created with temp-then-move (lines 162-200). `ListProfiles` skips unreadable files and blank names and sorts case-insensitively (113-138).
- **CRUD.** Create (204), clone (225, deep copy with a new `CreatedUtc`), rename (249: same-path rename rewrites the name; otherwise it refuses an existing target and deletes the old file), delete (276).
- **Capture and apply.** Capture keeps `CreatedUtc` (294-335). Apply leaves components that are not in the profile untouched and updates `LastUsedUtc` (343-381). In the GUI, its only caller is `ProfileManagerDialog.ActivateButton_Click` (line 115), which then saves `ActiveProfileName` through `SettingsManager`.
- **Deleting the active profile in the GUI.** The dialog's delete (line 206) sets `ActiveProfileName = null`, which is written as JSON null.
- **Profile CLI.** `RunProfileAsync` (3700): `-a/--action` (case-insensitive, required); `-n/--name`; `clone`/`rename` use `--from` or else `--name`; text output by default; `--json` for list and show only. `show --json` uses PascalCase names because System.Text.Json ignores the Newtonsoft attributes.
- **CLI activation.** `ProfileCliActivate` (3937) reads the typed fields case-insensitively (from the current file or the legacy file) and merges them into the **current** file only. An unreadable current file is replaced (`ModSyncSettings.cs` `MergeIntoJson`).
- **Byte-order mark.** A UTF-8 BOM is accepted in both profile and settings files, because `File.ReadAllText` strips it (probes A6, A7).

## 5. Rust state before this change (`ac9cf3a`)

- **`src/profiles.rs`.**
  - Profile schema, sanitize, lock-guarded CRUD, capture and apply.
  - The typed settings accessors listed in §2, which merge per key and keep unknown keys.
  - Legacy fallback, and `redacted`.
- **`src/profile_cli.rs`.**
  - The `profile` verb with `list`, `show`, `create`, `clone`, `rename`, `delete`, `activate`, plus Rust-only `capture` and `apply`.
  - Output is always JSON.
- **Missing.**
  - No `settings` verb at all.
  - Profile and settings files with a BOM were rejected.
  - Activation read `activeProfileName` and `managedDeploymentEnabled` by exact case, and kept a string `"true"`.
  - Capture wrote component GUIDs as the guide spelled them, including braces or uppercase.
  - `profile list` failed on an unreadable settings file.
  - `redacted` masked only the first case variant, and turned null into `"***"`.
  - No `-a`/`-n` short flags, a case-sensitive action, and `clone`/`rename` needed `--from`.

## 6. Ranked gaps

| # | Gap | Impact | Status after this change |
|---|---|---|---|
| 1 | No `settings list/get/set` verb (F17 generic read, write, redaction) | High: the only way to change most GUI keys without the desktop | **Done**, checked live against the DLL (receipt `settings-check.json`) |
| 2 | Masking: case variants and null values | High (secret exposure) | **Done**, stricter than C#: every case variant is masked |
| 3 | Activation's managed fields: case-insensitive, and a wrong type resets them | Medium | **Done**, probes A1-A3 |
| 4 | BOM in profile and settings files | Medium: files edited on Windows were skipped | **Done**, probes A6-A7 |
| 5 | GUID spelling on capture/apply (braces, case) | Medium: C# cannot map braced keys consistently | **Done**: capture writes the C# `Guid.ToString()` form, apply compares normalized forms |
| 6 | `profile list` with an unreadable settings file | Low | **Done**: warns and lists everything as inactive, like C# (A5) |
| 7 | Profile CLI flags (`-a`, `-n`, `ignore_case`, `--name` for clone/rename, clone reports a missing source first) | Low | **Done** |
| 8 | Typed load of the whole settings file with C# defaults and all-or-nothing reset on a type error; the GUI applies all 24 keys | High for the desktop, but it needs `ui.rs` | **Open**. Out of scope here (`ui.rs` is frozen). The §2 table is the spec |
| 9 | Theme to TargetGame mapping | Medium | **Open** (F17/F24) |
| 10 | Replacing unreadable settings on write (`settings set`, activation) | n/a | **Deliberately not ported**. Rust refuses instead of discarding data (tests pin activation and deactivation) |
| 11 | Profile CLI text output (C# default) versus Rust's always-JSON output | Low: scripts use `--json` | **Open**. The existing Rust integration test reads JSON without `--json` |
| 12 | C# CLI `set-nexus-api-key` verb (validates with Nexus, rewrites a narrower schema) | Low | **Open**. Rust uses `settings set --key nexusModsApiKey` |
| 13 | Strict GUID and DateTime typing (C# skips profiles with a bad GUID key or a bad date; Rust reads them as text) | Low | **Open**. Rust is more permissive |
| 14 | Parse edge cases: `Infinity` stays a string only because host ICU uses `∞`; thousands separators vary by culture | Low | Matches an en-US host. Other locales are not modeled |
| 15 | Native macOS and Windows folder checks | Low | Source-verified only |

## Current F16 schema and CLI implementation (2026-10-02)

The shared store now uses `Profile::decode` for persisted profiles. Known profile/selection property names match without regard to case, missing fields retain reference defaults, GUID D/N/B/P/X spellings normalize to lowercase D form, and date fields require valid supported DateTime text. ISO timestamps with/without offsets, date-only ISO, and the legacy Microsoft JSON date spelling are accepted; year bounds are 1–9999 and leap seconds are rejected. Broader invariant-culture DateTime parsing is not claimed. Explicit null selection dictionaries/lists are rejected because the reference's apply path dereferences them; no successful apply behavior is inferred from mere deserialization acceptance.

Public `Profile` and `Selection` field types are unchanged. `Profile::decode_reference` rejects non-GUID selection identities. The compatibility decoder returns `DecodedProfile { profile, identity_format, warnings }`; legacy Rust guides could use arbitrary local text identities, so those are preserved with `LegacyOpaqueIds` warnings. Historical unmarked text IDs cannot be distinguished reliably from malformed GUIDs in purported C# profiles. They are never reported as reference-compatible. Saving such a profile adds `_modsyncRustIdentityFormat: "opaque-v1"`; it does not invent UUIDs or discard component/option entries. C# cannot load these opaque identities. Duplicate normalized GUID dictionary keys follow authored last-write precedence; capture rejects duplicate normalized component identities instead of silently overwriting a component.

`Store::load_report` and `list_report` expose diagnostics to UI callers. Existing `load`/`list` wrappers log the warnings; unreadable profile records remain skipped by list. Reads are bounded to 16 MiB per profile and reject unsafe leaf files. Capture refuses to replace an existing profile that fails the decoder. General profile-root/symlink/locking hardening is a separate ownership slice.

The profile CLI now uses readable default output for list/show/CRUD/activation. `--json` retains explicit machine-readable output, including Rust-only mutating verbs. Capture/apply have explicit readable confirmations. This intentionally changes the previous Rust JSON-by-default behavior. Frozen tests that assumed JSON without `--json` were neither edited nor compiled in this slice; no unchanged-test-contract claim is made.

Settings-root isolation is explicit: `Store::new(root)` never consults the user's global legacy settings. Callers wanting automatic default migration must use `Store::from_settings_directory(None)`; `Some(path)` stays isolated even when the path happens to equal the default directory. CLI profile commands preserve the original optional directory argument. This deliberately differs from the old C# profile CLI's global fallback for explicit roots and prevents an isolated invocation from importing another settings store's paths, active profile or secrets.

These changes supersede ranked gaps 11 and the implemented subset of 13 above, but do not close F16/F17. Typed whole-desktop settings/default application, native interaction/platform/locale evidence and the documented remaining parser differences remain separate work. The managed UI is concurrently evolving in the T6 lane; earlier absence statements in source audits describe their inspected snapshot, not the final current UI.

## F17 typed library and CLI boundary (2026-10-02)

`DesktopSettings` now models all 24 reference fields and defaults. `decode` checks known fields case-insensitively, including each occurrence before alias normalization; a wrong known type rejects the whole projection. `Store::desktop_settings` reports fallback to complete defaults with diagnostics, without overwriting the malformed source. Nullable text fields preserve null. Effective theme/encoding/FOMOD helpers apply the reference's null/blank fallback rules. Theme mapping checks Light first, then Kotor2/TSL, then Kotor/K1; it is a preference and does not claim installed-build detection.

`save_desktop_settings` merges the recognized typed fields under the shared store lock, preserves unknown keys, bounds the final output, and refuses malformed input. The store's explicit-versus-default constructor distinction controls legacy migration. This deliberately preserves extensions where the reference whole-object save would discard them.

The CLI exposes `settings --action inspect` for the typed/default projection with secrets masked unless `--reveal-secrets` is explicit. The new `SetNexusApiKeyArgs` implements the reference positional key command with explicit `--skip-validation`, plus settings-directory and JSON output. The shared `save_validated_nexus_api_key` performs cancellable validation, rejects invalid keys, rechecks the old stored key under lock, merges unrelated changes, and saves only after a final cancellation check. Responses never print the key. Existing unvalidated setters remain available for compatibility; callers must explicitly choose the validation API.

This completes the typed library/CLI slice, not application of all flags to desktop/engine behavior. Flags such as copy-for-move, encoding, archive checks, watcher preference and output-directory policy require their consumers to use this projection. Native key validation/settings restart, HTTP behavior, locale parsing, and platform evidence were not run in this slice.

## Storage isolation and private replacement source (2026-10-02)

Profile and settings readers now share bounded 16 MiB regular-file reads, reject
symlink/reparse paths and wrong-owner storage boundaries, and propagate current
file errors rather than silently consulting the legacy store. Existing system
ancestors are checked for ordinary directory shape, without requiring `/` or
`/home` to belong to the application user. Explicit roots remain isolated; only
the implicit default store may read the legacy file, which remains untouched.

New Unix directories use 0700 and new/replacement files and locks use 0600.
Existing directories are never chmod'd; a chosen directory writable by another
user is rejected. Readable legacy files may be copied forward into private new
files. Windows source creates new directories/files with an explicit protected
current-user DACL, validates the current owner SID, refuses reparse points and
publishes the prepared private file with same-volume replacement. Existing
Windows directory ACLs are preserved, not rewritten. Filesystem protection still
relies on the OS enforcing these owner/ACL boundaries; this is not a transaction
against a malicious process running as the same account.

All cooperating profile/desktop/settings writers use the same regular-file lock
with a 15-second bounded wait. `SettingsFile::update` performs read/edit/write
under that lock; CLI setting edits and peer-setting edits use it, preserving
unrelated concurrent settings. The existing explicit full-object `save` API is
retained and now also locks and refuses unreadable current files. Writes reject
oversized serialized output before creating artifacts and sync file data before
publication; Unix additionally syncs the parent directory. No filesystem test,
native ACL probe or crash injection was executed. Windows/macOS native compilation
and migration/permissions acceptance remain required evidence.

### Case matching preference consumer (2026-10-02)

The additive `Engine::new_with_case_insensitive_pathing(settings, bool)` selects
matching before the first filesystem inventory. `Engine::new` retains its prior
case-insensitive default and public `Settings` fields are unchanged. Fresh
validation engines preserve the policy. Unix exact mode retains distinct UTF-8
filename identities, uses exact source globs and directory prefixes, and does not
case-fold duplicate-removal filename stems. Non-UTF-8 exact inventories are refused
instead of being lossily aliased. On Unix volumes that resolve case aliases,
resolved paths and output publication additionally check physical spelling and
refuse an alias; checks are bounded and observe cancellation.

This is deliberately a bounded consumer: exact mode rejects Windows construction,
managed deployment, checkpoint resume/install and the existing case-folded conflict
report. Checkpoint-mode refusal precedes the recovery lock/snapshot writes. The
managed/history schemas do not yet encode path policy, so changing those semantics
would not be a safe compatibility default. Archive member admission retains its
cross-platform collision rules. External trusted process behavior and native
filesystem semantics are not claimed to become case-sensitive through this flag.
No runtime or filesystem tests were executed for this source slice.

### Reference preference consumers audited at d31ccbd3 (2026-10-02)

These findings come from production source, not runtime measurements:

| Preference | Pinned source behavior | Implementation consequence |
|---|---|---|
| `useMultiThreadedIO` | `src/ModSync.Core/Instruction.cs:1065` and `:1194` choose semaphore maximum 16 or 1 but always initialize **one** permit; corresponding `finally` blocks (`:1121`, `:1258`) release exactly one. `Services/FileSystem/RealFileSystemProvider.cs:101` likewise starts one permit and releases once at `:276`. | Effective file-operation concurrency remains one in both modes. Rust serial publication matches that effective contract. Introducing concurrent mutation merely to consume this flag would add behavior beyond the pinned implementation. |
| `validateAndReplaceInvalidArchives` | `Services/DownloadCacheService.cs:741` and `:819` gate log statements for existing files. The downloaded-file branch at `:1258` assigns literal `bool isValid = true` at `:1261`, making its corrupt-file deletion branch unreachable. | Do not replace Rust's verified, non-clobbering acquisition behavior with a guessed archive replacement policy. The stored preference has no demonstrated validation/deletion effect in these reference branches. |
| `enableFileWatcher` | `SettingsDialog.cs:287` copies the flag to picker properties. `Controls/DirectoryPickerControl.axaml.cs:700` starts the watcher on path text changes; `:1034` never reads the flag. `Services/ComponentValidationWatcherService.cs:52` starts watching the current component's top-level registered filenames independently of the global flag. | The reference does have watcher behavior; its stored enable flag is ineffective in these consumers. Rust's bounded watcher uses the flag as an explicit opt-in/out improvement, with validation invalidation only and no automatic instruction validation or mutation. |

The new shared `resource_watcher` observes at most 4096 registered relative resource
paths with one-second metadata polling and a coalesced generation-tagged event
slot. It reads no file contents, traverses no unregistered directory trees, and
never changes installation state. UI ownership retains the thread, stops the old
request before replacement, discards stale generations, and invalidates validation
when observed metadata changes or observation fails. Missing registered files are
tracked, so creation/deletion is observable. Polling is a change hint rather than
content-integrity proof: changes preserving all observed metadata or occurring
entirely between polls can be missed. Cancellation wakes the delay and is checked
between metadata calls; the OS can still stall an individual filesystem call.
No watcher runtime or filesystem test was executed for this source slice.

The FOMOD CLI mode adapter now reads the shared Store's case-insensitive setting
lookup and preserves the caller's explicit-versus-implicit directory provenance.
An absent mode remains `None`, preserving terminal-based CLI fallback rather than
injecting the desktop `warn-continue` default. Explicit skip and choices still
bypass the mode lookup; settings then environment retain their existing precedence.

## Native numeric symbols and additive parser (2026-10-02)

The production settings CLI now uses an immutable native-culture snapshot and
parses before the settings lock. This supersedes the en-US-only production setter
gap while retaining the old public helper bodies. The
[implementation receipt](settings-numeric-culture.md) documents native providers,
integer/floating grammar, negative-zero preservation, bounded input, and remaining
Windows ICU/NLS dataset and custom-culture differences. Empty-value removal avoids
native lookup. Source review/compilation does not close native acceptance.
