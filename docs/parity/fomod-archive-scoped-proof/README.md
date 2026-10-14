# FOMOD "configured without archive-scoped instructions" proof

The contract and triage are in [fomod-archive-scoped.md](../fomod-archive-scoped.md).
The base is `223ed92`.

`probe.py` follows the [gate-scope probe](../fomod-gate-scope-proof/README.md)
conventions:

- a `FOMOD_PARENT_NETNS` guard and a network namespace with no routes;
- private HOME, XDG, mods, game and scratch directories, and generic zip fixtures;
- production verbs only: `validate`, both plain and with `--validation-report`, and `install`;
- every validate leaves the guide, mods and game byte-identical, and every install
  leaves the guide byte-identical;
- output goes to `target/cloud-proof/fomod-archive-scoped`, one JSON per run plus a
  receipt that binds the binary SHA-256, the git HEAD and porcelain, and the source hashes.

Every fixture registers `empty.zip` and has valid saved `FomodChoices`. Unless a case
says otherwise, it also has the prompt status `configured` and an Extract route.

| Case | Checks |
| --- | --- |
| Installer that maps no files (one group, two plugins without files) | Report exits 0. The FOMOD stage passes with `hasWarnings`, the summary `All detected FOMOD archives are configured (1 warning(s)).` and exactly `WARNING: Example Consumer: FOMOD archive 'empty.zip' is marked configured but has no matching archive-scoped instructions. Its installer maps no files for any choice, so this configuration installs nothing from the archive.` Plain validate prints the event once. Install exits 0, writes nothing to the game and prints the warning. |
| Installer with no steps | The same warning. |
| Only file in a conditional pattern no single plugin can satisfy (C# skips it) | The same warning. |
| Two components register and configure the archive | One warning each, in order: `(2 warning(s))`. |
| No Extract route as well | This warning and the Rust-only route warning, once each. |
| Controls | No warning when files are mapped (install copies `b`). No warning when only the unchosen plugin has a file, or when the conditional pattern is attached to a plugin. No warning for status absent, `warned` or `Configured`, for a non-FOMOD archive, or (as a hard error) for `configured` status without saved choices. |

## C# oracle

[`csharp-oracle/`](csharp-oracle) runs the pinned reference `ModSync.Core.dll` (`d31ccbd3`,
SHA-256 `6e7de00d37bf9e0dabc63a303f31ec024bd1af36e50a3865988fe3d48f136824`) on the
probe's own fixture archives, using the .NET 10.0.12 runtime with `DOTNET_ROLL_FORWARD=Major`
under `unshare -Urn`. For each archive it follows C#'s configure flow: `FomodToComponentMapper.Map`, then
`FomodConfiguredComponentMerger.MergeInto`, then `MarkConfigured`, then
`FomodConfigurationGate.Validate`.
[`result.json`](csharp-oracle/result.json) shows that C# warns for the empty, no-steps and
unattached-pattern installers, and does not warn for the mapped, unchosen-plugin-only and
attached-pattern ones. That is the same partition Rust now produces.

```bash
cd docs/parity/fomod-archive-scoped-proof/csharp-oracle
dotnet build -c Release -o out -p:ModSyncCoreDll=<reference>/ModSync.Core.dll
DOTNET_ROLL_FORWARD=Major unshare -Urn dotnet out/probe.dll <reference-dir> <fixture-root>/{empty,nosteps,unattached,mapped,partial,attached}/mods
```

## Results (cloud Linux, default-feature debug build)

- **This change:** 37/37 checks and 16 recorded production invocations.
- **Pre-fix:** the same probe against the `223ed92` binary
  (`FOMOD_ARCHIVE_SCOPED_ALLOW_UNBOUND=1 MODSYNC_BINARY=<copy>`, output
  `target/cloud-proof/fomod-archive-scoped-pre223ed92`) gives 30/37, with 7 failures.
  These are `empty_report_stage`, `empty_plain_event` and `empty_install_event`
  (no warning), `nosteps_report`, `unattached_report`, `two_messages` and `no_route_both`
  (only the route warning). All controls pass on both builds.
- **Regressions,** each in its own namespace: S1 339/339, inherited 41/41,
  S2a 224/224, S2b 254/254, fomod-gate-scope 74/74, download-residuals 183/183,
  fetch-shared-url 382/382 and H1a 138/138.
- **Gates,** all exit 0:
  - `cargo fmt --all -- --check`;
  - `cargo clippy --locked --lib --bin modsync -j4 -- -D warnings`, with and without
    `--no-default-features`;
  - the same clippy for `--target x86_64-pc-windows-gnu`.

  No Rust test or harness was created or run.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j2
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-archive-scoped-proof/probe.py
```

Set `MODSYNC_PROOF_KEEP=1` to keep the temporary fixture root.
