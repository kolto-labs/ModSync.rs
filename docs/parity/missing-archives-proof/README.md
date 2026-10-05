# Missing archives proof

Contract: [missing-archives-contract.md](../missing-archives-contract.md).

[probe.py](probe.py) builds 23 disposable fixtures. Each has `game/{chitin.key,
Override/}`, `mods/m.txt`, an existing `mods/a.zip`, and a TOML guide whose components
Extract existing or missing archives, in their own steps or in an option. The probe
runs `validate --output json --dry-run` (the validation pipeline behind the desktop
Validate step) on a fresh copy for each side and compares three things, giving 69
checks:

- `decision`: the exit code and `success`;
- `counts`: `errorCount`, `warningCount` and `passedCount`;
- `stage`: the `ComponentValidation` stage's `passed`, `hasWarnings`, `summary` and
  `messages`.

The reference is the pinned C# CLI itself, `dotnet ModSync.Core.dll validate` from a
`d31ccbd3` build, run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major` (the build targets .NET 9; the host has .NET 10). No
custom oracle is needed. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network either.

`KNOWN` holds the documented differences (seven cases, 19 checks). A known check
passes only if Rust differs from the reference and its archive-stage messages are
exactly the expected Rust messages.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 47 match / 22 known difference / 0 fail |
| this slice, recorded reference | 47 match / 22 known difference / 0 fail |
| `3024e4d` (pre-fix), live reference | 33 match / 9 known difference / 27 fail |
| `3024e4d` (pre-fix), recorded reference | 33 match / 9 known difference / 27 fail |
| [empty-extract-report](../empty-extract-report-contract.md) (2026-10-05), recorded reference | 50 match / 19 known difference / 0 fail |
| `b8b4d57` (before empty-extract-report), recorded reference | 47 match / 19 known difference / 3 fail |

On `3024e4d`, 14 checks of 11 matching cases fail: every archive stage that reports a
missing archive has the message `archive/destination validation: Source matches no
files: ...` instead of `Missing Required Archives: [...]`, and `two_missing`,
`duplicate_missing` and `two_components_missing` also count one error too few. The
other 13 failures are known-difference checks whose Rust messages had the old
wording.

Since empty-extract-report, `empty_source_only` (`Extract []` only) matches: Rust's
archive stage reports `Missing Required Archives: []` as C# does, so it left `KNOWN`;
on `b8b4d57` its three checks fail (Rust passed it).

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/missing-archives-proof/probe.py                       # recorded reference
python3 docs/parity/missing-archives-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies.
