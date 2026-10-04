# Drafted Rename proof

Contract: [drafted-rename-contract.md](../drafted-rename-contract.md).

[probe.py](probe.py) drafts instructions from one component's prose and runs the
production `modsync` on a fresh fixture each time: `game/{chitin.key, Override/}` and
`mods/old.tga` (text `old`). The cases are one-component JSON guides with a fixed GUID:

- drafted Renames: "Rename the file old.tga to new.tga." (`rename_file`), the same with
  quotes (`rename_quoted`), "Rename old.tga to new.tga." (`rename_short`) and "Rename
  the file old.tga as new.tga." (`rename_as`);
- fail-closed controls: "... to newname." (`no_extension`), "... to sub/new.tga."
  (`folder_part`), the corpus sentence "... rename them to PLC_CompPnl_b, ..."
  (`pronoun`) and "... to <<kotorDirectory>>/Override/new.tga." (`placeholder`);
- `markdown`: the `rename_file` prose as the Installation Instructions of a Markdown
  guide with a ModSync block (GUID, `IsSelected`) and no instructions.

Checks per case (61 in all):

- `draft`: `convert IN D --parse-directions --format json`; the drafted Action, Source
  and Destination of each instruction and `DraftReview.RequiresReview`;
- `csharp` (JSON cases): the drafted instructions equal the pinned C# drafter's for
  the same prose, from [csharp-drafts.json](csharp-drafts.json);
- `gate`: `install -i D -s mods -g game --no-checkpoint` on the unreviewed draft fails
  and leaves the files unchanged;
- `review`: `convert IN R --parse-directions --review-draft <GUID> --format json`
  succeeds (it fails while unparsed prose remains);
- `plain`: `validate -i R -s mods -g game` passes;
- `full_dry`: `validate -i R ... --output json --full --dry-run` exit code and
  `success`;
- `install`: `install -i R -s mods -g game --no-checkpoint` exit code and every file
  under `game/` and `mods/` with its text afterwards;
- `md_validate`, `md_install` (`markdown`): `validate` and `install` on the `.md`
  guide itself, which drafts on load, fail at the review gate (the install writes
  nothing).

Expected: the drafted Renames draft `Rename [<<modDirectory>>\old.tga] -> new.tga`,
and after review validate and install, leaving `mods/new.tga` with `old`. The
`no_extension`, `folder_part` and `pronoun` drafts keep `<<kotorDirectory>>\Override`
and are refused after review with nothing written; `placeholder` drafts no Rename and
its unparsed prose blocks the review. The four drafted Renames' `csharp` checks are
the documented intentional difference (`known`): it passes only when Rust differs
from the pinned drafter and drafts exactly the expected Rename.

[csharp-drafts.json](csharp-drafts.json) is the `0-csharp.json` output of
`tools/check_drafts.py` (the managed probe over `DraftInstructionService`, under
`bwrap` with no network) for the cases `probe.py --cases-json` writes, against
`ModSync.Core.dll` from a `d31ccbd3` build, run with `DOTNET_ROLL_FORWARD=Major`.

## Results (cloud Linux, 2026-10-04)

| Binary | Result |
| --- | --- |
| this slice | 57 match / 4 known difference / 0 fail |
| `b5df010` (pre-fix) | 37 match / 0 known difference / 24 fail |

On `b5df010` the five drafted Renames (`rename_*` and `markdown`) drafted the Override
Destination (with `new.tga` as a second Source after "to"), so their `draft`, `plain`,
`full_dry` and `install` checks fail (20): after review every one was refused and
`mods/old.tga` stayed. Their four `csharp` checks fail too, as Rust then matched the
pinned drafter. The review gate, the review and the controls gave the same results on
both builds.

`tools/check_drafts.py` on both builds, against the same pinned DLL: 0 differences over
`tests/fixtures/drafts.json` (19 components), these cases (8) and the K1/K2 full and
spoiler-free guides of `mod-builds` `21b2f04` parsed by `convert --format json`
(186, 145, 180 and 139 components). This slice adds 1 intentional Rename difference
in the fixture and 4 in these cases, and none in the corpus.

## Reproduce

```bash
cargo build --release --locked --lib --bin modsync -j4
python3 docs/parity/drafted-rename-proof/probe.py
python3 docs/parity/drafted-rename-proof/probe.py --cases-json <cases.json>
DOTNET_ROLL_FORWARD=Major python3 tools/check_drafts.py --reference-dll <ref>/ModSync.Core.dll \
  --rust target/release/modsync --output <scratch> --input <cases.json>
```

`--bin` picks another binary (default `target/release/modsync`); `--verbose` prints
each command's last output line. `MODSYNC_PROOF_OUT`, when set, is the parent of the
probe's temporary directory. `<ref>` is the `d31ccbd3` build output; the C# drafter
output is recorded, so the probe itself needs no .NET.
