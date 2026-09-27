# F20 merge audit: C# oracle vs Rust `src/merge.rs`

Oracle: pinned C# `d31ccbd3` (`/home/brunner56/.codex/worktrees/5813/ModSync`).
Rust base: `ac9cf3a`, lane branch `lane-merge-f20`.
Paths below are relative to the C# checkout unless marked Rust.

## C# behavior

### Entry points

- CLI `merge` verb: `src/ModSync.Core/CLI/ModBuildConverter.cs:485-560` (flags) and
  `RunMergeAsync` at `:2384-2780`. `convert --merge` redirects there (`:1905-1943`).
- The CLI always uses `MergeHeuristicsOptions.CreateDefault()` (`:2541`). No CLI flag
  changes a heuristic.
- The CLI calls the async path `MergeInstructionSetsAsync` (`:2608`). C# tests
  (`FullBuild*Tests`) call the sync path `MergeInstructionSets`. The two paths differ
  on exclusions; see below.
- GUI: file loading opens `ComponentMergeConflictDialog`
  (`src/ModSync.GUI/Services/FileLoadingService.cs:182`, `:443`) with its own matcher
  `existing.Guid == incoming.Guid || FuzzyMatcher.FuzzyMatchComponents(...)`. The
  3060-line `ComponentMergeConflictViewModel.cs` does per-item linking, GUID choice and
  per-field choice. It does not call `ComponentMergeService`.

### Options (`MergeHeuristicsOptions.cs:12-27`, `ComponentMergeService.cs:15-87`)

Heuristic defaults: name exact on, author exact on, name similarity on, author
similarity off, domain fallback on, ignore case on, ignore punctuation on, trim on,
`MinNameSimilarity` 0.85, link validation on, skip blank updates on.

Field preferences default to "prefer incoming". `--prefer-existing-fields` sets all
ten. `--prefer-incoming-fields` is an `else if` (`ModBuildConverter.cs:2545-2552`), so
it clears flags that were never set. It has no observable effect.

### Target and source (`ComponentMergeService.cs:151-216`)

Default order: the result starts as the incoming list (target) and existing is merged
into it (source). `--use-existing-order` swaps them: existing is the target.

`UpdateModComponentFields(target, source)` copies a field from source when the
matching `PreferExisting*` flag is set (`:623-671`). In existing-order mode the source
is incoming, so `--prefer-existing-name` then takes the **incoming** name. Probe p10
confirms this inversion. Rust keeps the same target/source structure.

### Matching (`:346-392`, `:1182-1254`)

1. GUID dictionary over the whole source list. The last duplicate wins. It ignores
   the matched set.
2. Otherwise `FindHeuristicMatch` over unmatched source rows, in source order. It
   keeps the strictly highest score that is at least `MinNameSimilarity`.
   - Normalize (`:1137-1160`): trim, drop `char.IsPunctuation` (Unicode P* only, not
     symbols such as `+`), then lowercase. Whitespace is not collapsed.
   - Score: +1 exact name, +1 exact author, + Jaccard(name) split on single spaces,
     + 0.5 x Jaccard(author) when enabled. Emptiness checks use the target's values.
   - Domain fallback (`:1218-1226`): when the score is below 0.5, add 0.6 if both
     first `ResourceRegistry` URL hosts match. A domain alone (0.6) never passes 0.85.
     It needs Jaccard of at least 0.25 on the name.

### Unmatched source rows (`:394-401`, `:565-588`)

Each unmatched source row, in source order, goes before the next later source row
whose GUID is already in the result. It goes to the end when none is. The lookup
uses post-merge GUIDs. In existing-order mode a heuristic pair keeps different GUIDs,
so the lookup misses it (probe p10b).

### Exclusions (`:174-216` async, `:239-267` sync)

| Path | Default order | Existing order |
|---|---|---|
| Async (CLI) | `exclude-existing-only` drops rows whose GUID is not in incoming. `exclude-incoming-only` is ignored (`:208-212` tests the wrong flag). | `exclude-existing-only` drops rows whose GUID is not in incoming. That includes heuristic-matched existing rows. `exclude-incoming-only` is ignored. |
| Sync (tests) | `exclude-incoming-only` drops rows whose GUID is not in existing. `exclude-existing-only` is ignored. | Same as async. |

No C# test asserts `ExcludeIncomingOnly` removal. `MergeDirectionsPriorityTest.cs:120`
only sets it to false.

### Field merge (`:595-946`)

- GUID (`:601-620`): an empty GUID takes the other one. When GUIDs differ, the
  default order copies the existing GUID into the incoming row. Existing order keeps
  the target GUID.
- Name, author, description, directions, tier, installation method: copied from
  source only when preferred and the source is not blank. Category also skips an
  all-blank list.
- Language (`:674-685`): case-insensitive union, target values first.
- Resource registry (`:688-753`): runs when the source has URLs and ModLinks are
  not preferred. With link validation on, it drops non-http(s) source URLs and
  non-http(s) target URLs. New URLs copy `Files` only. Shared URLs add missing
  filenames.
- Dependencies, restrictions, InstallAfter and InstallBefore: GUID set union.
- Instructions (`:800-853`): when not preferred, an empty target copies all. Otherwise
  it keys by `action|destination` (lowercase) over the target's **original** list.
  An unkeyed source instruction appends. A keyed one replaces the original once. A
  second same-key source instruction is dropped (probe p9b). Duplicate keys in the
  target throw `ArgumentException` from `ToDictionary` and fail the whole merge
  (probe p9).
- Options (`:856-945`): matched by trimmed lowercase name. Empty target GUID takes
  the source GUID. A non-blank source description overwrites. Restrictions and
  dependencies union. Instructions merge by key. Unmatched options append.
- Network URL validation (`:339-344`, `:483-560`, `:949-1135`) runs only when a
  `DownloadCacheService` exists, which the CLI creates only for `--download`.

## Rust state at `ac9cf3a`

`src/merge.rs` had 286 lines. `src/main.rs:109-139` wires `merge EXISTING INCOMING
OUTPUT` with thirteen flags.

| Area | Rust before this lane |
|---|---|
| Match order | First source row where GUID **or** heuristic matched. An earlier name match could steal a later GUID match. |
| Score | First hit at or above threshold, not the best score. |
| Heuristic options | Only threshold and punctuation. No domain fallback, author similarity, case or trim toggles. |
| Punctuation | `is_ascii_punctuation`: strips `+`, `$`, `<`, `>` and so on. Keeps Unicode dashes and quotes. |
| Unmatched rows | Appended at the end. |
| Exclusions | Either flag skipped **all** unmatched source rows, then filtered by GUID set. `exclude-incoming-only` in default order therefore also dropped existing-only rows, and `exclude-existing-only` in existing order also dropped incoming-only rows (p6d). |
| Installation method | No field and no flag. The generic metadata fill copied it when the target lacked it. |
| Language | Target kept; no union. |
| Link validation | None. |
| Instruction keys | Checked against the live target, so duplicate source keys collapsed differently. Duplicate target keys handled first-wins. |

## Ranked gaps

1. **Match precedence and best score.** Wrong pairs change GUIDs, fields and
   instructions. Fixed in this lane.
2. **Unmatched row placement.** An existing-only row that sat before a shared row
   landed at the end instead of in place (p1, p10). Fixed.
3. **Exclusion semantics.** `exclude-incoming-only` dropped existing-only rows. Fixed
   to the flag's stated intent; see divergences.
4. **Domain fallback and heuristic options.** Fixed. All `MergeHeuristicsOptions`
   fields exist in `merge::Heuristics`. The CLI still uses defaults, like C#.
5. **Normalize punctuation class.** Fixed with `\p{P}`.
6. **Installation method preference and Language union.** Fixed. The CLI now has
   `--prefer-existing-installation-method`.
7. **Static link validation.** Fixed for the http(s)+host filter.
8. **Instruction key rules.** Fixed to original-target keys and replace-once.
9. **Network URL validation during `--download`.** Open. Rust `merge` has no
   `--download`, `--select`, `--source-path`, `--spoiler-free` or
   `--auto-generate-local`.
10. **GUI merge conflict dialog.** Open. Out of scope for this lane; `src/ui.rs` was
    not touched.
11. **CLI surface.** Rust takes positional paths. C# takes `-e/-n/-o` and `-f`, and
    writes stdout without `-o`. Open.

## Intentional divergences

- **p6, p6c: `exclude-incoming-only`.** C# CLI ignores it (async path bug,
  `ComponentMergeService.cs:208-212` vs `:259-263`). Rust removes incoming-only rows,
  as the help text says and as the sync path does in default order.
- **Heuristic-matched rows under exclusion.** Rust decides by match state, not by
  GUID sets. C# existing order with `exclude-existing-only` drops heuristic-matched
  existing rows. Rust keeps them.
- **p9: duplicate instruction keys in the target.** C# fails the merge. Rust keeps
  the first key and continues.
- **Instruction GUIDs.** C# `CloneInstruction` drops instruction GUIDs. Rust keeps
  the source GUID.
- **Extra Rust merging.** Rust also merges option resource registries, nested
  options and unknown metadata keys when the target lacks them. C# does not.

## Probes

Fixtures are 25 JSON guide pairs: 21 from `/tmp/f20-probe/gen.py`, plus p9b, p10b,
p11b and p11c written separately. They are dependency-free, so the C# loader's dependency reorder does
not affect order. Order parity is claimed only for these fixtures.

- C#: the prebuilt `ModSync.Core.dll` from
  `/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0`
  (receipt `docs/parity/validation-reference/receipt.json`), run as
  `dotnet ModSync.Core.dll merge -e E -n I -f json -o cs.json --plaintext FLAGS` with
  `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `DOTNET_CLI_HOME` under
  `/tmp/f20-probe/home`. No C# was compiled.
- Rust before: `ac9cf3a` debug binary. Rust after: this lane's debug binary. Both run as
  `modsync merge E I out.json FLAGS`. The baseline has no
  `--prefer-existing-installation-method`, so p7b is recorded as missing.
- `/tmp/f20-probe/cmp.py` compares order, GUID, name, author, description, tier,
  installation method, language, instructions and registry URL keys. It ignores
  `Files`, because C# JSON output drops them (open item in `COMPATIBILITY.md`). It
  also normalizes path separators and ignores Extract destinations, which are
  serializer differences.

Result: 10 of 25 matched C# before, 22 of 25 after. The three remaining differences
are the intentional divergences above.

| Probe | Checks | Rust before | Rust after | Note |
|---|---|---|---|---|
| p10_existing_order | Existing order + prefer-existing-name inversion; insertion | DIFF | SAME |  |
| p10b_existing_order_heuristic | Existing order, heuristic pair, unmatched appended | SAME | SAME |  |
| p11_links | Non-http URLs dropped on matched merge | DIFF | SAME |  |
| p11b_links_unmatched | Unmatched row keeps non-http URL | SAME | SAME |  |
| p11c_links_source_empty | Empty source registry: no filter | SAME | SAME |  |
| p12_guid_keep | Heuristic pair keeps existing GUID | SAME | SAME |  |
| p12b_guid_keep_existing_order | Same, existing order | SAME | SAME |  |
| p13_prefer_all | `--prefer-existing-fields` | SAME | SAME |  |
| p14_blank | Blank existing value not copied | SAME | SAME |  |
| p1_order | Existing-only row placed before next shared row | DIFF | SAME |  |
| p2_guid_first | GUID match beats earlier name match | DIFF | SAME |  |
| p3_best_score | Best heuristic score, not first hit | DIFF | SAME |  |
| p4_domain | Name Jaccard 0.4 + same host 0.6 matches | DIFF | SAME |  |
| p4b_domain_other_host | Different host: no match | SAME | SAME |  |
| p5_symbol_plus | `+` is a symbol, not stripped | DIFF | SAME |  |
| p5b_unicode_dash | En dash stripped as punctuation | DIFF | SAME |  |
| p6_excl_incoming | `--exclude-incoming-only` | DIFF | DIFF | Divergence: C# CLI ignores flag |
| p6b_excl_existing | `--exclude-existing-only` | SAME | SAME |  |
| p6c_excl_incoming_existing_order | `--exclude-incoming-only --use-existing-order` | DIFF | DIFF | Divergence: C# CLI ignores flag |
| p6d_excl_existing_existing_order | `--exclude-existing-only --use-existing-order` | DIFF | SAME |  |
| p7_language_method | Language union; incoming method kept | DIFF | SAME |  |
| p7b_method_prefer | `--prefer-existing-installation-method` | DIFF | SAME | Before: flag missing |
| p8_instr | Instruction key replace and append | SAME | SAME |  |
| p9_dup_keys | Duplicate target instruction keys | DIFF | DIFF | Divergence: C# throws |
| p9b_dup_source_keys | Duplicate source keys: replace once | DIFF | SAME |  |

Extra checks:

- p15 (no `Guid` keys): both CLIs refuse the input. C# fails with "GetValue cannot
  return null for a required value". Rust fails with "Every component and option needs
  a Guid and Name" (Rust `src/model.rs:182`). Blank-GUID pairing is therefore not
  reachable from the CLI.
- TOML output for p1, p8 and p4 (`-f toml` for C#, `.toml` output for Rust): component
  order, GUIDs, names, instruction order and registry URL keys match. Rust writes
  `\` path separators where C# writes `/`, and C# writes empty `Files` maps. Both are
  serializer or loader differences, not merge rules. The Markdown pipeline part of the
  F20 acceptance row is not claimed here.
