# Provider plan: independent feasibility review

**PLAN APPROVED** after one H1 correction. Fresh reviewer inspected actual Rust
classifier/planner/record/Nexus client/Peer gate and pinned C# handlers before plan
assessment. No edits, builds, tests, Rust harnesses, network or Git mutations.

The default Nexus constructor used reqwest default redirects while only the base-
override constructor confined origin. Requests attach custom apikey; reqwest's
sensitive-header stripping does not include that header. This is a source defect,
not evidence Nexus has redirected or exposed a key in a real request. H1 now
requires one bounded origin policy used by both constructors, production HTTPS,
forbidden redirect rejection before destination contact, separate credential-free
archive requests, and controlled same-origin/loop/cross-host/port/downgrade proof.
Source evidence must connect both constructors to the exercised common policy.

Other scope is grounded: C# supports Nexus category/wildcard discovery, anonymous
DeadlyStream session/CSRF/multiple-file flow and actual MEGA decrypting download.
GameFront is manual-only in the reference. Multiple-file identity/per-file hashes
must be settled before expansion; one resource hash cannot represent all siblings.
MEGA dependency choice, runtime expansion representation and metadata caller choices
remain explicit implementation prerequisites, not completed decisions or proof.

Confidence high in corrected scope; no F13 or live-provider acceptance.
[Approved plan](host-providers-plan.md).
