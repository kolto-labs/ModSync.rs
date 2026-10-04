# Choose option matching

This contract covers how a Choose instruction's Source entries pick options at
validation and install, and how legacy Markdown bullets split a Choose Source. The
reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/`. Evidence:
[choose-option-match-proof](choose-option-match-proof/README.md). Loading a Choose
Source (the raw-Action rule and the `;`/`,` split of a structured scalar) is covered
by [scalar-empty-source-contract.md](scalar-empty-source-contract.md) and does not
change.

## Triage at `a09ca4a`

These candidates come from the PR #22 review.

**1. Legacy Markdown Choose bullets: not a gap.** C# splits a legacy `**Source:**`
bullet on `,` only, trims each part and drops empty ones, for a top-level instruction
(`Parsing/MarkdownParser.cs:1096-1101`) and an option instruction (`:1267-1272`). The
`;`/`,` split belongs to `GetValue<List<string>>` (`Services/ModComponentSerializationService.cs:3078-3105`),
which the legacy parser never calls. Rust's legacy reader does the same comma split
(`src/markdown/legacy.rs:204`, `list` at `:237-243`). The live oracle confirms it: a
legacy `**Source:** <g1>;<g2>` loads in C# as the single entry `"<g1>;<g2>"`, and
`<g1>, <g2>` as two entries. Rust loads the same lists. No change.

**2. Choose option matching: a gap.**

- **C# install and dry run.** `ModComponent.ExecuteInstructionsAsync` runs a Choose
  through `Instruction.GetChosenOptions` (`ModComponent.cs:1648-1667`). It keeps the
  selected options whose `Guid.ToString()` is in the Source list, compared with
  `StringComparer.OrdinalIgnoreCase` (`Instruction.cs:1609-1611`). `Guid.ToString()`
  is the lowercase hyphenated D form, so only that spelling (in any case) names an
  option. A braced `{g}`, a parenthesized `(g)`, an N-form (32 hex digits) or a padded
  `" g "` entry names none. An entry that names no option is ignored, and the install
  still returns `Success`. The dry run (`Services/Validation/DryRunValidator.cs:498-512`)
  runs the same `ExecuteInstructionsAsync` on the virtual file system, so it gives the
  same result. The option's own GUID is typed, so `{g}`, upper case or N-form there is
  normalized when the guide loads, and the Source entry `g` matches it.
- **C# `Guid.TryParse` path.** `ComponentValidationService.GetMissingFilesForComponentAsync`
  (`Services/ComponentValidationService.cs:683-690`) parses entries with
  `Guid.TryParse`, which accepts braces. It is called only from tests at `d31ccbd3`,
  not from validation or install, so it does not decide what installs.
- **Rust at `a09ca4a`.** `chosen_options` (`src/engine.rs`, shared by the install,
  the validation gates and the dry-run report) compared entries with
  `guid::eq_ascii`, which parses both sides with the .NET `Guid.TryParse` grammar. A
  braced, parenthesized, N-form or padded entry therefore named the option and Rust
  installed it, where C# installs nothing. The value comparison came from
  [guid-selection-ordering.md](guid-selection-ordering.md), which cites the C# typed GUID lookups
  in `ComponentSelectionService` and `DependencyResolverService`, not
  `GetChosenOptions`. It was not a decision about Choose entries.
- **Unknown entries: deliberate.** Rust has refused a Choose entry that names no
  option (`Unknown option <id>`) since the initial import. This is in line with the
  rule "Reject unsafe/unsupported instructions" in AGENTS.md, and no document says
  otherwise. C# ignores the entry. The refusal is kept.

## Behaviour after this slice

`chosen_options` (`src/engine.rs:1757-1794`) now uses `names_option` (`:1796-1805`),
the C# text rule. An entry names an option when it equals the option's GUID in
lowercase D form (`Uuid` display of the parsed option ID), ignoring ASCII case. The
option's own ID is still parsed with the .NET grammar, so an option written `{g}`,
`G` or in N form is matched by the entry `g`, as in C#. An option ID that does not
parse (C# refuses such a guide at load) keeps the earlier ASCII-folded text
comparison.

Each entry must name an option, so an entry C# ignores is refused, as an unknown GUID
already was. When the entry parses as the GUID of an existing option, the message
gives the spelling to use:
`Unknown option {g}: a Choose entry names an option only as g`. All verbs share this:
the dry-run report gives an Error, and plain `validate` and `install` fail
`FOMOD configuration check failed: ...`, with `install` blocked before any write.

Unchanged: the canonical spelling in any case, unselected options (named but not
run), unknown GUIDs and non-GUID text (still refused), the loader, and the legacy
split.

## Known differences

- **Unrecognized entry.** C# ignores an entry that names no option, whether it is an
  unknown GUID, non-GUID text, a legacy `<g1>;<g2>` or a non-canonical spelling of a
  real option, and installs the rest. Rust refuses the guide in every verb before any
  write. So Rust never installs a tree that differs from the C# tree without telling
  the author. Before this slice, Rust installed the option for a non-canonical
  spelling, which C# skips.
- **Opaque option IDs.** An option whose `Guid` is not a GUID (for example `foo`) makes
  C# refuse the TOML guide at load (`No valid components found`). Rust loads it and
  matches the entry `foo`, as before. This is the opaque legacy-identity policy of
  [guid-selection-ordering.md](guid-selection-ordering.md), not part of this slice.
