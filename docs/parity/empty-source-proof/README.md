# Empty Source list proof

Contract: [empty-source-contract.md](../empty-source-contract.md).

[probe.py](probe.py) builds 17 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 51 checks:

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

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 51 match / 0 fail |
| this slice, recorded reference | 51 match / 0 fail |
| `fba4eb1` (pre-fix) | 25 match / 26 fail |
| after [extract-destination](../extract-destination-contract.md), recorded reference | 49 match / 2 known difference / 0 fail |

On `fba4eb1` every non-strict empty Delete fails all three checks: Rust rejected
it with `Instruction has no Source`, and the reference installs it as a no-op.
Every strict Delete and every Copy/Move/Extract case fails the dry-run check: Rust
reported an Error, and the reference reports `DryRunSkipped` and passes. The
Rename control matches on both binaries.

Since extract-destination (2026-10-05) the probe has `KNOWN`, which must be a
refusal in Rust where the reference accepts: the dry runs of `extract_empty` and
`extract_nokey`, whose Extract names Destination `Override`. Rust's basic report fails
an Extract with a Destination, as the C# CLI's does (ComponentValidation); this
reference runs `DryRunValidator` alone. Plain `validate` and `install` still match.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/empty-source-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/empty-source-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/empty-source-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
