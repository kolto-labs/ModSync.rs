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

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 20 match / 0 fail |
| this slice, recorded reference | 20 match / 0 fail |
| `5905259` (pre-fix) | 15 match / 5 fail |

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
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
