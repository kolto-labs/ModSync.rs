# Component text getter intake

This bounded production slice implements the represented-value string getter for component Name, Author, Description, Directions and Tier, and option Name/Description. Existing typed strings retain their text; Tier subsequently uses its existing normalization. Component Category/Language use the reference list getter and its scalar fallback. Other option metadata, GUIDs, instruction Source, dependencies and the remaining component text fields are outside this change.

## Conversion contract

| Input | Scalar text | Category/Language |
| --- | --- | --- |
| String | Preserve exactly | Split comma/semicolon, trim .NET whitespace, remove empty tokens |
| Null/missing | Existing optional empty/default | Empty/default |
| Boolean | `True` or `False` | Scalar text fallback, then split |
| Represented signed integer | Invariant decimal | Scalar text fallback, then split |
| JSON unsigned integer above Int64 within UInt64 | Empty optional conversion result | Empty optional fallback |
| Other-format unsigned beyond Int64 | Explicit unsupported native-type error | Same |
| Finite exactly integral Double within Int32 range | Invariant fixed decimal, including floating `-0` | Scalar text fallback, then split |
| Array | Flatten one enumerable layer, omit blank/null text, join with comma-space | Flatten one consolidation layer plus one list-conversion layer; omit blank/null text |

An array item string retains its original spacing. After list construction, only a singleton containing a comma or semicolon is split and trimmed. Multiple items retain their internal delimiters. This distinguishes scalar `"  Category A  "` from singleton array `["  Category A  "]`. Whitespace is the explicit .NET9 Char.IsWhiteSpace set; NUL is not whitespace here.

Collection primitive formatting uses the reference's current-culture `ToString`, whereas scalar primitive conversion is invariant. Positive integers use decimal digits without grouping. Negative integer and negative-zero collection items use the validated native NegativeSign through one lazily captured NumberCultureSnapshot shared across the guide's component and option normalization, including YAML documents in a single load. No global locale mutation or silent invariant fallback is introduced. Exactly integral floating values in the Int32 range share the proven fixed-decimal profile. Broader floating G-format conversion and CLR type-name formatting for complex/deeper values remain explicit unsupported forms; they are never dropped to synthesize a surviving partial value.

JSON's represented unsigned integers above Int64 and within UInt64 are specifically Newtonsoft BigInteger, whose scalar Convert.ChangeType fails because it does not implement IConvertible. Its positive collection ToString is decimal and remains admitted. This also corrects JSON IsSelected/Overwrite conversion to false for that represented range. Direct serde/internal Boolean intake retains its represented-number policy without claiming Newtonsoft origin.

## Bounds and publication

Original typed scalar strings keep the existing guide input ceiling of 8 MiB. Generated/coerced text has a cumulative 8 MiB budget, collection work is limited to 4096 visited items per field and 100,000 per guide, and list splitting is bounded to4096 items. Exceeding a bound reports an error without truncation. Intermediate generated text also consumes budget. These are explicit implementation limits, not framework limits. Normalization operates on the candidate Value and only publishes a Guide after all fields deserialize successfully.

The existing native culture capture follows the documented platform providers. The subsequent [YAML scalar intake](yaml-scalar-intake.md) replaces automatic plain Boolean/numeric typing with decoded string/null/tag handling matching the bounded pinned object-reader profile. Explicit numeric tags remain subject to their documented wire limits. Native numeric widths in non-JSON inputs are not inferred from unsigned serde storage alone. JSON integer literals above UInt64 currently lose their integer origin in serde floating storage; correcting that parser-origin boundary remains separate source work.

## Source anchors and evidence

Pinned `ModComponentSerializationService.cs`: component assignments1435–1513, option assignments2449–2464, exact/insensitive lookup and first consolidation2664–2750, scalar string-to-list3081–3104, collection string join3117–3145, list second-layer conversion3147–3275, and invariant scalar conversion3279. [.NET9 Number.Formatting.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Number.Formatting.cs)816–823 and927–932 establish integer decimal/default negative-sign behavior;560–570 establishes the default Double roundtrip digit profile. [Newtonsoft13.0.4 JsonTextReader.cs](https://raw.githubusercontent.com/JamesNK/Newtonsoft.Json/13.0.4/Src/Newtonsoft.Json/JsonTextReader.cs)2021–2060 preserves Int64 overflow as BigInteger. The existing format-property and Action/Boolean ordering contracts remain in effect.

No tests, fixtures, harnesses, runtime probes, builds or CI were run by this implementing agent. Independent source SPEC and QUALITY review and parent production gates are separate evidence.
