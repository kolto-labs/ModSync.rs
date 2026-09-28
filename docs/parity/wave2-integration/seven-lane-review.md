## Review Summary — READY_FOR_GATES

I retract my initial "peer sharing is dead code" theory — my first pass had a grep typo (`Acquire for" ` with a trailing quote) that silently returned nothing everywhere. Redone properly, here's the corrected picture.

**claim:** `librqbit` `disable-upload` feature forces uploads off globally, breaking `torrent-seed`/share ratio.
**referent:** `librqbit-9.0.1/src/torrent_state/mod.rs:124-133` — `disable_upload()` is `false` unconditionally when the feature is *absent*; the feature only makes the field *settable*, default `false` (`session.rs:505-506`). `p2p.rs:1213-1218` passes a locally-computed `disable_upload` bool per session (download vs. share), not a global flag.
**checked:** read both files directly.
**verdict:** does not hold — no defect.
**confidence:** high.

**claim:** `Source::Peer`/`RunSettings.peer` is never constructed as `Some(...)` anywhere (grep `Acquire for`, `RunSettings {` across `src/`), and `ui.rs`/`ui/` never reference `download_orchestrator` — the P2P lane isn't wired into either CLI's guide-driven downloads or the GUI.
**referent:** confirmed by clean greps this time; `download_orchestrator.rs:789-794` fails closed with "Peer sharing paused" when `peer: None`.
**checked:** `Grep "Acquire for"` (0 hits), `Grep "orchestrator" src/ui.rs src/ui/` (0 hits).
**verdict:** holds as fact, but is not a defect — README.md:86-90 and GUI-INVENTORY.md:126 (F14 "partial") already document `torrent-download`/`torrent-seed` as standalone manual CLI verbs, not integrated into guide-driven orchestration or the GUI. Nothing claims otherwise. Fail-closed-when-`None` is the documented, correct behavior.
**confidence:** high.

**claim:** stored/env Nexus key could leak to a caller-chosen `--nexus-api-base` host via the new `--input` acquisition path.
**referent:** `main.rs` computes `key` (explicit-flag-only when `nexus_api_base.is_some()`) *before* branching on `--input`; `download_orchestrator.rs:511-516` only uses `settings.nexus_api_key`/`nexus_api_base` to build one `NexusClient` scoped to `Provider::Nexus` sources.
**checked:** read both sites.
**verdict:** does not hold — explicit-key-only-with-override invariant is preserved on the new path.
**confidence:** high.

**claim:** tracker-list/magnet-tracker fetching could DNS-leak or bypass the proxy.
**referent:** `p2p.rs:804-811` (`.no_proxy()` deliberate, for public list bootstrap only) and `p2p.rs:891-902` (literal-IP-only trackers kept from magnets to avoid resolver leaks), both with explanatory comments.
**checked:** read both blocks.
**verdict:** does not hold — intentional, documented, correct.
**confidence:** high.

**claim:** `engine.scan`'s peer-staging filter matches spec ("skips unaccepted peer staging only under mods root").
**referent:** `engine.rs:225-233`, `download_plan.rs:64-73`, `download_orchestrator.rs:824-835,870-877` (staging dirs created directly under `source_dir`, i.e. mods root).
**checked:** read all four sites.
**verdict:** holds.
**confidence:** high.

No P1 or P2 cross-lane defects found in main.rs merge/secret-scoping, P2P-proxy-to-orchestrator wiring, or the scanner fix. Documented limitations (F14 partial, Peer fail-closed) already state what's not wired, so no false-parity claim exists either.

**READY_FOR_GATES**
