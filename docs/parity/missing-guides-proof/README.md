# Missing guides proof

Contract: [missing-guides-contract.md](../missing-guides-contract.md).

[probe.py](probe.py) builds a disposable fixture per run: a TOML guide `g.toml`
(component `A` copies `<<modDirectory>>/a/x.txt` to `<<kotorDirectory>>/Override`)
and an empty folder `adir`. Each command runs in the fixture with the relative paths
below, under `unshare -Urnm` on both sides, with `HOME` and `XDG_*` inside the fixture.
`nope.toml` and `nope2.toml` do not exist.

| Case | Command | C# |
| --- | --- | --- |
| `convert_missing` | `convert -i nope.toml` | `Input file not found: nope.toml` |
| `convert_missing_out` | `convert -i nope.toml -o out.toml` | same, no `out.toml` |
| `convert_missing_json` | `convert -i nope.toml -f json -o out.json` | same, no `out.json` |
| `convert_dir` | `convert -i adir -o out.toml` | `Input file not found: adir` |
| `merge_existing_missing` | `merge -e nope.toml -n g.toml -o out.toml` | `Existing file not found: nope.toml` |
| `merge_incoming_missing` | `merge -e g.toml -n nope.toml -o out.toml` | `Incoming file not found: nope.toml` |
| `merge_both_missing` | `merge -e nope.toml -n nope2.toml` | `Existing file not found: nope.toml` |
| `merge_existing_dir` | `merge -e adir -n g.toml` | `Existing file not found: adir` |
| `merge_incoming_dir` | `merge -e g.toml -n adir -o out.toml` | `Incoming file not found: adir` |
| `legacy_existing_missing` | `convert --merge -e nope.toml -n g.toml -o out.toml` | `Existing file not found: nope.toml` |
| `legacy_incoming_missing` | `convert --merge -e g.toml -n nope.toml` | `Incoming file not found: nope.toml` |
| `convert_ok` (control) | `convert -i g.toml -o out.toml` | writes `out.toml` |
| `merge_ok` (control) | `merge -e g.toml -n g.toml -o out.toml` | writes `out.toml` |

Every C# case except the controls exits 1. Three checks per case:

- `exit`: the exit code is zero or not;
- `message`: the first `Input file not found: <path>`, `Existing file not found:
  <path>` or `Incoming file not found: <path>` text in stdout and stderr (none when
  there is none);
- `written`: the names that appeared in the fixture's top folder (the output file, or
  nothing).

Two Rust-only cases (`RUST_ONLY`, C# has no positional forms) expect exit 1, the
message and nothing written: `convert_positional_missing` (`convert nope.toml
out.toml`, `Input file not found: nope.toml`) and `merge_positional_incoming_missing`
(`merge g.toml nope.toml out.toml`, `Incoming file not found: nope.toml`). 45 checks in
all. There are no known differences; C#'s stream and log prefix (`[<time>] [Error]`
on stdout, Rust `Error:` on stderr) are outside the compared text.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run on a copy (without `Logs`) with a stub `Resources/holopatcher` (`exit 2`, unused
here), under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists), isolated `HOME`/`XDG_*`, with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json).

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ...`) | 45 match / 0 known difference / 0 fail |
| this slice, recorded reference | 45 match / 0 known difference / 0 fail |
| `5796295` (pre-fix), live reference (`--reference ... --record`) | 32 match / 0 known difference / 13 fail |
| `5796295` (pre-fix), recorded reference | 32 match / 0 known difference / 13 fail |

On `5796295` the 13 failures are the `message` checks of the eleven C# cases with a
missing guide or folder and of the two Rust-only cases: Rust printed `Read <path>: No
such file or directory (os error 2)` (or, for `adir`, `Read adir: Archive source must
be a regular file, not a symlink or special file`), which has no `file not found`
text. Exit codes and the written names match on both binaries.

Run:

```sh
python3 -B docs/parity/missing-guides-proof/probe.py --bin <modsync> \
  [--reference <d31ccbd3 build dir> [--record]]
```
