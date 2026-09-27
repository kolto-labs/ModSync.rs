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
| Async (CLI) | `exclude-existing-only` drops rows whose GUID is not in incoming. `exclude-incoming-only` is ignored: the branch at `:208-212` handles only `ExcludeExistingOnly`, and no `ExcludeIncomingOnly` branch exists. | `exclude-existing-only` drops rows whose GUID is not in incoming. That includes heuristic-matched existing rows. `exclude-incoming-only` is ignored. |
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
   Duplicate keys and duplicate option names no longer merge silently: Rust keeps
   going and warns on stderr when content is lost or ambiguous (see divergences).
9. **Network URL validation during `--download`.** Open. Rust `merge` has no
   `--download`, `--select`, `--source-path`, `--spoiler-free` or
   `--auto-generate-local`.
10. **GUI merge conflict dialog.** Open. Out of scope for this lane; `src/ui.rs` was
    not touched.
11. **CLI surface.** Rust takes positional paths. C# takes `-e/-n/-o` and `-f`, and
    writes stdout without `-o`. Open.

## Intentional divergences

- **p6, p6c: `exclude-incoming-only`.** The C# CLI ignores it. The async path used by
  the CLI has no `ExcludeIncomingOnly` branch at all (`ComponentMergeService.cs:174-216`);
  the sync path has one in default order (`:259-263`). Rust removes incoming-only rows,
  as the help text says and as the sync path does.
- **Heuristic-matched rows under exclusion.** Rust decides by match state, not by
  GUID sets. C# existing order with `exclude-existing-only` drops heuristic-matched
  existing rows. Rust keeps them.
- **p9: duplicate instruction keys in the target.** C# `ToDictionary` throws (`:815`,
  and `:858` for option names, `:916` for option instructions) and the whole merge
  fails. That happens on shipped builds: merging `KOTOR1_Full.toml` with itself exits 1
  in C# with "Key: move|<<kotordirectory>>\override". The reviewer counted duplicate
  keys in 11 of 165 K1 components and 15 of 101 K2 components. Rust keeps going: it
  replaces the first instruction of a duplicate group and skips a source instruction
  whose key was already replaced. It prints a stderr warning naming the component,
  the option if any, and the key, but only when content is lost or ambiguous:
  - a skipped source instruction that is not already present in the group (p9b);
  - a replacement landing in a duplicate-key group that differs from the original
    (p9).
  The same rule covers option instructions and option names: a merge into the first
  of several same-named target options warns when it changes that option. A
  self-merge with identical content stays silent.
- **Instruction GUIDs.** The C# model has no instruction GUID (`Instruction.cs` has no
  `Guid` property). Rust keeps its optional instruction GUID from the source.
- **q1: new resource URL.** C# copies only `Files` into the new entry (`:719-734`).
  Rust copies the whole metadata object (`FileSize`, `HandlerMetadata` and so on).
  Not changed in this lane.
- **q2: duplicate source option names.** Fixed to keep the first. Target options
  match by name against the target's original options only, as the C# `optMap`
  does (`:858`). A later source option that repeats the name and instructions of one
  already appended is dropped, and a warning is printed if any other value differed.
  This matches the C# result, where the loader removes the repeat before the merge
  ("Duplicate option detected ... Removing duplicate", seen with `convert`). When the
  instructions differ, both options are appended, as C# merge does.
- **Extra Rust merging.** Rust also merges option resource registries, nested
  options and unknown metadata keys when the target lacks them. C# does not.

## Probes

The probe scripts, fixtures, recorded C# outputs and a receipt with binary hashes live
in `docs/parity/merge-probes/` (`receipt.json`). They are probe scripts, not Rust
tests: Rust tests stay frozen until full parity.

Fixtures are 28 JSON guide pairs: 21 from `gen.py`, plus p9b, p10b, p11b, p11c and p15
written separately, and the reviewer's q1 and q2. Order fixtures are dependency-free,
so the C# loader's dependency reorder does not affect order. Order parity is claimed
only for these fixtures.

- C#: the prebuilt `ModSync.Core.dll` from
  `/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0`
  (receipt `docs/parity/validation-reference/receipt.json`), run as
  `dotnet ModSync.Core.dll merge -e E -n I -f json -o cs.json --plaintext FLAGS` with
  `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `DOTNET_CLI_HOME` under
  `/tmp/f20-probe/home`. No C# was compiled.
- Rust before: `ac9cf3a` debug binary. Rust after: this lane's debug binary. Both run as
  `modsync merge E I out.json FLAGS`. The baseline has no
  `--prefer-existing-installation-method`, so p7b is recorded as missing.
- `cmp.py` compares order, GUID, name, author, description, tier, installation method,
  language, instructions, registry URL keys and option GUID, name and description. It
  ignores `Files` and other registry metadata, because C# JSON output drops them. It
  normalizes path separators and ignores Extract destinations, which are serializer
  differences. Missing or failed C# output always counts as DIFF, so "error vs error"
  is not a match.

Result: 11 of 28 matched C# before, 24 of 28 after. The four remaining differences are
p6 and p6c (intentional), p9 (C# throws) and p15 (both CLIs reject the input).

| Probe | Checks | Rust before | Rust after | Note |
|---|---|---|---|---|
| p1_order | Existing-only row placed before next shared row | DIFF | SAME |  |
| p2_guid_first | GUID match beats earlier name match | DIFF | SAME |  |
| p3_best_score | Best heuristic score, not first hit | DIFF | SAME |  |
| p4_domain | Name Jaccard 0.4 + same host 0.6 matches | DIFF | SAME |  |
| p4b_domain_other_host | Different host: no match | SAME | SAME |  |
| p5_symbol_plus | `+` is a symbol, not stripped | DIFF | SAME |  |
| p5b_unicode_dash | En dash stripped as punctuation | DIFF | SAME |  |
| p6_excl_incoming | `--exclude-incoming-only` | DIFF | DIFF | Divergence: C# CLI has no branch for the flag |
| p6b_excl_existing | `--exclude-existing-only` | SAME | SAME |  |
| p6c_excl_incoming_existing_order | `--exclude-incoming-only --use-existing-order` | DIFF | DIFF | Divergence: same |
| p6d_excl_existing_existing_order | `--exclude-existing-only --use-existing-order` | DIFF | SAME |  |
| p7_language_method | Language union; incoming method kept | DIFF | SAME |  |
| p7b_method_prefer | `--prefer-existing-installation-method` | DIFF | SAME | Before: flag missing |
| p8_instr | Instruction key replace and append | SAME | SAME |  |
| p9_dup_keys | Duplicate target instruction keys | DIFF | DIFF | Divergence: C# throws; Rust warns on stderr |
| p9b_dup_source_keys | Duplicate source keys: replace once | DIFF | SAME | Rust warns on stderr (skipped e2.tga) |
| p10_existing_order | Existing order + prefer-existing-name inversion; insertion | DIFF | SAME |  |
| p10b_existing_order_heuristic | Existing order, heuristic pair, unmatched appended | SAME | SAME |  |
| p11_links | Non-http URLs dropped on matched merge | DIFF | SAME |  |
| p11b_links_unmatched | Unmatched row keeps non-http URL | SAME | SAME |  |
| p11c_links_source_empty | Empty source registry: no filter | SAME | SAME |  |
| p12_guid_keep | Heuristic pair keeps existing GUID | SAME | SAME |  |
| p12b_guid_keep_existing_order | Same, existing order | SAME | SAME |  |
| p13_prefer_all | `--prefer-existing-fields` | SAME | SAME |  |
| p14_blank | Blank existing value not copied | SAME | SAME |  |
| p15_no_guids | No Guid keys | DIFF | DIFF | Both CLIs reject the input (error vs error = DIFF) |
| q1_rr_meta | New URL metadata (reviewer) | SAME | SAME | Keys only. Metadata still differs (open divergence q1) |
| q2_dup_opts | Duplicate source option names (reviewer) | DIFF | SAME | Rust keeps the first and warns |

### Warnings printed by the Rust CLI

| Probe | stderr |
|---|---|
| p9 | `component 'Alpha': instruction key 'copy\|<<kotordirectory>>\override' appears 2 times; replaced only the first ('Copy <<modDirectory>>\b.tga' with 'Copy <<modDirectory>>\a.tga')` |
| p9b | `component 'Alpha': instruction key 'copy\|<<kotordirectory>>\override' was already replaced; skipped 'Copy <<modDirectory>>\e2.tga'` |
| q2 | `component 'Alpha': duplicate option 'Opt' (00000012-...) dropped; kept the first (00000011-...)` |
| all others | none |

### Shipped-build self-merge (`run_self.sh`)

| Build | C# exit | C# error | Rust exit | Rust stderr | Rust output vs Rust `convert` |
|---|---|---|---|---|---|
| `KOTOR1_Full.toml` (189 components) | 1 | `Key: move\|<<kotordirectory>>\override` | 0 | none | identical |
| `KOTOR2_Full.toml` (145 components) | 1 | `Key: extract\|` | 0 | none | identical |

A self-merge changes no content, so the warning rule stays silent.

### Other checks

- p15 (no `Guid` keys): both CLIs refuse the input. C# fails with "GetValue cannot
  return null for a required value". Rust fails with "Every component and option needs
  a Guid and Name" (Rust `src/model.rs:182`). Blank-GUID pairing is therefore not
  reachable from the CLI.
- q1: registry keys match, but the metadata differs as listed under divergences. C#
  writes `FileSize` 0 and empty `HandlerMetadata` for the copied URL. Rust keeps
  `FileSize` 123, `HandlerMetadata` and `Files`.
- TOML output for p1, p8 and p4 (`run_toml.sh`): component order, GUIDs, names,
  instruction order and registry URL keys match. Rust writes `\` path separators where
  C# writes `/`, and C# writes empty `Files` maps. Both are serializer or loader
  differences, not merge rules. The Markdown pipeline part of the F20 acceptance row
  is not claimed here.
