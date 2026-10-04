# Copy/Move onto itself proof

Contract: [copy-move-self-contract.md](../copy-move-self-contract.md).

[probe.py](probe.py) builds 16 disposable fixtures (plain files, one read-only,
one symlink) and runs the production `modsync` two ways on a fresh copy of each,
giving 32 checks:

- plain `validate`, Rust's strict install gate: the decision, against the
  reference install;
- `install`: the decision, against the reference install, and for an accepted
  install the final game and mod trees (names and contents; a symlink is
  recorded as `-> target`).

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

## Results (cloud Linux, 2026-10-04)

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
