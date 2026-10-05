# No-managed proof

Contract: [no-managed-contract.md](../no-managed-contract.md).

[probe.py](probe.py) builds 9 disposable fixtures. Each has `game/{chitin.key,
Override/}`, `mods/` and one TOML guide of two components; each copies
`mods/<name>.txt` into `<<kotorDirectory>>/Override`:

| Guide | Components | Mod files |
| --- | --- | --- |
| good | `a`, `b` | `a.txt`, `b.txt` |
| broken | `a`, `f` | `a.txt` (`f.txt` is missing) |

The `settings_*` cases also write `<XDG_CONFIG_HOME>/ModSync/settings.json` with
`managedDeploymentEnabled: true` and `activeProfileName: "p"` (no such profile
exists). The cases:

| Case | Run and arguments | C# CLI |
| --- | --- | --- |
| `control` | install, good | installs a, b |
| `no_managed` | install, good, `--no-managed` after the roots | installs a, b |
| `no_managed_first` | install, good, `--no-managed` before `-i` | installs a, b |
| `no_managed_broken` | install, broken, `--no-managed` | installs a, then fails at f |
| `settings_no_managed` | install, good, settings, `--no-managed` | installs a, b (classic) |
| `settings_only` | install, good, settings | fails: profile `p` not found, nothing written |
| `both_flags` | install, good, `--managed --no-managed` | `Cannot combine --managed and --no-managed.`, nothing written |
| `validate_no_managed` | validate with roots, `--no-managed` | usage error |
| `rootless_no_managed` | validate without roots, `--no-managed` | usage error |

Runs and checks (17):

- install: `install -i G -s mods -g game --no-checkpoint` plus the case arguments (C#
  also gets `-y` but not `--skip-validation`, so it validates first;
  `--no-checkpoint` is left out of `both_flags`, because Rust's parser refuses it
  beside `--managed`): the exit code is zero or not, and every file under `game/` and
  `mods/` with its text afterwards (C#'s `.modsync` folder left out);
- validate: `validate -i G -s mods -g game` or `validate -i G` plus the case
  arguments: the exit code is zero or not;
- `both_flags` also checks that the output contains `Cannot combine --managed and
  --no-managed.`

`KNOWN` lists the documented differences (3 checks):

- `no_managed_broken/tree`, where the C# install writes `a` before it fails at `f`:
  it passes only if Rust leaves the files unchanged (its exit code must match);
- both checks of `settings_only`, where C# follows settings.json into a managed install
  that fails and the Rust CLI installs classic: they pass only if Rust's result equals
  the C# result of `settings_no_managed`.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy with a `Resources/holopatcher` stub (a `#!/bin/sh` script that
exits 2) under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`, and with `HOME`, `XDG_CONFIG_HOME` and
`XDG_DATA_HOME` inside each fixture, as
[select-values-proof](../select-values-proof/README.md) does. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need no network or .NET.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, twice; the oracle was unchanged) | 14 match / 3 known difference / 0 fail |
| this slice, recorded reference | 14 match / 3 known difference / 0 fail |
| `bef5735` (pre-fix), recorded reference | 7 match / 3 known difference / 7 fail |

On `bef5735` Rust stopped with a usage error at `--no-managed`; the failures are listed
in the contract.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/no-managed-proof/probe.py                       # recorded reference
python3 docs/parity/no-managed-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
output line of each run. `MODSYNC_PROOF_OUT`, when set, is the parent of the probe's
temporary directory.
