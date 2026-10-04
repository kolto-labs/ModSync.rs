# Option Dependencies proof

Contract: [option-dependencies-contract.md](../option-dependencies-contract.md).

[probe.py](probe.py) builds 10 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/{a/x.txt, a/y.txt, b/x.txt, b.txt, c.txt}`. Component `A` copies
`a/x.txt` into `Override` and has an option `O` (`A option`); component `B` copies
`b/x.txt` into `Override` and `b.txt` into the game folder, and has the case's
`Dependencies`. Every Copy has `Overwrite = true`, so `Override/x.txt` shows which of
`A` and `B` installed last.

- control: `B` depends on `A` (a component);
- `O` and `A` selected: `B` depends on `O` (`option_selected`); the same with `B`
  listed before `A` (`option_listed_first`); with the GUID braced and upper case
  (`option_braced_upper`); on `A` and `O` (`option_and_component`); on the second of
  two options, the first not selected (`option_second_of_two`); on `O` when `A`'s only
  step is a Choose of `O`, whose step does the copy (`option_chosen_step`); and `C`
  (copies `c.txt`) depends on an option of `B`, which depends on `O` (`option_chain`);
- unchanged refusals: `O` not selected (`option_unselected`); `O` selected but `A` not
  (`option_component_unselected`).

Both loaders drop an option whose instructions equal an earlier sibling's, so the two
options in `option_second_of_two` copy different files (`a/x.txt` and `a/y.txt`; they
never run, as nothing chooses them).

Every run gets `--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`;
- `basic`: `validate ... --output json --dry-run`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it runs its full pipeline first;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 60 in all: `full_dry/decision` and `basic/decision` (exit code zero or not, and
`success`); `full_dry/counts` (`errorCount`, `warningCount`, `passedCount`);
`full_dry/stages` (each stage's name and `passed`); `install` (exit code zero or not,
and every file under `game/` and `mods/` with its text afterwards, C#'s `.modsync`
folder left out); `plain` (Rust's plain `validate` decides as C# `install`).

`KNOWN` lists the documented differences (12 checks, all six checks of the two
unchanged refusals). A known check passes only if Rust differs from the reference and
refuses: plain `validate`, the full report and `install` fail, and the files are
unchanged.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build. Its full pipeline's Environment stage needs a HoloPatcher in the build's
`Resources` folder, so the probe copies the build into its temporary directory and
adds `Resources/holopatcher`, a `#!/bin/sh` script that exits 2, as
[full-pipeline-destination-proof](../full-pipeline-destination-proof/README.md) does.
It runs under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need no network either.

C# results: every case passes every report (`errorCount` 0, all six stages passed)
and installs. `Override/x.txt` is `from B` except in `option_listed_first`, where `B`
installs first and `A` overwrites it (a Dependency on an option adds no ordering edge).
In the two refusal cases C# installs `B` too (and `A` where it is selected).

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 48 match / 12 known difference / 0 fail |
| this slice, recorded reference | 48 match / 12 known difference / 0 fail |
| `0bc0efa` (pre-fix), recorded reference | 6 match / 12 known difference / 42 fail |

On `0bc0efa` the 42 failures are all six checks of the seven selected-option cases:
Rust refused the full and basic reports (`InstallOrder`, `FomodConfiguration` and
`DryRun` failed with `B requires selected component <O>`), plain `validate` and
`install`. The live reference was recorded once with the pre-fix binary and once with
the fixed one; both gave the same `csharp-oracle.json`.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/option-dependencies-proof/probe.py                       # recorded reference
python3 docs/parity/option-dependencies-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of Rust's plain `validate` and `install` output per case. `MODSYNC_PROOF_OUT`,
when set, is the parent of the probe's temporary directory.
