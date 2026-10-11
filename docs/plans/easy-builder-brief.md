# Easy-mode instruction builder: restated brief (2026-10-09)

Restated from Boden's request, plus earlier ModSync design discussions
(private notes, not in this repo).

## Goal in one line

Let a guide author build a mod build in ModSync the same way they build it by hand
from the mod-builds `full.md`, and get a working instruction file out as a side effect.

## What Boden asked for

1. **Start over and do manual installs first.** Redo K1 and TSL from a clean vanilla
   copy. Install every mod by hand, following
   `KOTOR-Community-Portal/mod-builds` `full.md` for each game. These are
   `k1_manual` and `tsl_manual`, and they are the oracle.
2. **Add an easy mode to ModSync's instruction builder.** It sits beside the
   existing editor and lets a user build a mod build exactly as `full.md`
   describes it:
   - It has its own file browser that looks like Windows Explorer, KDE Dolphin or
     macOS Finder. The user can switch between the three looks.
   - It has a built-in patcher panel that runs the TSLPatcher/HoloPatcher step the way
     the guide says (OdyPatcher underneath).
   - Following the guide inside ModSync should feel the same as following it
     outside. The only extras are live feedback: tooltips, progress, and the
     instruction list growing as each choice is made.
   - It has undo/redo, and saves automatically. Nothing is lost on close.
   - The whole build can be made without touching anything outside ModSync.
     Writing to real disk is optional and comes second.
3. **Predict the future state.** By default the browser and patcher show a
   virtual filesystem: what the game folder *will* look like at this point in the
   instruction list, even if nothing is installed yet. A "real disk" mode
   applies each action to disk right away; it is not the default.
4. **Prove it end to end.**
   - Use easy mode in real-disk mode to author the K1 and TSL instructions.
   - Run those instructions automatically four ways:
     `k1_auto_odypatcher`, `tsl_auto_odypatcher`, `k1_auto_providedpatcher` and
     `tsl_auto_providedpatcher`. "Provided patcher" means the patcher shipped in
     each mod archive, which is usually TSLPatcher.exe under wine.
   - Diff every auto tree against its manual tree after each mod, not just at
     the end. Diff providedpatcher against odypatcher after each mod as well,
     to show OdyPatcher introduces no differences.
   - The target is zero discrepancies.
5. **Remove tedium as it shows up.** Add shortcuts, defaults and keyboard paths
   wherever a click or navigation step is repetitive.
6. **Hot reload ModSync** so GUI changes do not need a full restart, if possible.
7. Use OdyPatcher as the patcher dependency, not KPatcher.
8. All subagents run on Sonnet.

## Guide-author constraints (summary)

The detailed source notes are private and are not in this repo.

- Authors maintain several build variants (K1/K2 × full, spoiler-free, mobile).
  They will not hand-edit configs or code-like syntax. Extra upkeep is a dealbreaker.
- Help them confirm things. Do not do things for them: report differences and never
  auto-rewrite their text.
- Install lists change a lot during testing, so reordering, dropping and
  inserting mods must not cost rework.
- Drop-in idea: snapshot vanilla, let the author install by hand inside
  ModSync, and record instructions from what they did. Track what a patcher
  actually writes (changes.ini), not only its InstallList.
- Generated docs must match the guide markdown and stay short.
- Trust bar: a per-mod diff of a ModSync install against a manual install
  shows zero differences.
- Rollback to "after mod X, before mod Y" is valuable. Make it optional.
- Domain rules to model:
  - Texture priority is TPC > DDS > TGA > ERF.
  - DelDuplicate TGA/TPC is a breakpoint.
  - CleanList deletes depend on the chosen mods.
  - Compat patches can complete a mod later in the order.
  - TSL needs Aspyr and 3C-FD detection.
  - Variants differ in mods, versions and order.

## How this maps onto the code

- Existing: `Engine` dry run with an in-memory file map (`Content::{Unknown,Disk,Scratch}`),
  `Engine::predict` and `editor::easy::Session` (durable undo/redo,
  uncommitted), `EditOperation`, the egui editor panel, and markdown to draft
  instructions.
- Missing:
  - the easy-mode GUI: browser, patcher panel, live instruction feed, autosave.
  - a predicted-state browser backed by `Session::predict`.
  - a real-disk action mode.
  - CLI entry points for easy sessions and per-mod diffs.
  - a hot-reload dev loop.
  - OdyPatcher wiring as the default engine.

## Session goal (set by Boden, 2026-10-09)

Build ModSync's easy-mode builder in tandem with installing the K1 and TSL mod
builds inside it by hand from the guides. Then prove the automated installs built
from those easy-mode instructions match the manual installs. Every other criterion
given during the session must also be met.
