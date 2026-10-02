# Desktop active profile binding

The profile-manager selection (`profile_name`) is now separate from the execution
binding (`active_profile_name`). Merely choosing, editing, or cloning a manager
entry does not redirect managed staging/manifests or change the execution context.
Checkpoint installation and deployment management capture the active binding.
The checkpoint panel displays it read-only; activation belongs to Install profiles.

Desktop preference load supplies the persisted active value. Successful profile
UI actions re-read that value and synchronize both the execution binding and the
typed settings model, including rename/delete. This prevents a later unrelated
settings save from restoring the old active profile. Existing profile helper
bodies remain unchanged; production UI wrappers own synchronization. A failure
reading the post-operation active state forces preferences to reload and gates
managed execution until that load settles.

Profile operations are disabled during settings load/save. Managed execution and
deployment operations also require preferences loaded for the current settings
root. The manager selector itself no longer participates in the install input
stamp. Explicit activation still applies profile selections and game/mod paths,
as the pinned reference ProfileManagerDialog does.

No automatic profile application on startup is claimed: the reference persisted
active setting and explicit profile activation are distinct operations. Existing
explicit staging/manifest override behavior remains owned by deployment resolution.

Evidence: source inspection, targeted formatting and diff checking only. No tests,
builds, live settings mutations, native execution or CI activity were performed.
