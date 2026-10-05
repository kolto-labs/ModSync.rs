# Source order proof

Contract: [source-order-contract.md](../source-order-contract.md).

[probe.py](probe.py) builds 9 disposable fixtures. Each has `game/{chitin.key,
Override/old.txt}` and `mods/{a/x.txt, b/x.txt, b/y.txt}` (`A`, `B`, `BY`), and a TOML
guide with one step into `<<kotorDirectory>>/Override`, `Overwrite = true` unless
noted:

| Case | Step | C# CLI `Override/x.txt` |
| --- | --- | --- |
| `copy_ba` | `Copy [b/x.txt, a/x.txt]` | `A` |
| `copy_wildcards_ba` | `Copy [b/*, a/*]` | `A` (and `y.txt`) |
| `move_ba` | `Move [b/x.txt, a/x.txt]` | `A` |
| `copy_ba_b` | `Copy [b/x.txt, a/x.txt, b/x.txt]` | `A` |
| `copy_ba_false` | `Copy [b/x.txt, a/x.txt]`, `Overwrite = false` | `B` |
| `copy_ab`, `move_ab` | Source order equals path order (controls) | `B` |
| `copy_ab_false` | `Copy [a/x.txt, b/x.txt]`, `Overwrite = false` (control) | `A` |
| `copy_one` | `Copy [b/x.txt]` (control) | `B` |

Each case runs twice on a fresh copy, giving 18 checks:

- `validate`: Rust's plain `validate -i G -s mods -g game` decides as the C# CLI
  `install`;
- `install`: `install -i G -s mods -g game --no-checkpoint` (C# also gets `-y` but not
  `--skip-validation`): the exit code is zero or not, and every file under `game/`
  and `mods/` with its text afterwards (C#'s `.modsync` folder left out).

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy with a `Resources/holopatcher` stub (a `#!/bin/sh` script that
exits 2) under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`, as
[loose-source-proof](../loose-source-proof/README.md) does. Its results are in
[csharp-oracle.json](csharp-oracle.json); every case installs. The Rust runs need no
network or .NET.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, on `b8b4d57`, then without `--record`) | 18 match / 0 fail |
| this slice, recorded reference | 18 match / 0 fail |
| `b8b4d57` (pre-fix), live and recorded reference | 13 match / 5 fail |

On `b8b4d57` the installs of `copy_ba`, `copy_wildcards_ba`, `move_ba`, `copy_ba_b`
and `copy_ba_false` fail: Rust handled the files in path order, so `Override/x.txt`
was the other Source's file. The Extract cases are in
[extract-overwrite-proof](../extract-overwrite-proof/README.md).

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/source-order-proof/probe.py                       # recorded reference
python3 docs/parity/source-order-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`). `MODSYNC_PROOF_OUT`, when
set, is the parent of the probe's temporary directory.
