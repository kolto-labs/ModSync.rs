# Extract Destination proof

Contract: [extract-destination-contract.md](../extract-destination-contract.md).

[probe.py](probe.py) builds 13 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/{m.zip, b.txt, out/keep.txt}`; `m.zip` holds `Override/a.tga`
and `readme.txt`. Component `A` extracts `<<modDirectory>>/m.zip` with the case's
Destination (`OV` is `<<kotorDirectory>>/Override`); component `B` copies `b.txt`
into `Override`. Every Move and Copy has `Overwrite = true`.

- a Destination and nothing after it: `OV` (`override`), `<<kotorDirectory>>\Override`
  (`override_backslash`), `<<kotorDirectory>>` (`game_root`), `<<modDirectory>>/out`
  (`mod_folder`), `<<modDirectory>>/new`, missing (`missing_folder`), and `OV` on the
  Extract of a selected option `O` that `A`'s only step chooses (`option_override`);
- a later step written for C#'s layout: `OV` or `<<modDirectory>>/out`, then `Move
  <<modDirectory>>/m/Override/* -> OV` (`override_then_move`, `mod_folder_then_move`);
- a later step that names the Destination: `<<modDirectory>>/out`, then `Copy
  <<modDirectory>>/out/Override/a.tga -> OV` (`mod_folder_then_copy`);
- unchanged: `<<modDirectory>>/../outside` (`outside_roots`, Rust refuses the path);
- controls: no Destination, then the Move (`no_destination`); `<<modDirectory>>`, the
  archive's own folder, then the Move (`archive_folder`); and no Destination, then
  `Delete <<modDirectory>>/m/readme.txt` with Destination `OV` (`delete_destination`).

Every run gets `--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`;
- `basic`: `validate ... --output json --dry-run`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it runs its full pipeline first;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 91 in all: `full_dry/decision` and `basic/decision` (exit code zero or not,
and `success`); `full_dry/counts` and `basic/counts` (`errorCount`, `warningCount`,
`passedCount`); `full_dry/stages` (each stage's name and `passed`); `install` (exit
code zero or not, and every file under `game/` and `mods/` with its text afterwards,
the archive as `<archive>`, C#'s `.modsync` folder left out); `plain` (Rust's plain
`validate` decides as C# `install`).

`KNOWN` lists the documented differences (9 checks): every check of `outside_roots`
but `basic/decision` (both basic reports fail it), which passes only if Rust refuses
(both reports fail and plain `validate` and `install` fail with the files unchanged);
and `full_dry/counts`, `full_dry/stages` and `basic/counts` of `mod_folder_then_copy`,
which pass only if both reports fail: C# fails the later Copy's Source in
`VerifyExtractPaths` (two `Failed to find ... in any archives!` errors) and, in the
full pipeline, in its Environment stage, where Rust gives one error in its
ComponentValidation stage.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build. Its full pipeline's Environment stage needs a HoloPatcher in the build's
`Resources` folder, so the probe copies the build into its temporary directory and
adds `Resources/holopatcher`, a `#!/bin/sh` script that exits 2, as
[rename-file-name-proof](../rename-file-name-proof/README.md) does. It runs under
`unshare -Urnm` (no network, a tmpfs over `/Override` when that folder exists) with
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network or .NET.

C# results: every case with a Destination fails the basic report with one error
(`'Destination' key cannot be used with this action`, `ComponentValidation` failed,
`errorCount` 1), and passes the full report (`errorCount` 0, all six stages passed),
except `mod_folder_then_copy`. Every install extracts to `mods/m/` and installs `B`,
whatever the Destination, except `mod_folder_then_copy`, which validation refuses
before any write. `no_destination` passes everything.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 82 match / 9 known difference / 0 fail |
| this slice, recorded reference | 82 match / 9 known difference / 0 fail |
| `c0e9cef` (pre-fix), recorded reference | 44 match / 6 known difference / 41 fail |

The 41 failures on `c0e9cef` are listed in the contract. The C# oracle was first
recorded live with an intermediate build of this slice; the recording made with the
final binary was identical.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/extract-destination-proof/probe.py                       # recorded reference
python3 docs/parity/extract-destination-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of each Rust plain `validate` and `install` (and C# install) output.
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
