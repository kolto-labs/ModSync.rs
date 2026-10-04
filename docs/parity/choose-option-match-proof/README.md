# Choose option matching proof

Contract: [choose-option-match-contract.md](../choose-option-match-contract.md).

[probe.py](probe.py) builds 23 disposable fixtures. In 20 of them, one component's
only instruction is a Choose, with an option `One` (Copy `a.txt` to `Override`,
selected) and an option `Two` (Copy `b.txt`). The three `gen_*` cases are generated:
an authored guide with options that Move folders of `mods/Foo.zip` (`FolderA`,
`FolderB`, `FolderC`, each with one `.tga`), passed through `modsync convert
--auto-generate-local` by the binary under test. Its output, with Extract, Choose and
the inferred options, is the guide every run uses, the reference runs included. The
probe runs the production `modsync` three ways on a fresh copy of each, giving 69
checks: `validate --output json --dry-run` (the
decision), plain `validate` (the decision) and `install` (the decision and the final
game tree).

The reference results are in [csharp-oracle.json](csharp-oracle.json). They come from
[csharp-oracle/Program.cs](csharp-oracle/Program.cs), built against `ModSync.Core.dll`
from `d31ccbd3`. It is the delete-strict oracle with two changes. It loads any
format through `FileLoadingService.LoadFromFile`, and its `load` mode prints the
loaded Choose Source and option GUIDs (`Guid.ToString()`). Dry run uses
`DryRunValidator.ValidateInstallationAsync` and install uses `ModComponent.InstallAsync`.
A Markdown guide loads its components unselected, so before a dry run or install
the oracle selects every component when none is selected, as the Rust CLI does.
Each reference run uses its own fixture and HOME under `unshare -rn` (no network).
The Rust runs need no network; this probe has no netns guard variable.

The expected Rust result is derived from the reference:

| Reference | Expected Rust result |
| --- | --- |
| a `gen_*` case | the reference's dry-run decision, install decision and final tree: C# generators write `Option.Guid.ToString()` (`AutoInstructionGenerator.cs:1911`, `:1916`, `:2058`, `:2063`), so a generated guide that fails to name its options is a Rust generator bug |
| every Choose entry equals a loaded option GUID, ignoring case | the reference's dry-run decision, install decision (plain `validate` and `install`) and final tree |
| some entry names no option (C# ignores it and installs the rest) | every verb refuses, and `install` writes nothing (deliberate fail-closed difference) |

`G1` is `a4444444-4444-4444-4444-44444444444a` (option `One`) and `G2` is
`b5555555-5555-5555-5555-55555555555b` (option `Two`).

| Case | Format | Choose Source | Option `One` GUID | Reference install tree |
| --- | --- | --- | --- | --- |
| `plain` | TOML | `[G1]` | `G1` | `a.txt` |
| `source_upper` | TOML | `[G1]` upper case | `G1` | `a.txt` |
| `option_upper` | TOML | `[G1]` | `G1` upper case | `a.txt` |
| `option_braced` | TOML | `[G1]` | `{G1}` | `a.txt` |
| `option_nform` | TOML | `[G1]` | `G1` without dashes | `a.txt` |
| `both_selected` | TOML | `[G1, G2]`, `Two` selected | `G1` | `a.txt`, `b.txt` |
| `unselected` | TOML | `[G2]`, `Two` unselected | `G1` | empty |
| `source_braced` | TOML | `["{G1}"]` | `G1` | empty (entry ignored) |
| `source_paren` | TOML | `["(G1)"]` | `G1` | empty (entry ignored) |
| `source_nform` | TOML | `[G1 without dashes]` | `G1` | empty (entry ignored) |
| `source_padded` | TOML | `[" G1 "]` | `G1` | empty (entry ignored) |
| `scalar_braced` | TOML | `"{G1}"` (scalar) | `G1` | empty (entry ignored) |
| `braced_and_plain` | TOML | `["{G1}", G2]`, `Two` selected | `G1` | `b.txt` |
| `unknown` | TOML | `[99999999-…]` | `G1` | empty (entry ignored) |
| `unknown_and_known` | TOML | `[99999999-…, G1]` | `G1` | `a.txt` |
| `not_a_guid` | TOML | `["foo"]` | `G1` | empty (entry ignored) |
| `legacy_single` | legacy Markdown | `**Source:** G1` | `G1` | `a.txt` |
| `legacy_comma` | legacy Markdown | `**Source:** G1, G2`, `Two` selected | `G1` | `a.txt`, `b.txt` |
| `legacy_semicolon` | legacy Markdown | `**Source:** G1;G2`, `Two` selected | `G1` | empty (one entry `"G1;G2"`, ignored) |
| `legacy_braced` | legacy Markdown | `**Source:** {G1}` | `G1` | empty (entry ignored) |
| `gen_plain_option` | generated | `G1` (authored `Authored1` reused) plus the inferred `FolderB` and `FolderC` options | `G1`, moves `FolderA` | `a.tga` |
| `gen_braced_option` | generated | as above | `{G1}` upper case, moves `FolderA` | `a.tga` |
| `gen_consolidate_braced` | generated | `G1` plus the inferred `FolderB` option; the authored `G2` option (moves `FolderA`) is merged into `Authored1` | `{G1}` upper case, moves `FolderA` and `FolderC` | `a.tga`, `c.tga` |

The C# dry run reports no Error for any case, and every C# install returns `Success`.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ... --reference ...`): 23 reference runs + 69 checks | 92 match / 0 fail |
| this slice, recorded reference | 69 match / 0 fail |
| `5eb80d5` (engine fix, generators not yet fixed) | 63 match / 6 fail |
| `a09ca4a` (pre-fix) | 48 match / 21 fail |

On `a09ca4a` the seven cases with a braced, parenthesized, N-form or padded entry
(`source_braced`, `source_paren`, `source_nform`, `source_padded`, `scalar_braced`,
`braced_and_plain`, `legacy_braced`) pass every verb and install option `One`, which
the reference skips. After the fix they are refused, as an unknown GUID was before.
The other 16 cases match on `a09ca4a`, whose generator output it still accepted.
That includes `legacy_semicolon`: both legacy readers split only on `,`, so the
legacy split needs no change.

On `5eb80d5` the generator wrote the authored `{G1}` into the Choose Source, by
reusing an authored option (`gen_braced_option`) or by consolidating the authored `G2`
option into the braced one (`gen_consolidate_braced`). The engine then refused the
generated guide in every verb (`Unknown option {A4444444-…}`). C# writes the
hyphenated GUID, and the reference installs it. The other 21 cases match on
`5eb80d5`.

Recording the three `gen_*` cases left the recorded reference results of the other
20 cases unchanged.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/choose-option-match-proof/probe.py            # recorded reference
# Live reference: build the oracle against a d31ccbd3 ModSync.Core.dll first.
dotnet build docs/parity/choose-option-match-proof/csharp-oracle -c Release -o <out> -p:ModSyncCoreDll=<ref>/ModSync.Core.dll
python3 docs/parity/choose-option-match-proof/probe.py --csharp <out>/probe.dll --reference <ref> [--record]
```

The C# run needs the full reference output directory (`<ref>`) next to
`ModSync.Core.dll`, so that its dependencies resolve. The probe sets
`DOTNET_ROLL_FORWARD=Major` so that a newer runtime can run the reference.
