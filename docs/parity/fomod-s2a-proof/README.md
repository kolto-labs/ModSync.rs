# FOMOD S2a: reconfiguration, stale choices, C#-configured guides, install route

Scope is the bounded S2a slice of [the remaining plan](../fomod-remaining-plan.md)
(S2 paragraph two plus the registration-without-Extract sentence). The base is
`5f81e82`. Source is commit `0d6aad1`, its review follow-up commit `93837e5`
([section (d)](#d-review-follow-up)), the round-2 follow-up commit `a436c05`
([section (e)](#e-round-2-follow-up)) and the round-3 follow-up
([section (f)](#f-round-3-follow-up)), bound by hash in the receipt. Priority sorting, recursive folder copy, authored renaming, Unicode matching,
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
  (keyed by resolved path, one per archive). The install `Extract` branch uses the
  same helpers, with stale context.
- `src/validation.rs` `fomod_configuration` takes `Result<Vec<String>, String>`.
  Warnings keep the stage passing, set `hasWarnings`, add to `warningCount`, and
  are listed as `WARNING:` messages, following the dry-run warning convention.
- `src/fomod_cli.rs`:
  - The explicit choices source (flag, then `MODSYNC_FOMOD_CHOICES`, unless
    `--fomod-skip`) loads once through a cache.
  - For each archive with saved choices (valid or stale), a `--fomod-choices` entry
    for that archive (never the environment variable) is validated against the current archive. It then replaces every
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
  additive CLI route is limited to a `--fomod-choices` entry for that archive.
  `MODSYNC_FOMOD_CHOICES` keeps its S1 role: it configures pending consumers only,
  and neither reconfigures valid saved choices nor recovers stale ones (stale
  consumers are S1 errors, not pending). `--fomod-skip` wins over the flag, as in S1.
  Interactive, warn and skip never touch valid or configured consumers. The S1 lazy
  contract holds: when every consumer of an archive has valid choices, a choices
  file that cannot be loaded is ignored and the run does not fail. When a consumer
  has stale saved choices, a flag file that cannot be read or parsed is reported
  once in `errors`. A needed load error for pending consumers still fails as before.
  The flag now also recovers stale saved choices. S1 reported those as errors, and
  no S1 case covered this, so S1 expectations are unchanged.
- **Invalid entry.** An entry that does not apply to the current archive commits
  nothing. Consumers with valid saved choices stay `resolved` and usable, stale
  consumers keep their own error, pending consumers fail as in S1 Choices mode, and
  the entry error is a run error (exit 1). `unresolved` and follow-ups therefore
  agree with the consumer state.
- **C# configured guides** are not imported. Their generated instructions stay
  untouched, but status alone never authorizes the `Extract` of a FOMOD archive.
  The supported import route is `fomod-inspect --save-choices` followed by
  `fetch-downloads --record --fomod-choices`; it is the only route the recovery
  hint names. That route is proved end to end
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
| Shared archive, differing saved choices | Invalid explicit entry: exit 1, guide unchanged, both consumers stay `resolved`/usable, one run error, `unresolved` 0, no follow-ups. | Only the differing consumer is reconfigured. |
| Choices source (valid saved `Alpha`) | `MODSYNC_FOMOD_CHOICES` with `Beta`: `resolved`, exit 0, no session, guide unchanged. `--fomod-skip --fomod-choices`: the same. | The same file through `--fomod-choices` gives `reconfigured`. The environment still configures a pending consumer of the same archive while the valid one stays `Alpha`. |
| Lazy choices file | — | Valid saved choices with a missing `--fomod-choices` file: `resolved`, exit 0, no errors. |
| Archive replaced with renamed installer (fetched over HTTPS) | Fetch (`--non-interactive` and `--interactive`), validate and install all refuse with the recovery route. Install leaves the game untouched. A missing or malformed `--fomod-choices` file: exit 1, `error`, and the file error (path included) in `errors`. The environment variable does not recover. | An explicit entry gives `reconfigured`, then validate passes and install writes the v2 bytes. |
| Compatible replacement | — | `resolved`. Install writes the current (v2) bytes. |
| C#-style configured-only guide | Validate, install, and Fetch interactive/warn give actionable requirements. There is no prompt and no guide change. | `--record --fomod-choices` imports. Validate passes, install writes bytes, and the authored instructions are preserved. |
| Registration without Extract | Missing or invalid choices fail the gate with the route. | Valid choices pass with a `WARNING` and `warningCount` 1. Two registrants of one archive give one warning naming both. Install copies only the unrelated file. The same guide with `Extract` has no warning and installs. |
| Saved names apply, mapping is unsafe | Validate and install refuse with the recovery route (install's mapping step now carries it). The game stays untouched. | — |
| Unparseable saved `FomodChoices` (version 2; duplicate archive) | Fetch with a valid `--fomod-choices` entry: exit 1, `error`, no session, saved choices and registry unchanged; the consumer error names the repair hint and no output names the Fetch recovery route. Validate (both shapes) and install (version 2) show only the repair hint; the game stays untouched. A flag file without this archive's entry reports no missing-entry error (nothing to recover). | After the component's `FomodChoices` is removed from the guide, `--record --fomod-choices` gives `configured` and saves the entry. |
| Unparseable (version 2) and stale parseable consumers of one archive | The unparseable consumer: `error` with the repair hint, metadata value-equal. Exit 1. | The same `--record --fomod-choices` run reconfigures the other consumer: outcomes `error`, `reconfigured`; one session naming only its GUID; its new choices saved. |
| Stale saved choices, flag file without this archive's entry | Exit 1, `error`, one run error `Choices file <path> has no entry for archive 'example.zip'`, choices kept. | The existing explicit-entry recovery (`reconfigured`). |
| Every validate run | Guide, mod and game trees byte-identical before and after (scratch excluded). | — |

### Results (source `a436c05` plus the round-3 follow-up, cloud Linux)

- `probe.py`: **224/224 checks, 55 production invocations**. Default-feature debug
  binary SHA-256 `4eb8d8da757f9bb7cf0a32a6c8829ca3b18bc6a44ab9464cb24f5ced33818469`
  (host-specific). Git HEAD `a436c05` plus the round-3 follow-up in `src/fomod.rs`,
  bound by hash in the receipt.
- Original pre-fix demonstration (the first 117 checks): against a
  `--no-default-features` binary built from a `git archive` of `5f81e82`
  (`FOMOD_S2A_ALLOW_UNBOUND=1`), **84/117; 33 checks failed**: every reconfiguration
  and shared-session check; stale validate exit 0 and recovery exit 1; all
  actionable-text checks; the registered-only route warning; and registered invalid
  choices passing validation (exit 0).
- Follow-up pre-fix demonstration: the current probe against a
  `--no-default-features` binary built from a `git archive` of `0d6aad1` (output
  `target/cloud-proof/fomod-s2a-pre0d6aad1`) gives **163/182; 19 checks fail**. See
  [section (d)](#d-review-follow-up).
- Round-2 pre-fix demonstration: the current probe against a
  `--no-default-features` binary built from a `git archive` of `93837e5` (output
  `target/cloud-proof/fomod-s2a-pre93837e5`) gives **209/217; 8 checks fail**. See
  [section (e)](#e-round-2-follow-up).
- Round-3 pre-fix demonstration: the current probe against `--no-default-features`
  binaries built from `a436c05` gives **218/224; 6 checks fail** (output
  `target/cloud-proof/fomod-s2a-prea436c05`), and with the unparseable skip in
  `dispatch` removed **215/224; 9 checks fail** (output
  `target/cloud-proof/fomod-s2a-prea436c05-noskip`). The tree was restored after
  each build. See [section (f)](#f-round-3-follow-up).
- Regressions in isolated namespaces: S1 `probe.py` **339/339 (61 cases)**,
  `inherited_controller.py` **41/41**, Nexus H1a `probe.py` **138/138**. No S1
  expectation changed.
- Gates, all exit 0:
  - `cargo fmt --all -- --check`;
  - `cargo build`/`clippy -D warnings --locked --lib --bin modsync -j2`, with and
    without `--no-default-features`;
  - `x86_64-pc-windows-gnu` Clippy `-D warnings` in both feature modes.

  No Rust test or harness was created, modified, compiled or run, and
  `--all-targets` was not used.

## (d) Review follow-up

The [SPEC and QUALITY reviews](../fomod-s2a-review.md) of `0d6aad1` required changes.
The follow-up applies the controller decisions:

- **Recovery hint** (`src/fomod.rs` `CHOICES_RECOVERY_HINT`) names only
  `fomod-inspect --save-choices` and `fetch-downloads --record --fomod-choices`. The
  run-scoped `validate`/`install --fomod-choices` route is no longer advertised; it
  is an open S2 item below.
- **Choices source.** Only `--fomod-choices` reconfigures valid saved choices or
  recovers stale ones (`Options::choices_flag`). The environment variable supplies
  pending consumers only, and `--fomod-skip` still wins over the flag.
- **Needed load error.** When a consumer has stale saved choices, a flag file that
  cannot be read or parsed is reported once in `errors`; the parse error names the
  path. With only valid consumers it is still ignored (S1 lazy contract).
- **Invalid entry** keeps valid consumers `resolved` (see Design decisions).
- **Gate** (`src/engine.rs`): one no-route warning per archive path naming every
  registrant, so `warningCount` counts archives. An archive already checked for a
  component in the registered pass is not unpacked and checked again for that
  component's `Extract`.
- **Install** maps choices under the same stale context, so an unsafe mapping of
  still-valid names shows the recovery route.
- **Saved-choices parse error** has one wording through
  `fomod::saved_choices_file` for Fetch, the gate and install (its hint is the round-2 repair hint, section (e)).
- **Probe**: every validate run asserts unchanged guide, mod and game trees. The
  fixture root is removed at exit unless `MODSYNC_PROOF_KEEP=1`; receipts under
  `target/cloud-proof` are kept.

The 19 checks that fail against `0d6aad1`:

- invalid entry: `shared_invalid_outcomes`, `_consumer_errors`, `_run_error`;
- environment reconfiguration: `env_valid_outcomes`, `_sessions`,
  `_guide_unchanged`, `env_outcomes`, `env_saved`, `env_session_scope`, and
  `stale_env_exit`, `_outcomes`, `_choices_kept`. `skip_flag_guide_unchanged` and
  `flag_control_outcomes` follow from the environment having already reconfigured
  that fixture. Skip precedence itself already held at `0d6aad1`;
- discarded load error: `stale_missing_file_reported`, `stale_malformed_file_reported`;
- duplicate warning: `registered_shared_warning_count`, `_names`;
- install hint: `stale_mapping_install_actionable`.

The validate tree-invariance checks and the lazy missing-file case pass at both
commits. They are invariants, not fixes.

## (e) Round-2 follow-up

The [re-review](../fomod-s2a-review.md) of `93837e5` required changes:

- **Unparseable saved choices** (`src/fomod.rs` `CHOICES_REPAIR_HINT`, used by
  `saved_choices_file` for Fetch, the gate and install). Saved `FomodChoices` that
  `ChoicesFile::parse` rejects no longer advertise `fetch-downloads --record
  --fomod-choices`, which re-parses the same metadata and always failed. The hint
  says to repair (round 3: removal qualified) that component's `FomodChoices`, then capture
  with `fomod-inspect --save-choices` and record with `fetch-downloads --record
  --fomod-choices`. `CHOICES_RECOVERY_HINT` stays for parseable choices that are
  stale or invalid against the current archive.
- **No overwrite.** Fetch (`src/fomod_cli.rs` `dispatch`) excludes consumers with
  unparseable saved choices from a flag reconfiguration session, so they keep their
  own error and are never rewritten, which could discard other archives' entries.
  Other consumers of the archive are still reconfigured (a stale co-consumer is
  probed in round 3, section (f)) or configured (pending co-consumer; same
  `targets` path, not probed).
- **Missing entry.** When a consumer has stale (parseable) saved choices and the
  loaded `--fomod-choices` file has no entry for that archive, the run reports
  `Choices file <path> has no entry for archive '<name>'` once in `errors`.
  Unparseable consumers do not count as needing the file.

The 8 checks that fail against `93837e5`:

- repair hint instead of the Fetch recovery route:
  `unparseable_version2_fetch_repair_hint`, `_no_recovery_route`,
  `unparseable_version2_validate_repair_hint`, `unparseable_install_repair_hint`, and
  `unparseable_duplicate_fetch_repair_hint`, `_no_recovery_route`,
  `unparseable_duplicate_validate_repair_hint`;
- missing entry: `stale_noentry_reported`.

The unparseable Fetch exit, outcome, unchanged saved choices and empty sessions pass
at both commits (`apply_bound_archive` already refused to re-parse the metadata), as
do the no-entry control and the repaired positive control.

## (f) Round-3 follow-up

The [re-review](../fomod-s2a-review.md) of `a436c05` approved SPEC and required
QUALITY changes:

- **Probe for the unparseable skip** (`src/fomod_cli.rs` `dispatch`, unchanged).
  New case `unparseable_shared`: one archive with an unparseable (version 2)
  consumer and a stale parseable consumer, and a `--record --fomod-choices` file with
  a valid entry. It asserts exit 1, outcomes `error`, `reconfigured`, one session
  naming only the second GUID, the unparseable metadata value-equal, the second
  consumer's new choices saved and the repair hint on the first. The archive is
  pre-placed: two components fetching one URL concurrently race on the destination
  file (`File exists`), an S1 download behavior outside this slice.
- **Repair hint wording** (`src/fomod.rs` `CHOICES_REPAIR_HINT`): repair comes first,
  and removal is qualified as discarding every archive's saved choices for the
  component. The capture and record steps stay. The probe's `REPAIR` text follows.
- **PROGRESS** names `a436c05` and this follow-up.

Against `a436c05`, the 6 hint checks fail: `unparseable_version2_fetch_repair_hint`,
`unparseable_version2_validate_repair_hint`, `unparseable_install_repair_hint`,
`unparseable_duplicate_fetch_repair_hint`,
`unparseable_duplicate_validate_repair_hint` and `unparseable_shared_repair_hint`.
The new case's other checks pass there, because the skip is `a436c05` behavior.
With the skip removed, 3 more fail: `unparseable_shared_outcomes` (`error`,
`error`), `_session_scope` (none) and `_other_saved` (still `Beta`): the session
fails on re-parsing the unparseable metadata, so neither consumer changes.
`unparseable_shared_unparseable_unchanged` passes either way because
`apply_bound_archive` refuses to re-parse the metadata.

## Observations reported, not fixed (out of scope)

- Plain `validate` (without `--validation-report`) and `install` do not run the
  registered-archive gate. They catch stale choices only on the `Extract` path.
- S1 warn/skip on a C# `configured` consumer without choices rewrites its status to
  `warned`/`dismissed` under `--record`. This is honest but loses the C# marker.
  The new stderr message names the import route.
- Gate dependency-scope expansion (C# `ExpandWithHardDependencies`) and missing
  registered archives without prompt state (a C# soft warning) remain unimplemented.

## What remains of S2

- Run-scoped `validate`/`install --fomod-choices` (`ChoicesFile::apply_to_remaining`,
  `merge=false`) replaces **all** embedded archive choices for that run, so a file
  naming one archive drops the others for the run. It is a source reading only, is
  not probed, and is not advertised as a recovery route.
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
cargo build --locked --lib --bin modsync -j2
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-s2a-proof/probe.py
```

Set `MODSYNC_PROOF_KEEP=1` to keep the temporary fixture root.
