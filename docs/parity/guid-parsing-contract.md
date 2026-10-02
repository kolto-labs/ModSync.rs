# Profile GUID parsing contract

## Scope and evidence

`src/guid.rs` implements the character-input success/value contract of pinned
.NET 9 `Guid.TryParse`. `profiles/schema.rs::guid` uses it to produce the existing
lowercase, hyphenated D output. This is a source port with independent source
review; no framework comparison, runtime, platform, or test evidence is claimed.

The public profile `normalize_guid` interface and the persisted `opaque-v1`
compatibility policy retain their behavior for unrecognized text. Profile decode
still enforces its existing 16 MiB input bound and opaque identity validation.
Recognized compatibility spellings now canonicalize as reference GUIDs.

This parser does not rewrite guide IDs, immutable checkpoints, receipt IDs, or
stored guide definitions. It does not implement `Serializer.FixGuidString`,
required nonzero component IDs, list preprocessing, or an identity migration.
Nil is a successfully parsed GUID here; required-ID rules belong to their callers.

## Grammar

The ordinary forms are N (32 hex characters), D (36 characters with fixed dashes),
B (D in braces), and P (D in parentheses). Parsing trims the pinned .NET whitespace
set. It rejects URNs. The original UTF-16 minimum length and dispatch positions
are checked before any X whitespace removal. See the
[Microsoft API description](https://learn.microsoft.com/en-us/dotnet/api/system.guid.tryparse?view=net-9.0).

D compatibility runs only after ordinary D decoding fails and the input contains
`x`, `X`, or `+`. Its five initial numeric segments accept optional `+` and `0x`
prefixes within their fixed widths. Exhausted prefixes parse as zero. The final
eight-character numeric segment follows the number parser instead: one or more
ASCII hex digits, optionally followed only by NULs. It accepts neither prefixes
nor whitespace.

X removes all pinned whitespace and requires the nested brace/comma structure
with eleven mandatory outer `0x` prefixes. Each remaining numeric span must be
nonempty but may contain optional `+` and another `0x`; an exhausted inner prefix
parses as zero. Arbitrary leading zeros are ignored. Significant digits must fit
32 bits; the second and third values truncate to 16 bits, and the final eight
values must fit bytes. Both cases of hexadecimal characters and `x` are accepted.
These compatibility rules are pinned to
[Guid.cs](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Guid.cs#L466),
[the numeric hex parser](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Number.Parsing.cs#L516),
and [the trailing NUL helper](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/Common/src/System/Number.Parsing.Common.cs#L301).

Whitespace is explicitly pinned to U+0009–000D, U+0020, U+0085, U+00A0, U+1680,
U+2000–200A, U+2028, U+2029, U+202F, U+205F, and U+3000, following
[Char.IsWhiteSpace](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Char.cs)
and its Unicode category lookup. Rust's evolving character tables do not decide
this admission rule. Rust string input cannot represent lone UTF-16 surrogates;
those are not valid GUID digits, delimiters, or whitespace.

## Work and storage

The parser uses constant auxiliary space: a 32-unit dispatch prefix, fixed UUID
fields, and a streaming X iterator. It does not copy compacted X text or allocate
per numeric token. Work is linear in the supplied text, including long leading
zeros and whitespace. The existing profile admission bound limits persisted
input. The helper adds no smaller text cap, preserving public normalization of
valid long padding; callers outside persisted profile intake must bound their
own input if required.

Canonical bytes use the displayed field order, independent of host endianness.
Malformed text returns `None`; exception classes from the reference throwing
APIs are outside this `TryParse` contract.
