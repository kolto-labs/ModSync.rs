# Replacing the drafter with TSLPatcher-syntax installs

Status: proposal, for discussion. Nothing here is implemented yet.

## Why

The drafter guesses install steps from English prose. It will always be
somewhat wrong, so every draft needs a review afterwards (see
`docs/draft-review.md`). It also cannot answer the question players care
about most: do these mods work together? File-write histories in
`src/conflicts.rs` show that two mods write the same file, but not whether the
second one destroys the first one's changes.

TSLPatcher's configuration already says, exactly, what a mod does to the game:
which files it copies in whole (`[InstallList]`), and which rows, fields and
strings it changes inside shared files (`[2DAList]`, `[GFFList]`, `[TLKList]`,
`[SSFList]`, `[CompileList]`, `[HACKList]`). If every mod in a build is
described in that one syntax, installs need no guessing and compatibility can
be decided exactly.

## What exists already

- **OdyPatcher** (`odypatcher` crate, pinned in `Cargo.toml`) parses
  `changes.ini` into `PatcherConfig` and `namespaces.ini` into
  `PatcherNamespace` values, and `summary::build` reports what a configuration
  would change without applying it.
- **ModSync** runs OdyPatcher natively, predicts patch results during
  validation (`src/engine/patcher_simulation.rs`,
  `src/engine/patcher_prediction.rs`), offers namespace variants for provided
  installers (`src/engine/provided_installer.rs`), and binds a single-run
  TSLPatcher archive exactly (`src/archive_generation/resolved.rs`).
- **The drafter's open cases** are exactly the ones TSLPatcher syntax answers:
  "Multi-run patcher order and namespace selections remain unresolved", and an
  archive with `namespaces.ini` stays unbound.

## Proposed stages

1. **Exact patcher mods.** Bind archives with `namespaces.ini` and multi-run
   patchers from the archive: the namespaces become the component's options,
   with names and descriptions from `namespaces.ini`. Guide prose only decides
   which option starts selected, and that one choice is what a person reviews.
   This resolves a whole class of drafts that stay unresolved today, without
   reading any more prose.
2. **Loose-file mods as an install list.** Describe a loose-file mod's steps as
   a TSLPatcher `[InstallList]` (authored in OdyEdit, or generated from the
   archive and reviewed) instead of Move and Copy steps drafted from prose. Then
   every mod in a build is described in one syntax that OdyPatcher executes.
3. **Compatibility from the configuration.** With every mod declared as either
   replacing a file or patching parts of it, conflicts become exact:
   - a mod that replaces a file after another mod patched it undoes that patch,
     which is a real incompatibility;
   - patching after a replacement is fine;
   - patches to different rows or fields of the same file are compatible;
   - patches to the same row or field depend on order, and the guide's order
     decides.
   This would feed the Conflicts report alongside the write histories it has
   today.
4. **The drafter becomes an authoring aid only.** It can still prefill a draft
   for a guide author to correct in OdyEdit, but players and CI only ever use
   reviewed configurations.

## Questions for the maintainer

- How to reach OdyEdit: `kolto-labs/OdyEdit`, linked from #98, returns not
  found for this contributor's account, and stage 2 depends on it.
- Whether reviewed configurations belong in `kolto-labs/mod-builds`, pinned per
  guide version.
- Whether the instruction format should reference a `changes.ini` shipped next
  to the guide, or carry the install list inline.
- Which stage to start with. Stage 1 is self-contained and helps today's
  builds; the others depend on the answers above.
