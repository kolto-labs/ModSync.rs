# FOMOD S2a: reconfiguration, stale choices, C#-configured guides, install route

Scope is the bounded S2a slice of [the remaining plan](../fomod-remaining-plan.md)
(S2 paragraph two plus the registration-without-Extract sentence). The base is
`5f81e82`. The changes are uncommitted working-tree source, bound by hash in the
receipt. Priority sorting, recursive folder copy, authored renaming, Unicode matching,
diagnostic/ownership corrections and desktop work are out of scope. This slice does
not change F15 or F29 status.

## (a) Audit: what S1 already delivered

| S2 obligation | State at `5f81e82` | Evidence |
| --- | --- | --- |
| Cloned bound guide per session, all-or-nothing multi-consumer edits | **Delivered by S1** (`fomod::apply_bound_archive` clones and commits once) | S1 `atomic_consumers` |
| `--record` saves accepted configuration and status after dispatch, even when only FOMOD changed; without record the guide is byte-identical | **Delivered by S1** (record-timing dependency) | S1 `restart_*`, `nofiles_*`, `*_guide_unchanged` |
| Mixed per-archive success, failed session keeps prior state, cancellation keeps only committed sessions | **Delivered by S1** | S1 `mixed_choices`, `mixed_signal` |
| Other archives' choices survive configuring one archive | **Delivered by S1** for first configuration | S1 `other_archive` |
| Same-archive **re**configuration replaces prior choices | **Missing.** `apply_bound_archive` would replace the entry, but Fetch never reached it: a consumer with valid saved choices was `resolved`, and an explicit `--fomod-choices` entry naming that archive was silently ignored. No other persistent CLI route existed. The pre-fix run below shows this. | pre-fix `reconfigure_*` |
| Fresh engine mapping uses the current archive | **Delivered before S1**: the engine `Extract` branch re-inspects the archive on disk and maps saved names against it; stored copies are never executed. It had no proof with a replaced archive. | `compatible_*` (new proof, no source change) |
| Refuse stale/invalid saved selections after the archive changes | **Partial.** Fetch (S1) refused them, but its error gave no recovery route, and even an explicit replacement could not recover the consumer. The validation gate (`validate --validation-report`) checked only that an entry **existed**: stale or invalid choices passed validation, and registered-only archives passed with no install-time check at all. Install preflight refused them only through the raw `apply_to` error. | pre-fix `stale_validate_*`, `registered_invalid_*`, `stale_recover_*` |
| C#-configured guide (status `configured`, generated instructions, no `FomodChoices`) | **Partial.** It was refused, but with "has no explicit FomodChoices configuration" and no mention of the status or a route. Fetch `--interactive` suppressed it silently. | pre-fix `csharp_*_actionable` |
| Registration without Extract: gate validity vs install route | **Missing.** Valid choices on a registered-only archive passed the gate silently, which implied an install that never happens. | pre-fix `registered_validate_warning` |

## (b) Changes

- `src/fomod.rs` adds `CHOICES_RECOVERY_HINT`, `required_saved_choices`,
  `stale_choices_context` and `current_archive_choices` (near `saved_archive_choices`),
  shared by Fetch, the validation gate and install:
  - `required_saved_choices` returns the saved entry or an actionable error. When
    `fomodPromptStatus` is `configured`, the error says that status is not
    executable choices and that C# configuration is not imported.
  - `current_archive_choices` applies choices to a copy of the current archive's
    installer, maps them without writing, and returns the normalized capture.
- `src/engine.rs` `validate_fomod_configuration` now validates saved choices against
  the current archive for both registered and `Extract` archives. It returns
  warnings for registered archives with valid choices but no applying `Extract`
  (keyed by resolved path). The install `Extract` branch uses the same helpers,
  with stale context.
- `src/validation.rs` `fomod_configuration` takes `Result<Vec<String>, String>`.
  Warnings keep the stage passing, set `hasWarnings`, add to `warningCount`, and
  are listed as `WARNING:` messages, following the dry-run warning convention.
- `src/fomod_cli.rs`:
  - The explicit choices source (flag, then `MODSYNC_FOMOD_CHOICES`, unless
    `--fomod-skip`) loads once through a cache.
  - For each archive with saved choices (valid or stale), an explicit entry for that
    archive is validated against the current archive. It then replaces every
    consumer whose saved choices differ or are stale, and configures pending ones,
    in one atomic bound session. The new outcome is `reconfigured`.
  - Identical choices are a no-op with no edit. Consumers without saved choices
    keep S1's lazy mode path unchanged.
  - Stale saved choices report the recovery route.
  - A pending consumer marked `configured` without choices prints the C#-import
    requirement on stderr. This covers interactive suppression and warn/skip.

### Design decisions

- **Archive identity for staleness.** C# `FomodArchiveChoices` carries only
  `archiveFileName`, and so does the version-1 Rust file. C# `FomodChoicesApplier`
  re-applies choices to the freshly extracted current archive every time and throws
  for missing steps or groups. The minimal check justified by that behavior is: the
  current archive's installer must accept every saved step, group and plugin name,
  validate, and map to safe paths. Rust stays strict on unknown plugin names (a
  documented divergence). No hash field was added: a byte identity would change the
  interchange format and reject compatible re-downloads. Authored
  `ContentHashSHA256` already guards acquisition bytes. **Residual:** a replacement
  archive that still accepts the saved names is accepted. Its files are mapped
  fresh from the current bytes (proved by `compatible_install_bytes`).
- **Reconfiguration route.** C# post-download never reconfigures a `configured`
  archive (`ShouldPrompt` gates choices mode too). Its reconfiguration is the GUI
  merger, which removes prior archive-scoped instructions. Rust stores choices, not
  instructions, so replacing the single archive entry is the equivalent. The
  additive CLI route is limited to an explicit choices entry for that archive.
  Interactive, warn and skip never touch valid or configured consumers. The S1 lazy
  contract holds: when every consumer of an archive has valid choices, a choices
  file that cannot be loaded is ignored and the run does not fail. A needed load
  error still fails as before. An explicit entry now also recovers stale saved
  choices. S1 reported those as errors, and no S1 case covered this, so S1
  expectations are unchanged.
- **C# configured guides** are not imported. Their generated instructions stay
  untouched, but status alone never authorizes the `Extract` of a FOMOD archive.
  The supported import route is `fomod-inspect --save-choices` followed by
  `fetch-downloads --record --fomod-choices`. That route is proved end to end
  through validate and install.

## (c) Controlled production proof

`probe.py` follows the S1 conventions: `FOMOD_PARENT_NETNS` guard, a namespace with
no routes, a loopback HTTPS origin whose leaf is signed by a private CA (added with
`--extra-ca-file`), and private HOME/XDG/settings/mods/game/scratch directories.
It has generic archives and guides and uses only real production verbs:
`fetch-downloads`, `validate --validation-report` and `install`. Output goes to
`target/cloud-proof/fomod-s2a`. Each run is retained as JSON (argv, exit, stdout,
stderr, before/after hashes) with before/after fixture trees, plus
`reconfigure-guide-diff.json`. The receipt binds git HEAD and porcelain, source
hashes, and the binary SHA-256. It asserts that the binary mtime is not older than
the sources.

| Case | Negative | Positive control |
| --- | --- | --- |
| Reconfigure one archive twice (Beta → Alpha → Beta) | Before the fix, explicit choices were ignored (`resolved`, guide unchanged). | One `example.zip` entry each time. `second.zip` stays byte-exact, and nothing outside `FomodChoices` changes. The Beta guide bytes after the round trip equal the first configuration. A repeat run is a no-op, and without `--record` the guide is unchanged. Install yields only the current selection's file. |
| Shared archive, differing saved choices | Invalid explicit entry: exit 1 and guide unchanged. | Only the differing consumer is reconfigured. |
| Archive replaced with renamed installer (fetched over HTTPS) | Fetch (`--non-interactive` and `--interactive`), validate and install all refuse with the recovery route. Install leaves the game untouched. | An explicit entry gives `reconfigured`, then validate passes and install writes the v2 bytes. |
| Compatible replacement | — | `resolved`. Install writes the current (v2) bytes. |
| C#-style configured-only guide | Validate, install, and Fetch interactive/warn give actionable requirements. There is no prompt and no guide change. | `--record --fomod-choices` imports. Validate passes, install writes bytes, and the authored instructions are preserved. |
| Registration without Extract | Missing or invalid choices fail the gate with the route. | Valid choices pass with a `WARNING` and `warningCount` 1. Install copies only the unrelated file. The same guide with `Extract` has no warning and installs. |

### Results (this working tree, cloud Linux)

- `probe.py`: **117/117 checks, 34 production invocations**. Default-feature debug
  binary SHA-256 `5d7690af5463a72dac08abf3b336385dcedd730c86b5ff67d083addaee523cc3`
  (host-specific). Git HEAD `5f81e82` plus uncommitted `src/{engine,fomod,fomod_cli,validation}.rs`.
- Pre-fix demonstration: the same probe against a `--no-default-features` binary
  built from a `git archive` of `5f81e82` (`FOMOD_S2A_ALLOW_UNBOUND=1`, output
  `target/cloud-proof/fomod-s2a-prefix`) gives **84/117; 33 checks fail**. The
  failures are:
  - every reconfiguration and shared-session check;
  - stale validate exit 0 and recovery exit 1;
  - all actionable-text checks;
  - the registered-only route warning;
  - registered invalid choices passing validation (exit 0).
- Regressions in isolated namespaces: S1 `probe.py` **339/339 (61 cases)**,
  `inherited_controller.py` **41/41**, Nexus H1a `probe.py` **138/138**. No S1
  expectation changed.
- Gates, all exit 0:
  - `cargo fmt --all -- --check`;
  - `cargo build`/`clippy -D warnings --locked --lib --bin modsync -j3`, with and
    without `--no-default-features`;
  - `x86_64-pc-windows-gnu` Clippy `-D warnings` in both feature modes.

  No Rust test or harness was created, modified, compiled or run, and
  `--all-targets` was not used.

## Observations reported, not fixed (out of scope)

- Plain `validate` (without `--validation-report`) and `install` do not run the
  registered-archive gate. They catch stale choices only on the `Extract` path.
- `validate`/`install --fomod-choices` (`ChoicesFile::apply_to_remaining`,
  `merge=false`) replaces **all** embedded archive choices for that run. A file
  naming one archive therefore drops the others for the run. This is a source
  reading only; it is not probed and the guide is not persisted.
- S1 warn/skip on a C# `configured` consumer without choices rewrites its status to
  `warned`/`dismissed` under `--record`. This is honest but loses the C# marker.
  The new stderr message names the import route.
- Gate dependency-scope expansion (C# `ExpandWithHardDependencies`) and missing
  registered archives without prompt state (a C# soft warning) remain unimplemented.

## What remains of S2

- Unicode (OrdinalIgnoreCase) name matching.
- Visible diagnostics for fallback semantics.
- Ownership propagation.
- The items above.
- Desktop reconfiguration and native acceptance.
- The S3 restart/recovery and collision corpora.
- Serialized C# interop beyond explicit refusal plus the choices import route.

F15 remains partial.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j3
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-s2a-proof/probe.py
```
