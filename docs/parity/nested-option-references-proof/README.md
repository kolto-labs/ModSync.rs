# Nested option references proof

Contract: [nested-option-references-contract.md](../nested-option-references-contract.md).

[probe.py](probe.py) builds 10 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/{a/x.txt, a/y.txt, b/x.txt, b.txt, c.txt}`. Component `A` copies
`a/x.txt` into `Override` (or, where stated, has only a Choose of `O`) and has a selected
option `O` (`A option`), which has an option `N` (`Nested`, selected unless stated)
of its own (`[[thisMod.Options.Options]]`). Component `B` copies `b/x.txt` into
`Override` and `b.txt` into the game folder. Every Copy has `Overwrite = true`.

- a Restriction of `B` naming `N` (`restriction_nested`), and naming `N` when `N` is
  not selected (`restriction_nested_unselected`);
- a third step of `B` copying `c.txt` into the game folder whose Dependencies name `N`
  (`step_dependency_nested`) or whose Restrictions name `N`
  (`step_restriction_nested`), and the same naming `O` (`step_dependency_option`,
  `step_restriction_option`, controls);
- `A`'s only step is a Choose of `O`, and `O`'s step copies `a/y.txt` into `Override`
  with Dependencies (`option_step_dependency_nested`) or Restrictions
  (`option_step_restriction_nested`) naming `N`;
- unchanged known differences: a Restriction of `B` naming `O`, where `A` chooses `O`
  and `O` copies `a/y.txt` (`restriction_option`); and `A` chooses `O`, whose step is a
  Choose of `N`, which copies `a/y.txt` (`nested_chosen`).

Every run gets `--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`;
- `basic`: `validate ... --output json --dry-run`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it runs its full pipeline first;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 70 in all: `full_dry/decision` and `basic/decision` (exit code zero or not, and
`success`); `full_dry/counts` (`errorCount`, `warningCount`, `passedCount`);
`full_dry/stages` (each stage's name and `passed`); `full_dry/conflicts` (the
`Conflicts` stage's messages); `install` (exit code zero or not, and every file under
`game/` and `mods/` with its text afterwards, C#'s `.modsync` folder left out); `plain`
(Rust's plain `validate` decides as C# `install`).

`KNOWN` lists the documented differences (8 checks: all seven of `restriction_option`,
where Rust must refuse before any write, and the install of `nested_chosen`, where
Rust must install C#'s files plus `Override/y.txt`).

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build. Its full pipeline's Environment stage needs a HoloPatcher in the build's
`Resources` folder, so the probe copies the build into its temporary directory and
adds `Resources/holopatcher`, a `#!/bin/sh` script that exits 2, as
[full-pipeline-destination-proof](../full-pipeline-destination-proof/README.md) does.
It runs under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need no network either.

C# results: every case passes every report (`errorCount` 0, all six stages passed, no
Conflicts message) and installs `Override/x.txt` (`from B`) and `b.txt`. `c.txt` is
installed only in `step_dependency_option` and `step_restriction_nested`, and
`Override/y.txt` only in `option_step_restriction_nested`: a GUID naming `N` names
nothing for C#. In `restriction_option` C# deselects `O` while loading the guide
(AttemptFixes), so `A`'s Choose runs nothing and `y.txt` is not installed; in
`nested_chosen` `O`'s Choose finds no option.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 62 match / 8 known difference / 0 fail |
| this slice, recorded reference | 62 match / 8 known difference / 0 fail |
| `c386d33` (pre-fix), recorded reference | 51 match / 8 known difference / 11 fail |

On `c386d33` the 11 failures are all seven checks of `restriction_nested` (Rust failed
the `Conflicts`, `FomodConfiguration` and `DryRun` stages with `B conflicts with <N>`
and refused plain `validate` and `install`) and the install of the four
step-condition cases naming `N` (Rust ran the steps C# skips and skipped the ones C#
runs).

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/nested-option-references-proof/probe.py                       # recorded reference
python3 docs/parity/nested-option-references-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of Rust's plain `validate` and `install` output per case. `MODSYNC_PROOF_OUT`,
when set, is the parent of the probe's temporary directory.
