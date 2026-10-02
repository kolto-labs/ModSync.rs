# Versioned ordinal name matching

The shared `fomod::fold_case` and `eq_ignore_case` entry points now use a retained
Unicode 15.0 simple-uppercase table. FOMOD archive, step, group, plugin and flag
names use it, along with existing selection, discovery and authoring consumers.
The public string API and its callers are unchanged.

## Reference and dataset

.NET 9's ICU ordinal BMP page builder calls `u_toupper` and explicitly preserves
U+0131 (dotless i) and U+017F (long s). Its supplementary comparison uses the
managed `CharUnicodeInfo.ToUpper` scalar mapping. The source is
[pal_common.c](https://github.com/dotnet/runtime/blob/v9.0.0/src/native/libs/System.Globalization.Native/pal_common.c)
and [OrdinalCasing.Icu.cs](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Globalization/OrdinalCasing.Icu.cs).

The production generator reads simple uppercase field 12 (zero based) from the
exact official UCD source already retained under `src/maintenance/casing`.
It checks the source SHA256, ordered scalar mappings and unchanged UTF16 width,
then excludes the two ordinal exceptions. The table has 1448 mapping pairs.
Its semantic SHA256 is
`e291cd8564cd0a84c4eb04b5dce57a3948b272f22613096dbbb042454502273c`.
The source/license hashes, canonical digest encoding and reference links are
retained in `src/fomod/ordinal/provenance.json`.

This replaces compiler-version-dependent full-uppercase conversion and the
handwritten Greek expansion corrections. Dotless i and long s no longer become
ASCII I and S. Sharp s does not expand to SS; simple Greek and supplementary
mappings come from the declared dataset. No normalization, language-specific
conversion or multi-scalar expansion is applied.

## Compatibility and evidence boundary

Scalars absent from this mapping table retain exact identity, including unassigned
or later-version characters. This name-key API cannot report unsupported input;
it does not claim equivalence to newer ICU data or every Windows NLS version.
Unlike filesystem maintenance, it neither mutates filenames nor persists a
recovery algorithm descriptor. Existing GUID consumers remain ASCII compatible.

Only the production data generator was executed. No tests, filesystem fixtures,
custom harnesses, application probes or .NET comparison runs were executed.
Independent source review and parent-owned production compilation are separate
gates; native/corpus acceptance remains open.
