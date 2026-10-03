# Guide `lastModified` metadata contract

Decided 2026-10-03: follow the reference. The C# value is observable in saved
TOML and YAML guides, so Rust normalizes it on save the same way.

## Reference behavior

Pinned `ModSync.Core` (C# reference `ModOrchestration` at `d31ccbd3`; line
numbers are from that commit):

- `MainConfig.cs:408` stores `public static DateTime? LastModified`.
- TOML intake, `ModComponentSerializationService.cs:2012` and `:2050-2054`.
  The value is reset to null, then only the `lastModified` key of
  `[metadata]` is read. A `LastModified` key is ignored, so it is never
  stored or saved. Its `ToString()` goes through
  `DateTime.TryParse(text, CultureInfo.InvariantCulture, DateTimeStyles.None)`.
  For a native TOML date-time, `ToString()` is Tomlyn 0.17.0
  `TomlDateTime.ToString()`. That omits the fraction when its millisecond part
  is zero.
- YAML intake, `:608-615`. It reads `LastModified` when that key exists, or
  else `lastModified`, with the same `TryParse`. There is no fallback: when
  `LastModified` exists but fails to parse (or is null), `lastModified` is not
  consulted and nothing is stored.
- When the parse fails, nothing is stored and the field is not written on save.
- TOML save, `:3719-3722`. The code is `metadataTable["lastModified"] = DateTime`.
  Tomlyn writes this as a native local date-time with no offset. It truncates
  to milliseconds and writes `.fff` only when the milliseconds are nonzero.
- YAML save, `:4424-4427`. The output is `lastModified: "{0:O}"`: seven fraction
  digits, plus the host offset for a Local-kind value.
- JSON, XML and Markdown saves never write the field.

`DateTimeStyles.None` converts a `Z`, `GMT` or explicit offset to host Local
time. A zone-free value stays Unspecified, and no value is ever UTC-kind. The
culture is fixed to invariant, but the time zone is the host's.

**The saved bytes depend on the host.** For a zoned input, both
implementations convert to the host time zone, including its daylight-saving
rule on that date. The same guide saved on hosts in different zones, or in
one zone on either side of a DST change in the stored date, gives different
TOML and YAML bytes. Rust follows the reference here on purpose.

## Rust implementation

- `src/profiles/schema.rs` `invariant_try_parse` reuses the shared bounded
  invariant `DateTime.TryParse` model. That is
  `profiles/date/fallback.rs` (`date_compat::fallback::parse`) and the immutable
  local-zone provider (`ContextSnapshot::normalize_fallback`). No new parser was
  added.
  - `DateValue::without_roundtrip_kind` (`src/profiles/date.rs`) maps the UTC
    designator to a zero-offset Local adjustment, as `DateTimeStyles.None` does.
  - The canonical output is widened to the `O` pattern.
- `src/model.rs` `select_last_modified_key` runs in the TOML and YAML loaders
  and applies the reference key choice:
  - **TOML:** `LastModified` is removed with a warning, since the reference
    never reads or writes it. `lastModified` is untouched.
  - **YAML:** when `LastModified` exists, any `lastModified` is removed and
    `LastModified` is renamed to `lastModified` in place, keeping its
    position. A failing `LastModified` therefore never falls back.
- `src/model.rs` `normalize_last_modified` runs in `Guide::render` for TOML and
  YAML output only, on the `lastModified` key:
  - **Parsed:** YAML gets the `O` string. TOML gets a native local date-time
    truncated to milliseconds. The key keeps its position.
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
    failures. This is the same policy as the other date fields. A retained
    native TOML value stays native in TOML and is written as its date-time
    text in YAML.
  - Removals use `shift_remove`, so other metadata keys keep their order.
    Each warning is logged once for the same message and authored value in a
    row, so repeated desktop raw-preview renders do not repeat it.
- `src/json_native.rs`: an object that holds only `toml`'s private date-time
  key, with a valid TOML date-time string, serializes as a native date-time in
  TOML output and as that date-time's text in YAML output. Native TOML
  date-times that Rust read in any field round-trip as date-times. Before this
  change they leaked a `"$__toml_private_datetime"` table.
  - The key is the `toml` crate's reserved protocol, not a ModSync marker. It
    is kept on purpose. A TOML load already yields this exact object for a
    native date-time, and the `toml` crate cannot tell the two apart. No key
    that a `serde_json::Value` can hold is unforgeable, so a ModSync-only
    marker would be no safer. As a result, a JSON, YAML or XML object of
    exactly this shape is also written as a date-time.

## Known differences

- The TOML fraction is written by `toml_datetime` with trailing zeros removed
  (`.5`), where Tomlyn writes `.500`. The parsed TOML value is the same.
- YAML string quoting follows the shared YAML writer. The string value is the
  same.
- Rust passes through metadata keys the reference drops. `LastModified` is
  the exception, because YAML intake would read it.
- In YAML output, a retained value keeps its authored text, written by the
  shared YAML writer. For example, a native TOML local time `10:20:30` is
  written as `lastModified: 10:20:30`, where the reference writes today's
  date with that clock.
- Only TOML and YAML intake choose a key. A guide read from JSON, XML or
  Markdown that carries a `metadata.lastModified` value is still normalized on
  a TOML or YAML save. The reference does not read it from those formats.
- The reference rejects some forms that Rust keeps, such as `2024-02-30` and
  `T24:00:00`. The reference also parses time-only values with the current
  date. Rust keeps the authored text for all of these, with a warning.
- A native TOML date-time with more than seven fraction digits fails the whole
  reference TOML load (Tomlyn, "not following RFC3339"). Rust loads it and
  normalizes it. The same text as a YAML string parses in both and matches
  (rounded to seven digits).
- The reference caches `TimeZoneInfo.Local` for the life of the process. Rust
  takes a snapshot for each save.

## Proof

`lastmodified-proof/probe.py` drives the production `modsync convert`. Its
case table has 133 reference cases, written as TOML and YAML guides,
including:

- ISO with `Z`, with an offset and with no zone;
- fractions, including eight digits;
- date-only, space-separated, month-name and RFC 1123 forms;
- invalid, empty, digit-free and integer values;
- native TOML date-times, offset and range edges;
- `LastModified` alone and with `lastModified`, in TOML and YAML, including a
  failing or null `LastModified` that must not fall back.

Each case is converted to TOML and YAML under `UTC`, `America/New_York` or
`Asia/Kolkata`. Retained cases are compared with the authored value read back
from the output. Every output is also checked for a stray `LastModified` key.
Cases the reference cannot load are printed as `REFERENCE-ERROR` and counted
separately.

`csharp-oracle.json` records the reference results. It was captured by
`Program.cs`/`probe.csproj` against `ModSync.Core` built from `d31ccbd3`, on
the .NET 10 runtime (`DOTNET_ROLL_FORWARD=Major`), with
`probe.py --csharp <probe.dll> --record`. `--csharp` alone reruns the
reference live.

Results on 2026-10-03, against the live reference and again against the
recorded oracle: 249 match, 18 documented authored retentions, 2 reference
load errors (the eight-digit native TOML case), 0 failures. The 116 cases
recorded earlier gave the same reference results.

## Deferred

- In-repo regression tests for the key choice, the YAML date-time text, the
  key order and the warning dedupe. AGENTS.md freezes Rust tests until full
  parity, so none were added. The proof is the convert differential above.
