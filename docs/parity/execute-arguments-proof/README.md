# Execute arguments proof

Contract: [execute-arguments-contract.md](../execute-arguments-contract.md).

[probe.py](probe.py) builds 22 disposable fixtures, each with one Execute of
`<<modDirectory>>/args.sh` and the case's `Arguments`. The script writes every
argument it receives, as `<arg>` on its own line, to `mods/args.txt`, so the final
tree records how the text was split. The probe runs the production `modsync` five ways
on a fresh copy of each case, all with `--allow-processes`, giving 110 checks:

- `validate --output json --dry-run`, the validation pipeline: the overall decision,
  the dry-run decision (no Error issue) and whether the report has the
  `DryRunSkipped` warning, against the reference pipeline;
- plain `validate`, against the reference install's decision;
- `install`, `install --component-checkpoints` and `install --component-checkpoints
  --managed` (private staging and manifest folders), against the reference install
  and, for an accepted install, the final game and mod files (names and contents, with
  the fixture folder written as `<ROOT>`). Rust's recovery and checkpoint state lives
  in hidden folders, which the tree comparison skips.

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
archive-member-brackets oracle unchanged: `pipeline` runs
`InstallationValidationPipeline.RunAsync` with `DryRun` and `UseFileSelection`, and
`install` runs `ModComponent.InstallAsync` on the real file system (which runs
`args.sh`) and passes on `InstallExitCode.Success`. It is built against
`ModSync.Core.dll` from `d31ccbd3`. Each reference run uses its own fixture and HOME
under `unshare -rn` (no network). The Rust runs need no network either.

`KNOWN` holds the deliberate differences: `unclosed_quote` and
`quoted_placeholder_backslash` (C# closes the quote at the end; Rust refuses) and
`unknown_placeholder` (C# passes `<<foo>>` as text; Rust refuses), all five checks
each, where the check fails unless Rust refuses; and `lowercase_placeholder` and
`root_with_space` (the three installing checks), where the check fails unless Rust
installs and passes exactly the documented argument (the game folder, as one
argument).

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 89 match / 21 known difference / 0 fail |
| this slice, recorded reference | 89 match / 21 known difference / 0 fail |
| `0cc50a7` (pre-fix), live reference | 68 match / 21 known difference / 21 fail |
| `0cc50a7` (pre-fix), recorded reference | 68 match / 21 known difference / 21 fail |

On `0cc50a7` the 21 failures are `install`, `--component-checkpoints` and `--managed`
of `single_quoted`, `backslash_path`, `backslash_space`, `hash_word`, `newline`,
`placeholder_backslash` and `doubled_quote_in_quotes`: Rust split the text with POSIX
shell rules on Linux, so the program received other arguments (for example
`<ROOT>/gameOverride` for `<<kotorDirectory>>\Override`, or only `x` for `x #y z`).
Plain `validate` and the pipeline matched there, because both splitters accept these
texts.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/execute-arguments-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/execute-arguments-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/execute-arguments-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve. Build the oracle outside the
repository, or remove the `obj/` folder it creates.
