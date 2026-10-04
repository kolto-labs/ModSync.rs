# Directory Sources and wildcard rules proof

Contract: [delete-directory-contract.md](../delete-directory-contract.md).

[probe.py](probe.py) builds 42 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 126 checks (the first 19
fixtures, 57 checks, are the original Delete slice):

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
`OV/[ab].tga`, `MD/d/e/n.txt`, or a ZIP `MD/example.zip`.

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

Known differences (`KNOWN` in the probe) must be refusals in Rust where the
reference accepts, or the check fails:

- `copy_dir_and_file_omitted`, all three checks: Rust requires every Source to
  match (F05-X2); the reference drops the missing literal because another Source
  matched.
- `extract_dir_omitted` dry run: Extract keeps the folder rule, so Rust tries to
  unpack the folder's files and reports an error; the reference skips.
- `extract_bracket_literal` dry run: any missing Extract archive is a
  ComponentValidation error in Rust's full report (also on `2241235`); the
  reference only skips. Validate and install match.

## Results of the follow-up (cloud Linux, 2026-10-04)

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
