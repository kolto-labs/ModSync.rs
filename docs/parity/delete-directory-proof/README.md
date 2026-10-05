# Directory Sources and wildcard rules proof

Contract: [delete-directory-contract.md](../delete-directory-contract.md).

[probe.py](probe.py) builds 60 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 180 checks (the first 19
fixtures, 57 checks, are the original Delete slice; 42 fixtures, 126 checks, the
first follow-up; 55 fixtures, 165 checks, the second):

- `validate --output json --dry-run`: the decision (no Error issue) and whether
  the report has the `DryRunSkipped` warning, against the reference dry run;
- plain `validate`, Rust's strict install gate: the decision, against the
  reference install;
- `install`: the decision, against the reference install, and for an accepted
  install the final game and mod files (names and contents).

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
copy-move-self oracle unchanged, built against `ModSync.Core.dll` from
`d31ccbd3`. Dry run uses `DryRunValidator.ValidateInstallationAsync`; install
uses `ModComponent.InstallAsync` on the real file system and passes on
`InstallExitCode.Success`. The oracle also records the final tree after a failed
install; only accepted trees are compared. Each reference run uses its own
fixture and HOME under `unshare -rn` (no network). The Rust runs need no network
either; this probe has no netns guard variable.

Every fixture has `game/chitin.key`, `game/Override/{a.tga, b.2da, s.tga}`,
`game/Override/sub/x.tga`, `game/Override/sub/deeper/y.tga`, the empty directory
`game/Override/empty` and `mods/d/m.txt`. `OV` is `<<kotorDirectory>>/Override`,
`MD` is `<<modDirectory>>`. Some fixtures add a file literally named
`OV/[ab].tga`, `MD/d/e/n.txt`, a ZIP `MD/example.zip`, `MD/[e]xample.zip` or
`MD/n/deep.zip`, a list `MD/d/clean.csv` (`Mandatory Deletions,a.tga`), or an
executable `MD/x/run.sh` that writes `MD/x/ran.txt`. The Execute/Run cases pass
`--allow-processes` to Rust, so that a folder Source would really run.

| Case | Delete step(s) | Reference dry run / install |
| --- | --- | --- |
| `dir_omitted` | `OV/sub`, Overwrite omitted (false for Delete) | pass / pass, nothing deleted |
| `dir_false` | `OV/sub`, false | pass / pass, nothing deleted |
| `dir_true` | `OV/sub`, true | DryRunSkipped / `MissingSourceFiles` |
| `override_omitted` | `OV`, omitted | pass / pass, nothing deleted |
| `override_true` | `OV`, true | DryRunSkipped / fails |
| `mod_dir_omitted` | `<<modDirectory>>/d`, omitted | pass / pass, nothing deleted |
| `dir_trailing_slash_omitted` | `OV/sub/`, omitted | pass / pass, nothing deleted |
| `dir_backslash_omitted` | `<<kotorDirectory>>\Override\sub`, omitted | pass / pass, nothing deleted |
| `dir_case_omitted` | `OV/SUB`, omitted | pass / pass, nothing deleted |
| `dir_and_file_false` | `OV/sub`, `OV/a.tga`, false | pass / pass, only `a.tga` deleted |
| `dir_and_file_true` | `OV/sub`, `OV/a.tga`, true | DryRunSkipped / fails |
| `empty_dir_omitted` | `OV/empty`, omitted | pass / pass |
| `empty_dir_true` | `OV/empty`, true | DryRunSkipped / fails |
| `glob_dir_and_file_omitted` | `OV/s*` (file `s.tga`, directory `sub`), omitted | pass / pass, only `s.tga` deleted |
| `glob_star_omitted` | `OV/*`, omitted | pass / pass, top-level files deleted, `sub/` kept |
| `glob_star_true` | `OV/*`, true | pass / pass, the same |
| `dir_readonly_omitted` | `OV/sub` with `sub/x.tga` read-only, omitted | pass / pass, nothing deleted |
| `dir_then_later_omitted` | `OV/sub`, then `OV/a.tga`, both omitted | pass / pass, only `a.tga` deleted |
| `game_root_omitted` | `<<kotorDirectory>>`, omitted | pass / pass, nothing deleted |
| `doublestar_omitted` | `OV/**` | pass / pass, top-level files deleted, `sub/` kept |
| `doublestar_deep_omitted` | `OV/**/y.tga` (`y.tga` two levels down) | pass / pass, nothing deleted |
| `doublestar_one_level_omitted` | `OV/**/x.tga` | pass / pass, `sub/x.tga` deleted |
| `star_level_omitted` | `OV/*/x.tga` | pass / pass, `sub/x.tga` deleted |
| `dir_glob_only_omitted` | `OV/su*` (only a directory matches) | pass / pass, nothing deleted |
| `question_mark_omitted` | `OV/?.tga` | pass / pass, `a.tga`, `s.tga` deleted |
| `bracket_literal_omitted` | `OV/[ab].tga`, the literal file present | pass / pass, only `[ab].tga` deleted |
| `bracket_absent_omitted` | `OV/[ab].tga`, no such file | pass / pass, nothing deleted |
| `copy_doublestar` | Copy `MD/d/**` -> `OV`, with `d/e/n.txt` | pass / pass, only `m.txt` copied |
| `copy_bracket_literal` | Copy `OV/[ab].tga` -> `MD/d` | pass / pass, the literal file copied |
| `copy_dir_omitted`, `copy_dir_false` | Copy `OV/sub` -> `MD/d` | DryRunSkipped / `MissingSourceFiles` |
| `copy_mod_dir_omitted` | Copy `MD/d` -> `OV` | DryRunSkipped / fails |
| `copy_dir_glob_control` | Copy `MD/d/*` -> `OV` | pass / pass, `m.txt` copied |
| `copy_dir_and_file_omitted` | Copy [`MD/d`, `MD/d/m.txt`] -> `OV` | pass / pass, `m.txt` copied (Rust: known difference, refuses) |
| `move_dir_omitted`, `move_dir_false` | Move `OV/sub` -> `MD/d` | DryRunSkipped / fails |
| `move_override_omitted` | Move `OV` -> `MD/d` | DryRunSkipped / fails |
| `rename_dir_omitted`, `rename_dir_false` | Rename `OV/sub` -> `c.txt` | DryRunSkipped / fails |
| `rename_missing_file_omitted` | Rename `OV/nope.txt` -> `c.txt` | DryRunSkipped / fails |
| `extract_dir_omitted` | Extract `MD/d` -> `MD/out` | DryRunSkipped / fails (Rust dry run: known difference) |
| `extract_bracket_literal` | Extract `MD/[e]xample.zip` with `example.zip` present | DryRunSkipped / fails (Rust dry run: known difference) |
| `extract_bracket_file_literal` | Extract `MD/[e]xample.zip` (so named) -> `MD/bracket` | pass / pass, extracted |
| `extract_bracket_file_default_dest` | the same, no Destination | pass / pass, extracted to `[e]xample/` |
| `extract_nested_dir` | Extract `MD/n` (holds `deep.zip`) -> `MD/out` | DryRunSkipped / fails (Rust accepts: known difference) |
| `rename_wildcard_nomatch` | Rename `OV/nope*.txt` -> `c.txt` | Error / `UnknownError` |
| `rename_literal_and_wildcard_nomatch` | Rename [`OV/nope.txt`, `OV/nope*.txt`] | DryRunSkipped / `MissingSourceFiles` |
| `rename_present_and_wildcard_nomatch` | Rename [`OV/a.tga`, `OV/nope*.txt`] | pass / pass |
| `cleanlist_dir` | CleanList [`MD/d`] -> `OV` | DryRunSkipped / `MissingSourceFiles` |
| `cleanlist_file_control` | CleanList [`MD/d/clean.csv`] -> `OV` | pass / pass, `a.tga` deleted |
| `cleanlist_missing_file` | CleanList [`MD/d/nope.csv`] | DryRunSkipped / fails |
| `execute_dir`, `run_dir` | Execute / Run `MD/x` | Error / `UnknownError`, nothing runs |
| `execute_file_control` | Execute `MD/x/run.sh` | pass / pass, `ran.txt` written |
| `execute_missing_file` | Execute `MD/x/nope.sh` | Error / `UnknownError` |
| `cleanlist_wildcard_nomatch` | CleanList [`MD/d/*.nope`] | Error / `UnknownError` |
| `execute_wildcard_nomatch`, `run_wildcard_nomatch` | Execute / Run [`MD/x/*.nope`] | DryRunSkipped / `MissingSourceFiles` |
| `execute_wildcard_folder_only` | Execute [`MD/*`] (only folders at that level) | DryRunSkipped / `MissingSourceFiles` |
| `execute_literal_and_wildcard_nomatch` | Execute [`MD/x/nope.sh`, `MD/x/*.nope`] | Error / `UnknownError` |

Known differences (`KNOWN` in the probe) must be refusals in Rust where the
reference accepts, or the check fails. `KNOWN_ACCEPTS` (only `extract_nested_dir`,
plain `validate` and `install`; all three checks before extract-destination) must be
acceptances in Rust where the reference refuses or skips: Extract keeps the folder
rule for FOMOD root-folder flows. `KNOWN_LAYOUT` (only
`extract_bracket_file_literal`, install) must be an install in both whose Rust tree is
the reference's with the named folders renamed.

- `copy_dir_and_file_omitted`, all three checks: Rust requires every literal Source
  to match (F05-X2); the reference drops the missing Source because another Source
  matched. `rename_present_and_wildcard_nomatch` matches since
  [wildcard-miss](../wildcard-miss-contract.md): a wildcard Source that matches
  nothing is dropped beside a matching one, as in the reference.
- `extract_dir_omitted` dry run: Extract keeps the folder rule, so Rust tries to
  unpack the folder's files and reports an error; the reference skips.
- `extract_bracket_literal` dry run: any missing Extract archive is a
  ComponentValidation error in Rust's full report (also on `2241235`); the
  reference only skips. Validate and install match.
- Since [extract-destination](../extract-destination-contract.md) (2026-10-05):
  `extract_bracket_file_literal` and `extract_nested_dir` dry runs: Rust's basic
  report fails an Extract with a Destination, as the C# CLI's does
  (ComponentValidation); this reference runs `DryRunValidator` alone and passes.
  `extract_bracket_file_literal` install (`KNOWN_LAYOUT`): Rust, like the C# CLI,
  never uses an Extract Destination and extracts to `[e]xample/`; the reference
  `ModComponent.InstallAsync` alone extracts into `bracket/`.

## Results after wildcard-miss (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| wildcard-miss, recorded reference | 170 match / 10 known difference / 0 fail |
| `f6d2ec5` (pre-fix), recorded reference | 167 match / 10 known difference / 3 fail |

`rename_present_and_wildcard_nomatch` (dry run, validate, install) now matches the
reference: Rust renames `a.tga` and drops `nope*.txt`.

## Results after extract-destination (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| extract-destination, recorded reference | 167 match / 13 known difference / 0 fail |

## Results of the third follow-up (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| third follow-up, live reference (`--csharp ... --record`) | 169 match / 11 known difference / 0 fail |
| third follow-up, recorded reference | 169 match / 11 known difference / 0 fail |
| `ab6b389` (the second follow-up) | 165 match / 11 known difference / 4 fail |
| `df67229` | 156 match / 11 known difference / 13 fail |

On `ab6b389` the dry runs of `cleanlist_wildcard_nomatch` (it passed with
`DryRunSkipped`), `execute_wildcard_nomatch`, `run_wildcard_nomatch` and
`execute_wildcard_folder_only` (they failed with an error) differ from the reference.

## Results of the second follow-up (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| second follow-up, live reference (`--csharp ... --record`) | 154 match / 11 known difference / 0 fail |
| second follow-up, recorded reference | 154 match / 11 known difference / 0 fail |
| `df67229` (the first follow-up) | 142 match / 11 known difference / 12 fail |
| `4fb71a6` | 103 match / 7 known difference / 55 fail |
| `2241235` | 93 match / 7 known difference / 65 fail |

On `df67229` the 12 failures are: `rename_wildcard_nomatch` dry run (it passed with
`DryRunSkipped`); `cleanlist_dir`, all three (it read `clean.csv` from the folder
and deleted `a.tga`); `execute_dir` and `run_dir`, all three each (with
`--allow-processes` Rust ran `x/run.sh`); and the dry runs of
`execute_missing_file` (typed as a skip) and `cleanlist_missing_file` (an error).

## Results of the first follow-up (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| follow-up, live reference (`--csharp ... --record`) | 121 match / 5 known difference / 0 fail |
| follow-up, recorded reference | 121 match / 5 known difference / 0 fail |
| `4fb71a6` (the first slice) | 88 match / 1 known difference / 37 fail |
| `2241235` (pre-fix) | 78 match / 1 known difference / 47 fail |

On `4fb71a6` the new cases fail: `OV/**` and `OV/**/y.tga` delete into
subfolders; `[ab].tga` deletes `a.tga` instead of the literal file (or `a.tga`
when the literal is absent); `Copy MD/d/**` also copies `d/e/n.txt`; Extract
`[e]xample.zip` extracts `example.zip`; Copy, Move and Rename of a directory are
accepted and copy, move or overwrite the subtree; a missing Rename Source is an
error in the dry-run report instead of `DryRunSkipped`. `copy_dir_and_file_omitted`
also fails there, because `4fb71a6` accepted it (the folder expanded to the file
that was also named); see the known differences. `2241235` additionally fails the
10 Delete-directory installs of the first slice.

## Results of the first slice (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| first slice, live reference (`--csharp ... --record`) | 57 match / 0 fail |
| first slice, recorded reference | 57 match / 0 fail |
| `2241235` (pre-fix) | 47 match / 10 fail |

On `2241235` the install of every directory case with `Overwrite` false or
omitted fails the tree check: Rust deleted every file under the directory
(`override_omitted` left only `chitin.key`; `mod_dir_omitted` removed
`mods/d/m.txt`; `dir_readonly_omitted` removed the read-only file too). The
`Overwrite = true` cases, the wildcard cases and `game_root_omitted` match in
both builds, as do every dry-run and plain-validate decision.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/delete-directory-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/delete-directory-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/delete-directory-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
