# F09 patcher C# audit

## Audit status and source binding

This is a source audit for F09 only: external execution, HoloPatcher/KPatcher/OdyPatcher routing, argument construction, Python resources and setup, packaged-backend availability, and a patch-byte differential against the C# behavior. It does not claim a build, a runnable package, or patch-byte parity. F09 remains partial until the probes in `docs/parity/patcher-plan.md` produce durable receipts (`docs/parity/ACCEPTANCE.md:44`, `docs/parity/ACCEPTANCE.md:121`, `docs/parity/ACCEPTANCE.md:316-324`; `COMPATIBILITY.md:115-119`, `COMPATIBILITY.md:149-152`).

The C# oracle is `/home/brunner56/.codex/worktrees/5813/ModSync` at commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`, confirmed with `git -C /home/brunner56/.codex/worktrees/5813/ModSync rev-parse HEAD`. The Rust planning base observed at final review is `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`, confirmed with `git -C /home/brunner56/Workspaces/ModSync.rs rev-parse HEAD`; the per-file hashes in section 8 bind the source snapshot if concurrent work advances that branch again. All C# references below are relative to the pinned worktree; all Rust references are relative to this repository. `AGENTS.md` references are to the committed 12-line file (`git show HEAD:AGENTS.md`), not to a modified working copy.

No build, test, external patcher, or game tree was run for this audit. That boundary is required by the current test freeze and by the rule that validation must not mutate trees or launch programs (`docs/parity/ACCEPTANCE.md:26-32`; `AGENTS.md:7-10`).

## 1. C# execution call chain

The persisted backend names are `Holopatcher`, `KPatcher`, and `OdyPatcher`; HoloPatcher is the default (`src/ModSync.Core/PatcherEngines.cs:7-13`; `src/ModSync.Core/MainConfig.cs:194-210`). The core configuration also stores separate KPatcher and OdyPatcher paths (`src/ModSync.Core/MainConfig.cs:213-229`).

`ModComponent` resolves a patcher's sources, rejects an empty resolution, applies the managed-install hook, and then calls `ExecuteTSLPatcherAsync`; `Execute` and `Run` instead call `ExecuteProgramAsync` (`src/ModSync.Core/ModComponent.cs:1630-1646`). Destination validation defaults an empty patcher destination to the game and rejects destinations other than the game (`src/ModSync.Core/ComponentValidation.cs:265-290`).

The patcher path loops through every resolved source in guide order (`src/ModSync.Core/Instruction.cs:1298-1306`). The generic external-program path likewise resolves possibly multiple sources, executes them in order, and returns on the first nonzero result (`src/ModSync.Core/Instruction.cs:1469-1515`). This ordering and stop condition are observable C# behavior, not presentation behavior.

The virtual-filesystem route simulates a patcher instruction without starting a process (`src/ModSync.Core/Instruction.cs:1308-1325`). That is planning or validation metadata only; it is not evidence that a real backend is available or that a patch was applied (`src/ModSync.Core/Services/FileSystem/VirtualFileSystemProvider.cs:1076-1087`; `docs/parity/ACCEPTANCE.md:121`).

## 2. Patcher preparation, arguments, and result checks

Before each C# patcher launch, both supported install-log names are deleted from the patch data directory (`src/ModSync.Core/Instruction.cs:1328-1338`). The C# implementation then rewrites the selected INI so the patcher emits a plaintext log, does not look up the game folder, and does not display its confirmation message (`src/ModSync.Core/Instruction.cs:1340-1342`). This mutates the resolved source tree; it is observable oracle behavior but conflicts with the Rust project's stronger rule that validation and probing must not mutate mod trees (`AGENTS.md:7-8`).

The common C# argument tail is:

```text
--install --game-dir <game> --tslpatchdata <source>
```

It is constructed at `src/ModSync.Core/Instruction.cs:1344-1361`. OdyPatcher additionally receives `--cli -y`, appended after `--tslpatchdata <source>` and before any `--namespace-option-index` (`src/ModSync.Core/Instruction.cs:1354-1373`). Rust emits the same order: the common tail, then `--cli -y`, then the namespace index appended by the caller (`src/engine.rs:1706-1713`, `src/engine.rs:1589-1591`). KPatcher on non-Windows receives `--console` in the shared runner (`src/ModSync.Core/Services/InstallationService.cs:771-787`).

Any nonblank patcher `Arguments` value is trimmed and forwarded as `--namespace-option-index <value>` (`src/ModSync.Core/Instruction.cs:1369-1373`). If the value parses as an integer, the C# UI warning describes it as a namespace choice (`src/ModSync.Core/Instruction.cs:1389-1398`). The C# path does not interpret a nonnumeric value as the name of an alternate INI.

After process exit, a nonzero exit code is a patcher failure (`src/ModSync.Core/Instruction.cs:1427-1431`). A zero exit code is still insufficient: C# requires a patcher log, rejects lines containing `Error: ` or `[Error]`, and reports a missing or unreadable log separately (`src/ModSync.Core/Instruction.cs:1433-1448`; `src/ModSync.Core/Instruction.cs:1556-1603`).

The C# delete and read locations are not the same directory. The patch directory is the source itself when it is a directory, or its parent when it is a file (`src/ModSync.Core/Instruction.cs:1298-1302`), and the stale-log delete runs there (`:1328-1338`). `VerifyInstall()` is called with no argument, so it walks every `RealSourcePaths` entry after each source, and it reads the log from the parent of each source path (`:1435`, `:1563-1566`, `:1579-1591`). For an `.exe` source the two directories coincide. For a directory source, C# deletes logs in `<source>` but reads them from `<source>/..`, so a stale parent log can satisfy or fail the check. Rust deletes and reads the same directory, `base` or the staged copy (`src/engine.rs:1579-1587`, `src/engine.rs:1646-1651`, `src/engine.rs:1686-1687`, `src/engine.rs:1718-1722`). Rust also sets the child working directory to the source parent (`src/engine.rs:1656`). Which directory Holo, K and Ody actually write the log to is not established by source; `docs/parity/patcher-plan.md` P11 measures it.

The generic `Execute`/`Run` path replaces placeholders inside the instruction's argument string for each source, preserves source order, and stops at the first failure (`src/ModSync.Core/Instruction.cs:1497-1515`). Arguments are quoted through the C# process-argument helper (`src/ModSync.Core/Instruction.cs:1535-1554`).

The shared process runner captures standard output and standard error asynchronously, then waits for process completion (`src/ModSync.Core/Utility/PlatformAgnosticMethods.cs:434-517`). The patcher calls do not supply a timeout, so this path does not provide the Rust engine's cancellation polling behavior (`src/ModSync.Core/Services/InstallationService.cs:747-790`; `src/ModSync.Core/Utility/PlatformAgnosticMethods.cs:359-396`).

## 3. Backend routing and discovery

### HoloPatcher

C# prefers a native HoloPatcher unless its Python preference is enabled, then falls back to the bundled Python source when available (`src/ModSync.Core/Services/InstallationService.cs:364-408`). Windows native discovery looks under application resources for `holopatcher.exe` (`src/ModSync.Core/Services/InstallationService.cs:299-318`). Non-Windows discovery checks packaged application/resource locations (`src/ModSync.Core/Services/InstallationService.cs:319-340`).

If the selected executable is external, C# uses the common CLI runner. Otherwise it chooses the native bundled runner or the Python runner (`src/ModSync.Core/Instruction.cs:1376-1425`). HoloPatcher is therefore a three-way route in the oracle: configured/external native, bundled native, or bundled Python source.

### KPatcher

C# checks a configured KPatcher path, then the process search path, an application sibling, and application resources (`src/ModSync.Core/Services/InstallationService.cs:617-688`). A configured path that is absent produces a warning and discovery continues (`src/ModSync.Core/Services/InstallationService.cs:621-633`). On non-Windows systems the launch receives `--console` (`src/ModSync.Core/Services/InstallationService.cs:771-787`).

### OdyPatcher

C# uses the same configured-path, search-path, application-sibling, and resources pattern for OdyPatcher (`src/ModSync.Core/Services/InstallationService.cs:690-745`). The argument order is fixed in `Instruction.cs`: `--cli -y` comes after `--tslpatchdata <source>` (`src/ModSync.Core/Instruction.cs:1354-1367`). The OdyPatcher runner only trims the joined string and launches it (`src/ModSync.Core/Services/InstallationService.cs:753-769`); it adds no arguments.

### Settings surfaces are not symmetrical

The CLI exposes backend selection and both KPatcher and OdyPatcher path options (`src/ModSync.Core/CLI/ModBuildConverter.cs:640-650`). It applies those options before a direct-Markdown install and requires OdyPatcher for that route (`src/ModSync.Core/CLI/ModBuildConverter.cs:3367-3391`). However, the CLI's serialized settings data contains the engine and KPatcher path but not an OdyPatcher path (`src/ModSync.Core/CLI/ModBuildConverter.cs:178-186`, `src/ModSync.Core/CLI/ModBuildConverter.cs:294-295`, `src/ModSync.Core/CLI/ModBuildConverter.cs:352-356`).

The GUI settings dialog exposes only HoloPatcher and KPatcher (`src/ModSync.GUI/Dialogs/SettingsDialog.cs:119-174`). GUI settings persistence likewise maps only the engine and KPatcher path (`src/ModSync.GUI/Models/AppSettings.cs:78-84`, `src/ModSync.GUI/Models/AppSettings.cs:143-145`, `src/ModSync.GUI/Models/AppSettings.cs:201-203`). The missing OdyPatcher GUI/persistence surface is an oracle limitation, not a feature to remove from Rust.

## 4. Python runtime and resources

The C# core depends on `Python.Included` 3.11.6 (`src/ModSync.Core/ModSync.Core.csproj:71-74`). Initialization is serialized, disables the keyring integration, provisions Python, initializes Python.NET, and checks for the Python dependencies used by HoloPatcher (`src/ModSync.Core/Services/InstallationService.cs:89-126`). If dependencies are absent, the C# application invokes pip at runtime (`src/ModSync.Core/Services/InstallationService.cs:129-246`).

The bundled HoloPatcher Python entry point is expected at `Resources/PyKotor/Tools/HoloPatcher/src/holopatcher` (`src/ModSync.Core/Services/InstallationService.cs:345-362`). The Python route constructs `sys.argv`, runs the HoloPatcher `__main__.py`, captures Python output, and maps failures to the common result type (`src/ModSync.Core/Services/InstallationService.cs:425-492`, `src/ModSync.Core/Services/InstallationService.cs:512-566`).

The GUI project copies `.py` files from its PyKotor vendor tree into publish output (`src/ModSync.GUI/ModSync.csproj:218-231`). It also contains platform-conditioned native HoloPatcher copy steps for macOS, Linux x64, and a final Windows-shaped branch (`src/ModSync.GUI/ModSync.csproj:233-301`). The declared runtime list is wider than those explicit packaging branches, including Linux arm64 and multiple Windows architectures (`src/ModSync.GUI/ModSync.csproj:48-60`). Therefore the declared runtime list alone is not proof that every artifact contains a usable native backend.

The C# repository's agent setup scripts manually link the Linux HoloPatcher binary and link resources into the core output (`scripts/agents/ensure_linux_holopatcher.sh:4-28`; `scripts/agents/common.sh:4-15`). Their documentation says that full runs need this setup and can fail when HoloPatcher is absent (`scripts/agents/README.md:21-29`, `scripts/agents/README.md:81-104`, `scripts/agents/README.md:134-148`). This reinforces that source presence is weaker than packaged-artifact availability.

## 5. C# behaviors that are not parity targets

The C# environment check launches each selected backend with help or deliberately incomplete install arguments (`src/ModSync.Core/Services/InstallationService.cs:816-923`). Rust must not port that behavior into validation because this project's validation contract forbids external program execution (`AGENTS.md:8`; `src/engine.rs:864-873`). Backend executability belongs to an explicit install/probe action.

Runtime pip installation is also not a safe packaging contract. The parity outcome is an offline-capable HoloPatcher route with all required resources, not an implicit network mutation during installation (`src/ModSync.Core/Services/InstallationService.cs:89-246`; `AGENTS.md:7-10`).

The GUI and CLI omission of persisted OdyPatcher configuration is not a behavior to copy. The core explicitly supports an OdyPatcher path, and Rust already models one (`src/ModSync.Core/MainConfig.cs:213-229`; `src/engine.rs:30-49`; `src/profiles.rs:352-394`).

The broad final packaging condition after the macOS and Linux-x64 branches must not be treated as proof for Linux arm64 merely because that runtime is declared (`src/ModSync.GUI/ModSync.csproj:48-60`, `src/ModSync.GUI/ModSync.csproj:233-301`). Each produced artifact needs its own backend manifest and executable probe.

## 6. Rust implementation inventory and deltas

| Area | Current Rust behavior | C# oracle and required disposition |
| --- | --- | --- |
| Backend model | HoloPatcher is the default and settings hold one path per backend (`src/engine.rs:22-49`). | Same three core backend choices and default (`src/ModSync.Core/PatcherEngines.cs:7-13`; `src/ModSync.Core/MainConfig.cs:194-229`). Keep. |
| Resolution | `src/patcher.rs` checks a configured path, backend-specific names, application locations, resources, and selectively PATH (`src/patcher.rs:8-79`). An invalid configured path fails immediately (`src/patcher.rs:20-30`). | C# continues discovery after an invalid configured KPatcher/OdyPatcher path (`src/ModSync.Core/Services/InstallationService.cs:617-745`). Preserve Rust's fail-closed configured-path rule as an explicit safety difference. |
| Source cardinality | External processing requires exactly one source (`src/engine.rs:1555-1558`). | C# loops every resolved patcher or executable source in order (`src/ModSync.Core/Instruction.cs:1298-1306`, `src/ModSync.Core/Instruction.cs:1469-1515`). Add ordered multi-source execution and stop on failure. |
| Trust gate | External execution requires explicit permission (`src/engine.rs:1550-1554`). | C# directly launches the configured instruction path (`src/ModSync.Core/Instruction.cs:1298-1457`, `src/ModSync.Core/Instruction.cs:1458-1533`). Preserve the Rust gate; external execution remains off by default (`COMPATIBILITY.md:128-129`). |
| Arguments | Rust constructs structured backend arguments, including KPatcher `--console` and OdyPatcher `--cli -y` (`src/engine.rs:1587-1600`, `src/engine.rs:1700-1715`). | The common backend shape and the OdyPatcher order match C# (`src/ModSync.Core/Instruction.cs:1349-1373`); KPatcher's `--console` prefix matches the runner (`src/ModSync.Core/Services/InstallationService.cs:771-787`). Retain structured arguments. |
| Alternate INI | A numeric value becomes a namespace index; a safe nonnumeric `.ini` is staged as `changes.ini` (`src/engine.rs:1589-1638`). | C# forwards every nonblank value as the namespace index (`src/ModSync.Core/Instruction.cs:1369-1373`). Treat named INI as a Rust extension and exclude it from claims of exact C# argument parity. |
| Generic process arguments | Rust tokenizes raw arguments with the Windows command-line rules on every host (`src/engine/process_arguments.rs`; `shlex` on Unix before 2026-10-04). | C# performs placeholder replacement per source before launch (`src/ModSync.Core/Instruction.cs:1497-1515`). Add placeholder replacement without abandoning structured process arguments. 2026-10-04: placeholders are substituted per token, and the text is split with the Windows rules .NET applies to `ProcessStartInfo.Arguments` on every host (no longer `shlex` on Unix); see [execute-arguments-contract.md](execute-arguments-contract.md). |
| Logs | Rust deletes stale logs, scans a present log for errors, but accepts absence (`src/engine.rs:1647-1651`, `src/engine.rs:1685-1693`, `src/engine.rs:1718-1733`). It reads the same directory it cleaned (`src/engine.rs:1722`). | C# requires a readable log and rejects its error markers (`src/ModSync.Core/Instruction.cs:1433-1448`, `src/ModSync.Core/Instruction.cs:1556-1603`). It cleans the patch directory but reads the parent of each source (`:1328-1338` versus `:1582-1591`), so C# does not guarantee freshness for directory sources. Missing-log success is a parity defect. A fresh-log requirement is a deliberate deviation, not C# parity. |
| Process control | Rust redirects output, polls cancellation, kills on cancellation, and caps captured streams at 1 MiB each (`src/engine.rs:1653-1684`). | C# asynchronously captures output and waits without a patcher timeout (`src/ModSync.Core/Utility/PlatformAgnosticMethods.cs:434-517`). Preserve cancellation; document the bounded-output safety difference. |
| Validation | The Rust validation path is expressly non-executing (`src/engine.rs:864-873`). | C# launches patchers during its environment check (`src/ModSync.Core/Services/InstallationService.cs:816-923`). Preserve Rust behavior. |
| Packaged backend | Resolution knows application and resources locations (`src/patcher.rs:68-79`), but the Cargo manifest only declares Rust dependencies and profiles (`Cargo.toml:13-47`). | C# has explicit resource-copy rules, albeit with proof gaps (`src/ModSync.GUI/ModSync.csproj:174-301`). Rust needs a pinned asset manifest, artifact staging, and artifact-level probes. |
| Managed byte accounting | Engine processing rescans after an external process (`src/engine.rs:1694-1697`). | C# snapshots game hashes around patching and records changed files as managed provenance (`src/ModSync.Core/Services/Installation/ManagedInstallSession.cs:202-290`). Integration with the existing Rust recovery/deployment lane must be reviewed separately before parity is claimed. |

The existing CLI and GUI already expose backend/path configuration through adapters (`src/main.rs:321-333`, `src/main.rs:382-398`, `src/main.rs:924-947`; `src/ui.rs:1454-1495`). Those files are presentation surfaces. The execution and install semantics must remain in the shared engine and patcher library (`AGENTS.md:5`; `src/lib.rs:16-22`).

## 7. Patch-byte proof boundary

Matching a generated destination list, a virtual file system, command-line text, or a successful process exit does not demonstrate an equivalent patch (`docs/parity/ACCEPTANCE.md:121`, `docs/parity/ACCEPTANCE.md:316-324`; `COMPATIBILITY.md:115-119`). F09 requires a fresh baseline for each run, the same pinned backend binary and patch input on both the C# and Rust routes, and a byte-for-byte manifest of the resulting game tree. The manifest must include at least path, file size, and SHA-256, and the receipt must retain backend hashes, source hashes, argv, exit status, logs, and the baseline identity. This is the acceptance method specified in `docs/parity/patcher-plan.md`, not evidence produced by this audit.

Named-INI staging, backend-to-backend comparisons, and virtual/simulated installs may be useful secondary checks, but none substitutes for comparing Rust and C# outputs with the same backend and fresh baseline (`src/engine.rs:1593-1638`; `src/ModSync.Core/Instruction.cs:1308-1325`; `docs/parity/ACCEPTANCE.md:121`).

## 8. Audited source hashes

These hashes bind the audit to the files inspected. They were obtained read-only with `sha256sum` in the two repositories.

| Repository | File | SHA-256 |
| --- | --- | --- |
| C# | `src/ModSync.Core/PatcherEngines.cs` | `1b7796fb158e9ed148b05b0a02ee23383189d0fde9e27c589c02fa9f40b687d5` |
| C# | `src/ModSync.Core/Instruction.cs` | `1948c010164df1f7c3d46e095ffd9eeea33f8305bae3cb12d3151a27ff538337` |
| C# | `src/ModSync.Core/Services/InstallationService.cs` | `3989b0e267edcd80eda4a1f8aea9b9c349a5322fbdac6dbd34b8d9651d713230` |
| C# | `src/ModSync.Core/ModComponent.cs` | `52a36fcefea7110612c794d734acd01921015bc91d13029a8a785a8530c9b0bb` |
| C# | `src/ModSync.Core/MainConfig.cs` | `ae850751e8d7849510a74d652ea2840688b9a71be389ff114e0ba343b13c51ae` |
| C# | `src/ModSync.Core/CLI/ModBuildConverter.cs` | `bff23959668fe4192c30d4cabed7b85359aedc585fee0212b62853f9f2296de3` |
| C# | `src/ModSync.Core/ModSync.Core.csproj` | `fc6006465a7706169224e6698dca501f4c3c26038de3571e231934fba277c8ed` |
| C# | `src/ModSync.GUI/ModSync.csproj` | `b9621e616152f4b20889f1f9b61d6d475663494dcbbf582cdbeeac34c1d365bc` |
| C# | `src/ModSync.GUI/Models/AppSettings.cs` | `4967bea5e804e96ed71996df8c5b972c53ddaabd9c2f93a917c3d323762b5d5a` |
| C# | `src/ModSync.GUI/Dialogs/SettingsDialog.cs` | `4f3e69b98cc5e26b3d9bb643edef86c72809483be2990c29ef8170e49b594c6b` |
| Rust | `src/patcher.rs` | `50ebe6ff192e6137facecfd22fbd05d02e985af716a31bde5e7f098a34f2b1a1` |
| Rust | `src/engine.rs` | `05b9de341f7734734ec7a18eb57b378549eaae795bf4db9803fa0c451911c1dc` |
| Rust | `src/main.rs` | `d8881ea10808cd1a97f450af182d7198fb3c473c622a5ca4ad8e057984395ad1` |
| Rust | `src/profiles.rs` | `a986d19482f1e76eb4ed2d9abd3329849f3cf2db8e1b4f805ccf2cd60420142d` |
| Rust | `Cargo.toml` | `048cdee72040f02acde7431f66ee978843ce005dfebe66d5d2c2ed8774858cb8` |

`ACCEPTANCE.md` and `COMPATIBILITY.md` were read as working-tree documents because they already contain concurrent edits. Their file:line references are authoritative for this audit's current planning context, but their contents are not represented as committed at the Rust base SHA.
