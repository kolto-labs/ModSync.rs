# Guide `lastModified` metadata contract

Decided 2026-10-03: follow the reference. The C# value is observable in saved
TOML and YAML guides, so Rust normalizes it on save the same way.

## Reference behavior

Pinned `ModSync.Core` (C# reference at `/home/user/ModOrchestration`):

- `MainConfig.cs:408` stores `public static DateTime? LastModified`.
- TOML intake, `ModComponentSerializationService.cs:2020` and `:2058-2062`.
  The value is reset to null, then only the `lastModified` key of
  `[metadata]` is read. Its `ToString()` goes through
  `DateTime.TryParse(text, CultureInfo.InvariantCulture, DateTimeStyles.None)`.
  For a native TOML date-time, `ToString()` is Tomlyn 0.17.0
  `TomlDateTime.ToString()`. That omits the fraction when its millisecond part
  is zero.
- YAML intake, `:608-615`. It reads `LastModified`, or else `lastModified`, with
  the same `TryParse`.
- When the parse fails, nothing is stored and the field is not written on save.
- TOML save, `:3727-3730`. The code is `metadataTable["lastModified"] = DateTime`.
  Tomlyn writes this as a native local date-time with no offset. It truncates
  to milliseconds and writes `.fff` only when the milliseconds are nonzero.
- YAML save, `:4432-4435`. The output is `lastModified: "{0:O}"`: seven fraction
  digits, plus the host offset for a Local-kind value.
- JSON, XML and Markdown saves (`:4471-4530`) never write the field.

`DateTimeStyles.None` converts a `Z`, `GMT` or explicit offset to host Local
time. A zone-free value stays Unspecified, and no value is ever UTC-kind. The
culture is fixed to invariant, but the time zone is the host's.

## Rust implementation

- `src/profiles/schema.rs` `invariant_try_parse` reuses the shared bounded
  invariant `DateTime.TryParse` model. That is
  `profiles/date/fallback.rs` (`date_compat::fallback::parse`) and the immutable
  local-zone provider (`ContextSnapshot::normalize_fallback`). No new parser was
  added.
  - `DateValue::without_roundtrip_kind` (`src/profiles/date.rs`) maps the UTC
    designator to a zero-offset Local adjustment, as `DateTimeStyles.None` does.
  - The canonical output is widened to the `O` pattern.
- `src/model.rs` `normalize_last_modified` runs in `Guide::render` for TOML and
  YAML output only:
  - **Parsed:** YAML gets the `O` string. TOML gets a native local date-time
    truncated to milliseconds. If the key was `LastModified`, it is renamed to
    `lastModified`.
  - **Rejected** (the reference `TryParse` fails, so the reference omits the
    field): the field is removed and a warning is logged. This covers null,
    Boolean, empty, whitespace-only and digit-free text. It also covers a lone
    signed integer token, typed fallback errors such as a weekday mismatch, an
    offset over 14 hours or a hash or NUL envelope error, and a local conversion
    outside the DateTime range.
  - **Unsupported:** the authored value is kept and a warning is logged. This
    covers forms outside the bounded grammar, such as time-only values (the
    reference infers today's date), impossible calendar dates, `24:00`,
    non-integer numbers and compound values. It also covers local-zone provider
    failures. This is the same policy as the other date fields.
- `src/json_native.rs`: an object that holds only `toml`'s private date-time
  key now serializes as a native TOML date-time. Native TOML date-times that
  Rust read in any field round-trip as date-times. Before this change they
  leaked a `"$__toml_private_datetime"` table.

## Known differences

- The TOML fraction is written by `toml_datetime` with trailing zeros removed
  (`.5`), where Tomlyn writes `.500`. The parsed TOML value is the same.
- YAML string quoting follows the shared YAML writer. The string value is the
  same.
- When both `lastModified` and `LastModified` are present, only the camel-case
  key is normalized and the other is passed through. Rust also passes through
  metadata keys the reference drops.
- The reference rejects some forms that Rust keeps, such as `2024-02-30` and
  `T24:00:00`. The reference also parses time-only values with the current
  date. Rust keeps the authored text for all of these, with a warning.
- A native TOML date-time with more than seven fraction digits fails the whole
  reference TOML load (Tomlyn). Rust loads it.
- The reference caches `TimeZoneInfo.Local` for the life of the process. Rust
  takes a snapshot for each save.

## Proof

`lastmodified-proof/probe.py` drives the production `modsync convert`. The
inputs are TOML and YAML guides with 48 `lastModified` forms (116 recorded reference cases), including:

- ISO with `Z`, with an offset and with no zone;
- fractions;
- date-only, space-separated, month-name and RFC 1123 forms;
- invalid, empty, digit-free and integer values;
- native TOML date-times, offset and range edges.

Each case is converted to TOML and YAML under `UTC`, `America/New_York` and
`Asia/Kolkata`. `csharp-oracle.json` records the reference results. It was
captured by `Program.cs`/`probe.csproj` against the reference `ModSync.Core`
build on the .NET 10 runtime (`DOTNET_ROLL_FORWARD=Major`). `--csharp <probe.dll>`
reruns the reference live.

Results on 2026-10-03:

- Against the recorded oracle: 217 match, 18 documented authored retentions,
  0 failures.
- Against the live reference: 217 match, 18 documented authored retentions,
  0 failures.
