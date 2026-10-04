# Convert dependency order proof

Contract: [convert-dependency-order-contract.md](../convert-dependency-order-contract.md).

[probe.py](probe.py) writes 35 guides. Components are named after their GUIDs (`A` is
`aaaaaaaa-…`), each with one Copy step, and some name others in `InstallBefore` or
`InstallAfter`. Each side runs, in a fresh folder,
`convert -i guide.<ext> -f <format> -o out.<ext> [--ignore-errors]` (or `--stdin`).

Checks, 140 in all (four per case):

- `decision`: exit code zero or not, and whether the output file was written;
- `order`: the top-level component names in the output, in file order;
- `refs`: each component's InstallBefore and InstallAfter GUIDs as written (compared
  in lower case without braces; not read from Markdown);
- `errors`: the N of `Dependency resolution failed with N errors` when the run fails.

Ordering cases (15): one InstallAfter, one InstallBefore, a tie with an unconstrained
component (`B after A`, `C`, `A` gives `A B C`), two InstallAfter targets in authored
order, both lists together, a chain, unselected components, a braced uppercase
reference, JSON and YAML input, JSON, YAML and Markdown output, and `--stdin`.
Error cases (8): a missing InstallAfter, a missing InstallBefore, a missing reference
on an unselected component, a self reference, a two-way and a three-way cycle, a cycle
in JSON, and `--ignore-errors` with a missing reference (ordered around it, reference
kept). Controls (4): no ordering fields, an already ordered guide, an option-level
reference (not checked by C#), and `--ignore-errors` on a valid guide. Markdown input
(6): `<!--<<ModSync>>` blocks with InstallAfter, in file order `C`, `B after A`, `A`;
a missing reference; a cycle; the first two also with `--ignore-errors`, and the missing
reference through `--stdin`. C# keeps the file order and never refuses them.

Known differences (14 checks): `--ignore-errors` with a self reference or a cycle (2
cases, 8 checks): C# writes a guide with no components (exit 0); Rust refuses and
writes nothing. The six Markdown-input cases' `refs`: C# drops the references, Rust
keeps them.

The reference is the pinned C# CLI, `dotnet ModSync.Core.dll` from a `d31ccbd3` build,
run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major`. Its results are in [csharp-oracle.json](csharp-oracle.json).

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 126 match / 14 known difference / 0 fail |
| this slice, recorded reference | 126 match / 14 known difference / 0 fail |
| `24897e6` (pre-fix), recorded reference | 69 match / 14 known difference / 57 fail |

On `24897e6` the 15 ordering cases fail on `order` (Rust wrote the file order), the 7
refusal cases fail on all four checks (Rust wrote a guide), and the four
`--ignore-errors` cases that C# completes (two of them Markdown) fail on `decision`,
`order` and `errors`, and the two TOML ones also on `refs` (`unexpected argument
'--ignore-errors'`).

## Reproduce

```bash
cargo build --locked --release --lib --bin modsync -j4
python3 docs/parity/convert-dependency-order-proof/probe.py --bin target/release/modsync          # recorded reference
python3 docs/parity/convert-dependency-order-proof/probe.py --bin target/release/modsync \
    --reference <ref> [--record]                                                                 # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary
directory.
