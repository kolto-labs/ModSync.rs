# Loose Source proof

Contract: [loose-source-contract.md](../loose-source-contract.md).

[probe.py](probe.py) builds 18 disposable fixtures. Each has `game/{chitin.key,
Override/old.tga}` and `mods/{m.zip, n.zip, d.zip, b.txt, loose/x.tga}`; `m.zip`
holds `Override/a.tga` and `readme.txt`, `n.zip` holds `n.txt` and `d.zip` holds
`d/Override/a.tga`. Component `A` has the case's steps (`OV` is
`<<kotorDirectory>>/Override`; every Move and Copy has `Overwrite = true`); component
`B` copies the loose `b.txt` into `Override` with no Extract.

- refused by the C# CLI (a Source in the mod folder that no archive of the component
  holds, next to an Extract of `m.zip`): `Copy loose/x.tga` (`loose_copy`), `Move
  loose/*` (`loose_wildcard`), `Delete loose/x.tga` then `Move m/Override/*`
  (`loose_delete`), the Copy in an option `O` that `A` chooses
  (`selected_option_loose`) or in an unchosen `O` after `Move m/Override/*`
  (`unselected_option_loose`), the Copy in `A` with the Extract only in an unchosen
  `O` (`unselected_option_extract`), `Rename m/readme.txt` to `notes.txt` then `Copy
  m/notes.txt` (`renamed_source`), and `A` extracting `n.zip` and copying
  `m/Override/a.tga`, which a component `M` before it extracts
  (`other_component_archive`);
- installed by both: `Copy m/Override/a.tga` (`member_copy`), `Move m/Override/*`
  (`member_wildcard`), `Delete OV/old.tga` then the Move (`game_source`), `Copy
  loose/x.tga` and `Delete loose/x.tga` with no Extract (`no_extract`), the loose Copy
  with `Dependencies` on an unselected component (`condition_unmet`), and the loose
  Copy in a component named `Improved AI` (`improved_ai`, which C# exempts);
- refused by both: an Extract of a missing `gone.zip` and the loose Copy
  (`missing_archive`);
- known differences: `Copy M/OVERRIDE/A.TGA` (`member_case`), `Move m/Override/*` then
  `DelDuplicate [".tga", ".tpc"] -> OV` (`delduplicate`), and `Extract d.zip` then
  `Copy d/Override/a.tga` (`doubled_folder`).

Every run gets `--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`;
- `basic`: `validate ... --output json --dry-run`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it runs its full pipeline first;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 126 in all: `full_dry/decision` and `basic/decision` (exit code zero or not,
and `success`); `full_dry/counts` and `basic/counts` (`errorCount`, `warningCount`,
`passedCount`); `full_dry/stages` (each stage's name and `passed`); `install` (exit
code zero or not, and every file under `game/` and `mods/` with its text afterwards,
an archive as `<archive>`, C#'s `.modsync` folder left out); `plain` (Rust's plain
`validate` decides as C# `install`).

`KNOWN` lists the documented differences (46 checks):

- `full_dry/counts`, `full_dry/stages` and `basic/counts` of the eight refused cases
  and the first two of `missing_archive` (26), which pass only if both Rust reports
  fail and, for the counts, differ: C# gives two errors per Source in its basic
  report and stops its full pipeline in the Environment stage;
- every check of `member_case` and `delduplicate` (14), which C# refuses and Rust
  accepts: they pass only if Rust's result equals the C# result of `member_copy` and
  `member_wildcard` (the same files installed);
- every check of `doubled_folder` but `basic/decision` (6), which C# rewrites and
  installs: they pass only if Rust refuses (both reports fail; plain `validate` and
  `install` fail with the files unchanged).

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy with a `Resources/holopatcher` stub (a `#!/bin/sh` script that
exits 2) under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`, as
[extract-destination-proof](../extract-destination-proof/README.md) does. Its results
are in [csharp-oracle.json](csharp-oracle.json). The Rust runs need no network or
.NET.

C# results: the eight refused cases, `member_case` and `delduplicate` fail the full
report in its Environment stage (`errorCount` 1, one stage) and the basic report in
ComponentValidation (`errorCount` 2, 4 for `delduplicate`'s two extensions), and
`install` stops before any write. `missing_archive` fails both (basic `errorCount` 1).
`doubled_folder` passes the full report and installs `game/Override/a.tga` from
`mods/d/d/Override/a.tga`, but fails the basic report with one error. The other cases
pass everything and install.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 80 match / 46 known difference / 0 fail |
| this slice, recorded reference | 80 match / 46 known difference / 0 fail |
| `6ca4d35` (pre-fix), recorded reference | 59 match / 46 known difference / 21 fail |

The 21 failures on `6ca4d35` are listed in the contract. The C# oracle was first
recorded live with the pre-fix binary; the recording made with the final binary was
identical.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/loose-source-proof/probe.py                       # recorded reference
python3 docs/parity/loose-source-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of each Rust plain `validate` and `install` (and C# install) output.
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
