# Desktop NXM registration preference

The desktop settings window now exposes native status, register, unregister, and
an independent disable-startup-preference action. Operations run on an owned
worker, with cancellation and a retained join handle. Settings-directory changes
and application close request cancellation and wait for worker settlement.

On Linux and Windows, a loaded true `registerNxmProtocolHandler` preference runs
one startup reconciliation per settings directory. Existing ownership is already
satisfied and incurs no mutation or settings rewrite. Absent registration uses
the shared owned registration service. Foreign handlers and other ModSync
packages/channels are preserved. macOS does not automatically register on load.

Explicit replacement requires confirmation of the displayed handler. The worker
rechecks it, then the shared `register_confirmed` operation checks the same
scheme/state/handler fingerprint under its mutation lock. Noncooperating native
OS tools remain outside that cooperative serialization boundary.

Unregister only removes owned registration. macOS additionally requires explicit
confirmation of whole-bundle scope because Launch Services unregisters the app,
including both `modsync` and `nxm`; the shared service checks both schemes remain
owned. The independent preference-only action disables startup reconciliation
without touching any native handler, including a foreign handler.

A successful native operation and its settings merge are separate receipts.
The adapter preserves the native report and exposes an incomplete result if
preference persistence fails; it never claims rollback. Full native reports are
retained in the desktop log even if the settings directory changes before the
result settles. Late cancellation cannot erase a completed operation's receipt.

`Store::save_nxm_registration_preference` merges only the case-insensitive
preference key under the existing shared settings lock. It retains unrelated
fields and uses the original explicit/implicit directory provenance, including
implicit legacy settings migration. The normal settings save controls and
profile-directory changes are gated by the same desktop busy state while the
protocol worker remains owned. Explicit transport probes do not launch these
operations.

Source implementation and independent review are separate from native operating
system acceptance. No registration, native probe, tests, or runtime interaction
was performed to implement this slice.
