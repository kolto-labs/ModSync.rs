# Destination path parts proof

Contract: [destination-path-parts-contract.md](../destination-path-parts-contract.md).

[probe.py](probe.py) builds 25 disposable fixtures. Each has `game/{chitin.key, r.txt,
Override/x.tga, Override/x.tpc}` and `mods/{m.txt, n.txt, l.csv}` (`l.csv` lists
`r.txt`); the `dotdot_existing`, `dotdot_cleanlist` and `dotdot_deldup` cases add
`game/a..b/{r.txt, x.tga, x.tpc}`, and `tab_existing` adds the same under a folder
named `t<TAB>b`. The TOML guide copies `m.txt` (or moves it, or runs a CleanList with
`l.csv` or a DelDuplicate of `.tga`) into the case's Destination:

- controls: `KD/Override`, `KD/Override/.x`, `KD/Override/a.b`, and a `.` part in a
  component that is not selected;
- a `.` part: `KD/Override/.`, `KD/Nope/.`, `KD/.`, `KD/./Override`,
  `KD\Override\.`, `MD/.` (copying `KD/r.txt`), a Move, a selected option's step, an
  unselected option's step, a step whose Dependencies name an unselected component,
  a second step after a good one, a second component;
- `..` inside a name: `KD/a..b` (existing), `KD/Over..ride`, `KD/Override/..x`, and a
  CleanList and a DelDuplicate into `KD/a..b`;
- a tab: `KD/Override/a<TAB>b` (missing) and `KD/t<TAB>b` (existing);
- a CleanList and a DelDuplicate into `KD/Override/.` (refused by both before).

(`KD` is `<<kotorDirectory>>`, `MD` `<<modDirectory>>`.) Every run gets
`--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`;
- `full`: the same with `--full` only;
- `basic`: `validate ... --output json --dry-run`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it runs its full pipeline first;
- `install_skip` (C# only): `install ... -y --skip-validation`;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 200 in all: `full_dry/decision`, `full/decision`, `basic/decision` (exit code
zero or not, and `success`); `full_dry/counts` (`errorCount`, `warningCount`,
`passedCount`); `full_dry/stages` (each stage's name and `passed`, and the
`ComponentValidation` and `DryRun` messages with the fixture root as `<root>`);
`install` (exit code zero or not, and the files under `game/` and `mods/` afterwards,
C#'s `.modsync` folder left out); `install_skip` (Rust `install` against C# `install
--skip-validation`); `plain` (Rust's plain `validate` decides as C# `install`).

`KNOWN` lists the documented differences (62 checks). A known check passes only if
Rust differs from the reference and shows the documented Rust result (`known_ok`): a
refused install for `install_skip`, a failed `ComponentValidation` stage naming the
folder for `full_dry/stages`, and for `dot_condition_unmet` an accepted guide.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build. Its full pipeline's Environment stage needs a HoloPatcher in the build's
`Resources` folder, so the probe copies the build into its temporary directory and
adds `Resources/holopatcher`, a `#!/bin/sh` script that exits 2, as
[full-pipeline-destination-proof](../full-pipeline-destination-proof/README.md) does.
It runs under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need no network either.

C# results: the four controls pass every full report and install (`KD/Override/.x`
and `KD/Override/a.b` fail the basic report as missing folders, which the full
pipeline creates). Every other case fails every report and `install`; all but
`dot_missing` fail in the Environment stage (`errorCount` 1, `passedCount` 0), and
`dot_missing` fails `ComponentValidation` with `Got '<root>/game/Nope/.'`. `install
--skip-validation` installs the Copy and Move cases (including into
`<root>/game/Over..ride` and `Override/a<TAB>b`) and refuses the CleanList and
DelDuplicate ones.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 138 match / 62 known difference / 0 fail |
| this slice, recorded reference | 138 match / 62 known difference / 0 fail |
| `a5b2053` (pre-fix), recorded reference | 82 match / 39 known difference / 79 fail |

On `a5b2053` the 79 failures are: `install`, `install_skip` and `plain` for each of
the 11 Copy/Move `.` cases that C# refuses (Rust installed and plain `validate`
passed; 33), and for the seven `..` and tab cases also the full and dry-run
decisions, the stages and, where the basic report differs, the basic decision (Rust
passed every report; 46). The live reference was recorded once with the pre-fix
binary and once with the fixed one; both gave the same `csharp-oracle.json`.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/destination-path-parts-proof/probe.py                       # recorded reference
python3 docs/parity/destination-path-parts-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of Rust's plain `validate` and `install` output per case. `MODSYNC_PROOF_OUT`,
when set, is the parent of the probe's temporary directory.
