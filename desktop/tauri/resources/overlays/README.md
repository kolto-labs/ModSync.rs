# Instruction overlays for the community builds

The community builds (kolto-labs/mod-builds) describe how to install each mod in
words. ModSync drafts steps from those words but, on purpose, never runs a
draft nobody has confirmed, so most mods of a build would not install. The
files here give those mods real ModSync instructions.

- `k1_full.toml`: steps for 92 mods of the KOTOR 1 Full Build.
- `k1_full.report.json`: every mod the generator looked at, whether it got
  steps, and why not when it did not.

The desktop app merges an overlay into its build when the build loads
(`src-tauri/src/overlays.rs`, with the engine's `merge::preview`), before the
build's prose is drafted. A mod that gets steps here is then installed from
these authored steps; the build's own name, links, description and order are
kept. Mods the build already gives steps to are left alone.

## Where the steps come from

They were not written by hand. `tools/overlay_from_old_installer.py` runs the
old kotor-mod-manager installer (the Python app this interface comes from,
which installed 172 of 173 KOTOR 1 Full Build mods in its own full-build test)
over the whole build, against a throwaway copy of the game, with the archives
ModSync downloaded. Its per-mod rules are used unchanged: archive layout
detection, the build-guide directives it read from the guide, and its
hand-checked rules per mod (`installer/build_overrides.py`). The script records
what that installer did and writes it as ModSync steps:

- each archive it unpacked: an Extract step;
- each file it copied into the game: a Copy step from the same file in the
  unpacked archive to the same game folder (a whole folder becomes one
  `folder\*` source only when every file in it was copied);
- a file it installed under another name: copied to a `renamed-N` folder in the
  mod's own folder, renamed there, then copied into the game;
- each file it deleted or renamed in the game, or in the mod before its patcher
  ran: a Delete (that does not fail when the file is already gone) or Rename;
- each patcher it would have run: a Patcher step on the same tslpatchdata folder
  with the same namespace (option) number;
- its end-of-install clean-up of duplicate textures (a `.tpc` with a `.tga` or
  `.dds` twin in Override is deleted): one DelDuplicate step on the Remove
  Duplicate TGA/TPC entry.

Patchers are never started while generating; their folder and option are
recorded instead. A mod gets no steps here when the old installer did not
finish it, left it to the player (for example the game program file patches),
would start a program that came with the mod, or did something these steps
cannot express exactly. `NPC Clothing M` is left out for that reason: the old
installer also wrote its `.txi` file over the renamed `.tga`.

The file names in the steps are the ones downloaded when the overlay was made.
A mod whose download is renamed in a new release no longer matches, and the
app's check reports its archive as missing; regenerate the overlay then.

## Regenerating

You need the old app's source (with its Python dependencies), the build's
Markdown converted with `modsync convert`, a ModSync mods folder with the
build's downloads and the JSON reports `fetch-downloads` printed for them,
and a copy of the game that may be modded. See the script's own help for the
arguments, and pass `--only` with the mods that still have no runnable steps
so mods the engine can already confirm keep the engine's own steps.

This content may later belong in kolto-labs/mod-builds instead of the app.
