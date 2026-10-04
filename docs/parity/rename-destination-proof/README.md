# Rename Destination proof

Contract: [rename-destination-contract.md](../rename-destination-contract.md).

[probe.py](probe.py) builds 9 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/{a/x.txt, a/sub/keep.txt, b/x.txt, b.txt}`. Component `A` has the
case's Rename step; component `B` copies `b/x.txt` into `Override` and `b.txt` into the
game folder. Every Copy has `Overwrite = true`.

- a placeholder path as the Destination of `Rename <<modDirectory>>/a/x.txt`:
  `<<kotorDirectory>>/Override/z.txt` (`placeholder_game`), `<<modDirectory>>/a/z.txt`
  (`placeholder_mod`) and `<<gameDirectory>>/Override/z.txt` (`placeholder_legacy`);
- `A` copies `a/x.txt` into `Override`, then renames `<<kotorDirectory>>/Override/x.txt`
  to `<<kotorDirectory>>/Override/z.txt` with `Overwrite = true`
  (`placeholder_after_copy`);
- controls: the `placeholder_game` step with Dependencies naming a GUID nothing has
  (`placeholder_excluded`); the same step in an unselected option `O` of `A`, which
  `A`'s only step chooses (`placeholder_unselected_option`); a plain file name,
  `z.txt` (`file_name`);
- unchanged known differences: `sub/z.txt` (`subfolder`) and `../z.txt` (`parent`).

Every run gets `--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`;
- `basic`: `validate ... --output json --dry-run`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it runs its full pipeline first;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 54 in all: `full_dry/decision` and `basic/decision` (exit code zero or not, and
`success`); `full_dry/counts` (`errorCount`, `warningCount`, `passedCount`);
`full_dry/stages` (each stage's name and `passed`); `install` (exit code zero or not,
and every file under `game/` and `mods/` with its text afterwards, C#'s `.modsync`
folder left out); `plain` (Rust's plain `validate` decides as C# `install`).

`KNOWN` lists the documented differences (29 checks: the four report checks of each
placeholder case, the install of `placeholder_after_copy`, and all six checks of
`subfolder` and `parent`). A known check passes only if Rust differs from the reference
and refuses: both reports, plain `validate` and `install` fail, and the files are
unchanged.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build. Its full pipeline's Environment stage needs a HoloPatcher in the build's
`Resources` folder, so the probe copies the build into its temporary directory and
adds `Resources/holopatcher`, a `#!/bin/sh` script that exits 2, as
[full-pipeline-destination-proof](../full-pipeline-destination-proof/README.md) does.
It runs under `unshare -Urnm` (no network, a tmpfs over `/Override` when that folder
exists) with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json). The Rust runs need no network either.

C# results: every case passes every report (`errorCount` 0, all six stages passed).
The four placeholder cases fail the install (`Install of 'A' failed with exit code
UnknownError`): `B` is not installed and `a/x.txt` is not renamed; in
`placeholder_after_copy` `A`'s Copy has already written `Override/x.txt` (`from A`).
The three controls install `A` and `B` (`file_name` leaves `a/z.txt`). `subfolder`
installs `a/sub/z.txt` and `parent` installs `mods/z.txt`.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 25 match / 29 known difference / 0 fail |
| this slice, recorded reference | 25 match / 29 known difference / 0 fail |
| `044fca5` (pre-fix), recorded reference | 18 match / 12 known difference / 24 fail |

On `044fca5` the 24 failures are all six checks of the four placeholder cases: Rust
passed every report and plain `validate` and installed, moving `a/x.txt` to
`game/Override/z.txt` (`placeholder_game`, `placeholder_legacy`), to `mods/a/z.txt`
(`placeholder_mod`), or `Override/x.txt` to `Override/z.txt`
(`placeholder_after_copy`). The controls and the two unchanged differences gave the
same results on both builds. The eight cases other than `placeholder_unselected_option`
were first recorded live with the pre-fix binary; the recording made with the fixed
binary after that case was added left their records unchanged.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/rename-destination-proof/probe.py                       # recorded reference
python3 docs/parity/rename-destination-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of Rust's plain `validate` and `install` output per case. `MODSYNC_PROOF_OUT`,
when set, is the parent of the probe's temporary directory.
