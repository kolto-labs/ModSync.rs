# Scalar empty Source in XML and Markdown

This contract covers an instruction whose `Source` is a single empty or
whitespace-only string rather than a list: `<Source></Source>` or `<Source>  </Source>`
in XML, and `Source = ""` (TOML) or `Source: ''` (YAML) in a Markdown `<!--<<ModSync>>`
metadata block. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/`. Evidence: [scalar-empty-source-proof](scalar-empty-source-proof/README.md).
The list forms (`[]`, `[""]`, no key) are covered by
[empty-source-contract.md](empty-source-contract.md) and do not change.

## Triage at `3911e74`

**C# loading.** `Serializer.DeserializePathInDictionary` wraps a string Source in a
list through `PrefixPath` (`Utility/Serializer.cs:130-137`), which throws
`ArgumentException` for an empty or whitespace path (`:237-242`). The callers are
`Services/ModComponentSerializationService.cs:2144` and `:2353`. A list goes through
the enumerable branch instead, which skips blank entries (`Serializer.cs:149-178`).

- **XML.** `DeserializeModComponentFromXmlString` (`ModComponentSerializationService.cs:4527-4573`)
  turns the throw into `InvalidDataException("Failed to parse XML content.")`, so the
  guide does not load. Newtonsoft gives `""` for `<Source></Source>`; whitespace-only
  text is dropped by `XmlDocument`, with the same result. A self-closing `<Source />`
  is `null`, which no branch rewrites, and loads as an empty list (`:2185-2186`).
- **Markdown.** File loading goes through `GuideIngestService`
  (`Services/FileLoadingService.cs:60-69`). `MarkdownParser.ParseModSyncMetadata`
  catches the throw from the TOML or YAML parser and falls back to the legacy metadata
  parser (`Parsing/MarkdownParser.cs:844-902`, catches at `:868-871` and `:894-897`).
  That parser reads nothing from a TOML or YAML block, so the guide loads, but the
  component has no instructions and a generated Guid: the whole metadata block is
  silently dropped. The live oracle shows this for all four Markdown scalar cases.
- **Other formats.** Standalone TOML, JSON and YAML guides fail to load in C# too
  ([source-normalization-scalar-oracle](source-normalization-scalar-oracle/README.md)).

**Rust at `3911e74`.** TOML, JSON and YAML keep the scalar as `[""]`: the loader
filters blank entries only from arrays (`src/model.rs:814-816`), and path resolution
then fails with `Path must start with <<modDirectory>> or <<kotorDirectory>>`
(`src/paths.rs:173`) in the dry-run report, plain `validate` and `install`. Two
readers differed:

- XML turned `""` into an empty list (`src/xml.rs:144`, `array`), and whitespace into
  `["  "]`, which the model filter then dropped. `<Source/>` read as `""` too.
- The Markdown metadata pass kept `[""]`, but the component was then normalized a
  second time through `Guide::from_value` (`src/markdown.rs:398`), whose filter dropped
  the entry.

Both took the empty-list behaviour: a non-strict Delete installed as a no-op, and a
Copy, Move or Extract passed the dry-run report with `DryRunSkipped`.

**Destination and other fields.** Not the same bug. C# reads `Destination` as a plain
string with no `PrefixPath` (`ModComponentSerializationService.cs:2187`, `:2393`), so an
empty Destination loads in both implementations, and no Rust reader collapses it.
The other XML fields that `array` collapses (`Category`, `Language`, `Dependencies`,
`Restrictions`, `InstallBefore`, `InstallAfter`) are not paths.

## Behaviour after this slice

- **XML.** In an instruction, `<Source></Source>` and whitespace-only `<Source>` stay a
  scalar string (`src/xml.rs:238-267`), so the loader keeps `[""]` and the guide is
  refused as a TOML guide is: Error in `validate --output json --dry-run`, non-zero
  plain `validate`, `install` refused before any write. `<Source />` is now read as
  null (`src/xml.rs:103-110`, `:217-220`) and stays an empty list, as in C#. A stray
  component-level `<Source></Source>` stays an empty list, as before.
- **Markdown.** The metadata pass keeps the scalar as `[""]`, and the instruction
  serializer now writes a single blank Source back as a scalar (`source_out`,
  `src/model.rs:30-40`, `:86`), so the second pass keeps it too. The loaders have
  already removed blank list entries, so only a scalar can leave one. The guide is
  then refused as above.
- **Choose.** C# skips path deserialization for Choose
  (`ModComponentSerializationService.cs:2141-2145`, `:2350-2354`), so a scalar empty
  Choose Source is not prefixed; `GetValueOrDefault<List<string>>` gives null for `""`
  and the loader substitutes an empty list (`:2185-2186`). Both readers keep that: the
  XML scalar-keep skips an instruction whose Action is Choose (OrdinalIgnoreCase), and
  `drop_blank_choose_sources` (`src/markdown.rs:411-434`) empties a Markdown Choose
  scalar. Such a Choose is an empty no-op in both implementations.
- **Saving and `convert`.** Because the serializer writes a single blank Source as a
  scalar, every save path keeps it: TOML `Source = ""`, JSON `"Source": ""`, YAML and
  Markdown `Source: ''`, XML `<Source></Source>` (the Markdown export no longer drops a
  blank Source, `src/markdown/export.rs:198`). A refused guide stays refused after
  conversion to any format. Before, it read back as an empty list.
- **Unchanged.** `[]`, `[""]`, a missing key and `<Source/>` keep the empty-list
  behaviour. Non-blank sources and the TOML, JSON and YAML loaders are unchanged.

## Known differences

- **Refusal stage.** C# refuses the XML guide at load. Rust, as for TOML, JSON and
  YAML, loads it and refuses it in every verb that resolves paths, before any write.
  `convert` and `list` still accept it; C# `convert` fails on the input.
- **Unselected options.** Because Rust refuses only through validation, a scalar-blank
  Source inside an option that is not selected (for example a Delete) loads and
  validates, as validation covers only the selected instructions. C# refuses the whole
  XML guide at load whatever is selected.
- **Standalone TOML, JSON and YAML Choose.** `Source = ""` on a Choose keeps `[""]` in
  those loaders, as before this slice, where C# loads an empty Choose. That divergence
  predates this slice and is not changed here.
- **Markdown.** C# loads the guide and drops the metadata block, so the component
  installs with no instructions. Rust refuses the guide instead, as it refuses other
  metadata that the reference's typed parser cannot read (malformed documents fail
  closed). The proof records the reference result and checks the Rust refusal.
- **Message.** Rust reports `Path must start with <<modDirectory>> or
  <<kotorDirectory>>:` with the component and step; C# reports `Failed to parse XML
  content.`
