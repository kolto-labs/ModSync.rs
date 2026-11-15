# ModSync

Your build. In order.

ModSync is a [Kolto Labs](https://koltolabs.bodecloud.workers.dev/) tool for installing Knights of the Old Republic I and II mod builds. Bring your own game, choose your mods, and follow the guide's install order, with a check before anything changes and a snapshot to restore from. The desktop app and command line share one engine for selection, validation, archives and installation. ModSync contains no mods or game files.

**Website and documentation: https://kolto-labs.github.io/ModSync.rs/**

## Documentation

| You want to | Read |
|---|---|
| Install a mod build | [Getting started](https://kolto-labs.github.io/ModSync.rs/getting-started.html), then [Walkthrough: your first build](https://kolto-labs.github.io/ModSync.rs/walkthrough-install.html) |
| Fix a problem | [Troubleshooting](https://kolto-labs.github.io/ModSync.rs/troubleshooting.html), [Safety and recovery](https://kolto-labs.github.io/ModSync.rs/safety.html), [FAQ](https://kolto-labs.github.io/ModSync.rs/faq.html) |
| Write or convert a guide | [Instruction files](https://kolto-labs.github.io/ModSync.rs/instruction-files.html), [Walkthrough: writing a guide](https://kolto-labs.github.io/ModSync.rs/walkthrough-authoring.html), [Format reference](https://kolto-labs.github.io/ModSync.rs/format-reference.html) |
| Use the command line | [Command line](https://kolto-labs.github.io/ModSync.rs/cli.html) overview and the full [CLI reference](https://kolto-labs.github.io/ModSync.rs/cli-reference.html) |
| Use the `modsync` crate | [Library API](https://kolto-labs.github.io/ModSync.rs/api.html) |
| See what is supported | [Features and roadmap](https://kolto-labs.github.io/ModSync.rs/status.html) and [COMPATIBILITY.md](COMPATIBILITY.md), which records every supported behavior, proof boundary and known difference |

The site's source is in [`site/`](site/) and deploys from `main` through `.github/workflows/pages.yml`.

A Cloudflare Workers deployment is also available at [modsync-site.bodecloud.workers.dev](https://modsync-site.bodecloud.workers.dev/). The site is static HTML, CSS and JavaScript and needs no build step. To deploy the checked-out site's contents to Workers, run these commands from the repository root with Node.js and npm installed.

```sh
npx wrangler login
npx wrangler deploy
```

The root `wrangler.jsonc` publishes only `site/`. Local preview is available with `python3 -m http.server 8000 --directory site` at `http://localhost:8000/`.

## Download

Packaged builds are published on the [Releases page](https://github.com/kolto-labs/ModSync.rs/releases).

## Build from source

Install Rust stable and a C/C++ build toolchain, then:

```sh
git clone https://github.com/kolto-labs/ModSync.rs.git
cd ModSync.rs
cargo run --release                         # desktop app
cargo build --release --no-default-features # CLI only: ./target/release/modsync
```

Linux needs an X11 or Wayland session with OpenGL support, and file dialogs use the desktop portal. Windows builds need the MSVC C++ tools; macOS needs the Xcode command-line tools. The RAR reader compiles the bundled UnRAR C++ library.

## Try it on sample folders

These files are demonstration data, not game assets.

```sh
mkdir -p demo-work/game/Override demo-work/mods
printf 'sample texture\n' > demo-work/mods/sample.tga
printf 'old texture\n' > demo-work/game/Override/sample.tpc
cargo run -- gui --input examples/demo.toml --game-dir demo-work/game --source-dir demo-work/mods
```

Or from the CLI:

```sh
cargo run -- validate --input examples/demo.toml --game-dir demo-work/game --source-dir demo-work/mods --json
cargo run -- install  --input examples/demo.toml --game-dir demo-work/game --source-dir demo-work/mods
```

The result is `game/Override/sample.tga`; the conflicting `sample.tpc` is removed.

## Common commands

```sh
modsync list KOTOR1_Full.toml
modsync validate --input KOTOR1_Full.toml --game-dir /path/to/game --source-dir /path/to/mods
modsync install  --input KOTOR1_Full.toml --game-dir /path/to/game --source-dir /path/to/mods
modsync restore  --game-dir /path/to/game --source-dir /path/to/mods
modsync convert  guide.toml guide.json
```

Every command and option is in the [CLI reference](https://kolto-labs.github.io/ModSync.rs/cli-reference.html).

## Safety in brief

- Validation never changes the game or mod folders and never runs external programs.
- Installation validates first, snapshots both folders, follows guide order and stops at the first failure. After a failure, run `restore`, fix the cause and rerun the whole guide from the start. Never append a failed mod at the end.
- Patchers and other external programs run only when you allow them (`--allow-processes`).

Details: [Safety and recovery](https://kolto-labs.github.io/ModSync.rs/safety.html).

## Development

Production checks:

```sh
cargo fmt --all -- --check
cargo build --locked --lib --bin modsync -j2
cargo clippy --locked --lib --bin modsync -j2 -- -D warnings
cargo build --locked --no-default-features --lib --bin modsync -j2
cargo clippy --locked --no-default-features --lib --bin modsync -j2 -- -D warnings
```

Rust tests and custom Rust harnesses are frozen during the parity effort; do not build or run test targets or use `--all-targets`. CI runs production checks on Linux, including a Windows cross-check; native Windows and macOS builds require manual dispatch. These gates verify compilation and linting, not full parity.

The pinned C# fixtures in `tests/fixtures/csharp` run in normal Cargo tests. On Linux, `tools/check_formats.py --reference-dll <built-ModSync.Core.dll> --rust <modsync-binary> --output <scratch-folder>` reruns the bidirectional format checks with bubblewrap isolating C# settings/cache and network access. The check compares install-relevant fields and records its scope in `receipt.json`.

The in-progress Qt desktop is described in [desktop/qt/README.md](desktop/qt/README.md), and packaging in [packaging/README.md](packaging/README.md).

The pinned C# fixtures in `tests/fixtures/csharp` run in normal Cargo tests. On Linux, `tools/check_formats.py --reference-dll <built-ModSync.Core.dll> --rust <modsync-binary> --output <scratch-folder>` reruns the bidirectional checks with bubblewrap isolating C# settings/cache and network access. The check compares install-relevant fields and records its scope in `receipt.json`.

To draft and inspect instructions without installing:

```sh
cargo run -- convert guide.md draft.json --parse-directions
cargo run -- convert draft.json reviewed.toml --review-draft <component-guid>
```

Drafts inferred from exactly one local archive record that archive's SHA-256. `convert draft.json reviewed.toml --source-path <archive-folder> --review-verified-drafts` clears review only for drafts whose stored instructions a fresh replay of that archive reproduces exactly (no gaps, no authored edits, no prose drafts; a method-bound draft qualifies only when it is a method-only `Loose-File` or single-run `TSLPatcher`/`HoloPatcher` binding that `--resolved-archives` produced and a fresh binding replay reproduces), records a `Verification` receipt, and prints `{cleared, skipped}` on stderr. Everything else stays fenced.

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

`show` and `delete` use `--name`; `clone` and `rename` use `--from` and `--to`. Commands print a readable summary; add `--json` for JSON output. Capture updates an existing profile while retaining its creation time. Apply changes selections in the output guide; components absent from the profile retain their current selections. Activate saves `activeProfileName` in settings while preserving unrelated settings. Desktop activation and platform-default settings discovery remain unfinished; these commands require an explicit settings directory. Applying a profile does not itself install anything. The guide loader's existing all-unselected fallback still applies when an exported guide is subsequently reloaded.

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
## Attribution

Third-party Rust crates retain their respective licenses. `tests/fixtures/version.rar` comes from unrar 0.5.8; its MIT license is included beside the fixture.
