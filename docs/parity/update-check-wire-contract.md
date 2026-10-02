# Update checking: wire defaults, identity and selection

Production source correction against pinned C# `d31ccbd3`:
`ModUpdateCheckService` lines 27-29, 88-118 and 166-193, and
`Services/Download/NexusApiClient` lines 65-82.

- Metadata-only `update-apply` preserves authored selection, matching the
  existing update-check and saved-summary intake paths.
- Installed/provider version equality uses the shared Unicode 15.0 ordinal
  case key. Output strings retain their authored spelling.
- Update resource keys retain exact Nexus host admission and require a nonempty
  domain plus an entire ASCII decimal ID segment in signed 64-bit range.
  Leading zeros are removed for shared request caching and offline version keys.
  The existing lowercase domain policy remains.
- Mod-info missing/null name and version become empty strings. Empty versions
  leave metadata and badges untouched. Numbers and booleans convert as the
  pinned `(string)JToken` cast does after `JObject.Parse`: integer tokens keep
  their digits (`-0` becomes `0`), other numbers become a `double` in .NET's
  invariant shortest round-trip text (`1.0` is `1`, `1e15` is `1E+15`,
  `0.00001` is `1E-05`, overflow is `Infinity`), and booleans become `True` or
  `False`. Objects and arrays are refused. Newtonsoft's `DateParseHandling`
  would also reformat an ISO-date-shaped version string; Rust keeps the string
  as sent.
- Missing/noninteger timestamp becomes zero; an integer outside signed 64-bit
  range is refused. Decimal/exponent notation is classified lexically so an
  overflowing exponent is still a noninteger default, including when arbitrary
  precision is enabled. Missing/nonboolean availability becomes true.
- Conflict limitations now describe supported simulated InstallList writes and
  excluded external/binary effects accurately. Collector and engine Unicode
  filesystem identity remain a coordinated future correction.

The pinned C# URL regex also admits a numeric prefix followed by nondigits and
unanchored host substrings. Rust retains whole-segment ID qualification and
exact host admission. The pinned provider casts Int64 IDs to Int32; Rust retains
its existing Int64 request spelling instead of reproducing narrowing wrap.
These are explicit intake differences, not a claim of universal URL parity.

This is source evidence. No runtime/provider request, fixture, test, CI,
installation or platform probe was executed for this correction.
