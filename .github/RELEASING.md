# Checks and releases

## Checks on every commit

`.github/workflows/ci.yml` runs the production gates (format, build and strict
Clippy, with and without the desktop app, plus a Windows cross-check) on every
push to `main` and to any other branch. Pull requests from a branch in this
repository reuse that push run instead of starting a second one. Pull requests
from forks run on their own.

The Rust test suite has its own job, `tests`. It stays skipped while the parity
freeze in `AGENTS.md` is in effect. When the freeze ends, set the repository
variable `RUN_RUST_TESTS` to `true` (Settings > Secrets and variables > Actions >
Variables). Every commit will then run `cargo test` too, and no workflow edit is
needed.

## Making a release

Either:

- open Actions > Release > Run workflow and enter a version such as `0.2.0`, or
- push a tag such as `v0.2.0`.

The workflow then:

1. checks that the version is three numbers and that no release with that name
   exists yet,
2. builds every target in `packaging/targets.json` through
   `package-manual.yml`,
3. creates the tag and a GitHub release with the packages, their receipts and a
   `SHA256SUMS.txt`. Releases are marked as pre-releases by default, since the
   packages are still unsigned and their `nativeAcceptance` is unverified.
4. uploads to Nexus Mods if that is set up (see below).

## Nexus Mods (optional)

Publishing to Nexus Mods only happens when the repository has:

| Kind     | Name                     | Value                                            |
|----------|--------------------------|--------------------------------------------------|
| Secret   | `NEXUSMODS_API_KEY`      | A Nexus Mods API key that can upload to the page |
| Variable | `NEXUS_WINDOWS_FILE_ID`  | File group ID for the Windows x86_64 download    |
| Variable | `NEXUS_LINUX_FILE_ID`    | File group ID for the Linux x86_64 download      |
| Variable | `NEXUS_MACOS_FILE_ID`    | File group ID for the macOS Apple Silicon download |

When the secret is missing, every platform is skipped with a notice and the
release still succeeds. If one platform's variable is missing, only that
platform is skipped. This keeps forks and repositories without a Nexus page
releasing cleanly. Each platform needs its own file group, because uploading
archives the previous file in the same group.

A successful upload attaches `nexus-published-<platform>.json` to the GitHub
release. Re-running the job skips platforms that already have that marker, so a
retry never uploads twice. To leave Nexus Mods out of a single manual run,
untick "Also publish to Nexus Mods".
