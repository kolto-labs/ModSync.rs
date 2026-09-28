# Markdown sequence: independent plan review

**PLAN APPROVED** after two corrections, by fresh independent feasibility reviewer.
Actual Rust and pinned C# source inspected before assessing proposed contracts.
No edits, builds, tests, harnesses, network or production processes by reviewer.

M1 originally promised DraftReview round trips without owning the importer allowlist
that drops that field. It now owns that seam, preserves RequiresReview plus complete
Result for components/options in an optional existing YAML metadata field, and
emits comments for diagnostics-only components. Actual pending/reviewed/gapped/
conditional/rejected round trips must retain safety; malformed or missing records
cannot approve actions. This is an explicit Rust extension, not attributed to C#.

M2 originally required common normalization while implying C# legacy raw preservation.
It now chooses uniform safe Rust normalization across both model passes, preserves
Choose exemption and execution roots, and labels the compatibility difference.
Reviewer inspected the pinned C# production conversion oracle: exit0 retains raw
`folder\\example.tga`, DLL hash89d9b2361aee4364f95151b41db59a4133122a6ad41bc2703e232cf10a4f5a6b.
Separate stored-model and execution comparisons remain mandatory.

M3's actual full.md-to-draft evidence path was approved initially: no replacement
parser or silent bulk review approval, no inflation of historical1080-component
projection into current pipeline proof. No broader F01/F02/F03 completion follows.
[Plan](markdown-remaining-plan.md) and [oracle](legacy-source-oracle/README.md).
