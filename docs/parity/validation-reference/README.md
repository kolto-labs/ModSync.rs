# Pinned C# native ValidatePage reference

This is a bounded native run of C# commit
`d31ccbd3bfcddb4705917eafa6483715c166de3b` on 2026-09-27. It does not establish
Rust parity. `receipt.json` binds source, built assemblies, runtime host, fixture,
screenshots and logs. The pinned checkout stayed clean. Build: 0 errors,
22 warnings; the build log includes pre-existing obsolete API and AngleSharp
NuGet audit warnings. No product source was edited.

The session used owned Xvfb `:2`, screen 1280x900, client 1280x800 at (0,0),
default Light appearance and a synthetic zero-byte `swkotor.exe`. The JSON guide
has one selected component and one Copy instruction. No download or install was
started. Native mouse input drove the active embedded wizard from Welcome to
Validation and, after a successful rerun, Ready to Install.

## Observed behavior

| Case | Errors / warnings / passed | Native observation |
|---|---|---|
| Source exists, bundled patcher present | 0 / 0 / 6 | Real 17% environment progress; Run disabled while running; completion collapses log; passing summary and stage cards available by scrolling. |
| Source `sample.tga` removed | 0 / 1 / 5 | **Warning-only**, not failure. Dry run skips Fixture because source archives/files are unavailable. Warning card has amber border and log opens. |
| Private build's HoloPatcher symlink removed | 1 / 0 / 0 | Environment failure, red Unknown card with missing-patcher message; log opens. Next is clickable but rejects advancement with a critical-errors Information modal. |
| Patcher restored through repository helper | 0 / 0 / 6 | Failure state replaced by clean summary and collapsed log. Next advances to Ready to Install. |

Clicking **Go to first issue** after manually collapsing the failed log visibly
opens it. Exact first-error line placement was not established: the active
host's outer scroller clips most log/result content at this client geometry.
The red and amber highlighted cards are separately captured. This is not full
independent-pane scroll, splitter, or accessibility proof.

`tree-before.json` equals `tree-after-clean.json` and `tree-after-all.json`.
The removed source was restored byte-for-byte between cases; the only patcher
change was to the owned temporary build symlink. No game output was created.
These are checkpoint file manifests, not a syscall-level nonmutation trace.

## Clipboard limitation

The actual Copy report button showed **Copied!**, but `xclip` and `xsel` could
not retrieve report bytes from the private X clipboard. A controlled xclip
marker roundtrip on `:2` worked. A second app run used `XDG_SESSION_TYPE=x11`,
no `WAYLAND_DISPLAY`, and its own `dbus-run-session`; report readback still
failed (`clipboard-readback.error`). The initial run inherited the user's DBus
address and Wayland session-type metadata despite rendering on private X11;
the second run corrected those environment variables.

There is deliberately no successful copied-report artifact. Button feedback
alone is not clipboard proof, and no reconstructed report is presented as a
copied report. Complete native stage/report text comparison remains open.

## Interaction record

1. Declined telemetry in initial private configuration; moved/resized owned
   main window and traversed Welcome, preamble, workspace, game, selection,
   downloads and validation with Next. No Fetch Downloads action.
2. Ran Copy validation; captured running environment, completion, scrolled cards
   and summary; tried Copy report and independent clipboard readback.
3. Removed only disposable source, reran, observed warning-only result and amber
   card; exercised first-issue control from a collapsed log.
4. Restarted only the owned app with private DBus and X11 session metadata.
   Re-entered the same disposable game path in its native picker when Next
   initially reported no selected installation despite displayed detection.
5. Repeated clean validation and Copy; preserved clipboard failure.
6. Removed only private build HoloPatcher symlink, ran failure, clicked Next and
   captured rejection modal. Dismissed modal, collapsed log, clicked first issue
   and captured expansion.
7. Restored patcher with repository helper, reran, captured clean summary and
   successful navigation to review. Captured final game/mod manifests.
8. Saved artifacts and terminated only verified owned app PID 1328857 and Xvfb
   PID 1279212. Both exited; private DBus wrapper exited. User :0/:94 sessions
   and their applications were not operated or terminated.

The build remains at
`/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync/debug_net9.0`.
The fixture remains under the adjacent `fixture/`. Use the same absolute path,
fixture bytes, client size and Light theme for the Rust comparison, with fresh
private settings and display ownership.

Other themes/platforms/DPI, minimum geometry, full builds, long issue lists,
conflicts/order/archive cases, cancellation, keyboard coverage, and external
accessibility behavior are outside this bounded evidence.
