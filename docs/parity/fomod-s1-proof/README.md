# Fetch FOMOD S1 production evidence

Candidate source: `f3f27d346c9a07dd102d23ed8992f72d19f3c8d2`.
Independent source SPEC and QUALITY review are still required. This evidence does
not accept full F15, S2, the desktop acquisition flow, or any real game install.

The controller assigned the narrow S2 record-timing dependency to S1 in
`docs/parity/fomod-remaining-plan.md`. The implementation saves only accepted
GUID/archive edits after dispatch when `--record` is set. It preserves the
original guide's selection, unrelated fields, and acquisition facts. A failed
current session has no edit to replay; earlier accepted sessions can persist and
are listed separately. Without `--record`, the original guide stays byte-identical
and the summary includes accepted archive choices.

## Final measured results

- `receipts/receipt.json`: **319/319 checks**, **58 production Fetch cases**,
  plus public help and the existing standalone `fomod-inspect` route.
- `inherited-controller/receipt.json`: **41/41 inherited controller checks**.
  `inherited_controller.py` is the accepted controller probe with only its output
  directory redirected here. This reruns selected/all/file-selection, dry-run,
  malformed-settings bypass, Nexus-key requirement, peer-mode planning, recording,
  and FOMOD follow-up compatibility through the production executable.
- `build-receipt.json`: formatting, headless production build, headless strict
  Clippy, desktop production build, desktop strict Clippy all exited **0**.
  Commands specify only `--lib --bin modsync`, the existing debug target, and `-j2`.
- `freeze.json`: **51** frozen AGENTS/test file or inline-test-suffix entries are
  byte-identical to original `9ecf8e4`. Cargo manifest/lock changes are separately
  authorized: direct target-specific declarations for already-locked libc
  `0.2.189` and windows-sys `0.61.2`, with only two root dependency edges added.
  No Rust test or custom Rust harness was created, modified, compiled, or run.
- `binding.json` binds source commit, binary, production receipts, and gates.
  Every production source hash in the build/runtime receipts was also checked
  against the committed `f3f27d3` blob.

Final binary SHA-256:
`9314848a80c55c1bd06fdcbf4f18556ad3bbc2a30f820944c8cb90cd03f13195`.

## Exercised behavior

`probe.py` invokes the shipped executable in a disposable network namespace with
no routes and a loopback HTTPS server using a CA-signed server leaf. Generic
archives, guides, settings, HOME/XDG directories, scratch and output directories
are private fixtures. No live game, baseline, archive store, public peer list,
default desktop, or external download host is used. The receipt records the
namespace, empty route table, certificate commands, and origin request transcript.

The cases cover public flag discovery; explicit skip/choices/environment/settings/
TTY precedence; conflict rejection before acquisition; deferred settings reads;
malformed and missing choices; generic-guide and dry-run bypass; actual loopback
TLS acquisition and acquisition failure; redirected warning/skip behavior; valid,
missing and invalid saved choices; repeat Fetch after warning and configuration;
configured-first/pending-first and warned-first/pending-first shared archives;
preservation of unselected options, authored selection, other archives, and prior
valid configuration; atomic multi-consumer failure; mixed accepted/error sessions;
all five group types; required conflicts, explicit clear, back and visibility;
refusal, EOF, q, and serial archive prompting; and standalone choices capture.

Private PTY and live-pipe cases send SIGINT while waiting for the configure answer
and a numbered selection, without another input line. The pipe writer stays open.
They measure exit 130, final JSON, preserved uncommitted state, restored original
stdin flags, unchanged canonical terminal modes, and process termination without a
surviving reader. Additional cases exercise canonical erase, Ctrl-D, malformed and
split UTF-8, partial final pipe lines, automatic TTY prompting, and an accepted
first session followed by cancellation of the second.

Per-case JSON retains exact argv, synthetic mode environment, input text or byte
and signal actions, exit, raw stdout/stderr, and before/after hashes. The matching
`receipts/fixtures/<case>/{before,after}` directories retain actual guide, sidecar,
settings and archive bytes. Shared restart cases retain each invocation's state.

## Source boundaries and self-review

- `src/main.rs`: four Fetch flags, captured scan/dispatch, delayed scoped recording,
  additive summary, and consistent cancellation/error/unresolved/success exits.
  Cancellation is latched after recording/cleanup so JSON and exit describe the
  same completed-run boundary.
- `src/download_orchestrator.rs`: eligibility precedes inspection deduplication.
  The additive full assessment binds only exact selected top-level report GUIDs;
  it does not enroll options or unrelated components sharing an archive name.
  The existing public desktop follow-up wrapper preserves its recursive prompt
  policy and suppression behavior while fixing first-consumer deduplication.
- `src/fomod.rs`: shared typed prompt core with the old standalone wrapper intact;
  atomic bound archive application preserves other archives and rejects malformed
  metadata rather than panicking.
- `src/fomod_cli.rs`: lazy policy, independent per-consumer validity assessment,
  single shared archive session, typed outcomes, accepted edit ledger, and owned
  cancellation-aware terminal input.

Self-review corrected the legacy wrapper's behavior, the Windows raw-handle trait
import, Windows Ctrl-Z wakeup and aborted-console-read handling, and the final
cancellation summary/exit latch. The initial exploratory TLS fixture incorrectly
used a CA certificate as its server leaf; final receipts use the accepted
CA-plus-signed-leaf pattern and pass actual HTTPS acquisition. Earlier exploratory
results are not the final source/binary binding.

## Remaining boundaries

Windows input has a native implementation using ReadConsoleW/ReadFile, repeated
CancelSynchronousIo, and mandatory worker join. Its declarations and control-flow
were checked against the installed windows-sys and Rust std sources. **Windows was
not cross-compiled or run here.** Native console, redirected-pipe, cancellation,
EOF, encoding, and device-driver behavior still require Windows runtime acceptance.
Unexpected cancellation errors are retained; detached readers are not a fallback.

The shared assessment/application functions accept an excluded/completed GUID
list. Standalone Fetch has no existing recovery-completed input and supplies an
empty list. Actual recovery-aware desktop proof remains pending; a fake Rust
caller was not substituted. Desktop behavior has source preservation only here,
not new native GUI acceptance.

S1 preserves valid configured consumers, including when an explicit choices file
names them. Broader same-archive reconfiguration, semantic/Unicode corrections,
changed-archive invalidation, native editor comparison, engine install-byte corpus,
and serialized C# interoperability remain S2/S3 obligations. These receipts prove
bounded Fetch behavior and emitted selections, not full FOMOD semantics or installed
game bytes. F15 and F29 remain partial.

## Reproduction

Run production gates serially:

```bash
python3 docs/parity/fomod-s1-proof/gates.py
```

Then run the real CLI fixtures in isolated namespaces:

```bash
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-s1-proof/probe.py
unshare -Urn python3 docs/parity/fomod-s1-proof/inherited_controller.py
```

The target is intentionally fixed to
`/home/brunner56/.cache/modsync-rs-target-lead`; remeasure free space and respect the
single build owner before rerunning. No release build or test target is necessary.
