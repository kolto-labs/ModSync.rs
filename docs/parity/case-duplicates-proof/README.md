# Case duplicates proof

Contract: [case-duplicates-contract.md](../case-duplicates-contract.md).

[probe.py](probe.py) builds 9 disposable fixtures. Each has `game/{chitin.key,
Override/g.txt}`, `mods/a/x.txt`, the case's extra files and a TOML guide with one
component `A`; `MD` is `<<modDirectory>>`, `KD` is `<<kotorDirectory>>`, `OV` is
`KD/Override`. "Copy" is Copy [`MD/a/x.txt`] -> `OV`; "missing archive" is Extract
[`MD/m1.zip`] (absent), then Copy [`MD/m1/f1.txt`] -> `OV`. Every run, on both sides,
is under `unshare -Urnm`; a read-only folder is bind-mounted read-only there.

| Case | Extra files | Steps | C# `validate --full` |
| --- | --- | --- | --- |
| `game_files` | `game/Override/a.txt`, `game/Override/A.txt` | Copy | fails (Environment, duplicates) |
| `game_folders` | `game/override/b.txt` (beside `Override`) | Copy | fails (Environment, duplicates) |
| `game_nested` | `game/Override/sub/t.tga`, `.../sub/T.TGA` | Copy | fails (Environment, duplicates) |
| `game_files_missing_archive` | as `game_files` | missing archive | fails (Environment, duplicates) |
| `game_files_mods_readonly` | as `game_files`, `mods` read-only | Copy | fails (Environment, duplicates) |
| `both_files` | as `game_files`, plus `mods/a/X.TXT` | Copy | fails (Environment, duplicates) |
| `mods_files` (known) | `mods/a/X.TXT` | Copy | passes |
| `ok` (control) | none | Copy | passes |
| `ok_readonly_mods` (control) | `mods` read-only | Copy | fails (Environment, Mod directory) |

C#'s Environment stage runs `holopatcher --install` and expects exit code 2, so the
reference copy gets a stub `Resources/holopatcher` (`exit 2`). Each case runs these ways
on a fresh fixture, 11 checks per case plus `install` for eight, 107 in all:

- `full/decision`, `full/counts`, `full/stages`, `full/environment`: `validate -i G
  -s mods -g game --output json --full`: the exit code is zero or not, `success`;
  `errorCount`, `warningCount`, `passedCount`, `componentCount`; each stage's name and
  `passed`; the Environment stage's first message;
- `text/decision`, `text/message`: the same with text output; the exit code is zero
  or not, and whether C#'s duplicate message is in the output;
- `dro/decision`, `dro/counts`, `dro/environment`: `validate ... --output json
  --full --dry-run-only`;
- `basic/decision`: `validate ... --output json --dry-run` (no `--full`);
- `details` (Rust only): the lines Rust lists in the `--full` Environment stage after
  the first message (`ERROR: <root>/<path> has a duplicate, please resolve before
  attempting an install.`), compared with the probe's expectation without case,
  because which spelling Rust's scan meets second depends on the directory listing;
- `install` (Rust only, not for `ok_readonly_mods`): `install -i G -s mods -g game`:
  the exit code is zero or not, the last `Error:` line, the `Override` listing and
  whether an unfinished install (`.modsync-*/pending`) was left beside the folders;
  expected unchanged: refused with `Case-colliding files: <path>` (or `directories`),
  nothing written, nothing to restore. C#'s `install` runs the full pipeline first and
  refuses the same cases; it is not run here.

Known differences (`KNOWN` in the probe), which must show the documented Rust result
or the check fails:

- `mods_files` (7 checks: the decisions, counts and stages): C# checks only the game
  folder and passes; Rust's scan refuses the mod folder's duplicate before any report
  (no stages, zero counts), as before.
- `basic/decision` in `game_files`, `game_folders`, `game_nested`,
  `game_files_mods_readonly`, `both_files`: C#'s basic report has no Environment stage
  and passes; Rust's scan refuses the tree in every mode. (`game_files_missing_archive`
  fails in both, so it matches.)

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy (without `Logs`) with the stub as `Resources/holopatcher`, under
`unshare -Urnm` (no network, a tmpfs over `/Override` when that folder exists),
isolated `HOME`/`XDG_*`, with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json).

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, then without `--record`) | 95 match / 12 known difference / 0 fail |
| this slice, recorded reference | 95 match / 12 known difference / 0 fail |
| `0f41c1f` (pre-fix), recorded reference | 53 match / 12 known difference / 42 fail |

On `0f41c1f` the six game-duplicate cases each fail `full/counts` (`[0, 0, 0, 0]` for
C#'s `[1, 0, 0, 1]`), `full/stages` (none for `[["Environment", false]]`),
`full/environment`, `dro/counts`, `dro/environment`, `text/message` and `details`. The
decisions match (both fail), as do `install` and the controls.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/case-duplicates-proof/probe.py                       # recorded reference
python3 docs/parity/case-duplicates-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe copies it, without `Logs`, and adds the
stub to its copy). `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
