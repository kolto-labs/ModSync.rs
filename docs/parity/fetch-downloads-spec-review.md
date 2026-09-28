# Selected-guide fetch: independent specification review

Reviewed 2026-09-28 by fresh Codex subagent `fetch_spec_review`. **SPEC APPROVED** for source `efefba1420ff7eb167f4de39a17db0acbce77b6e`, evidence `54383a8`, against baseline `9ecf8e4`. Quality review remains separate.

The reviewer inspected the approved contract and actual source before the implementer receipt. No edits, builds, tests, custom Rust harnesses, network activity or Git mutations were performed by the reviewer.

| Contract | Evidence checked | Verdict and boundary |
|---|---|---|
| O1–O3 | Orchestrator callback identity rebinding, delayed/revocable Peer grant, racer joins/staging cleanup, terminal Nexus marker; four Nexus free/error Race/Fallback receipts | Holds. General HTTPS Origin runtime remains deferred under acceptance item 8. |
| P1–P4 | Concrete adapter metadata/name/infohash checks before lazy policy; exact-file versus directory destinations; caller archive options; typed cancellation; known/unknown/shared-pool/bad-metadata/budget/hash/cancel receipts | Holds for source and local fixtures. Once-only policy construction follows from source, not connection counts. |
| C1–C5 | Shared selectors, execution clone, effective settings, explicit Off, API override key isolation, dry-run ordering, preserved-guide recording, loader defaults and fenced Markdown propagation; direct CLI scenario outputs | Holds. Existing public load/parse defaults remain unchanged. |
| Acceptance 1–7 | Individual scenario argv/stdout/stderr/report files, exact bytes/hashes, filesystem snapshots and controller rerun | Adequate bounded evidence. No full-domain promotion. |
| Isolation and binding | Driver requires distinct namespace and empty route table; fixture children inherit it; all eight current source hashes and binary hash independently recomputed | Holds. Binary SHA256 `f7c7f2ba55ea9a59ab6968416aa86075e3980a6978a820214d7458cec5909bc3`. |
| Test freeze and gates | All 53 recorded files/test suffixes independently compared with `9ecf8e4`; raw production build/Clippy logs checked against receipt | Holds. No Rust tests created, modified, compiled or run. |

No concrete specification blocker or required source correction was found. Confidence is high within these source and fixture boundaries. The reviewer parsed individual outputs rather than treating the aggregate assertion count as coverage.

Evidence: [implementation receipt](fetch-downloads/RECEIPT.md), [source binding](fetch-downloads/source-binding.json), [acceptance receipt](fetch-downloads/acceptance/receipt.json), [controller rerun](fetch-controller-review/README.md), [approved contract](fetch-downloads-cli-plan.md).

HTTPS Origin runtime, persistent sharing, GUI lifecycle, advanced FOMOD interaction, complete host automation and full-build/live-game acceptance remain open. This review does not close F12–F15/F26 or any broader public domain.

## Correction spec re-review

**SPEC APPROVED** for correction `2889ec5032c7d4b2b3d4c4d5cee70b381900d536`
and refreshed evidence `14783d2`. Reviewer inspected the sole source diff first,
then independently parsed the combined-flags result: exactly one object with
`type` and `plan`, effective Fallback, exit 0, empty stderr, no events/run summary.
The branch returns before policy/signals/run/record/FOMOD; ordinary pretty output
is unchanged. All eight current and commit source hashes were recomputed and
matched, as did production binary
`1ba0f02f7577114030d559ea66dd5bcca8f9c3c9c835f6119af77738da947e74`.
Before/after case/HOME/XDG snapshots match; relay/API/event arrays are empty.
Fixture namespace `net:[4026532919]` had an empty route table. Previous bounded
approval carries forward; HTTPS Origin runtime remains separate.
