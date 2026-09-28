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

## Load-time duplicate options

C# `RemoveDuplicateOptions` (`Services/ModComponentSerializationService.cs:1808`) runs from `DeserializeModComponentFromString` (`:865`) on every loaded component. An option is dropped when its instruction list matches an earlier option's. Names don't count. Instruction Guid and Parent are not compared. Dependencies and Restrictions compare as GUIDs. The dropped GUIDs leave the component's own Choose sources. Option-level Restrictions that point at a dropped option are left as they are.

Rust runs the same pass in `Guide::parse` for TOML, JSON, YAML and XML, before the select-all fallback. Markdown is exempt on purpose. C# sends Markdown through `GuideIngestService.IngestMarkdown` and `MarkdownParser` (`Ports/Guides/GuideServices.cs:40-58`, `Services/FileLoadingService.cs:44-48`), and that path never deduplicates. Rust's fenced-block Markdown uses `Guide::parse_without_dedup` for the same reason. C# returns no components for fenced-block Markdown, so that route has no C# oracle.

The probes were checked against the prebuilt C# `convert` from `d31ccbd3`. They covered same-name duplicates, renamed duplicates, an untouched component, Choose cleanup with braced and non-GUID entries, same names with different instructions, empty instruction lists, GUID-case dependencies, differing Overwrite, and prose Markdown. Rust matched C# on option lists and Choose sources in every case. The eight mod-builds TOMLs and `content/k1/full.md` convert byte-identically before and after the change. C# logs no duplicates for the TOMLs.

### Path-normalized Source comparison

C# rewrites every non-Choose `Instruction.Source` entry at deserialize time, before `AreInstructionsIdentical` ever runs: `Serializer.DeserializePathInDictionary` calls `PathHelper.FixPathFormatting` (backslash-to-slash, collapse repeated separators, trim a trailing separator) and then `PrefixPath` (prepend `<<modDirectory>>/` when the path doesn't already start with a `<<...Directory>>` placeholder). Two options whose Source lists are textually different but normalize to the same path — different separators, a missing placeholder prefix, mixed case on the placeholder — are therefore still "identical instructions" in C# and get deduplicated.

Rust's authored `Source` strings are kept as-authored, so `same_instructions` in `src/model.rs` applies the same rewrite on the fly via `source_key` before comparing non-Choose sources (Choose sources are still compared as literal GUID lists, since C# never runs path-rewrite logic on them). Probes `j` and `k` in the differential cover this: `j` mixes `\x\*.tga` and `/x/*.tga` in what should be duplicate options, and `k` mixes backslash, doubled-slash, a bare relative path missing the `<<modDirectory>>` prefix, an all-caps placeholder, and a leading-backslash path — all five normalize to the same key and all five matched the C# oracle's dedup decision after a from-scratch rebuild.
