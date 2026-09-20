# Open shared-loader metadata parity

Reference: C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`. This is an implementation obligation, not a completed feature.

The native run in `wizard-reference/` showed eleven pages for the bare JSON fixture because C# kept its default preamble. Rust currently has ten for the same guide. Defaults belong in the shared guide-loading boundary so CLI and GUI receive identical effective metadata.

| C# load path | Fresh-process behavior | Replacement-load behavior |
|---|---|---|
| JSON component array | Keeps MainConfig content defaults | Keeps previous guide metadata |
| TOML | Clears content metadata, then overlays supplied values | Same deterministic clear-and-overlay |
| YAML | Defaults remain unless present metadata keys replace them | Missing keys retain previous values |
| Direct Markdown deserialize | Nonblank parsed sections replace defaults | Missing/blank sections retain previous values |
| Normal GUI Markdown file load | Component-only load leaves metadata unchanged | Retains previous metadata |
| Dedicated Markdown paste/import | Applies four parsed content fields, including empty values | Applies metadata before replace/merge/cancel decision; installation warning can retain its previous value |

Source pointers in the C# checkout: `MainConfig.cs:303` (five content defaults), `Services/ModComponentSerializationService.cs:502` (YAML), `:652` (direct Markdown), `:719` (JSON), `:1998` (TOML reset); `Services/FileLoadingService.cs:56` (core Markdown file load), and GUI `Services/FileLoadingService.cs:413` (dedicated Markdown metadata assignment).

The remaining Rust work is deterministic fresh-load defaults for all applicable formats and all five content fields: preamble, epilogue, widescreen warning, Aspyr warning, and installation warning. Current-file authored values and explicit empty overrides must win. Preserve isolated `Guide` values; do not reproduce C# previous-guide leakage or cancelled-import mutation. Any intentional difference from the inconsistent Markdown entry points must be documented.

Acceptance needs shared-loader fixtures for fresh defaults, partial metadata, explicit empty overrides, all cross-format replacement pairs, and export effects. Native routing must then match the effective preamble/Aspyr/widescreen metadata. The JSON fixture alone does not prove the full format/content contract.
