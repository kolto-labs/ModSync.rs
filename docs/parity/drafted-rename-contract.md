# Drafted Rename as a file name

This contract covers a `Rename` that the natural-language drafter writes from guide
prose such as "Rename the file old.tga to new.tga." The drafter gave every drafted
Rename a `<<kotorDirectory>>\Override` Destination and, for "rename a.ext to b.ext",
the new name as a second Source, so no reviewed drafted Rename could install: Rust
refuses a placeholder Rename Destination since
[rename-destination-contract.md](rename-destination-contract.md) (and refused this one
before that too, as a folder target). The drafter now keeps the new name as a bare
file name, which is what C# `RenameFile` reads. This deliberately departs from the
pinned C# drafter (`d31ccbd3`, C# paths under `src/ModSync.Core/` there); the newer
C# parser on ModOrchestration's default branch (`66e468a`) keeps the bare name too.
Rust lines before this slice are at `b5df010`. Evidence:
[drafted-rename-proof](drafted-rename-proof/README.md). F03 and F05-RENAME stay partial.

## Why this gap

The rename-destination slice left it as a follow-up ("Drafted Renames"). Drafting runs
whenever a Markdown guide is loaded for validate or install (`src/model.rs:154`), from
`convert --parse-directions` (`src/main.rs:2317-2319`) and from the desktop's "Draft
instructions from prose" button (`src/ui.rs:2169`); all three call `draft::generate`.
A drafted Rename that cannot install is only noise for the reviewer, who must rewrite
it by hand. Matching the pinned drafter here is not useful: its Rename output fails
the C# install too.

## Triage

**C# at `d31ccbd3`.** The two Rename patterns
(`Parsing/NaturalLanguageInstructionParser.cs:266-277`) capture a `source` and a
`destination` of `[\w\-_.]+`. `ExtractSources` (`:1020-1036`) first tries the
`file_range` entity (`:398`, `a.ext (through|to|-) b.ext`) on the whole sentence, so
"Rename the file old.tga to new.tga" gives the Sources `<<modDirectory>>\old.tga` and
`<<modDirectory>>\new.tga`. The Destination goes through `NormalizeDestination`
(`:913-918`, `:1274-1306`), which returns `<<kotorDirectory>>\Override` for any name
without a separator. `DraftInstructionService.TrySanitizeInstruction`
(`Parsing/DraftInstructionService.cs:177-194`) drops any instruction whose Destination
is not under a root placeholder. So every drafted Rename names the Override folder,
which `RenameFile` (`Instruction.cs:935-1016`) joins to the source's folder as a file
name: the C# install fails it, or on Linux with `\` renames the source to a file
literally named `<<kotorDirectory>>\Override`.

**Newer C# (ModOrchestration default branch `baio/laughing-colden-6090a5`, `66e468a`).**
The parser keeps a bare new name: a Rename Destination without `<<` is not replaced
(`NaturalLanguageInstructionParser.cs:1398-1403`), and a Rename or Copy Destination
with an extension and no separator is kept as the file name (`:1415-1424`); a Copy
with such a Destination becomes a Rename (`:1444-1453`). The `file_range` Sources are
unchanged (`:508`, `:1720-1737`). Its `TrySanitizeInstruction` then parks the bare
name under `<<modDirectory>>\` (`Parsing/DraftInstructionService.cs:480-490`), which
its unchanged `RenameFile` (`Instruction.cs:1420-1511`) again joins as a name, so the
newer C# end to end still drafts a Rename its install fails.

**Rust at `b5df010`.** A port of the pinned drafter: the same Rename patterns
(`src/draft.rs:1170-1183`), `sources` tries `file_range` and the numeric range on the
whole unit (`:623-631`), `from_match` passes every captured Destination to
`destination()` (`:576-578`, `:769-799`), which returns `<<kotorDirectory>>\Override`
for a bare name, and `sanitize` (`:828-859`) keeps only root-placeholder Destinations.
The engine refuses the result (`rename_file_name`, `src/engine.rs:218-235`), so a
reviewed drafted Rename failed plain `validate`, every dry-run report and `install`.

**User-visible difference** (fixture `mods/old.tga`; the draft reviewed with
`convert --review-draft`):

```
Prose                                   pinned C# draft                 Rust before   Rust after
Rename the file old.tga to new.tga.     [old.tga, new.tga] -> Override  refused       mods/new.tga
Rename the file old.tga as new.tga.     [old.tga] -> Override           refused       mods/new.tga
Rename the file old.tga to newname.     [old.tga] -> Override           refused       refused
```

## Behaviour after this slice

- **The new name.** `from_match` (`src/draft.rs:576-586`) keeps a Rename's captured
  Destination, without quotes, as the Destination when it is one file name with an
  extension and no `/`, `\`, `:`, `<` or `>` (`is_file_name`, `:725-728`). Any other
  captured Destination goes through `destination()` as before.
- **One Source.** `sources` (`:631-643`) no longer reads a file range or numeric range
  from the sentence for a Rename: "old.tga to new.tga" is the source and its new name.
  The Rename patterns capture a single source token, so a Rename keeps exactly the
  source the prose names.
- **Sanitising.** `sanitize` (`:844-876`) accepts such a file name as a Rename
  Destination; every other Destination must still start with a root placeholder, and
  Sources are unchanged.
- **Still a draft.** The component still gets the review warning and `DraftReview`
  with `RequiresReview = true`; the engine's review gate refuses it until it is
  reviewed (`convert --review-draft`, the desktop review button), and Markdown loaded
  for validate or install drafts and stops at that gate as before. After review the
  engine checks the step like any authored Rename (`rename_file_name`), so the source
  must exist and the name is used in the source's own folder.
- **Fail closed, unchanged.** A new name without an extension (`to newname`, the
  corpus's `rename them to PLC_CompPnl_b`) or with a folder part (`to sub/new.tga`,
  where the pattern captures only `sub`) keeps the `<<kotorDirectory>>\Override`
  Destination, which the engine refuses after review. A placeholder in the prose
  (`to <<kotorDirectory>>/Override/new.tga`) matches no Rename pattern, so it stays
  an unparsed gap, which blocks the review. Prose that names no new name drafts no
  Rename (the gap blocks the review too).

Callers checked. `from_match`, `sources`, `destination()` and `sanitize` are private
to the drafter or called only from `draft::generate` (`src/draft.rs:323-400`); its
callers are Markdown ingestion (`src/model.rs:154`), `convert --parse-directions`
(`src/main.rs:2317-2319`) and the desktop button (`src/ui.rs:2169`). The review
records (`src/draft/review.rs`) hash the drafted instructions and never redraft. The
Copy-and-rename patterns are unchanged (see below), and drafts already saved in a
guide are not redrafted (`generate` skips a component with instructions or a
`DraftReview`).

## Proof

[probe.py](drafted-rename-proof/probe.py) runs 8 prose cases from JSON (4 drafted
Renames, 4 fail-closed controls) and the first Rename as a Markdown guide. Per case it
checks the drafted instructions, the pinned C# drafter's instructions for the same
prose ([csharp-drafts.json](drafted-rename-proof/csharp-drafts.json), recorded with
`tools/check_drafts.py`), the review gate on the unreviewed draft, the review, plain
`validate`, the full dry-run report and `install` (61 checks).

| Binary | Result |
| --- | --- |
| this slice | 57 match / 4 known difference / 0 fail |
| `b5df010` (pre-fix) | 37 match / 0 known difference / 24 fail |

On `b5df010` the five Rename cases (four JSON, one Markdown) drafted the Override
Destination (and two Sources for "to"), so after review plain `validate`, the report
and `install` refused them with `mods/old.tga` unchanged (20 fails); the four C# checks
fail because Rust still matched the pinned drafter. The controls and the review gate
gave the same results on both builds.

`tools/check_drafts.py` against the pinned C# drafter, over `tests/fixtures/drafts.json`
(19 components), the probe's cases and the K1/K2 full and spoiler-free guides from
`mod-builds` `21b2f04` (650 components): 0 differences on both builds; this slice adds
one intentional Rename difference in the fixture and four in the probe's cases. No
corpus guide drafts a Rename with a file name (the only corpus Rename is K2's "rename
them to PLC_CompPnl_b"), so the corpus drafts are unchanged.

## Known differences

- **Drafted Rename against the pinned drafter (intentional).** Rust drafts
  `Rename [<<modDirectory>>\old.tga] -> new.tga`; the pinned C# drafter drafts
  `[<<modDirectory>>\old.tga, <<modDirectory>>\new.tga] -> <<kotorDirectory>>\Override`
  ("to") or `[<<modDirectory>>\old.tga] -> <<kotorDirectory>>\Override` ("as"). Four
  checks in the probe. `tools/check_drafts.py` now lists exactly this shape (a Rename
  whose Rust Destination is a bare file name, whose C# Destination is a placeholder,
  and whose Rust Sources are C#'s without the new name) under
  `intentional_rename_differences` instead of failing; any other difference still
  fails.
- **Against the newer C# drafter.** Newer C# keeps the bare name in its parser but its
  sanitiser writes `<<modDirectory>>\new.tga`, and it keeps the `file_range` second
  Source; Rust keeps the bare name and one Source. Not run (no newer C# build here).
- **Copy-and-rename prose (unchanged).** "Make a copy of old.tga and rename it to
  new.tga" still drafts what the pinned drafter drafts (here a Move of
  `<<modDirectory>>\o\*` to Override, a regex quirk); newer C# turns a Copy with a file
  name Destination into a Rename. Rust has no Copy-as action that keeps the original,
  and a Rename would lose it, so this is left for a later slice.
- **Where the Rename runs.** The drafted Source is `<<modDirectory>>\<name>`, the root
  of the mod folder, as the pinned drafter writes it; a file inside an extracted
  archive folder is not found and the reviewed step fails as a missing source. The
  reviewer fixes the Source; nothing is moved silently.
- **Frozen Rust test.** `tests/drafts.rs`
  (`drafts_match_reference_service_and_preserve_authored_instructions`) compares the
  drafter with `tests/fixtures/csharp/drafts.json`, the pinned C# capture, and now
  differs on its `Rename` case. Rust tests are frozen and were neither changed nor run;
  the C# capture stays as recorded.
- **Messages and platform.** Unchanged; the proof ran on Linux.
