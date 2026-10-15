# Reserved names and Peer staging folders proof

Contract: [reserved-staging-paths-contract.md](../reserved-staging-paths-contract.md).

[probe.py](probe.py) builds 36 disposable instruction fixtures. Each has `game/{chitin.key,
Override/}`, `mods/{m.txt, sub/}` and a Peer staging folder
`mods/.peer-0123456789abcdef0123456789abcdef.part/{s.txt, s.zip}` (what an unaccepted
Peer download leaves), plus a TOML guide with one component and one step. Every
placeholder Destination folder exists in the fixture, so a control passes every report
mode and a refusal comes from the name alone. The cases:

- 14 Windows device names that must be refused: `COM¹`, `COM²` (under `Override`),
  `com³.d`, `LPT¹`, `LPT²` (under the mod folder), `LPT³.txt`, `CONIN$`,
  `conout$.txt`, `COM0`, `lpt0.d` and `CON .d` (a space before the extension) as Copy
  Destinations; `COM¹.txt` as a Copy Source; `LPT².txt` as a bare Rename name;
  `CONOUT$.txt` as an archive member of an Extract;
- 9 staging paths that must be refused: the folder as a Copy and a Move Destination, a
  subfolder of it, its upper-case spelling, one under `<<modDirectory>>/sub/`, a Copy
  Source inside it, a wildcard Source `<<modDirectory>>/.peer-*.part/*.txt`, an Extract
  of an archive inside it, and a Copy Source that is a *file* named
  `.peer-notes.part` (fail closed: the scan hides only folders);
- 13 controls that must keep working: `Override`, `COM10`, `com1x`, `COM¹¹`, `LPT²x`,
  `CONINx` and `CLOCK$` as Destinations, `com1x.txt` as an archive member and as a
  Source, and `.peer-<nonce>.part` under the game folder, `peer-<nonce>.part` (no
  dot), `.peer-<nonce>.partial` and `.peer-.part` (no nonce) under the mod folder.

Every Rust run gets a fresh copy and `--use-file-selection`: plain `validate`,
`validate --output json --dry-run` (`basic`), `--output json --full --dry-run`
(`full_dry`, the desktop Validate step), `--output json --full` (`full`), and `install
--no-checkpoint`. Checks, 275 for these cases:

- `plain`, `basic`, `full_dry`, `full`: the decision (exit code zero or not) is the
  expected one: refused, or passed for a control;
- `install`: a refused case exits non-zero and leaves every file of the fixture as it
  was (no write); a control installs exactly the files the reference installs;
- `wording` (refused cases): plain `validate`, the `full_dry` report and `install` all
  carry `Reserved filename` or `Peer download staging folder`;
- `reference/full_dry`, `reference/install`: Rust's `full_dry` decision, and its
  install decision and files, against the pinned C# CLI.

`KNOWN` lists the deliberate differences: the 12 device-name cases C# accepts and the
9 staging cases (C# has no Peer staging folders), two reference checks each, 42 in all.
A known check passes only if Rust differs from the reference and all of that case's
own checks pass.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run with `validate ... --output json --full --dry-run` and `install ...
--no-checkpoint -y` (no `--skip-validation`, so it validates first). As in the
[full pipeline destination proof](../full-pipeline-destination-proof/README.md), the
probe copies the build into its temporary directory and adds a
`Resources/holopatcher` stub that exits 2, and runs it under `unshare -Urnm` (no
network, a tmpfs over `/Override` when that folder exists) with
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
C# refuses `COM0` and `lpt0.d` (its `PathValidator` lists them) and accepts and
installs every other case, including the staging ones.

Then 13 Rust-only checkpoint checks, each in its own fixture:

- `legacy_store/*` (9): `--writer-bin`, a build from before this slice (`e756730`),
  runs `checkpoint capture` while `game/Override` holds `COM0`, `CONIN$` and `COM¹`.
  The binary under test must then `list`, `verify`, `history`, `inspect` and
  `provenance --path Override/COM0` that store; `restore` its checkpoint after
  `game/new.txt` is added (exit 0, the three files back, `new.txt` gone); `recover`
  (exit 0, no `recoveryError`; recovery restores the unfinished restored session with
  the device names on disk); and `list` again. Without `--writer-bin` these are
  skipped and printed as `skip`.
- `new_capture/*` (4): with `game/Override/CONIN$`, `install --component-checkpoints`
  and `checkpoint capture` must fail with `Checkpoint cannot record Override/CONIN$:
  Windows device name` and leave `game/` and `mods/` unchanged; after the install's
  refusal `modsync restore` must clear the whole-run snapshot it took first, and an
  install without checkpoints must then install `Override/m.txt`.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, recorded reference | 246 match / 42 known difference / 0 fail |
| this slice, live reference (`--reference ... --record`) | 246 match / 42 known difference / 0 fail |
| `81003bd` (first version of this slice), recorded reference | 236 match / 42 known difference / 10 fail |
| `e756730` (pre-fix), recorded reference | 116 match / 0 known difference / 172 fail |

All runs used `--writer-bin` set to an `e756730` release build.

On `e756730` every device-name case failed eight checks: all five modes passed and
`install` wrote the file (`game/COM¹/m.txt`, `mods/dev/CONOUT$.txt`, ...), so no
refusal was carried, and Rust's own checks failing turned both reference checks into
failures too (for `COM0` and `lpt0.d` the reference refuses, so they differed from C#
as well). The staging Destinations (folder, subfolder, nested, Move) failed seven: the
basic report already refused them as a missing Destination (the scan hides the
folder), but plain `validate`, both full reports and `install` passed and installed
into the staging folder; the upper-case spelling, which the scan does not hide on
Linux, and the file named like a staging folder failed all eight. The staging Source
and wildcard cases failed five: plain `validate` and `install` already refused them as
missing Sources, but the basic and full dry-run reports passed. The staging Extract was
already refused everywhere as a missing archive, with other wording (three failures).
Its checkpoint capture and component-checkpoint install recorded `Override/CONIN$`
(three failures). The 13 instruction controls and the legacy store checks passed.

On `81003bd` the instruction cases matched as now, but the wider rule also read stored
checkpoint trees: `list`, `verify`, `history`, `inspect`, `provenance`, `restore`,
`recover` and the second `list` of the `e756730` store failed with `Reserved filename:
Override/COM0` (eight), and the new captures refused with that bare message (two).

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/reserved-staging-paths-proof/probe.py --writer-bin <old>                       # recorded reference
python3 docs/parity/reserved-staging-paths-proof/probe.py --writer-bin <old> --reference <ref> [--record]   # live
```

`<old>` is a release build of `e756730` (or any build from before this slice), used
only to write the legacy checkpoint store. `<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`). `MODSYNC_PROOF_OUT`, when
set, is the parent of the probe's temporary directory.
