# Platform protocol and packaging C# parity audit

Audit date: 2026-09-27. Rust source is bound to `a716ca42c6b3294295077f800e7ae9dd19e65e56`; the pinned C# oracle is `d31ccbd3bfcddb4705917eafa6483715c166de3b`. The active acceptance ledger keeps F23 and F28 partial until OS delivery/registration and native artifacts are proven (`docs/parity/ACCEPTANCE.md:28-32`, `docs/parity/ACCEPTANCE.md:58-64`).

## Scope and proof rules

- This audit covers the remaining F23 platform work around `nxm://` and `modsync://`: Windows named-pipe single-instance forwarding, executable Windows registration, macOS bundle registration and URL delivery, and native acceptance (`docs/parity/ACCEPTANCE.md:58-58`).
- It covers F28 packaging/resources, platform launch, packaged patcher availability, settings migration, and manual release artifacts (`docs/parity/ACCEPTANCE.md:63-64`).
- Existing parser, fetch, resolver, Nexus download, and Unix-socket behavior is inventoried only where it constrains the remaining work; the broader acquisition implementation belongs to its own slice (`docs/parity/download-orchestration-plan.md:355-368`).
- Source presence is not runtime proof. F23 requires second-instance delivery on every supported platform, and F28 requires native artifacts bound to the tested commit (`docs/parity/ACCEPTANCE.md:58-64`).
- No Rust test or harness may be created, modified, or run before full C# parity; production CLI/native probes are the permitted evidence path (`docs/parity/ACCEPTANCE.md:20-28`).

## Executive verdict

F23 remains partial. The shared Rust parser, guarded instruction fetcher, top-level `nxm` resolver, GUI handoff path, and Unix per-user socket exist (`src/protocol.rs:47-60`, `src/protocol.rs:62-152`, `src/protocol.rs:154-213`, `src/ui.rs:1755-1821`, `src/single_instance.rs:24-76`). Windows is a no-op single-instance implementation, Windows/macOS registration is generation-only, and the registered Windows command would invoke Rust's parse-and-print positional branch instead of the GUI handoff (`src/single_instance.rs:78-84`, `src/protocol_registration.rs:11-46`, `src/main.rs:403-417`).

F28 remains partial. The crate describes a desktop binary and has a three-OS CI test matrix, but it has no Rust release-artifact workflow, app-bundle/desktop-install resources, or packaged patcher payload contract (`Cargo.toml:1-12`, `.github/workflows/ci.yml:1-22`, `Cargo.toml:43-48`). Runtime patcher discovery already searches the executable directory and its `Resources` child, so packaging must populate and verify that contract rather than introduce another lookup path (`src/patcher.rs:8-18`, `src/patcher.rs:59-80`). Settings can read the current `ModSync/settings.json` and legacy `KOTORModSync/settings.json`, but migration still needs packaged-binary probes that prove platform paths, field preservation, permissions, and idempotence (`src/profiles.rs:12-20`, `src/profiles.rs:275-323`).

## F23 C# oracle

### Protocol parsing and component resolution

- The C# `nxm` parser accepts the four-segment `mods/{modId}/files/{fileId}` path, case-insensitive names, and optional `key`, `expires`, and `user_id`; invalid numeric or path forms fail (`C# src/ModSync.Core/Services/Download/NxmUrl.cs:59-139`).
- The C# `modsync` parser accepts `install`/`open`, optional `kotor`/`kotor2`, `url` or `instruction`, and only absolute HTTP(S) instruction URLs (`C# src/ModSync.Core/Services/Protocol/ModSyncUrl.cs:46-139`).
- The C# resolver matches game domain plus mod ID against component `ResourceRegistry` keys and returns `Ambiguous` when different components match (`C# src/ModSync.Core/Services/Download/NxmComponentResolver.cs:32-93`).
- The Rust parser and resolver implement the same core shape, including ambiguity across matching top-level components (`src/protocol.rs:238-347`, `src/protocol.rs:154-213`).

### Single-instance launch contract

- C# parses `--nxm=`, `--modsync=`, and bare positional protocol URLs before the UI starts; when both schemes are present, `nxm` is the single forwarded payload (`C# src/ModSync.GUI/CLIArguments.cs:37-50`, `C# src/ModSync.GUI/CLIArguments.cs:52-115`).
- The first C# process claims a per-user named pipe; a secondary protocol launch forwards the URL and exits, while a secondary ordinary launch forwards an activation token and exits (`C# src/ModSync.GUI/Program.cs:15-64`).
- C# names the pipe per user, sends one BOM-less UTF-8 line with a 3000 ms default timeout, uses one byte-mode asynchronous server, and shuts its listener down with a bounded wait (`C# src/ModSync.GUI/Services/SingleInstanceService.cs:58-68`, `C# src/ModSync.GUI/Services/SingleInstanceService.cs:108-172`, `C# src/ModSync.GUI/Services/SingleInstanceService.cs:270-299`).
- C# uses `System.IO.Pipes` on Windows and Unix, probes a live owner before removing a stale Unix socket, and loops after each connection (`C# src/ModSync.GUI/Services/SingleInstanceService.cs:14-22`, `C# src/ModSync.GUI/Services/SingleInstanceService.cs:70-106`, `C# src/ModSync.GUI/Services/SingleInstanceService.cs:173-242`).
- C# routes received `nxm://`, `modsync://`, and activation messages to different queues/events (`C# src/ModSync.GUI/Services/SingleInstanceService.cs:244-268`).

### Registration contract

- C# Windows registration is per-user under `HKCU\Software\Classes`, writes `URL Protocol`, icon, and quoted open-command values, and executes every `reg.exe` command (`C# src/ModSync.GUI/Services/NxmProtocolRegistrationService.cs:95-143`, `C# src/ModSync.GUI/Services/NxmProtocolRegistrationService.cs:224-237`; `C# src/ModSync.GUI/Services/ModSyncProtocolRegistrationService.cs:67-112`, `C# src/ModSync.GUI/Services/ModSyncProtocolRegistrationService.cs:190-205`).
- C# Linux registration writes a desktop file, asks `xdg-mime` to select it, and refreshes the desktop database (`C# src/ModSync.GUI/Services/NxmProtocolRegistrationService.cs:262-292`; `C# src/ModSync.GUI/Services/ModSyncProtocolRegistrationService.cs:219-244`).
- C# macOS registration is declarative and succeeds only while running inside an app bundle (`C# src/ModSync.GUI/Services/NxmProtocolRegistrationService.cs:239-260`; `C# src/ModSync.GUI/Services/ModSyncProtocolRegistrationService.cs:207-216`).
- The pinned C# `Info.plist` declares `nxm` but not `modsync`, so its runtime service expresses intended `modsync` support more completely than its shipped bundle (`C# src/ModSync.GUI/Info.plist:21-31`; `C# src/ModSync.GUI/Services/ModSyncProtocolRegistrationService.cs:47-64`). This is an oracle defect to correct truthfully, not a defect to reproduce.

## F23 Rust inventory and gaps

| Area | Current Rust evidence | Verdict |
|---|---|---|
| Shared parser | `parse` accepts only `nxm` and `modsync`; typed payloads carry the required coordinates (`src/protocol.rs:13-60`). | Present; retain as the shared authority. |
| Safe `modsync` fetch | Scheme/host/IP checks, redirect revalidation, 5 MiB cap, temporary file, and no-clobber publication exist (`src/protocol.rs:62-152`, `src/protocol.rs:215-236`). | Present source behavior; native network acceptance remains separate. |
| `nxm` resolution | Exactly one matching top-level registry entry is required (`src/protocol.rs:154-213`). | Present; acquisition parity is owned by the download slice. |
| GUI first-instance handoff | `--protocol` reaches `ui::launch`; initial and received URLs call the same `accept_protocol` path (`src/main.rs:30-45`, `src/main.rs:417-432`, `src/ui.rs:2848-2858`, `src/ui.rs:3171-3204`). | Present on the source path. |
| Unix second instance | Per-user Unix socket forwards one activation or protocol string; stale cleanup occurs only after forwarding fails (`src/single_instance.rs:24-76`). | Present source behavior; Linux and macOS runtime proof is still required. |
| Unix takeover safety | Every connect or write error enters the same unlink-and-bind path, although those failures do not prove that no listener owns the socket (`src/single_instance.rs:40-50`, `src/single_instance.rs:72-75`). | Race-prone; distinguish refused/missing from permission, timeout, resource, and write failures and serialize takeover. |
| Windows second instance | Non-Unix `claim_or_forward` always returns a disconnected receiver and guard (`src/single_instance.rs:78-84`). | Missing named-pipe parity. |
| Message framing | Unix reads at most 8193 bytes and admits only non-empty messages at most 8192 bytes (`src/single_instance.rs:53-65`). | Bound exists; Windows must share it and reject extra frames/oversize input. |
| Delivery acknowledgement | A secondary reports success after connect/write; the listener later discards channel-send failure (`src/single_instance.rs:53-65`, `src/single_instance.rs:72-75`). | Transport success is not proof that the primary validated or queued the message. |
| Windows registration execution | Pure string builders exist for both schemes (`src/protocol_registration.rs:11-37`). | Missing execute/status/unregister functions and native proof. |
| Windows launch command | Builders register `"<exe>" "%1"` (`src/protocol_registration.rs:31-36`), while the Rust external-subcommand branch parses, prints JSON, and exits (`src/main.rs:403-415`). | Incorrect for desktop delivery; must register `gui --protocol "%1"` or normalize positional URLs into that same GUI path. |
| macOS declaration | Separate fragments exist for the two schemes (`src/protocol_registration.rs:40-46`). | Not a bundle, not a combined plist, and not running registration proof. |
| Linux registration | One desktop entry declares both MIME schemes and invokes `gui --protocol %u`; runtime calls are best-effort and their exit status is discarded (`src/protocol_registration.rs:48-77`). | Existing partial behavior; registration status/unregister and failure reporting remain open. |
| Handler ownership | Rust has no read-only query showing which executable currently owns either scheme (`src/protocol_registration.rs:1-78`). | Missing; native acceptance cannot distinguish “entry exists” from “ModSync is active” without it. |

## F28 C# oracle

### Package shape and resources

- The C# GUI project declares Windows/macOS icons, app-host output, x86/x64/arm64 platforms, and Windows/Linux/macOS runtime identifiers (`C# src/ModSync.GUI/ModSync.csproj:9-20`, `C# src/ModSync.GUI/ModSync.csproj:34-60`, `C# src/ModSync.GUI/ModSync.csproj:99-107`).
- Its post-build contract copies PyKotor sources and a platform patcher into `Resources`, using `holopatcher` on Linux/macOS and `holopatcher.exe` on Windows (`C# src/ModSync.GUI/ModSync.csproj:216-301`).
- It also selects architecture-specific 7-Zip resources for x64/x86 and conditionally arm64 (`C# src/ModSync.GUI/ModSync.csproj:303-340`).
- Its manual release workflow covers Windows x86/x64/arm64, Linux x64/arm64, and macOS x64/arm64, creates a real `.app` for macOS, zips each RID, retains workflow artifacts, and makes GitHub publication opt-in (`C# .github/workflows/build-and-release.yml:78-130`, `C# .github/workflows/build-and-release.yml:229-280`, `C# .github/workflows/build-and-release.yml:282-374`).
- The C# release runbook requires explicit readiness before the opt-in publish action and distinguishes workflow artifacts from a public release (`C# docs/manual-release.md:9-45`).

### Settings migration

- C# uses the platform `ApplicationData/ModSync/settings.json` path and falls back to `ApplicationData/KOTORModSync/settings.json` only when the current file is absent (`C# src/ModSync.GUI/Models/AppSettings.cs:250-282`).
- C# reads settings case-insensitively and falls back to defaults on absent, invalid, or unreadable data (`C# src/ModSync.GUI/Models/AppSettings.cs:259-311`).
- C# writes the current path and attempts owner-only permissions on Unix (`C# src/ModSync.GUI/Models/AppSettings.cs:315-360`).

## F28 Rust inventory and gaps

| Area | Current Rust evidence | Verdict |
|---|---|---|
| Package metadata | The crate has name/version/description/repository/license and a default desktop feature (`Cargo.toml:1-12`). | Source metadata only; not an installed application identity. |
| Desktop dependencies | `eframe` enables X11, Wayland, and AccessKit; `rfd` is optional with desktop (`Cargo.toml:36-40`). | Build inputs exist; no platform package follows from them. |
| CI | One matrix runs format, Clippy, and Rust tests on Ubuntu, Windows, and macOS (`.github/workflows/ci.yml:1-22`). | Test workflow only; it does not build, inspect, package, retain, or publish release artifacts. |
| Icons and bundle metadata | Cargo contains no resource/bundle configuration beyond release compiler settings (`Cargo.toml:43-48`), and the Rust package inventory has no tracked `.ico`, `.icns`, app plist, or install desktop entry. | Missing release resources and package manifests. The inventory claim must be rechecked when implementation begins. |
| Patcher lookup | Configured path wins; KPatcher/OdyPatcher may come from `PATH`; every backend may come from the executable directory's `Resources` child (`src/patcher.rs:8-18`, `src/patcher.rs:20-80`). | Shared runtime contract exists. |
| macOS patcher layout | A conventional app executable lives under `Contents/MacOS`, so the current executable-relative lookup resolves `Contents/MacOS/Resources`; the C# bundle stages resources under `Contents/Resources` (`src/patcher.rs:8-17`, `src/patcher.rs:45-57`, `C# scripts/ci/bundle-macos-app.ps1:37-60`). | Current lookup and normal `.app` layout disagree; packaging and resolver ownership must change together. |
| Patcher use | Engine construction resolves the selected patcher; a Patcher instruction fails if none is configured or the resolved file is missing (`src/engine.rs:188-220`, `src/engine.rs:1565-1574`). | Package must provide a supported executable or report the capability unavailable before installation. |
| Patcher argv | KPatcher adds `--console` off Windows; all patchers receive install/game/TSLPatcher-data arguments; OdyPatcher adds `--cli -y` (`src/engine.rs:1700-1714`). | Artifact probes must exercise the packaged executable, not merely find its filename. |
| Current settings path | Default settings use the platform config directory plus `ModSync` (`src/profiles.rs:12-20`). | Intended path matches the C# application name; native path resolution remains unproven. |
| Legacy settings | Reads fall back to `KOTORModSync/settings.json`; ordinary preference readers preserve the full object, while managed-only reads deliberately retain two fields (`src/profiles.rs:275-307`). | Partial migration logic exists; full-field write-forward and malformed-input behavior need direct probes. |
| Explicit settings root | Legacy lookup uses the process-wide platform config root even when `Store` was created with an explicit root (`src/profiles.rs:122-139`, `src/profiles.rs:275-297`). | An isolated `--settings-dir` can read unrelated user legacy data; F28 must hand this correction to the `profiles.rs` owner. |
| Malformed settings | Rust propagates filesystem, JSON, and object-shape failures, while the C# GUI returns defaults after load/deserialize failures (`src/profiles.rs:278-307`, `C# src/ModSync.GUI/Models/AppSettings.cs:266-313`). | Behavioral difference is explicit; desktop and CLI policy must be settled and proven separately. |
| Atomic preference writes | API key, spoiler, patcher, theme, and directory changes write the current `settings.json` atomically (`src/profiles.rs:306-323`, `src/profiles.rs:324-350`, `src/profiles.rs:352-410`, `src/profiles.rs:413-425`). | Useful migration primitive; package/native receipts still required. |
| Release controls | Rust has no manual artifact or release workflow beyond `.github/workflows/ci.yml:1-22`. | Missing F28 artifact chain. Public publication remains outside this plan without a separate user request (`docs/parity/ACCEPTANCE.md:63-64`). |

## Proof available from the Linux host

The following evidence can be produced on Linux after implementation without claiming native runtime behavior:

1. Build a release Linux artifact and a Windows cross-target artifact from one commit, then record commit, `Cargo.lock`, executable hashes, file types, archive manifests, and dependency/import tables. This proves artifact composition, not Windows launch (`Cargo.toml:1-48`, `.github/workflows/ci.yml:1-22`).
2. Inspect the Windows PE for the intended subsystem, icon/version resources, target architecture, and packaged patcher/resources. This proves static embedding and file layout, not registry or named-pipe execution (`src/protocol_registration.rs:11-37`, `src/patcher.rs:59-80`).
3. Extract the macOS package produced by a native CI artifact and inspect `Info.plist`, executable architecture, both URL schemes, icon, patcher/resources, and archive hashes. A Linux inspection proves package contents but not Launch Services delivery (`src/protocol_registration.rs:40-46`, `C# src/ModSync.GUI/Info.plist:5-31`).
4. Install the Linux package under a disposable `XDG_DATA_HOME`/`XDG_CONFIG_HOME`, query both MIME handlers, launch each URL, forward a second URL to the live primary, migrate a copied legacy settings file, and remove the package. This is Linux native evidence only (`src/protocol_registration.rs:48-77`, `src/single_instance.rs:24-76`, `src/profiles.rs:12-20`).

## Evidence that requires a native target

- Windows must prove HKCU registration/status/unregistration for both schemes, browser/shell launch into a first instance, named-pipe forwarding into a running primary, activation-only forwarding, stale/no-primary recovery, quoted executable paths, and x64 plus arm64 artifacts where shipped (`src/protocol_registration.rs:11-37`, `src/single_instance.rs:78-84`).
- macOS must prove an installed `.app` declares both schemes, Launch Services opens a cold app, a warm app receives another URL without losing it, ordinary second launch activates the existing window, quarantine/signing behavior is recorded, and the packaged patcher runs on each shipped architecture (`src/protocol_registration.rs:40-46`, `src/ui.rs:3171-3204`, `src/patcher.rs:45-57`).
- Native proof must be bound to the exact artifact hash and commit; CI job success or Linux-side archive inspection cannot substitute for installed-artifact behavior (`docs/parity/ACCEPTANCE.md:58-64`).

## Acceptance blockers

1. Windows `claim_or_forward` is still a stub (`src/single_instance.rs:78-84`).
2. The generated Windows open command does not enter the GUI handoff (`src/protocol_registration.rs:31-36`, `src/main.rs:403-415`).
3. macOS fragments are not integrated into an app bundle and do not establish URL event delivery (`src/protocol_registration.rs:40-46`, `Cargo.toml:1-48`).
4. No platform exposes truthful register/status/unregister behavior for both schemes (`src/protocol_registration.rs:1-78`).
5. No Rust release workflow emits commit-bound packages or validates resource/patcher composition (`.github/workflows/ci.yml:1-22`, `src/patcher.rs:59-80`).
6. Settings fallback has no packaged-native migration receipt (`src/profiles.rs:275-323`).
7. Explicit settings roots can consult the global legacy root, and malformed-settings behavior differs from the C# GUI (`src/profiles.rs:122-139`, `src/profiles.rs:275-307`, `C# src/ModSync.GUI/Models/AppSettings.cs:266-313`).
8. Conventional macOS bundle resources are outside the current patcher lookup path (`src/patcher.rs:8-17`, `src/patcher.rs:45-57`, `C# scripts/ci/bundle-macos-app.ps1:37-60`).
9. No Windows or macOS installed-artifact receipt exists, so F23/F28 cannot close (`docs/parity/ACCEPTANCE.md:58-64`).

## Non-claims

- The existing Unix socket does not prove Windows named-pipe behavior (`src/single_instance.rs:6-12`, `src/single_instance.rs:78-84`).
- Registration command strings do not prove registry or Launch Services state (`src/protocol_registration.rs:11-46`).
- A cross-compiled PE does not prove Windows GUI startup, named-pipe delivery, or registry ownership (`src/single_instance.rs:78-84`).
- A plist containing scheme names does not prove macOS cold/warm URL delivery (`src/protocol_registration.rs:40-46`).
- Finding a patcher-shaped file does not prove it launches, accepts the Rust argv, or produces correct patch bytes (`src/engine.rs:1565-1574`, `src/engine.rs:1700-1714`).
- A workflow artifact is not a public release, and this scope does not authorize publication (`C# docs/manual-release.md:28-45`, `docs/parity/ACCEPTANCE.md:63-64`).

## Owned registration source implementation (2026-10-02)

`protocol_registration::owned` now exposes shared `Request`, `Scheme`, typed
per-scheme ownership status and register/status/unregister operations. Identity
binds a validated package token and channel to the canonical executable. Explicit
replacement is required before another active association can be changed.
CLI/UI adapters consume this API; source presence does not close F23.

- Linux uses separate per-scheme desktop files, exact marker/command comparison,
  durable prior-handler receipts, and bounded `xdg-mime` queries. Unregister removes
  only this identity's exact artifact and MIME entries; an inactive old package
  can clean its own artifact while preserving the selected newer handler. A prior
  handler is restored only when this identity was still selected.
- Windows uses HKCU Registry APIs, exact command/ownership values and subtree
  shape checks before removal. Windows UserChoice overrides are reported and
  preserved; users must change a protected default through Windows Settings.
  The registered command enters `gui --protocol "%1"`.
- macOS validates the package token/channel/executable and both declarations in
  the actual bundle Info.plist, queries the selected application URL, registers
  through Launch Services and verifies the resulting bundle location. Whole-bundle
  unregister requires both schemes to remain owned and both to be requested;
  per-scheme-only Launch Services unregister is not claimed. A loose executable
  cannot register as an owned application bundle.

Cooperating mutations are serialized with per-user regular-file locks, bounded
waits and cancellation. Backends recheck ownership before removal; unrelated OS
configuration tools do not honor these locks, so the APIs do not claim a transaction
against arbitrary concurrent external configuration writers. OS helper commands
have a 15-second deadline, retained child/process-group cleanup and bounded UTF-8
output reads. Their 256KiB output-file ceiling is polled, not a hard disk quota.
Failures propagate, including failures after an earlier scheme changed; there is
no implicit rollback or partial-success claim.

No registration, native protocol delivery, package installation, filesystem tests
or acceptance probes were executed for this slice. Windows/macOS compilation and
native association behavior remain acceptance obligations, as do Linux desktop
integration and A-register/B-upgrade/A-uninstall acceptance with real packages.
