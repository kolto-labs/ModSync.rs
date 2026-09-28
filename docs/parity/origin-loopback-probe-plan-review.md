# Loopback origin proof plan review

Read-only independent review; implementation remains pending.
Reviewer session `b49e9422-4bca-4174-8418-990da5ae5f34`.

## Review verdict: APPROVED, with two required corrections to the plan text (not blockers) and one dependency to flag

### Independent findings (before reading advisor's input)

- `src/download.rs:222-226` builds the reqwest blocking client fresh per call inside private `fetch_inner`; `fetch_to`→`fetch_inner` already forwards all args (`download.rs:176-193`), so threading an optional loopback-CA reference through unchanged public signatures is mechanically straightforward — matches the plan's claim.
- `RunSettings` (`download_orchestrator.rs:408-419`) has no CA field and the plan doesn't propose adding one; it proposes a parallel entry point. That avoids the real hazard of adding a new `pub` field to a struct with non-`..Default` literal constructors elsewhere.
- `Cargo.toml:36`: `reqwest = { version = "0.12", features = ["blocking", "rustls-tls"] }` — no `socks` feature. `ClientBuilder::add_root_certificate`, `.no_proxy()`, `.resolve()`, `.redirect(Policy::custom(...))` are all present in reqwest 0.12's blocking builder regardless of TLS backend, so the proposed mechanism is available.
- The `--extra-ca-file`, `--nexus-api-base`, loopback-only scoping, and "never in settings/env" constraints in this plan are **not novel** — they restate open questions 3 and 10 and the redirect-policy note already approved in `docs/parity/download-orchestration-plan.md:323-326,389,459,466`. That's a real precedent referent, not an invented approval.
- Nexus fake-API loopback base is already implemented and scoped separately (`download_orchestrator.rs:513-516`, `updates.rs::with_base_url`), consistent with the plan's claim that it stays "separately scoped."

### Advisor's points, checked against source

**Point 1 (possible blocker: race needs `settings.peer.is_some()`, tunnel-down default is `None`)** — checked `src/p2p.rs`: it already has `PeerSource` (`p2p.rs:1825`), `PeerPolicy` (`:1155`), and working `socks5://` proxy plumbing (`:326-338,565-677`), i.e., a concrete non-"pending tunnel" transport already exists in this worktree, ahead of the historical plan's open question 11. `fetch-downloads` isn't in `main.rs` yet (grep: no match) — Task 2a hasn't landed, which matches this plan's own line 3 ("Start after the selected-guide CLI slice is reviewed"). So this is a sequencing dependency, not a defect in the probe plan: whoever wires `fetch-downloads` needs to actually pass a live `PeerSource` into `RunSettings.peer` for probes 3/4's peer-win/race claims to be non-vacuous. **Downgraded from blocker to a note for the CLI-slice reviewer**, since it's outside this plan's file ownership.

**Point 2 (negative checks may pass vacuously in an isolated netns) — holds, real gap.** Refusing a public host or a deceptive suffix only proves something if that target is reachable and would otherwise succeed. Correction needed in the "Direct production acceptance" section: item 2 must specify a positive control — a reachable non-loopback address inside the netns (e.g., a dummy interface with a TEST-NET IP) plus a netns-scoped hosts file mapping `localhost`/deceptive names to it, and for the proxy claim, an `HTTPS_PROXY` pointed at a recording CONNECT proxy showing the *ordinary* client uses it while the override client makes zero connections. Also correct: SOCKS relay only covers peer traffic, not reqwest's HTTPS proxy path (no `socks` feature built).

**Point 3 (redirect policy: `stop()` vs `error()`) — holds, real gap, concrete correction.** If the alternative `Policy::none()` is chosen, a blocked redirect returns the 3xx response itself to `fetch_inner`, which then runs its HTML/challenge sniffing (`download.rs:254-311`) — a security refusal could get misreported as `ManualRequired(HtmlLanding)` rather than a clean refusal. Correction: the custom policy must use `Action::error(..)` for a disqualified hop, not `Action::stop()`, so an escape attempt is classified `Permanent`, not conflated with a host gate. Add this as an explicit line in "Proposed implementation."

### Report

- **APPROVED**, conditioned on folding two corrections into the doc before implementation:
  1. Direct-production-acceptance item 2 needs positive controls (reachable dummy target, hosts-file-mapped deceptive names, recording CONNECT proxy) — without them the negative claims are unfalsifiable in an isolated netns.
  2. Proposed-implementation section must specify `Action::error()` (not `stop()`) for a disqualified redirect hop, so refusals aren't misclassified as `ManualRequired`.
- **Dependency, not a blocker on this doc**: probes 3/4's peer-race claims need whoever lands the `fetch-downloads` CLI slice to actually wire a live `PeerSource` (already implemented in `src/p2p.rs`) into `RunSettings.peer`; flag this to that reviewer, out of this plan's file ownership.
- No leak/downgrade/credential concern found: no system trust store touch, no env/settings read, no global state, HTTPS+loopback-only enforced at both entry and every redirect hop, Nexus key path stays disjoint. Python-only fixtures (TLS server, recording relay, disposable netns) can establish the stated claims without a Rust harness, matching the precedent already set in `download-orchestration-plan.md`'s own "Acceptance probes without Rust tests" section.

## Controller resolution

Both required text corrections are incorporated: reachable positive controls
for rejected non-loopback/redirect/proxy routes, and redirect policy error
instead of stop. The actual Peer adapter remains a prerequisite, and its real
local transfer proof is required by the preceding CLI slice. No runtime proof
is claimed by this plan review.
