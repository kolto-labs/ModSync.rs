# Implementer self-review

- Scope: only the three approved Rust files changed. Public existing entry points
  and RunSettings fields are unchanged; new state is private and per invocation.
- Trust: custom certificate is not used by Nexus API clients, ordinary archive
  clients, environment/settings, or the system store. The conditional client
  construction applies no_proxy and localhost pinning only with explicit CA.
- Redirects: initial and every hop are checked; HTTPS-only, literal loopback or
  exact localhost; ten-hop bound; forbidden redirects return policy error. No
  3xx HTML body is mistaken for a manual gate.
- Error disclosure: file-read/certificate errors use fixed text; override-only
  reqwest errors strip URLs. Ordinary diagnostics remain unchanged.
- Certificate parsing: locked reqwest 0.12.28 rustls from_pem defers validation;
  empty/non-PEM and DER validity now checked before returning configuration.
- Fixture controls: actual reachable TEST-NET hosts, independent certificate-
  verified HTTP(S) clients, malicious localhost resolution, separate CONNECT
  proxy, and real production torrent peer transfer prevent vacuous negatives.
- Gate and race: both modes prohibit peer work at a host gate; real measured
  Origin/Peer winners each publish once, join/cancel losers and remove staging.
- Cancel/retry: three-request bounds and prompt SIGINT cleanup checked. Existing
  last-failure status during retry-delay cancellation is disclosed in README.
- Isolation: all network children share one namespace, public list endpoints are
  disabled, and only fixture-owned children/directories are cleaned. No live
  game tree, archive store, baseline, release target, or user process is touched.
- Validation: production build/Clippy for default and no-default features, fmt,
  frozen input hashes, and actual CLI fixture only. No Rust tests or fake adapter.
- Remaining limitations: isolated synthetic traffic proves these transport and
  orchestration behaviors, not live-host/browser downloading, GUI wiring, full
  feature parity or persistent sharing. Independent reviews remain the acceptance
  authority.
