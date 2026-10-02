# YAML scalar wire intake

The structured guide loader and embedded Markdown YAML metadata now use a bounded decoded-event adapter in `model/yaml.rs`. It uses the already locked `unsafe-libyaml 0.2.11` parser directly, preserving scalar style, tags and mapping-key position before projecting into the existing JSON-backed model. This corrects the previous automatic plain-number/Boolean typing from serde_yaml. Existing YAML guide envelopes, metadata document routing, format-specific property precedence and raw review guards remain in their established paths.

## Pinned reader policy

Pinned C# `DeserializeYamlComponent` builds YamlDotNet16.2.0 with PascalCase naming and unmatched-property ignoring. It does not enable unknown-type scalar inference. Default object scalars are strings, including unquoted numeric and Boolean spellings. Explicit tags can select a different CLR type.

| Wire form | Admitted result |
| --- | --- |
| Untagged or string-tagged scalar | Decoded string, subject to the null rule below |
| Plain nonkey empty, `~`, `null`, `Null`, `NULL` | Null, including a plain explicit string-tagged null spelling |
| Explicit null tag | Null regardless of scalar style; null-tagged containers skip their contents |
| Explicit Boolean tag | Case-insensitive true/y/yes/on or false/n/no/off; one final LF is allowed by the pinned regex end anchor |
| Explicit integer tag | Checked Int32 result from the pinned integer grammar |
| Explicit float tag | Finite Double, preserving negative zero and floating type |
| Timestamp or unknown tag | Explicit unsupported wire-contract error |

Unknown/timestamp tags are refused before implicit-null handling. Plain implicit-null spellings in mapping keys remain strings. Explicit null keys and explicitly typed nonstring keys are refused; this prevents collapsing a native numeric key into an authored string key. Complex keys also fail. This key profile is intentionally narrower than nested CLR dictionaries with arbitrary object keys; no universal key ToString conversion is claimed.

Integer parsing preserves source quirks: only lowercase `b`/`x` identify binary/hex after an initial zero; other multi-character initial-zero forms are octal; underscores are removed; decimal sexagesimal chunks are not constrained below60. Its UInt64 multiply/add accumulator wraps as in the pinned unchecked helper, followed by checked signed conversion and the final Int32 cast. Radix conversion and decimal chunks retain their distinct sign/whitespace rules. No `0o` or uppercase-prefix extension is introduced. Explicit integer/float token text is limited to4096 bytes.

Double parsing uses the existing reviewed .NET Float|AllowThousands grammar with fixed YamlFormatter symbols: decimal `.`, group `_`, and nonfinite symbols `.nan`, `.inf`, `-.inf`. It does not use integer-first settings parsing or return malformed numbers as strings. Overflow, infinities and NaN report the JSON-model representation boundary instead of becoming null or strings. Broad floating-to-string component conversion is a separate contract.

## Graph and ownership

The parser lives at a stable heap address and borrows its input until deletion. Each successful event has an owner that releases library buffers on every conversion return. Scalar bytes are length-checked before copying; tag/anchor strings are bounded separately. No external entities, network access or filesystem reads are introduced.

Initial anchors register a pending node before reading children. Repeated anchor names retain the prior binding until the replacement node completes. An alias resolves to the binding visible at its occurrence. Forward aliases without a declaration fail, matching the pinned reader. Cycles fail during projection because the public model is a finite tree; repeated acyclic references expand under the shared budgets. Null-tagged container children are skipped without registering their anchors, matching the reference skip operation.

Mappings preserve first insertion position when an exact duplicate replaces its value. Known case-alias rules remain in the subsequent YAML key projection. No merge-key expansion is invented: `<<` remains ordinary authored data under this reader profile.

## Bounds and integration

One budget is shared across documents in a YAML guide, across YAML fenced blocks in a Markdown guide, and across that guide's prose YAML metadata blocks. Input, decoded data and expanded scalar data each have an8MiB ceiling. Events and projection visits each have a100,000 ceiling; nesting is limited to128 and tags/anchors to4096 bytes. These are explicit bounded implementation limits. Numeric and Boolean tag conversions never invoke a host process or change locale.

Candidates reach `Guide::from_value_with_format` only after successful projection, where existing raw review guards run before known-key projection/null removal. A failure never publishes a partially parsed guide. Rendering continues to use the existing YAML serializer. General timestamp wire reconstruction and typed/complex mapping-key support remain named follow-ups; the JSON arbitrary-precision origin problem is not modified by this change.

## Primary source anchors

- [YamlDotNet DeserializerBuilder16.2.0](https://raw.githubusercontent.com/aaubry/YamlDotNet/v16.2.0/YamlDotNet/Serialization/DeserializerBuilder.cs): default tag mappings71–78, null/scalar ordering90–91, disabled unknown-type inference59/144.
- [NullNodeDeserializer](https://raw.githubusercontent.com/aaubry/YamlDotNet/v16.2.0/YamlDotNet/Serialization/NodeDeserializers/NullNodeDeserializer.cs): explicit-null and plain nonkey policy45–52.
- [ScalarNodeDeserializer](https://raw.githubusercontent.com/aaubry/YamlDotNet/v16.2.0/YamlDotNet/Serialization/NodeDeserializers/ScalarNodeDeserializer.cs): object/string113–123, Boolean133–147, integer151–299, explicit Double93–94.
- [YamlFormatter](https://raw.githubusercontent.com/aaubry/YamlDotNet/v16.2.0/YamlDotNet/Serialization/YamlFormatter.cs): number symbols28–42.
- [AliasValueDeserializer](https://raw.githubusercontent.com/aaubry/YamlDotNet/v16.2.0/YamlDotNet/Serialization/ValueDeserializers/AliasValueDeserializer.cs): declaration/placeholder/rebinding93–128; [DictionaryDeserializer](https://raw.githubusercontent.com/aaubry/YamlDotNet/v16.2.0/YamlDotNet/Serialization/NodeDeserializers/DictionaryDeserializer.cs): duplicate replacement35–42.

No tests, fixtures, harnesses, runtime probes, builds or CI were run by the implementing agent. Source review and parent production gates remain separate evidence.
