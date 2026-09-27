# F18 managed deployment: C# audit and Rust gaps

Oracle: `/home/brunner56/.codex/worktrees/5813/ModSync` at `d31ccbd3`. C# paths below are relative to `src/`.
Rust: `src/deployment.rs` and the `deployment` CLI arm in `src/main.rs` (`DeploymentRoots` at line 245, `DeploymentAction` at 271, dispatch at 474).
Row: `docs/parity/ACCEPTANCE.md` F18 (partial). Notes: `COMPATIBILITY.md` "Managed deployment" row and the "Managed deployment manifests" section.

## 1. What C# does

### Deployment service (`ModSync.Core/Services/Deployment/DeploymentService.cs`)

| Behavior | C# file:line |
|---|---|
| Roots: `<manifestRoot>/manifests` and `<manifestRoot>/backups`, created on construction | `DeploymentService.cs:42-67` |
| Deploy walks every staged file. The walk follows directory symlinks, and each file's destination is built with `Path.Combine(game, rel)`. | `:115-120` |
| Conflict warning when another manifest already owns the path (case-insensitive index) | `:104`, `:122-126`, `:523-543` |
| An existing destination is moved into `backups/<guid>/<rel>`. **Any existing backup at that path is deleted first.** | `:141-158` |
| Hardlink first, then `File.Copy(overwrite: false)` as the fallback | `:160-168`, `HardLinkHelper.cs:27-56` |
| The manifest is saved only after every file is placed. There is no rollback on a mid-loop exception. | `:170-173` |
| `RecordLiveGameFilesAsync`: patcher provenance. It records changed game files with `OverwroteExisting=false`. The comment admits it does not restore pre-patcher bytes. | `:184-270` (comment `:188-189`) |
| `CaptureGameFileHashIndexAsync` / `DiffGameFileHashIndexAsync` | `:277-329` |
| Uninstall deletes a file only when its hash matches, ignoring case. A mismatched file is skipped and its backup stays. The backup is restored only if `OverwroteExisting` is set and the file was removed. | `:338-432` |
| Purge: newest `DeployedUtc` first | `:437-450` |
| Unreadable manifests are skipped with a warning | `:465-481`, `:496-504` |
| Atomic manifest save (temp file, then delete and move) | `:545-561` |
| Confinement: rejects rooted paths and `..`; prefix check ignores case | `:641-676` |
| Empty directories pruned upward in the game tree; backup tree pruned recursively | `:582-627` |
| Manifest shape: `ComponentGuid` (Guid, `D` format), `DeployedUtc`, `DeploymentMethod` 0/1 | `DeploymentManifest.cs:17-96` |

### Managed install session (`ModSync.Core/Services/Installation/ManagedInstallSession.cs`)

| Behavior | C# file:line |
|---|---|
| Roots per profile: `profiles/<sanitized>/staging` and `profiles/<sanitized>/deployment` | `ManagedInstallSession.cs:152-156`, `Services/Profiles/ProfileService.cs:103-107` |
| Managed mode requires an active or overriding profile that exists on disk | `:131-147`, `ManagedInstallCliOverrides.cs:24-61` |
| Only Extract, Move, Copy and Rename are staged | `:175-181` |
| Redirect: sources under the game directory are remapped into staging. So are destinations. | `:294-355` |
| Patcher steps are not redirected. They are only recorded for provenance. | `:301-305` |
| The per-component game hash index is captured before *every* component in managed mode | `ModComponent.cs:858-865` |
| Staged tree deployed after a successful component, then patcher provenance recorded | `ModComponent.cs:885-895`, `:391-436`, `:222-292` |
| Hook call sites: Delete and DelDuplicate call it too, but `ShouldStageAction` makes it a no-op for them | `ModComponent.cs:966-969`, `:1586-1628` |
| Session lifetime and `LastManagedInstallResult` | `Services/InstallationService.cs:40-86` |

### Profile switching and GUI

- **Profile switching.** C# has no switch operation. Activating a profile only changes `ActiveProfileName`, so the next session resolves a different artifact directory.
- **Deleting the active profile.** The confirmation says linked deployments are "not purged automatically" (`ModSync.GUI/Dialogs/ProfileManagerDialog.axaml.cs:224`).
- **GUI actions.** These cover status, purge, per-component uninstall and the deployed badge:
  - `ModSync.GUI/Services/ManagedDeploymentActions.cs:46-172`
  - `MenuBuilderService.cs:98-120`, `:419-470`
  - `ModListItem.axaml.cs:550-555`
- **Install guard.** Purge and uninstall are refused while an install is running (`ManagedDeploymentActions.cs:49-55`, `:81-86`).

## 2. What Rust had at `ac9cf3a`

`src/deployment.rs` had 260 lines. It supported deploy, uninstall and purge with the C# manifest shape, and the CLI exposed status, deploy, uninstall and purge. Gaps found against the oracle and the filesystem:

## 3. Ranked gaps and what this lane did

| Rank | Gap | Evidence | Status |
|---|---|---|---|
| 1 | **Engine staging redirection.** Normal installs never stage, so managed mode does not exist for real installs. | `COMPATIBILITY.md` row 89. `src/engine.rs` has no staging hook. | **Open (engine.rs owner).** Design in section 4. |
| 2 | **Redeploy destroys the original backup.** C# deletes the existing backup and replaces it with its own hardlink. | Live C# probe D3: after two managed installs of one component the backup held `CSNEW` and purge left `CSNEW`. The original `ORIG` was lost. | Fixed. Rust refuses to redeploy a component that has a manifest. A stale backup left by a skipped uninstall also blocks deploy before anything changes. |
| 3 | **Partial deploy leaves files nobody owns.** Rust rejected legal filenames (`[`, `]`, trailing dot) partway through the loop. Any I/O error after earlier files were linked left them live, left backups moved, and wrote no manifest. C# has the same no-rollback shape. | Old `confined()` used `paths::relative(rel, false)` | Fixed. The whole staged tree is preflighted before the game directory changes. Deploy-path rules follow C# (relative, no `..`, no NUL, no drive on Windows). Failures roll back and no manifest is written (probes P7, P10). |
| 4 | **Symlink escape.** `exists()` and `fs::copy` followed symlinks, so a symlinked `Override` would write outside the game tree. C# behaves the same. | Probe P8 | Fixed. Symlinked or non-directory ancestors and non-file targets are refused. Staged symlinks are refused, matching `engine::scan`. Copy fallback uses `create_new`. |
| 5 | **Patcher provenance missing.** | C# `:184-329` | Ported: `capture_index`, `diff_index`, `record_live`. One deliberate difference: a pre-existing file changed by the patcher is recorded only when a prior copy with the old hash is supplied. That copy becomes its backup. C# records such a file without a backup, so its uninstall deletes, for example, `dialog.tlk`. CLI: `deployment snapshot` and `deployment record` (P15). |
| 6 | **Out-of-order uninstall orphans the original.** Uninstall A while a later B still overwrites the same path. A's file hash mismatches, so it is skipped, and A's backup holding the original is abandoned. B's uninstall later restores A's bytes, which nobody owns. C# behaves the same. | Probe P6 | Fixed (deliberate difference). If the newest manifest deployed *after* A has a backup whose hash equals A's deployed hash, A's original moves into that backup slot (or that entry's backup is dropped when A displaced nothing). Reported as `transferred`. |
| 7 | **Profile roots and switching.** | C# `ManagedInstallSession.cs:152-156` | `Service::for_profile` and `Service::resolve`. The CLI accepts `--profile`/`--settings-dir`, or uses the active profile, with the C# missing-profile message. `deployment switch --to <p> [--purge-current]` is new behavior. It optionally purges the active profile's deployments, then activates the target through the existing `profiles::Store` API and reports its staged-but-undeployed components. It does not redeploy them, because install order lives in the install run, not the staging tree (P14). Without `--purge-current`, it warns when the current profile still has deployments in that game directory (P17). |
| 8 | **One corrupt manifest blocked status and purge.** C# skips it with a warning. | Probe P12 | Fixed: `deployed_with_warnings`; the CLI prints warnings on stderr. |
| 9 | **Backup tree not pruned recursively.** An empty `backups/<guid>/Override/` stayed behind. | Old `prune(&backups, Some(&backup))` | Fixed: `remove_empty_tree`, the equivalent of C# `DeleteDirectoryIfEmptyRecursive` (P2). |
| 10 | **Hash comparison was case-sensitive, and backups were restored without checking `OverwroteExisting`.** | C# `:370`, `:387` | Fixed. |
| 11 | **GUID casing.** C# names manifests `Guid.ToString("D")`. Rust used the raw ID. | Probes P13, P16 | GUID-shaped IDs are unbraced and lowercased for the manifest name and `ComponentGuid`. Manifest and backup lookups fall back to a case-insensitive filename match, so manifests written by the earlier CLI under an uppercase name are still found by uninstall, purge and the redeploy guard (P16). The staging lookup falls back to the raw name. |
| 12 | **DeployedUtc precision.** chrono wrote nine fractional digits. | C# manifest shows `2026-09-27T17:01:49.3313133Z` | Now written with seven digits plus `Z`. Reading accepts RFC 3339 or offset-less text. |
| 13 | **Case-aware destinations on case-sensitive filesystems.** `override/a.tga` created a second `override/` next to `Override/`. C# on Linux does the same. | Probe P11 | Fixed (Linux-only difference). Existing case variants are reused. The manifest records the on-disk path. Case-colliding entries are refused. |
| 14 | Conflict (ownership) warnings | C# `:122-126` | Ported. They go to stderr, and deploy stdout stays manifest-shaped. |
| 15 | GUI management: status indicator, per-component uninstall, purge, deployed badge, install-in-progress guard | `ManagedDeploymentActions.cs` | Open. `ui.rs` is out of scope for this lane. The library now returns the reports a GUI needs. |
| 16 | **Two live profiles over one game directory.** The ownership handoff (rank 6) and conflict warnings search only one profile's manifest root. Manifests do not record which game directory they target. Out-of-order uninstalls *across* profiles can therefore still orphan an original backup, as in C#. | Code reading; P14 covers only the purge path | Open. The switch warning (P17) is the only guard. A full fix needs a game-directory field or a shared ownership index, which changes the C# manifest shape. |
| 17 | In-place edit of a hardlinked deployed file also edits the staged copy | Hardlink semantics, in both implementations | Documented. Uninstall still detects the edit because the manifest hash was taken at deploy time. The staged tree is then no longer pristine. |

### Not changed on purpose

- Uninstall stays hash-guarded and never deletes user-modified bytes, as in C#.
- Purge order stays newest `DeployedUtc` first.
- The manifest JSON shape stays C#-compatible. It is verified in both directions (section 5).
- `Service::{new, deploy, uninstall, purge, deployed, manifest}` and the `Entry`/`Manifest` fields keep their signatures and types. Whether `tests/deployment.rs` still compiles is **unverified**: the gates build `--lib --bins` only, and the test freeze ruled out compiling tests.

## 4. Engine hook design (for the `src/engine.rs` owner)

C# redirects resolved instruction paths per instruction (`ManagedInstallSession.cs:294-355`). The Rust engine is different: every write goes through one virtual file map (`Engine::files`/`directories`, keyed by lowercase path) and three choke points: `actual()`, `put()` and `delete()` (`engine.rs:1238-1313`). Redirecting there instead of per instruction keeps these working unchanged:
- stateful Copy→Rename→Move chains
- case-aware matching
- FOMOD extraction
- sandbox checks

### Hook surface

```rust
// engine.rs, Settings
pub managed: Option<ManagedDeployment>,

pub struct ManagedDeployment {
    pub service: Arc<crate::deployment::Service>, // game root must equal settings.roots.game
}

// Engine
staged: BTreeMap<String /*logical game key*/, PathBuf /*physical staged path*/>,
current_component: Option<String /*normalized guid*/>,
```

### Rules

1. **Logical keys stay game paths.** `files` keeps keys under `roots.game`, so later instructions and components resolve sources the same way in both modes. For a key whose bytes live in staging, `Content::Disk(p)` points at the staged path.
2. **`put(path, …)` for a path under `roots.game`, in managed mode, during Extract/Copy/Move/Rename** (C# `ShouldStageAction`):
   - write to `staging/<guid>/<rel>` instead, with `rel` from `strip_prefix(roots.game)` using the case chosen by `actual()`
   - record `staged[key] = physical`

   `roots.check()` still runs on the logical path. It also runs on the physical path, against a staging root added to `Roots`.
3. **`delete(path)` of a key in `staged`** removes the staged file only. Delete, DelDuplicate and CleanList on non-staged keys still act on the live game directory. C# does not track them either (its hook is a no-op for them). Record them as untracked live edits in the run receipt.
4. **Sources follow staged outputs.** C#'s "follow prior staged outputs" (`:314-340`) is automatic here, because `Content` already points at the staged file. The C# redirect has a flaw: it remaps *every* game-path source into staging, so Move/Rename of a pre-existing live game file fails in managed mode. Do not copy that. Unstaged keys keep reading the live file.
5. **Patcher/Execute is a barrier.** C# runs the patcher against the live game directory and deploys the staged tree *afterwards*. A patcher step that follows a staged Copy into `Override` therefore patches the old bytes, and the later deploy backs up the patched file. Before each Patcher/Execute/Run step inside a managed component:
   - `service.deploy_report(guid, name)` the tree staged so far (skip it if the tree is empty)
   - then `capture_index()` just before the process runs (C# hashes the whole game tree before every component, which is costly on a 14 GB install)
   - after the process, `record_live(guid, name, &before, Some(recovery_snapshot_game_dir))`
   - clear `staged` for that component and call `scan()`

   `recovery::begin` already copies the game tree to `state_dir/snapshot/game` (`recovery.rs:217-240`). That copy is the `backup_source` that lets provenance restore pre-patcher bytes.

   A second deploy for the same component after a barrier needs one of two things:
   - a per-barrier sub-ID (`<guid>` for the first segment, then `<guid>-2`, and so on), or
   - an append mode on `Service`

   Use `Service::deploy_append(component, name)` for each barrier and for the end of the component (section 6). The staged tree keeps its earlier files, and `deploy_append` skips the ones already deployed unchanged.
6. **End of component.** If `staged` is non-empty, call `deploy_report`, then `scan()`. The deployed hardlinks become ordinary game keys for later components. Conflict-tracker writes are recorded by logical game path (`put` already does this), so conflict analysis is identical in classic and managed mode.
7. **FOMOD.** Extract and Choose resolve member paths before `put`, so FOMOD output is staged by rule 2 without FOMOD-specific code. The FOMOD fail-closed gates run first, as in C#.
8. **Recovery.** `recovery::begin` snapshots only the game and mod directories. A failed managed run restored from that snapshot would leave stale manifests and backups behind. Add the profile artifact directory to the snapshot, or journal the manifest IDs written during the run and uninstall them in reverse on restore. The journal is smaller and composes with purge.
9. **Reinstall of a deployed component.** `deploy_report` refuses when the component already has a manifest (rank 2). Before running a managed component, the engine calls `Service::prepare_reinstall(guid)` (section 6). It fails closed without changing anything when an owned file would be skipped. Otherwise it uninstalls and clears the component's staging tree. Do not bypass the guard.
10. **Recovery state is outside the game tree.** `recovery::state_dir` is a sibling of the game directory (`.modsync-<hash>` next to it, `recovery.rs:23-33`). So `capture_index` and `record_live` never see the snapshot, pending or completion files. Keep it that way. If the state directory ever moves under the game root, exclude it from the index.
11. **Validation and dry-run** stay virtual. With `install == false`, rules 2-6 only update `staged` and the conflict tracker. Nothing touches staging or the manifest root.
12. **Parity receipt.** Record `ManagedInstallResult` fields (`ManagedModeUsed`, `ManifestsWritten`, `ActiveProfileName`, `PatcherComponentNames`, `PatcherProvenanceRecorded`) for the completion page.

## 5. Verification

The real CLI binary was run: `target-dep/debug/modsync deployment …` (no-default-features build) against disposable trees in `/tmp/depprobe/run/*`. Script: `docs/parity/deployment-probe.sh`. Result: **45 pass, 0 fail**. Manifest notation: `path+bk` means a backup exists, `/hl` means hardlink, `/cp` means copy.

| Probe | Before manifests | After manifests | Result |
|---|---|---|---|
| P1 deploy into empty target, then uninstall | (none) | `aaaaaaaa[Override/a.tga/hl]`, then (none) | same inode as the staged file; DeployedUtc has 7 digits; `Override/` pruned |
| P2 deploy over a pre-existing file | (none) | `aaaaaaaa[Override/a.tga+bk/hl]`, then (none) | `ORIG` restored; `backups/<guid>` fully removed |
| P3 same component deployed twice | `aaaaaaaa[Override/a.tga+bk/hl]` | unchanged | second deploy exits 1 ("already deployed"); backup still `ORIG`; uninstall restores `ORIG` |
| P4 user-replaced deployed file | `aaaaaaaa[a.tga+bk, b.tga]` | (none) | report `deleted=[b.tga] restored=[] skipped=[a.tga]`; `USER` kept; backup `ORIG` retained |
| P5 purge order across 3 components | `aaaaaaaa[x+bk] bbbbbbbb[x+bk] cccccccc[c.txt, x+bk]` | (none) | order `c, b, a`; `ORIG` restored; 2 conflict warnings |
| P6 uninstall A while B still overwrites | `aaaaaaaa[x+bk] bbbbbbbb[x+bk]` (B backup=`A`) | `bbbbbbbb[x+bk]` (B backup=`ORIG`) | `transferred=[Override/x.2da]`; after B's uninstall: `ORIG`, no orphan backups |
| P7 `Movies/intro[1].bik`, `Override/odd.` | (none) | `aaaaaaaa[Movies/intro[1].bik/hl, Override/odd./hl]` | deployed and removed |
| P8 symlinked `Override` | (none) | (none) | exit 1 "Refusing to deploy through a symlink"; nothing outside; no partial deploy |
| P9 staged symlink | (none) | (none) | exit 1; game untouched |
| P10 unwritable subdirectory mid-deploy | (none) | (none) | exit 1 "...earlier files were rolled back"; `ORIG` back; no manifest or backups |
| P11 staged `override/a.tga` vs game `Override/A.TGA` | (none) | `aaaaaaaa[Override/A.TGA+bk/hl]` | no second directory; uninstall restores |
| P12 corrupt manifest present | `aaaaaaaa[...] dddddddd…json:CORRUPT` | `dddddddd…json:CORRUPT` | status and purge exit 0 with a stderr warning; good component purged |
| P13 `{AAAAAAAA-…}` then `AAAAAAAA-…` | (none) | `aaaaaaaa….json` | lowercase manifest name; uppercase uninstall finds it |
| P14 profiles One and Two, one game dir | One: `aaaaaaaa[x+bk]` | One: (none); Two: `bbbbbbbb[x+bk]` | `switch --to Two --purge-current` restored `ORIG`; the active profile resolves roots; `purge --profile Two` gives `ORIG` |
| P16 legacy `AAAAAAAA-….json` and `backups/AAAAAAAA-…/` (earlier CLI naming) | `AAAAAAAA-…[Override/a.tga+bk]` | (none) | redeploy refused; purge restores `ORIG` and removes the backup directory |
| P17 `switch --to Two` with One still deployed, no purge | One: `aaaaaaaa[x.txt]` | unchanged | stderr warning "still has 1 deployment(s)"; files stay |
| P15 snapshot, "patcher" edits `dialog.tlk` and adds `new.2da`, record with `--backup-source` | (none) | `aaaaaaaa[Override/new.2da/cp, dialog.tlk+bk/cp]` | uninstall restores `DIALOG0` and removes `new.2da`; without a backup source the changed `dialog.tlk` is skipped |

### C# differential (built DLL, nothing compiled)

`dotnet …/ModSync.GUI/bin/Debug/net10.0/ModSync.Core.dll install --managed --profile One -y --skip-validation --no-checkpoint` was run on the host with `XDG_CONFIG_HOME=/tmp/csprobe/cfg`. Script: `docs/parity/deployment-csharp-differential.sh`. Rust's `profile --action create` wrote the profile. The DLL is a Sep 15 build from the main ModSync checkout, not the pinned oracle. Its current `DeploymentService.cs` and `ManagedInstallSession.cs` sources are byte-identical to `d31ccbd3`, but whether the DLL was built from exactly those sources is not proven. The evidence is the observed behavior below.

| Probe | Result |
|---|---|
| D1 C# writes, Rust reads and uninstalls | C# staged into `profiles/One/staging/<guid>/Override` and deployed 2 hardlinks, 1 overwrite. Manifest: `DeployedUtc "…49.3313133Z"`, `BackupRelativePath: null`. Rust `status --profile One` parsed it. Rust `uninstall --component AAAAAAAA-…-00A` gave `deleted=[x.2da, added.tga] restored=[x.2da]`, and the live file is `ORIG`. |
| D2 mixed writers, Rust purge | Rust deployed `RustB` into the same profile, then C# installed A over it. C# logged `Conflict: 'Override/x.2da' … already deployed by component 'RustB'`, so C# parsed the Rust-written manifest. It backed up the Rust hardlink. Rust purge order was A (C#), then B (Rust), and the live file is `ORIG` with no leftovers. |
| D3 C# redeploy of the same component | Run 1: backup=`ORIG`. Run 2: backup=`CSNEW` (C# `DeploymentService.cs:150-155`). Purge leaves `CSNEW`, so the original is lost. Rust refuses this case (P3). |

Not covered: a C# purge or uninstall of a Rust-written manifest, because C# exposes no CLI verb for it (GUI only). Windows and macOS filesystem behavior was not checked.

## 6. Engine-facing API added after review of `68adc4d`

These are library-only additions, with no CLI verb yet. They are **unverified at runtime**: no existing verb reaches them, and the test freeze rules out tests. The walkthrough below is from the code.

### `Service::deploy_append(component, name)` (`src/deployment.rs:502`)

This is the barrier deploy for rule 5. It deploys what is staged so far for a component. It leaves the component open, so later steps can stage more and call it again.

1. **No manifest yet.** It delegates to `deploy_report` (`:508-510`), so the first barrier behaves like a normal deploy.
2. **Preflight the whole staged tree** before the game tree changes (`:529-614`). The checks match `deploy_report`: staged symlinks are refused, destinations are resolved case-aware, case-duplicate staged files are refused, and symlinked ancestors are refused.
3. **Owned path with an unchanged staged hash** is skipped (`:561-563`). This covers files already deployed by an earlier barrier.
4. **Owned path with a changed staged hash.** The live file is replaced only if its bytes still equal the manifest hash. Otherwise the call fails before anything changes (`:564-580`). The entry keeps its original backup. This covers:
   - a later Copy over a file staged earlier;
   - a staged Copy over patcher output that `record_live` recorded.
   In both cases uninstall still restores the pre-install original.
5. **Unowned path** is backed up and deployed exactly as in `deploy_report`, with the same conflict warning and stale-backup guard (`:582-614`).
6. **Execution.** Replaced live files are parked under `<manifest root>/append-pending/<id>` before they are overwritten (`:616-649`). A leftover parking directory from an interrupted run blocks the call.
7. **Failure.** Any error, including a failed manifest save, rolls back the batch (`:651-667`):
   - new files are removed and their backups restored (`roll_back`);
   - parked files are moved back.
   Earlier batches stay deployed and recorded.
8. **Save.** The manifest is saved once per batch (`:668-691`):
   - replaced entries get the new hash, size and method;
   - new entries are appended;
   - `DeployedUtc` moves to now, so purge unwinds the component after anything deployed before it.
   Then the parking directory is removed.

### `Service::reinstall_blockers(component)` (`:696`) and `prepare_reinstall(component)` (`:742`)

- **`reinstall_blockers`** is read-only. It uses the same resolution as `uninstall_report` and returns owned paths that uninstall would skip:
  - unsafe paths;
  - paths that are no longer regular files;
  - files with a hash mismatch that no later deployment has backed up (`later_owner`).
- **`prepare_reinstall`**:
  1. Fails closed on any blocker, before any change (`:744-749`).
  2. Runs `uninstall_report`. As a defensive check, it errors if the tree changed in between and something was still skipped (`:750-763`).
  3. Removes the component's staging directory, only if it is a real directory, so the reinstall stages from scratch (`:765-769`).
  4. With no manifest, it only clears staging.

### Engine call order (for the engine owner)

1. `prepare_reinstall(guid)` before the component's first instruction, in managed installs only.
2. Stage Extract/Copy/Move/Rename per rules 2-4.
3. At each Patcher/Execute/Run step:
   - `deploy_append(guid, name)`;
   - `capture_index()`;
   - run the process;
   - `record_live(guid, name, &before, Some(snapshot_game))`;
   - `scan()`.
4. At the end of the component: `deploy_append(guid, name)`, then `scan()`.
