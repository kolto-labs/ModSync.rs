# Root destination proof

Contract: [root-destination-contract.md](../root-destination-contract.md).

[probe.py](probe.py) builds 26 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 78 checks:

- `validate --output json --dry-run`, the validation pipeline: whether the
  archive/destination stage (`ComponentValidation`) passed, the dry-run decision (no
  Error issue) and whether the report has the `DryRunSkipped` warning, against the
  reference pipeline;
- plain `validate`, Rust's strict install gate: the decision, against the reference
  install;
- `install`: the decision, against the reference install, and for an accepted
  install the final game and mod files (names and contents).

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
cleanlist-list-choice oracle with one added mode, `pipeline`: it runs
`InstallationValidationPipeline.RunAsync` with `DryRun` and `UseFileSelection` (the
wizard preset without the environment and conflict stages, which need a real game
install) and reports each stage's `Passed` flag and messages. Install uses
`ModComponent.InstallAsync` on the real file system and passes on
`InstallExitCode.Success`. It is built against `ModSync.Core.dll` from `d31ccbd3`.
Each reference run uses its own fixture and HOME under `unshare -rn` (no network).
The Rust runs need no network either.

Every fixture has `game/{chitin.key, x.tga, x.tpc, r.txt}`,
`game/Override/{o.txt, q.tga, q.tpc}`, `mods/{m.txt, y.tga, y.tpc}` and the list
`mods/l.csv` (`Mandatory Deletions,r.txt`). The cases are listed in the contract.

`KNOWN` holds `rename_destination_kotor` (all three checks): Rust refuses a Rename to
the bare `<<kotorDirectory>>` placeholder (a dry-run error, plain validate and install
refused) where the reference renames the file to one named `<<kotorDirectory>>`. The
check fails unless Rust refuses.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 75 match / 3 known difference / 0 fail |
| this slice, recorded reference | 75 match / 3 known difference / 0 fail |
| `8c79761` (pre-fix) | 41 match / 3 known difference / 34 fail |

On `8c79761` the 34 failures are: the archive/destination stage for every case with a
root Destination (19), plain `validate` and `install` for the seven cases with a
DelDuplicate in a root (14), and `copy_override_dot`, whose `.` part Rust passed where the
reference stage fails (1).

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/root-destination-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/root-destination-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/root-destination-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
