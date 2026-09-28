---
artifact_contract: "ce-handoff/v1"
created_at: "2026-09-28T05:17:19.588451+00:00"
title: "Cloud continuation of Rust ModSync parity"
summary: "Repository-backed complete context: FOMOD SPEC approved, QUALITY next, portable build and fixtures, public reference pins, no workstation dependencies."
keywords: ["modsync", "rust", "cloud", "parity", "handoff"]
repository: "bodencrouch/ModSync.rs"
source_checkpoint: "c277c49ef0cda66473d8b22c79c07f0a414359c7"
resume_focus: "Complete Rust ModSync parity; first independent FOMOD S1 quality review."
---

# Cloud entry point

Repository: https://github.com/bodencrouch/ModSync.rs (private; cloud agent must have
repository read access). This document and every required continuation artifact
are committed. Use the exact commit URL supplied with the handoff, or main for the
latest state. No workstation path, /tmp file, cached binary, memory database,
installed local skill or old subagent handle is required.

```sh
gh repo clone bodencrouch/ModSync.rs
cd ModSync.rs
```

Read [cloud bootstrap](../parity/cloud/README.md) for actual production-only build,
portable CLI/PTY proof, and pinned public C#/guide fetch instructions. Historical
receipt paths are provenance, not prerequisites. Dependencies/network/auth and
Linux namespace capabilities must exist on the cloud runner. The bootstrap is
explicit about later licensed/native acceptance inputs rather than assuming a
private cache is available.

# User intent and stopping point

The user requested complete Rust ModSync functionality against the C# specification,
without omitted functionality, using subagents. Available Codex subagents were
explicitly authorized after Sonnet rate limiting. Latest instruction explicitly
authorizes pushing to the existing repo and a cloud-usable handoff. No public PR
or release was requested. Repository visibility stays private.

**Full parity is incomplete: F01–F28 partial; F29 unverified.** The latest product
source is FOMOD S1 `f3f27d346c9a07dd102d23ed8992f72d19f3c8d2`, evidence
`6d60a2f5778cc4bb2c952a0ec730e6ad9f5d9abf`. Independent SPEC approved;
independent QUALITY has not started. Source checkpoint after subsequent docs is
`c277c49`. Cloud publication changes documentation, proof path plumbing and CI;
it does not make another product-parity claim.

Immediate continuation after the current user's resume confirmation is an
independent FOMOD QUALITY review. Read [retained SPEC verdict](../parity/fomod-s1-spec-review.md),
[contract](../parity/fomod-remaining-plan.md), [input note](../parity/fomod-prompt-io-note.md)
and [evidence](../parity/fomod-s1-proof/README.md), then inspect actual source diff
`2d0ecd4..f3f27d3`; do not substitute approved SPEC or assertion count for quality.
Fix findings with the implementer/replacement and repeat review before moving on.

# Background tasks and agents

No live child agent or task-owned build/probe remained at capture. None is expected
to continue across cloud sessions; spawn fresh agents with explicit bounded tasks.

| Historical task | Completed result now in Git |
| --- | --- |
| fomod_s1_implement / Carver | Sourcef3f27d3, evidence6d60a2f;319/319 production checks58cases plus41inherited; five production gates; build ownership released. |
| fomod_s1_spec | SPEC APPROVED; retained in `docs/parity/fomod-s1-spec-review.md`. Its old requested /tmp/cache report was never written; this committed verdict removes that dependency. |
| host_parity_plan / Bohr | Provider plan plus engine/archive/patcher remaining plan. |
| host_plan_review / Pasteur | Provider plan approved; anonymous full-Markdown output comparison retained under `docs/parity/full-markdown-baseline/`. |
| engine_plan_review | PLAN APPROVED; `docs/parity/engine-archive-patcher-plan-review.md`. |
| cloud_portability_audit | Identified CI test-freeze conflict, hardcoded proof paths and reference prerequisites; corrected current FOMOD entry points and documented remaining historical limitations. |
| Prior fetch/loopback/normalization implementers and reviewers | Completed bounded slices; source/evidence/reviews linked below. |

No scheduler, sidebar task, remote implementation daemon or pending process session
is needed. The eight historical full-Markdown conversions completed. Do not wait
for Sonnet or attempt to reconnect these handles.

# Accepted work, limits and authoritative ledger

Start with [PROGRESS](../parity/PROGRESS.md), [ACCEPTANCE](../parity/ACCEPTANCE.md),
[remaining scope](../parity/remaining-scope-map.md) and [COMPATIBILITY](../../COMPATIBILITY.md).
Older dated statements are historical to their commit. Latest source-bound reviews
and current ledger override stale claims that a delivered CLI does not exist.

- Wave-two/profile switch: source714e59b/evidence90efdf2,39direct CLI assertions,
  SPEC/QUALITY accepted; reviewed checkpoint9ecf8e4. Earlier merged P2P/profiles/
  download/texture/dedup/completion lanes and82-check deployment retain their scope.
- Fetch: efefba1 +2889ec5/evidence14783d2,312acquisition assertions, SPEC/QUALITY;
  `docs/parity/fetch-downloads/` and adjacent spec/quality reviews.
- Loopback HTTPS:6c08656/evidence51aa491+e4eeb0b,53cases956assertions, SPEC/QUALITY;
  `docs/parity/origin-loopback-proof/` and adjacent reviews. Actual CA/leaf, redirect
  positive controls, Origin/Peer races, retry/cancel, key isolation. No livehost claim.
- Source normalization:62b6f70/evidence0f48868, SPEC/QUALITY; four formats,
  embedded Markdown, Choose/dedup, actual same-guide Copy and containment.
  `docs/parity/source-normalization-proof/` and adjacent reviews. C# Copy required
  skip-validation because patcher prerequisites were absent; not validation parity.
- FOMOD S1: SPEC approved only. CLI policies, exact captured consumer scope,
  cancellable Unix prompts, scoped atomic recording. Windows native input exists
  but native runtime and cross-compilation remain unverified. Recovery-aware GUI,
  S2 reconfiguration/Unicode/stale-choice/interchange and install bytes remain open.

# Remaining implementation map

All plans below are under `docs/parity/`, with their independent review beside them.
One production implementer/build owner at a time; read-only planning may run beside
it. Fresh implementer per bounded task, independent SPEC then independent QUALITY,
fix/re-review loops. No local skill package is needed to follow this workflow.

1. Complete FOMOD QUALITY and any corrections first.
2. Controller's proposed next bounded fix is Nexus H1a from `host-providers-plan.md`:
   both NexusClient constructors must use one bounded same-origin redirect policy;
   production HTTPS preserved, explicit loopback HTTP fixture exception only.
   Current default client uses default redirects with custom apikey; no observed
   live leak, but source lacks confinement. Encode nxm key through URL query
   construction rather than raw interpolation. Own updates.rs plus generic proof;
   preserve public APIs and separate credential-free archive client. Controlled
   same-origin success, loop limit, cross-host/port/downgrade zero destination
   requests with positive controls, exact reserved-key decoding, and archive
   zero-apikey proof. This task is prepared conceptually here, NOT dispatched.
3. `fomod-remaining-plan.md`: S2/S3 semantics/reconfiguration/interchange/install/
   native corpus. Narrow post-dispatch record dependency already landed inS1.
4. `downloads-gui-plan.md`: real wizard/panel/settings shared service, context-bound
   results, stop/join, isolated private UI proof; initialize explicit settings/key
   context before defaults/protocol forwarding. No synthetic GUI messages.
5. `persistent-sharing-plan.md`: app-lifetime owner, durable CLI admission, fair
   queue, active-time/upload budgets, stop/install/restore/exit ownership. GlobalOff
   cancels acquisition+sharing; no128-lifetime-cap or manual re-fetch workaround.
6. `host-providers-plan.md`: remaining Nexus selection/multi-file discovery,
   anonymous DeadlyStream, actual MEGA decryption and metadata. GameFront is
   manual-only in reference. Settle per-file hashes before multiple-file expansion.
7. `markdown-remaining-plan.md`: M1export with DraftReview, M2legacy bullets with
   explicit safe normalization, M3actual full.md pipeline/coverage.
8. `checkpoint-plan.md` and `checkpoint-source-audit.md`: immutable per-component
   manifests, durable intent/HEAD, provenance/root binding, verified before-N
   restore/replay, resume and native history. Do not copy C# disabled-restore false
   success or invent mandatory C# store migration.
9. `engine-archive-patcher-remaining-plan.md`: E1missing/stale/error-log failclosed;
   E2limited InstallList VFS effects with no process/root mutation during validate;
   E3external action arguments/multiple sources and residual action matrix;
   E4cleanup final sets; E5archive timestamps/corpus/SFX safety; E6packaged/Python
   discovery; E7real decoded patch resources/platforms. All action branches exist;
   partial rows do not justify rewriting the engine.
10. `remaining-scope-map.md` also retains remaining format/error/native selection,
    paste/editor/conflicts/update/profile UI, wizard, CLI, diagnostics and packaging.
    End withF29: actual isolated C#/Rust/manual full.md lanes, same inputs/selections,
    ordered per-step resource comparison, final content and live-game evidence.

# Proof/operation constraints

- No Rust tests or custom/temporary Rust harnesses created/modified/compiled/run
  until full parity; no all-targets. Production lib/bin fmt/build/Clippy and actual
  CLI/private native UI/PTY fixtures are allowed. AGENTS and CI now state this
  explicitly. Historical51entry freeze included AGENTS; its documentation change
  for cloud publication is intentional, not an assertion of unchanged hash.
- No fake Acquire, reflection oracle or fabricated success/message injection.
  Existing source/library presence or compilation is not runtime/full parity.
- Network fixture children share an isolated namespace with public discovery off.
  No API credentials on archive clients. No CAPTCHA/login/payment/wait bypass,
  including via Peer. Generic fixture names only in this Rust repository.
- Preserve authored selection and completed context, unrelated metadata and guide
  order. If stepN fails: stop, verify restore beforeN, fix, replayN onward. Never
  append an early failed step at the end. Missing optional delete targets are no-op.
- Real published names/raw guide outputs stay outside this Rust repo. Manual guides
  are content/k1/full.md and content/k2/full.md, not stale prebuilt TOMLs. Reuse
  already-present archives; full-install runs need legitimate supplied inputs.
- Do not publish PRs/releases or claim full parity from this pushed checkpoint.

# Historical full-guide baseline caveat

`docs/parity/full-markdown-baseline/` retains anonymous comparison, not raw private
outputs. K1had186roots/72draft instructions; K2had145roots/56. Fields matched after
GUID mapping and explicit serializer omission treatment; C# Delete Overwrite
omission versus true model default remains distinct from false draft intent.
Rust-only DraftReview/unresolved masters remain explicit. Equal counts are not
complete prose coverage or installation. Crucially, oldK2input was locally edited;
cloud bootstrap provides a public clean pin and its different hash. Generate new
bilateral evidence from that pin; old raw outputs are not a continuation dependency.

# Resume semantics

This is a status snapshot, not automatic action authority. When invoked through
ce-handoff resume, orient from this document and current repository state, then
wait for current-user confirmation according to that skill. Once execution is
confirmed, the prior requested workflow is continuous bounded implementation and
review, not repeated permission checks between already-authorized tasks.
