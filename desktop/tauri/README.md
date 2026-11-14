# KOTOR Mod Installer desktop interface

The kotor-mod-manager desktop interface (Tauri 2 + React) running on the
ModSync engine. The interface under `src/` is the original one, unchanged
apart from `src/lib/api.ts`: instead of HTTP calls to a Python backend, its
requests go to the `api_request` Tauri command and its live events arrive as
the `ws-event` window event. `src-tauri/src/api.rs` serves the same routes
(paths, request bodies and replies) on top of the `modsync` library, so there
is no separate backend process.

This crate is independent of the root workspace (its own `Cargo.toml` and
`Cargo.lock`), like `desktop/qt`, so the root crate's checks are unchanged.

## Where each part runs

- Builds (`builds.rs`): the build list is `modsync::builds::Catalog`; loading
  a build fetches its instruction file and reads it with
  `Guide::load_for_install_preserving_selection`.
- Install (`pipeline.rs`): the selected mods are set with
  `modsync::selection` (adding what each mod needs), their files are fetched
  with `download_orchestrator` (plan, discover, run, record), the install is
  checked with `Engine::validate_report_with_options_observed` (nothing on disk
  changes), then installed with `Engine::install_observed_report` into the game
  install's library (`with_component_checkpoints` plus
  `with_managed_deployment`). Mod states are sent as the original `status`,
  `progress`, `install_progress`, `manual` and `pipeline` events. Stop sets
  the engine's cancel flag.
- Game installs: ModSync profiles (`profiles::Store`); "KOTOR 1" and "KOTOR 2"
  are the default installs. Found installs come from
  `game_locator::find_installs`.
- Installed mods and Conflicts (`library.rs`): `deployment::Service`
  (`management_status`, `deploy_report`, `manage` with `Uninstall` or `Purge`,
  `remove_from_library`, `duplicates`, `installed_conflicts`). Restore game
  backup puts back the restore point of an install that stopped
  (`recovery::restore`) and removes every library mod.
- Account: DeadlyStream sign-in with `deadlystream_account::sign_in`; the
  Nexus Mods key is saved in the shared ModSync settings and checked with
  `updates::NexusClient::validate_with_cancel`.
- Mod details and screenshots: `mod_info::fetch` and `mod_info::fetch_image`
  (served to the window through the `kmm-image` protocol).
- Updates and What's New: `app_update::check`, `app_update::download` and
  `app_update::changelog_section` with `CHANGELOG.md`; the program is swapped
  and restarted by `apply_update`.
- Settings: the mod (download) folder, patcher path and Nexus key are shared
  ModSync settings; the rest is kept in `kotor-mod-manager.json` next to them.

Set `MODSYNC_SETTINGS_DIR` to keep all settings in a separate folder, for
example while trying the app.

## Differences from the Python version

The screens are the same. Some actions behave differently because the engine
works differently:

- Downloads from DeadlyStream are made without the signed-in session (the
  engine's DeadlyStream downloader is anonymous); sign-in is checked and
  remembered, the password is not stored.
- Pause and resume are not supported during an install; Stop is.
- Importing a folder or archive into the library is not supported.
- GameFront mods are listed as manual downloads; the GameFront session opens
  the site in the browser.
- `nxm://` links are not claimed.
- The 7-Zip setting is kept but the engine extracts archives itself.
- The mod build source site setting is kept but builds come from the build
  catalog.

Known engine limit seen on Windows: the check before installing stops with
"Process-fence namespace has an untrusted owner" (or "Recovery directory grants
mutation to another principal") when a folder above the game folder is owned by
TrustedInstaller (such as `C:\`) or lets other users delete files (the default
on extra drives). This comes from the engine's restore-point safety checks.

## Build and run

Requirements: Rust (MSVC on Windows), Node 20+, and the WebView2 runtime on
Windows. The first build compiles the engine, which takes a few minutes.

```sh
cd desktop/tauri
npm install
npm run build                 # type-check and build the interface into dist/
cargo build --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npx tauri dev                 # run with live reload
npx tauri build --no-bundle   # release binary in src-tauri/target/release
```

`npm run build` must run before `cargo build`, which embeds `dist/`. A debug
binary started directly loads the interface from `http://localhost:5173`
(`npm run dev`).

Try it only with isolated folders, never a real game install.

## Licensing note

The interface files under `src/` (and the icons) come from kotor-mod-manager,
which is GPL-3.0-only. ModSync.rs is under BSL 1.1. This has not been resolved
here.
