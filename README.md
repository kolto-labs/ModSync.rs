# ModSync

A desktop app and command line tool that installs KOTOR and KOTOR 2 mod builds from ModSync instruction files: the mods you pick, in the order the guide sets, with a check before anything changes and a snapshot to restore from. The desktop app and the CLI share one engine for selection, validation, archives and installation.

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

Rust tests and custom Rust harnesses are frozen during the parity effort; do not build or run test targets or use `--all-targets`. CI runs production checks on Linux, including a Windows cross-check; native Windows and macOS builds require manual dispatch. These gates verify compilation and linting, not full parity. See [AGENTS.md](AGENTS.md) for contributor rules and [the cloud bootstrap](docs/parity/cloud/README.md) for isolated production CLI proofs and their prerequisites.

The pinned C# fixtures in `tests/fixtures/csharp` run in normal Cargo tests. On Linux, `tools/check_formats.py --reference-dll <built-ModSync.Core.dll> --rust <modsync-binary> --output <scratch-folder>` reruns the bidirectional format checks with bubblewrap isolating C# settings/cache and network access. The check compares install-relevant fields and records its scope in `receipt.json`.

The in-progress Qt desktop is described in [desktop/qt/README.md](desktop/qt/README.md), and packaging in [packaging/README.md](packaging/README.md).

## Attribution

Instruction behavior is based on [ModSync](https://github.com/th3w1zard1/ModSync). Its BSL 1.1 license and notices are retained in [LICENSE.txt](LICENSE.txt). Third-party Rust crates retain their respective licenses. `tests/fixtures/version.rar` comes from unrar 0.5.8; its MIT license is included beside the fixture.
