# Instruction Action intake

This slice adds private `model/action.rs` and narrow model deserialization/normalization changes. Public Instruction fields remain unchanged. Unknown instruction properties are discarded, matching the typed reference loader; they cannot become executable instruction fields after a save/reload.

## Reference contract

Pinned `Instruction.cs` declares signed-int ActionType values0–11: Unset, Extract, Execute, Patcher, Move, Copy, Rename, Delete, DelDuplicate, Choose, Run, CleanList. `ModComponentSerializationService.cs` instruction intake2135–2190 obtains raw getter text, preprocesses Source, handles whole-value TSLPatcher/HoloPatcher aliases, then calls `Enum.TryParse(ignoreCase:true)`. Failed parsing sets Unset. Overwrite defaults use the parsed action.

The parser preserves these distinctions:

- Alias comparison is ordinal-ignore-case on the whole raw text before trimming. Padded aliases are invalid; padded actual enum names remain valid.
- Signed decimal strings accept any valid i32, not just defined members. Values0–11 serialize as their canonical names; other valid i32 values serialize as decimal text.
- Numeric leading whitespace follows Enum's Unicode whitespace trim. Numeric trailing whitespace is only ASCII09–0D or20, optionally followed by terminal NUL characters. A NUL followed by whitespace is invalid. No hexadecimal, grouping, decimal point, or exponent is accepted as numeric string syntax.
- Named comma-separated tokens are trimmed and matched ordinal-ignore-case. Every token must match. Values are combined with bitwise OR even though the enum lacks Flags. Empty, trailing, unknown or mixed numeric/name tokens invalidate the entire result; a partial accumulated result is never returned.
- Missing/null Action, malformed input, and numeric overflow become Unset. Existing engine preflight and execution reject Unset and unknown decimal actions; no skip or success authority is introduced.

Source preprocessing still checks raw getter text for Choose. Consequently numeric9 can parse as Choose while taking the reference's earlier non-Choose Source preprocessing route. Conversely numeric7 must receive the parsed Delete action's false Overwrite default. Canonical Action publication occurs only after these two decisions.

## Bounded getter profile

String values are limited to4096 bytes. Collections are flattened exactly one layer, with at most1024 visited items and4096 joined bytes. Null/blank items are omitted as in the getter; nonblank string spacing is retained. Complex/deeper items invalidate the entire action rather than disappearing and exposing a remaining executable token.

Primitive numeric scalars within i32, including finite exactly integral doubles, have an action-equivalent fixed decimal representation. They are not rounded or truncated. Nonintegral/out-of-range numeric values produce Unset. Boolean values produce the same invalid-enum outcome as their reference text.

The collection getter calls each item's parameterless ToString, unlike invariant scalar conversion. Nonnegative integral i32 collection values have a supported action-equivalent representation. Negative numeric collection items are explicitly refused because CurrentCulture can change the sign text; no native culture snapshot or invented invariant conversion is added here. Other nonintegral/out-of-range numeric or complex items poison the whole action. These are declared bounded intake rules, not a claim of complete culture-sensitive collection conversion.

Existing Source/dependency list conversion differences remain separate work: this change does not add numeric Source stringification, GUID repair, or general-purpose primitive-to-string conversion.

## Primary sources and evidence

[.NET9 Enum.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Enum.cs), TryParseByValueOrName826–876 and TryParseByName942–1020; [Number.Parsing.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Number.Parsing.cs), trailing-character processing441–465; pinned GetValue collection conversion3117–3145 and invariant scalar fallback3279; Number.Formatting.cs default Double G precision560–570 grounds the admitted i32 numeric profile.

No tests, test helpers, fixtures, harnesses, parser probes, builds or CI were run by the implementing agent. Scoped source formatting, independent spec/quality review and parent-owned production compilation are separate evidence.
