# Patcher INI names with brackets

This contract covers how the virtual Patcher simulation treats `[` and `]` in the
names a patcher's own INI files give: the `[InstallList]` folder values, the
`File*`/`Replace*` file names in each `install_folder*` section, and the
namespaces.ini `IniName` and `DataPath` of the selected namespace. The reference is
pinned C# `d31ccbd3`; C# paths below are under `src/ModSync.Core/` there. Evidence:
[patcher-ini-brackets-proof](patcher-ini-brackets-proof/README.md). F09 and F19 stay
partial.

## Triage at `c2225e3`

**C# behaviour.** During a dry run, `Instruction.ExecuteTSLPatcherAsync` hands each
patcher folder to `TslPatcherVfsSimulator.SimulateInstallAsync`
(`Instruction.cs:1308-1325`). That reads `changes.ini` from the folder or its
`tslpatchdata` folder (`TSLPatcher/TslPatcherVfsSimulator.cs:113-131`), parses it with
a plain line parser (`:173-209`; a line is a section header only when it starts with
`[` and ends with `]`, so a `File0=[HD]tex.tga` value is ordinary text), and for every
`install_folder*` entry copies each `File*`/`Replace*` value to
`Path.GetFullPath(Path.Combine(game, folder, file))` in the virtual tree (`:66-109`,
`:150-170`). Nothing treats the names as patterns: brackets are ordinary characters,
and the file is present for later steps. The virtual writes raise `FileWritten`, so
`FileConflictAnalyzer` attributes them to the Patcher step
(`Services/Conflicts/FileConflictAnalyzer.cs:88-110`). The simulation ignores
namespaces.ini; the real patcher reads it.

**Rust at `c2225e3`.** `src/engine/patcher_simulation.rs` turned each INI value into a
path with `paths::relative(value, false)` (lines 154 and 163 for `IniName` and
`DataPath`, 215 and 223 for the InstallList folder and file). That function refuses
`*`, `?`, `[` and `]` (`src/paths.rs:98-105`), because guide paths use those characters
as wildcards. So a patcher whose INI names a file such as `[HD]tex.tga`, a folder such
as `Override/[x]`, or a namespace folder such as `[A] Option` failed with
`Wildcards are not allowed here: [HD]tex.tga`. The simulation serves validation,
conflict analysis and install preflight (`src/engine.rs:2693`,
`src/engine/source_resolution.rs:848`), so `validate`, `validate --dry-run`,
`conflicts` and `install` all refused the guide before any write; the desktop
validation page did the same. C# validates and reports such a guide.

**User-visible difference.** A guide with a Patcher step whose mod ships bracketed
file or folder names could not be validated, analysed or installed in Rust, and the
error blamed wildcards that no guide author wrote.

## Change

The four INI-derived names now go through `paths::literal_relative`
(`src/paths.rs:112-114`), the rule archive members already use: `[` and `]` are
ordinary characters, and everything else `relative` refuses stays refused (`*`, `?`,
`..`, a rooted path, `:`, `<`, `>`, `|`, `"`, a trailing dot or space, device names).
The joined paths still pass the root containment checks. The `Arguments` named-INI
form (`relative(arguments, false)`, line 119, and the matching staging check in
`src/engine.rs`) is guide-authored text and is unchanged.

Callers: `simulation_configuration` is used by `simulate_patcher` (virtual execution)
and `qualify_action` (source qualification, also at install time); both now accept a
bracketed namespace `IniName`/`DataPath`. Real installation runs the patcher, which
reads its own INI; Rust does not parse INI names there.

## Cases and results

Each case runs the pipeline (`validate --output json --dry-run`), plain `validate`
and `conflicts` against the reference pipeline and `FileConflictAnalyzer`. Most cases
add a Rename of the simulated file, which passes only if the file is present.

| Case | INI content | Reference | Rust now | Rust at `c2225e3` |
| --- | --- | --- | --- | --- |
| `plain_file` | `File0=tex.tga` (control) | passes | match | match |
| `bracket_file` | `File0=[HD]tex.tga`, then Rename | passes | match | refused |
| `bracket_file_only` | the same, no Rename | passes | match | refused |
| `bracket_replace` | `Replace0=[a]b.2da`, then Rename | passes | match | refused |
| `closing_bracket_only` | `File0=tex]1.tga` | passes | match | refused |
| `bracket_class_lookalike` | `File0=t[ae]x.tga` (only that file exists) | passes | match | refused |
| `bracket_folder` | `install_folder0=Override/[x]` | passes | match | refused |
| `bracket_payload_missing` | `File0=[HD]none.tga`, no payload | passes | match | refused |
| `bracket_two_entries` | `[1]a.tga` and `b[2].tga` | passes | match | refused |
| `namespace_bracket_datapath` | namespaces.ini `DataPath=[A] Option` | passes | match | refused |
| `namespace_bracket_ininame` | namespaces.ini `IniName=[A]changes.ini` | passes | match | refused |
| `conflict_plain` | two patchers install `tex.tga` (control) | one conflict | match | match |
| `conflict_bracket` | two patchers install `[HD]tex.tga` | one conflict at `<<kotorDirectory>>/Override/[HD]tex.tga` | match | refused |
| `star_file` | `File0=s*.tga` | passes | known difference | known difference |
| `question_file` | `File0=q?.tga` | passes | known difference | known difference |

## Known differences

- **Simulation destination (pre-existing).** C# always places simulated writes under
  `MainConfig.DestinationPath`; Rust uses the step's resolved Destination
  (`src/engine/patcher_simulation.rs`, around line 232). Every probe case uses
  `<<kotorDirectory>>`, so this slice's results are unaffected.
- **Backslash in an INI name on Linux (pre-existing).** C# `Path.Combine` keeps `\` as a
  literal character on Linux; Rust treats it as a separator, as on Windows. The probe
  uses `/` only.

- **`*` and `?` in an INI name (deliberate, probed).** No Windows file can hold these
  characters, so a mod that names one cannot install on Windows. C# on Linux simulates
  `Override/s*.tga` and passes; Rust refuses the INI entry on every host, as it
  refuses such archive members, so a guide does not change meaning across platforms
  (`AGENTS.md`: reject unsafe or unsupported input). Six known differences (two cases,
  three checks each).
- **Namespaces (deliberate, unchanged).** C# simulation always reads `changes.ini` and
  ignores namespaces.ini; Rust simulates the namespace the step selects, as the real
  patcher does. The namespace cases give both INIs the same InstallList, so only the
  bracket admission is compared.
- **Missing payload (unchanged).** C# writes an empty file; Rust records presence with
  unknown content and a `PatcherSimulation` warning. Both pass; a later step that
  needs the bytes fails visibly in Rust.
- **Unsafe INI names (deliberate, unchanged, not probed).** `..`, a rooted path and the
  other names `literal_relative` refuses are still refused. C# `Path.Combine` would
  follow them, possibly outside the game folder.
- **Install (not compared with the reference).** The reference install runs
  HoloPatcher, which this host does not have. A manual Rust run with a stub
  `--patcher` that copies the payload and writes a log installed `bracket_file`
  (exit 0, `Override/renamed.tga`); `c2225e3` refused it before running the patcher.
- **Named-INI `Arguments` (unchanged).** `Arguments = "[x].ini"` is still refused; it
  is guide text, and C# passes it to the patcher as a namespace index anyway.
