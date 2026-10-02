# Structured guide property access

This source slice implements format-specific known-property lookup for the pinned C# serialization service. It does not change public Guide, Component or Instruction structures. Source files: `model/keys.rs`, the ingestion/normalization prefix of `model.rs`, XML coercion in `xml.rs`, the raw review guard in `draft.rs`, and the existing embedded metadata format dispatch in `markdown.rs`. Resource property projection is implemented in the new keys module; the existing resource migration implementation is retained.

## Dictionary policies

| Route | Policy |
| --- | --- |
| TOML component | Exact canonical property wins; otherwise first ordinal-ignore-case match, except explicit presence gates below |
| JSON | Existing serde preserve-order intake matches JObject exact-key replacement position; known properties then undergo first-character uppercase and ignore-case dictionary collapse, retaining the first converted spelling and last retained-property value |
| YAML component | Ignore-case dictionary construction rejects distinct alias collisions |
| YAML option | Alias collisions are conservatively unsupported; the reference's alternate grouped option route is not generalized here |
| XML | Exact root/wire aliases remain; known virtual dictionary spellings are retained through scalar/list coercion, before canonical model projection |
| Internal generated values | Canonical/internal ordinal intake; external format context is supplied explicitly |
| Embedded Markdown metadata | Legacy generated metadata is internal; parsed TOML/YAML uses its corresponding policy. Structured fenced guides already use the normal format parser |

JSON exact duplicates do not move to the end: Newtonsoft `JContainer.ReadProperty` replaces the existing property at its original position, as does the current serde preserve-order map. The later case-alias collapse therefore uses retained property order, not raw final textual occurrence order.

Only known schema properties are projected. Unknown Rust metadata retains authored names/values as a deliberate extension to the C# typed component model. URL keys, filenames and HandlerMetadata content are opaque to this new projection. The pre-existing JSON legacy-filename conversion remains format-specific and unchanged.

## Access that must remain distinct

Component `Instructions` uses literal presence in ordinal TOML/internal dictionaries, and comparator-aware presence after JSON/XML/YAML conversion. Options use getter lookup for `Instructions`. Ignored or losing known executable aliases are removed so rendering to another supported format cannot activate them later. An ignored executable container emits one nonsecret advisory without authored names or content.

Instruction dictionaries are ordinal after reserialization. Private access evidence is captured after virtual case-alias collapse and null-property removal, before canonical output projection. Literal `Source` controls path rewriting for non-Choose actions. Getter-selected Source arrays independently remove null/blank entries; nonblank strings retain their spacing if path rewriting is not authorized. Unsupported numeric elements remain explicit deserialization errors.

Literal `Overwrite` controls whether an authored value overrides the existing action default. Thus retained virtual `SOURCE`/`OVERWRITE` spellings can differ from canonical `Source`/`Overwrite`, even when a later alias supplied their value. A null-winning alias is removed before this evidence is captured.

Literal Dependencies/Restrictions evidence is retained separately. Rust's existing opaque GUID-string representation is unchanged: this slice does not introduce the reference's separate GUID repair/coercion algorithm. Instruction Guid/Parent behavior and unknown-instruction rejection retain their existing Rust contracts.

ResourceRegistry's ordinal presence gate accepts only its established Pascal/camel spellings. Known resource record properties use getter selection; dynamic dictionary keys are not visited. XML Url/url/PrimaryUrl/primaryUrl and Value/value wire aliases remain explicitly enumerated. Arbitrary XML HandlerMetadata scalar type reconstruction is outside this slice; no blanket string guessing was added.

Singleton Category/Language values without commas or semicolons preserve nonblank authored text; blank values become empty lists. Delimited values retain existing splitting/trimming.

## Review state and limits

Raw review validation occurs before known-property selection, case-alias collapse, or component null removal. Noncanonical or ambiguous reserved DraftReview/UnresolvedMasters aliases reject. A review-marker InstallationWarning cannot be hidden by another alias. Known review receipt/diagnostic property aliases reject rather than changing receipt semantics. All raw recognized Options/container aliases are inspected; arbitrary resource/handler dictionaries are outside this traversal.

After projection, the existing DraftReview validity check runs again against the effective canonical Guid and selected Options. Existing receipt hashes, OriginalResult/Resolution payloads and installation review gates are preserved.

Property projection is bounded to 64 component nesting levels and 100,000 visited records/properties. Raw review traversal has independent depth/work bounds. These are explicit supported-intake limits. Parsing remains transactional: an error returns no partially normalized Guide.

## Reference and evidence

Pinned `ModComponentSerializationService.cs`: component getter/default access1435–1770; instruction access2135–2190; option fields2449–2464; `GetValue`2664–2745; YAML preprocessing1053 onward; `JTokenToDictionary`5895 onward. `Utility/Serializer.cs`: exact Source/GUID-list preprocessing111–228 and ordinal instruction reserialization293–338. Newtonsoft13.0.4 `JContainer.ReadProperty`843–870 and replacement530–538 preserve exact-duplicate position.

This is a production source implementation receipt. No tests, test helpers, fixtures, harnesses, runtime probes, builds or CI were run by the implementing agent. Independent source review and parent-owned production gates are separate evidence.
