---
artifact_contract: "ce-handoff/v1"
created_at: "2026-09-28T14:32:00Z"
title: "Cloud continuation of Rust ModSync parity (session 2)"
summary: "S1 QUALITY, Nexus H1a, S2a, S2b and Fetch shared-destination work merged; downloads GUI step 1 in PR #5 awaiting re-review; sibling repos integrated; CI blocked by account Actions minutes."
keywords: ["modsync", "rust", "cloud", "parity", "handoff"]
repository: "bodencrouch/ModSync.rs"
supersedes: "docs/handoffs/rust-parity-cloud.md (read it for constraints and the full implementation map)"
source_checkpoint: "main 70067ef; PR #5 head ae1ba56"
resume_focus: "Finish review/merge of downloads GUI step 1 (PR #5), then downloads GUI steps 2-3, then the remaining map."
---

# Entry point

## Subsequent continuation: downloads GUI step 2

PR #5 was merged as `d438528`; its Stop-during-planning correction is on main.
The docs-only audit merge `1d92ba4` was incorporated without altering product source.
The bounded downloads GUI step 2 is now implemented and accepted: preserving peer
settings/disclosure, serial context-bound FOMOD questions, explicit error retry or
dismissal, readiness guards, and responsive navigation. Independent review and
exact final source hashes are in [the review](../parity/downloads-gui-step2-review.md).
All five production gates and 46/46 native assertions across five isolated Linux
aarch64 scenarios pass; 53 frozen files/regions are unchanged. The
[evidence README](../parity/downloads-gui-step2/README.md) and controller index retain
the proof boundaries and remaining cases. No Rust tests or custom Rust harnesses ran.

Next is the remaining downloads step-3 native matrix, then `persistent-sharing-plan.md`.
Full parity remains incomplete. The original in-flight snapshot and map below are
historical; do not restart step 1 or step 2 from their older status descriptions.

Read [the previous handoff](rust-parity-cloud.md) first. Its constraints still hold:
- No Rust tests or temporary harnesses until full parity. Production gates and
  isolated fixtures only.
- Network fixtures only under `unshare -Urn`.
- Stop on failure, and preserve guide order.

It also has the bootstrap in [cloud README](../parity/cloud/README.md). This
document records what this session changed and where to resume.

Working rhythm used throughout: one fresh implementer per bounded slice, then an
independent read-only SPEC+QUALITY review, then fix/re-review loops, then a PR into
`main` with a merge commit. Each slice has its own proof directory with a
`probe.py` driving the production binary.

# Merged into `main` this session

| PR | Content | Evidence |
| --- | --- | --- |
| #1 | FOMOD S1 QUALITY (3 rounds), Nexus H1a (redirect policy, key encoding/redaction), FOMOD S2a (reconfiguration, stale/unparseable choices, C# configured boundary) | `fomod-s1-quality-review.md`, `nexus-h1a-review.md`, `fomod-s2a-review.md`; S1 339/339 + 41/41, H1a 138/138, S2a 224/224 |
| #2 | Fetch shared-destination grouping (union-find, serialized groups), Linux-only CI | `fetch-shared-url-proof/` |
| #3 | Unnamed targets adopt a present file under their own hash; Peer race/cancel polish | `fetch-shared-url-proof/` 381/381 |
| #4 | FOMOD S2b: per-archive run-scoped choices, Unicode OrdinalIgnoreCase, FOMOD gate on plain validate/install, C# fallback warnings | `fomod-s2b-proof/` 254/254 |

# In flight

- **PR #5, downloads GUI step 1** (`docs/parity/downloads-gui-plan.md`, G1–G5):
  - Head `ae1ba56`, on branch `baio/nice-archimedes-fmxu3k`.
  - The first review asked for changes. All seven findings are fixed in `ae1ba56`, and an independent re-review was running at capture.
  - Native Xvfb proof in `docs/parity/downloads-gui/` passes 46/46.
  - Next action: read the re-review result, fix anything it raises, then merge PR #5.
  - Unproven in this environment:
    - GUI save and record diff (needs the portal dialog).
    - Peer (no IPv6).
    - Window-manager close.
    - Stale-message runtime (covered by source review only).

# Remaining map, in order

1. Downloads GUI step 2: settings disclosure, guarded FOMOD queue with a lifecycle helper, validate/install blocking, navigation. Step 3: native acceptance, including the 600×400 layout.
2. `persistent-sharing-plan.md`.
3. `host-providers-plan.md`: the rest of H1 (runtime https-to-http downgrade seam, typed auth boundary, premium/free/rate-limit matrix, discovery), then H2–H4.
4. `markdown-remaining-plan.md` M1–M3.
5. `checkpoint-plan.md`.
6. `engine-archive-patcher-remaining-plan.md` E1–E7.
7. `remaining-scope-map.md`, ending with F29.

Open residuals recorded in `docs/parity/PROGRESS.md`:
- **FOMOD S2:**
  - ownership propagation
  - gate hard-dependency scope
  - missing-archive soft warning
  - folder sources named like archives
  - desktop reconfiguration
  - S3 corpora
- **Fetch:**
  - Origin-present race adoption cannot fall back to Peer
  - `adopt_present` hash ignores cancel
  - unnamed Nexus API name present
  - concurrent known-name publish
- **Downloads:** Origin progress always reports `total: 0` (`src/download.rs:453`).

# Sibling repositories (all integrated and on default branches)

- **ModOrchestration** (C# reference, default `baio/laughing-colden-6090a5`):
  - PR #1 merged all five feature branches, plus review fixes.
  - Completed-unverified installs exit 2.
  - The final duplicate sweep was un-wired pending dry-run parity.
  - The workflow trigger and runner fix was prepared but blocked by the permission check. Its workflows only trigger on main/master, so CI has never run. The owner must approve ("apply the ModOrchestration CI changes").
- **KotorPhasor:** PR #1 merged `codex/lua-boundary-validation`. 40/40 ctest on gcc and clang with LuaJIT.
- **holowan-biolabs/mod-builds:** PR #1 squash-merged the `feat/flows` v12 API data into `dev`. Open question for the author: the `k2/build.yml` schema requires a `main` section.

# Environment facts

- **GitHub Actions:** every job on this account fails within seconds with no steps (since 2026-09-16). This matches exhausted private-repo included minutes. It is not a code problem, so every PR carries local gate evidence and one explanatory comment.
- **Windows cross-check:** clippy needs `gcc-mingw-w64-x86-64` plus a shim dir with `PowrProf.h`/`Wbemidl.h` that includes the lowercase MinGW headers. See `.github/workflows/ci.yml` for the recipe.
- **`holowan-biolabs/kq`:** was not accessible; not needed so far.
- **.NET 9 SDK:** unreachable. ModOrchestration was verified with the apt `dotnet-sdk-10.0` plus roll-forward.
- **`/compound-engineering:*` skills:** not installed in this session; the workflow was followed manually.

# Resume semantics

This is a status snapshot. The user asked for hands-off execution: take the
recommended path at each decision and do not pause for confirmation, except where a
permission check blocks an action. Those go back to the user.
