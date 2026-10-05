# Environment relations proof

Contract: [environment-relations-contract.md](../environment-relations-contract.md).

[probe.py](probe.py) builds 10 disposable fixtures. Each has `game/{chitin.key,
Override/g.txt}`, `mods/a/{x,y,z}.txt` and a TOML guide; `MD` is `<<modDirectory>>`,
`KD` is `<<kotorDirectory>>`, `OV` is `KD/Override`. Component `A` copies
[`MD/a/x.txt`] -> `OV`, `B` copies `y.txt`, `C` copies `z.txt`; "missing archive" is
Extract [`MD/m1.zip`] (absent), then Copy [`MD/m1/f1.txt`] -> `OV`. Every component is
`IsSelected = true` unless the table says otherwise. Every run, on both sides, is under
`unshare -Urnm`; a read-only folder is bind-mounted read-only there.

| Case | Guide | Selection arguments | C# `validate --full` |
| --- | --- | --- | --- |
| `restrict_all` | `A` Restrictions [`C`], plus `B`, `C` | none (every component) | fails (Environment, components) |
| `restrict_select` | `A` (category X) Restrictions [`C`], `B` (Y), `C` (X) | `--select category:X` | fails (Environment, components) |
| `restrict_mutual` | `A` Restrictions [`C`], `C` Restrictions [`A`] | none | fails (Environment, components) |
| `restrict_missing_archive` | `A` Restrictions [`C`] and extracts a missing archive, `C` | none | fails (Environment, components) |
| `restrict_readonly_mods` | `A` Restrictions [`C`]; `mods` read-only | none | fails (Environment, components) |
| `depends_select` (known) | `A` (X) Dependencies [`B`], `B` (Y) | `--select category:X` | fails (Environment, components) |
| `depends_file_selection` (known) | `A` Dependencies [`B`], `B` not selected | `--use-file-selection` | passes |
| `restrict_option` (known) | `A` Restrictions [option `O` of `C`], `O` selected | none | passes |
| `restrict_unknown` (control) | `A` Restrictions [a GUID naming nothing] | none | passes |
| `ok` (control) | `A` Dependencies [`B`] | none | passes |

C#'s Environment stage runs `holopatcher --install` and expects exit code 2, so the
reference copy gets a stub `Resources/holopatcher` (`exit 2`). Each case runs these ways
on a fresh fixture with its selection arguments, 11 checks per case plus `install` for
eight, 118 in all:

- `full/decision`, `full/counts`, `full/stages`, `full/environment`: `validate -i G
  -s mods -g game --output json --full`: the exit code is zero or not, `success`;
  `errorCount`, `warningCount`, `passedCount`, `componentCount`; each stage's name and
  `passed`; the Environment stage's first message;
- `text/decision`, `text/message`: the same with text output; the exit code is zero
  or not, and whether C#'s `Some components failed to validate.` is in the output;
- `dro/decision`, `dro/counts`, `dro/environment`: `validate ... --output json
  --full --dry-run-only`;
- `basic/decision`: `validate ... --output json --dry-run` (no `--full`);
- `details` (Rust only): the lines Rust lists in the `--full` Environment stage after
  the first message (`ERROR: Cannot install 'A' due to 'C' being selected for
  install.`, or `... not being selected for install.`);
- `install` (Rust only, not for `restrict_missing_archive` and
  `restrict_readonly_mods`): `install -i G -s mods -g game` with the selection
  arguments: the exit code is zero or not, the last `Error:` line, the `Override`
  listing and whether an unfinished install (`.modsync-*/pending`) was left beside the
  folders; expected unchanged: refused before any write (`Installation blocked before
  any write: FOMOD configuration check failed: A conflicts with <GUID>`), except
  `depends_select` and the controls, which install. C#'s `install` runs the full
  pipeline first and is not run here.

Known differences (`KNOWN` in the probe) name Rust's documented result for each check;
a check fails when Rust shows anything else:

- `basic/decision` in `restrict_all`, `restrict_select`, `restrict_mutual`,
  `restrict_readonly_mods`: C#'s basic report passes a restriction conflict; Rust's dry
  run refuses it (fail closed, unchanged).
- `depends_select` (9 checks): C#'s `--select` adds no Dependency, so its Environment
  stage fails; Rust's `--select` adds the dependency closure and passes.
- `depends_file_selection` (10 checks): C#'s loader selects the Dependency
  (AttemptFixes) and passes; Rust keeps the file's selection and its Environment stage
  fails with C#'s message.
- `restrict_option` (10 checks): C#'s loader deselects the restricted option and
  passes; Rust keeps it and its Environment stage fails with C#'s message.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3`
build, run on a copy (without `Logs`) with the stub as `Resources/holopatcher`, under
`unshare -Urnm` (no network, a tmpfs over `/Override` when that folder exists),
isolated `HOME`/`XDG_*`, with `DOTNET_ROLL_FORWARD=Major`. Its results are in
[csharp-oracle.json](csharp-oracle.json).

## Results (cloud Linux, 2026-10-05)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`, then without `--record`) | 85 match / 33 known difference / 0 fail |
| this slice, recorded reference | 85 match / 33 known difference / 0 fail |
| `1ab3d27` (pre-fix), recorded reference | 53 match / 21 known difference / 44 fail |

On `1ab3d27` `restrict_all`, `restrict_select` and `restrict_mutual` each fail
`full/counts` (`[3, 0, 2, 3]`, `[3, 0, 2, 2]`, `[4, 0, 2, 2]` for C#'s `[1, 0, 0, n]`),
`full/stages` (a passed Environment stage, then failed Conflicts, InstallOrder and
FomodConfiguration stages for C#'s `[["Environment", false]]`), `full/environment`,
`dro/counts`, `dro/environment`, `text/message` and `details`.
`restrict_missing_archive` fails five of those (its `--dry-run-only` report already
failed the Environment stage on the missing archive) and `restrict_readonly_mods`
four (the stage failed with the mod folder message). `depends_file_selection` and
`restrict_option` fail seven each, as Rust's documented result for them changed. The
decisions, `install` and the controls match on both binaries.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/environment-relations-proof/probe.py                       # recorded reference
python3 docs/parity/environment-relations-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies; it is not changed (the probe copies it, without `Logs`, and adds the
stub to its copy). `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
