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
