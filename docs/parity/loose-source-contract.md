# A loose Source next to an Extract

This contract covers a component that extracts an archive and also names, in another
step, a file in the mod folder that none of its archives holds, such as `Extract
<<modDirectory>>/m.zip` and then `Copy <<modDirectory>>/loose/x.tga`. C# validation
refuses such a step (`Failed to find '...' in any archives!`), even when the file
exists, and the C# CLI `install` and the desktop Validate step run that validation
before installing. Rust's reports ran the same check, but plain `validate` and
`install` did not, so Rust installed guides the C# CLI refuses. The reference is
pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there. Rust lines before
this slice are at `6ca4d35`. Evidence:
[loose-source-proof](loose-source-proof/README.md). Tracker row F05-LOOSE-SOURCE
(found by the extract-destination slice) is closed; F05-MOVE and F05-COPY stay
partial.

## Why this gap

It changes a pass/fail decision and the files installed: the C# CLI refuses the guide
before any write, while Rust copied the loose file into the game. The check is what
keeps a C# guide's Sources tied to the archives it extracts, so a guide that relies on
a stale file left in the mod folder by an earlier install, or on a file another
component extracts, is refused instead of installing whatever is on disk.

**Impact on real guides.** The check only applies to a component that extracts at
least one archive that exists. A component with no Extract step, which is how guides
for mods the user unpacks by hand are written ("move the files from the mod's folder
to Override"), is never checked, in C# or in Rust; `no_extract` in the proof installs
in both. Converted with `convert --parse-directions`, the mod-builds corpus
(`content/k1` and `content/k2`: `full`, `spoiler-free` and `full_mobile`) drafts
steps for 210 components; 205 have no Extract and name loose Sources (unaffected).
The five others (one per guide but K1 `full_mobile`) draft an Extract of a one-letter
archive name misread from the prose (`<<modDirectory>>\r`, `<<modDirectory>>\p`),
which no mod folder holds, so they fail as a missing archive before and after this
slice, and they need review before installing anyway. No authored mod-builds guide with
instructions is on disk. A guide written for C# already passes this check, since the
C# CLI and GUI refuse it otherwise.

## Triage

**C# at `d31ccbd3`.** `ComponentValidation.Run` runs `VerifyExtractPaths` first
(`ComponentValidation.cs:42-44`). `GetAllArchivesFromInstructions` (`:191-243`) lists
the existing files of every Extract step of the component and of its immediate
options, selected or not, whose instruction conditions are met (`:206-209`,
`:227-240`). With none (no Extract, or every archive missing), only each top-level
Extract is an error (`Missing Required Archives`, `:74-89`). Otherwise every step of
the component and its options except Extract and Choose (`:90-122`) has each Source
checked: one under `<<gameDirectory>>` or `<<kotorDirectory>>` is skipped
(`:133-137`); the others go to `IsSourcePathInArchives` (`:394-471`), which strips
`<<modDirectory>>\` and asks `ArchiveHelper.MatchArchivePath`
(`Utility/ArchiveHelper.cs:1118-1224`) whether any archive's stem (`:1166-1175`), one
of its entries under the stem (`:1180-1196`) or one of their folders (`:1198-1205`)
matches the Source with `PathHelper.WildcardPathMatch`
(`FileSystemUtils/PathHelper.cs:760-820`: one level per `*`, case-sensitive,
`RegexOptions.None` at `:819`). An `.exe` archive matches anything
(`ArchiveHelper.cs:1140-1148`). A miss adds `Failed to find '<Source>' in any
archives!` (`ComponentValidation.cs:469`) unless the component is named `Improved AI`
(`:461-464`); with `MainConfig.AttemptFixes` (on by default) the check is retried
with the first folder doubled (`<stem>\<stem>\...`), which rewrites the Source when
it matches and otherwise adds the error again and fails (`:149-171`). A
DelDuplicate's Sources are extensions, which this check reads as paths, so it fails
too. The callers that decide are the full pipeline's Environment stage
(`Services/InstallationService.cs:1007-1009`, which stops the pipeline:
`Services/Validation/InstallationValidationPipeline.cs:66-81`), the pipeline's
ComponentValidation stage (`:374-417`, counting every error) and the per-component
desktop install (`Services/InstallationService.cs:1107-1111`). The CLI `install` runs
the full pipeline and stops on failure unless given `--skip-validation`
(`CLI/ModBuildConverter.cs:3576-3599`); the desktop wizard's Validate step runs it
and blocks on a failed stage. `InstallAllSelectedComponentsAsync` itself does not run
`ComponentValidation`.

Run live (contained, see the proof): a loose Copy, Move (wildcard) or Delete next to
an Extract, the same Copy in a chosen or an unchosen option, a loose Copy whose
component's only Extract is in an unchosen option, a Copy of a file an earlier Rename
made, and a Copy of a file another component extracts are all refused by the C# CLI
`install`, its full report (Environment stage) and its basic report (two errors). A
Copy or wildcard Move of the archive's own files, a game Source, loose Sources in a
component with no Extract, and a step whose condition is not met install.

**Rust at `6ca4d35`.** The archive stage (`validate_component_archives`,
`src/engine.rs:1062-1222`) collected the component's archive members by extracting
each archive to scratch (`:1083-1118`) and then checked each Source against them
(`:1197-1219`), ending the component's checks with `Failed to find '<Source>' in any
archives` on a miss. Only the reports run that stage. Plain `validate`
(`:576-583`) and `install` (`:1659-1663`) ran the FOMOD and Destination gates and then
the steps, which only need the Source to exist.

**User-visible difference** (`A` extracts `mods/m.zip`, holding `Override/a.tga` and
`readme.txt`; `mods/loose/x.tga` exists):

```
A's later step                   C# (CLI, GUI)       Rust before           Rust after
Copy loose/x.tga (or Move        refused before      reports refuse;       refused before
loose/*, Delete loose/x.tga),    any write           plain validate        any write
also in an option, chosen or                         passes; install
not                                                  copies it
Copy m/notes.txt after a Rename  refused             installs              refused
Copy m/Override/a.tga; a game    installs            installs              installs
Source
Copy loose/x.tga with no Extract installs            installs              installs
```

## Behaviour after this slice

- **The gate.** `archive_source_gate` (`src/engine.rs:642-686`) runs in plain
  `validate` (`:589`) and `install` (`:1756-1757`, before any write) after the
  Destination gate. For each selected component not yet installed it collects the
  archive members (`archive_members`, `:1230-1277`, over the steps
  `archive_check_instructions` gives, `:1213-1228`) and, when at least one archive
  exists and none is missing, checks every step of the component and its options
  whose conditions are met (`check_archive_sources`, `:1279-1314`). The first miss
  fails with `<component> [/ <option>] / step <n> (<action>): Failed to find
  '<Source>' in any archives`; `install` adds `Installation blocked before any
  write`. A component with a missing archive is left to the steps, which report it as
  before.
- **One check for reports and gate.** The archive stage (`:1116-1211`) now uses the
  same helpers, so the reports and the gate refuse the same Sources. The members
  are now listed without extracting to scratch (`archive::list_original_with_size`,
  files only); the paths and their admission rules are unchanged (the listing adds a
  16 MiB bound on the names), and an archive larger than the free scratch space is
  no longer refused by this check (the Extract step still is).
- **`Improved AI`.** As in C#, a component of that name (ignoring case) is not checked,
  in the reports too; before, Rust's reports refused it and its install did not.
- **Unchanged.** Game Sources, Extract and Choose steps, a DelDuplicate's extensions,
  components with no Extract and steps whose conditions are not met are not checked.
  Matching stays Rust's (case-insensitive by default, as its install; C#'s wildcard
  rules since delete-directory).

Callers checked. `archive_members` and `check_archive_sources` serve the archive
stage (the basic and full reports, the desktop Validate step) and the new gate (plain
`validate`, `validate_remaining`, `install`, `install_remaining` and the desktop
install). `conflicts` and the dry run do not run either, as before. The rootless
basic validation (`src/validation/basic.rs:568`) already reported the error
and is unchanged.

## Proof

[probe.py](loose-source-proof/probe.py) runs 18 cases through the full report with a
dry run, the basic dry-run report, `install` and, for Rust, plain `validate`, compared
with the live C# CLI with a HoloPatcher stub (126 checks).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 80 match / 46 known difference / 0 fail |
| this slice, recorded reference | 80 match / 46 known difference / 0 fail |
| `6ca4d35` (pre-fix), recorded reference | 59 match / 46 known difference / 21 fail |

On `6ca4d35` the 21 failures are the eight refused cases' plain `validate` and
`install` (Rust passed and installed the loose file; 16) and `improved_ai`'s five
report checks (Rust's reports refused it).

Other proofs changed: [extract-overwrite](extract-overwrite-proof/README.md) (its
`stale_*` cases are now refused as by the C# CLI, so they are ordinary known
differences from its InstallAsync reference; still 4 match / 16 known, 6 fail on
`6ca4d35`), and the FOMOD
[S2b](fomod-s2b-proof/README.md) and [gate scope](fomod-gate-scope-proof/README.md)
probes, whose fixtures copied a loose note in a component that extracts (C# refuses
those guides); the note now has its own component or is left out, with the same
counts (265/265, 74/74).

## Known differences

- **Counts and stages (both refuse).** C#'s basic report gives two errors per Source
  (the check and its AttemptFixes retry) and keeps going; Rust gives one error and
  ends the component's checks. C#'s full pipeline stops in its Environment stage with
  one error; Rust's fails its ComponentValidation stage. 24 checks.
- **Case.** C# matches the Source against the members case-sensitively on every
  platform, so `M/OVERRIDE/A.TGA` for `m.zip`'s `Override/a.tga` is refused; Rust
  matches as its install resolves paths (case-insensitively by default) and installs
  it. 7 checks.
- **DelDuplicate.** C# checks its Sources (extensions such as `.tga`) as paths and
  refuses every DelDuplicate in a component with an archive; Rust does not check
  them and runs it. 7 checks.
- **The doubled folder.** For `d.zip` holding `d/Override/a.tga`, C# rewrites the
  Source `d/Override/a.tga` to `d/d/Override/a.tga` (AttemptFixes) and installs it;
  Rust refuses the Source as written (fail closed, unchanged; both basic reports
  fail). 6 checks.
- **Missing archive (not this slice).** Both refuse; C#'s full pipeline stops in its
  Environment stage, Rust's reports the archive in ComponentValidation. 2 checks.
- **Not probed.** An `.exe` archive (C# matches any Source; Rust lists the archive
  and checks its members) and `install --skip-validation` (C# skips the check; Rust
  has no such flag). Messages: the error text keeps Rust's wording (the Source as
  written, no `!`). Windows: not run; nothing here depends on the host.

## Notes for guide authors

In a component that extracts an archive, name only files that archive (or another
archive the same component extracts) holds, under the folder named after it:
`<<modDirectory>>/m/...` for `m.zip`. A file another component extracts, or one a
Rename in the same component makes, is refused by both C# and Rust; move such steps
to the component that extracts the file, or rename after copying. Loose files the
user unpacks by hand belong in a component without an Extract step.
