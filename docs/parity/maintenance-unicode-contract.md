# Maintenance Unicode lowercase contract

This is a production source contract. Rust tests, custom harnesses, runtime probes and CI remain frozen. The data generator is a production source transformation; its execution is not native maintenance or .NET parity acceptance.

## Reference behavior

The pinned C# reference at `d31ccbd` calls `String.ToLowerInvariant()` for each descendant file and directory name in `FileOperationService.cs`. Its default Debug projects target .NET 9.

In .NET 9, `String.ToLowerInvariant()` calls invariant `TextInfo`. The ICU path selects `GlobalizationNative_ChangeCaseInvariant`, which decodes individual UTF-16 scalars, preserves U+0130, and otherwise calls ICU `u_tolower`. Each result is one scalar with unchanged UTF-16 width. ICU defines this as `Simple_Lowercase_Mapping` from `UnicodeData.txt`; it does not use expanding mappings, contextual final sigma, normalization or case folding.

Primary source references:

- [.NET 9 String.ToLowerInvariant](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/String.Manipulation.cs)
- [.NET 9 ICU dispatch](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/TextInfo.Icu.cs)
- [.NET 9 native invariant casing](https://github.com/dotnet/runtime/blob/v9.0.0/src/native/libs/System.Globalization.Native/pal_casing.c)
- [ICU simple lowercase API](https://unicode-org.github.io/icu-docs/apidoc/released/icu4c/uchar_8h.html#u_tolower)
- [UAX #44 mapping fields](https://www.unicode.org/reports/tr44/#Simple_Lowercase_Mapping)

The separate .NET globalization-invariant mode uses managed Unicode tables. Its .NET 9 generator declares Unicode 15.0 and preserves U+0130, U+0131 and U+017F for invariant compatibility. The latter two already have identity simple lowercase mappings. This provides an explicit version anchor for the selected dataset, rather than identifying the data version of every installed ICU library.

- [.NET 9 table version](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/Tools/GenUnicodeProp/GenUnicodeProp.csproj)
- [.NET 9 invariant table exceptions](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/Tools/GenUnicodeProp/CategoryCasingInfo.cs)

## Declared dataset

The supported profile is exactly:

| Field | Value |
| --- | --- |
| `algorithm` | `dotnet-icu-invariant-simple-lower-v1` |
| `unicodeVersion` | `15.0.0` |
| `sourceSha256` | `806e9aed65037197f1ec85e12be6e8cd870fc5608b4de0fffd990f689f376a73` |
| `tableSha256` | `cb52d6454792a898d04ae3102a8c227ea351f70f15321589f7d4cf9db6015400` |

The exact official [Unicode 15.0 source](https://www.unicode.org/Public/15.0.0/ucd/UnicodeData.txt) is retained at `src/maintenance/casing/UnicodeData-15.0.0.txt`. The official [Unicode license](https://www.unicode.org/license.txt) is retained at `src/maintenance/casing/LICENSE.txt`; its downloaded SHA-256 is `e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96`.

`src/maintenance/casing/generate.py` refuses any source with a different digest. It reads exactly 15 fields per UCD row and field 13, zero-based, for simple lowercase. It validates codepoint order, paired UCD range endpoints and scalar/UTF-16 width admission, removes the U+0130 mapping, and compresses adjacent assigned ranges. The generated `data.rs` contains 707 assigned ranges covering 286,719 scalar values and 1,432 nonidentity lowercase pairs. Private-use scalars are included as assigned; surrogate codepoints and unassigned scalars are excluded.

`provenance.json` records the URLs, hashes, counts and canonical semantic digest encoding. The table digest binds the algorithm/version, invariant U+0130 exception, complete assigned-range admission and all mapping pairs. Production admission independently checks the compiled tables and recomputes this digest once per process. The retained source is not loaded or downloaded during maintenance.

To regenerate the production data, run:

```text
python3 src/maintenance/casing/generate.py
```

The generator reads only its retained source and license, writes only its adjacent generated data/provenance files, and opens no game/mod tree. No build or startup downloads are involved.

## Mapping and persisted admission

`Capability.casing` is optional and independent of the native mutation `Capability.profile`. Its `CasingProfile` object contains exactly the four fields above and rejects unknown fields. Current supported native capabilities declare this dataset. Unknown algorithms, versions or either digest are refused.

For a declared Unicode profile, each descendant basename is processed scalar by scalar. Assigned values with a pair receive that single scalar; assigned values without a pair remain unchanged. U+0130 remains unchanged. A scalar unassigned in Unicode 15.0 is refused, even if a newer Rust compiler or ICU version knows it. This prevents an unassigned future uppercase character from silently receiving an identity mapping under a claimed old dataset.

The selected subtree's own name is excluded, as in the C# reference. Existing root/subtree/filename UTF-8 and portable path admission still applies. This slice adds no support for invalid UTF-8 Unix filenames or unpaired UTF-16 surrogate names. Name comparisons and collisions remain exact authored strings; the filesystem's native alias behavior can impose additional refusal. Exclusive native rename never merges or overwrites an existing object.

Preview collision preflight and operation creation use the same mapper as persisted journal operation derivation. Recovery dispatches from the recorded dataset metadata, then proves its derived operations against the immutable original inventory. It cannot reinterpret an old operation using the current default dataset.

Absence of `Capability.casing` retains the original ASCII-only lowercase contract. Its `None` value is omitted during serialization, preserving existing Linux intent bytes/digests. Native profile validation separately admits the frozen legacy Linux capability. Future dataset additions must retain exact old dispatch or explicitly refuse the old tuple; they must not replace the data behind an existing tuple.

## Backend and acceptance limits

This profile matches the documented .NET invariant simple lowercase algorithm on its declared Unicode 15.0 dataset. It does not claim that every .NET 9 deployment loads ICU data version 15.0. Native backend and Unicode data changes can alter mapping for newly assigned characters.

Windows .NET can select ICU or NLS. Its NLS path uses invariant `LCMapStringEx` without linguistic casing. A Unicode table profile therefore remains separate from Windows NLS/OS data policy; universal NLS equivalence is not asserted by this dataset slice.

- [.NET Windows backend selection](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/GlobalizationMode.Windows.cs)
- [.NET NLS casing call](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/TextInfo.Nls.cs)
- [Microsoft native casing flags](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-lcmapstringex)

Source checks consist of retained official input hashes, bounded production generation, formatting, whitespace checks, source inspection and compiled-table admission logic. No filesystem maintenance, .NET comparison run, native platform or interruption acceptance has been performed for this slice.
