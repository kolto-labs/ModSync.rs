# Delete with a directory Source proof

Contract: [delete-directory-contract.md](../delete-directory-contract.md).

[probe.py](probe.py) builds 19 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 57 checks:

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
`game/Override/empty` and `mods/d/m.txt`. `OV` is `<<kotorDirectory>>/Override`.

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

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 57 match / 0 fail |
| this slice, recorded reference | 57 match / 0 fail |
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
