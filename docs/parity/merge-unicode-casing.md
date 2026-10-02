# Merge Unicode casing source contract

## Reference operations

Pinned C# `d31ccbd3` ComponentMergeService applies `ToLowerInvariant` to
name/author normalization (line1156), instruction ActionString+Destination keys
(lines815/820 and916/921), and trimmed option names (lines858/861).
Destination/argument preservation uses `OrdinalIgnoreCase` (lines839/843).
Language union constructs an ordinal-ignore-case set from all target values,
then adds nonblank source values (lines674–684).

.NET 9 ICU invariant lowercase is a scalar simple mapping with U+0130 preserved;
it has no expansion or contextual final-sigma processing. Ordinal comparison is
a separate simple-uppercase operation with the U+0131/U+017F exceptions. Lower
keys and ordinal equality therefore have distinct semantics.
[Invariant casing source](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/native/libs/System.Globalization.Native/pal_casing.c),
[ordinal casing source](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/native/libs/System.Globalization.Native/pal_common.c).

## Current implementation

A private `unicode_case` module imports each retained Unicode 15.0 generated
table once. Its scalar functions provide simple lowercase keys, ordinal keys
and streaming ordinal equality. Unmapped scalars retain identity; this is a
pinned dataset contract, not a claim about newer ICU datasets or Windows NLS.
The existing FOMOD ordinal wrapper delegates to the shared ordinal operation.

Maintenance retains its own persisted-profile policy: exact descriptor admission,
cached semantic-table verification, assigned-scalar refusal and legacy ASCII
projection. Only its data-module reference changes. Descriptor fields, generated
bytes, source SHA and semantic-table SHA remain unchanged. The module aliases
metadata and table references without copying tables.

The merge adapter uses simple lowercase for name/author, instruction keys and
trimmed option names. Destination/argument preservation uses ordinal equality.
Duplicate-extension destination checks use the same ordinal semantics while
their existing exact argument comparison stays intact. Language has a separate
ordinal union preserving the first authored spelling and target-first order.
Its empty-incoming gate and source-blank filter are unchanged.

GUID and action handling retain their existing ASCII semantics. URI parsing,
IDN behavior and host comparison have a separate proof boundary; the current
manual host parser is not established as equivalent to .NET Uri.Host by this
casing change. Duplicate-warning extensions retain their existing scope.

## Pinned UTF-16 punctuation continuation

The reference normalization iterates `char` code units and calls
`Char.IsPunctuation(char)`. Supplementary punctuation appears as two surrogate
code units, neither punctuation, and therefore survives normalization. The old
Rust scalar regex removed those characters. The production adapter now retains
supplementary values and removes only BMP punctuation using a binary search over
133 pinned ranges containing 625 code units. Trim, punctuation removal and
invariant lowercase retain their reference order.

.NET 9's [Unicode property generator](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/Tools/GenUnicodeProp/GenUnicodeProp.csproj)
selects Unicode 15.0. The [Char overload](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Char.cs)
and [category source](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/CharUnicodeInfo.cs)
use the code unit's General_Category. A retained production generator selects
Pc/Pd/Ps/Pe/Pi/Pf/Po from the existing exact-hash UCD, excluding non-BMP values
and surrogates. It validates field/category/order/range structure and records
source, license, generator and semantic-table digests. Existing lowercase and
ordinal tables and persisted maintenance descriptors remain unchanged.

The punctuation semantic SHA-256 is
`2c7f77d76585808337b3b76d8e1ef0ff31a3859c29d074cf3656f707b1a0ba0f`.
The provenance is retained beside `src/unicode_case/punctuation.rs`. This
eliminates the merge punctuation consumer's dependence on the regex crate's
Unicode category version. Source review and production gates are recorded
separately; no runtime/corpus acceptance is inferred.

## Narrow HTTP domain alias continuation

For HTTP(S) domain extraction, the exact `loopback` DNS alias now projects to
`localhost`, as in [.NET DomainNameHelper.ParseCanonicalName](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.Uri/src/System/DomainNameHelper.cs).
Trailing-dot names remain distinct. Extracted hosts use the shared invariant
lowercase operation. This correction is limited to known HTTP(S) syntax:
[Uri host construction](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.Uri/src/System/Uri.cs)
dispatches DNS, UNC, basic and unknown host types differently. The existing manual
parser does not establish port, IDN, IPv4/IPv6 canonicalization, file/custom-scheme
or complete Uri.Host equivalence. Those remain explicit source follow-ups.

## Evidence and limits

Independent source specification/quality review accepted the frozen implementation.
Production compilation is parent-owned and recorded in the production gate log.
No tests, fixtures, harnesses, filesystem/runtime probes or CI were run for this
source slice. Full merge corpus and native acceptance remain open. See
[maintenance Unicode admission](maintenance-unicode-contract.md) and
[ordinal dataset provenance](ordinal-name-matching.md) for retained source/license
and exact semantic digests.

## Qualified HTTP IP domain continuation

HTTP(S) domain extraction now canonicalizes qualified numeric IP authorities
before the existing DNS path. The new private adapter requires at most one
userinfo separator and an empty or ASCII-decimal port within65535 (a bare colon
is admitted). IPv4 parsing supports one to four components with decimal,
leading-zero octal and `0x`/`0X` hexadecimal. Prefix components must fit one byte;
the last component has32/24/16/8bits according to the component count. Checked
u32 accumulation rejects overflow and malformed parts. Successful values emit
four decimal bytes. A failed IP parse retains the existing DNS path.

Bracketed standard IPv6 addresses parse into eight16-bit words. Formatting
follows the pinned longest zero run (minimum two, first tie), lowercase hex
and embedded IPv4 decisions for compatible/mapped, SIIT and ISATAP shapes.
The formatter checks embedded output at word6 before compression, preserving
the reference behavior for an ISATAP tail of zero words. Brackets are retained
in the host and URI ports are omitted from the domain key.

Primary sources are [.NET Uri host construction](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.Uri/src/System/Uri.cs)
`CheckAuthorityHelper` and `CreateHostStringHelper`, [IPv4 canonicalization](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.Uri/src/System/IPv4AddressHelper.cs),
[IPv4 grammar](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/Common/src/System/Net/IPv4AddressHelper.Common.cs),
[IPv6 formatting](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.Uri/src/System/IPv6AddressHelper.cs)
and [IPv6 compression/embedding decisions](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/Common/src/System/Net/IPv6AddressHelper.Common.cs).
The reference implementation is distributed under the .NET Foundation MIT license.

Rust's standard IPv6 parser supplies a qualified admission subset. Scope IDs,
prefix lengths, unusual embedded spellings, Unicode IRI/IDN, full malformed
authority handling and custom/file schemes remain separate boundaries. The
legacy fallback is unchanged for inputs outside this correction; this slice
does not establish whole Uri.Host admission equivalence. Source specification
and quality review and parent production gates are recorded separately. No
runtime URL parsing, test fixtures, probes or harnesses were run.
