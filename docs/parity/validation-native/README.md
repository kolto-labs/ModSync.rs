# Native validation page evidence

Bounded Linux native handoff, 2026-09-27. Product source **47c3e675b93abcbd8660ad84a0b1fdd74b4aad95**. Copied, live and restored target executable SHA256 **5a77c2c24d9f067a1033f60f765ece53a971955a921c6e437f67b9a73eedb34c**. Final integrated review remains pending; this is not full C# GUI acceptance.

[Archive manifest](archive-manifest.json) records each original path, byte count and SHA256. Copies preserve original bytes, including logs and clipboard text; no normalization was applied during packaging. The 91 MB executable is omitted; its copied hash was independently verified against the worker receipt. Current source/font hashes and every final receipt artifact hash were also independently verified. Core before/after game/mod file manifests are equal; this is checkpoint evidence, not syscall tracing. Only owned disposable fixtures were used; no install was invoked. Worker-owned GUI/Xvfb cleanup is recorded in the final receipt and cleanup log. Compiler availability was handed back to the parent, not reserved by this documentation task.

## Final source-bound cases

[Final receipt](47c3e67/receipt.json) and [implementation handoff](reviews-and-rulings/task-2-report.md) preserve commands, ownership and limitations.

| Case | Evidence | Observed scope |
|---|---|---|
| Clean validation | [capture](47c3e67/clean.png), [native clipboard](47c3e67/clean-report.txt) | 0 errors / 0 warnings / 6 passed |
| Missing loose Copy | [settled capture](47c3e67/warning-settled.png), [native clipboard](47c3e67/warning-report.txt) | 0/1/5, truthful review warning; warning.png is an intermediate83% frame |
| Hard Environment failure | [capture](47c3e67/failure.png), [native clipboard](47c3e67/failure-report.txt) | Pending recovery fixture,1/0/0, Next blocked; different cause from C# missing patcher |
| Restored inputs and continuation | [report](47c3e67/repaired-report.txt), [Next result](47c3e67/repaired-next.png) | 0/0/6, actual navigation to Ready to Install; no install |
| Responsive Stop/cancel | [running](47c3e67/small-running-visible.png), [confirmation](47c3e67/small-stop-modal.png), [cancelled](47c3e67/small-cancelled-log.png), [expanded text](47c3e67/small-cancelled-expanded.png) | Actual600×400 run, Stop confirmation, cancelled terminal/no report/Next blocked |
| Setup failure/reset | [setup failure](47c3e67/setup-failure.png), [non-validation clearing](47c3e67/nonvalidation-clears.png) | Separated header and actionable error; old presentation removed |
| Copy feedback | [feedback](47c3e67/copy-feedback.png), [expired](47c3e67/copy-feedback-expired.png), [readback check](47c3e67/clipboard-readback-check.json) | Independent Rust xclip output, timed feedback; no trailing newline |
| Theme behavior | [K1 hover](47c3e67/k1-failure-hover.png), [K2 hover](47c3e67/k2-failure-hover.png) | Source-grounded tokens; no exact Avalonia metrics claim |

## Earlier bindings retained

[5548736 binding](5548736/binding.json) covers [stale input rejection](5548736/stale-result.png), [large-window cancellation](5548736/cancelled.png), [settings route](5548736/settings-route.png) and [validation utilities](5548736/validation-tools.png). Its ui.rs lifecycle hash matches final47c3e67; the page layout changed. These remain earlier-binary evidence.

[556b165 binding](556b165/acceptance-binding.json) accompanies [scrolled results](556b165/acceptance-results-scrolled.png) and [manual first-issue targeting](556b165/acceptance-manual-first-issue.png). The initial receipt contains additional historical evidence references not all copied here; the archive manifest explicitly enumerates this selected package.

[Native C# reference](../validation-reference/README.md) remains separately pinned tod31ccbd. Matching guide SHA256 is3c184ea3922d6454cc3b18dc26e3a8e7250fc164ca9075ab83723ef78e0c873b. C# actual clipboard readback is unresolved, so Rust xclip does not prove bilateral copied-text parity.

## Review, verification and limits

[Reviews and rulings](reviews-and-rulings/) preserve task reports, original findings, corrective reviews, briefs and the implementation ledger without duplicating giant Git diffs. Historical pending statements describe their capture time; the integration document and this README describe current state. [Production logs](build-logs/) record final default/headless builds, production lib/bin Clippy and formatting. No Rust tests were added, adapted or run after the user freeze. Existing obsolete inline message fixtures remain; no all-target/test compilation is claimed.

[Integration rulings](../validation-page-integration.md) record truthful warning copy, raw Report versus pure projection, proactive Next gating, strict install/receipt protections and backend limits. Remaining native gaps include retained-remaining/effective-base FOMOD, greater-than-five mixed diagnostics, comprehensive final splitter/reset/independent-scroll behavior, DPI, keyboard/screen-reader coverage, cross-platform GUI and full/live installation. Monochrome emoji, egui font metrics/control sizing and the responsive stacked action row differ from Avalonia;600×400 needs whole-page scrolling. Final acceptance must remain bounded to the cases above.

Packaging verification: the complete staged whitespace check flags only two literal blank-context lines in the historical raw `validation-missing-source-fix/prefix-hoist.diff` (lines32 and45). Those original patch bytes are preserved, SHA256 `d9fe8f3c50112f9719acfcf40a53b2b22aeade94571d08edb98acea752397f10`. After explicitly tracking ignored logs, three original C# runtime logs also contain original trailing whitespace/CRLF. The scoped check excludes these four exact immutable artifacts, listed with hashes in [packaging-verification.json](packaging-verification.json); no copied report/log was normalized.
