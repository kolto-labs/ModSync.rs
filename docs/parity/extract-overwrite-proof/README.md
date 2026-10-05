# Extract Overwrite=false proof

Contract: [extract-overwrite-contract.md](../extract-overwrite-contract.md).

[probe.py](probe.py) builds 10 disposable fixtures (ZIP archives written by
Python, plus files that already exist where the archive extracts) and runs the
production `modsync` two ways on a fresh copy of each, giving 20 checks:

- plain `validate`, Rust's strict install gate: the decision, against the
  reference install;
- `install`: the decision, against the reference install, and for an accepted
  install the final game tree and the extracted mod files (names and contents;
  the archives themselves are left out).

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
delete-strict oracle unchanged, built against `ModSync.Core.dll` from `d31ccbd3`.
Install uses `ModComponent.InstallAsync` on the real file system and passes on
`InstallExitCode.Success`. The oracle also records the reference dry run
(`DryRunValidator.ValidateInstallationAsync`); it is not compared, because the
reference virtual provider nests every extraction under an archive-named folder
(see the contract's known differences). Each reference run uses its own fixture
and HOME under `unshare -rn` (no network). The Rust runs need no network either;
this probe has no netns guard variable.

`pack.zip` holds `x.txt` = `NEW` and `y.txt`; `a.zip` and `b.zip` each hold only
`x.txt` (`FROM A`, `FROM B`). `out` is `<<modDirectory>>/out`. Except for the two
`game_override` cases, the guide then copies the extracted `x.txt` into `Override`.

| Case | Extract step(s) | Existing file | Reference `x.txt` |
| --- | --- | --- | --- |
| `stale_false` | `pack.zip` → `out`, false | `out/x.txt` = `OLD` | `NEW` |
| `stale_true` | the same, true | `out/x.txt` = `OLD` | `NEW` |
| `stale_omitted` | the same, omitted (true) | `out/x.txt` = `OLD` | `NEW` |
| `fresh_false` | `pack.zip` → `out`, false | none | `NEW` |
| `stale_default_destination_false` | `pack.zip`, no Destination (→ `pack/`), false | `pack/x.txt` = `OLD` | `NEW` |
| `two_archives_false` | `a.zip`, `b.zip` → `out` in one step, false | none | `FROM B` |
| `two_archives_true` | the same, true | none | `FROM B` |
| `two_steps_false` | `a.zip` → `out`, then `b.zip` → `out`, both false | none | `FROM B` |
| `game_override_false` | `pack.zip` → `Override`, false | `Override/x.txt` = `OLD` | `NEW` |
| `game_override_true` | the same, true | `Override/x.txt` = `OLD` | `NEW` |

Every reference install succeeds.

## Since extract-destination (2026-10-05)

An Extract Destination is no longer used, as in the C# CLI and GUI, whose validation
clears it before the install
([extract-destination-contract.md](../extract-destination-contract.md)); the
reference above, `ModComponent.InstallAsync` alone, extracts into it. So the probe
compares every case whose Extract has a Destination (all but
`stale_default_destination_false`) with the C# CLI `install` instead: `run_cli` runs
`install -i G -s mods -g game --no-checkpoint -y` on a copy of the `d31ccbd3` build
with a HoloPatcher stub in `Resources` (as
[rename-file-name-proof](../rename-file-name-proof/README.md)), under `unshare -Urnm`
with a tmpfs over `/Override`, and records the decision and, for an accepted install,
the trees in [csharp-cli-oracle.json](csharp-cli-oracle.json) (C#'s `.modsync`
folder left out). Such a check is `known` when Rust equals the C# CLI and differs from
InstallAsync, `match` when it equals both, and fails otherwise.

The C# CLI refuses `stale_false`, `stale_true` and `stale_omitted` for an older
reason: its validation (`ComponentValidation.VerifyExtractPaths`) finds no archive
member for the loose Copy Source `out/x.txt` (`Failed to find 'out/x.txt' in any
archives!`). Since [loose-source](../loose-source-contract.md) (2026-10-05) Rust's
plain `validate` and `install` refuse it too, so those six checks are ordinary known
differences from InstallAsync (before, Rust had to accept and copy the stale
`out/x.txt`, `OLD`, into `Override`). The C# CLI refuses
`fresh_false` and the `two_*` cases (there is no `out/x.txt`) and installs the two
`game_override` cases with `x.txt` left `OLD` in `Override` and the archive extracted
to `mods/pack/`; Rust gives the same.

## Results (cloud Linux)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`), 2026-10-04 | 20 match / 0 fail |
| this slice, recorded reference, 2026-10-04 | 20 match / 0 fail |
| `5905259` (pre-fix) | 15 match / 5 fail |
| extract-destination, live C# CLI (`--cli-reference ... --record`), 2026-10-05 | 4 match / 16 known difference / 0 fail |
| extract-destination, recorded references | 4 match / 16 known difference / 0 fail |
| `c0e9cef` (before extract-destination), recorded references | 4 match / 3 known difference / 13 fail |
| loose-source, live C# CLI (`--cli-reference ... --record`), 2026-10-05 | 4 match / 16 known difference / 0 fail |
| loose-source, recorded references | 4 match / 16 known difference / 0 fail |
| `6ca4d35` (before loose-source), recorded references | 4 match / 10 known difference / 6 fail |

On `5905259` the five `Overwrite = false` collisions fail the install check:
`stale_false`, `stale_default_destination_false` and `game_override_false` keep
`OLD`, and `two_archives_false` and `two_steps_false` keep `FROM A`. Validate
passes in both builds, as the reference install does.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/extract-overwrite-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/extract-overwrite-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/extract-overwrite-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
python3 docs/parity/extract-overwrite-proof/probe.py --cli-reference <ref> [--record]   # live C# CLI
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
