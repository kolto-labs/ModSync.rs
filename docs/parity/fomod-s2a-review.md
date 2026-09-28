# FOMOD S2a independent reviews

Source `0d6aad1` (on `5f81e82`). Read-only reviewers; no cargo, tests or fixtures.

## SPEC: CHANGES REQUIRED

Scope respected (no priority sorting, folder copy or renaming; no mapper/desktop
change); staleness-by-current-installer justified against C# `FomodArchiveChoices`
(name only) and `FomodChoicesApplier` (throws on missing step/group); every
claimed item has a negative and a positive control; receipt source hashes equal
the commit.

1. Medium: `CHOICES_RECOVERY_HINT` recommends one-run `validate/install
   --fomod-choices`, which uses `apply_to_remaining` with `merge=false` and drops
   other archives' choices for that run. Unproven route contradicting "other-archive
   configuration survives".
2. Low: README/PROGRESS still say "uncommitted".
3. Low: `MODSYNC_FOMOD_CHOICES` reconfiguration claimed but never exercised.
4. Info: `src/fomod_cli.rs` is S1-owned; S1 ownership was released when S1 QUALITY
   was accepted (`c02a060`), so the narrow edit is permitted.

## QUALITY: CHANGES REQUIRED

Verified sound: normalized comparison on one unchanged module; clone-then-commit
atomicity; exit mapping; validation stays pure; consistent gate path keys.

1. should-fix: explicit choices file load error discarded when saved choices are
   stale, so the user's recovery file is never mentioned.
2. should-fix: probe never asserts validate leaves mod/game trees unchanged.
3. should-fix: no negative control for (1).
4-10. nits: contradictory `outcome:error, usableChoices:true`; env var silently
   reconfigures; duplicate no-route warnings; double check of one archive; install
   `copy_instructions` lacks recovery hint; inconsistent parse-error wording; probe
   temp root not removed.

## Controller decisions

- SPEC 1: remove the one-run validate/install parenthetical from the hint and docs;
  the `merge=false` one-run replacement stays an open S2 item (not advertised).
- SPEC 3 / QUALITY 5: reconfiguration of valid saved choices is limited to the
  explicit `--fomod-choices` flag; the environment variable keeps its S1 role of
  supplying pending consumers only.

## Re-review of `93837e5`: SPEC CHANGES REQUIRED, QUALITY APPROVED

All round-1 findings resolved per the controller decisions; `(path, guid)` checked
set cannot skip a needed check. New:

1. Medium (SPEC): unparseable saved `FomodChoices` metadata now shows the Fetch
   `--record --fomod-choices` hint in Fetch, validate and install, but that route
   re-parses the same metadata and always fails.
2. Low (QUALITY): a loadable recovery file lacking this archive's entry is not
   mentioned.
3. Nit: "uncommitted" wording; COMPATIBILITY overclaims the import route.

Controller decision for 1: unparseable metadata gets its own repair hint (repair or
remove that component's `FomodChoices`, then capture and record); flag
reconfiguration does not overwrite unparseable metadata, which could discard other
archives' entries.

## Re-review of `a436c05`: SPEC APPROVED, QUALITY CHANGES REQUIRED

SPEC: round-2 findings resolved; the repair hint no longer names a route that
always fails, and a missing flag entry is reported.

1. Should-fix (QUALITY): `src/fomod_cli.rs` `dispatch` skips consumers with
   unparseable saved choices during flag reconfiguration, but no probe exercised it;
   the claim that other consumers of the archive are still reconfigured was source
   reading only.
2. Low (QUALITY): `CHOICES_REPAIR_HINT` presented removal as equal to repair,
   although removing `FomodChoices` discards every archive's saved choices for that
   component.
3. Info: `PROGRESS.md` did not name the round-2 follow-up commit.

Fixed in the round-3 follow-up ([proof section (f)](fomod-s2a-proof/README.md#f-round-3-follow-up)):
probe case `unparseable_shared` (fails 3 checks with the skip removed) and a
`COMPATIBILITY.md` sentence; the hint now says to repair, with removal qualified;
`PROGRESS.md` names `a436c05` and the round-3 follow-up. Probe 224/224.

## Final QUALITY check of `ae8d7e9`: APPROVED

Both round-3 findings resolved; the `unparseable_shared` case exercises the
shared-archive skip non-vacuously (3 behavior checks fail without it). Two README
wording nits fixed by the controller. **FOMOD S2a is accepted (SPEC and QUALITY).**

Separate pre-existing defect found (not S2a): `plan` deduplicates by component only
(`src/download_orchestrator.rs` ~270/287) and `plan_url` adds one `ArchiveTarget`
per component for the same URL and filename (~397; `src/acquisition.rs` ~111), so
two components with one URL download concurrently to one path; the loser fails
`persist_noclobber` with `EEXIST` (`src/download.rs` ~277/417/473), is excluded
from FOMOD binding and `record`, and Fetch exits non-zero. Fix: merge targets by
destination at plan time and fan the outcome out to every owning component; prove
with two components on one URL and no pre-placed archive.
