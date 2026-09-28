# Persistent sharing plan review

Fresh independent reviewer `fetch_spec_review` read existing source and acquisition/GUI contracts before the proposed plan. Initial verdict: correction required. Final corrected plan: **PLAN APPROVED**. No runtime or implementation acceptance.

Three concrete findings were resolved:

1. Admission-time 24h deadlines plus FIFO expired queued identities before sharing. Contract now persists cumulative active sharing duration and uploaded bytes, excludes waiting/offline time, and uses five-minute quanta with 60-second no-upload yielding. Real idle/active A plus receiving B acceptance must prove fairness.
2. Optional CLI admission while a lifetime owner lock was held lost successful eligibility. Mandatory idempotent pending records now use a separate short transaction lock; owner commits durably before removing pending work. Real concurrent CLI/desktop proof requires automatic sharing without another Fetch.
3. A fixed128 lifetime catalog cap had no recovery. The cap is removed; bounded in-memory pages cover durable pending/ready records, and compact terminal tombstones retain accounting. Acceptance includes more than one page, later attempts, restart/duplicates and storage-failure recovery.

Global Off remains accessible during work and cancels the whole acquisition plus finished sharing, awaiting joins. KeepSharing Off affects finished sharing only. Other settings retain next-run behavior. This expressly overrides downloads GUI G9 only as stated.

Reviewer verified that existing private-copy/proxy primitives support the design through planned additive observations and cancellation-aware preparation. The plan does not assume those new APIs exist already. No daemon, general IPC framework or new authoring feature is required. No synthetic messages, Rust harnesses or screenshot-only proof permitted. Source refs and exact production acceptance live in [the plan](persistent-sharing-plan.md).
