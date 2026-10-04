# Full pipeline destination proof

Contract: [full-pipeline-destination-contract.md](../full-pipeline-destination-contract.md).

[probe.py](probe.py) builds 19 disposable fixtures. Each has `game/{chitin.key, r.txt,
Override/}` (no `Override/` in `override_absent`), `mods/{m.txt, n.txt, l.csv}` and
`mods/a.zip` holding `x.txt`, and a TOML guide whose components Copy, Move, CleanList
or DelDuplicate into folders that exist or not (`Nope`, `Nope/Deeper`, a missing
`Override`), in their own steps or in an option. Every run gets
`--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`, the C#
  wizard's preset (`WizardFull`) and Rust's desktop Validate step;
- `full`: the same with `--full` only;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but
  not `--skip-validation`, so it runs `WizardFull` before installing;
- `basic`: `validate ... --output json --dry-run`, for five cases;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 138 in all:

- `full_dry/decision`, `full/decision`: the exit code is zero or not, and `success`;
- `full_dry/counts`, `full/counts`: `errorCount`, `warningCount`, `passedCount`;
- `full_dry/stages`: each stage's name and `passed`, and the `ComponentValidation`
  and `DryRun` stages' messages, with the fixture root as `<root>`;
- `install`: the exit code is zero or not, and the files under `game/` and `mods/`
  afterwards (C#'s `.modsync` folder left out; empty folders are not compared);
- `modes`: Rust's plain `validate` decides as its `full_dry` report;
- `basic/decision`: as `full_dry/decision`, for the basic report.

`KNOWN` lists the documented differences (five cases, 18 checks). A known check passes
only if Rust differs from the reference and, where `KNOWN_RUST_MESSAGES` names them,
its `ComponentValidation` messages are exactly the expected Rust messages.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build. Its full pipeline's Environment stage needs a HoloPatcher in the build's
`Resources` folder, so the probe copies the build into its temporary directory and
adds `Resources/holopatcher`, a `#!/bin/sh` script that exits 2 (what the stage
expects from `holopatcher --install` without paths). It runs with the same arguments
under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder exists)
and `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need no network either.

C# results: the 14 cases with a folder C# can create (and the control) pass the full
report with and without a dry run, with `OK: <component>`, and `install` installs the
files, because the Environment stage created the folders (AttemptFixes). An empty
Destination, a file in the way and a missing archive fail the Environment stage; a
`.` part fails `ComponentValidation` with `Got '<root>/game/Nope/.'`; these four
refuse to install. The basic report fails every missing folder.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 120 match / 18 known difference / 0 fail |
| this slice, recorded reference | 120 match / 18 known difference / 0 fail |
| `38c6ca9` (pre-fix), recorded reference | 42 match / 18 known difference / 78 fail |

On `38c6ca9`, each of the 13 cases with a creatable missing folder failed six checks:
the full report failed (`Destination cannot be found! Got '<root>/game/...'`, one
error per step) with and without a dry run, so its decision, counts and stages
differed from C#, and plain `validate` passed while the report failed. `install`
matched on both builds, as did the basic report (it fails in both). The known
differences are the same on both builds.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/full-pipeline-destination-proof/probe.py                       # recorded reference
python3 docs/parity/full-pipeline-destination-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`). `MODSYNC_PROOF_OUT`, when
set, is the parent of the probe's temporary directory.
