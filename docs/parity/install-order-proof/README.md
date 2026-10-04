# Install order proof

Contract: [install-order-contract.md](../install-order-contract.md).

[probe.py](probe.py) builds 34 disposable fixtures. Each has `game/{chitin.key,
Override/}` and `mods/` with `X.txt` and `X/z.txt` (holding `X`) for each component key.
Component `X` copies both into `Override`, so the final `Override/z.txt` names the last
component installed. Each side runs on a fresh copy, always with
`--use-file-selection`:

- `install`: `install -i G -s mods -g game`, plus `-y --skip-validation
  --no-checkpoint` for C# (it asks for confirmation, and its environment check needs a
  packaged HoloPatcher) and `--no-checkpoint` for Rust;
- `validate`: `validate -i G -s mods -g game --output json --dry-run`;
- `rootless`: `validate -i G --output json`, the report without roots.

Checks, 132 in all:

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
- Widescreen-only components (2): one with no base component after it (known
  difference), one a base component must follow (matches).
- Controls (3): no references, an already ordered guide, a Dependencies target listed
  after its dependant.
- Report listing (3): the resolver's order in the archive stage.
- Refusal (12): missing InstallAfter and InstallBefore, a self reference, cycles
  among unselected components and with one unselected member, on unselected
  components; missing and circular references on selected ones; JSON input; the report
  without roots (two); install (two).
- Markdown (3): a missing reference on an unselected component and a Dependencies link
  (match), an InstallAfter link (known difference).
- Dry run (1): a component that moves the file a later-installed component creates
  (known difference).

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network either.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 127 match / 5 known difference / 0 fail |
| this slice, recorded reference | 127 match / 5 known difference / 0 fail |
| `e4c0b4d` (pre-fix), recorded reference | 56 match / 2 known difference / 74 fail |

On `e4c0b4d` every install-order case installed the stable order (for `A`, `B` after
`A`, `C`: `A, B, C` where C# gives `A, C, B`, so `Override/z.txt` held `c` instead of
`b`), the report listed components in file order, and every refusal case either
validated and installed where C# stopped, or refused inside the report where C#
stopped before it. The pre-fix build matched the Markdown InstallAfter case and the dry
run case, which are known differences after the fix (see the contract).

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/install-order-proof/probe.py                       # recorded reference
python3 docs/parity/install-order-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `--bin` picks another binary (default `target/release/modsync`).
`MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary directory.
