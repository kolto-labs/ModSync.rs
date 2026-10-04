# Install order proof

Contract: [install-order-contract.md](../install-order-contract.md).

[probe.py](probe.py) builds 44 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/` with `X.txt` and `X/z.txt` (holding `X`) for each component key.
Component `X` copies both into `Override`, so the final `Override/z.txt` names the last
component installed. Each side runs on a fresh copy, with `--use-file-selection`
except under `--direct-markdown` (which refuses it):

- `install`: `install -i G -s mods -g game`, plus `-y --skip-validation
  --no-checkpoint` for C# (it asks for confirmation, and its environment check needs a
  packaged HoloPatcher) and `--no-checkpoint` for Rust;
- `direct`: the same install with `--direct-markdown --patcher-engine OdyPatcher`
  (Rust `ody-patcher`), without `--use-file-selection`;
- `phase_base` (Rust only): the Rust install with `--phase base`;
- `validate`: `validate -i G -s mods -g game --output json --dry-run`;
- `rootless`: `validate -i G --output json`, the report without roots.

Checks, 162 in all:

- install: `decision` (exit code zero or not), `order` (the components in the order
  they were installed: C# `[i/n] Installing: X`, Rust `X / n: Action` progress lines)
  and `files` (the game folder's files outside `.modsync`, with contents);
- validate and rootless: `decision` (exit code and `success`), `shape` (`report` when
  stages were written, `error` when the run stopped before them), `errors` (the N of
  `Dependency resolution failed with N errors`), `listing` (the `ComponentValidation`
  messages in order) and `dryrun` (the `DryRun` stage messages).

Cases:

- Install order (10): InstallAfter, Dependencies, InstallBefore, a link through an
  unselected component, a chain, a guide the resolver reorders, mixed edges, two
  InstallAfter targets, JSON and YAML input.
- Widescreen-only components (4): one with no base component after it, one a base
  component must follow, one listed first, two around a base link. All take their
  breadth-first place, as in the C# CLI.
- Controls (3): no references, an already ordered guide, a Dependencies target listed
  after its dependant.
- Report listing (3): the resolver's order in the archive stage.
- Refusal (12): missing InstallAfter and InstallBefore, a self reference, cycles
  among unselected components and with one unselected member, on unselected
  components; missing and circular references on selected ones; JSON input; the report
  without roots (two); install (two).
- Markdown (7): a missing reference on an unselected component, a Dependencies link,
  and InstallAfter, backward InstallAfter and InstallBefore references, which C# drops,
  in `.md`, `.txt` and extensionless files.
- Direct Markdown (2): a Dependencies link and a backward InstallAfter; both keep the
  source order.
- Dry run (1): a component that moves the file a later-installed component creates
  (known difference).
- Rust-only `--phase base` (2): a base component that must follow a widescreen-only
  one through an unselected component (refused before any write), and a control that
  installs the base build. These are checked against the documented Rust result in
  `EXPECTED`, not against C#.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network either.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 161 match / 1 known difference / 0 fail |
| this slice, recorded reference | 161 match / 1 known difference / 0 fail |
| `174482a` (first version of this slice), recorded reference | 138 match / 1 known difference / 23 fail |
| `e4c0b4d` (pre-fix), recorded reference | 77 match / 0 known difference / 85 fail |

On `e4c0b4d` every install-order case installed the stable order (for `A`, `B` after
`A`, `C`: `A, B, C` where C# gives `A, C, B`, so `Override/z.txt` held `c` instead of
`b`), the report listed components in file order, every refusal case either validated
and installed where C# stopped or refused inside the report where C# stopped before
it, and `--phase base` installed a component it must refuse. The pre-fix build matched
the dry run case, which is a known difference after the fix (see the contract). On
`174482a` the widescreen, Markdown, direct Markdown and `--phase base` cases fail.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/install-order-proof/probe.py                       # recorded reference
python3 docs/parity/install-order-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
