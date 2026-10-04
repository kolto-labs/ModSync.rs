# Patcher INI brackets proof

Contract: [patcher-ini-brackets-contract.md](../patcher-ini-brackets-contract.md).

[probe.py](probe.py) builds 15 disposable fixtures. Each has a patcher folder
(`mods/<patch>/TSLPatcher.exe` and `tslpatchdata/changes.ini`, with payloads) and a
guide with one Patcher step per component, usually followed by a Rename of the file
the InstallList installs, which passes only when the simulation made it present. Most
INI names hold `[` or `]`. The probe runs the production `modsync` three ways on a
fresh copy of each case, all with `--allow-processes` (no program runs: validation
and conflict analysis simulate the patcher), giving 45 checks:

- `validate --output json --dry-run`, the validation pipeline: the overall decision,
  the dry-run decision (no Error issue) and whether the report has the
  `DryRunSkipped` warning, against the reference pipeline;
- plain `validate`, against the reference pipeline's decision;
- `conflicts`, against the reference `FileConflictAnalyzer`: the conflict paths,
  writers (GUID, name, instruction index), winners, per-component counts and the
  analyzed component count.

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs): `pipeline` runs
`InstallationValidationPipeline.RunAsync` with `DryRun` and `UseFileSelection` (as the
execute-arguments oracle does), and `conflicts` runs `FileConflictAnalyzer.AnalyzeAsync`
over the guide's components. It is built against `ModSync.Core.dll` from `d31ccbd3`.
Each reference run uses its own fixture and HOME under `unshare -rn` (no network). The
Rust runs need no network either.

`KNOWN` holds the deliberate differences: `star_file` and `question_file` (C# on
Linux simulates a file named with `*` or `?`; Rust refuses it on every host), all
three checks each, where the check fails unless Rust refuses.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 39 match / 6 known difference / 0 fail |
| this slice, recorded reference | 39 match / 6 known difference / 0 fail |
| `c2225e3` (pre-fix), live reference | 6 match / 6 known difference / 33 fail |
| `c2225e3` (pre-fix), recorded reference | 6 match / 6 known difference / 33 fail |

On `c2225e3` the 33 failures are every check of the 11 bracket cases: Rust refused
the guide with `Wildcards are not allowed here: [HD]tex.tga` (or the folder, IniName
or DataPath), in the dry-run report, in plain validate and in `conflicts`. Only the
two controls (`plain_file`, `conflict_plain`) matched.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/patcher-ini-brackets-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/patcher-ini-brackets-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/patcher-ini-brackets-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve. Build the oracle outside the
repository, or remove the `obj/` folder it creates.
