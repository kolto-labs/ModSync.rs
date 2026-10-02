# Windows profile date provider: proposed source contract

## Status and integration

This is the approved design for a bounded Windows host-local provider. Production implementation is awaiting the parent-owned checkpoint and release. Windows local dates currently retain authored text with a warning. Pure UTC and Unspecified canonicalization already works independently of a host zone. This document records no native execution or platform acceptance.

The provider will preserve the existing `DateValue`, `Kind` and `ContextSnapshot` interfaces. A decode/save call captures one immutable snapshot lazily and reuses it for both profile date fields, including a captured failure. Provider errors retain the authored string and emit the existing bounded warning containing the field key and provider context. Diagnostics must omit authored profile text.

Proposed source ownership:

| Source | Responsibility |
| --- | --- |
| `src/profiles/date/local/windows.rs` | Native snapshot capture, registry handle ownership and qualification |
| `src/profiles/date/local/windows/rules.rs` | Pure rule construction, offset selection and fold handling |
| `src/profiles/date/local.rs` | Windows cfg dispatch; existing context API |
| `src/profiles/date/local/calendar.rs` | Proposed shared tick/calendar primitives; ownership to be coordinated with the Unix provider owner |
| `Cargo.toml` | Add only the required `Win32_System_Time` feature to existing Windows bindings |

## Reference authority

The pinned [.NET 9 Windows loader](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Win32.cs) obtains the local key and disabled-DST flag, then loads its registry rules. Historical conversion uses those rules; its separate one-year cache supports `DateTime.Now`. Default `TZI.Bias` supplies the base offset. Dynamic annual records extend the first rule toward year 1 and the last toward year 9999. Missing Dynamic DST uses the default record. Registry rule construction uses the bias delta and negated DaylightBias; StandardBias is not added. Successful registry loading with disabled DST removes all adjustment rules, including historical bias changes.

The native [GetDynamicTimeZoneInformation contract](https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-getdynamictimezoneinformation) identifies the configured local zone. [GetTimeZoneInformationForYear](https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-gettimezoneinformationforyear) exposes annual native information; it will not replace the reference registry loader or the pure conversion algorithm. The initial provider will not call native per-date conversion APIs.

## Immutable native capture

Admission will require a valid, nonempty, NUL-terminated UTF-16 `TimeZoneKeyName`, at most 127 code units, containing one registry key component. Native failure, missing keys and fallback lookup forms will remain unsupported with a warning; they will not invent UTC.

Capture will pin read-only handles under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones\<key>` and its optional `Dynamic DST` child, using the process-default registry view. It will require exact REG_BINARY 44-byte TZI records and exact REG_DWORD year bounds, with `1 <= FirstEntry <= LastEntry <= 9999`. Every required annual value must be present. Proposed resource limits are 1,024 annual records and 1 MiB total admitted data, with a fixed bounded number of registry reads. No retry loop will wait indefinitely for a changing registry.

The intake will compare relevant [registry last-write stamps](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryinfokeyw) before and after reading, then repeat the native zone query and compare its typed fields. Exact [value types and lengths](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryvalueexw) are part of admission. Detected changes will fail the shared snapshot. These observations provide a bounded consistency check; they do not constitute an atomic transaction across Windows configuration stores.

Windows zone authority will come from the OS configuration. Unix `TZ`/`TZDIR` overrides will not be applied to this provider. Snapshot lifetime remains per decode/save, an explicit difference from the reference's process-wide local-zone cache.

## Pure conversion contract

The immutable representation will contain a base offset and ordered adjustment rules with date ranges, base deltas, daylight deltas and fixed/floating transitions. Reference operation names will guide the port: `get_adjustment_rule_for_time`, `get_utc_offset_from_utc`, `get_daylight_time`, `get_is_daylight_savings_from_utc`, `get_is_daylight_savings`, `check_is_dst` and `transition_time_to_date_time`.

The shared [.NET rule engine](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.cs) selects ordinary UTC rules using the date after adding the current base offset. Transition UTC offsets differ at start and end. Ordinary northern and southern seasons, positive and negative daylight deltas, and daylight-side ambiguous intervals require separate UTC and local calculations. The proposed port will preserve these distinctions and its carried fold bit.

All calculations will use 100 ns ticks. Proposed common calendar helpers will centralize tick conversion, separately named checked/clamped shifts, calendar-year changes with leap-day clamping, and nth/last weekday construction. Rule lookup, marker stitching and fold policy remain provider-specific. The existing Unix normalization algorithm will not be broadly refactored as part of Windows intake.

Numeric ISO zones identify an instant for host conversion; Microsoft offset tokens request host conversion without using the token's numerical offset. The final local clock and fold bit will be passed through a second local-offset selection for serialization. Existing Newtonsoft out-of-range handling remains part of this flow. [.NET DateTime](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/DateTime.cs) preserves ambiguity when conversion is in range and clamps its endpoints; its `AddYears` calendar behavior supplies the shared leap-day contract.

## Initial unsupported forms and follow-ups

The first slice will retain authored text with a warning for:

- Missing, inaccessible, changing, oversized or malformed snapshots, including missing annual records.
- Empty/bogus key fallback searches, native custom-zone fallback and dummy-UTC fallback.
- Beginning/end-of-year daylight markers requiring adjacent-rule stitching.
- Shifted transition or ambiguity windows crossing a calendar year beyond ordinary southern-season ordering.
- Daylight deltas outside ±12 hours requiring the reference's transfer of one day between daylight and base components.
- Adjusted endpoint dates requiring special daylight rule selection; constant zones can use exact endpoint clamping.
- Invalid or unsupported transition structures or effective offsets.

Marker predicates and the 24-hour delta transfer are distinct contracts in [.NET AdjustmentRule](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.AdjustmentRule.cs). Marker detection must not be restricted to fixed-date rules. These cases are named follow-ups; the scoped provider does not complete broad DateTime parsing or all local-zone parity.

No tests, fixtures, builds, runtime probes or native API calls were performed to produce this design. Source implementation, independent review, compilation and native acceptance remain separate receipts.
