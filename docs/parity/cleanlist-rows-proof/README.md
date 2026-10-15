# CleanList rows and listed names proof

Contract: [cleanlist-rows-contract.md](../cleanlist-rows-contract.md).

[probe.py](probe.py) builds 47 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 141 checks:

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
root-destination oracle unchanged (`pipeline` mode runs
`InstallationValidationPipeline.RunAsync` with `DryRun` and `UseFileSelection`;
install uses `ModComponent.InstallAsync` on the real file system and passes on
`InstallExitCode.Success`), built against `ModSync.Core.dll` from `d31ccbd3`. Each
reference run uses its own fixture and HOME under `unshare -rn` (no network). The Rust
runs need no network either.

Every fixture has `game/{chitin.key, x.txt}` and
`game/Override/{a.tga, b.2da, c[1].tga, d.tga, sub/x.tga}`, and one component,
`CleanList Row Probe`, with `CleanList [<<modDirectory>>/l.csv] ->
<<kotorDirectory>>/Override`. Only the text of `mods/l.csv` changes; `{GAME}` in it
stands for the fixture's absolute game directory (and is written back in recorded
trees). The cases are listed in the contract.

`KNOWN` holds `dotdot`, `absolute`, `drive_relative`, `drive_absolute`,
`trailing_dot` and `reserved_name` (all three checks): Rust refuses where the
reference accepts, or the check fails. `KNOWN_TREES` holds `backslash_subdir` and
`case_variant` (install): both accept, and the Rust tree must equal the reference
tree with the file Windows would delete removed (`Override/sub/x.tga`,
`Override/a.tga`).

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 121 match / 20 known difference / 0 fail |
| this slice, recorded reference | 121 match / 20 known difference / 0 fail |
| `f9eab15` (pre-fix) | 91 match / 20 known difference / 30 fail |

On `f9eab15` the 30 failures are ten cases, three checks each (`quoted_file`,
`quoted_name_with_comma`, `quoted_files_with_comma`, `bracket_literal`,
`bracket_missing`, `star`, `question`, `star_in_subdir`, `colon`, `angle_pipe`): Rust
refused the listed name (`Unsafe path`, `Wildcards are not allowed here`) and failed
the step where the reference skipped it, or deleted `c[1].tga`, and succeeded.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/cleanlist-rows-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/cleanlist-rows-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/cleanlist-rows-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
