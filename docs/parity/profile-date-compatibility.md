# Profile date reader: bounded ISO compatibility

## Confirmed reference contract

The pinned ModSync.Core project references Newtonsoft.Json 13.0.4.
`Services/Profiles/ProfileService.cs:38` configures formatting and null inclusion,
without changing date culture or zone handling. The upstream
[JsonReader](https://github.com/JamesNK/Newtonsoft.Json/blob/13.0.4/Src/Newtonsoft.Json/JsonReader.cs)
defaults to invariant culture and RoundtripKind and tries the framework DateTime
parser after its specialized parser. This is distinct from settings CLI numeric
parsing, which uses the current culture.

The upstream
[DateTimeParser](https://github.com/JamesNK/Newtonsoft.Json/blob/13.0.4/Src/Newtonsoft.Json/Utilities/DateTimeParser.cs)
accepts hour 24 only at zero minutes, seconds and fractional ticks. Numeric zone
hours/minutes are two digits with a parser limit of 99; the DateTime path does
not impose DateTimeOffset's 14-hour limit. The
[DateTimeUtils](https://github.com/JamesNK/Newtonsoft.Json/blob/13.0.4/Src/Newtonsoft.Json/Utilities/DateTimeUtils.cs)
constructor advances hour 24 to the following day before zone handling.

## Implemented source correction

`src/profiles/date.rs` adds an allocation-free parser limited to 19–40 bytes:

- exact ISO calendar/time separators and four-digit year, valid calendar dates;
- optional 1–7 fractional digits and uppercase/lowercase UTC marker;
- numeric offsets in `+HH`, `+HHmm`, or `+HH:mm` form, including negative forms;
- `24:00:00` with optional all-zero fraction, including date/year rollover;
- rejection of rollover beyond year 9999, leap seconds, and nonzero minute,
  second or fractional fields accompanying hour 24.

The existing shared persisted-profile decoder calls this helper before its
previous ISO/date-only/Microsoft-date handling. Public profile field types and
serialization are unchanged: authored date strings remain strings. This change
accepts confirmed missing spellings; it does not normalize them to the host local
time zone or invent date values from the current time.

The broader framework invariant-culture fallback, local-zone normalization,
reference edge-case spellings outside these explicit forms, and all native/runtime
acceptance remain separate. No culture-name table or guessed day/month ordering
was introduced. Existing parser acceptance outside this additional path was not
changed. No tests, fixtures, harnesses, builds or runtime probes were performed.
