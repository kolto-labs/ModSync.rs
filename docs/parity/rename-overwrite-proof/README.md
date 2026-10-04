# Rename Overwrite=false proof

Contract: [rename-overwrite-contract.md](../rename-overwrite-contract.md).

[probe.py](probe.py) builds nine disposable fixtures and runs the production
`modsync validate` and `modsync install` on a fresh copy of each. That is 18
checks. Each check compares the accept/reject decision with the reference. For an
accepted install, it also compares the final game tree (file names and contents).

The reference results are in [csharp-oracle.json](csharp-oracle.json). They were
produced by [csharp-oracle/Program.cs](csharp-oracle/Program.cs), built against
`ModSync.Core.dll` from `d31ccbd3`. Validate uses
`DryRunValidator.ValidateInstallationAsync`, and passes when there is no Error
issue. Install uses `ModComponent.InstallAsync` on the real file system, and
passes on `InstallExitCode.Success`. Each reference run uses its own fixture and
HOME under `unshare -rn`.

| Case | Guide after copying `a.txt`, `b.txt` into `Override` | Reference |
| --- | --- | --- |
| `rename_clash_false` | Rename `Override/a.txt` to `b.txt`, Overwrite false | rejects both |
| `rename_clash_default` | same, Overwrite omitted (true) | accepts; `b.txt` = A |
| `rename_clash_true` | same, Overwrite true | accepts; `b.txt` = A |
| `rename_free_false` | only `a.txt` copied; Overwrite false | accepts; `b.txt` = A |
| `rename_clash_one_of_two` | sources `Sub/a.txt` (free) and `Override/a.txt` (clash) | rejects both |
| `rename_clash_wildcard` | source `Override/a.*` | rejects both |
| `rename_clash_then_later` | clash, then Delete `Override/b.txt` | rejects both |
| `copy_clash_false` | Copy `b.txt` again, Overwrite false | accepts; skip |
| `move_clash_false` | Move `b.txt` again, Overwrite false | accepts; skip |

## Results (cloud Linux)

| Binary | Result |
| --- | --- |
| this slice | 18 match / 0 fail |
| `5643276` (pre-fix) | 10 match / 8 fail: every Rename clash case is accepted by validate and install |

On `5643276`, `rename_clash_then_later` installs and deletes the pre-existing
`b.txt`, leaving only `Override/a.txt`. `rename_clash_one_of_two` renames
`Sub/a.txt` and leaves the clashing source in place.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/rename-overwrite-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/rename-overwrite-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/rename-overwrite-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve.
