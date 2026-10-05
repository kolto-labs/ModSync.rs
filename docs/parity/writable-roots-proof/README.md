# Writable roots proof

Contract: [writable-roots-contract.md](../writable-roots-contract.md).

[probe.py](probe.py) builds 6 disposable fixtures. Each has `game/{chitin.key,
Override/g.txt}`, `mods/a/x.txt` and a TOML guide with one component `A`; `MD` is
`<<modDirectory>>`, `KD` is `<<kotorDirectory>>`, `OV` is `KD/Override`. "Copy" is
Copy [`MD/a/x.txt`] -> `OV`; "missing archive" is Extract [`MD/m1.zip`] (absent),
then Copy [`MD/m1/f1.txt`] -> `OV`. Every run, on both sides, is under `unshare
-Urnm`, where the case's folders are bind-mounted read-only (`mount --bind`, then
`mount -o remount,bind,ro`), so even the namespace root cannot write to them:

| Case | Read-only | Steps | C# `validate --full` |
| --- | --- | --- | --- |
| `game_readonly` | `game` | Copy | fails (Environment, Install directory) |
| `mods_readonly` | `mods` | Copy | fails (Environment, Mod directory) |
| `both_readonly` | `mods`, `game` | Copy | fails (Environment, Mod directory) |
| `game_readonly_missing_archive` (known) | `game` | missing archive | fails (Environment, components) |
| `ok` (control) | none | Copy | passes |
| `override_readonly` (control) | `game/Override` | Copy | passes |

C#'s Environment stage runs `holopatcher --install` and expects exit code 2, so the
reference copy gets a stub `Resources/holopatcher` (`exit 2`). Each case runs these
ways on a fresh fixture, giving 9 checks per case plus `install` for five, 59 in all:

- `full/decision`, `full/counts`, `full/stages`, `full/environment`: `validate -i G
  -s mods -g game --output json --full`: the exit code is zero or not, `success`;
  `errorCount`, `warningCount`, `passedCount`; each stage's name and `passed`; the
  Environment stage's first message;
- `text/decision`: the same with text output; the exit code is zero or not;
- `dro/decision`, `dro/counts`, `dro/environment`: `validate ... --output json
  --full --dry-run-only`;
- `details` (Rust only): the lines Rust lists in the `--dry-run-only` Environment
  stage after the first message, compared with the expected `ERROR: A: <error>` lines
  in the probe;
- `install` (Rust only, not for `override_readonly`): `install -i G -s mods -g game`:
  the exit code is zero or not, the last `Error:` line, the `Override` listing and
  whether an unfinished install (`.modsync-*/pending`, a snapshot to restore) was
  left beside the folders. The expected result is in the probe: refused with
  `Installation blocked before any write: <C#'s message>`, nothing written, nothing to
  restore. C#'s `install` runs the full pipeline first and refuses the same cases; it
  is not run here, because its confirmation callback then tries chmod, chown and sudo
  on the folder.

Known difference (`KNOWN` in the probe), which must show the documented Rust result
or the check fails:

- `game_readonly_missing_archive` (`full/environment`): C#'s Environment stage also
  runs the component checks, and their message (`Some components failed to validate.
  ...`) replaces the read-only one; Rust's full report runs them later, in its
  ComponentValidation stage, so its Environment stage gives the Install directory
  message. Under `--dry-run-only` both give the component message.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy (without `Logs`) with the stub as `Resources/holopatcher`, under
`unshare -Urnm` (no network, a tmpfs over `/Override` when that folder exists),
isolated `HOME`/`XDG_*`, with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need `unshare` for the
read-only mounts but no network or .NET.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, then without `--record`) | 58 match / 1 known difference / 0 fail |
| this slice, recorded reference | 58 match / 1 known difference / 0 fail |
| `2de5a47` (pre-fix), recorded reference | 28 match / 0 known difference / 31 fail |

On `2de5a47` the three read-only cases passed every report (exit 0, five or six
stages passed) and fail all eight report checks each; their `install` fails too: for
`game_readonly` and `both_readonly` Rust took its snapshot and stopped at the first
write (`Install stopped. Restore before retrying; ... Read-only file system (os error
30)`), leaving an install to restore, and `mods_readonly` installed. In
`game_readonly_missing_archive` the full report failed in ComponentValidation, not
Environment, so `full/counts`, `full/stages` and the known `full/environment` fail,
and its `install` refused the missing archive instead of the folder. The controls are
the same on both binaries.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/writable-roots-proof/probe.py                       # recorded reference
python3 docs/parity/writable-roots-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe copies it, without `Logs`, and adds the
stub to its copy). `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
