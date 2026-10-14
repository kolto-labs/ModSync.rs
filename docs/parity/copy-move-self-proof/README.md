# Copy/Move onto itself proof

Contract: [copy-move-self-contract.md](../copy-move-self-contract.md).

[probe.py](probe.py) builds 21 disposable fixtures (plain files, one read-only,
one symlink, four with a ZIP archive declared in `ResourceRegistry`) and runs the
production `modsync` two ways on a fresh copy of each. That gives 42 checks, plus
13 install-warning checks, 55 in all:

- plain `validate`, Rust's strict install gate: the decision, against the
  reference install;
- `install`: the decision, against the reference install, and for an accepted
  install the final game and mod trees (names and contents; a symlink is
  recorded as `-> target`, a ZIP as its member list);
- install warnings, wherever the reference install succeeds (except the symlink
  case): the number of Rust install stdout lines `WARNING: Destination file
  already exists and overwrite is false: <path>` must equal the number of
  reference `[Warning] ... skipping file. Reason: Overwrite set to False` log
  lines.

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), the
delete-strict oracle unchanged, built against `ModSync.Core.dll` from `d31ccbd3`.
Install uses `ModComponent.InstallAsync` on the real file system and passes on
`InstallExitCode.Success`. The oracle also records the reference dry run
(`DryRunValidator.ValidateInstallationAsync`) and, for every install, the final
tree, including after a failure. Neither is compared; the contract uses them in
its notes. Each reference run uses its own fixture and HOME under `unshare -rn`
(no network). The Rust runs need no network either; this probe has no netns
guard variable.

Every fixture has `game/Override/x.txt` = `X` and `mods/after.txt`. Each guide
ends with `Copy <<modDirectory>>/after.txt -> Override` (Overwrite omitted), a
marker that the component went on after the step under test. `OV` is
`<<kotorDirectory>>/Override`.

| Case | Step under test | Reference install |
| --- | --- | --- |
| `copy_false` | `Copy OV/x.txt -> OV`, false | success, `x.txt` kept |
| `copy_true` | the same, true | `UnknownError`, `x.txt` deleted |
| `copy_omitted` | the same, omitted (true) | `UnknownError`, `x.txt` deleted |
| `move_false` | `Move OV/x.txt -> OV`, false | success, `x.txt` kept |
| `move_true` | the same, true | `UnknownError`, `x.txt` deleted |
| `move_omitted` | the same, omitted (true) | `UnknownError`, `x.txt` deleted |
| `copy_wildcard_false` | `Copy OV/*.txt -> OV` (`x.txt`, `y.txt`), false | success, both kept |
| `copy_mixed_false` | `Copy [OV/x.txt, mods/z.txt] -> OV`, false | success, `z.txt` copied |
| `move_mixed_false` | `Move [OV/x.txt, mods/z.txt] -> OV`, false | success, `z.txt` moved |
| `copy_mod_directory_false` | `Copy mods/m.txt -> <<modDirectory>>`, false | success |
| `copy_subdirectory_false` | `Copy OV/sub/s.txt -> OV/sub`, false | success |
| `copy_readonly_false` | `copy_false` with `x.txt` read-only | success |
| `copy_source_case_false` | `Copy OV/X.TXT -> OV`, false | success, `x.txt` kept |
| `copy_destination_case_false` | `Copy OV/x.txt -> <<kotorDirectory>>/OVERRIDE`, false | success, `x.txt` kept |
| `copy_symlink_false` | `Copy OV/l.txt -> OV`, false; `l.txt` → `x.txt` | success (known difference) |
| `rename_self_false` | `Rename OV/x.txt -> x.txt`, false | `UnknownError` (control) |
| `copy_collision_false` | `Copy mods/x.txt -> OV`, false (an ordinary collision) | success, `x.txt` kept |
| `copy_recovered_mixed_false` | `Copy [mods/Mod/z.txt, OV/x.txt] -> OV`, false; `z.txt` only in `mods/Mod.zip` | success, `x.txt` kept (tree: known difference) |
| `move_recovered_mixed_false` | the same, Move | success, `x.txt` kept (tree: known difference) |
| `copy_recovered_mixed_true` | the same Copy, true | `UnknownError`, `x.txt` deleted |
| `move_recovered_mixed_omitted` | the same Move, omitted (true) | `UnknownError`, `x.txt` deleted |

The four `recovered` fixtures declare `ResourceRegistry."Mod.zip".Files."z.txt" =
true`. Rust's miss-only recovery binds `Mod/z.txt` from the archive; the
reference's direct binding drops the missing literal because the other source
exists. Both accept the `false` cases and keep `x.txt`, but Rust also extracts
`Mod/` and copies or moves `z.txt`. That whole-tree difference is reported as
known (not the self-target rule; see the contract). The check still fails if the
decisions differ or `Override/x.txt` changes.

## Results after the follow-up (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| follow-up, live reference (`--csharp ... --record`) | 51 match / 4 known difference / 0 fail |
| follow-up, recorded reference | 51 match / 4 known difference / 0 fail |
| `6b8baff` (pre-fix) | 36 match / 2 known difference / 17 fail |
| follow-up, the probe as it was at `6b8baff` | 30 match / 2 known difference / 0 fail |

On `6b8baff` the two recovered `false` cases fail validate and install with
`Source equals destination` and give no install warning, and every other accepted
case gives 0 install warnings where the reference logs 1 (2 for the wildcard).
The recovered `true`/omitted cases refuse in both builds. The four known
differences are the two symlink checks and the two recovered `false` trees.

## Results of the first slice (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 30 match / 2 known difference / 0 fail |
| this slice, recorded reference | 30 match / 2 known difference / 0 fail |
| `354055b` (pre-fix) | 10 match / 2 known difference / 20 fail |

On `354055b` every accepted `Overwrite = false` case except the symlink fails
both checks with `Source equals destination`. The `Overwrite = true` and omitted
cases and the Rename control match in both builds. The two known differences are
`copy_symlink_false`: Rust refuses any symlink in an install tree (a deliberate
fail-closed rule, `src/engine.rs:399`), where C# copies nothing and succeeds.
The probe fails if Rust ever accepts that case.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/copy-move-self-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/copy-move-self-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/copy-move-self-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
