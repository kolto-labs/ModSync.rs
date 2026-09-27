# Task 2 fix round 2

Commit `28565433815d60349dc1bf12c8924d1990ff2a4d`, BASE `7e804fa25f2468ffff23b863b0524fa2085ec7a1`. Owned src/ui.rs only. Parent authorized narrow correction to review-discovered terminal-order regression. No tests changed/run.

Done now computes one actual stopped reason (cancelled, changed inputs, or missing accepted report), clears report/presentation first, then sets the local failure and appends the same actual reason to Activity. Removed the later blanket clear that erased the failure. Discarded global status no longer incorrectly insists inputs changed when cancellation caused rejection. Successful readiness remains report-success + successful worker + uncancelled + matching input stamp. Default build passed15.46s (/tmp/task2-fix2-build.log). Final native stale/cancel proof uses the subsequent authorized utility-route commit, which adds access needed to configure a watched choices-file input. No native proof inferred from this source/build alone.
