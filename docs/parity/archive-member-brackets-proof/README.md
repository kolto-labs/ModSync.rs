# Archive member brackets proof

Contract: [archive-member-brackets-contract.md](../archive-member-brackets-contract.md).

[probe.py](probe.py) builds 17 disposable fixtures and runs the production
`modsync` four ways on a fresh copy of each, giving 68 checks:

- `validate --output json --dry-run`, the validation pipeline: the overall decision,
  the dry-run decision (no Error issue) and whether the report has the
  `DryRunSkipped` warning, against the reference pipeline;
- plain `validate`, Rust's strict install gate, against the reference install;
- `install`, against the reference install, and for an accepted install the final
  game and mod files (names and contents);
- `install --component-checkpoints`, compared the same way. Rust's recovery and
  checkpoint state lives in hidden folders, which the tree comparison skips.

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
root-destination oracle unchanged: `pipeline` runs
`InstallationValidationPipeline.RunAsync` with `DryRun` and `UseFileSelection`, and
`install` runs `ModComponent.InstallAsync` on the real file system and passes on
`InstallExitCode.Success`. It is built against `ModSync.Core.dll` from `d31ccbd3`.
Each reference run uses its own fixture and HOME under `unshare -rn` (no network).
The Rust runs need no network either.

Every fixture has `game/{chitin.key, Override/o.txt}` and `mods/m.txt`, plus the
case's zip. The cases are listed in the contract.

`KNOWN` holds `star_member` and `question_member` (all four checks): the reference on
Linux extracts a member named `s*.tga` or `q?.tga`, which no Windows file can be;
Rust refuses the archive. The check fails unless Rust refuses.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 60 match / 8 known difference / 0 fail |
| this slice, recorded reference | 60 match / 8 known difference / 0 fail |
| `31e98ec` (pre-fix) | 15 match / 8 known difference / 45 fail |

On `31e98ec` the 45 failures are every check of the eleven cases whose archive holds
a bracketed member (the FOMOD gate refused plain `validate` and `install`; the
pipeline's archive/destination stage failed with `Cannot inspect archive`), and the
checkpointed install of `existing_game_bracket_file`, where checkpoint capture
refused `Override/pre[1].txt`.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/archive-member-brackets-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/archive-member-brackets-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/archive-member-brackets-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve. Build the oracle outside the
repository, or remove the `obj/` folder it creates.
