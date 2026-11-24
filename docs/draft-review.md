# Reviewing drafts

The drafter turns a guide's written directions into instructions with fixed
patterns. Guides are written for people, so it will always misread some mods
(see [What drafts get wrong](../site/instruction-files.html#draft-mistakes)).
Every draft therefore needs a human review after it is made, and nothing
drafted installs until it has one. The drafter is a starting point for guide
authors, not a way to install prose guides unattended.

## Making a reviewed instruction set

A reviewed instruction set is a guide's instructions, drafted once, checked
and fixed by a person, and marked reviewed. Draft from one exact
`kolto-labs/mod-builds` version, for example tag `v12.3.15`, so the reviewed
file always matches the guide it came from:

```sh
git -C mod-builds checkout v12.3.15
modsync convert mod-builds/content/k1/full.md k1-full.draft.json --parse-directions
```

Review every drafted component against its archive, the guide and the mod's
readme, as described in
[How to review a draft](../site/instruction-files.html#draft-review). Fix
wrong steps in the file or the desktop editor, write steps by hand where the
drafter gave none, then mark each component reviewed:

```sh
modsync convert k1-full.draft.json k1-full.reviewed.json --review-draft <guid> --review-draft <guid> ...
modsync validate --input k1-full.reviewed.json --game-dir <test copy> --source-dir <mods>
```

Keep the component GUIDs the converter gave. JSON keeps every review receipt;
TOML cannot encode some of them (see `COMPATIBILITY.md`).
