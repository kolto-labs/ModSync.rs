# Windows profile date provider: bounded source contract

## Status and integration

The bounded Windows host-local provider is implemented in production source. Native intake and ordinary rules were independently reviewed and passed the parent-owned production gates at checkpoint `5102edc`. Pure rule extensions and the parser fallback bridge passed gates at `1d8ea0c`. Native fallback discovery passed independent source review and the combined Linux production build/strict-lint and Windows GNU compile/link gates recorded in the production receipt. The system ICU alternative-ID extension below passed independent source review and awaits parent-owned gates. Supported conversions canonicalize through an immutable native snapshot. Unsupported local dates retain authored text with a warning. Pure UTC and Unspecified canonicalization works independently of a host zone. This document records no native execution or platform acceptance.

The provider preserves the existing `DateValue`, `Kind` and `ContextSnapshot` interfaces. A decode/save call captures one immutable snapshot lazily and reuses it for both profile date fields, including a captured failure. Provider errors retain the authored string and emit the existing bounded warning containing the field key and provider context. Diagnostics omit authored profile text.

Source ownership:

| Source | Responsibility |
| --- | --- |
| `src/profiles/date/local/windows.rs` | Native snapshot capture, registry handle ownership and qualification |
| `src/profiles/date/local/windows/iana.rs` | Qualified system ICU module lifetime, mapping and authority observations |
| `src/profiles/date/local/windows/rules.rs` | Pure rule construction, offset selection and fold handling |
| `src/profiles/date/local.rs` | Windows cfg dispatch; existing context API |
| `src/profiles/date/local/calendar.rs` | Shared tick/calendar primitives implemented by the Unix provider owner |
| `Cargo.toml` | Required Time, LibraryLoader and SystemInformation features in existing Windows bindings |

## Reference authority

The pinned [.NET 9 Windows loader](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Win32.cs) uses the native key/disabled flag and historical registry rules; its separate one-year cache supports `DateTime.Now`. Default Bias supplies the base. First/last annual rules extend to DateTime bounds; missing Dynamic DST uses the default. Registry math uses the bias delta and negated DaylightBias, ignoring StandardBias. Disabled registry loading removes all adjustments, including bias changes.

The native [GetDynamicTimeZoneInformation contract](https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-getdynamictimezoneinformation) identifies the configured local zone. [GetTimeZoneInformationForYear](https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-gettimezoneinformationforyear) exposes annual native information; it will not replace the reference registry loader or the pure conversion algorithm. The initial provider will not call native per-date conversion APIs.

## Immutable native capture

Admission requires a valid NUL-terminated UTF-16 `TimeZoneKeyName`, at most 127 code units. A nonempty key must be one bounded registry component: backslash, NUL, LF, CR and dot components are rejected. Forwardslash is a literal registry-name character and permits an IANA ID to reach mapping. Native standard/daylight names must terminate within their 32-code-unit fields. An empty key invokes discovery. A semantic nonempty direct-load failure invokes the qualified mapping branch before ordered native discovery.

Capture pins read-only handles under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones`, its candidate keys and optional `Dynamic DST` children, using the process-default registry view. Qualified rule records are REG_BINARY 44-byte TZI and REG_DWORD year bounds within years 1–9999. Missing or malformed records within admitted types are known semantic load failures and can select the reference fallback chain. Unsupported types, inaccessible data and over-budget intake remain warning failures. Limits are 1,024 keys per enumeration, 255 UTF-16 code units per enumerated name, 1,024 annual records per load and 2,048 bytes per value. The aggregate limits across both decision passes are 1 MiB admitted data and 32,768 registry calls. There is no changing-registry retry loop.

The intake compares relevant [registry last-write stamps](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryinfokeyw) before and after reading. It reopens keys and replays the complete decision witness once: failed direct lookup, alternative mapping and mapped-load observations, ordered enumeration, visited candidate prefix, selected generic-load observations and terminal branch. It then repeats the native zone query and compares typed fields. Exact [value types and lengths](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryvalueexw) are part of admission. Detected changes fail the shared snapshot. These observations provide a bounded consistency check; they do not constitute an atomic transaction across Windows configuration stores.

## Native fallback discovery

Reference order is returned key, first comparison match, then native custom construction. Both registry loads use the generic alternative-ID hook on semantic failure. First-match load failure stops comparison search even if mapping cannot resolve it. Matching uses Bias/StandardBias/StandardDate and ordinal `Std`; equal native dates bypass daylight comparison, otherwise DaylightBias/DaylightDate must match. Search's disabled flag remains false, including its mapped load; the returned-key path retains the original native flag. Native construction retries without transitions, then selects UTC if both attempts fail. Native `TIME_ZONE_ID_INVALID` selects UTC too. Operational or qualification failures never select UTC.

[Native enumeration](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regenumkeyexw) may return keys in any order. The provider preserves native index order without sorting and compares the ordered list between passes. Its ordinal standard-name comparison follows [RegistryKey](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/Microsoft.Win32.Registry/src/Microsoft/Win32/RegistryKey.cs) string mechanics for admitted REG_SZ: odd final bytes are zero-padded and exactly one trailing NUL is removed. Native names use the terminated UTF-16 prefix. MUI resources and translations are not used for matching.

Failed native queries discard their output fields. The explicit INVALID branch requires a second INVALID result; mixed invalid/success statuses retain authored text. Native custom construction uses one all-date rule, or the qualified constant native bias on disabled retry, matching the reference fallback rather than supplying historical registry data.

Windows zone authority comes from the OS configuration. Unix `TZ`/`TZDIR` overrides are not applied to this provider. Snapshot lifetime remains per decode/save, an explicit difference from the reference's process-wide local-zone cache.

## Qualified system ICU alternative IDs

The generic [.NET lookup](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.cs) tries an alternative ID after a semantic non-success. Both the returned native key and the first comparison-selected registry name use that hook. A mapped Windows ID is loaded once with the original branch's disabled flag. A successful no-map observation or semantic mapped-load failure resumes the existing fallback chain; it does not search later comparison candidates or recursively map another ID.

The [.NET conversion gate](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.FullGlobalizationData.cs) disables mapping in invariant or NLS mode. This profile captures bounded `DOTNET_SYSTEM_GLOBALIZATION_INVARIANT` and `DOTNET_SYSTEM_GLOBALIZATION_USENLS` observations, accepting absent/empty or `0`/`false` as default false and `1`/`true` as true (ASCII case-insensitive). Malformed configured values are unsupported. True mode disables mapping without loading ICU. Arbitrary managed AppContext/runtimeconfig settings are not observable from the Rust process: support means the declared default-system profile or the qualified environment modes, not equivalence to every externally configured .NET process.

The default reference [loader](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/native/libs/System.Globalization.Native/pal_icushim.c) tries System32 `icu.dll` before split legacy libraries. This slice admits only the combined module. It uses `LoadLibraryExW` with `LOAD_LIBRARY_SEARCH_SYSTEM32`, then validates the loaded module path against `GetSystemDirectoryW` before resolving exports or making ICU calls. One RAII module handle is held through both complete decision passes. Paths are bounded to 1,023 UTF-16 units and compared with ASCII case-insensitivity. Loaded-module reuse and [DLL redirection](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw) cannot silently select an application-directory image. Split legacy libraries, app-local ICU and other loading profiles remain unsupported; [Microsoft's ICU packaging contract](https://learn.microsoft.com/en-us/windows/win32/intl/international-components-for-unicode--icu-) distinguishes combined-library use from legacy COM initialization.

The dynamically resolved mapper has the [SDK C ABI](https://raw.githubusercontent.com/microsoft/win32metadata/main/generation/WinSDK/RecompiledIdlHeaders/um/icu.h), not the Windows system calling convention. `ucal_getWindowsTimeZoneID` receives terminated UTF-16 input with length `-1` and a fixed 100-unit output buffer, matching the [reference shim](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/native/libs/System.Globalization.Native/pal_timeZoneInfo.c). Input is bounded to 255 units for a selected registry name (the native returned key remains 127). Status at most zero is ICU success, including warnings. Returned lengths 1–100 are read by length, permitting the nonterminated length-100 warning; output must be valid UTF-16 and a qualified Windows registry component. Successful zero length is no mapping. Missing optional mapper export is a witnessed no-map result. Other status, invalid output or excess length is a warning failure; the buffer is never grown. No hand-maintained mapping or case conversion is used.

Required metadata queries bracket each conversion: `u_getVersion`, `u_getDataVersion`, `ulocdata_getCLDRVersion`, `ucal_getTZDataVersion` and `u_getDataDirectory`. Version arrays have four bytes, TZ version is at most 64 bytes, and directory text is at most 256 bytes. Environment observations are at most 256 UTF-16 units each. Nonempty app-local ICU, `ICU_DATA` or a nonempty ICU data directory is unsupported. [ICU data selection](https://unicode-org.github.io/icu/userguide/icu_data/) can involve application-provided common data and directory overrides; this provider neither supplies data nor invokes global setters or cleanup. Arbitrary external in-process ICU mutation is outside the admitted profile. Version labels and duplicate mapping results are consistency witnesses, not a cryptographic proof of data provenance or atomicity.

Each mapping's configuration and metadata are rechecked immediately afterward and included in the complete two-pass witness. Required query failures remain warning errors. The maximum is 48 ICU function calls across both passes, separate from the existing aggregate registry limits; loader/export resolution is one fixed module acquisition and six fixed export lookups. Diagnostics do not echo raw environment values. No ICU API is called after snapshot construction; conversion uses the immutable registry/native rules.

## Pure conversion contract

The immutable representation contains a base offset and ordered adjustment rules with date ranges, effective standard offsets, daylight deltas and fixed/floating transitions. Reference operation names guide the port: `get_adjustment_rule_for_time`, `get_utc_offset_from_utc`, `get_daylight_time`, `get_is_daylight_savings_from_utc`, `get_is_daylight_savings`, `check_is_dst` and `transition_time_to_date_time`.

The shared [.NET rule engine](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.cs) selects ordinary UTC rules using the date after adding the current base offset. Transition UTC offsets differ at start and end. The port preserves northern/southern seasons, positive/negative daylight deltas and the carried fold bit. Year markers consult one neighboring year; successful stitching suppresses calendar-year rebasing. Bias-only rules retain their sentinel transitions for these helpers. Cross-year ambiguity retries use `AddYears(+1)`, then `AddYears(-1)`, swallowing range errors only in those optional retries. Endpoint UTC lookup selects local MinValue/MaxValue rules below January 2, year 1, or above December 31 midnight, year 9999. Rule admission validates the effective daylight total; a separately limited standard component would reject reference-admitted rules.

All calculations use 100 ns ticks. Common calendar helpers centralize tick conversion, separately named checked/clamped shifts, calendar-year changes with leap-day clamping, and nth/last weekday construction. Rule lookup, marker stitching and fold policy remain provider-specific. The existing Unix normalization algorithm is unchanged by Windows intake.

Numeric ISO zones identify an instant for host conversion; Microsoft offset tokens request host conversion without using the token's numerical offset. The final local clock and fold bit pass through a second local-offset selection for serialization. Existing Newtonsoft out-of-range handling remains part of this flow. [.NET DateTime](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/DateTime.cs) preserves ambiguity when conversion is in range and clamps its endpoints, clearing the fold bit on saturation. Transition arithmetic remains checked; `AddYears` supplies the shared leap-day contract.

The full-date numeric-offset fallback has a separate `normalize_fallback` seam following [.NET DateTimeParse.AdjustTimeZoneToLocal](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/DateTimeParse.cs). It includes first-day handling and reselects the original Unspecified wall offset when adjusted UTC is out of range. Its final out-of-range result produces a typed invalid-date error rather than fast-path clamping. The existing snapshot and final local serializer are shared.

Marker predicates and the 24-hour delta transfer follow [.NET AdjustmentRule](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.AdjustmentRule.cs). Floating January transitions can be markers too. Delta construction transfers exactly one day between daylight and standard components before validating the resulting delta and effective offset; it does not repeatedly reduce the delta. A separate `has_daylight` flag preserves sentinel transitions without enabling daylight calculations for bias-only rules.

## Remaining unsupported forms and follow-ups

The provider retains authored text with a warning for:

- Inaccessible, changing or oversized discovery observations.
- Non-REG_BINARY TZI forms, non-REG_DWORD annual-bound forms and other out-of-range years beyond the qualified 1–9999 profile. The reference's invalid `-1` sentinel or reversed bounds are known semantic rejection instead.
- Expanded or unknown `Std` registry string forms. Generic .NET registry byte-array coercions and environment expansion remain separate acceptance work.
- Unsupported ICU configuration/loading/data profiles, inaccessible or changing required mapping metadata, failing mapping status and unqualified output. A known no-map result remains semantic fallback, including a disabled mapping mode or absent optional mapper export.
- Unqualified UTF-16 key/name forms or unsupported registry path components.
- Checked transition/marker arithmetic that exceeds the DateTime range. The adapter preserves authored text rather than inventing a clamped transition; final fast-path conversion saturation remains a separate operation.

Parser acceptance beyond the admitted deterministic fallback remains separately tracked; this provider does not complete broad DateTime parsing or all local-zone parity. No operational, stability, budget or unsupported-form error is classified as a semantic constructor failure.

No tests, fixtures, builds, runtime probes or native API calls were performed by the implementing agent. Source implementation, independent review, compilation and native acceptance remain separate receipts.

Parent integration subsequently passed Linux production build/strict Clippy in
both feature modes and Windows GNU no-default library/binary compilation and
linking (session35911,54.71s). See the 2026-10-02 production receipt. No Windows
binary or native API probe was executed.
