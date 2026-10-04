# Windows device names and Peer staging folders in instruction paths

This contract covers two fail-open gaps in Rust's path rules, found while reviewing
[full-pipeline-destination-contract.md](full-pipeline-destination-contract.md): Windows
device names the reserved-name rule did not know (`COM¹`, `LPT³`, `CONIN$`,
`CONOUT$`, `COM0`, `LPT0`), and instruction paths into a Peer download staging folder
(`<<modDirectory>>/.peer-<nonce>.part/`). Both are a fail-closed hardening, not a parity
relaxation: Rust now refuses guides it used to install. The reference is pinned C#
`d31ccbd3`; C# paths are under `src/ModSync.Core/` there. Rust lines before this slice
are at `e756730`. Evidence: [reserved-staging-paths-proof](reserved-staging-paths-proof/README.md).
F05 and F08 stay partial.

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
(`FileSystemUtils/PathHelper.ArchiveExtraction.cs:19-65`). C# has no Peer staging
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

- **Device names.** `is_reserved_name` (`src/paths.rs:137-162`) refuses a path part
  whose stem (up to the first `.`, spaces before it ignored, ASCII case ignored) is
  `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, or `COM`/`LPT` followed by exactly
  one of `0`-`9`, `¹`, `²`, `³`. `checked_relative` calls it for every part
  (`:131`) with the existing wording, `Reserved filename: <path>`, so every caller of
  `relative`, `literal_relative` and `one_component` gets it on every host. Longer
  stems stay ordinary names: `COM10`, `com1x.txt`, `COM¹¹`, `LPT²x`, `CONINx`,
  `CLOCK$`.
- **Peer staging folders.** `Roots::resolve` (`src/paths.rs:195-231`) refuses a
  `<<modDirectory>>` path any of whose parts is a Peer staging name, matched with
  `download_orchestrator::is_peer_staging` on the ASCII-lower-cased part
  (`is_peer_staging_part`, `:164-174`; the check at `:210-213`), with `Path is inside a
  Peer download staging folder: <path>`. This covers the folder itself, anything under
  it, one at any depth, any case, and a wildcard part with that shape
  (`.peer-*.part`). `<<kotorDirectory>>` and `<<gameDirectory>>` paths are not
  checked: staging folders live only in the mod folder and the game scan does not skip
  them.
- Both refusals come from path resolution, so they reach plain `validate`, the
  install preflight (before any write), the dry run, the basic and full reports and
  the desktop Validate step with the step's usual prefix (`A / step 1 (Copy): ...`;
  the archive stage shows `archive/destination validation: ...`).

Callers checked. `Roots::resolve` has two callers: the engine's `path`
(`src/engine.rs:2077-2088`), through which every instruction Source, Destination and
placeholder Rename target resolves in validation, simulation and install, and the FOMOD
Extract source lookup (`src/fomod/resolved_choices.rs:120`). Peer and download code never
calls it: staging folders are created and published by `PeerStaging`
(`src/download_orchestrator.rs:2095-2170`), which joins paths itself, and download
names go through `download::one_component` (`src/download.rs:710-718`), which the
staging rule does not touch; the download-residuals (183) and fetch-shared-url (382)
probes, which drive Peer and download staging, pass unchanged. ResourceRegistry
archive keys (`src/engine.rs:1228-1243`, `src/engine/source_resolution.rs:611-618`)
join the mod folder directly; they are read only, and the second must also be in the
engine scan, which hides staging folders. The device-name rule also applies to
everything else `checked_relative` serves: archive members and checkpoint tree entries
(`literal_relative`, `src/archive.rs:48`, `src/checkpoint/read.rs:167`,
`src/checkpoint/history.rs:320`), FOMOD sources and destinations (`src/fomod.rs:686-696`),
patcher INI names (`src/engine/patcher_simulation.rs:157-233`), local and resolved
archive filenames (`src/archive_generation.rs:74`,
`src/archive_generation/resolved.rs:276`), download filenames, maintenance subtrees,
the resource watcher and the editor browser. Each refuses only the exact device stems
above; the probe's `com1x.txt` archive member and Source still install.

## Proof

[probe.py](reserved-staging-paths-proof/probe.py) runs 35 cases (14 device names, 8
staging paths, 13 controls) through plain `validate`, the basic and full reports with
and without a dry run, and `install`, and compares the full dry-run decision and the
install against the live C# CLI with a HoloPatcher stub, 267 checks.

| Binary | Result |
| --- | --- |
| this slice, recorded and live reference | 227 match / 40 known difference / 0 fail |
| `e756730` (pre-fix), live reference | 106 match / 0 known difference / 161 fail |

On `e756730` every refused case passed at least one mode and carried no refusal; the
device-name cases and the Destinations into a staging folder were installed. The
controls matched on both builds.

## Known differences

- **Device names C# does not list (deliberate, fail closed).** `COM¹`-`COM³`,
  `LPT¹`-`LPT³`, `CONIN$`, `CONOUT$` and a stem with spaces before the extension are
  refused in Rust on every host, as Source, Destination, Rename name or archive member;
  C# accepts and installs them (on Windows they would name a device). 12 cases, 24
  checks.
- **Peer staging folders (Rust only).** C# has no Peer downloads and installs into or
  from `.peer-<nonce>.part` like any folder; Rust refuses those paths. 8 cases, 16
  checks.
- **Stem boundary.** Rust's stem ends at the first `.` (`CON.tar.gz` is refused), as
  Windows reads it; C#'s ends at the last (`CON.tar.gz` passes). Predates this slice;
  not probed.
- **`CLOCK$` and longer names.** `CLOCK$` was a DOS device but is not one on Windows NT,
  and Microsoft does not list it, so neither side refuses it. `COM10` and `com1x` are
  ordinary on both.
- **A bare Rename name shaped like a staging folder.** `Rename` to `.peer-<nonce>.part`
  without a placeholder is not refused: it makes a file, which the scan does not skip.
  With `<<modDirectory>>/...` it is refused.
- **Platform.** The reference ran on Linux under `unshare -Urnm` with a tmpfs over
  `/Override`; Windows device behaviour itself is not probed.
