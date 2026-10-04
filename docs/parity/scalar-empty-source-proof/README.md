# Scalar empty Source proof

Contract: [scalar-empty-source-contract.md](../scalar-empty-source-contract.md).

[probe.py](probe.py) builds 23 disposable fixtures and runs the production
`modsync` three ways on a fresh copy of each, giving 69 checks, as in
[empty-source-proof](../empty-source-proof/README.md): `validate --output json
--dry-run` (decision and `DryRunSkipped`), plain `validate` and `install` (decision,
and the final game tree after an accepted install). Every guide first copies `a.txt`
and `b.txt` into `Override`; the case step follows. Six cases are also converted with
`modsync convert` to TOML, JSON, XML, YAML and Markdown, and each output is run
through plain `validate` (30 checks), which must give the input's validate decision.

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

Converted: `xml_empty_delete_false`, `md_toml_empty_delete_false`,
`md_yaml_empty_delete_false` (must stay refused), and `xml_selfclose_delete_false`,
`md_toml_empty_list_delete_false`, `xml_empty_choose` (must stay accepted).

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice, live reference (`--csharp ...`): 23 reference loads + 99 checks | 122 match / 0 fail |
| this slice, recorded reference | 99 match / 0 fail |
| `3911e74` (pre-fix) | 69 match / 30 fail |

On `3911e74` the scalar cases took the empty-list behaviour. Each scalar Copy passed
the dry-run report with `DryRunSkipped` (it was refused by validate and install, as
expected). Each scalar non-strict Delete passed all three verbs, installing as a
no-op, where the reference refuses the XML guide. Every converted refused guide read
back as an empty list and validated (15 convert checks). The list forms, `<Source />`,
Choose and the controls match on both binaries. The first version of this slice
(`3c6aff7`) refused the XML and Markdown Choose cases ("FOMOD configuration check
failed: Unknown option") and let the converted guides validate; both are fixed.

## Reproduce

```bash
cargo build --locked --lib --bin modsync -j4
python3 docs/parity/scalar-empty-source-proof/probe.py            # recorded reference
# Live reference: a d31ccbd3 build output directory containing ModSync.Core.dll.
python3 docs/parity/scalar-empty-source-proof/probe.py --csharp <ref>/ModSync.Core.dll [--record]
```

The reference targets .NET 9; the probe sets `DOTNET_ROLL_FORWARD=Major` so that a
newer runtime can run it.
