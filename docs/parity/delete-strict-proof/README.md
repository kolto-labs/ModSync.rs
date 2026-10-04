# Delete Overwrite=true missing-source proof

Contract: [delete-strict-contract.md](../delete-strict-contract.md).

[probe.py](probe.py) builds 13 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 39 checks:

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

Every guide first copies `a.txt` and `b.txt` into `Override`. `c.txt` never exists.

| Case | Delete step(s) on `Override/` | Reference dry run / install |
| --- | --- | --- |
| `missing_omitted` | `c.txt`, Overwrite omitted (false for Delete) | pass / pass, tree unchanged |
| `missing_false` | `c.txt`, false | pass / pass |
| `missing_true` | `c.txt`, true | pass with DryRunSkipped / `MissingSourceFiles` |
| `present_true` | `a.txt`, true | pass / pass, `b.txt` left |
| `present_omitted` | `a.txt`, omitted | pass / pass, `b.txt` left |
| `present_and_missing_true` | `a.txt`, `c.txt`, true | skipped / fails |
| `present_and_missing_false` | `a.txt`, `c.txt`, false | pass / pass, `b.txt` left |
| `wildcard_nomatch_true` | `*.zzz`, true | skipped / fails |
| `wildcard_nomatch_false` | `*.zzz`, false | pass / pass |
| `wildcard_match_true` | `a.*`, true | pass / pass, `b.txt` left |
| `present_and_wildcard_nomatch_true` | `a.txt`, `*.zzz`, true | pass / pass, `b.txt` left |
| `deleted_earlier_true` | `a.txt` false, then `a.txt` true | skipped / fails |
| `missing_true_then_later` | `c.txt` true, then `b.txt` false | skipped / fails |

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 39 match / 0 fail |
| this slice, recorded reference | 39 match / 0 fail |
| `8fad43c` (pre-fix) | 24 match / 15 fail |

On `8fad43c` all five strict-miss cases fail all three checks: the dry-run
report has no `DryRunSkipped` warning, plain validate passes, and install
succeeds. `missing_true_then_later` installs and runs the later step, deleting
`b.txt`.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/delete-strict-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/delete-strict-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/delete-strict-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
