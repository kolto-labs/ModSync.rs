# Desktop completed-sharing integration

Source implementation only. Production compilation and independent review belong to the parent acceptance pass. No runtime, filesystem fixture, test, custom harness, or CI result is claimed here.

## Ownership and UI behavior

`ui/sharing.rs` owns the desktop controller and background admission, startup, and shutdown handles. Startup uses a controller that begins paused; the UI releases its mutation pause only after receiving ownership, so a Stop or Off request during startup remains effective. Normal closing cancels admission and the controller, polls completion, joins only finished background handles, and keeps the window visible until acquisition and sharing finish. A shutdown error keeps closing blocked and visible. An abnormal application teardown has a defensive synchronous Drop cleanup; it can block on underlying library or filesystem calls and is not a bounded-shutdown claim.

The always-visible completed-sharing panel presents the matching owner generation's actual state, candidate identity, uploaded bytes, and active time. Stop, Resume, Sharing Off, and KeepSharing Off remain available around acquisition. Resume is unavailable during install, restore, guide mutation, and FOMOD editing. Upload-cap text explicitly limits the aggregate claim to completed sharing; acquisition sessions have separate caps.

Sharing Off persists the setting and cancels the entire active acquisition plus completed sharing. KeepSharing Off affects only completed sharing. Settings writes are serialized, including Off requests received while a settings read or save is running. Resume and settings-directory changes remain gated until active and queued settings operations settle. A failed settings save remains visible and does not claim successful persistence.

## Accepted results and mutation barriers

An acquisition terminal retains its authoritative captured report and plan. Before accepted recording mutates guide metadata, an asynchronous sharing pause barrier completes. Only successfully recorded, context-matching results reach background enrollment. The verifier receives the captured guide, plan, report, archive options, and cancellation token. Deferred catalog records initially have no saved guide authorization. The UI accepts the returned receipt only for the same current context, then asks the owner to authorize the captured identities. Cancelled and stale receipts stay paused. Enrollment failure is reported separately from acquisition success.

Unsaved accepted records use session authorization. Successful guide/selection saves may promote known admitted identities through the owner's saved-file provenance checks; a failed save or authorization is visible. Promotion is only requested after a settled mutation pause. Admission identities are cleared when the catalog directory changes; saved promotion filters out historical identities that the current guide no longer authorizes, while malformed catalog data or saved-file mismatches for current identities remain errors. User stops, settings pauses, exhausted budgets, and unclean restart holds are not cleared by the automatic resume-after-mutation API.

Guide load, protocol guide replacement, profile/root controls, selection/editor controls, install, restore, and update checks wait for sharing cleanup. Successful engine mutations reauthorize the current guide before requesting a conditional mutation resume; each candidate still undergoes the owner's fresh hash, archive, and provenance checks before networking. Sharing completion never supplies an install receipt or validation result.

## Remaining acceptance

The production acceptance cases in `persistent-sharing-plan.md` remain required: native UI behavior at both viewport sizes, real receiving clients, proxy-negative and waiting states, bandwidth and cumulative-budget measurements, fairness over more than one page, restart and ownership collision, fault injection through actual production operations, and cleanup on cancellation/exit. Static control flow is not evidence that those runtime cases passed.
