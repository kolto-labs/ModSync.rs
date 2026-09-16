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

CLI validation and installation select all top-level components by default, matching ModSync. Use `--use-file-selection` to honor `IsSelected`, or repeat `--select NAME_OR_GUID` to select specific components. Repeat `--option GUID` to enable options. Option defaults from the guide remain intact. Dependencies must be selected explicitly; missing dependencies and incompatible selections stop validation.

The desktop can save selections as a TOML file. The CLI reads the same file with `--use-file-selection`.

## Run patchers

Point to an installed OdyPatcher executable:

```sh
modsync install --input guide.toml --game-dir /path/to/game --source-dir /path/to/mods \
  --allow-processes --patcher /path/to/odypatcher
```

`Patcher` accepts a namespace index or an INI filename in `Arguments`. Named INI installs use a temporary copy of `tslpatchdata`, leaving the original patcher configuration intact. `Execute` and `Run` invoke the specified local program directly, without a shell. External programs have your user permissions and require the explicit trust switch. Windows executables require a Windows host; no implicit Wine execution is provided.

Validation checks process inputs but does not execute programs or predict their effects. It may reject a later step that requires a file an external program would create. A successful validation report is not proof that an external patcher will succeed.

## Download archives

```sh
modsync download https://example.org/mod.zip /path/to/mods/mod.zip --sha256 EXPECTED_HASH
```

Direct HTTPS ZIP, 7z, RAR, TAR, and GZip downloads are checked before being saved. HTML responses, archive corruption, unsafe names, and checksum mismatches fail. Existing downloads are never overwritten. Nexus/DeadlyStream landing pages and MEGA links remain browser workflows; the desktop exposes guide links for that purpose. Archive expansion and file copies stream through temporary storage. Use `--scratch-dir <existing-folder>` for validate/install to choose storage outside the game and mod trees, and `--max-expanded-bytes <bytes>` to set an optional per-archive expansion budget. Both controls are available under Archive storage in the desktop. Download integrity checks accept the same byte budget.

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
