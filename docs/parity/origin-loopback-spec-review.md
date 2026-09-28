# Loopback HTTPS: independent specification review

**SPEC APPROVED** for source `6c08656` against accepted `2889ec5`, evidence
`51aa491` with build logs retained by `e4eeb0b`. Fresh reviewer read source and
approved plan before receipts. No edits/builds/tests/harness/network/Git mutations.

- Existing public fetch/run methods delegate with no override; RunSettings and
  Cargo dependencies are unchanged. Only three authorized production files changed.
- Initial and every redirect are HTTPS exactlocalhost/literal loopback, bounded
  with policy errors. Hostname/IP positive controls prove real reachability before
  refusal. Localhost pinning survives a malicious namespace-local hosts mapping.
- Absent/wrong CA fail with certificate errors; correct CA publishes exact bytes.
  Malformed PEM/DER is rejected before dry-run. Ordinary client CONNECT positive
  and override zero-CONNECT success independently establish proxy behavior.
- Eight login/challenge/wait gate scenarios in both modes show no Peer events or
  connections. Real Origin/Peer winners each show exact hash, one publication,
  classification before Peer connection, superseded loser and clean staging.
- Retry429 recovers on third request;503 stops afterthree. Body/backoff/race SIGINT
  returns130, report.cancelled=true, no publication/staging, in approximately
  0.60/0.46/1.58s. Existing backoff targetfailed status is retained and documented.
- Unsafe/unknown-name, hash, corrupt archive, budget, eventidentity, explicit-key
  isolation and dry-run no-work behaviors are present in raw cases.
- All source/proof/binary hashes independently recomputed and matched source
  commit. All94 frozen files matched baseline/current. Five production-only gates
  pass;53 cases/956 assertions/errornull, assessed by scenarios rather than tally.

Binary `e40e218304a3b1070fffa516ba60d74dd3128abb03798485316976e18732d82f`.
[Evidence and exact case mapping](origin-loopback-proof/README.md). Confidence high
within isolated fixtures. No live-host, GUI, persistent-sharing, fullF13/F14 orF29
acceptance. Independent quality review remains separate.
