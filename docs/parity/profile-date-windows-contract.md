# Windows profile date provider: bounded source contract

## Status and integration

The bounded Windows host-local provider is implemented in production source. Native intake and ordinary rules were independently reviewed and passed the parent-owned production gates at checkpoint `5102edc`. The pure rule extensions and fallback bridge described below have been independently source reviewed and passed the production gates recorded in the current gate receipt. Supported conversions canonicalize through an immutable native snapshot. Unsupported local dates retain authored text with a warning. Pure UTC and Unspecified canonicalization works independently of a host zone. This document records no native execution or platform acceptance.

The provider preserves the existing `DateValue`, `Kind` and `ContextSnapshot` interfaces. A decode/save call captures one immutable snapshot lazily and reuses it for both profile date fields, including a captured failure. Provider errors retain the authored string and emit the existing bounded warning containing the field key and provider context. Diagnostics omit authored profile text.

Source ownership:

| Source | Responsibility |
| --- | --- |
| `src/profiles/date/local/windows.rs` | Native snapshot capture, registry handle ownership and qualification |
| `src/profiles/date/local/windows/rules.rs` | Pure rule construction, offset selection and fold handling |
| `src/profiles/date/local.rs` | Windows cfg dispatch; existing context API |
| `src/profiles/date/local/calendar.rs` | Shared tick/calendar primitives implemented by the Unix provider owner |
| `Cargo.toml` | Add only the required `Win32_System_Time` feature to existing Windows bindings |

## Reference authority

The pinned [.NET 9 Windows loader](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Win32.cs) obtains the local key and disabled-DST flag, then loads its registry rules. Historical conversion uses those rules; its separate one-year cache supports `DateTime.Now`. Default `TZI.Bias` supplies the base offset. Dynamic annual records extend the first rule toward year 1 and the last toward year 9999. Missing Dynamic DST uses the default record. Registry rule construction uses the bias delta and negated DaylightBias; StandardBias is not added. Successful registry loading with disabled DST removes all adjustment rules, including historical bias changes.

The native [GetDynamicTimeZoneInformation contract](https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-getdynamictimezoneinformation) identifies the configured local zone. [GetTimeZoneInformationForYear](https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-gettimezoneinformationforyear) exposes annual native information; it will not replace the reference registry loader or the pure conversion algorithm. The initial provider will not call native per-date conversion APIs.

## Immutable native capture

Admission requires a valid, nonempty, NUL-terminated UTF-16 `TimeZoneKeyName`, at most 127 code units, containing one registry key component. Native failure, missing keys and fallback lookup forms remain unsupported with a warning; they do not invent UTC.

Capture pins read-only handles under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones\<key>` and its optional `Dynamic DST` child, using the process-default registry view. It requires exact REG_BINARY 44-byte TZI records and exact REG_DWORD year bounds, with `1 <= FirstEntry <= LastEntry <= 9999`. Every required annual value must be present. Resource limits are 1,024 annual records and 1 MiB total admitted data across both passes, with a fixed bounded number of registry reads. There is no retry loop for a changing registry.

The intake compares relevant [registry last-write stamps](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryinfokeyw) before and after reading. It reopens the keys and rereads the complete payload once, then repeats the native zone query and compares its typed fields. Exact [value types and lengths](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryvalueexw) are part of admission. Detected changes fail the shared snapshot. These observations provide a bounded consistency check; they do not constitute an atomic transaction across Windows configuration stores.

Windows zone authority comes from the OS configuration. Unix `TZ`/`TZDIR` overrides are not applied to this provider. Snapshot lifetime remains per decode/save, an explicit difference from the reference's process-wide local-zone cache.

## Pure conversion contract

The immutable representation contains a base offset and ordered adjustment rules with date ranges, effective standard offsets, daylight deltas and fixed/floating transitions. Reference operation names guide the port: `get_adjustment_rule_for_time`, `get_utc_offset_from_utc`, `get_daylight_time`, `get_is_daylight_savings_from_utc`, `get_is_daylight_savings`, `check_is_dst` and `transition_time_to_date_time`.

The shared [.NET rule engine](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.cs) selects ordinary UTC rules using the date after adding the current base offset. Transition UTC offsets differ at start and end. The port preserves northern/southern seasons, positive/negative daylight deltas and the carried fold bit. Year markers consult one neighboring year; successful stitching suppresses calendar-year rebasing. Bias-only rules retain their sentinel transitions for these helpers. Cross-year ambiguity retries use `AddYears(+1)`, then `AddYears(-1)`, swallowing range errors only in those optional retries. Endpoint UTC lookup selects local MinValue/MaxValue rules below January 2, year 1, or above December 31 midnight, year 9999. Rule admission validates the effective daylight total; a separately limited standard component would reject reference-admitted rules.

All calculations use 100 ns ticks. Common calendar helpers centralize tick conversion, separately named checked/clamped shifts, calendar-year changes with leap-day clamping, and nth/last weekday construction. Rule lookup, marker stitching and fold policy remain provider-specific. The existing Unix normalization algorithm is unchanged by Windows intake.

Numeric ISO zones identify an instant for host conversion; Microsoft offset tokens request host conversion without using the token's numerical offset. The final local clock and fold bit pass through a second local-offset selection for serialization. Existing Newtonsoft out-of-range handling remains part of this flow. [.NET DateTime](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/DateTime.cs) preserves ambiguity when conversion is in range and clamps its endpoints, clearing the fold bit on saturation. Transition arithmetic remains checked; `AddYears` supplies the shared leap-day contract.

The full-date numeric-offset fallback has a separate `normalize_fallback` seam following [.NET DateTimeParse.AdjustTimeZoneToLocal](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/DateTimeParse.cs). It includes first-day handling and reselects the original Unspecified wall offset when adjusted UTC is out of range. Its final out-of-range result produces a typed invalid-date error rather than fast-path clamping. The existing snapshot and final local serializer are shared.

Marker predicates and the 24-hour delta transfer follow [.NET AdjustmentRule](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.AdjustmentRule.cs). Floating January transitions can be markers too. Delta construction transfers exactly one day between daylight and standard components before validating the resulting delta and effective offset; it does not repeatedly reduce the delta. A separate `has_daylight` flag preserves sentinel transitions without enabling daylight calculations for bias-only rules.

## Remaining unsupported forms and follow-ups

The provider retains authored text with a warning for:

- Missing, inaccessible, changing, oversized or malformed snapshots, including missing annual records.
- Empty/bogus key fallback searches, native custom-zone fallback and dummy-UTC fallback.
- Invalid or unsupported transition structures or effective offsets.
- Checked transition/marker arithmetic that exceeds the DateTime range. The adapter preserves authored text rather than inventing a clamped transition; final fast-path conversion saturation remains a separate operation.

Native fallback discovery remains a separate source scope. Parser acceptance beyond the admitted deterministic fallback remains separately tracked; this provider does not complete broad DateTime parsing or all local-zone parity.

No tests, fixtures, builds, runtime probes or native API calls were performed by the implementing agent. Source implementation, independent review, compilation and native acceptance remain separate receipts.
