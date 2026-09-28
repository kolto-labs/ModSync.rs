# Selected-guide fetch: independent quality review

Fresh read-only correctness reviewer inspected contract and source before receipts. Candidate `efefba1420ff7eb167f4de39a17db0acbce77b6e` versus `9ecf8e4`. **QUALITY APPROVED** after correction `2889ec5` and evidence `14783d2` (initial finding below).

## FETCH-QUALITY-01 — P2

Claim: `fetch-downloads --dry-run --progress-json` violates C5's standalone JSON per line contract.

Referent: `src/main.rs:1514-1516` unconditionally uses `serde_json::to_string_pretty(&plan)` and returns before checking `progress_json`. C4 requires the plan, but provides no exemption from C5.

Checked: source and accepted flag combination. A line containing only `{` cannot independently parse as JSON. No execution or mutation by reviewer.

Verdict: holds; high confidence. Emit a compact documented plan envelope for this combination; preserve ordinary pretty dry-run. Add actual production CLI acceptance checking independently parseable lines and no writes/network activity.

Controller ruling: use exactly one `{"type":"plan","plan":...}` envelope for the dry-run/progress combination; no acquisition events or run summary because acquisition did not run. Approved contract C5 now records this clarification. Fix assigned to original implementer; re-review pending.

The reviewer traced callback identity, premium validation, race grant/result consumption, staging/publication, lazy policy, metadata/name safety, cancellation, credential precedence, selection/recording and loader defaults. No other actionable source defect was found at sufficient confidence. HTTPS Origin runtime remains unproven until its separate slice; retained production gates were inspected, not rerun by reviewer.

## Correction re-review

The same independent quality reviewer inspected the correction diff before refreshed
evidence and approved it with no remaining findings. `main.rs:1514-1521` now selects
compact plan-envelope output only for progress mode, retains the ordinary pretty
expression, and returns before adapter construction/signals/run. Raw combined-flags
output is exactly one plan line; ordinary dry-run remains multiline. Retained
case/environment snapshots match and API/relay observations are empty.

Current `main.rs` independently hashed to
`e71dda24932aa0bef699d0581dd99e85c9eeeb14ce0e9b8a50af0b1efeea8346`, matching
source binding for `2889ec5032c7d4b2b3d4c4d5cee70b381900d536`. Binary binding is
`1ba0f02f7577114030d559ea66dd5bcca8f9c3c9c835f6119af77738da947e74`.
The reviewer inspected retained 312/312 and five passing production-gate receipts;
these were not executions by the reviewer. No broader parity claim follows.
