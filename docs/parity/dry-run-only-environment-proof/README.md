# Dry-run-only environment proof

Contract: [dry-run-only-environment-contract.md](../dry-run-only-environment-contract.md).

[probe.py](probe.py) builds 13 disposable fixtures. Each has `game/{chitin.key,
Override/g.txt}` and `mods/a/{x.txt, y.tga}`, some cases add `mods/m1.zip` (holding
`f1.txt`) or a patcher folder `mods/p/` (`TSLPatcher.exe`, `tslpatchdata/changes.ini`,
`tslpatchdata/n.tga`), and a TOML guide; `MD` is `<<modDirectory>>`, `KD` is
`<<kotorDirectory>>`, `OV` is `KD/Override`. Component `A` holds the steps; `B` is a
second component that copies `MD/a/y.tga` into `OV`. "Missing archive" is Extract
[`MD/m1.zip`] (absent), then Copy [`MD/m1/f1.txt`] -> `OV`:

| Case | Steps | C# `--full --dry-run-only` |
| --- | --- | --- |
| `missing_archive` | missing archive | fails (Environment) |
| `missing_archive_second` | `B`, then `A` with a missing archive | fails (Environment) |
| `missing_archive_unselected` | as above, `A` not selected (the CLI selects all) | fails (Environment) |
| `loose_not_in_archive` | Extract `MD/m1.zip`, Copy [`MD/m1/nope.txt`] -> `OV` | fails (Environment) |
| `destination_file` | Copy [`MD/a/x.txt`] -> `OV/g.txt/sub` | fails (Environment) |
| `destination_empty` | Copy [`MD/a/x.txt`] -> `""` | fails (Environment) |
| `ok` (control) | Copy [`MD/a/x.txt`] -> `OV` | passes |
| `file_selection_unselected` (control) | as `missing_archive_unselected`, `--use-file-selection` | passes |
| `destination_missing_folder` (control) | Copy [`MD/a/x.txt`] -> `KD/Nope` | passes |
| `patcher_no_destination` (control) | Patcher [`MD/p/TSLPatcher.exe`], no Destination | passes |
| `extract_destination` (control) | Extract [`MD/m1.zip`] -> `OV` | passes |
| `missing_copy_source` (control) | Copy [`MD/a/nope.txt`] -> `OV` | passes |
| `missing_archive_option` (known) | Choose `O`; `O` extracts the missing archive | fails (DryRun) |

C#'s Environment stage runs `holopatcher --install` and expects exit code 2, so the
reference copy gets a stub `Resources/holopatcher` (`exit 2`); no case installs.
Each case runs three ways on a fresh copy, giving 8 checks per case and 104 in all:

- `full_only/decision`, `full_only/counts`, `full_only/stages`,
  `full_only/environment`: `validate -i G -s mods -g game --output json --full
  --dry-run-only`: the exit code is zero or not, `success`; `errorCount`,
  `warningCount`, `passedCount`; each stage's name and `passed`; the Environment
  stage's first message (the fixture folder shown as `<root>`);
- `text/decision`: the same with text output; the exit code is zero or not;
- `only/decision`, `only/counts`: `validate ... --output json --dry-run-only`, no
  `--full` (a control; unchanged);
- `details` (Rust only): the lines Rust lists in the Environment stage after C#'s
  message, compared with the expected `ERROR: A: <error>` lines in the probe.

Known differences (`KNOWN` in the probe) must show the documented Rust result, or
the check fails:

- `missing_archive_option` (`full_only` counts, stages, environment): Rust's
  Environment stage fails an option's missing archive (fail closed), with C#'s message
  and one error; C# passes that stage and fails the option in its dry run.
- `destination_file` (`only` decision and counts, unchanged): without `--full`, C#'s
  virtual dry run passes a Copy below a file; Rust's refuses it.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy with the stub as `Resources/holopatcher` under `unshare -Urnm`
(no network, a tmpfs over `/Override` when that folder exists) with
`DOTNET_ROLL_FORWARD=Major`, as [patcher-destination-proof](../patcher-destination-proof/README.md)
does. Its results are in [csharp-oracle.json](csharp-oracle.json). The Rust runs need
no network or .NET.

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, then without `--record`) | 99 match / 5 known difference / 0 fail |
| this slice, recorded reference | 99 match / 5 known difference / 0 fail |
| `72ed369` (pre-fix), recorded reference | 66 match / 2 known difference / 36 fail |

On `72ed369` the four cases with a missing archive or loose Source passed `--full
--dry-run-only` (exit 0, one `DryRunSkipped` warning) and fail six checks each;
`destination_file` and `destination_empty` failed in the dry run instead of the
Environment stage and fail four each (counts, stages, environment, `details`).
`missing_archive_option` fails four: it matched C# there, and the probe fails a known
difference that shows no difference, and it had no `details` line. The controls and
the `only` checks are the same on both binaries.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/dry-run-only-environment-proof/probe.py                       # recorded reference
python3 docs/parity/dry-run-only-environment-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe adds the stub to its own copy). `--bin`
picks another binary (default `target/release/modsync`). `MODSYNC_PROOF_OUT`, when
set, is the parent of the probe's temporary directory.
