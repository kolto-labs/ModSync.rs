# Modern resource field investigation

Pinned reference: `d31ccbd3bfcddb4705917eafa6483715c166de3b`.
Raw observations and input/DLL hashes: [modern-resource-formats.json](modern-resource-formats.json).
Reproducer: `tools/inspect_resource_formats.py`; fixture: `tests/fixtures/modern-resources.json`.

This is an inspection report, not a passing equivalence gate. The tool renders a modern resource fixture into each format using Rust, then independently loads the identical file with the pinned C# service and Rust CLI. It records raw objects, including fields omitted from earlier install-oriented format projections. It neither contacts hosts nor downloads or installs mods.

| Field | C# JSON/TOML/XML | C# YAML | Rust JSON/TOML/YAML | Rust XML |
|---|---|---|---|---|
| URL | Preserved | Preserved through legacy fallback | Preserved | Preserved |
| ContentKey, MetadataHash | Preserved | Replaced with normalized-URL SHA-1 | Preserved | Preserved |
| FileSize | 123 | 0 | 123 | 123 |
| Files true/false/null | Empty map | Preserved through legacy fallback | Preserved | Preserved |
| HandlerMetadata | Empty map | Empty map | Preserved, including nested numeric value | Nested number 17 becomes string "17" |
| FirstSeen, LastVerified | Preserved | Null | Preserved | Preserved |

## Why the reference loses maps

`ModComponentSerializationService.GetValue<T>` enumerates any non-string collection into a `List<object>` before its target-type dispatch (lines 2684 onward). A dictionary therefore reaches `GetValue<object>` as a list. `DeserializeResourceRegistry` passes this list into map conversion helpers, which do not recognize it and return empty dictionaries (Files and HandlerMetadata, lines 6247–6253).

The YAML component preprocessing additionally flattens ResourceRegistry itself. The emitted ModLinkFilenames compatibility map recovers URLs and nullable filename flags, creating fresh minimal metadata and URL-derived hashes. It does not recover the original resource identity, handler data, size or verification timestamps.

The reference's `ResourceMetadataSerializationTests` includes preservation expectations for files and handler metadata. Those source assertions are evidence of the intended contract, not evidence that the tests passed: this investigation did not run that suite. The live loader outputs above contradict preservation for the exercised inputs.

## Acceptance consequences

- Prior format checks that compare component/instruction identity and resource URL order do not prove resource metadata parity. Preserve their narrower scope.
- The earlier single JSON Files difference is one instance of a cross-format loader problem; it cannot be closed merely by ignoring Files during comparisons.
- Do not discard Rust's file-selection maps as an assumed fix for full functional parity. The reference behavior and its preservation assertions conflict and remain explicitly recorded.
- Rust's XML nested scalar type loss is independently unresolved. Inferring numbers from arbitrary text would corrupt legitimate strings such as version identifiers; a solution needs an explicit wire representation and verified C# interoperability.
- F01/F12 remain partial. Next resource acceptance work must exercise cache/resource identity and per-file selection through consumers, alongside resolving the XML representation boundary. A successful parser or these fixtures alone cannot establish that end state.

## Validation

The inspection completed for all four formats against the pinned DLL. Formatting, clippy with warnings denied, and 43 regular Rust integration tests passed in both feature configurations. The added regression explicitly records the unresolved XML type change; its passing status does not establish XML metadata equivalence. The explicit 513 MiB test was not rerun locally for this evidence/test change. Preceding commit `c8a07bc3991609394af778436f97eba94a4917c4` passed native CI run 35059827754.
