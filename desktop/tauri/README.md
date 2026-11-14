# KOTOR Mod Installer desktop app

The kotor-mod-manager desktop interface (Tauri 2 + React) running on the
ModSync engine. It is a separate crate with its own `Cargo.toml` and
`Cargo.lock`, like `desktop/qt`, and is not part of the root build.

## Layout

- `src/` the React interface. `src/lib/api.ts` sends each request to the
  `api_request` Tauri command and receives live events as the `ws-event`
  window event.
- `src-tauri/src/api.rs` serves the requests on top of the `modsync` library;
  `builds.rs`, `pipeline.rs`, `library.rs` and `extras.rs` hold the larger
  routes, `state.rs` and `prefs.rs` the app state and saved settings.
- `CHANGELOG.md` holds the notes shown in What's New.

## What each view uses

- Builds: `builds::Catalog` for the build list (plus instruction files opened
  from the computer), `Guide::load_for_install_preserving_selection` to read a
  build, `selection` for the chosen mods and their options. Check runs the
  download (`download_orchestrator`) and
  `Engine::validate_report_with_options_observed`; Install runs them again and
  then `Engine::install_observed_report` into the game install's library
  (`with_component_checkpoints`, `with_managed_deployment`). Restore uses
  `recovery::restore`.
- Mod details: `mod_info::fetch` and `mod_info::fetch_image` (served through
  the `kmm-image` protocol).
- Installed mods: `deployment::Service` (`management_status`, `deploy_report`,
  `manage`, `remove_from_library`, `duplicates`).
- Conflicts: the selected mods' `Restrictions` (mods the build says not to
  combine) and `conflicts::Presentation` of the engine's conflict analysis, as
  kept by `library::set_analysis`.
- Settings: game installs are ModSync profiles (`profiles::Store`, found ones
  from `game_locator::find_installs`); DeadlyStream sign-in uses
  `deadlystream_account::sign_in`; the Nexus key is checked with
  `updates::NexusClient`; the patcher choice, spoiler-free mode, mod folder,
  Nexus key and widescreen filtering (`filterDownloadsByResolution`, with the
  size of the monitor the app is on, as in the ModSync desktop app) are shared
  ModSync settings; updates use `app_update`.

`MODSYNC_SETTINGS_DIR` keeps all settings in another folder, for example while
trying the app with test folders.

## Build and run

Requirements: Rust (MSVC on Windows), Node 20 or newer, and the WebView2
runtime on Windows.

```sh
cd desktop/tauri
npm install
npm run build
cargo build --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npx tauri dev
npx tauri build --no-bundle
```

`npm run build` must run before `cargo build`, which embeds `dist/`. A debug
binary started on its own loads the interface from `http://localhost:5173`
(`npm run dev`).

## Licensing

The files under `src/` and the icons come from kotor-mod-manager
(GPL-3.0-only); ModSync.rs is under BSL 1.1.
