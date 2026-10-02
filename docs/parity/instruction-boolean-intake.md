# Selection and Overwrite Boolean intake

This source slice adds private `model/boolean.rs`, delegates Component IsSelected and Instruction Overwrite serde intake to it, and normalizes those fields after the format-aware property projection. XML no longer eagerly rejects these two fields through Rust's strict boolean parser; it preserves their values and virtual key spellings until the shared model getter runs. Other XML flags and resource selection conversion remain unchanged. The reader preserves selfclosing events: attribute-free selfclosing component/option IsSelected or instruction Overwrite leaves (including known case aliases), qualified after actual XML collection-wrapper selection, become null, while explicit open/close empty leaves remain empty strings. Null instruction properties disappear before literal-presence evidence, so selfclosing Overwrite uses the action default; an explicit empty Overwrite remains present and becomes false. Other empty-element wire behavior, including attributed leaves, remains unchanged in this deliberately narrow correction. Raw reserved-review checks still run before known-key projection and null removal.

## Optional getter semantics

For represented structured values:

| Value | Result |
| --- | --- |
| Boolean | Unchanged |
| Represented native number | Zero, including negative floating zero, is false; nonzero is true |
| JSON integer above Int64 within serde's unsigned range | False: Newtonsoft reads BigInteger, which fails optional IConvertible conversion |
| String | Ordinal-ignore-case true after admitted boundary trimming is true; false and malformed text yield false |
| Null, array or object | False, matching failed optional conversion/defaulting |

Boolean strings are limited to4096 bytes; exceeding that intake limit returns an error. The exact boundary characters are NUL plus .NET9 Char.IsWhiteSpace: U+0009–000D,0020,0085,00A0,1680,2000–200A,2028,2029,202F,205F,3000. Boundary characters can occur on either end. Interior NULs/whitespace are not removed, numeric strings are not interpreted as booleans, and no process locale is consulted. This is a policy for values representable by the existing structured parser; it does not add special floating-point tokens to that parser.

Format-aware normalization preserves the JSON BigInteger distinction from [Newtonsoft13.0.4 JsonTextReader.cs](https://raw.githubusercontent.com/JamesNK/Newtonsoft.Json/13.0.4/Src/Newtonsoft.Json/JsonTextReader.cs)2021–2060. Direct serde/internal intake has no JSON-origin evidence and keeps its represented-number policy. The subsequent [YAML scalar intake](yaml-scalar-intake.md) now preserves default plain scalar strings and handles explicit tags separately, so plain numeric IsSelected text follows string conversion rather than being silently admitted as a native number.

## Presence and ordering

Format-specific key precedence and instruction literal-presence evidence remain unchanged. IsSelected conversion applies to both components and options. The existing preserving-selection loader and the ordinary loader's all-unselected fallback remain separate policies; this conversion does not select components on its own.

For Overwrite, the virtual instruction dictionary is collapsed and null properties removed before literal-presence evidence is captured. Only a surviving canonical Overwrite property reaches Boolean conversion. A present malformed value becomes false. An absent property retains the parsed action's default: false for Delete and true for other actions. A noncanonical alias ignored by the source's ordinal presence gate cannot alter that default. The previously implemented Action raw-Choose/source-preprocessing order is unchanged.

Unknown instruction fields continue to be discarded under the existing Action-intake change. This slice does not broaden Source/dependency conversion, normalize additional XML flags, or rewrite arbitrary metadata.

## XML wire anchor

Newtonsoft13.0.4 [XmlNodeConverter.cs](https://raw.githubusercontent.com/JamesNK/Newtonsoft.Json/13.0.4/Src/Newtonsoft.Json/Converters/XmlNodeConverter.cs)1249–1259 emits null for an attribute-free selfclosing element and an empty string for an explicit empty element. A shared element-completion helper handles Empty/End events without changing collection grouping. The reader initially marks eligible selfclosing Boolean-name leaves as temporary null candidates. Role qualification runs after actual array-wrapper selection in the existing normalizer: direct role-correct Boolean candidates remain null, actual selected Options/Instructions recurse, and every other occurrence is restored to the previous empty-string shape. Opaque singular wrapper names and dynamic resource/handler paths are therefore not broadly retyped by property name. Restoration has a shared 100,000-value work bound and a 256-level value-depth bound, in addition to the reader’s existing XML nesting limit. This is not a general XML null/type reconstruction claim.

## Reference and evidence

Pinned `ModComponentSerializationService.cs`: IsSelected getter assignment1763, instruction Overwrite presence/default2173 onward, optional getter null handling2664–2750 and final invariant Convert.ChangeType/catch3279 onward. [.NET9 Boolean.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Boolean.cs)228–296 defines case-insensitive parsing and boundary whitespace/NUL removal; [Convert.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Convert.cs)309–382 defines numeric zero/nonzero conversion.

No tests, fixtures, harnesses, parser probes, builds or CI were run by the implementing agent. Independent SPEC/QUALITY review and parent-owned production gates are separate evidence.
