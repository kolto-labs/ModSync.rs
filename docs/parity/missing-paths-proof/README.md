# Missing paths proof

Contract: [missing-paths-contract.md](../missing-paths-contract.md).

[probe.py](probe.py) builds a disposable fixture per run: `game/{chitin.key,
Override/g.txt}`, `mods/a/x.txt`, a TOML guide `g.toml` (component `A` copies
`<<modDirectory>>/a/x.txt` to `<<kotorDirectory>>/Override`), a regular file `afile`
and an empty folder `adir`. Each command runs in the fixture with the relative paths
below, under `unshare -Urnm` on both sides, with `HOME` and `XDG_*` inside the fixture.
`nogame`, `nomods` and `nope.toml` do not exist; `J` is `--output json`.

| Case | Command | C# |
| --- | --- | --- |
| `game_missing_json_dry` | `validate -i g.toml -s mods -g nogame J --dry-run` | `error`: `Game directory not found: nogame` |
| `game_missing_json_full` | `validate -i g.toml -s mods -g nogame J --full` | same |
| `mods_missing_json_full` | `validate -i g.toml -s nomods -g game J --full` | `Source directory not found: nomods` |
| `both_missing_json_full` | `validate -i g.toml -s nomods -g nogame J --full` | `Game directory not found: nogame` |
| `mods_missing_json_dro` | `validate -i g.toml -s nomods -g game J --full --dry-run-only` | `Source directory not found: nomods` |
| `game_file_text_dry` | `validate -i g.toml -s mods -g afile --dry-run` | `Error: Game directory not found: afile` |
| `mods_missing_text_full` | `validate -i g.toml -s nomods -g game --full` | `Error: Source directory not found: nomods` |
| `input_missing_json_full` | `validate -i nope.toml -s nomods -g nogame J --full` | `Input file not found: nope.toml` |
| `input_missing_json_basic` | `validate -i nope.toml J` | same |
| `input_dir_text` | `validate -i adir -s mods -g game` | `Error: Input file not found: adir` |
| `install_game_missing` | `install -i g.toml -s mods -g nogame` | `Error: Game directory not found: nogame` |
| `install_mods_missing` | `install -i g.toml -s nomods -g game` | `Error: Source directory not found: nomods` |
| `install_mods_file` | `install -i g.toml -s afile -g game` | `Error: Source directory not found: afile` |
| `install_both_missing` | `install -i g.toml -s nomods -g nogame` | `Error: Game directory not found: nogame` |
| `install_input_missing` | `install -i nope.toml -s mods -g nogame` | `Error: Input file not found: nope.toml` |
| `install_input_dir` | `install -i adir -s nomods -g game` | `Error: Input file not found: adir` |
| `plain_mods_missing` (known) | `validate -i g.toml -s nomods -g game` | fails its FOMOD stage; no `not found` line |
| `ok_json_dry` (control) | `validate -i g.toml -s mods -g game J --dry-run` | passes |
| `ok_json_full` (control) | `validate -i g.toml -s mods -g game J --full` | passes |

Every C# case except the controls exits 1. Three checks per case:

- `exit`: the exit code is zero or not;
- `message`: with `--output json`, the report's `error` field; otherwise the first
  `Input file not found: <path>`, `Game directory not found: <path>` or `Source
  directory not found: <path>` text in stdout and stderr (none when there is none);
- `unchanged`: every file under `game` and `mods` is as before, and nothing named
  `.modsync*` was left beside them.

Two Rust-only cases (`RUST_ONLY`, C# has no `conflicts` verb) expect exit 1, the
message and an unchanged fixture: `conflicts_game_missing` (`conflicts -i g.toml -s
mods -g nogame`, `Game directory not found: nogame`) and `conflicts_input_missing`
(`conflicts -i nope.toml -s mods -g game`, `Input file not found: nope.toml`). 63
checks in all.

Known differences (`KNOWN` in the probe) name Rust's documented result; a check fails
when Rust shows anything else:

- `plain_mods_missing/message`: C#'s plain `validate` ignores `-s`/`-g` and fails its
  FOMOD stage; Rust uses the folders and says `Source directory not found: nomods`.
  Both exit 1.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run on a copy (without `Logs`) with a stub `Resources/holopatcher` (`exit 2`, needed
by the `--full` control), under `unshare -Urnm` (no network, a tmpfs over `/Override`
when that folder exists), isolated `HOME`/`XDG_*`, with `DOTNET_ROLL_FORWARD=Major`.
Its results are in [csharp-oracle.json](csharp-oracle.json).

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, then without `--record`) | 62 match / 1 known difference / 0 fail |
| this slice, recorded reference | 62 match / 1 known difference / 0 fail |
| `b0483d5` (pre-fix), recorded reference | 44 match / 0 known difference / 19 fail |

On `b0483d5` the 19 failures are `message` checks: the five JSON folder cases
(`No such file or directory (os error 2)`), the two JSON guide cases (`Read nope.toml:
No such file or directory (os error 2)`), the two text folder cases, `input_dir_text`,
the six `install` cases, the two `conflicts` cases (no `not found` line), and the known
`plain_mods_missing` (no `not found` line: equal to C#'s none, but not Rust's
documented result). Exit codes and the unchanged fixtures match on both binaries.

Run:

```sh
python3 -B docs/parity/missing-paths-proof/probe.py --bin <modsync> \
  [--reference <d31ccbd3 build dir> [--record]]
```
