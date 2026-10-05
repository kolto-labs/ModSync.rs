# Select values proof

Contract: [select-values-contract.md](../select-values-contract.md).

[probe.py](probe.py) builds 14 disposable fixtures. Each has `game/{chitin.key,
Override/}`, `mods/{a,b,c,d,e}.txt` and one TOML guide of six unselected components,
`a` to `f`; each copies `mods/<name>.txt` into `<<kotorDirectory>>/Override`, and
`mods/f.txt` is missing:

| Component | Category | Tier |
| --- | --- | --- |
| `a` | Graphics Improvement | 1 - Essential |
| `b` | Bugfix, Graphics Improvement | 2 - Recommended |
| `c` | Bugfix | 3 - Suggested |
| `d` | Immersion | 4 - Optional |
| `e` | Immersion | Custom Tier |
| `f` | Broken | 4 - Optional |

The cases differ only in their selection arguments, given after `-i`, `-s` and `-g`
unless noted:

| Case | Arguments | C# CLI `install` |
| --- | --- | --- |
| `one_value` | `--select category:Bugfix` | installs b, c |
| `one_value_broken` | `--select category:Broken` | fails, nothing written |
| `no_value` | `--select --use-file-selection` | usage error |
| `no_match` | `--select category:Nope tier:Nope` | fails (no component) |
| `two_categories` | `--select category:Bugfix category:Immersion` | installs b, c, d, e |
| `category_and_tier` | `--select "category:Graphics Improvement" "tier:1 - Essential"` | installs a |
| `two_tiers` | `--select "tier:custom tier" tier:Recommended` | installs a, b, e |
| `values_then_flag` | `two_categories` then `--use-file-selection` | installs b, c, d, e |
| `values_before_roots` | `two_categories`, before `-i` | installs b, c, d, e |
| `unknown_type_in_list` | `--select category:Bugfix name:x` | installs b, c |
| `broken_in_list` | `--select category:Bugfix category:Broken` | installs b, c, then fails at f |
| `repeated_flag` | `--select category:Bugfix --select category:Immersion` | usage error |
| `equals_then_value` | `--select=category:Bugfix category:Immersion` | installs b, c, d, e |
| `name_in_list` | `--select category:Bugfix a` | installs b, c |

Each case runs three times on a fresh copy, giving 42 checks:

- `full_dry`: `validate ... --output json --full --dry-run`: the exit code is zero
  or not, and `success` (null when stdout holds no JSON, as after a usage error);
- `install`: `install ... --no-checkpoint` (C# also gets `-y` but not
  `--skip-validation`): the exit code is zero or not, and every file under `game/`
  and `mods/` with its text afterwards (C#'s `.modsync` folder left out);
- `plain` (Rust only): plain `validate` decides as the C# CLI `install`.

`KNOWN` lists the documented differences (8 checks):

- every check of `repeated_flag` (3), which C# refuses and Rust accepts: they pass
  only if Rust's result equals the C# result of `two_categories`;
- every check of `equals_then_value` (3), which C# installs and Rust refuses with a
  usage error: they pass only if Rust fails with the files unchanged;
- `name_in_list/install`, where Rust also installs `a` (an exact component name):
  it passes only if Rust installs `a`, `b` and `c`;
- `broken_in_list/install`, where the C# install writes b and c before it fails at
  f: it passes only if Rust fails with the files unchanged.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy with a `Resources/holopatcher` stub (a `#!/bin/sh` script that
exits 2) under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`, as
[loose-source-proof](../loose-source-proof/README.md) does. Its results are in
[csharp-oracle.json](csharp-oracle.json). The C# full report passes every case that
parses and selects a component, `one_value_broken` and `broken_in_list` included
(a missing Copy Source is a warning there), and fails `no_match`. The Rust runs need
no network or .NET.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, twice; the oracle was unchanged) | 34 match / 8 known difference / 0 fail |
| this slice, recorded reference | 34 match / 8 known difference / 0 fail |
| `b97c090` (pre-fix), recorded reference | 12 match / 7 known difference / 23 fail |

On `b97c090` Rust stopped with a usage error in the nine cases with two values after
one flag; the failures are listed in the contract.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/select-values-proof/probe.py                       # recorded reference
python3 docs/parity/select-values-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of each Rust plain `validate` and `install` (and C# install) output.
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
