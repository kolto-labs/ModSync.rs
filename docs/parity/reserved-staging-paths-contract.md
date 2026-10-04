# Windows device names and Peer staging folders in instruction paths

This contract covers two fail-open gaps in Rust's path rules, found while reviewing
[full-pipeline-destination-contract.md](full-pipeline-destination-contract.md): Windows
device names the reserved-name rule did not know (`COM¹`, `LPT³`, `CONIN$`, `CONOUT$`,
`COM0`, `LPT0`), and instruction paths into a Peer download staging folder
(`<<modDirectory>>/.peer-<nonce>.part/`). Both are a fail-closed hardening, not a parity
relaxation: Rust now refuses guides it used to install. The reference is pinned C#
`d31ccbd3`; C# paths are under `src/ModSync.Core/` there. Rust lines before this slice
are at `e756730`. Evidence:
[reserved-staging-paths-proof](reserved-staging-paths-proof/README.md). F05 and F08 stay
partial.

## Triage

**Rust at `e756730`, device names.** Every guide path (`paths::relative`, Source,
Destination, a bare Rename name, CleanList rows, FOMOD destinations), every literal path
(`paths::literal_relative`: archive members, checkpoint tree entries, FOMOD sources,
patcher INI names) and every download filename (`download::one_component`) passes
`checked_relative` (`src/paths.rs:116-142`). Its reserved-name check took the part up
to the first `.` and refused `CON`, `PRN`, `AUX`, `NUL` and `COM`/`LPT` with one digit
`1` to `9` (`src/paths.rs:131-137`), on every host so a guide cannot change meaning
across platforms (`:97`). It let through `COM¹`, `COM²`, `COM³`, `LPT¹`, `LPT²`,
`LPT³` (Windows reads the superscripts as device numbers), `CONIN$` and `CONOUT$`
(the console devices), `COM0` and `LPT0` (listed as reserved by Microsoft and by C#),
and `CON .txt` (Windows ignores spaces before the extension). Plain `validate`, every
report and `install` passed such a guide; on Linux `install` created
`game/COM¹/m.txt`, and on Windows the same step would open a device instead.
Checkpoint stores used the same rule in both directions: a capture scanned both
folders and checked every entry with `literal_relative` through `validate_tree`
(`src/checkpoint/store.rs:376-425`, `src/checkpoint/read.rs:157-167`), and every read
of a store, its history or a maintenance journal checked the recorded names again
(`src/checkpoint/read.rs:415-416`, `src/checkpoint/lifecycle.rs:1036-1046`,
`src/checkpoint/history.rs:320`, `src/maintenance/journal.rs:214`, `:255`,
`src/maintenance/fence.rs:139`). So a game folder holding `Override/COM1` could not be
captured, while one holding `Override/COM0` or `Override/CONIN$` was captured and
recorded.

**Rust at `e756730`, Peer staging folders.** A Peer download stages each copy in
`<mod directory>/.peer-<nonce>.part/` until the race accepts it
(`PeerStaging::create`, `src/download_orchestrator.rs:2096-2112`, the name rule
`is_peer_staging` at `:2026-2037`); empty old ones are swept (`:2042-2081`), nonempty
ones are kept for inspection. The engine scan skips these folders under the mod folder
(`src/engine.rs:470-475`), as do the FOMOD choice inventory
(`src/fomod/resolved_choices.rs:41-46`) and download planning
(`src/download_plan.rs:199-206`), so their contents never count as present. But
`Roots::resolve` (`src/paths.rs:163-195`) did not know the name, so a Destination
inside one resolved: plain `validate` and both full reports passed (the folder is
creatable in the full pipeline), and `install` copied into the staging folder, where
nothing the guide wrote is visible to later validation and a Peer race may still be
writing. The basic report refused it only as a missing Destination. A Source or a
wildcard Source inside one was refused by plain `validate` and `install` as missing,
but the dry-run reports passed it. An upper-case spelling (`.PEER-<NONCE>.PART`) was
not skipped by the scan at all; on a case-insensitive volume it is the same folder.

**C# at `d31ccbd3`.** `PathValidator.IsValidPath` refuses a path when any part's
`Path.GetFileNameWithoutExtension` equals, ignoring case, one of `CON`, `PRN`, `AUX`,
`NUL`, `COM0`-`COM9`, `LPT0`-`LPT9` (`FileSystemUtils/PathValidator.cs:24-28`,
`:71-81`, `:147-164`). It does not list the superscript forms, `CONIN$` or `CONOUT$`,
it takes the stem up to the *last* dot (`CON.tar.gz` passes) and it keeps spaces before
the dot (`CON .txt` passes). `ComponentValidation` refuses a Destination that fails it
(`ComponentValidation.cs:343-349`), so `COM0` and `LPT0` fail C#'s validation and its
`install`. Archive members are only checked for traversal
(`FileSystemUtils/PathHelper.ArchiveExtraction.cs:19-66`). C# has no Peer staging
folders, so it reads and writes `.peer-<nonce>.part` like any other folder.

**User-visible difference** (Copy `<<modDirectory>>/m.txt` into each Destination):

```
Destination                         C# install   Rust before   Rust after
<<kotorDirectory>>/COM¹             installs     installs      refused, no write
<<kotorDirectory>>/CONIN$           installs     installs      refused, no write
<<kotorDirectory>>/COM0             refused      installs      refused, no write
<<modDirectory>>/.peer-<nonce>.part installs     installs      refused, no write
<<kotorDirectory>>/COM10, CLOCK$    installs     installs      installs
```

## Behaviour after this slice

- **Device names.** `is_reserved_name` (`src/paths.rs:150-175`) refuses a path part
  whose stem (up to the first `.`, spaces before it ignored, ASCII case ignored) is
  `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, or `COM`/`LPT` followed by exactly
  one of `0`-`9`, `¹`, `²`, `³`. `checked_relative` (`:129-148`) applies it to every
  part (`:144`) with the existing wording, `Reserved filename: <path>`, so every
  caller of `relative`, `literal_relative` and `one_component` gets it on every host.
  Longer stems stay ordinary names: `COM10`, `com1x.txt`, `COM¹¹`, `LPT²x`, `CONINx`,
  `CLOCK$`.
- **Stored state keeps the older rule.** Reading what Rust stored earlier uses
  `stored_relative` (`src/paths.rs:115-127`), which applies the rule as it was before
  this slice (`legacy_reserved_name`, `:177-190`: `CON`, `PRN`, `AUX`, `NUL`,
  `COM1`-`COM9`, `LPT1`-`LPT9`): checkpoint trees (`validate_tree`,
  `src/checkpoint/read.rs:169-179`, used by every store read, `lifecycle.rs` and the
  auxiliary snapshots), checkpoint provenance paths (`src/checkpoint/history.rs:320`),
  and maintenance journals and fences (`src/maintenance/journal.rs:214`, `:255`,
  `src/maintenance/fence.rs:139`). A store recording `Override/COM0` or
  `Override/CONIN$` stays listable, verifiable, restorable and recoverable.
- **New checkpoint sessions refuse device names, with context.** A new session
  (`checkpoint capture`, `install --component-checkpoints`, `--managed`) checks both
  scanned folders, and directory participants, with the wider rule before any content
  is stored, and stops with `Checkpoint cannot record <path>: Windows device name`
  (`refuse_device_names`, `src/checkpoint/read.rs:157-167`; called from
  `capture_trees` at `src/checkpoint/store.rs:352-355` and `capture_all` at
  `src/checkpoint/auxiliary.rs:206-212`). The safety session a restore or a recovery
  takes first (`scope` `safety`, `src/checkpoint/store.rs:205-213`), component commits
  (`src/checkpoint/journal.rs`) and resumes (`src/checkpoint/resume.rs`) extend or
  protect existing state and keep the older rule, so restoring a store that recorded
  such a file, and recovering after it, still work. Maintenance inventories and the
  managed preimage already scan with `relative`/`literal_relative`
  (`src/maintenance/plan.rs:402`, `src/engine/managed.rs:171`) and so refuse with
  `Reserved filename`.
- **Peer staging folders.** `Roots::resolve` (`src/paths.rs:223-259`) refuses a
  `<<modDirectory>>` path any of whose parts is a Peer staging name, matched with
  `download_orchestrator::is_peer_staging` on the ASCII-lower-cased part
  (`is_peer_staging_part`, `:192-202`; the check at `:238-241`), with `Path is inside
  a Peer download staging folder: <path>`. This covers the folder itself, anything
  under it, one at any depth, any case, a wildcard part with that shape
  (`.peer-*.part`), and a file whose name has that shape (Known differences).
  `<<kotorDirectory>>` and `<<gameDirectory>>` paths are not checked: staging folders
  live only in the mod folder and the game scan does not skip them.
- Both instruction-path refusals come from path resolution, so they reach plain
  `validate`, the install preflight (before any write), the dry run, the basic and
  full reports and the desktop Validate step with the step's usual prefix (`A / step 1
  (Copy): ...`; the archive stage shows `archive/destination validation: ...`).

Callers checked. `Roots::resolve` has two callers: the engine's `path`
(`src/engine.rs:2077-2088`), through which every instruction Source, Destination and
placeholder Rename target resolves in validation, simulation and install, and the FOMOD
Extract source lookup (`src/fomod/resolved_choices.rs:120`). Peer and download code
never calls it: staging folders are created and published by `PeerStaging`
(`src/download_orchestrator.rs:2095-2170`), which joins paths itself, and download names
go through `download::one_component` (`src/download.rs:710-718`), which the staging rule
does not touch; the download-residuals (183) and fetch-shared-url (382) probes, which
drive Peer and download staging, pass unchanged. ResourceRegistry archive keys
(`src/engine.rs:1228-1243`, `src/engine/source_resolution.rs:611-618`) join the mod
folder directly; they are read only, and the second must also be in the engine scan,
which hides staging folders. The wider device-name rule applies to everything else
`relative`, `literal_relative` and `one_component` serve: archive members
(`src/archive.rs:48`), FOMOD sources and destinations (`src/fomod.rs:686-696`), patcher
INI names (`src/engine/patcher_simulation.rs:157-233`), local and resolved archive
filenames (`src/archive_generation.rs:74`, `src/archive_generation/resolved.rs:276`),
download filenames, new maintenance requests and inventories
(`src/maintenance/plan.rs:176`, `:402`), the managed preimage
(`src/engine/managed.rs:171`, `:209`), the resource watcher and the editor browser. Each
refuses only the exact device stems above; the probe's `com1x.txt` archive member and
Source still install.

## Proof

[probe.py](reserved-staging-paths-proof/probe.py) runs 36 instruction cases (14 device
names, 9 staging paths, 13 controls) through plain `validate`, the basic and full
reports with and without a dry run, and `install`, and compares the full dry-run
decision and the install against the live C# CLI with a HoloPatcher stub (275 checks).
It then runs 13 Rust-only checkpoint checks: a store written by an `e756730` build
(`--writer-bin`) while the game held `Override/COM0`, `Override/CONIN$` and
`Override/COM¹` is listed, verified, inspected, queried for provenance, restored (the
files come back) and recovered; and with `Override/CONIN$` in the game folder,
`install --component-checkpoints` and `checkpoint capture` refuse with the new message
and leave the game and mod folders unchanged, `modsync restore` clears the whole-run
snapshot taken before the refusal, and an install without checkpoints still works.
288 checks in all.

| Binary | Result |
| --- | --- |
| this slice, recorded and live reference | 246 match / 42 known difference / 0 fail |
| `81003bd` (first version of this slice), recorded reference | 236 match / 42 known difference / 10 fail |
| `e756730` (pre-fix), recorded reference | 116 match / 0 known difference / 172 fail |

On `e756730` every refused instruction case passed at least one mode and carried no
refusal; the device-name cases, the Destinations into a staging folder and the file
named like one were installed. Its checkpoint captures recorded `Override/CONIN$`
(three failures); the legacy store checks pass there, as that build wrote the store.
`81003bd` refused to read that store at all (`Reserved filename: Override/COM0` from
`list`, `verify`, `history`, `inspect`, `provenance`, `restore`, `recover` and the
second `list`, eight failures) and refused the new captures without context (two).
The controls matched on every build.

## Known differences

- **Device names C# does not list (deliberate, fail closed).** `COM¹`-`COM³`,
  `LPT¹`-`LPT³`, `CONIN$`, `CONOUT$` and a stem with spaces before the extension are
  refused in Rust on every host, as Source, Destination, Rename name or archive
  member; C# accepts and installs them (on Windows they would name a device). 12
  cases, 24 checks.
- **Peer staging folders (Rust only).** C# has no Peer downloads and installs into or
  from `.peer-<nonce>.part` like any folder; Rust refuses those paths. 8 cases, 16
  checks.
- **A file named like a staging folder (fail closed).** The scan hides only folders,
  but the rule matches the name, so a placeholder path to a *file* such as
  `<<modDirectory>>/.peer-notes.part` is refused too; C# copies it. One case, two
  checks.
- **Checkpoints and a device name already on disk (deliberate).** A new checkpoint
  session (`checkpoint capture`, `install --component-checkpoints`, `--managed`) and a
  maintenance run refuse a game or mod folder that holds a file or folder with a
  device name, anywhere, even one the guide never touches. For an install the refusal
  comes after the whole-run recovery snapshot and before any game or mod write, so
  `modsync restore` must clear that snapshot before the next install, as it already
  had to for `COM1`. Rename or remove the file, or install without checkpoints. Stores
  written before stay readable, restorable and recoverable, and restore and recovery
  may put such a file back. Not compared with C#.
- **Stem boundary.** Rust's stem ends at the first `.` (`CON.tar.gz` is refused), as
  Windows reads it; C#'s ends at the last (`CON.tar.gz` passes). Predates this slice;
  not probed.
- **`CLOCK$` and longer names.** `CLOCK$` was a DOS device but is not one on Windows
  NT, and Microsoft does not list it, so neither side refuses it. `COM10` and `com1x`
  are ordinary on both.
- **A bare Rename name shaped like a staging folder.** `Rename` to
  `.peer-<nonce>.part` without a placeholder is not refused, since a bare name is not
  resolved under `<<modDirectory>>`; written as `<<modDirectory>>/...` the same target
  is refused, as any placeholder path of that shape is.
- **Platform.** The reference ran on Linux under `unshare -Urnm` with a tmpfs over
  `/Override`; Windows device behaviour itself is not probed.