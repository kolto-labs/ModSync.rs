# Wildcard miss proof

Contract: [wildcard-miss-contract.md](../wildcard-miss-contract.md).

[probe.py](probe.py) builds 14 disposable fixtures. Each has `game/{chitin.key,
Override/g.tga}` and `mods/{m/a.tga, m/b.txi, c.txt, backup/, p.zip}` (`p.zip` holds
`x.tga`), and a TOML guide; `MD` is `<<modDirectory>>`, `OV` is
`<<kotorDirectory>>/Override`. Component `A` (and `B`, where named) holds the steps:

| Case | Steps | C# CLI `install` |
| --- | --- | --- |
| `move_one_miss` | Move [`MD/m/*.tga`, `MD/m/*.tpc`] -> `OV` | installs `a.tga` |
| `copy_two_of_three` | Copy [`MD/m/*.tga`, `MD/m/*.tpc`, `MD/m/*.txi`] -> `OV` | `a.tga`, `b.txi` |
| `copy_miss_first` | Copy [`MD/m/nope*.tga`, `MD/m/a.tga`] -> `OV` | `a.tga` |
| `copy_missing_folder` | Copy [`MD/m/*.tga`, `MD/q/*.tga`] -> `OV` (no `q/`) | `a.tga` |
| `copy_game_sources` | Copy [`OV/*.tga`, `OV/*.tpc`] -> `MD/backup` | `backup/g.tga` |
| `option_move` | Choose `O`; `O`: Move [`MD/m/*.tga`, `MD/m/*.tpc`] -> `OV` | `a.tga` |
| `next_component` | `move_one_miss`'s step, then `B`: Copy `MD/c.txt` -> `OV` | `a.tga`, `c.txt` |
| `extracted_other_component` | Extract `MD/p.zip`; `B`: Move [`MD/p/*.tga`, `MD/p/*.tpc`] -> `OV` | `x.tga` |
| `rename_wildcard_miss` | Rename [`MD/m/a.tga`, `MD/m/nope*.txt`] -> `z.tga` | `m/z.tga` |
| `all_miss` (control) | Copy [`MD/m/*.tpc`, `MD/m/*.dds`] -> `OV` | fails, nothing written |
| `rename_literal_miss` (control) | Rename [`MD/m/a.tga`, `MD/m/nope.txt`] -> `z.tga` | renames, then fails |
| `extracted_same_component` (control) | Extract `MD/p.zip`, Move [`MD/p/*.tga`, `MD/p/*.tpc`] -> `OV` | refused (`Failed to find`) |
| `literal_miss` (known) | Copy [`MD/m/a.tga`, `MD/m/nope.tga`] -> `OV` | `a.tga` |
| `extract_wildcard_miss` (known) | Extract [`MD/p.zip`, `MD/*.7z`]; Move `MD/p/*.tga` -> `OV` | `x.tga` |

Each case runs four ways on a fresh copy, giving 6 checks per case and 84 in all:

- `full_dry/decision`, `full_dry/counts`: `validate -i G -s mods -g game --output json
  --full --dry-run` (C# `WizardFull`, the wizard's Validate page and install
  preflight; Rust's desktop Validate step): the exit code is zero or not, `success`,
  and `errorCount`, `warningCount`, `passedCount`;
- `basic/decision`, `basic/counts`: the same without `--full`;
- `install`: `install -i G -s mods -g game --no-checkpoint` (C# also gets `-y` but not
  `--skip-validation`, so it validates first): the exit code is zero or not, and every
  file under `game/` and `mods/` with its text afterwards (C#'s `.modsync` folder left
  out, `p.zip` listed by name);
- `plain`: Rust's plain `validate -i G -s mods -g game` decides as the C# CLI
  `install`.

Known differences (`KNOWN` in the probe) must show the documented Rust result, or
the check fails:

- `literal_miss` (install, plain, both counts): a literal Source that names no file
  still fails the step, before any write; the reports pass with a `DryRunSkipped`
  warning, where C# drops the Source and installs `a.tga` (fail closed).
- `extract_wildcard_miss` (all six): an Extract still needs every Source to match
  (missing-archives contract); Rust's archive stage reports the step and the install
  is refused before any write.
- `rename_literal_miss` (install): both fail; C# renames `a.tga` to `z.tga` first and
  keeps it, Rust refuses before any write.
- `extracted_same_component` (both counts): both refuse; the counts differ as in the
  loose-source contract (C#'s basic report gives two errors per Source, its full
  pipeline stops in its Environment stage).

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy with a `Resources/holopatcher` stub (a `#!/bin/sh` script that
exits 2) under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`, as
[no-managed-proof](../no-managed-proof/README.md) does. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need no network or .NET.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, then without `--record`) | 71 match / 13 known difference / 0 fail |
| this slice, recorded reference | 71 match / 13 known difference / 0 fail |
| `f6d2ec5` (pre-fix), recorded reference | 31 match / 13 known difference / 40 fail |

On `f6d2ec5` seven of the nine fixed cases fail four checks each (install and plain
refused, a `DryRunSkipped` warning in both reports), and `option_move` and
`rename_wildcard_miss` fail all six (Rust's reports also failed the step): 40 checks.
The controls and known differences are the same on both binaries.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/wildcard-miss-proof/probe.py                       # recorded reference
python3 docs/parity/wildcard-miss-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`). `MODSYNC_PROOF_OUT`, when
set, is the parent of the probe's temporary directory.
