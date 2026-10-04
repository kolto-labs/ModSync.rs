# CleanList list-file choice proof

Contract: [cleanlist-list-choice-contract.md](../cleanlist-list-choice-contract.md).

[probe.py](probe.py) builds 14 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 42 checks:

- `validate --output json --dry-run`: the decision (no Error issue) and whether
  the report has the `DryRunSkipped` warning, against the reference dry run;
- plain `validate`, Rust's strict install gate: the decision, against the
  reference install;
- `install`: the decision, against the reference install, and for an accepted
  install the final game and mod files (names and contents).

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
copy-move-self oracle unchanged (the same one the delete-directory proof uses),
built against `ModSync.Core.dll` from `d31ccbd3`. Dry run uses
`DryRunValidator.ValidateInstallationAsync`; install uses `ModComponent.InstallAsync`
on the real file system and passes on `InstallExitCode.Success`. Each reference run
uses its own fixture and HOME under `unshare -rn` (no network). The Rust runs need
no network either.

Every fixture has `game/chitin.key`, `game/Override/{a.tga, b.2da, s.tga}`, a game
list `game/Override/z.csv` (`Mandatory Deletions,s.tga`) and two mod lists,
`mods/l/a.csv` (`Mandatory Deletions,a.tga`) and `mods/l/b.csv`
(`Mandatory Deletions,b.2da`). The deleted game file shows which list was read. The
cases are listed in the contract.

`KNOWN` holds `folder_then_literal` (all three checks): Rust requires every Source to
match (F05-X2) and must refuse where the reference reads `b.csv`, or the check fails.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 39 match / 3 known difference / 0 fail |
| this slice, recorded reference | 39 match / 3 known difference / 0 fail |
| `0997b0f` (pre-fix) | 30 match / 3 known difference / 9 fail |

On `0997b0f` the nine failures are install trees: Rust read the list with the
smallest sorted key (`a.csv`, or `z.csv` under the game root) instead of the first
authored Source's list.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/cleanlist-list-choice-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/cleanlist-list-choice-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/cleanlist-list-choice-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
