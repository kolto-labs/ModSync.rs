# Patcher missing source proof

Contract: [patcher-missing-source-contract.md](../patcher-missing-source-contract.md).

[probe.py](probe.py) builds 18 disposable fixtures. Each has `game/{chitin.key,
Override/b.txt}` and `mods/` with `a.txt` to `d.txt`; a present patcher is
`mods/patch/TSLPatcher.exe` with `tslpatchdata/changes.ini` installing `tex.tga` into
`Override`. Component `B` (and `C` where named) runs the case's steps. Each side runs
on a fresh copy with `--use-file-selection`:

- `dryrun`: `validate -i G -s mods -g game --output json --dry-run`;
- `install`: `install -i G -s mods -g game`, plus `-y --skip-validation
  --no-checkpoint` for C# (it asks for confirmation, and its environment check needs a
  packaged HoloPatcher) and `--no-checkpoint` for Rust.

Checks, 69 in all:

- dryrun: `decision` (exit code zero or not, and the report's `success`), `counts`
  (`errorCount` and `warningCount`), `dryrun` (the `DryRun` stage messages in order,
  as `skipped` for `[DryRunSkipped]`, else `error` or `warning`) and `issues` (each
  `dryRun.issues` entry's severity, componentName and componentGuid, plus category
  and instructionIndex for warnings; an error's wording, category and step differ by
  design);
- install: `decision` (exit code zero or not).

Cases:

- A component's own Patcher step misses its Source (8): the patcher folder is
  missing, its `tslpatchdata` folder is missing (with namespace index `0`), the
  executable is missing from a present folder, the miss follows a present Copy, the
  miss precedes a Copy, another component passes, two components miss, and the miss
  follows a Copy with `Overwrite = false` onto an existing file (Rust's kept-file
  warning is dropped with the skip, as C# reports only the skip).
- Controls (6): a present patcher, a present patcher followed by a Rename of the file
  it installs, a Patcher step whose instruction dependency is not selected, a missing
  Copy source, a Patcher miss in a chosen option (an error in both), and an unselected
  component.
- Known differences (3): a wildcard Source and an empty Source list (Rust refuses,
  C# skips), and a Source naming the present patcher folder (Rust binds it, C# skips).
- Install (1): a Patcher miss after a Copy.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network either.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 58 match / 11 known difference / 0 fail |
| this slice, recorded reference | 58 match / 11 known difference / 0 fail |
| `0f71baa` (pre-fix), recorded reference | 26 match / 11 known difference / 32 fail |

On `0f71baa` each of the eight missing-patcher cases failed validation with
`ERROR: [EngineSimulation] B / step <n> (Patcher): Missing external program/mod:
<Source>` where C# passes with one `DryRunSkipped` warning per component, so all four
checks of each failed. The controls, the known-difference cases and the install
decision are the same on both builds.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/patcher-missing-source-proof/probe.py                       # recorded reference
python3 docs/parity/patcher-missing-source-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
