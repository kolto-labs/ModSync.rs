# Shared diagnostics integration

Working-source continuation, 2026-10-02. The later changes described here are
outside the last recorded production build boundary. No test, runtime transport,
filesystem acceptance, native interaction or CI run establishes these changes.

## Ownership and terminal results

`cli_session.rs` initializes typed settings, local logging and configured telemetry
around parsed CLI dispatch. Explicit transport probes retain their isolated
settings boundary. Typed exit-code errors return through session completion;
JSON report failures do not acquire a second error payload. Log mirroring remains
off for CLI output. Session completion runs once through explicit completion or
the drop fallback.

The desktop owns `telemetry::lifecycle::Lifecycle`. Configuration is coalesced;
revocation closes admission immediately and the matching completion receipt
confirms exporter shutdown. The controller also accepts whole local-log batches
without disk I/O on the caller. Admission uses `try_lock`, at most 512 lines and
1 MiB of queued UTF-8, rejecting rather than truncating excess. Configuration and
shutdown take priority; all accepted log lines drain before the stopped receipt.
The UI retains its original log history and retries rejected queue admission.

Exporter shutdown can still wait for the operating-system resolver. The UI waits
for its owner off the repaint thread and does not claim stopped before the receipt.

## Automatic operation records

| Production boundary | Record semantics |
| --- | --- |
| External guide parse/load parsing stage | One parsing result; internal autodetection attempts and fenced subdocuments do not produce duplicate parse records |
| Final staged validation report | Success, error plus warning count, and elapsed time; early environment refusal is included |
| Actual engine component execution | Component outcome and authored instruction count; nested selected options are component executions |
| Top-level whole-run installation component | Mod installation result after actual component execution |
| Top-level checkpoint installation component | Mod installation success only after component checkpoint commit; preflight/commit errors produce failure |
| Actual file write/delete attempt | File operation result; simulation and skipped overwrite do not emit file-operation success |
| HTTPS archive fetch | One terminal transfer record per fetch attempt, with observed transfer bytes |
| MEGA decrypted transfer | One terminal transfer record per transfer attempt, with observed bytes and fixed provider category |

Component and archive names, error messages and panic locations pass through
the shared hashing recorders. Format labels and provider categories come from
fixed categories; raw signed URLs, authentication keys and filesystem paths are
not added to telemetry attributes. These metrics describe their named boundary,
not a claim of final installation durability or real-game equivalence. A later
run finalization failure can follow previously completed component records.

## Panic and privacy corrections

The panic hook installs once per process. It uses a best-effort `try_lock` for
local history/existing log output and a separate `try_record_panic` path for
bounded hashed enqueue. It never re-enters the ordinary logging/exporter mutex
path or waits for a telemetry flush while the panicking thread may hold those
locks. The previous panic hook remains chained; normal lifecycle shutdown owns
flush and join. A crash record can be lost when the lock or queue is unavailable.

Configuration and signing-key reads use bounded regular-file intake. Endpoint
diagnostics remove user information, query and fragment. Signed OTLP transport
rejects those endpoint forms and disables redirects. The legacy export helper
also rejects redirects; desktop callers use the owned recorder instead.

Independent source review checked simulation exclusion, checkpoint ordering,
outer-only parsing, transport count boundaries, log admission/draining, and
panic-hook lock re-entry. The panic-hook finding was corrected in source.

Remaining acceptance includes configured authenticated transport, consent-off
traffic measurement, revocation during export/DNS, queue pressure, shutdown and
native UI behavior. Persistent peer cache metrics and exact reference metric
corpus comparison remain separate from the operation records above.
