# Markdown and draft parity audit (F02/F03)

Reference: pinned C# `d31ccbd3` (`ModSync.Core.dll` sha256 `8f3b1e3f…5768f0`, the same build used by `markdown-check.json`). Corpus: `mod-builds` `336b36d9`, every `content/k1/*.md` and `content/k2/*.md` guide.

## Method

- The reference ran `dotnet ModSync.Core.dll convert --input <guide> --output <json> --format json --plaintext --non-interactive` inside bubblewrap with no network, as in `tools/check_formats.py`. No C# was compiled.
- The Rust CLI ran `convert <guide> <json>` for each guide.
- `tools/check_markdown_corpus.py` compares every key the reference emits for each component. The random component GUID is excluded. Dependency GUIDs are compared as component indices, and ResourceRegistry as ordered URL keys. That gives 33 keys per component, including Name, Heading, Author, Category, Tier, Language, all prose fields, resource links, Dependencies, DependencyNames, WidescreenOnly, IsSelected, Instructions and Options.

## Full-corpus result

| Guide | C# components | Rust components | Fully matching before | Fully matching after | Field values matching before | Field values matching after |
|---|---|---|---|---|---|---|
| k1/full | 186 | 186 | 173 | 186 | 6124/6138 | 6138/6138 |
| k1/spoiler-free | 180 | 180 | 166 | 180 | 5925/5940 | 5940/5940 |
| k2/full | 145 | 145 | 122 | 145 | 4762/4785 | 4785/4785 |
| k2/spoiler-free | 139 | 139 | 116 | 139 | 4564/4587 | 4587/4587 |
| k1/full_mobile | 112 | 112 | 107 | 112 | 3690/3696 | 3696/3696 |
| k1/spoiler-free_mobile | 107 | 107 | 102 | 107 | 3525/3531 | 3531/3531 |
| k2/full_mobile | 108 | 108 | 86 | 108 | 3542/3564 | 3564/3564 |
| k2/spoiler-free_mobile | 103 | 103 | 82 | 103 | 3378/3399 | 3399/3399 |
| Total | 1080 | 1080 | 954 | 1080 | 35510/35640 | 35640/35640 |

Before the change, these fields differed:
- Screenshots: 40 values.
- DependencyNames: 84 values. Rust did not emit them.
- Dependencies: 4 values, from the author-possessive master form.
- Heading: 2 values. The reference strips a trailing `[…]` from the heading.

The other ten guides have no `###` component sections. They cover android, ios, widescreen, delete and the redrob deletion lists. The reference returns `[]` with exit 0. Rust exits 1 with "Markdown has no component sections or structured guide blocks." This is kept on purpose: it stops an empty guide from reaching install. It is an open behaviour difference, not parity.

## What changed

- `src/markdown/extract.rs` (new) reproduces the reference's default `MarkdownImportProfile` field patterns. It does not rely on line prefixes. Each field is the leftmost match of that field's pattern over the whole section:
  - Matching ignores case, and the section keeps its newlines.
  - A missing colon is accepted for most prose fields, as in `Screenshots:?`.
  - The admonition fence form wins over the plain form.
  - The shared field boundary is honoured.
  - A blank first match counts as absent.
  - Definition-list prefixes are stripped, as in `StripDefinitionListPrefix`.
- The prose word "screenshots" now starts the Screenshots field, as in the reference. This closes the nine K1 Screenshots differences recorded in `markdown-check.json`.
- Headings follow the reference `^###\s+(?<heading>.+?)(?:\s*\[.*?\])?\s*$`. Name uses the case-insensitive NamePattern. `NameFieldContent` and resource links use the case-sensitive match, as the reference does. Links are collected with the reference `ModLinkPattern`, then trimmed and de-duplicated ignoring case.
- Category & Tier matches case-sensitively and splits on `&` and `,`.
- A section with neither a Name field nor a heading is skipped, as in the reference. It used to raise "Missing Markdown heading". WidescreenOnly positions now track only the sections kept.
- Masters resolution records `DependencyNames` and adds the reference fallback for author-possessive names. "JC's Cloaked Jedi Robes" resolves to the component named "Cloaked Jedi Robes" whose author contains "JC", with the JC and JCarter426 aliases.
- The old line-based `fields()` scanner remains only for the mixed prose and fenced-guide rejection check.

## Open gaps

### F02

1. The component-list JSON from the reference does not include preamble, epilogue, widescreen-warning or Aspyr warning content. This differential does not cover them. The fixture oracles `markdown-warning-oracle/` and `markdown-phase-oracle/` remain their evidence.
2. Component GUIDs differ by design. The reference uses random GUIDs; Rust uses deterministic UUIDv8. Tests or tools that compare GUIDs across implementations must map by index.
3. When a guide has no components, the reference returns an empty list and Rust returns an error (above).
4. Section splitting still differs in two ways. Rust skips `###`, `##` and `___` lines inside HTML comments and code fences; the reference `ComponentSectionPattern` does not. The reference `^###\s*` start also accepts `####` lines as section starts. Neither case occurs in the current corpus, since all 1080 components align. No reference fixture covers it yet.
5. Custom `MarkdownImportProfile` patterns (Raw mode), legacy bullet metadata and prose export are not implemented.
6. GUI paste import has no differential.
7. The pattern emulation is hand-written, not a regex engine. It is proven on this corpus and the existing fixtures. Unusual inputs may still expose differences in backtracking, for example a Name link that spans lines, or Unicode case folding beyond ASCII.

### F03

1. **ZIP renamed to `.exe`.** The reference emits only Extract; Rust emits Extract and Move. Reference trace in `~/.cache/modsync-rs-exe-inference-oracle/csharp.log`:
   - SharpCompress opens `.exe` with the 7z reader (`ArchiveHelper.OpenArchiveFromStream`). Reading entries throws `ArchiveOperationException`.
   - `AnalyzeArchive` falls back to the external `7z` CLI: "7z CLI listed 2 files in archive". The archive holds one file. The CLI parser (`TryListArchiveWithSevenZipCliAsync`) removes the leading `Path =` entry only when it equals the bare filename. The listing prints the full archive path, so that entry survives. That part is inferred from source.
   - The generator logs "Skipping Move instruction for root - no game files found". **Unverified:** why a flat `sample.tga` produced no Move has not been traced.
   - This behaviour depends on whether an external `7z` is installed and on how its listing is parsed. It is left unported on purpose. Emulating it would tie inference to an outside tool's output.
2. Direct-Markdown preflight policy (`src/direct_markdown.rs`). The reference `DirectMarkdownInstallPreflight` counts flagged drafts as executable. Rust adds any component that needs review to `unresolved_components`. This follows the draft service's never-auto-trust contract. The conflict is still to be settled by the owner.
   - The reference `drafted` count also includes option-level review flags. Rust counts component-level flags only.
3. Still open: URL-based generation, desktop inference controls, editing unresolved drafts, all parser edge cases and full-corpus installs. Drafting equivalence itself remains as recorded in `draft-check.json`. It was not re-run against the new parser output.

## Reproduce

```sh
python3 tools/check_markdown_corpus.py \
  --reference-dll <reference-build>/bin/ModSync.Core/debug_net9.0/ModSync.Core.dll \
  --rust target/debug/modsync --corpus <mod-builds> --output <scratch>
```

Under heavy load each full K1 guide takes 3–7 minutes in the reference.
