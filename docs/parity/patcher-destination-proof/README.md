# Patcher destination proof

Contract: [patcher-destination-contract.md](../patcher-destination-contract.md).

[probe.py](probe.py) builds 9 disposable fixtures. Each has `game/{chitin.key,
Override/g.tga}` and `mods/{p/TSLPatcher.exe, p/tslpatchdata/changes.ini,
p/tslpatchdata/n.tga, backup/}` (the InstallList puts `n.tga` into `Override`), and a
TOML guide; `MD` is `<<modDirectory>>`, `KD` is `<<kotorDirectory>>`, `OV` is
`KD/Override`. Component `A` holds the steps; "then Copy" is a second step, Copy
[`OV/n.tga`] -> `MD/backup`, which needs the patched file where C# puts it:

| Case | Patcher Destination | C# CLI `install` |
| --- | --- | --- |
| `no_destination` | none, then Copy | patches `game/`, copies `n.tga` |
| `override_destination` | `OV`, then Copy | patches `game/`, copies `n.tga` |
| `mod_destination` | `MD` | patches `game/` |
| `trailing_separator` | `KD/`, then Copy | patches `game/`, copies `n.tga` |
| `option_no_destination` | none, in chosen option `O`, then Copy in `O` | patches `game/`, copies `n.tga` |
| `game_destination` (control) | `KD`, then Copy | patches `game/`, copies `n.tga` |
| `gamedirectory_destination` (control) | `<<gameDirectory>>`, then Copy | patches `game/`, copies `n.tga` |
| `placeholder_case` (control) | `<<KotorDirectory>>`, then Copy | refused, nothing written |
| `unsafe_destination` (known) | `/tmp/elsewhere`, then Copy | patches `game/`, copies `n.tga` |

Both sides use the same HoloPatcher stub, a shell script: with `--game-dir G
--tslpatchdata T` it writes `G/Override/n.tga` (`patched`) and `T/installlog.txt` and
exits 0; without them it exits 2, which C#'s environment test (`holopatcher --install`)
expects. Each case runs four ways on a fresh copy, giving 8 checks per case and 72 in
all:

- `full_dry/decision`, `full_dry/counts`, `full_dry/stage`: `validate -i G -s mods -g
  game --output json --full --dry-run` (C# `WizardFull`, the wizard's Validate page and
  install preflight; Rust's desktop Validate step): the exit code is zero or not,
  `success`; `errorCount`, `warningCount`, `passedCount`; the ComponentValidation
  stage's `passed` and messages (the fixture folder shown as `<root>`);
- `basic/decision`, `basic/counts`, `basic/stage`: the same without `--full`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it validates first, and finds the stub in its `Resources`
  folder; Rust also gets `--allow-processes --patcher <stub>`. The exit code is zero
  or not, and every file under `game/` and `mods/` with its text afterwards (C#'s
  `.modsync` folder and the patcher folder `mods/p/` left out: C# writes its install
  log and INI edits there, Rust in a staged copy);
- `plain`: Rust's plain `validate -i G -s mods -g game` decides as the C# CLI
  `install`.

Known differences (`KNOWN` in the probe) must show the documented Rust result, or
the check fails:

- `unsafe_destination` (install, plain, `full_dry/decision`, both counts): a
  Destination outside the roots is still refused before any write (fail closed); the
  reports fail the dry run too (one more error), where C# sets the Destination to the
  game folder and installs.
- `placeholder_case` (both counts, both stages): both refuse; C# fails its Environment
  stage (full) or names the unresolved path (basic), Rust names the unknown
  placeholder and its dry run refuses the step too.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy with the stub as `Resources/holopatcher` under `unshare -Urnm`
(no network, a tmpfs over `/Override` when that folder exists) with
`DOTNET_ROLL_FORWARD=Major`, as [no-managed-proof](../no-managed-proof/README.md) does.
Its results are in [csharp-oracle.json](csharp-oracle.json). The Rust runs need no
network or .NET.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, then without `--record`) | 63 match / 9 known difference / 0 fail |
| this slice, recorded reference | 63 match / 9 known difference / 0 fail |
| `d4f30d1` (pre-fix), recorded reference | 34 match / 8 known difference / 30 fail |

On `d4f30d1` the five fixed cases fail 27 checks: every report failed an empty or
non-game Destination in ComponentValidation (`Path must start with <<modDirectory>> or
<<kotorDirectory>>: ` or `Patcher destination must be the game directory`) where C#
passes the full report and warns or errs in the basic one; `<<kotorDirectory>>/`
passed the basic report, which C# fails; and the install of `override_destination`
(refused, its Copy finding nothing) and `mod_destination` (`mods/Override/n.tga`) ran
the patcher on the Destination. `unsafe_destination` fails three more checks (its
stage messages and full-report counts used the old wording). The controls are the
same on both binaries.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/patcher-destination-proof/probe.py                       # recorded reference
python3 docs/parity/patcher-destination-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`). `MODSYNC_PROOF_OUT`, when
set, is the parent of the probe's temporary directory.
