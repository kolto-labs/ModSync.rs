# FOMOD S2b: per-archive run choices, Unicode names, plain-verb gate, fallback warnings

Scope is the bounded S2b slice of [the remaining plan](../fomod-remaining-plan.md) S2,
continuing [S2a](../fomod-s2a-proof/README.md#what-remains-of-s2). The base is `b6d1d88`;
the slice is commit `37cd4cc`, its review follow-up commit `c28dee0`, and an
second follow-up `6dfbf63` (SPEC and QUALITY approved) that aligns run-choice scope with engine source matching
(see "Review follow-up" below).
Ownership propagation, desktop work and S3 are out of scope. This slice does not
change F15 or F29 status.

## (a) C# reference findings

Paths are under `/home/user/ModOrchestration/src/ModSync.Core/`.

| Topic | C# behavior | Where |
| --- | --- | --- |
| Choices file on validate/install | None. `--fomod-choices` exists only for Fetch's post-download hook. Rust's run-scoped `validate`/`install --fomod-choices` is a Rust extension. | `CLI/ModBuildConverter.cs:382`, `:1111` |
| Name matching | Archive, step and group lookup and the plugin set all use `OrdinalIgnoreCase`. | `Services/Fomod/FomodChoicesApplier.cs:94`, `:101`, `:118`, `:51` |
| Flag names | The flag map is `OrdinalIgnoreCase`. Flag values compare `Ordinal`. | `Services/Fomod/FomodInstallerPresenter.cs:81`, `Services/Fomod/FomodToComponentMapper.cs:152-154`, `:367-368` |
| Prompt-status keys | A resource matches its archive `OrdinalIgnoreCase`. The status map is rebuilt `OrdinalIgnoreCase`, so a status is written under an existing key of any case. The key is only trimmed. | `Services/Fomod/FomodDownloadPromptState.cs:157-161`, `:119`, `:194`, `:122`, `:202` |
| Gate on every validate | The pipeline always runs `FomodConfigurationGate.Validate`. Its only skip is the GUI install-start readiness preset, and that page gates FOMOD itself. | `Services/Validation/InstallationValidationPipeline.cs:458`, `Services/Validation/ValidationPipelineOptions.cs:69-77` |
| Gate at install | Gate issues block installation before any work, both single-component and all-selected. Gate warnings are not logged there. | `Services/InstallationService.cs:1178`, `:1251` |
| Gate warnings | Soft findings: a registered archive missing without prompt state, and `configured` with no archive-scoped instructions. | `Services/Fomod/FomodConfigurationGate.cs:133`, `:174` |
| Mapper fallbacks | `Logger.LogWarning` fires when: a file or game dependency is not evaluated and treated as satisfied (step visibility, `:160`/`:165`; conditional patterns, `:350`); a conditional pattern is skipped as an unsupported shape (`:256`) or because no single plugin sets its flags (`:276`); or an authored rename is dropped (`:431`). | `Services/Fomod/FomodToComponentMapper.cs` |
| Defaults and auto-selection | Required/Recommended preselection and SelectAll / first-of-ExactlyOne/AtLeastOne defaults are applied silently. | `Services/Fomod/FomodToComponentMapper.cs:203-204`, `:220-240` |

C# has no warning for group defaults or auto-selection, so none was added. "Defaults"
here means the dependencies that are treated as satisfied by default; those warnings
are covered.

## (b) Changes

- **Per-archive run choices** (`src/fomod.rs` `apply_to_remaining` ~952 and
  `update_guide` ~1137). `validate`/`install --fomod-choices` now merges each file
  entry into the embedded choices:
  - An entry replaces that archive's choices (archive names match case-insensitively),
    and other archives keep theirs.
  - Completed components are skipped, as before.
  - An entry is applied only to a component (or option) that registers the archive or
    names it in one of its own Extract sources (`references_archive` ~815; wildcards
    follow the reference path matcher). Other components, including ones with
    unparseable saved choices, are untouched.
  - The guide is never written.
  - Unparseable embedded `FomodChoices` go through `saved_choices_file` and fail with
    the S2a repair hint. They are never silently replaced.
  - The editor merge (`merge_into_remaining`) now shares this code. Its behavior is
    unchanged.
- **Unicode names** (`src/fomod.rs` `fold_case`/`eq_ignore_case`, lines 5-36). These use
  the simple one-to-one uppercase mapping per scalar value (`Ä`≡`ä`; `ß`≢`SS`). Rust's
  `char::to_uppercase` is the full mapping, so a single-character result is taken as
  is, a multi-character expansion keeps the character, and the Greek iota-subscript
  letters (U+1F80-1F87, 1F90-1F97, 1FA0-1FA7, 1FB3, 1FC3, 1FF3), whose simple mapping
  is a titlecase letter, are mapped explicitly. They are used for:
  - choices-file archive lookup and duplicate detection;
  - step, group and plugin lookup in `apply_to`/`apply_names`;
  - flag names in step visibility and conditional-pattern attachment;
  - rename detection;
  - resource and prompt-status archive keys;
  - the bound-session archive check;
  - the gate's registered-name dedup (`src/engine.rs` ~754).

  `write_prompt_status` (~889) updates any existing key for the archive, in any case,
  instead of adding a differently cased twin. It inserts the old key form only when no
  key matches.
- **Gate on plain verbs** (`src/engine.rs` `fomod_gate` ~277).
  `validate_fomod_configuration` now borrows the engine, and it runs:
  - in `Engine::validate` before simulation, which covers plain `validate` and
    `--phase remaining` validate;
  - in `install_observed` after the scan and receipt check, before the silent
    preflight, the snapshot and any write.

  Issues fail with the existing actionable messages under "FOMOD configuration check
  failed" (install adds "Installation blocked before any write"). No-route warnings
  become `WARNING:` events. The gate reads archives into scratch only.
  - Each archive path is inspected once per gate run (`inspected`, `src/engine.rs`
    ~739), and every inspected `(path, component)` pair is recorded in `checked`, so
    the Extract pass never re-reads an archive. Inspection goes through
    `fomod::inspect_listed` (~706): a verified, non-spooling listing
    (`archive::list`, `src/archive.rs` ~116) decides, and only an archive that lists
    `fomod/ModuleConfig.xml` is spooled and parsed.
- **Fallback diagnostics** (`src/fomod.rs` `ModuleConfig::fallback_warnings` ~527,
  `log_fallback_warnings` ~586, `dropped_rename` ~594, `PatternPlacement`/`place_pattern`
  ~414-475). `selected_files` and the warnings now share one pattern classification.
  - `FlagCondition::Always` now carries the C# label (`src/fomod/xml.rs` ~114-117).
  - Warnings are logged per installer with C#'s exact `[FomodToComponentMapper]`
    text (`FomodToComponentMapper.cs:160`, `:165`, `:257`, `:277`, `:350`, `:432`),
    with no archive prefix, through `crate::logging::warning`. This happens once per
    archive in each gate run (`src/engine.rs` ~789, ~832), for Fetch archives with an
    accepted session (`src/fomod_cli.rs` ~661), and in `fomod-inspect`
    (`src/main.rs` ~998).
  - `main` mirrors the log to stderr for `validate`, `install`, `fetch-downloads` and
    `fomod-inspect` (`src/main.rs` ~782). The validation report is unchanged,
    matching C#'s report, and fallback warnings never reach stdout. The gate's
    no-route warning is an ordinary event, so plain `validate`/`install` (with or
    without `--json`) print one `WARNING:` event line for it on stdout.
- **Report summary** (`src/validation.rs` ~274) uses C#'s
  `All detected FOMOD archives are configured (N warning(s)).`.

### Design decisions

- **Run-scoped choices remain a documented Rust extension.** C# has no
  validate/install choices file. A per-archive merge is the least surprising meaning:
  it matches the Fetch reconfiguration route (S2a) and the editor merge, and a file
  for one archive no longer drops every other archive for the run.
  - With `--phase remaining` (context `Reconstruct`), the file is applied to every
    referencing component, completed or not. That is how the completed definitions
    are reconstructed from the same inputs as the base run, so both phases must use
    the same binary. A base recorded by a build before `37cd4cc`, or by `37cd4cc`
    before this follow-up (which scoped the file to referencing components), must be
    re-run.
  - The receipt then refuses a different file before any write (`phased_*`).
  - The desktop's `Retained` context skips completed components.
- **Unparseable embedded choices + file** fail instead of being overwritten, because
  a merge cannot preserve entries it cannot read. At `b6d1d88` the file silently
  replaced them.
- **Case mapping.** .NET ordinal casing is a simple per-character uppercase mapping.
  Rust `char::to_uppercase` is full mapping, so characters whose full uppercase is
  several characters (e.g. `ß`) are left unchanged, which matches .NET because their
  simple mapping is the identity. The only exceptions are the iota-subscript letters,
  whose simple mapping is a titlecase letter; they are mapped explicitly and covered
  by the `unicode_iota_*` fixtures. Unverified residual: .NET casing-table exceptions
  (for example U+0131, U+017F) are not replicated.
- **Gate at install and plain validate** follows C#. The desktop install now runs it
  too, through the shared engine. The missing-archive-without-state soft warning and
  hard-dependency scope expansion stay out of scope (S3 gate/report scope).
- **(Superseded 2026-10-03: since `f68dcfc` validate/install report fallbacks as counted FomodConfiguration warnings; see `../fomod-visible-fallbacks.md` and `../runtime-revalidation-2026-10-03.md`. The probe now checks that contract.)** **Fallback warnings are log lines, not report warnings.** C# logs them from the
  mapper, which runs at configuration time. Rust maps fresh at validate/install, so
  logging there is the minimal equivalent. Adding them to `warningCount` would change
  the report compared with C#.

## (c) Controlled production proof

`probe.py` follows the S2a conventions:

- a `FOMOD_PARENT_NETNS` guard;
- a namespace with no routes;
- a loopback HTTPS origin whose leaf is signed by a private CA;
- private HOME/XDG/settings/mods/game/scratch directories.

Fixtures are generic, and it uses only production verbs: `fetch-downloads`,
`fomod-inspect`, `validate` (plain and `--validation-report`) and `install`, including
`--phase base/remaining`. Output goes to `target/cloud-proof/fomod-s2b`. There is one
JSON per run (argv, exit, stdout, stderr, before/after hashes) with fixture trees. The
receipt binds git HEAD and porcelain, source hashes and the binary SHA-256.

Standing invariants:

- Every validate run leaves the guide, mod and game trees byte-identical (scratch
  aside).
- Every install leaves the guide byte-identical: run choices are never persisted.

| Case | Negative | Positive control |
| --- | --- | --- |
| Per-archive merge (two archives, embedded `Alpha`/`Alpha`) | An invalid entry fails install (game untouched) and the report. Unparseable embedded choices plus a file fail with the repair hint, game untouched. | Embedded only gives `a`,`c`. `{example: Beta}` on install/report/plain gives `b`,`c`. `EXAMPLE.ZIP` replaces and does not duplicate. Both entries give `b`,`d`. The guide and saved choices are unchanged. A parseable single-archive guide takes the entry. |
| Phased | `--phase remaining` with a different file is refused, and the game stays as the base left it. | `--phase base` then `--phase remaining` with the same file gives `b`,`c`, then `notes`. |
| Unicode archive/step/group/plugin | `öBEL` (another letter) and `STRASSE` (ß is not SS) fail report and install, game untouched. | For each level alone, a folded name passes report, plain validate and install (`b`). Exact names and `STRAßE` pass. A folded run-file entry `ÄRCHIVE.ZIP` replaces the embedded one. |
| Unicode flags | Value `AN` vs `an` stays case-sensitive: only `a`. | Flag `ä` set as `Ä` shows the step and attaches the pattern: `a`,`e`,`f`. |
| Prompt-status key | — | Key `ärchive.zip` is read for `Ärchive.zip` (and ASCII `example.zip` for `Example.zip`). Fetch `--record` with a folded or exact entry updates it in place to one key, `configured`, then installs `b`. |
| Gate, plain validate/install | Missing, stale and configured-only choices on a registered-only FOMOD archive, and a missing archive with recorded state: exit 1 with the actionable text. Install leaves the game and mods untouched. | Valid choices give exit 0 plus one ` / 0: WARNING: ` route event, and install copies `notes`. The same guide with `Extract` has no warning. A registered non-FOMOD archive and a missing archive without state pass. |
| Unicode iota subscript | — | `ᾈΛΦΑ` selects plugin `ᾀλφα` and `ᾼΒ` selects `ᾳβ` on report and install. |
| Run-choice scope | A registering-only component with a stale embedded choice fails plain validate. Without a file, a folder-source (`<<modDirectory>>/Pack`) and a character-class (`[e]xample.zip`) Extract fail install (no choices), game untouched. | An unrelated component with unparseable `FomodChoices` does not fail install/report/plain with a file for another component's archive (`b`,`notes`). The registering-only component takes the file entry (exit 0). A second archive keeps its own embedded choice (`b`,`c`). The folder-source and character-class components take the file entry: install gives `b`, report and plain validate exit 0. |
| Gate unpack count (inotify on `--scratch-dir`) | — | Plain validate spools a non-FOMOD archive registered by two components 0 times, a FOMOD archive (3 entries) registered by two components 3 entries (once), the same with one Extract 6 (gate once plus simulation once), and a non-FOMOD archive registered twice and extracted once 1 (simulation only). |
| Fallback warnings | — | All five C# warnings, each once and with C#'s exact text, on report/plain/install/`fomod-inspect`/Fetch stderr. The report keeps `warningCount` 0 and no stage warning. Install writes `b`,`e`,`g`. A resolved Fetch and a clean installer log nothing. |

### Results (cloud Linux)

- `probe.py`: **254/254 checks, 89 production invocations**. The binary is the
  default-feature debug build; its SHA-256 is host-specific and recorded in the
  receipt. The working tree is `c28dee0` plus the scope follow-up `6dfbf63`, bound
  by source hash in the receipt. (`c28dee0` recorded 232/232 with 81 invocations; the
  original slice recorded 200/200 with 67 invocations on an uncommitted `b6d1d88`
  tree.)
- Scope follow-up pre-fix demonstration: the same probe against a default-feature
  binary built from a `git archive` of `c28dee0` in a scratch directory with its own
  `CARGO_TARGET_DIR` (`FOMOD_S2B_ALLOW_UNBOUND=1`, output
  `target/cloud-proof/fomod-s2b-prec28dee0`) gives **244/254; 10 checks fail**:
  `scope_{folder,char_class}_install_exit`, `_install_bytes`, `_report_exit`,
  `_report_stage_passed` and `_plain_exit`. Each run failed with "FOMOD archive
  'example.zip' has no explicit FomodChoices configuration": the file entry never
  reached the component. The `_needs_choices_*` controls pass at both commits.
- First follow-up (`c28dee0`) pre-fix demonstration: its 232-check probe against a `--no-default-features`
  binary built from a `git archive` of `37cd4cc` in a scratch directory with its own
  `CARGO_TARGET_DIR` (`FOMOD_S2B_ALLOW_UNBOUND=1`, output
  `target/cloud-proof/fomod-s2b-pre37cd4cc`) gives **213/232; 19 checks fail**: all
  four `scope_unrelated_*` (the file reached the unrelated component and its
  unparseable choices failed the run), all six `unicode_iota_*`, the five
  `fallback_*_warnings` (archive prefix, and "is supported." without "in this
  slice"), and the four `unpack_*_spooled` counts, which were 2, 6, 9 and 4 instead of
  0, 3, 6 and 1.
- Original pre-fix demonstration (the 200-check probe committed in `37cd4cc`) against
  a binary built with `--no-default-features` from a `git archive` of `b6d1d88` in a scratch directory with its own
  `CARGO_TARGET_DIR` (`FOMOD_S2B_ALLOW_UNBOUND=1`, output
  `target/cloud-proof/fomod-s2b-preb6d1d88`) gives **130/200; 70 checks fail**:
  - merge: `merge_install_exit`/`_bytes`, `merge_report_exit`, `_stage_passed`,
    `merge_plain_exit`, `merge_case_exit`/`_bytes`; unparseable:
    `unparseable_install_exit`, `_repair_hint`, `_game_untouched` (the file silently
    replaced the choices and installed), `unparseable_report_exit`; phased: all five
    `phased_*` except `phased_other_file_exit`;
  - Unicode: all 16 `unicode_{archive,step,group,plugin}_*` checks,
    `unicode_file_entry_exit`/`_bytes`, `unicode_flags_bytes`,
    `unicode_status_status_read`, all six `unicode_fetch_folded_entry_*` checks, and
    `unicode_fetch_exact_entry_status_in_place` (a twin `Ärchive.zip` key was added);
  - gate: every `gate_missing_*`, `gate_stale_*`, `gate_configured_only_*` and
    `gate_archive_missing_*` check (at `b6d1d88` plain validate and install exit 0 and
    install copies `notes`), plus `gate_valid_plain_warning` and
    `gate_valid_install_warning`;
  - fallbacks: `fallback_report_warnings`, `_report_once`, `_plain_warnings`,
    `_install_warnings`, `_inspect_warnings`, `_fetch_warnings`.

  The negative controls (another letter, ß, flag value case), the clean and resolved
  quiet checks, the gate positive controls' exits and all tree invariants pass at
  both commits.
- Regressions in isolated namespaces, each with its own guard set from
  `readlink /proc/self/ns/net`: S2a `probe.py` **224/224**, S1 `probe.py`
  **339/339 (61 cases)**, `inherited_controller.py` **41/41**, Nexus H1a `probe.py`
  **138/138**, fetch-shared-url `probe.py` **381/381**. No expectation changed.
- Gates, all exit 0:
  - `cargo fmt --all -- --check`;
  - `cargo build`/`clippy -D warnings --locked --lib --bin modsync -j3`, with and
    without `--no-default-features`;
  - `x86_64-pc-windows-gnu` Clippy `-D warnings` in both feature modes.

  No Rust test or harness was created, modified, compiled or run, and
  `--all-targets` was not used.

## Review follow-up

The independent review of `37cd4cc` found five issues; commit `c28dee0` fixes them and
the proof above covers each:

- The gate unpacked every registered archive and re-unpacked non-FOMOD and shared
  archives; it now lists first, spools only FOMOD archives and inspects each path once
  (`unpack_*`).
- The fallback text had an archive prefix and dropped "in this slice"; it is now C#'s
  exact text (`fallback_*_warnings`).
- Iota-subscript letters did not fold like .NET (`unicode_iota_*`).
- A run choices file reached every component, so an unrelated component with
  unparseable `FomodChoices` failed the run (`scope_*`).
- Phased runs and the `--json` stdout claim are now documented precisely (above and in
  `COMPATIBILITY.md`).

The re-review of `c28dee0` found that its scope check compared only an Extract
source's last path component with the archive name, so a folder source or a glob
character class lost its choices (a regression from `37cd4cc`). The second
follow-up matches the way the engine resolves sources (`scope_folder_*`,
`scope_char_class_*`):

- The last source component is matched with the engine's own glob matcher
  (`*`, `?`, case-insensitive after Unicode folding). Since 2026-10-04 that matcher
  follows C# `PathHelper.WildcardPathMatch`: `[` and `]` are literal and `**` stays
  within one level (see [delete-directory-contract.md](../delete-directory-contract.md)).
  `scope_char_class_*` therefore now expects `[e]xample.zip` to name no file: the
  FOMOD stage still passes, but install, the report and plain validate refuse the
  missing source (exit 1, no game writes). The reference refuses the same Extract
  with `MissingSourceFiles` (delete-directory proof, `extract_bracket_literal`).
  A third label, `scope_bracket_literal_*` (11 checks), extracts an archive literally
  named `[e]xample.zip` that the Source names literally (Destination
  `<<modDirectory>>/bracket`): it is in scope and installs. The total is now 265.
- The archive's folder is not known when choices are applied, so any source whose last
  component is not an archive file name (`.zip`, `.7z`, `.rar`, `.exe`, `.tar`, `.gz`)
  is treated as a possible folder containing it. An unparseable glob is kept as well.
  Over-inclusion only applies an entry for a named archive to a component that may not
  extract it.
- Completed components are never touched. A completed component that no entry
  references was also untouched in the base phase, so its recorded definition
  still matches.

## What remains of S2

- Ownership propagation.
- Gate scope: hard-dependency expansion (`ExpandWithHardDependencies`) and the soft
  warning for a missing registered archive without prompt state (S3 gate/report
  scope).
- The C# "configured without archive-scoped instructions" warning has no direct
  analogue. Rust's closest one is the S2a no-Extract route warning.
- .NET casing-table exceptions (see Design decisions) are unverified.
- Prompt-status keys are still written lowercased (ASCII) where C# keeps the trimmed
  name.
- Desktop reconfiguration and native acceptance.
- The S3 restart/recovery and collision corpora.
- Serialized C# interop beyond explicit refusal plus the choices import route.

F15 remains partial.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j3
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-s2b-proof/probe.py
```

Set `MODSYNC_PROOF_KEEP=1` to keep the temporary fixture root.


Residual (documented at merge): a folder source whose own name ends in an archive
extension is compared as a file name, so its component fails for lack of choices rather
than installing unconfigured; an included component with unparseable `FomodChoices`
fails with the repair hint.

Update 2026-10-03: rooted resolution (`a185ced`) closed this residual for the CLI; a
suffixed directory now owns only the archives it contains. Ownership propagation, gate
dependency scope and the missing-archive soft warning are settled in
[the gate-scope contract](../fomod-gate-scope.md).
