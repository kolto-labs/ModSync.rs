# Archive member brackets proof

Contract: [archive-member-brackets-contract.md](../archive-member-brackets-contract.md).

[probe.py](probe.py) builds 19 disposable fixtures compared with the reference
install and pipeline, and runs the production `modsync` four ways on a fresh copy of
each (five ways for the two cases that run a program), giving 78 checks:

- `validate --output json --dry-run`, the validation pipeline: the overall decision,
  the dry-run decision (no Error issue) and whether the report has the
  `DryRunSkipped` warning, against the reference pipeline;
- plain `validate`, Rust's strict install gate, against the reference install;
- `install`, against the reference install, and for an accepted install the final
  game and mod files (names and contents);
- `install --component-checkpoints`, compared the same way. Rust's recovery and
  checkpoint state lives in hidden folders, which the tree comparison skips;
- for the two cases whose second component runs `Execute` on a harmless `run.sh`
  (`exit 0`), also `install --component-checkpoints --managed --allow-processes`
  with private staging and manifest folders. Every Rust verb of those cases gets
  `--allow-processes`; the reference runs the script too.

It then makes 13 FOMOD checks: three reference mapper results and five FOMOD
fixtures, each run with plain `validate` and `install` and compared with the expected
game tree. The reference install does not map a FOMOD at Extract time, so the
expectation comes from the reference mapper
([csharp-fomod-map/Program.cs](csharp-fomod-map/Program.cs), which runs
`FomodParser.ParseModuleConfigXml` and `FomodToComponentMapper.Map` on a one-file
ModuleConfig and prints the generated Copy; recorded in
[csharp-fomod-map.json](csharp-fomod-map.json)): the mapped Source keeps `[`/`]`
verbatim, and a literal Copy Source installs that file (`copy_literal`). A bracketed
wrapper folder must give the same result as the `HD Mod` control.

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
| review follow-up, live reference and mapper (`--csharp ... --fomod-map ... --record`) | 83 match / 8 known difference / 0 fail |
| review follow-up, recorded reference | 83 match / 8 known difference / 0 fail |
| `4a09589` (first fix) | 74 match / 8 known difference / 9 fail |
| first fix (68 checks), live and recorded | 60 match / 8 known / 0 fail |
| `31e98ec` (pre-fix, 68 checks) | 15 match / 8 known difference / 45 fail |

On `4a09589` the 9 failures are the managed install of
`execute_after_bracket_deploy`, which failed after component 1 had deployed
`Override/x[1].tga` (1), and `validate` and `install` of the four bracketed FOMOD
cases, refused as stale saved choices (8).

On `31e98ec` the 45 failures (of the first 68 checks) are every check of the eleven cases whose archive holds
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
dotnet build docs/parity/archive-member-brackets-proof/csharp-fomod-map -c Release -o <map> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/archive-member-brackets-proof/probe.py --csharp <out>/probe.dll --fomod-map <map>/fomod-map.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve. Build the oracle outside the
repository, or remove the `obj/` folder it creates.
