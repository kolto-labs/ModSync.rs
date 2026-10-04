# Root destination proof

Contract: [root-destination-contract.md](../root-destination-contract.md).

[probe.py](probe.py) builds 41 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 123 checks:

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
`mods/l.csv` (`Mandatory Deletions,r.txt`). The bare-root Source cases add
`mods/a.zip` and `mods/sub/b.zip`; the `_archives_only` ones drop every other mod
file. The cases are listed in the contract.

`KNOWN` holds `rename_destination_kotor` (all three checks): Rust refuses a Rename to
the bare `<<kotorDirectory>>` placeholder (a dry-run error, plain validate and install
refused) where the reference renames the file to one named `<<kotorDirectory>>`. The
check fails unless Rust refuses. `KNOWN_ACCEPTS` holds
`extract_destination_mod_root` (pipeline only): the reference's archive/destination
stage fails every Extract that has a Destination and Rust's passes (predates this
slice); the check fails unless Rust accepts where the reference refuses.
`KNOWN` also holds the `validate` and `install` checks of `copy_kotor_dot`,
`copy_override_dot` and `move_kotor_dot` (six checks): the reference install here is
`ModComponent.InstallAsync` alone, which copies into a `.` Destination part, while the
C# CLI `install` validates first and refuses it, as Rust does since the
[destination-path-parts](../destination-path-parts-contract.md) slice.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| follow-up, live reference (`--csharp ... --record`) | 119 match / 4 known difference / 0 fail |
| follow-up, recorded reference | 119 match / 4 known difference / 0 fail |
| `6e67b3c` (first fix) | 102 match / 4 known difference / 17 fail |
| `8c79761` (pre-fix) | 74 match / 4 known difference / 45 fail |
| after [destination-path-parts](../destination-path-parts-contract.md), recorded reference | 113 match / 10 known difference / 0 fail |

On `6e67b3c` the 17 failures are: DelDuplicate and CleanList into `KD/.` and `OV/.`
(dry run, validate and install accepted and deleted; 12), the bare-root Extract cases
(`extract_source_mod_root_archives_only` extracted both archives, 3 checks; the mixed
and game-root ones reported a dry-run error where the reference skips, 2).

On `8c79761` the 45 failures are: the archive/destination stage for every case with a
root Destination (20), plain `validate` and `install` for the seven cases with a
DelDuplicate in a root (14), `copy_override_dot` (1), DelDuplicate into `OV/.` and
CleanList into `KD/.` and `OV/.` (9), and the Extract into the mod root, which
extracted into the root instead of `mods/a/` (1).

The first slice (`6e67b3c`, 26 cases, 78 checks) recorded 75 / 3 / 0, and 41 / 3 / 34
on `8c79761`.

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
