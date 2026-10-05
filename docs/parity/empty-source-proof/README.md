# Empty Source list proof

Contract: [empty-source-contract.md](../empty-source-contract.md).

[probe.py](probe.py) builds 18 disposable fixtures and runs the production
`modsync` on a fresh copy of each. For the first 17 it compares three ways with the
reference below (51 checks):

- `validate --output json --dry-run`: the decision (no Error issue) and whether
  the report has the `DryRunSkipped` warning, against the reference dry run;
- plain `validate`, Rust's strict install gate: the decision, against the
  reference install;
- `install`: the decision, against the reference install, and for an accepted
  install the final game tree (file names and contents).

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
rename-overwrite oracle unchanged, built against `ModSync.Core.dll` from
`d31ccbd3`. Dry run uses `DryRunValidator.ValidateInstallationAsync`; install
uses `ModComponent.InstallAsync` on the real file system and passes on
`InstallExitCode.Success`. Each reference run uses its own fixture and HOME under
`unshare -rn` (no network). The Rust runs need no network either; this probe has
no netns guard variable.

Every guide first copies `a.txt` and `b.txt` into `Override`. "Empty" is
`Source = []`, "blank" is `Source = [""]`, "no key" omits `Source`.

| Case | Step(s) after the copy | Reference dry run / install |
| --- | --- | --- |
| `delete_empty_omitted` | Delete, empty, Overwrite omitted (false for Delete) | pass / pass, tree unchanged |
| `delete_empty_false` | Delete, empty, false | pass / pass, tree unchanged |
| `delete_empty_true` | Delete, empty, true | skipped / `MissingSourceFiles` |
| `delete_blank_false` | Delete, blank, false | pass / pass |
| `delete_blank_true` | Delete, blank, true | skipped / fails |
| `delete_nokey_false` | Delete, no key, false | pass / pass |
| `delete_nokey_true` | Delete, no key, true | skipped / fails |
| `delete_empty_false_then_later` | Delete empty false, then Delete `b.txt` | pass / pass, `a.txt` left |
| `delete_empty_true_then_later` | Delete empty true, then Delete `b.txt` | skipped / fails |
| `copy_empty` | Copy, empty, to `Override` | skipped / fails |
| `copy_blank` | Copy, blank | skipped / fails |
| `move_empty` | Move, empty | skipped / fails |
| `move_empty_true` | Move, empty, Overwrite true | skipped / fails |
| `extract_empty` | Extract, empty, to `Override` | skipped / fails |
| `extract_blank_nodest` | Extract, blank, no Destination | skipped / fails |
| `extract_nokey` | Extract, no key | skipped / fails |
| `rename_empty_control` | Rename, empty, to `x.txt` | Error / `UnknownError` (unchanged control) |

Since [empty-extract-report](../empty-extract-report-contract.md) (2026-10-05) every
case is also compared with the pinned C# CLI, recorded in
[csharp-cli-oracle.json](csharp-cli-oracle.json) (38 checks):

- `full`: Rust's `validate --output json --full --dry-run` against the C# CLI's (the
  exit code is zero or not, and `success`);
- `basic_cli`: Rust's `validate --output json --dry-run` against the C# CLI's (no
  error, and the ComponentValidation stage's `ERROR` lines);
- `extract_empty_beside_archive` (`Extract []`, `Extract m.zip`, `Copy m/c.txt` to
  `Override`, no copy of `a.txt` and `b.txt`) has no reference result above; its
  plain `validate` and `install` are compared with the C# CLI `install` instead (it
  validates first, without `--skip-validation`): 4 checks with the two above.

The C# CLI runs on a copy of the `d31ccbd3` build with a `Resources/holopatcher` stub
(a `#!/bin/sh` script that exits 2) under `unshare -Urnm` (no network, a tmpfs over
`/Override` when that folder exists) with `DOTNET_ROLL_FORWARD=Major`, as
[loose-source-proof](../loose-source-proof/README.md) does. It fails the full report
(Environment stage) and the basic report (`Missing Required Archives: []`) of the
three `extract_*` cases and refuses their install before any write; it passes both
reports of `extract_empty_beside_archive` and refuses its install (the empty Extract
fails at execution). The other cases' reports decide as the reference above.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 51 match / 0 fail |
| this slice, recorded reference | 51 match / 0 fail |
| `fba4eb1` (pre-fix) | 25 match / 26 fail |
| after [extract-destination](../extract-destination-contract.md), recorded reference | 49 match / 2 known difference / 0 fail |
| empty-extract-report, live C# CLI (`--cli-reference ... --record` on `b8b4d57`, then without `--record`) | 86 match / 3 known difference / 0 fail |
| empty-extract-report, recorded references | 86 match / 3 known difference / 0 fail |
| `b8b4d57` (before empty-extract-report), live and recorded | 80 match / 2 known difference / 7 fail |

On `fba4eb1` every non-strict empty Delete fails all three checks: Rust rejected
it with `Instruction has no Source`, and the reference installs it as a no-op.
Every strict Delete and every Copy/Move/Extract case fails the dry-run check: Rust
reported an Error, and the reference reports `DryRunSkipped` and passes. The
Rename control matches on both binaries.

Since extract-destination (2026-10-05) the probe has `KNOWN`, which must be a
refusal in Rust where the reference accepts: the dry runs of `extract_empty` and
`extract_nokey`, whose Extract names Destination `Override`. Rust's basic report fails
an Extract with a Destination; the C# CLI basic report fails these cases on its
empty-Source archive check first (`Missing Required Archives: []`), and this reference
runs `DryRunValidator` alone. Plain `validate` and `install` still match.

Since empty-extract-report (2026-10-05) Rust's reports fail all three `extract_*`
cases with `Missing Required Archives: []`, as the C# CLI does, so `KNOWN` lists the
dry runs of `extract_empty`, `extract_blank_nodest` and `extract_nokey`; a known check
passes only if Rust and the C# CLI basic report both refuse. On `b8b4d57` the seven
failures are those cases' `full` checks (Rust passed), their `basic_cli` checks (no
error, or the Destination error) and `extract_blank_nodest`'s dry run (Rust passed).

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/empty-source-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/empty-source-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/empty-source-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
python3 docs/parity/empty-source-proof/probe.py --cli-reference <ref> [--record]   # live C# CLI
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
