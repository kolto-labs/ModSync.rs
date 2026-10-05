# ModSync.rs

A Rust desktop app and CLI for installing mods from ModSync instruction files. The desktop wizard and CLI share the same selection, validation, archive, and installation engine. No .NET runtime is required.

This is an initial implementation, not a verified replacement for every feature in the C# application. See [COMPATIBILITY.md](COMPATIBILITY.md) for the supported behavior and remaining differences.

## Run

Install Rust stable and a C/C++ build toolchain, then:

```sh
git clone https://github.com/bodencrouch/ModSync.rs.git
cd ModSync.rs
cargo run --release
```

The default build includes the desktop app. Linux needs an X11 or Wayland session with OpenGL support. File dialogs use the desktop portal. Windows builds need the MSVC C++ tools; macOS needs the Xcode command-line tools. The RAR reader compiles the bundled UnRAR C++ library.

For a CLI-only binary:

```sh
cargo build --release --no-default-features
./target/release/modsync --help
```

## Try an isolated install

Use empty sample folders. These files are demonstration data, not game assets.

```sh
mkdir -p demo-work/game/Override demo-work/mods
printf 'sample texture\n' > demo-work/mods/sample.tga
printf 'old texture\n' > demo-work/game/Override/sample.tpc
cargo run -- gui --input examples/demo.toml \
  --game-dir demo-work/game --source-dir demo-work/mods
```

Load a guide, choose components and options, then run validation. Review the report and install. Work runs on a background thread; the Activity panel shows results and lets you request cancellation.

The same install from the CLI:

```sh
cargo run -- validate --input examples/demo.toml \
  --game-dir demo-work/game --source-dir demo-work/mods --json
cargo run -- install --input examples/demo.toml \
  --game-dir demo-work/game --source-dir demo-work/mods
```

The result is `game/Override/sample.tga`; the conflicting `sample.tpc` is removed.

## Use existing guides

```sh
modsync list KOTOR1_Full.toml
modsync validate --input KOTOR1_Full.toml --game-dir /path/to/game --source-dir /path/to/mods
modsync convert instructions.toml instructions.json
```

CLI validation and installation select all top-level components by default, matching ModSync. Use `--use-file-selection` to honor `IsSelected`, or repeat `--select NAME_OR_GUID` (or list several values after one `--select`) to select specific components. Repeat `--option GUID` to enable options. Option defaults from the guide remain intact. Dependencies must be selected explicitly; missing dependencies and incompatible selections stop validation.

The desktop can save selections as a TOML file. The CLI reads the same file with `--use-file-selection`.

For a prose Markdown guide that is the sole install authority, use `--direct-markdown`. It loads generated draft actions, skips 4GB Patcher and widescreen-only entries, and stops before game writes until every selected draft has been explicitly reviewed. It cannot be combined with component/profile/option/FOMOD selection overrides.

## Run patchers

Point to an installed patcher executable (HoloPatcher is the C# default backend):

```sh
modsync install --input guide.toml --game-dir /path/to/game --source-dir /path/to/mods \
  --allow-processes --patcher /path/to/odypatcher
```

`Patcher` selects HoloPatcher by default; select `--patcher-engine ody-patcher` for OdyPatcher (`--install --game-dir … --tslpatchdata … --cli -y`) or `--patcher-engine k-patcher` for KPatcher (which adds `--console` on non-Windows). All backends accept a namespace index or an INI filename in `Arguments`. Named INI installs use a temporary copy of `tslpatchdata`, leaving the original patcher configuration intact. `Execute` and `Run` invoke the specified local program directly, without a shell. External programs have your user permissions and require the explicit trust switch. Windows executables require a Windows host; no implicit Wine execution is provided.

Validation checks process inputs but does not execute programs or predict their effects. It may reject a later step that requires a file an external program would create. A successful validation report is not proof that an external patcher will succeed.

## Download archives

```sh
modsync download https://example.org/mod.zip /path/to/mods/mod.zip --sha256 EXPECTED_HASH
```

Direct HTTPS ZIP, 7z, RAR, TAR, and GZip downloads are checked before being saved. HTML responses, archive corruption, unsafe names, and checksum mismatches fail. Existing downloads are never overwritten. Nexus/DeadlyStream landing pages and MEGA links remain browser workflows; the desktop exposes guide links for that purpose. Archive expansion and file copies stream through temporary storage. Use `--scratch-dir <existing-folder>` for validate/install to choose storage outside the game and mod trees, and `--max-expanded-bytes <bytes>` to set an optional per-archive expansion budget. Both controls are available under Archive storage in the desktop. Download integrity checks accept the same byte budget.

For an exact archive published through BitTorrent, a guide maintainer runs `modsync torrent-seed <archive> --tracker <http-announce-url>`. It prints JSON with the magnet URI, the SHA-256 and `torrentInfo`, then seeds until Ctrl+C. The guide records all three for the archive (`Magnet`, `ContentHashSHA256`, `TorrentInfo`). A recipient runs `modsync torrent-download '<magnet-uri>' <output> --sha256 <digest> --torrent-info <base64>`. The torrent must contain one archive. ModSync downloads into temporary storage, requires the authored SHA-256, validates the archive, and publishes it without overwriting an existing file.

Peer traffic goes through a SOCKS5 proxy by default, so peers see the proxy's address, not yours. The proxy operator can see your traffic. Use `--peer-proxy socks5://IP:PORT`, or let ModSync try the public list at `--peer-proxy-list-url` (default: monosans/proxy-list). Trackers come from `--tracker-list-url` (default: the ngosang HTTP and HTTPS lists). With no working proxy, the command fails and opens no peer connection. In this mode ModSync opens no listening port and uses no DHT, UPnP, local discovery or UDP trackers. `torrent-seed` then prints `"peer": null`. `--no-proxy` connects directly, with a listening port, DHT and UPnP, and peers see your IP address; it prints the listening peer address, which a recipient can pass as `--peer <address>`.

To share under an authored identity, run `modsync torrent-seed <archive> --authored-magnet '<magnet>' --torrent-info <base64>`. ModSync never creates a new torrent in this mode. It shares a private copy of the archive, never the file itself, and deletes the copy when sharing stops. Sharing stops after 24 hours or an upload ratio of 1.0 (`--share-hours`, `--share-ratio`; 0 means no limit). Uploads are capped at 100 KiB/s (`--upload-kibps`). When sharing stops it prints a second JSON line with `outcome`, `reason`, `infoHash`, `uploadedBytes` and `seconds`.

## Recover a failed install

Installation validates first, locks both folders, and snapshots the game and mod trees. It stops at the first failure. A pending recovery marker prevents another install on top of partial changes.

```sh
modsync restore --game-dir /path/to/game --source-dir /path/to/mods
```

Restore verifies the snapshot hashes before changing either folder and verifies restored files afterward. Then fix the cause and rerun from the start, in guide order. Never append a failed early mod to the end of an installation.

Snapshots live beside the game directory in `.modsync-<id>/snapshot`. They require space for full copies of both folders. The most recent successful run's snapshot remains until the next run; `restore` is deliberately limited to unfinished installations. Keep independent backups of valuable game data. The process lock coordinates this app; do not edit either tree with other tools during installation or restoration.

## Check the build

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --no-default-features
```

The test suite covers simulated and real file changes, ZIP/7z/RAR, path and symlink rejection, dependency ordering, option selection, texture cleanup, cancellation, recovery hashes, and the CLI. CI runs on Linux, Windows, and macOS.

## Attribution

Instruction behavior is based on [ModSync](https://github.com/th3w1zard1/ModSync). Its BSL 1.1 license and notices are retained in [LICENSE.txt](LICENSE.txt). Third-party Rust crates retain their respective licenses. `tests/fixtures/version.rar` comes from unrar 0.5.8; its MIT license is included beside the fixture.

## Structured guide conversion

```sh
modsync convert --input guide.toml --output guide.xml --format xml
modsync convert --stdin --format yaml < guide.json
```

TOML, JSON, YAML and XML conversion uses formats readable by the C# application. JSON writes a component array; YAML writes a metadata document followed by component documents. Keep build-level metadata in TOML or YAML. File extensions are case insensitive, and unknown extensions trigger content detection. Markdown can import default prose fields and explicit YAML/TOML instruction comments. Desktop/installer Markdown loading drafts prose into reviewable instructions. Conversion uses `--parse-directions` to opt in. Markdown export remains in progress.

The pinned C# fixtures in `tests/fixtures/csharp` run in normal Cargo tests. On Linux, `tools/check_formats.py --reference-dll <built-ModSync.Core.dll> --rust <modsync-binary> --output <scratch-folder>` reruns the bidirectional checks with bubblewrap isolating C# settings/cache and network access. The check compares install-relevant fields and records its scope in `receipt.json`.

To draft and inspect instructions without installing:

```sh
cargo run -- convert guide.md draft.json --parse-directions
cargo run -- convert draft.json reviewed.toml --review-draft <component-guid>
```

For instruction-free components whose archives are already downloaded:

```bash
cargo run -- convert guide.toml generated.toml --auto-generate-local --source-path /path/to/mods
```

This infers extraction, loose-file moves, folder choices and patcher steps, then writes a per-component JSON report to stderr. Inspect the generated guide and validate it before installation. Existing top-level instructions are preserved; matching authored options are reused, and overlapping options are consolidated while retaining the first option's identity and selection. Archive matching uses the first resource link in guide order, as C# does; it does not fall back to later links when the first has no matching archive. EXEs that cannot be read as archives produce Execute instructions; running those steps requires the existing external-program trust setting. Desktop controls for archive inference are still in progress. Input archives are never deleted by inference.

Inspect every generated action/path before accepting it. Components with unparsed gaps, conditional drafts or rejected paths remain blocked from acceptance. The desktop provides the same drafting and review operations. A generated draft is not proof that the prose was interpreted correctly; see the compatibility document for known reference-parser limitations.

To inspect which filenames a host response would select for a component:

```sh
modsync download-plan guide.toml --component <GUID> \
  --url https://example.com/mod --source-dir /path/to/mods \
  --filename first.zip --filename alternative.zip
```

The JSON result reports `UrlEnabled` and `Filenames`. The command applies resource file flags and the reference's unsatisfied-Extract-pattern filter without downloading or modifying files. It is a filename-selection plan, not an install-readiness result: the reference also counts known registry filenames as satisfying patterns. Direct `download` remains a separate explicit action; host scraping, download queues and desktop plan controls remain unfinished.

## Profiles

Profiles store directories, the guide path, and component/option selections in C#-compatible JSON under `<settings-dir>/profiles/`.

```sh
modsync profile --settings-dir /path/to/settings --action create --name "Full Build"
modsync profile --settings-dir /path/to/settings --action list --json
modsync profile --settings-dir /path/to/settings --action capture --name "Full Build" \
  --input guide.toml --game-dir /path/to/game --source-dir /path/to/mods
modsync profile --settings-dir /path/to/settings --action apply --name "Full Build" \
  --input guide.toml --output selected-guide.toml
modsync profile --settings-dir /path/to/settings --action activate --name "Full Build"
```

`show` and `delete` use `--name`; `clone` and `rename` use `--from` and `--to`. Commands output JSON. Capture updates an existing profile while retaining its creation time. Apply changes selections in the output guide; components absent from the profile retain their current selections. Activate saves `activeProfileName` in settings while preserving unrelated settings. Desktop activation and platform-default settings discovery remain unfinished; these commands require an explicit settings directory. Applying a profile does not itself install anything. The guide loader's existing all-unselected fallback still applies when an exported guide is subsequently reloaded.

Use a stored profile directly for validation or installation:

```sh
modsync validate --settings-dir /path/to/settings --profile "Full Build"
modsync install --settings-dir /path/to/settings --active-profile
```

The profile supplies the guide path, game directory, mod directory, and selection state. Explicit `--input`, `--game-dir`, `--source-dir`, `--select`, and `--option` flags override those defaults. Profile selections are applied after loading, so an all-unselected profile stops with “Select at least one component” instead of installing everything. Validation and installation read the profile and settings without changing their activation timestamps. `--active-profile` requires an existing `activeProfileName`; it never silently falls back to another profile.

In the desktop, expand **Install profiles**, choose a settings directory and enter a profile name. **Save current profile** captures the loaded guide's selections and current directories. **Activate profile** applies that profile to the currently loaded guide and restores its game/mod directories; run validation again before installing. Profile management commands also support listing, cloning, renaming and deleting profiles.
Use **Refresh profiles** to populate the stored-profile selector. Enter a new name to clone or rename the selected profile. **Delete profile…** asks for confirmation and clears the active setting when applicable; it does not uninstall files.

`--settings-dir` is optional for profile commands and profile-backed validation/install. It defaults to `ModSync` within the platform configuration directory (Linux XDG config, macOS Application Support, Windows roaming application data). The desktop prepopulates that directory. Supply an explicit directory to keep a separate set of profiles.
When the current settings file is absent, profile operations can read the active-profile and managed-deployment fields from the legacy `KOTORModSync/settings.json` in the platform configuration directory. Activation writes to the current store and leaves the legacy file unchanged; profile files themselves are not migrated.

The desktop restores saved game/mod directories when launched without those explicit path arguments. On **Load guide**, **Save directories for next launch** writes them to the selected settings store while preserving unrelated settings. Use `modsync gui --settings-dir /path/to/settings` to launch with a custom store.

The desktop **Theme** menu saves Light, KOTOR, or KOTOR 2 in the selected settings store and restores it at startup. The game themes currently provide dark egui visuals with their reference accent colors; the full Avalonia appearance and theme-driven target-game behavior are still being ported.

On **Choose mods**, filter by tier or category, then use **Select through tier** or **Select category** to add matching base-build components. Known tiers are cumulative (1–4, then Optional); custom tiers match exactly. Existing selections are retained, and these bulk actions skip widescreen-only components.

**Spoiler-free names and descriptions** uses authored alternatives or generic fallback text in the selection page; search follows that displayed text. It does not redact URLs, raw draft review, or activity logs. The toggle is saved in the selected settings store and restored at startup. Hover a normal-mode description to read its full text.
Use **Select multiple categories…** to check several categories, then apply them together. Cancel leaves component selections unchanged.

### File-conflict reports

Use the shared installation simulation to report game files written by multiple components:

```bash
modsync conflicts --input guide.toml --source-dir /path/to/mods --game-dir /path/to/game
```

The JSON report lists each destination, its writers in install order, the last writer, and per-component conflict counts. The command accepts the same profile, selection, archive-budget and FOMOD choices arguments as `validate`. Neither installation directory is changed. Mod-workspace staging and skipped overwrites are excluded. External program effects cannot be predicted and are identified as a limitation; simulation errors stop the command instead of producing a partial success report.
