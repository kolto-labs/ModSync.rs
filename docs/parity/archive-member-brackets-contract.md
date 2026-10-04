# Archive members with `[` or `]` in their names

This contract covers an archive whose member names contain `[` or `]`, such as
`Override/x[1].tga` or a folder `[HD] textures/`, and a game tree that already holds
such a file when component checkpoints are on. The reference is pinned C#
`d31ccbd3`; C# paths below are under `src/ModSync.Core/` there. Evidence:
[archive-member-brackets-proof](archive-member-brackets-proof/README.md). F06 and F05
stay partial.

## Triage at `31e98ec`

**C# behaviour.** `RealFileSystemProvider.ExtractArchiveAsync` walks every entry
(`Services/FileSystem/RealFileSystemProvider.cs:210-260`) and writes it at the path
`PathHelper.TryGetZipSafeArchiveEntryExtractPath` gives
(`FileSystemUtils/PathHelper.ArchiveExtraction.cs:19-66`). That helper refuses only an
empty key, a rooted key, a `..` part and a path outside the extraction root; every
other character is kept. The dry run's `VirtualFileSystemProvider.ExtractArchiveAsync`
uses the same helper (`Services/FileSystem/VirtualFileSystemProvider.cs:899-945`). So
`x[1].tga` is extracted as a file named `x[1].tga`, and later Sources reach it with
`[` and `]` as ordinary characters (`WildcardPathMatch`, see
[delete-directory-contract.md](delete-directory-contract.md)).

**Rust at `31e98ec`.** Every archive reader admits each member name through
`Output::name` (`src/archive.rs:41-80`), which called `paths::relative(name, false)`.
`relative` with `wildcards = false` refuses `*`, `?`, `[` and `]`
(`src/paths.rs:109-112` before this slice), so one bracketed member failed the whole
archive with `Wildcards are not allowed here: Override/x[1].tga`. Every reader of an
archive shares that admission:

- the FOMOD configuration gate, which lists each Extract's archive
  (`fomod::inspect_listed`, `src/fomod.rs:776-791`, from `Engine::fomod_gate`,
  `src/engine.rs:493`): plain `validate` and `install` refused before any write
  (`FOMOD configuration check failed: ...`);
- the archive/destination stage of the validation pipeline
  (`validate_component_archives`, `src/engine.rs:885-890`): `Cannot inspect archive`;
- extraction itself (`src/engine.rs:2231`), implicit source recovery
  (`src/engine/source_resolution.rs:473`) and archive verification after a download or
  peer transfer (`src/download.rs:689`, `src/download_orchestrator.rs:1925`,
  `src/p2p.rs`, `src/peer_sharing.rs`), which reported such an archive as invalid;
- the editor's resource browser (`src/editor/browser.rs:104`,
  `src/ui/editor_panel.rs:173`, `:1181`).

Component checkpoints (`install --component-checkpoints`, the desktop checkpoint
option) validate every captured tree name with the same rule
(`checkpoint::read::validate_tree`, `src/checkpoint/read.rs:167`), so any game or mod
file whose name holds `[` or `]` made checkpoint capture fail, even with no archive
involved. `Store::provenance` (`src/checkpoint/history.rs:320`) refused such a path the
same way.

**Status.** A visible difference: Rust refused any guide whose archive holds a
bracketed member name (common for versioned readmes and `[HD]`-style folders), where
C# installs it; checkpointed installs refused any tree holding such a file.

## Behaviour after this slice

- `paths::literal_relative` (`src/paths.rs:107-114`) admits a relative path whose
  names are taken literally: the same rules as `relative` (no absolute paths, `..`,
  `:`, NUL, `<`, `>`, `|`, `"`, trailing space or dot, or device names) except that
  `[` and `]` are ordinary characters. `*` and `?` stay refused. `relative` keeps its
  behaviour; both share `checked_relative` (`:116-142`).
- Archive member admission (`src/archive.rs:48`), the editor's archive member tree and
  projection (`src/ui/editor_panel.rs:173`, `:1181`), checkpoint tree validation
  (`src/checkpoint/read.rs:167`) and checkpoint provenance
  (`src/checkpoint/history.rs:320`) use it. So every reader listed above now accepts
  bracketed members, and checkpoint capture accepts bracketed file names.
- Entry paths are joined to the extraction folder and stored as file keys; they are
  never used as patterns. Sources that later name them already treat `[`/`]` as
  literal (`source_wildcard_glob`), so `Copy <<modDirectory>>/a/Override/x[1].tga`
  reaches that file and not `x1.tga`.

### Review follow-up (on `4a09589`)

- **Managed process capture (F1).** Before a Patcher, Execute or Run step, a managed
  install (`--component-checkpoints --managed`) copies a preimage of every game file
  (`managed_preimage`, `src/engine/managed.rs:171`) and stages removed files
  (`:209`), reached from the process arm (`src/engine.rs:2673-2676`).
  Both parsed each game file name with `relative(.., false)`. `4a09589` let an earlier
  component deploy `Override/x[1].tga`; a managed install skips the simulation
  preflight, so the next process step failed with `Wildcards are not allowed here`
  after that component's files were already in the game. Both now use
  `literal_relative`; these are literal game file names, and deployment's own path
  check (`relative_parts`) already accepts brackets. Probed
  (`execute_after_bracket_deploy`, managed mode).
- **FOMOD extraction folder and mapped Sources (F2).** `ModuleConfig::copy_instructions`
  and `ArchiveModule::copy_instructions` (`src/fomod.rs:686`, `:852`) re-parsed the
  extraction folder (named after the archive, so `[e]xample` for `[e]xample.zip`) and
  the archive's wrapper folder (`content_root`, such as `[HD] Mod/`) with
  `relative(.., false)`, and so did each `file`/`folder` `source` (`:693`). The gate
  wrapped the refusal as `Saved FOMOD choices ... do not apply to the current archive`,
  which is misleading. The extraction folder, wrapper and `source` are literal names
  and now use `literal_relative`. The reference mapper agrees: `FomodToComponentMapper`
  (`Services/Fomod/FomodToComponentMapper.cs:386-410`, `NormalizeRelativePath`
  `:451-479`) copies a ModuleConfig `source` such as `[x]/a.tga` verbatim into the
  generated Copy Source (`<<modDirectory>>/a/[x]/a.tga`), refusing only `..` and `:`,
  and a Copy Source takes brackets literally (`copy_literal`). The live mapper output
  is in [csharp-fomod-map.json](archive-member-brackets-proof/csharp-fomod-map.json).
  A ModuleConfig `destination` keeps the guide Destination rule (`[`/`]` refused), and
  its error now says `FOMOD file destination '...' is not a usable game path; this is
  not a stale choice` under the gate's wrapper.

Callers checked: `relative` has unchanged semantics for all its other callers, which
still refuse `[` and `]`: Destinations, a Rename Destination, a Patcher Source,
download and resource file names, FOMOD ModuleConfig destinations and maintenance
plans (below). Managed deployment's own staging and manifests never used `relative`
(`relative_parts` accepts brackets); its process capture now uses `literal_relative`.

## Edge cases compared with the live reference

Fixture: `game/{chitin.key, Override/o.txt}`, `mods/m.txt` and the case's archive
`mods/a.zip`. `MD` is `<<modDirectory>>`, `OV` `<<kotorDirectory>>/Override`. Each
case is checked four ways (five for a case that runs a program, which adds
`install --component-checkpoints --managed --allow-processes`; every verb of such a
case gets `--allow-processes`): the validation pipeline (overall decision, dry-run
decision, `DryRunSkipped`), plain `validate`, `install` and `install
--component-checkpoints` (the last three against the reference install, with the
final tree). "Before" is `31e98ec`.

| Case | Archive / tree | Steps | Result (both after this slice) | Before |
| --- | --- | --- | --- | --- |
| `plain_control` | `Override/y.tga` | Extract, Move `MD/a/Override/*` -> `OV` | installs | same |
| `extract_only` | `Override/x[1].tga`, `Override/y.tga` | Extract | `mods/a/Override/x[1].tga` | refused (4 checks) |
| `move_wildcard` | same | Extract, Move `.../Override/*` | both files in Override | refused |
| `copy_literal` | same | Extract, Copy `.../x[1].tga` | `Override/x[1].tga` | refused |
| `copy_literal_backslash` | same | the same with `\` separators | as above | refused |
| `copy_bracket_wildcard` | same | Copy `.../x[*` | `Override/x[1].tga` | refused |
| `copy_literal_not_class` | `x[1].tga`, `x1.tga` | Copy `.../x[1].tga` | only `x[1].tga` copied | refused |
| `bracket_folder_copy` | `[HD] textures/{t.tga,u.tpc}` | Copy `MD/a/[HD] textures/*` | both copied | refused |
| `unmatched_brackets` | `a[.tga`, `b].tga`, `[c].tga` | Extract, Move `*` | all three moved | refused |
| `rename_bracket_source` | as `extract_only` | Rename `.../x[1].tga` -> `x.tga` | renamed | refused |
| `delete_bracket_source` | as `extract_only` | Delete `.../x[1].tga` | deleted | refused |
| `bracket_archive_name` | `[e]xample.zip` holding bracketed members | Extract, Move | installs | refused |
| `recovery_copy_literal`, `recovery_plain_control` | as `extract_only`, no Extract | Copy a member path | missing source in both (`DryRunSkipped`; validate and install refuse) | same |
| `existing_game_bracket_file` | game holds `Override/pre[1].txt` | Copy `MD/m.txt` -> `OV` | installs, also with checkpoints | checkpointed install refused |
| `star_member`, `question_member` | `Override/s*.tga`, `Override/q?.tga` | Extract, Move | known difference (below) | same |
| `execute_after_bracket_deploy` | as `extract_only`, plus `mods/run.sh` (`exit 0`) | component 1 Extract, Move `*`; component 2 Execute `MD/run.sh` | installs, also managed | managed install failed after component 1 (`4a09589`); refused (`31e98ec`) |
| `execute_after_plain_deploy` | `Override/y.tga`, `run.sh` | the same | installs, also managed | same |

FOMOD cases (Extract of an archive with a `fomod/ModuleConfig.xml` and saved choices,
`requiredInstallFiles` copying `source` to `Override`) are checked with plain
`validate` and `install` against expected results, because the reference install does
not map a FOMOD at Extract time (C# maps it into a component when the download is
configured). The expectation is the reference mapper's Copy (three mapper checks) and,
for a wrapper folder, the same result as the `HD Mod` control:

| Case | Archive | Wrapper | `source` | Result | `4a09589` |
| --- | --- | --- | --- | --- | --- |
| `fomod_control` | `a.zip` | `HD Mod` | `a.tga` | `Override/a.tga` installed | same |
| `fomod_bracket_wrapper` | `a.zip` | `[HD] Mod` | `a.tga` | as above | refused as stale choices |
| `fomod_bracket_archive_name` | `[e]xample.zip` | none | `a.tga` | as above | refused as stale choices |
| `fomod_bracket_source` | `a.zip` | none | `[x]/a.tga` | as above | refused as stale choices |
| `fomod_bracket_wrapper_and_source` | `a.zip` | `[HD] Mod` | `[x]/a.tga` | as above | refused as stale choices |

## Known differences

- **Patcher ini content with brackets.** A HoloPatcher `InstallList` entry, or a
  namespaces.ini `IniName`/`DataPath`, that holds `[` or `]` is refused by
  `relative(.., false)` in `src/engine/patcher_simulation.rs` (around lines 119, 154,
  163, 215 and 223). The simulation runs only virtually, so a non-managed validate or
  install refuses such a guide before any write, while a managed install (no
  simulation) proceeds. The namespaces.ini fields are also reached from
  `qualify_action` (`src/engine/source_resolution.rs`, around line 848), which can run
  at install time. Rare; not probed.
- **FOMOD destination error wording.** The bracketed-destination error is still
  wrapped by the gate's stale-choices recovery hint, so the message reads as both a
  stale choice and "not a stale choice". The refusal itself is correct and happens
  before any write.

- **`*` and `?` in a member name (deliberate).** No Windows file can have these
  characters. C# on Linux writes the file (`Override/s*.tga`); on Windows the write
  fails. Rust refuses the archive on every host, so a guide does not change meaning
  across platforms (`AGENTS.md`: reject unsafe or unsupported input). Counted as eight
  known differences (two cases, four checks each).
- **Other unsafe members (unchanged, deliberate).** C# skips a member with a `..`
  part or a rooted name and goes on; Rust refuses the archive (F06-SAFE in the
  [actions audit](engine-actions-archives-audit.md)). Names with `:`, `<`, `>`, `|`,
  `"`, a trailing dot or space, or a device name are refused in Rust; C# on Linux
  writes them. Not probed here.
- **FOMOD destinations (unchanged, deliberate).** A ModuleConfig `file` or `folder`
  `destination` with `[` or `]` is refused (`src/fomod.rs:696`), as every guide
  Destination is; C# would generate a Copy into `<<kotorDirectory>>/Override/[x]`. The
  message names the destination. Not probed against the reference. A `source` with
  `*` or `?` is still refused; C# keeps it and its Copy treats it as a wildcard. Not
  probed.
- **Maintenance transactions (unchanged, not probed).** The maintenance inventory
  (`modsync maintenance`, never an install step) refuses a subtree or a file name with
  `[` or `]`: `plan.rs:176` (the requested subtree), `plan.rs:402` (each walked name,
  `Maintenance name is unsafe or aliases another portable path`), and the journal
  checks `journal.rs:214`, `:255`. The refusal happens when the plan is built, before
  anything is changed, so a game tree holding a bracketed file cannot be maintained in
  Rust. C# has no matching maintenance transaction.
- **Literal non-Source paths (unchanged).** Destinations, a Rename Destination and a
  Patcher Source still may not contain `[` or `]`; see
  [delete-directory-contract.md](delete-directory-contract.md).
- **Download and peer verification (not probed).** These share archive admission, so a
  fetched archive with bracketed members is no longer reported as invalid. Proving it
  needs the loopback transfer harness.

## Results

| Binary | Archive member brackets proof |
| --- | --- |
| review follow-up, live reference and mapper (`--csharp ... --fomod-map ... --record`) | 83 match / 8 known difference / 0 fail (91 checks) |
| review follow-up, recorded reference | 83 match / 8 known difference / 0 fail |
| `4a09589` (first fix) | 74 match / 8 known difference / 9 fail |
| first fix, 68 checks | 60 match / 8 known / 0 fail; `31e98ec`: 15 / 8 / 45 |

On `4a09589` the 9 failures are the managed install of `execute_after_bracket_deploy`
(1) and the four bracketed FOMOD cases, `validate` and `install` each (8). On
`31e98ec` the 45 failures of the first 68 checks were the eleven bracketed-archive
cases marked "refused" above, all four checks each (44), and the checkpointed install
of `existing_game_bracket_file` (1).

Every regression probe listed in the earlier contracts stays at its baseline
(cleanlist-rows 121/20/0, root-destination 119/4/0, delete-directory 169/11/0, S2b
265, archive-generation 27/1 and the others).

## Guide-author note

Archive member names may contain `[` and `]`; they are extracted as ordinary
characters, as in C#. To name such a file in a Source, write the brackets as they
are (`<<modDirectory>>/mod/Override/x[1].tga`); they never form a character class. A
FOMOD archive, its wrapper folder and its ModuleConfig `source` paths may contain
brackets too; a ModuleConfig `destination` may not. An
archive with a member whose name holds `*` or `?` is refused, because no Windows file
can have that name.
