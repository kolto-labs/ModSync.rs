# Editor removal confirmation

Both the focused component controls' Delete key and the Remove selected button now open one confirmation dialog. This matches the pinned C# MainWindow route through `DeleteModWithConfirmationAsync` instead of submitting an immediate removal.

The pending request captures the component GUID, displayed name, complete guide identity and input context. A changed guide, missing target or changed context cancels the request. Confirmation rechecks the captured identity immediately before calling the existing shared editor transaction; dependency/reference validation remains authoritative. Cancel, Escape and dismissal leave the guide unchanged. Cancel receives initial focus, and dismissal returns logical focus to the component keyboard controls.

The existing editor busy, sharing, FOMOD and close gates still apply before presenting or committing the dialog. Explicit parent-enabled and desktop-settings/protocol busy checks cover the modal Area, which does not inherit its parent UI disabled state. Pending confirmation is held while unavailable and rechecked when the gates reopen. A pending request does not dispatch a new sharing continuation or bypass the existing edit gate. It does not modify files, run installation, or introduce a separate deletion implementation.

Source formatting and whitespace checks were performed. Native modal focus, keyboard behavior and accessibility remain acceptance work; no tests, builds, harnesses or runtime probes were run for this change.
