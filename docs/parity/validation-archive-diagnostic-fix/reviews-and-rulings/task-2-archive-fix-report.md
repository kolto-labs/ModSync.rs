# Final package archive-diagnostic correction

F1 from final-package-review.md is addressed in source-only commit `099834c108696cec3f413a003e13bb869d9ea73a`, BASE/documentation checkpoint `63c8bc63efd517b48d207ccd6cb82867aad8c1fe`. Only `src/validation.rs` changed (9 insertions, 1 deletion). Parent authorized this sole combined-path fix. No GUI, engine, mapper, Cargo, font or test changes; no push or subagents. Diff: task-2-archive-fix-review.diff.

## Change and contract

Report::component_validation now marks failed outcome messages `ERROR: ` unless already prefixed. It preserves the full engine-supplied component identity and diagnostic, without guessing or duplicating names. The existing shared projection consequently creates the individual component error card, retains the complete detail in the human Results and Log sections, and selects the actual archive ERROR line before the terminal “❌ Validation failed” line. Aggregate archive card and counters remain unchanged.

The raw JSON schema/field structure is unchanged. There is an intentional value-level correction: failed ComponentValidation.messages strings now begin with `ERROR: `. Consumers comparing exact error strings must account for that severity marker. Success strings and missing-Copy warning values/counters remain unchanged. The duplicate-prefix guard is source-reviewed, without synthetic runtime injection.

## Production checks

Final source commands ran with cwd/tmp, login:false, explicit manifest `/home/brunner56/Workspaces/ModSync.rs/Cargo.toml`, cargo `/home/brunner56/.cargo/bin/cargo`, compiler target `/tmp/modsync-rs-target`:

- `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo build --manifest-path ... -j2`: passed11.46s.
- `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo clippy --manifest-path ... --lib --bin modsync -j2 -- -D warnings`: passed6.15s.
- `cargo fmt --manifest-path ... --all -- --check`: exit0, no output.

Final logs copied into `/tmp/modsync-validation-archive-fix/{build,clippy,fmt}.log`. No headless build was necessary for this bounded shared message correction; the default GUI binary remained current. No Rust tests/harnesses created, modified or run. Both validation.rs and ui.rs complete existing `#[cfg(test)]` tails were verified byte-identical to BASE. Existing deferred inline-test API incompatibilities remain as previously disclosed.

## Direct CLI and strict installation probes

All exact argv and exit codes are preserved in `/tmp/modsync-validation-archive-fix/cli-commands.json`. Each of three fresh disposable fixtures received:

```sh
modsync-gui validate --input FIXTURE/guide.json --game-dir FIXTURE/game --source-dir FIXTURE/mods --use-file-selection --validation-report --full --dry-run
modsync-gui install --input FIXTURE/guide.json --game-dir FIXTURE/game --source-dir FIXTURE/mods --use-file-selection
```

| Fixture | Report errors/warnings/passed | Report exit | Strict install |
|---|---|---|---|
| Missing Extract package.zip |1/1/4|1|exit1, missing-source preflight rejection|
| Corrupt Extract package.zip |3/0/3|1|exit1, unsupported signature rejection|
| Missing Copy sample.tga |0/1/5|0|exit1, missing-source preflight rejection|

Both Extract archive messages now start `ERROR: Fixture: archive/destination validation:` and retain their actionable missing-path/unsupported-signature details. Actual strict attempts changed no original fixture/game/mod files. Six expected root lock files were created outside game/mod roots, plus empty recovery state directories; they are enumerated in receipt. No snapshots or pending markers were produced. Before/after manifests and stdout/stderr are preserved.

## Native and clipboard evidence

Fresh private Xvfb PID2207483/display:1, owned GUI PID2210322, private XDG/runtime/DBus. Launch argv/env are in launch.json. No existing user display/process was operated. Native GUI first loaded extract-missing; after recording missing evidence, its package.zip was populated with the exact23-byte invalid archive used in the separate corrupt CLI fixture and validation was rerun. That temporary native archive was removed afterward.

- `missing-auto-first.png`: actual1/1/4, red ❌ Fixture card with missing package.zip path, log positioned at actual archive ERROR line.
- `missing-scrolled-away.png` then `missing-manual-first.png`: deliberately scroll both panes away, click Go to first issue, see return to actionable component card/error line.
- `corrupt-auto-first.png`, `corrupt-scrolled-away.png`, `corrupt-manual-first.png`: same observed behavior for actual3/0/3 with corrupt archive.
- `corrupt-card-expanded-area.png`: collapsing log exposes the whole multiline component card, preserving full path and unsupported-signature explanation. The earlier expanded-log viewport clips the bottom of this card until scrolled/collapsed; content is retained.
- `missing-copied-report.txt` and `corrupt-copied-report.txt`: actual Copy report followed by independent `/usr/bin/xclip -selection clipboard -o`. Both Results contain the component diagnostic and complete Log contains actual ERROR marker before terminal failure. Receipt records exact bytes SHA256 and first error line.

Input used actual `/usr/bin/xdotool` on private display. Run104,304; Copy338,362; firstissue246,362; logtoggle92,362. Scroll-away used wheel-down at650,450 in log and650,570 in results. Captures are actual `/usr/bin/import -window root`, not reconstructed interfaces. No additional broad native matrix was repeated.

## Binding, cleanup, limits

Receipt: `/tmp/modsync-validation-archive-fix/receipt.json`. Commit and all four relevant source hashes, copied binary, actual live `/proc/2210322/exe`, default target, fixture manifests, argv and artifact hashes are bound. All three executable hashes match `66d1a3cf129bbd930fdb30a20de09bdf80f8d1d57a1dd37dda1d0cc6689c25bf`.

Verified owned GUI/Xvfb identities before SIGTERM and confirmed both exited. No native job/compiler remains; compiler slot free. Core fixture content restored; six lock files/empty recovery directories remain explicitly documented. Earlier47c3e67/5548736/556b165 evidence stays immutable and separately bound.

This resolves the scoped archive card/first-issue finding. It does not establish full exact C# parity, raw C# clipboard behavior, or previously unproven matrix cells. Those prior limits remain. Final scoped reviewer approval and durable packaging are owned by parent/reviewer/integrator.
