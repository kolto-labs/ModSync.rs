# Plain report simulation proof

Contract: [plain-report-simulation-contract.md](../plain-report-simulation-contract.md).

[probe.py](probe.py) builds 19 disposable fixtures. Each has `game/{chitin.key,
Override/b.txt}` and `mods/` with `a.txt` to `d.txt`; a present patcher is
`mods/patch/TSLPatcher.exe` with `tslpatchdata/changes.ini` installing `tex.tga` into
`Override`. Every run gets `--use-file-selection` and a fresh copy of the fixture:

- `report`: `validate -i G -s mods -g game --output json`, plus `--full` where the case
  says so; run by both sides;
- C# also runs the same command without `--output json` (its text mode); the exit code
  is recorded, not compared;
- Rust also runs plain `validate -i G -s mods -g game` (never with `--full`), and the
  report command with `--output text` and with `--validation-report` in place of
  `--output json`.

Checks, 69 in all:

- `decision`: the report's exit code is zero or not, and its `success`, against C#;
- `modes`: Rust's plain `validate`, `--output text`, `--validation-report` and
  `--output json` decide alike;
- `report`: Rust's `errorCount`, and the `ComponentValidation` stage's `passed` and
  messages, as the case expects;
- `wording` (not for the controls): the stage's `ERROR:` line is plain `validate`'s
  `Error:` text.

Cases:

- Refused by plain `validate` (12): a missing Copy, Move, Patcher, strict
  (`Overwrite = true`) Delete, Rename and CleanList source, an Execute step without
  `--allow-processes` (refused before its Source is looked up), a miss after a present
  Copy (step 2), a miss in the second of three components (the others stay `OK:`), a
  chosen option's Patcher miss, a missing Copy with `--full`, and a component without
  instructions.
- Controls (7): a present Copy, a present patcher, a lenient Delete of a missing file,
  a missing Copy whose instruction dependency is not selected, a missing Copy in an
  unselected component, a missing Extract archive (the archive stage fails first; one
  error, no simulation), and a present Copy with `--full`. Six pass in Rust and fail in
  C# (known differences, below).

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json),
with its reason: `Error during validation: 'path' cannot be null or whitespace.
(Parameter 'path')` for the three Patcher guides, `ERROR: HoloPatcher could not be
found in the Resources directory. ...` for the two `--full` cases, and otherwise the
FOMOD stage's `ERROR: FOMOD: Mod directory is not set or does not exist. ...` (after
`ERROR: B: Missing Required Archives: [<<modDirectory>>/nope.zip]` for the missing
archive). Its text mode exits 1 for every case. The Rust runs need no network either.

Without roots in its basic mode, C# resolves `<<kotorDirectory>>/Override` to
`/Override` and, when that folder is missing, creates it while validating
(`AttemptFixes`); run the live reference in a disposable container.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 63 match / 6 known difference / 0 fail |
| this slice, recorded reference | 63 match / 6 known difference / 0 fail |
| `fd9ec46` (pre-fix), recorded reference | 15 match / 6 known difference / 48 fail |

Known differences: the six passing controls' `decision` (C# fails every basic report
and every `--full` report here). On `fd9ec46` all four checks of each of the 12 refused
cases failed: the report passed with `OK: B` while plain `validate` exited 1. The
controls are the same on both builds.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/plain-report-simulation-proof/probe.py                       # recorded reference
python3 docs/parity/plain-report-simulation-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
