# Mod selection CLI probes

These are the CLI acceptance probes for mod-selection plan Tasks 1 and 2. They were copied from `/tmp/selection-probes` on 2026-09-27.

- Branch `selection-actions`, HEAD `384cca5bdb4bb42bef2ca735f5b4e6e4d0053e8e`. Base is `c28f4fc`.
- `receipt.json` is the original receipt. Its sha256 is `a601124fb817bdb80aac902a9d4cd64b380c0a9514556b7c102c358fa260a014`.
- The receipt pins the binaries by sha256:
  - `modsync-new` `4ef993df…4c55` was built from HEAD.
  - `modsync-base` `0fbf235f…6f96` was built from `c28f4fc`.
  - The binaries (181 MB) are not copied.
- `SHA256SUMS` hashes every file in this directory.
  - Checked against `receipt.json` `outputs` and `fixtures`: 172 files match and 0 differ.
  - The 12 `out-corpus/*.stdout|stderr` files are not copied. Their hashes remain in `receipt.json`. The `*.oracle.json` files, `summary.txt` and `inputs.sha256` are copied.
- `fixtures/games/unknown` is an empty directory in the original, so git does not keep it. Recreate it before rerunning `p4c`.
- Scripts: `run_probes.sh` covers the synthetic probes (`out/`), and `run_legacy.sh` covers probe 8 (`out-legacy/`; `out-legacy-exec/` uses `synthetic-exec.toml`). `run_corpus.sh` runs the real corpus, and `corpus_oracle.py` checks it independently.
- `run_probes.sh` is copied from `/tmp` as found. Its sha256 is `d191fa3f…45a5`, which does **not** match the receipt's `fa065141…eb97`. The other three scripts match. The /tmp copy (mtime 14:37) predates the receipt (14:42), so the receipt most likely hashed an earlier round's version. Treat it as the best available script, not a pinned one.
- No Rust tests or harnesses were created, changed or run.

## What the probes show

| Probe | Output | Shows |
|---|---|---|
| p1a | `out/p1a-tier2.stdout` | `--select tier:"2 - Recommended"` selects tiers 1 and 2 cumulatively. |
| p1b, p2 | `out/p1b-closure.stdout`, `out/p2-deselect-dep.stdout` | `outcomes[0].autoSelected` names Alpha Core (`…0001`). It was already selected before the reset, so it is in neither `selected` nor `deselected`. |
| p2 | `out/p2-deselect-dep.*` | `--deselect Alpha Core` also deselects its dependent Beta Addon, which appears in `autoDeselected`. |
| p3 | `out/p3-nested-option.*` | `--option` on a nested option selects its parent chain. |
| p4a, p4b | `out/p4a-legacy-default.*`, `out/p4b-k1-default.*` | Default-all on a known non-Aspyr game drops the Aspyr-only mod and its dependent. |
| p4c | `out/p4c-unknown-default.stderr` | An unknown game only warns: `Game version unknown; left 1 Aspyr-exclusive mod(s) selected`. |
| p4d, p4e | `out/p4d-refuse-direct.*`, `out/p4e-refuse-closure.*` | An Aspyr-only mod is refused whether it is picked directly or pulled in by the dependency closure. |
| p5a, p5b | `out/p5a-add-category.*`, `out/p5b-reset-category.*` | `--add-category` adds. `--select category:` resets. |
| p6 | `out/p6-candidates.stdout` | `downloadCandidates` for base, widescreen and all. **CLI half only.** |
| p7a, p7b | `out/p7a-mutual.*`, `out/p7b-default-quiet.*` | A mutual restriction keeps both mods and gives one warning that names both GUIDs. Default-all prints nothing. **CLI half only.** |
| p9 | `out/p9-argv-order-a.stdout`, `out/p9-argv-order-b.stdout` | Flags apply in argv order, so these two runs differ. |
| p11, p12 | `out/p11-numeric-tier.*`, `out/p12-bare-optional.*` | Bare `2` and bare `Optional` follow the cumulative rule. |
| p14 | `out/p14-direct-md-conflict.stderr` | `--direct-markdown` with a selection flag is a Clap conflict and exits 2. |
| p17, p18, p19 | `out/p17-*`, `out/p18-*`, `out/p19-*` | At `384cca5`, removing an Aspyr mod also clears its options (`…0015`) and whatever depends on them (`…0016`). |
| corpus | `out-corpus/summary.txt` | On KOTOR1_Full and KOTOR2_Full, the CLI selected set equals the independent oracle in all 6 runs. |
| legacy | `out-legacy/compare.txt`, `out-legacy-exec/compare.txt` | 9 of 12 legacy runs match byte for byte. The three differences (l7, l8 and l12) are described in `COMPATIBILITY.md`. |

The GUI halves of probes 6 and 7 are unverified. They need mod-selection Task 3, which wires `selection::apply` into `src/ui.rs`.
