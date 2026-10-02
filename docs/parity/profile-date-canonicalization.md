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

The provider models historical standard/daylight intervals, minute normalization of historical second offsets, adjustment offset limits, the pre-transition standard type, and supported POSIX future rules. Supported future forms include month/week/weekday, Julian dates, and the reference's restricted numeric-day rules. It deliberately reproduces the reference's transition-time day stripping. Fixed future offsets use the same effective-total normalization. A file without transitions uses the last standard type as the base, matching the reference constructor.

UTC-to-local conversion retains the ambiguous daylight-side bit. Serialization independently reselects the offset for the final local clock and fold bit, using the same 100 ns serializer as pure dates. Historical rule ends remain inclusive while ambiguity intervals remain end-exclusive; these are distinct source contracts. The original Unspecified wall clock is retained for Newtonsoft's out-of-range explicit-offset branch. UTC endpoint lookup now selects the local MinValue/MaxValue rule through the shared effective-rule selector, before conversion clamping. Arithmetic that exceeds the reference DateTime range still preserves authored text with a warning.

Primary provider references are [.NET 9 Unix timezone construction](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Unix.cs), [Unix zone-file selection](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Unix.NonAndroid.cs), and [rule lookup and fold handling](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.cs).

## Remaining boundary and named follow-ups

- Complete acceptance evidence for adjacent-year markers, daylight day transfers, cross-year transitions and endpoint selection. The shared reference operations below are implemented in source, but were not exercised by this source-only slice.
- The bounded Windows native snapshot provider is implemented separately and independently source reviewed. Its admitted forms and remaining rule shapes are recorded in [the Windows date contract](profile-date-windows-contract.md); unsupported Windows values retain authored text with warnings.
- Extend malformed/unusual zone-file acceptance only from primary source proof. Missing, unstable, oversized, inaccessible or unsupported snapshots preserve authored strings instead of silently inventing UTC. Warning text contains provider context, not authored profile date text.
- The reference caches `TimeZoneInfo.Local` process-wide. Rust snapshots per decode/save, so a timezone/environment change between calls can produce different normalization; both fields within one call share the same captured data. This is an explicit cache-lifetime difference.
- Broad invariant `DateTime.TryParse` acceptance and rounding remain incomplete. Existing other fallback strings remain authored; no guessed culture table or time-only/current-date parsing was added.

This is a source implementation receipt. No tests, fixture generation, runtime date probes, builds or CI were run by the implementing agent. Production compilation and independent review are separately owned.

## Neighbor-year and endpoint source continuation

The continuation replaces the earlier explicit marker, cross-year, extreme daylight-delta and endpoint refusals with shared bounded reference operations. Remaining invalid or unproved arithmetic retains the authored text.

1. Canonical fixed/floating transition definitions retain the reference marker fields. Future daylight deltas use the one-day standard/daylight transfer before transition offsets are calculated. Anchor: `TimeZoneInfo.AdjustmentRule.cs` `AdjustDaylightDeltaToExpectedRange`, `ValidateAdjustmentRule`, and both year-marker predicates.
2. A private effective-rule view represents historical intervals and the final POSIX replacement. UTC/local selection and previous/adjacent-year lookup share that view. Anchors: `TimeZoneInfo.cs` `GetAdjustmentRuleForTime`, `CompareAdjustmentRuleToDateTime`, `GetPreviousAdjustmentRule`, and `GetDaylightTime`.
3. Marker stitching follows `GetIsDaylightSavingsFromUtc`, `TryGetStartOfDstIfYearEndWithDst`, and `TryGetEndOfDstIfYearStartWithDst`. Cross-year recurring comparisons follow `CheckIsDst`; ambiguity retains the source's adjacent-year retry. These are fixed-count neighboring lookups, not year enumeration or recursive rule expansion.
4. Endpoint support is implemented as a separate operation using `GetUtcOffsetFromUtc`'s first/last-day selection and `DateTime.ToLocalTime` clamping. Checked arithmetic and clamping remain separate operations. Local reserialization continues to reselect the final offset with the retained fold bit.

A shared private calendar module now supplies Unix and the separately owned Windows provider with: tick constants/conversion, checked and clamped shifts, exact `DateTime.AddYears` leap-day clipping, and month/week/weekday date construction. Calendar helpers share mechanics only; platform capture, rule selection and DST fold policy remain in their providers. No public profile structure changes are required.

Additional primary anchor: [.NET 9 adjustment rule validation and marker predicates](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.AdjustmentRule.cs), and [DateTime calendar arithmetic and local conversion](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/DateTime.cs). The effective-rule selector substitutes the final POSIX rule before local interval comparisons, including previous-rule offset use. It preserves the reference binary-search ordering. Neighbor-year lookups use this same selector across the historical/future seam. Historical interval comparisons remain inclusive; recurring ranges use the reference year adjustment and end-exclusive rule. Local ambiguity honors marker suppression, while UTC ambiguity retains adjacent-year retries. No year enumeration or additional zone reads occur.

## Bounded invariant fallback continuation

The pinned profile serializer does not override `Culture`. Newtonsoft's default is `InvariantCulture` (`JsonSerializerSettings.DefaultCulture`), and `JsonReader.ReadDateTimeString` invokes `DateTime.TryParse` with `RoundtripKind` after the specialized readers. This work therefore needs invariant Gregorian grammar, not current-user date ordering or native locale discovery.

Current source classification, without runtime probes:

| Input shape | Current route |
| --- | --- |
| Supported direct ISO/MS | Typed canonicalization and supported local normalization |
| Full ISO date or zone-free full seconds clock, at most seven fraction digits | Deterministic canonical subset |
| Other Chrono-admitted forms outside the new explicit fallback grammar | Authored string retained; broad framework normalization remains incomplete |
| Full ISO date with hours/minutes but no seconds | Canonicalized by the new bounded fallback |
| Dot/comma fractional seconds on a full-date seconds clock | Binary64 accumulation and nearest-even tick rounding |
| Slash- or dash-separated full numeric dates with four-digit year first/last, or day/month-name/year and month-name/day/year | Canonicalized within the explicit bounded forms |

The bounded fallback implements complete ISO-style dates with `HH:mm` or `HH:mm:ss`, decimal point/comma fraction, and explicit zone or no zone. Keep original-text specialized dispatch first. The fallback fraction algorithm reproduces binary64 digit accumulation and nearest-even tick rounding; simple decimal truncation is not equivalent. Rounded carry must pass through checked calendar arithmetic.

Fallback local conversion uses a distinct typed route. `DateTimeParse.AdjustTimeZoneToLocal` validates the explicit offset within fourteen hours and rejects some out-of-range results that the fast Newtonsoft route clamps. Its first-day adjustment also differs. Reusing fast-parser LocalRequired semantics would erase that distinction. Both provider bridges use their existing immutable snapshot. Confirmed out-of-range fallback conversion returns a typed invalid-date error; unsupported provider operations preserve authored values with warnings. The numeric/month-name extension uses fixed `CalendarData.Invariant` ordering and vocabulary, with an explicit four-digit year and full calendar date. Missing dates, time-only values, two-digit-year inference and the complete framework whitespace/punctuation grammar remain outside this slice. The bounded weekday/AM-PM/wrapper extension is described below.

Primary anchors: [Newtonsoft default invariant culture](https://raw.githubusercontent.com/JamesNK/Newtonsoft.Json/13.0.4/Src/Newtonsoft.Json/JsonSerializerSettings.cs), [reader fallback dispatch](https://raw.githubusercontent.com/JamesNK/Newtonsoft.Json/13.0.4/Src/Newtonsoft.Json/JsonReader.cs), [.NET fallback parser](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/DateTimeParse.cs) (`ParseFraction`, `ParseISO8601`, `DetermineTimeZoneAdjustments`, `AdjustTimeZoneToLocal`), and [fixed invariant calendar data](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/CalendarData.cs). This section records source implementation and primary reference review; runtime acceptance is not claimed.

The Unix rule constructor first performs the reference one-day transfer and validates the resulting daylight delta. Effective daylight totals outside fourteen hours then shift the standard component into the supported total range. Fixed no-DST rules clamp their standard component through the same operation. Historical intervals and future rules share this construction order. Standard components of daylight rules are not independently limited to fourteen hours.


## Bounded weekday, AM/PM and delimiter extension

The generic fallback now accepts one leading invariant full or abbreviated English weekday, followed by whitespace with an optional comma. Duplicate weekday tokens are rejected. The weekday is checked against the rounded full wall date before timezone normalization; fractional rollover therefore affects the check. A whitespace-separated terminal weekday is also admitted; prefix plus suffix, or repeated suffix weekdays, reject. Weekday plus ISO `T`/`t` clock remains outside this bounded route.

Space-separated full-date clocks accept one or two digits per clock component. An hour-only clock requires one AM/PM suffix. The reference's generic `AdjustHour` behavior is retained: AM permits 0..12 and maps12 to0; PM permits0..23 and adds12 only below12. Markers are case-insensitive and require a suffix boundary (end or whitespace before an optional zone). Repeated markers reject. The ISO `T`/`t` route retains its distinct two-digit hour/minute grammar and does not gain AM/PM acceptance.

Before calendar parsing, the adapter validates optional paired outer hashes and a terminal NUL-only suffix. Unpaired/interior/extra hashes or any non-NUL character following the first NUL reject. Whitespace outside the enclosing hashes is admitted; punctuation is not stripped globally. Commas remain limited to the documented weekday/month-day token boundary or fractional-second position. Unsupported punctuation arrangements and missing-date/two-digit-year inference remain excluded.

Primary anchors in `DateTimeParse.cs`: `AdjustHour`1887–1911, `GetTimeOfN`1913–1931, weekday-token uniqueness893–907, weekday validation after fraction rounding2555–2564, `VerifyValidPunctuation`1097–1164, and the separate `ParseISO8601` routine2786 onward. These are source implementation claims only; no parser probes, tests, harnesses or runtime validation were performed.

## Explicit full-date token continuation

The generic space-clock route now accepts terminal case-insensitive `GMT` as UTC, using the same typed UTC serialization as `Z`. It does not admit the word `UTC`, and the separate ISO `T`/`t` route does not admit `GMT`. Lowercase `t` follows the same strict ISO clock grammar as uppercase `T`; it does not inherit generic AM/PM or weekday handling.

Numeric full dates can use homogeneous slashes or dashes. A four-digit first field selects year/month/day; a four-digit last field selects invariant month/day/year. Month and day remain one or two digits and the calendar date must exist. Mixed separators, implicit years, time-only inputs and current-date inference are excluded. The date prefix is normalized without rewriting the remaining clock or zone tokens.

A single invariant weekday may appear at the beginning or as a whitespace-separated final token, including after the generic zone token. Duplicate prefix/suffix weekdays reject. Both positions use the rounded wall date before local conversion, and neither is passed through the separate ISO parser. Terminal commas and other additional weekday punctuation remain outside this bounded route.

Primary anchors: `DateTimeParse.cs` timezone token handling1021–1038 (GMT/Z), weekday uniqueness893–907, post-rounding weekday check2555–2564, and the generic state reset2500–2506; `DateTimeFormatInfo.cs` token insertion1970–1975 and case-insensitive token matching2143–2145 establish lowercase `t` and GMT case handling. The reference's invariant short-date pattern supplies month/day/year ordering. This extension is source-only; no runtime parser evaluation, tests or harnesses were run.

## Effective-total normalization continuation

The private Unix adjustment constructor now mirrors the source's two-stage
construction. First it moves one day between the standard component and daylight
delta, then validates the delta within [-23,+14] hours. Only when the effective
total lies outside [-14,+14] hours does it shift the standard component by the
clamp difference and reconstruct the rule. That reconstruction invokes the
same single-day transfer once more; it is not a repeated normalization loop.
A remaining delta outside [-12,+12] after the first call can therefore change
during the second construction while the clamped effective total stays fixed.

The shared constructor covers historical DST/standard intervals, recurring
POSIX rules and fixed future offsets. Future transition creation still occurs
before rule construction, so an unsupported numeric-day tail retains the final
historical rule rather than failing early on an unused future delta. Existing
TZif intake and POSIX hour bounds keep all component arithmetic within i32; the
base offset still must qualify within fourteen hours. Invalid initial deltas
remain errors before any total clamping, as in the reference.

Primary anchors: [.NET Unix rule generation and normalization](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Unix.cs)
`TZif_GenerateAdjustmentRule`, `TZif_CreateAdjustmentRuleForPosixFormat` and
`NormalizeAdjustmentRuleOffset`; [internal adjustment factory](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.AdjustmentRule.cs)
`CreateAdjustmentRule`, `AdjustDaylightDeltaToExpectedRange` and validation.
This is source implementation; independent review and parent production gates
are recorded separately. No runtime zone-file evaluation, tests or harnesses ran.
