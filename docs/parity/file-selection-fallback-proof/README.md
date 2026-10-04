# File selection fallback proof

Contract: [file-selection-fallback-contract.md](../file-selection-fallback-contract.md).

[probe.py](probe.py) builds 20 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/{m.txt, n.txt}`, and a guide whose components copy those files
into `Override` (one copies `absent.txt`, which does not exist, so validating it shows
in the counts). Each side runs on a fresh copy:

- `validate`: `validate -i G -s mods -g game --output json --dry-run [flags]`;
- `rootless`: `validate -i G --output json [flags]`, the report without roots;
- `install`: `install -i G -s mods -g game [flags]`, plus `-y --skip-validation` for
  C# (it asks for confirmation, and its environment check needs a packaged
  HoloPatcher) and `--no-checkpoint` for Rust.

Checks, 58 in all:

- `decision`: exit code and `success` (install: exit code zero or not);
- `counts`: `errorCount`, `warningCount`, `passedCount`, `componentCount`;
- `stages`: each stage's name and `passed`, with the `ComponentValidation` messages,
  or the setup `error` when there is no report;
- `files` (install): the game folder's files outside `.modsync`.

Gap cases (11): all-false TOML with one or two components, with a missing source in
the second, with `IsSelected` omitted, with a selected or unselected option under an
unselected component, all-false JSON (a top-level array) and YAML, the rootless report
twice, and install. Controls (9): some components selected, no flag, and Markdown
(HTML-comment blocks), which keeps its flags on both sides.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network either. There are no known-difference checks.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 58 match / 0 fail |
| this slice, recorded reference | 58 match / 0 fail |
| `a0df4b7` (pre-fix), recorded reference | 28 match / 30 fail |

On `a0df4b7` every gap case fails: validate printed `No components are marked
IsSelected in the instruction file.` (exit 1, `componentCount` 0) where C# validated
every component, and install stopped with `FOMOD configuration check failed: Select at
least one component` where C# installed `Override/m.txt` and `Override/n.txt`. The
rootless cases fail on counts and stages only, because both sides exit 1 there (C#
fails the FOMOD stage without a mod directory).

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/file-selection-fallback-proof/probe.py                       # recorded reference
python3 docs/parity/file-selection-fallback-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary
directory.
