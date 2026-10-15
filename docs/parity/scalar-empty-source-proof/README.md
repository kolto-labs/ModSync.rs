# Scalar empty Source proof

Contract: [scalar-empty-source-contract.md](../scalar-empty-source-contract.md).

[probe.py](probe.py) builds 45 disposable fixtures. For 40 of them it runs the
production `modsync` three ways on a fresh copy of each, giving 120 checks, as in
[empty-source-proof](../empty-source-proof/README.md): `validate --output json
--dry-run` (decision and `DryRunSkipped`), plain `validate` and `install` (decision,
and the final game tree after an accepted install). Every guide first copies `a.txt`
and `b.txt` into `Override`; the case step follows. Ten cases are also converted with
`modsync convert` to TOML, JSON, XML, YAML and Markdown, and each output is run
through plain `validate` (50 checks). A converted guide
must give the input's validate decision and, for a Choose, read back with the
reference's Source list. Every case whose reference load has a Choose step also gets a
shape check (20 checks): the Source list Rust loads (`modsync convert -f json`) must be
the reference's. The five `*_split_choose` cases (a non-blank Choose scalar naming two
GUIDs) get only that check.

The reference result is the C# load, recorded in [csharp-oracle.json](csharp-oracle.json):
`ModSync.Core.dll convert --format json` from `d31ccbd3`, the
[scalar-oracle](../source-normalization-scalar-oracle/README.md) method, run under
`unshare -rn` (no network) with its own HOME. The expected Rust decision is derived
from it:

| Reference load | Expected Rust result |
| --- | --- |
| refuses the guide | every verb refuses |
| case step with an empty Source list | empty-list contract: Copy passes the report with `DryRunSkipped`, validate and install refuse; non-strict Delete passes everywhere |
| case step with a path | every verb passes |
| case step is Choose with an empty Source list | every verb passes (empty no-op Choose) |
| Markdown metadata block dropped (no instructions) | documented difference: every verb refuses |

| Case | Format | Case step | Reference load |
| --- | --- | --- | --- |
| `xml_empty_copy` | XML | Copy, `<Source></Source>` | refused |
| `xml_whitespace_copy` | XML | Copy, `<Source>  </Source>` | refused |
| `xml_empty_delete_false` | XML | Delete, `<Source></Source>`, Overwrite false | refused |
| `xml_selfclose_copy` | XML | Copy, `<Source />` | empty list |
| `xml_selfclose_delete_false` | XML | Delete, `<Source />`, Overwrite false | empty list |
| `xml_nokey_copy` | XML | Copy, no Source | empty list |
| `xml_path_copy` | XML | Copy, `<Source>a.txt</Source>` (control) | path |
| `xml_empty_choose` | XML | Choose, `<Source></Source>` | empty Choose |
| `xml_component_empty_source` | XML | Copy `a.txt`, plus a stray component-level `<Source></Source>` | path |
| `md_toml_empty_copy` | Markdown TOML | Copy, `Source = ""` | block dropped |
| `md_toml_whitespace_copy` | Markdown TOML | Copy, `Source = "  "` | block dropped |
| `md_toml_empty_delete_false` | Markdown TOML | Delete, `Source = ""`, Overwrite false | block dropped |
| `md_toml_blank_list_copy` | Markdown TOML | Copy, `Source = [""]` | empty list |
| `md_toml_empty_list_delete_false` | Markdown TOML | Delete, `Source = []`, Overwrite false | empty list |
| `md_toml_nokey_copy` | Markdown TOML | Copy, no Source | empty list |
| `md_toml_empty_choose` | Markdown TOML | Choose, `Source = ""` | empty Choose |
| `md_yaml_empty_copy` | Markdown YAML | Copy, `Source: ""` | block dropped |
| `md_yaml_whitespace_copy` | Markdown YAML | Copy, `Source: "  "` | block dropped |
| `md_yaml_empty_delete_false` | Markdown YAML | Delete, `Source: ""`, Overwrite false | block dropped |
| `md_yaml_empty_list_copy` | Markdown YAML | Copy, `Source: []` | empty list |
| `md_yaml_path_copy` | Markdown YAML | Copy, `Source: "a.txt"` (control) | path |
| `md_yaml_empty_choose` | Markdown YAML | Choose, `Source: ""` | empty Choose |
| `toml_empty_copy_control` | TOML | Copy, `Source = ""` (unchanged control) | refused |
| `toml_empty_choose` | TOML | Choose, `Source = ""` | empty Choose |
| `toml_whitespace_choose` | TOML | Choose, `Source = "  "` | empty Choose |
| `toml_comma_choose` | TOML | Choose, `Source = " , "` | empty Choose |
| `toml_semicolon_choose` | TOML | Choose, `Source = " ;, ; "` | empty Choose |
| `toml_lowercase_choose` | TOML | `Action = "choose"`, `Source = ""` | empty Choose |
| `toml_numeric_choose` | TOML | `Action = "9"`, `Source = ""` | refused |
| `json_empty_choose` | JSON | Choose, `"Source": ""` | empty Choose |
| `json_comma_choose` | JSON | Choose, `"Source": " , "` | empty Choose |
| `json_numeric_choose` | JSON | `"Action": 9`, `"Source": ""` | refused |
| `yaml_empty_choose` | YAML | Choose, `Source: ""` | empty Choose |
| `yaml_whitespace_choose` | YAML | Choose, `Source: "  "` | empty Choose |
| `yaml_comma_choose` | YAML | Choose, `Source: " , "` | empty Choose |
| `xml_comma_choose` | XML | Choose, `<Source> , </Source>` | empty Choose |
| `xml_numeric_choose` | XML | `<Action>9</Action>`, `<Source></Source>` | refused |
| `md_toml_comma_choose` | Markdown TOML | Choose, `Source = " , "` | empty Choose |
| `md_toml_numeric_choose` | Markdown TOML | `Action = "9"`, `Source = ""` | block dropped |
| `md_yaml_numeric_choose` | Markdown YAML | `Action: 9`, `Source: ""` | block dropped |
| `toml_split_choose` | TOML | Choose, `Source = " <g1> ,<g2>;"` (shape only) | `[g1, g2]` |
| `json_split_choose` | JSON | Choose, `"Source": "<g1>, <g2>"` (shape only) | `[g1, g2]` |
| `yaml_split_choose` | YAML | Choose, `Source: "<g1>;<g2>"` (shape only) | `[g1, g2]` |
| `xml_split_choose` | XML | Choose, `<Source><g1>, <g2></Source>` (shape only) | `[g1, g2]` |
| `md_toml_split_choose` | Markdown TOML | Choose, `Source = "<g1>, <g2>"` (shape only) | `[g1, g2]` |

Converted: `xml_empty_delete_false`, `md_toml_empty_delete_false`,
`md_yaml_empty_delete_false` (must stay refused), and `xml_selfclose_delete_false`,
`md_toml_empty_list_delete_false`, `xml_empty_choose`, `toml_empty_choose`,
`json_comma_choose`, `yaml_whitespace_choose` (must stay accepted, a Choose reading back
as an empty Choose). `toml_numeric_choose` is refused at load, but every save spells the
canonical action `Choose` with the blank scalar, which the reference reads as an empty
Choose (as `toml_empty_choose` shows), so its converted guides must be accepted with an
empty Choose.

## Results (cloud Linux, 2026-10-04)

Choose slice (after `1b3faeb`), 45 cases:

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ...`): 45 reference loads + 190 checks | 235 match / 0 fail |
| this slice, recorded reference | 190 match / 0 fail |
| `1b3faeb` (pre-fix) | 117 match / 73 fail |

On `1b3faeb` every standalone TOML, JSON and YAML blank Choose, and every `" , "` Choose
in any format, kept a one-entry Source and failed `FOMOD configuration check failed:
Unknown option` (shape and all three verbs), as did the converted guides of those
cases; no reader split a non-blank Choose scalar (5 shape checks); and the Markdown
`Action = 9` cases loaded as an empty Choose and passed every verb, where the
reference drops the block. The 23 earlier cases match on both binaries.

Earlier slice (23 cases, 99 checks): 122 match / 0 fail live, 99 match / 0 fail
recorded, 69 match / 30 fail on `3911e74`. On `3911e74` the scalar cases took the
empty-list behaviour. Each scalar Copy passed the dry-run report with `DryRunSkipped`
(it was refused by validate and install, as expected). Each scalar non-strict Delete
passed all three verbs, installing as a no-op, where the reference refuses the XML
guide. Every converted refused guide read back as an empty list and validated (15
convert checks). The first version of that slice (`3c6aff7`) refused the XML and
Markdown Choose cases ("FOMOD configuration check failed: Unknown option") and let the
converted guides validate; both are fixed.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/scalar-empty-source-proof/probe.py            # recorded reference
# Live reference: a d31ccbd3 build output directory containing ModSync.Core.dll.
python3 docs/parity/scalar-empty-source-proof/probe.py --csharp <ref>/ModSync.Core.dll [--record]
```

The reference targets .NET 9; the probe sets `DOTNET_ROLL_FORWARD=Major` so that a
newer runtime can run it.
