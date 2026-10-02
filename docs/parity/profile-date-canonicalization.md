# Profile date canonicalization

## Source contract

Pinned `ModSync.Core/Services/Profiles/ProfileService.cs` configures formatting and null inclusion only. `Profile.cs` stores `createdUtc` and `lastUsedUtc` as `DateTime`; the names do not enforce UTC Kind.

Primary references:

- [Newtonsoft.Json 13.0.4 DateTimeUtils](https://raw.githubusercontent.com/JamesNK/Newtonsoft.Json/13.0.4/Src/Newtonsoft.Json/Utilities/DateTimeUtils.cs): direct ISO/Microsoft readers and default ISO writer.
- [Newtonsoft.Json 13.0.4 JsonReader](https://raw.githubusercontent.com/JamesNK/Newtonsoft.Json/13.0.4/Src/Newtonsoft.Json/JsonReader.cs): specialized parse followed by `DateTime.TryParse` with `RoundtripKind`.
- [.NET 9 DateTimeParse](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/DateTimeParse.cs): invariant fallback and its distinct rounding/timezone behavior.

## Implemented production subset

The profile decoder uses original text for the direct ISO/Microsoft routes. Its internal date value distinguishes UTC, Unspecified and a requirement for local normalization. Public profile fields remain strings.

For direct supported ISO timestamps with no zone or Z/z, decoding and persisted output now use `yyyy-MM-ddTHH:mm:ss`, append only nonzero fractional 100 ns ticks (at most seven digits, trailing zeroes removed), and append `Z` only for UTC. Admitted 24:00:00 rolls to the next calendar day; rollover past year9999 is rejected. Fraction parsing stops at seven digits before accumulation can overflow.

Offset-free Microsoft epoch values normalize to the same UTC representation, including negative epochs. Existing bounded Microsoft offset-token acceptance remains. An offset token selects host Local conversion and its numeric value is ignored. Numeric ISO offsets identify the UTC instant that is converted to host Local; the authored offset is not substituted for the host offset. The bounded Unix provider below implements supported conversions; unsupported cases retain authored text with warnings.

The deterministic invariant fallback canonicalizes an exact full calendar date (midnight, Unspecified), or a full date plus a zone-free seconds clock with up to seven fraction digits. This subset allows surrounding whitespace and a space or T between date and clock. It does not infer a missing date or accept24:00 rollover through the fallback route. Microsoft text surrounded by whitespace no longer gains accidental acceptance by trimming into the specialized parser.

Date text is limited to4096bytes before parsing. The existing other Chrono-accepted fallback forms remain authored rather than claiming equivalent .NET normalization. Pure canonical output is also used by the persisted profile decoder, so load/save cycles stabilize within this supported subset.

## Immutable Unix local-zone provider

`profiles/date/local` captures a lazy immutable zone once per profile decode, reused for both date fields. Persisted save validation also decodes both fields with one snapshot. It reads a bounded ordinary TZif file selected by `TZ`, `TZDIR` and Unix localtime paths, checks file identity and metadata around intake, and parses at most 1 MiB, 65,536 transitions, and 256 types. Empty `TZ` selects UTC. Native APIs that mutate process locale or timezone are not used.

The provider models historical standard/daylight intervals, minute normalization of historical second offsets, adjustment offset limits, the pre-transition standard type, and supported POSIX future rules. Supported future forms include month/week/weekday, Julian dates, and the reference's restricted numeric-day rules. It deliberately reproduces the reference's transition-time day stripping. Fixed future offsets are range checked too. A file without transitions uses the last standard type as the base, matching the reference constructor.

UTC-to-local conversion retains the ambiguous daylight-side bit. Serialization independently reselects the offset for the final local clock and fold bit, using the same 100 ns serializer as pure dates. Historical rule ends remain inclusive while ambiguity intervals remain end-exclusive; these are distinct source contracts. The original Unspecified wall clock is retained for Newtonsoft's out-of-range explicit-offset branch. Simple fixed zones can clamp to the DateTime endpoints. Complex endpoint rule selection is rejected rather than approximated.

Primary provider references are [.NET 9 Unix timezone construction](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Unix.cs), [Unix zone-file selection](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Unix.NonAndroid.cs), and [rule lookup and fold handling](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.cs).

## Remaining boundary and named follow-ups

- Implement exact adjacent-year marker stitching for Jan-1-midnight POSIX tails, including adjoining historical rules and UTC transitions crossing calendar years.
- Implement transition rebasing for future daylight deltas beyond 12 hours; historical deltas already transfer one day between daylight and standard components as the reference does.
- Implement first/last DateTime-day rule selection and clamped historical daylight boundaries. These cases currently preserve authored text with a bounded warning.
- Add a separate Windows native snapshot provider. Windows local dates currently remain authored with a warning; pure UTC/Unspecified dates continue canonicalizing.
- Extend malformed/unusual zone-file acceptance only from primary source proof. Missing, unstable, oversized, inaccessible or unsupported snapshots preserve authored strings instead of silently inventing UTC. Warning text contains provider context, not authored profile date text.
- The reference caches `TimeZoneInfo.Local` process-wide. Rust snapshots per decode/save, so a timezone/environment change between calls can produce different normalization; both fields within one call share the same captured data. This is an explicit cache-lifetime difference.
- Broad invariant `DateTime.TryParse` acceptance and rounding remain incomplete. Existing other fallback strings remain authored; no guessed culture table or time-only/current-date parsing was added.

This is a source implementation receipt. No tests, fixture generation, runtime date probes, builds or CI were run by the implementing agent. Production compilation and independent review are separately owned.

## Next approved source contract (implementation held)

The next Unix slice will replace the listed explicit refusals in bounded steps. It is a design contract, not a claim that those paths currently normalize.

1. Canonical fixed/floating transition definitions retain the reference marker fields. Future daylight deltas use the one-day standard/daylight transfer before transition offsets are calculated. Anchor: `TimeZoneInfo.AdjustmentRule.cs` `AdjustDaylightDeltaToExpectedRange`, `ValidateAdjustmentRule`, and both year-marker predicates.
2. A private effective-rule view represents historical intervals and the final POSIX replacement. UTC/local selection and previous/adjacent-year lookup share that view. Anchors: `TimeZoneInfo.cs` `GetAdjustmentRuleForTime`, `CompareAdjustmentRuleToDateTime`, `GetPreviousAdjustmentRule`, and `GetDaylightTime`.
3. Marker stitching follows `GetIsDaylightSavingsFromUtc`, `TryGetStartOfDstIfYearEndWithDst`, and `TryGetEndOfDstIfYearStartWithDst`. Cross-year recurring comparisons follow `CheckIsDst`; ambiguity retains the source's adjacent-year retry. These are fixed-count neighboring lookups, not year enumeration or recursive rule expansion.
4. Endpoint support is a separate follow-on using `GetUtcOffsetFromUtc`'s first/last-day selection and `DateTime.ToLocalTime` clamping. Checked arithmetic and clamping remain separate operations. Local reserialization continues to reselect the final offset with the retained fold bit.

A shared private calendar module is planned for Unix and the separately owned Windows provider: tick constants/conversion, checked and clamped shifts, exact `DateTime.AddYears` leap-day clipping, and month/week/weekday date construction. Calendar helpers share mechanics only; platform capture, rule selection and DST fold policy remain in their providers. No public profile structure changes are required.

Additional primary anchor: [.NET 9 adjustment rule validation and marker predicates](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.AdjustmentRule.cs), and [DateTime calendar arithmetic and local conversion](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/DateTime.cs). Production edits remain held until the parent releases the next checkpoint boundary.
