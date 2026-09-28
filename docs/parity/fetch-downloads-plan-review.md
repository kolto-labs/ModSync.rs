# Selected-guide acquisition plan review

The controller-reconciled contract was independently reviewed by Sonnet/low,
session `eba4d06e-c66b-4a8c-a6b6-72de3986eb68`. Review was read-only; it did
not establish implementation or runtime acceptance.

## Initial review

## Verdict: APPROVED with two concrete corrections

I independently traced every "Verified interfaces" claim (plan:28-51) against current source and every one checked out exactly — `Acquire::acquire` (acquisition.rs:130-136), `AcquisitionEvent` fields (acquisition.rs:183-189, no `registry_url`), `ArchiveTarget::destination` file/dir split (acquisition.rs:111-116, mirrored in `PeerStaging::create` at download_orchestrator.rs:870-886), `SettingsFile::resolve/load` absent→`{}` / malformed→error (profiles.rs:648-675), `PeerSettings::from_settings/effective_mode/policy` (p2p.rs:370-473), and `fetch_authored`'s exact signature (p2p.rs:1282-1290, with `PeerSource::acquire` at p2p.rs:1839-1857 confirming it's meant to be wrapped, not duplicated).

Critically, `RunSettings` already has `pub peer: Option<Arc<dyn Acquire>>` (download_orchestrator.rs:418) and `race_origin_peer` already exists with private staging, no-clobber publish, loser discard/Superseded, and single-consume-per-racer logic (download_orchestrator.rs:940-1149) — so this is genuinely a "hardening/wiring" task, not a from-scratch build, and no frozen test covers `download_orchestrator.rs` (no `tests/download_orchestrator.rs`, no in-file `#[cfg(test)]`), so restructuring race timing carries no test-freeze risk.

**Exchange block 1 — O2 peer-start timing vs. current code**
claim: "In Race mode start Origin first; start Peer only after Origin's first Transferring ... or a confirmed non-gate failure" (plan:60-62)
referent: download_orchestrator.rs:988-997 — the Peer thread is `scope.spawn`ed unconditionally, concurrently with Origin, at race entry, with no gate check before spawn.
checked: read function body 940-1149; confirmed via P3 (plan:90-93) that "no policy/network for ... gated Origin" is instead an *adapter-internal* obligation (i.e., the thread starts immediately but must not bootstrap network activity until eligible).
verdict: holds as written, but is ambiguous — reads as an orchestrator-level ordering requirement while current code implements it as an adapter-level gating requirement. **Correction: reword O2's first sentence to clarify "start" means eligible-to-transfer, not thread spawn, so the implementer doesn't waste effort rewriting `race_origin_peer`'s threading model.**
confidence: high; the ambiguity is textual, not architectural — P3 already states the real mechanism correctly.

**Exchange block 2 — acceptance fixture scripts don't exist**
claim: "Reuse established p2p fixture scripts, inspect them before running" (plan:160)
referent: `Glob **/*seeder*`, `**/*socks*` → no files; `docs/parity/p2p-acquisition-plan.md:381` shows only disposable ad hoc `/tmp/p2p-probe3` fixtures from a prior one-off probe, not checked-in reusable scripts.
checked: searched all of `docs/parity/**` for socks/seeder/fake-nexus scripts — none found.
verdict: does not hold — no established, reusable SOCKS/seeder fixture scripts exist in this worktree. Scope item 5 (plan:19) does authorize building fixture drivers under `docs/parity/fetch-downloads/`, so this isn't a feasibility blocker, but the acceptance section's wording overclaims existing infrastructure. **Correction: change plan:160 to "build minimal local SOCKS/seeder/tracker fixture scripts under `docs/parity/fetch-downloads/` as part of item 5; none currently exist to reuse."**
confidence: high, direct negative search result.

**Exchange block 3 — local-network proof viability (P3 guide-authored-peer-ignore vs DHT/LSD off)**
claim: acceptance item 3 needs real local Peer completion via LOCAL SOCKS/seeder only, with public lists disabled and no DHT/LSD/UPnP fallback (plan:91-93, 157-160)
referent: `peerExtraPeers`, `peerTrackers`, `peerProxies`, `peerTrackerListUrl: null` are user-settings keys read in `PeerSettings::from_settings` (p2p.rs:418-438), independent of guide-authored peer addresses.
checked: p2p.rs:370-440.
verdict: holds — a fixture can point `PeerSettings` at a loopback tracker/seeder/proxy via settings.json without violating P3's "guide-authored addresses ignored" rule, so acceptance item 3 is achievable once fixture scripts (block 2) are built.
confidence: high.

No other blockers found: mode/settings precedence (C2/C3) matches `effective_mode`/`nexus_api_key` exactly; `apply_selection` staying main.rs-private is correct since it's a CLI-flag concern reused alongside existing commands, while plan/run/record/fomod_followups genuinely already live in the library (download_orchestrator.rs:258,486,1433,1536), so C4's "shared library" claim is accurate.

## Resolution

The fixture-script wording was corrected. The controller challenged the initial
adapter-side gate suggestion using the actual trait signature; the reviewer
re-derived the call-site requirement and approved the clarified contract.

`peer.acquire(...)` is called directly inside the spawned thread's closure body with no wait/gate before it — the call site is unconditional. `Acquire::acquire`'s signature (acquisition.rs:130-136) takes `target, destination, cancelled: &AtomicBool, on` — no gate/origin-state argument, confirming the adapter genuinely cannot see Origin's gate status. My previous claim that P3 could handle this adapter-side was wrong; the adapter has no channel to learn it.

claim: "orchestration MUST delay calling peer.acquire ... even if its thread is spawned earlier and waits inertly"
referent: acquisition.rs:130-136 (`Acquire::acquire` signature has no gate-state param) + download_orchestrator.rs:991-996 (`peer.acquire` called unconditionally at thread-body entry, no gate wait) + download-orchestration-plan.md:410 (probe8b: "Peer never starts" on gate, "tunnel state is irrelevant")
checked: read all three locations directly.
verdict: holds. Adapter-side gating (my original position) is structurally impossible since `Acquire::acquire` never receives Origin's gate/transfer status, so the delay must live in the orchestrator, at the call site before `peer.acquire` is invoked (thread may still spawn early, but must block or be invoked only after Origin's first `Transferring`/non-gate-failure signal is observed).
confidence: high — trait signature is decisive, not a matter of interpretation.

Revised O2, gating the actual `peer.acquire` call on Origin's first Transferring/non-gate-failure at the orchestrator level (download_orchestrator.rs:991-996), is correct and necessary. **Clarified contract approved.** No remaining blocker on this point.
