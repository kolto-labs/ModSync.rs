# CLI acquisition before processing

The binary's `cli_acquisition::before_process` adapter performs the explicit
acquisition prephase for an already selected guide. Callers supply the actual
loaded guide path separately from the mod download directory. Its `Options`
contains that optional guide path, an optional Nexus key override, worker count,
an optional FOMOD choices file, and completed-prefix IDs. Defaults use four workers. An explicitly
blank API key is an error; a nonblank override bypasses stored-key lookup.

## Shared execution and completion boundary

The adapter uses the production selection, acquisition planner, provider
discovery, transfer workers, archive verification, metadata observation,
resource recording, and FOMOD dispatcher. Existing eligible archives are
handled by the same planner and FOMOD inspection as Fetch Downloads. Stored
peer policy and the existing owned `PeerAcquire` implementation apply. This
adapter does not create a persistent sharing controller.

Every planned transfer must have exactly one successful terminal outcome with
a path. Invalid URLs, unsafe filenames, manual actions, failed transfers,
cancellation, failed FOMOD inspection, and unresolved FOMOD decisions prevent
processing from continuing. FOMOD dispatch is noninteractive and uses explicit
choices, the existing environment choices source, or saved policy/choices.
It does not automatically approve choices. Intentionally suppressed FOMOD
prompts retain the existing shared dispatch semantics.

All guide edits occur on a candidate clone. The caller's guide is replaced
only after acquisition, FOMOD configuration, recording, and the final
cancellation check succeed. A cancelled operation reports exit 130, including
cancellation observed when a shared service returns an error. The adapter
neither saves an instruction file nor creates an install snapshot. Downloads
already published before a later failure remain available for a subsequent
attempt; failure does not commit partial guide edits.

Provider metadata lookup failures and sharing catalog admission failures
remain advisory, matching the existing Fetch Downloads boundary. With durable
sharing enabled, verified completed or already-present archives are enrolled
using the actual loaded guide path. A stdin guide has no saved authority and
its admissions remain approval-held. Catalog errors are reported by count or
generic message without exposing URLs, keys, or provider response bodies.

## Scope and evidence

Main-command wiring owns whether this explicit prephase runs, the selected
component set, archive options, and cancellation signal lifetime. The adapter
uses no screen-resolution probe. It accepts 1–64 workers. An existing sharing
controller in another process is outside this adapter's lifecycle; later source
mutation requires the caller's applicable sharing/mutation policy.

This is production source implementation and static review scope. No download,
provider, FOMOD, native desktop, filesystem test, or CI acceptance was performed
for this slice. Production compilation is coordinated separately by the parent
agent after source freeze.

## Completed-prefix preservation

Remaining-phase callers pass their completed IDs. The adapter excludes those
components from acquisition and passes the same prefix to FOMOD inspection,
dispatch, and edit application. Full serialized definitions are captured before
processing and checked unchanged before sharing enrollment and guide publication.
Acquisition recording binds by component GUID plus registry URL; a shared URL
does not authorize edits to a completed consumer.

## Conversion authoring scope

`cli_acquisition::Options.include_unselected_authoring` defaults to false. Conversion
callers explicitly enable it to acquire all top-level resource owners, as the
reference conversion workflow does. Installation retains selected-only scope.

The adapter temporarily selects only top-level resource owners in its private
candidate clone. All top-level authored selection values and exact GUID spellings
are captured first; empty or duplicate case-equivalent identities fail before
acquisition. The full guide remains available for FOMOD consumer binding. Nested
selection values are not reset; intentional FOMOD option edits remain supported.

Before completed-prefix comparison, sharing enrollment, or guide publication, the
adapter verifies the same top-level identity inventory remains and restores every
original top-level selection. Missing/new/duplicate/changed identities reject the
candidate. An early no-resource return leaves the caller's guide untouched. These
are source guarantees, not runtime acceptance evidence; no tests or builds were
performed for this addition.

## Installation deadline and key policy

Install defaults to one acquisition worker; `--concurrent` selects four. The
`--nexus-api-key` alias accepts the explicit override, then stored settings, then the first
nonblank `KOTOR_MODSYNC_NEXUS_API_KEY` / `NEXUS_MODS_API_KEY`.
Transport probes retain their separate no-stored-settings policy.

The install acquisition deadline defaults to 48 hours; zero selects that default.
A timer owns a condition-variable wait, signals the operation cancellation token
on expiry, and is woken and joined before the adapter returns. The wrapper holds
guide edits in a private candidate until the timer is stopped and cancellation
checked. Expiry prints a generic time-limit diagnostic and returns cancellation
exit 130. Blocking operating-system calls may delay cooperative cancellation;
this is not a process-level hard kill or evidence of measured timeout behavior.
No timer, network, install, test, or runtime probe was executed for this source.

## Explicit FOMOD choices intake

The shared CLI policy resolver now uses `ChoicesFile::load` for both an explicit
choices path and the choices environment fallback. The shared regular-file,
no-follow, bounded reader replaces unbounded read_to_string. Skip still takes
precedence and choices resolution remains lazy after positive FOMOD inspection.
Acquisition composition requires zero unresolved consumers and no remaining
followups; skip or warn-continue cannot authorize an incomplete installation or
silently publish an incomplete conversion. Source change only; no runtime proof.

## Shared default-settings caller correction

Conversion/merge acquisition, filename-only generation, and installation acquisition
now share explicit → stored → environment Nexus key resolution. An explicitly blank
key remains an error. Implicit settings lookup passes `None` through to the shared
Store, preserving legacy fallback authority; an explicitly selected directory remains
isolated. Unrelated hosts do not trigger implicit key/settings lookup. Environment
values are read without mutation or logging, using the existing ordered environment
fallback helper. Stored-settings errors remain errors rather than silently selecting
another credential source.

Plain CLI install/validate/conflicts also load default stored patcher preferences,
without requiring `--settings-dir` or a profile flag. Explicit engine/executable
arguments retain precedence, and a stored executable is used only when its stored
engine matches the effective engine. Preference reading does not resolve or launch
a backend; the engine retains its existing lazy backend checks.

This correction is source-only; no tests, builds, network or runtime were performed.
