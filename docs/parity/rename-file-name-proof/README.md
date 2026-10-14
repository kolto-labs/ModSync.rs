# Rename file name proof

Contract: [rename-file-name-contract.md](../rename-file-name-contract.md).

[probe.py](probe.py) builds 14 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/{a/x.txt, b/x.txt, b.txt}`. Component `A` renames
`<<modDirectory>>/a/x.txt` to the case's Destination; component `B` copies `b/x.txt`
into `Override` and `b.txt` into the game folder. Every Copy has `Overwrite = true`.

- newly refused: `z..txt` (`dotdot_inside`), `..z.txt` (`dotdot_leading`), `z<TAB>.txt`
  (`control_tab`), `z<U+0001>.txt` (`control_soh`), `./z.txt` (`dot_part`) and
  `z.txt/` (`trailing_separator`);
- already refused by `relative` (unchanged): `z.txt.` (`trailing_dot`), `z.txt `
  (`trailing_space`), `con.txt` (`device_name`) and `z[1].txt` (`brackets`);
- controls: `z.txt` (`file_name`), `z .a.txt` (`dots_and_spaces`), the `z..txt` step
  with Dependencies naming a GUID nothing has (`excluded`), and the `z..txt` step in an
  unselected option `O` of `A`, which `A`'s only step chooses (`unselected_option`).

Every run gets `--use-file-selection` and a fresh copy of the fixture:

- `full_dry`: `validate -i G -s mods -g game --output json --full --dry-run`;
- `basic`: `validate ... --output json --dry-run`;
- `install`: `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
  `--skip-validation`, so it runs its full pipeline first;
- `plain` (Rust only): `validate -i G -s mods -g game`.

Checks, 84 in all: `full_dry/decision` and `basic/decision` (exit code zero or not, and
`success`); `full_dry/counts` (`errorCount`, `warningCount`, `passedCount`);
`full_dry/stages` (each stage's name and `passed`); `install` (exit code zero or not,
and every file under `game/` and `mods/` with its text afterwards, C#'s `.modsync`
folder left out); `plain` (Rust's plain `validate` decides as C# `install`).

The drafter part has 4 cases, each one component whose Directions prose is "Rename the
file old.tga as con.tga." (`draft_device_as`), "... to con.tga." (`draft_device_to`),
"... as new..tga." (`draft_dotdot_as`) or "... as ..new.tga." (`draft_leading_as`),
over `game/{chitin.key, Override/}` and `mods/old.tga`. Per case: `draft` (the
instructions `convert --parse-directions --format json` drafts must be `Rename
[<<modDirectory>>\old.tga] -> <<kotorDirectory>>\Override`), `review` (`convert ...
--review-draft GUID` succeeds), `install` (of the reviewed draft: fails, files
unchanged) and `csharp` (the drafted steps equal the pinned C# drafter's), 16 checks.

`KNOWN` lists the documented differences (63 checks: all six checks of the six newly
refused names except the `install` and `plain` checks of `trailing_separator`, whose C#
install fails too; all six checks of the four names already refused; the four report
checks of `unselected_option`; and `csharp` of `draft_device_to`). A known check passes
only if Rust differs from the reference and refuses: both reports fail and, for an
`install` or `plain` check, plain `validate` and `install` fail with the files
unchanged. The known drafter check passes only if Rust drafts exactly the expected
Rename.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build. Its full pipeline's Environment stage needs a HoloPatcher in the build's
`Resources` folder, so the probe copies the build into its temporary directory and
adds `Resources/holopatcher`, a `#!/bin/sh` script that exits 2, as
[rename-destination-proof](../rename-destination-proof/README.md) does. It runs under
`unshare -Urnm` (no network, a tmpfs over `/Override` when that folder exists) with
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
[csharp-drafts.json](csharp-drafts.json) is the `0-csharp.json` output of
`tools/check_drafts.py` for the cases `probe.py --cases-json` writes, against
`ModSync.Core.dll` from the same build (sha256
`fb8e3e973c21f3712081419c584177794b7374515a3aac188a9a5ca3c228b157`). The Rust runs
need no network or .NET.

C# results: every case passes every report (`errorCount` 0, all six stages passed).
The install renames `a/x.txt` to the name as written for every case except
`dot_part` (`a/z.txt`), `trailing_separator` (the install fails at `A`, nothing
written: `File.Move` to `a/z.txt/` fails) and `excluded` and `unselected_option`
(`a/x.txt` stays); `B` is installed whenever `A` succeeds. The pinned C# drafter drafts
`Rename [<<modDirectory>>\old.tga] -> <<kotorDirectory>>\Override` for the three "as"
cases and adds `<<modDirectory>>\con.tga` as a second Source for "to".

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 37 match / 63 known difference / 0 fail |
| this slice, recorded reference | 37 match / 63 known difference / 0 fail |
| `c2c9002` (pre-fix), recorded reference | 26 match / 24 known difference / 50 fail |

On `c2c9002` the 50 failures are: all six checks of the six newly refused names (36;
Rust passed every report and plain `validate` and installed, renaming to the name, or
to `z.txt` for `./z.txt` and `z.txt/`); the four report checks of `unselected_option`
(Rust's reports passed it); and the drafter's bare names (`draft` and `csharp` of all
four cases, plus `install` of `draft_dotdot_as` and `draft_leading_as`, which renamed
`old.tga` after review: 10). The four names already refused and the other controls
gave the same results on both builds. The C# oracle was first recorded live with the
pre-fix binary; the recording made with the fixed binary was identical.

`tools/check_drafts.py` over these drafter cases: on `c2c9002` 0 differences and 4
intentional Rename differences (Rust drafted the bare names); with this slice 0
differences and 1 intentional (`draft_device_to`, one Source).

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/rename-file-name-proof/probe.py                       # recorded reference
python3 docs/parity/rename-file-name-proof/probe.py --reference <ref> [--record]   # live
python3 docs/parity/rename-file-name-proof/probe.py --cases-json <cases.json>
DOTNET_ROLL_FORWARD=Major python3 tools/check_drafts.py --reference-dll <ref>/ModSync.Core.dll \
  --rust target/release/modsync --output <scratch> --input <cases.json>
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`); `--verbose` prints the last
line of each Rust plain `validate` and `install` (and C# install) output.
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
