# Option failure proof

Contract: [option-failure-contract.md](../option-failure-contract.md).

[probe.py](probe.py) builds 24 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/` with `a.txt` to `h.txt`. Component `B` copies `b.txt` into
`Override`, then runs the case's steps; its options are selected unless the case says
otherwise. Each side runs on a fresh copy with `--use-file-selection`:

- `dryrun`: `validate -i G -s mods -g game --output json --dry-run`;
- `install`: `install -i G -s mods -g game`, plus `-y --skip-validation
  --no-checkpoint` for C# (it asks for confirmation, and its environment check needs a
  packaged HoloPatcher) and `--no-checkpoint` for Rust.

Checks, 68 in all:

- dryrun: `decision` (exit code zero or not, and the report's `success`), `counts`
  (`errorCount` and `warningCount`) and `dryrun` (the `DryRun` stage messages in order,
  as `skipped` for `[DryRunSkipped]`, else `error` or `warning`; the wording differs);
- install: `decision` (exit code zero or not).

Cases:

- A chosen option misses a source (14): Copy, a Copy wildcard, Move, a strict
  (`Overwrite = true`) Delete, Rename, CleanList, Extract (its `counts` is a known
  difference: Rust's archive stage also reports the archive), a miss after a present
  file in the same option, the second of two chosen options, the second of two
  `Choose` steps, later steps after the `Choose`, another component that passes, two
  failing components, and an option step whose instruction dependency is not
  selected (nothing is missing).
- Controls (8): the miss in the component's own steps (literal, wildcard, and before
  a failing `Choose`), a present option, an unselected option, a selected option no
  `Choose` names, a `Choose` whose instruction dependency is not selected, and an
  unselected component.
- Install (2): a chosen option with a missing source, and the same with the file
  present.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network either.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 67 match / 1 known difference / 0 fail |
| this slice, recorded reference | 67 match / 1 known difference / 0 fail |
| `b89484d` (pre-fix), recorded reference | 30 match / 1 known difference / 37 fail |

On `b89484d` each missing-source case passed with one `DryRunSkipped` warning where C#
fails with one error per component (the Extract case already failed in Rust's archive
stage, so only its dry-run messages failed). The controls and the install decisions
match on both builds.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/option-failure-proof/probe.py                       # recorded reference
python3 docs/parity/option-failure-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
