# Guide input and autodetection contract

This source slice bounds public string ingestion before BOM removal or parser
selection, aligns terminal autodetection errors, and qualifies conversion stdin
as a UTF-8 console pipeline. It does not change successful format selection,
authored selection handling or duplicate-option policy.

## Admission and decoding

All Guide string parsing entry points share an 8 MiB UTF-8 byte limit, checked
before removing any leading U+FEFF characters. Ordinary file ingestion already
reads at most that limit plus one byte and rejects oversized raw input. Its
reference policy uses nonthrowing UTF-8 decoding and replaces every U+FFFD with
an underscore, including authored replacement characters.

Conversion stdin now uses bounded byte reading and nonthrowing UTF-8 decoding,
retaining U+FFFD. This follows the qualified UTF-8 Console.In route used by
`ModBuildConverter.cs:2020` in pinned C# `d31ccbd3`. Native Windows console code
pages remain outside this input profile. Both raw stdin and the resulting UTF-8
string must fit the bound: replacement expansion may therefore cause a second,
conservative admission failure. No input is truncated to make it fit.

## Autodetection

The parser tries TOML, Markdown and YAML in that order, returning the first
successful guide. If all fail, trimmed text beginning with `<` uses XML as the
terminal parser; other input uses JSON. The terminal failure retains its causal
error chain with format context. The reference's redundant second pure TOML
attempt is omitted. These decisions follow the pinned
`ModComponentSerializationService.cs:821-857`.

Explicit format errors remain explicit. No fallback can turn an oversized
string into a valid guide. BOM removal, successful selection defaults and
load-time duplicate-option handling are unchanged.

## Remaining boundaries

YAML remains fail closed if a document is invalid, whereas the reference can
skip an invalid document and retain later valid documents. Silently discarding
installation components is excluded from this Rust admission policy. Mixed
JSON scalar/component arrays, broader format extensions, guide GUID repair and
other scalar/property conversion contracts remain separate source work.

Source review and production compilation are separate from parser corpus and
CLI behavioral acceptance. No tests, fixtures, harnesses, CLI execution or
filesystem probes are part of this slice.
