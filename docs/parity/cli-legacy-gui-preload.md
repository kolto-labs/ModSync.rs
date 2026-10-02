# F26 legacy GUI preload adapter

Source authority: pinned C# `src/ModSync.GUI/CLIArguments.cs`, including equals-form preload arguments and explicit multiple-instance flag. Source-only implementation; no CLI or GUI process was run.

`cli_compat::normalize_args(Vec<OsString>) -> anyhow::Result<Vec<OsString>>` preserves argv0 and maps only top-level legacy GUI arguments to the existing `gui` subcommand:

| Legacy input | Shared GUI option |
| --- | --- |
| `--instructionFile=<path>` | `--input=<path>` |
| `--kotorPath=<path>` | `--game-dir=<path>` |
| `--modDirectory=<path>` | `--source-dir=<path>` |
| `--nxm=<url>` / `--modsync=<url>` | `--protocol=<url>` |
| `--allow-multiple-instances` | Existing flag unchanged under `gui` |

Names match case-insensitively and values retain reference outer quote trimming. A positional protocol URL can accompany preload fields. Values are emitted as one equals-form argument to avoid treating a leading hyphen as a new option. No URL is fetched or printed by this adapter.

Modern subcommands and argument vectors with no legacy switch pass through unchanged. Once legacy normalization applies, unknown/mixed arguments, empty values, missing equals values, or duplicate targets reject with non-sensitive diagnostics. In particular supplying both protocol aliases rejects ambiguity rather than silently choosing the reference's nxm-first precedence. Protocol parsing, SSRF/authentication rules, single-instance forwarding, and actual GUI behavior remain owned by the existing shared pipeline.

Parent integration: declare `mod cli_compat;`, normalize `std::env::args_os().collect()` before Clap `parse_from`/`try_parse_from`, and route normalization errors through the existing CLI error exit. No other CLI verb is rewritten.
