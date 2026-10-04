# Missing destination proof

Contract: [missing-destination-contract.md](../missing-destination-contract.md).

[probe.py](probe.py) builds 29 disposable fixtures. Each has `game/{chitin.key, r.txt,
Override/}`, `mods/{m.txt, l.csv}` and an existing `mods/a.zip`, and a TOML guide whose
components Copy, Move, CleanList or DelDuplicate into folders that exist (`Override`,
`override`) or not (`Nope`, `Nope2`), in their own steps or in an option. The probe
runs `validate --output json --dry-run` (the validation pipeline behind the desktop
Validate step) on a fresh copy for each side and compares three things, giving 87
checks:

- `decision`: the exit code and `success`;
- `counts`: `errorCount`, `warningCount` and `passedCount`;
- `stage`: the `ComponentValidation` stage's `passed`, `hasWarnings`, `summary` and
  `messages`.

C# prints the full path of the missing folder, so the probe replaces the fixture's
absolute path with `<root>` in the messages of both sides before comparing.

The reference is the pinned C# CLI itself, `dotnet ModSync.Core.dll validate` from a
`d31ccbd3` build, run with the same arguments under `unshare -rn` (no network) and
`DOTNET_ROLL_FORWARD=Major` (the build targets .NET 9; the host has .NET 10). No
custom oracle is needed. Its results are in [csharp-oracle.json](csharp-oracle.json).
The Rust runs need no network either.

`KNOWN` holds the documented differences (eight cases, 12 checks). A known check
passes only if Rust differs from the reference and its archive-stage messages are
exactly the expected Rust messages.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 75 match / 12 known difference / 0 fail |
| this slice, recorded reference | 75 match / 12 known difference / 0 fail |
| `1e2d133` (pre-fix), recorded reference | 53 match / 2 known difference / 32 fail |

On `1e2d133`, 21 checks of 18 matching cases fail. In 16 cases the archive stage
reports the missing folder as `archive/destination validation: Destination cannot be
found: <Destination as written>` instead of `Destination cannot be found! Got '<full
path>'`, and three of them (`two_missing`, `missing_between_present`,
`base_and_option_missing`) count one error too few, because Rust stopped at the first
missing folder. An empty or absent Destination gave `Path must start with
<<modDirectory>> or <<kotorDirectory>>: ` (2 cases). The other 11 failures are
known-difference checks: the Rust messages had the old wording, and in
`member_miss_and_missing` the missing folder hid the archive-member miss that C#
reports.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/missing-destination-proof/probe.py                       # recorded reference
python3 docs/parity/missing-destination-proof/probe.py --reference <ref> [--record]   # live
```

`<ref>` is the `d31ccbd3` build output directory holding `ModSync.Core.dll` and its
dependencies. `MODSYNC_PROOF_OUT`, when set, is the parent of the probe's temporary
directory.
