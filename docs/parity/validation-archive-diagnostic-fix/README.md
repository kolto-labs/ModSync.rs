# Actionable archive diagnostic integration fix

Source `099834c108696cec3f413a003e13bb869d9ea73a`, base documentation checkpoint `63c8bc63efd517b48d207ccd6cb82867aad8c1fe`. The copied/live/restored-target executable SHA256 is `66d1a3cf129bbd930fdb30a20de09bdf80f8d1d57a1dd37dda1d0cc6689c25bf`. **Scoped specification/quality review is approved and F1 is addressed:** [fix review](reviews-and-rulings/final-package-fix-review.md); the original [changes-requested review](reviews-and-rulings/final-package-review.md) is preserved verbatim. This focused evidence does not rebind the earlier [47c3e67 native package](../validation-native/README.md).

Whole-package review found that known per-component archive errors lacked an ERROR: marker, yielding only an aggregate card and causing first-issue lookup to target the final failure status. The shared failed-outcome mapper now adds the prefix once, retaining the component identity/detail. **Raw JSON message values change; schema, stage ordering, counters and warning policy do not.** This is shared report construction, not GUI-only severity rewriting or a second validation implementation. Existing prefixed errors are preserved; that idempotence branch was source-reviewed without synthetic runtime injection.

| Direct case | Full CLI outcome | Native evidence |
|---|---|---|
| Missing Extract archive | 1 error / 1 warning / 4 passed, exit1 | [automatic first issue](missing-auto-first.png), [scroll away](missing-scrolled-away.png), [manual first issue](missing-manual-first.png), [actual clipboard](missing-copied-report.txt) |
| Corrupt Extract archive | 3 errors / 0 warnings / 3 passed, exit1 | [automatic first issue](corrupt-auto-first.png), [scroll away](corrupt-scrolled-away.png), [manual first issue](corrupt-manual-first.png), [actual clipboard](corrupt-copied-report.txt) |
| Missing loose Copy | 0 errors / 1 warning / 5 passed, exit0 | Shared CLI warning behavior preserved; this focused fix did not repeat the earlier complete Copy GUI matrix |

Both archive native cases expose the affected path/detail in a component card, retain an actual ERROR log target for automatic/manual first-issue navigation, and block Next. Independent X11 clipboard text preserves the full human report. Native corrupt validation temporarily placed the same23-byte invalid archive into the loaded missing-archive fixture; it was removed afterward. The receipt records exact commands, paths and source/binary hashes.

All three strict installation probes reject with exit1 before snapshots/pending records or game/mod content writes. Original fixture files remain byte-identical. Six expected lock files and empty state directories were created outside game/mod content roots and are explicitly listed in [receipt.json](receipt.json); this is not a claim of zero filesystem writes. Before/after checkpoint manifests preserve the distinction.

The integrator independently checked current source hashes, copied executable hash and all36 receipt-artifact hashes before copying. [archive-manifest.json](archive-manifest.json) enumerates byte-preserved archived originals and fixture inputs; the executable is omitted. No log/report normalization, Rust tests, custom Rust harnesses or additional compiler/runtime probes were performed by the integrator. Worker production build, production Clippy and format logs are retained. Review reports retain their original verdicts and source bindings under `reviews-and-rulings/`.

Scope remains bounded: C# clipboard readback, full GUI/font/layout/accessibility equivalence, broader remaining/FOMOD matrix, comprehensive scrolling/splitter coverage, platforms and live/full installation remain open as before. This corrective evidence closes only the known archive-card/first-issue defect after scoped review.
