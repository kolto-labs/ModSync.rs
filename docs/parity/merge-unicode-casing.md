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

## Evidence and limits

Independent source specification/quality review accepted the frozen implementation.
Production compilation is parent-owned and recorded in the production gate log.
No tests, fixtures, harnesses, filesystem/runtime probes or CI were run for this
source slice. Full merge corpus and native acceptance remain open. See
[maintenance Unicode admission](maintenance-unicode-contract.md) and
[ordinal dataset provenance](ordinal-name-matching.md) for retained source/license
and exact semantic digests.
