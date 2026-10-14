# Patcher missing configuration proof

Contract: [patcher-missing-configuration-contract.md](../patcher-missing-configuration-contract.md).

[probe.py](probe.py) builds 18 disposable fixtures. Each has `game/{chitin.key,
Override/b.txt}` and `mods/` with `a.txt` to `d.txt`. Every case has the patcher
executable `mods/patch/TSLPatcher.exe`; the case varies its configuration. A complete
patcher also has `tslpatchdata/changes.ini` installing `tex.tga` into `Override`. Each
side runs `validate -i G -s mods -g game --output json --dry-run --use-file-selection`
on a fresh copy.

Checks, 72 in all (four per case):

- `decision`: the exit code is zero or not, and the report's `success`;
- `counts`: `errorCount` and `warningCount`;
- `dryrun`: the `DryRun` stage messages in order, as `skipped` for `[DryRunSkipped]`,
  else `error` or `warning`;
- `issues`: each `dryRun.issues` entry's severity, category, componentName,
  componentGuid and instructionIndex.

Cases:

- No `changes.ini` where the simulation looks (10): no `tslpatchdata` folder, a
  `tslpatchdata` folder with a payload but no INI, an otherwise empty `tslpatchdata`,
  the INI one folder too deep (`tslpatchdata/sub/changes.ini`), the INI under another
  name (`changes.txt`), namespace index `0` with no `namespaces.ini`, the Patcher step
  before a Copy, after a Copy, two such components, and one such component beside a
  complete patcher.
- Controls (7): a complete patcher, a complete patcher followed by a Rename of the
  file it installs, a patcher without an INI followed by a Rename of that file (the
  file is missing in both, so the component is skipped), `changes.ini` in the patcher
  folder itself, an empty `changes.ini`, a `changes.ini` without `[InstallList]`, and a
  missing executable (the component is skipped,
  [patcher-missing-source-contract.md](../patcher-missing-source-contract.md)).
- Known difference (1): the INI lists `tex.tga` but `tslpatchdata` lacks it; Rust
  keeps its deliberate `unknown payload` warning
  ([patcher-install-list-simulation.md](../patcher-install-list-simulation.md)), C#
  writes an empty file silently. Three known-difference checks (the decision
  matches).

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -Urnm` (no network, with a tmpfs over
`/Override` when that folder exists, so a C# run without roots could not write the
host's) and `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json): every case passes with no warning, except
the two skips. The Rust runs need no network either.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 69 match / 3 known difference / 0 fail |
| this slice, recorded reference | 69 match / 3 known difference / 0 fail |
| `c22836a` (pre-fix), recorded reference | 39 match / 3 known difference / 30 fail |

On `c22836a` each of the ten cases without an INI passed with one
`WARNING: [PatcherSimulation] No changes.ini found; no InstallList effects inferred`
per component where C# reports nothing, so `counts`, `dryrun` and `issues` failed
(the decision matched). The controls and the known difference are the same on both
builds.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/patcher-missing-configuration-proof/probe.py                       # recorded reference
python3 docs/parity/patcher-missing-configuration-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
