# Wave 2 integration checkpoint

Integration through `90efdf295877bd04be63350e6e37d50df8a7d77a` on `codex/rust-parity-wave2-resume`, based on
`4bfacda`. Seven approved lanes and the independently reviewed deployment
round four are merged locally. No push or release is claimed.

## Evidence

- [Seven-lane integration review](seven-lane-review.md): READY_FOR_GATES.
- [Direct CLI install and completion probes](wave2-cli-probes.json): 8 assertions.
- [Credential/endpoint merge probes](wave2-key-probes.json): 4 assertions, localhost
  and isolated network namespace only; no real credentials used.
- [Native launch receipt](wave2-native-receipt.json) and
  [capture](wave2-native.png): private Xvfb display, guide preloaded, visible
  Welcome page. Not a complete wizard or completion-flow acceptance pass.
- [Deployment round four](../deployment-round4/build-receipt.json): 82 CLI checks
  and 8 assertions against the pinned C# validation DLL, with separate static review.

After deployment integration, both production feature configurations built
and passed Clippy with warnings denied. The combined binary also passes all
82 deployment CLI checks. See the `wave2-final-*.log` command output and
[source receipt](source-receipt.json). The final specification review found a profile-switch lock/error-handling gap;
commit `2252894` fixes it. [Seven direct switch scenarios](switch-probes.json)
pass, with [reproducible disposable driver](switch-probes.py). [Spec re-review](spec-review-final.md)
passed at that head. [Quality review](quality-review-round1.md) then found a
modified-file soft-skip path that removes the manifest and allows switching,
plus an active-profile read before the game lock. The former was reproduced;
fix `714e59b` closes these with an all-manifest preflight and lock-before-read.
[Round-two CLI receipt](switch-round2-build-receipt.json) records 39 passing
assertions, and [desktop gates](switch-round2-desktop-receipt.json) also pass.
[Spec re-review](spec-review-round2.md) and
[quality re-review](quality-review-final.md) passed for this bounded integration. The source receipt above is bound to
pre-switch `285f55d`; [switch production gates](switch-build-receipt.json)
record the subsequent source and binary binding.
No Rust tests were created, modified or run. [Freeze verification](freeze-check.json)
compares AGENTS.md, the tracked tests directory, and the complete cfg(test) suffix
in each of the seven pre-existing source files that contains one. All are
byte-identical to the wave-one source.

## Remaining work

F01–F28 remain partial and F29 unverified. In particular, the acquisition
library and manual torrent commands exist but selected-guide Fetch Downloads
is not yet exposed through both frontends; the concrete PeerSource still needs
an acquisition-trait adapter and frontend wiring. This checkpoint does not
claim those planned tasks are done. The source-normalization plan is proposed
and has no implementation or independent approval yet.

The dirty main checkout and incomplete `modsync-wt-integ2` are preserved.
The pre-existing frozen UI test compilation error and CI test-freeze conflict
remain open; production gates are not all-target or CI acceptance.
