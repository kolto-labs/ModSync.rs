# Locale-aware settings numeric values

The production settings CLI now parses values before acquiring its settings lock,
using one immutable `NumberCultureSnapshot`. Existing `parse_cli_value`,
`set_cli_value`, and their frozen helpers remain unchanged. New additive public
snapshot methods and current-culture wrappers are available to other callers.
Missing/whitespace values still remove the exact-case key without locale lookup;
other fields and insertion ordering survive the locked merge.

## Data sources

Linux loads a same-major ICU uc/i18n pair through the platform loader, limited to
ABI majors 50–99. macOS loads `/usr/lib/libicucore.A.dylib`; its invariant/POSIX
default invokes the CoreFoundation current-locale fallback. No `setlocale`,
`uloc_setDefault`, environment mutation, locale subprocess, or locale table is
used. ICU identifiers and UTF-16 symbols/patterns have fixed read bounds. Formatter
handles close before library ownership ends. An unavailable provider or malformed
symbol is an error, never an implicit en-US substitution.

Unix mirrors the .NET ICU locale-symbol strategy: decimal/group separators,
positive/negative signs, infinity and NaN come from `unum_getSymbol` using a
`UNUM_DECIMAL` formatter. Negative infinity combines negative sign and positive
infinity. Negative-pattern space handling comes from normalized `unum_toPattern`.
Default ICU numbering-system keywords are not propagated to CultureInfo; Linux
`en_US_POSIX` maps to invariant. `DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1|true`
explicitly selects invariant symbols. This is an explicit configuration choice.

Windows uses `GetUserDefaultLocaleName` and `GetLocaleInfoEx`, retaining native
user overrides and the native number-negative pattern. Its snapshot identifies
`windows-nls-user-overrides`. It supports ordinary Windows settings setters;
it does not claim identical NaN/infinity data to a .NET process configured with
ICU plus NLS overrides. Custom .NET CultureInfo objects, app-local ICU versions,
and future Linux ICU ABI versions outside the bounded loader are separate
compatibility boundaries. No blanket Windows/macOS rejection is introduced.

## Grammar and bounds

The reader implements integer-first then floating-point syntax with native signs
and separators. Digits and exponent markers remain ASCII. Grouping is permitted
only after a digit and before the decimal separator; group sizes are deliberately
not enforced. Decimal separator has priority when symbols overlap. During floating
parsing, space may stand for a native NBSP/narrow-NBSP symbol; integer signs use
exact prefix matching and the integer reader's hyphen fallback precedence.
Floating negative zero, including negative underflow, retains its sign; integer
`-0` remains integer zero. The source-defined minus/hyphen
compatibility set, exponent signs, trailing numeric NULs, and negative-pattern
spacing are handled. Recognized nonfinite values and overflow fail rather than
writing invalid JSON. Input is capped at 16 MiB; symbol buffers at128 UTF-16 units,
pattern at512 and locale identifiers at157 bytes. Ordinary strings and JSON
objects/arrays retain the established parsing order.

Primary source contracts:

- Pinned `Services/Settings/SettingsFileStore.cs:138` calls current-culture integer
  then double parsing.
- [.NET numeric grammar](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/Common/src/System/Number.Parsing.Common.cs).
- [.NET ICU symbols](https://github.com/dotnet/runtime/blob/v9.0.0/src/native/libs/System.Globalization.Native/pal_localeStringData.c).
- [.NET negative pattern normalization](https://github.com/dotnet/runtime/blob/v9.0.0/src/native/libs/System.Globalization.Native/pal_localeNumberData.c).
- [.NET default locale](https://github.com/dotnet/runtime/blob/v9.0.0/src/native/libs/System.Globalization.Native/pal_locale.c).

No tests, custom harnesses, native locale probes, builds, or runtime execution were
performed for implementation. Source review and production compilation do not
establish native locale acceptance.
