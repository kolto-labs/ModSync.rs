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
| 6 | **Out-of-order uninstall orphans the original.** Uninstall A while a later B still overwrites the same path. A's file hash mismatches, so it is skipped, and A's backup holding the original is abandoned. B's uninstall later restores A's bytes, which nobody owns. C# behaves the same. | Probe P6 | Fixed (deliberate difference). If the newest manifest deployed *after* A has a backup whose hash equals A's deployed hash, A's original moves into that backup slot (or that entry's backup is dropped when A displaced nothing). Reported as `transferred`. The move is one `rename` inside the manifest root, which replaces the later owner's slot atomically, so a failure leaves both backups and both manifests as they were (P26). The match is on backup hashes plus later `DeployedUtc`, so `DeployedUtc` is never rewritten after creation (P21). |
| 7 | **Profile roots and switching.** | C# `ManagedInstallSession.cs:152-156` | `Service::for_profile` and `Service::resolve`. The CLI accepts `--profile`/`--settings-dir`, or uses the active profile, with the C# missing-profile message. `deployment switch --to <p> [--purge-current]` is new behavior. It optionally purges the active profile's deployments, then activates the target through the existing `profiles::Store` API and reports its staged-but-undeployed components. It does not redeploy them, because install order lives in the install run, not the staging tree (P14). Without `--purge-current`, it is refused while the current profile still has deployments (P17). |
| 8 | **One corrupt manifest blocked status and purge.** C# skips it with a warning. | Probe P12 | Fixed: `deployed_with_warnings`; the CLI prints warnings on stderr. |
| 9 | **Backup tree not pruned recursively.** An empty `backups/<guid>/Override/` stayed behind. | Old `prune(&backups, Some(&backup))` | Fixed: `remove_empty_tree`, the equivalent of C# `DeleteDirectoryIfEmptyRecursive` (P2). |
| 10 | **Hash comparison was case-sensitive, and backups were restored without checking `OverwroteExisting`.** | C# `:370`, `:387` | Fixed. |
| 11 | **GUID casing.** C# names manifests `Guid.ToString("D")`. Rust used the raw ID. | Probes P13, P16 | GUID-shaped IDs are unbraced and lowercased for the manifest name and `ComponentGuid`. Manifest and backup lookups fall back to a case-insensitive filename match, so manifests written by the earlier CLI under an uppercase name are still found by uninstall, purge and the redeploy guard (P16). The staging lookup falls back to the raw name. |
| 12 | **DeployedUtc precision.** chrono wrote nine fractional digits. | C# manifest shows `2026-09-27T17:01:49.3313133Z` | Now written with seven digits plus `Z`. Reading accepts RFC 3339 or offset-less text. |
| 13 | **Case-aware destinations on case-sensitive filesystems.** `override/a.tga` created a second `override/` next to `Override/`. C# on Linux does the same. | Probe P11 | Fixed (Linux-only difference). Existing case variants are reused. The manifest records the on-disk path. Deploy always scans each directory and refuses case-colliding entries, even when the exact spelling exists (P19). Uninstall uses the recorded spelling first, so a case variant created later does not hide a deployed file (P24). An entry that cannot be resolved safely stays in the manifest instead of being dropped. |
| 14 | Conflict (ownership) warnings | C# `:122-126` | Ported. They go to stderr, and deploy stdout stays manifest-shaped. |
| 15 | GUI management: status indicator, per-component uninstall, purge, deployed badge, install-in-progress guard | `ManagedDeploymentActions.cs` | Open. `ui.rs` is out of scope for this lane. The library now returns the reports a GUI needs. |
| 16 | **Two live profiles over one game directory.** The ownership handoff (rank 6) and conflict warnings search only one profile's manifest root, so out-of-order uninstalls across profiles orphaned an original (reproduced by the independent review). | Review finding 1; probes P17, P18 | Fixed by exclusivity, not by a shared index. Each manifest root records its game directory in a `game-directory` marker file. The manifest shape is unchanged. Deploy, append and record refuse while any other profile under the same settings directory has manifests for this game, or has manifests and no marker. `switch` without `--purge-current` is now refused while the current profile is live. A root with manifests also refuses every other game directory, for uninstall and purge too (P28). Explicit `--manifest-dir` roots have no peers to check; that path is for tooling and tests. |
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

The real CLI binary was run: `target-dep/debug/modsync deployment …` (no-default-features build) against disposable trees in `/tmp/depprobe/run/*`. Script: `docs/parity/deployment-probe.sh`. Result at `b1c60b1`: **63 pass, 0 fail**, exit 0. Binary: `--no-default-features` build copied to `/tmp/depprobe/modsync-nodefault-b1c60b1`, sha256 `838c24e64288424a1337d1b2f67aca54168619fbf4cd813678314a633aad3f89`. The script prints that hash and `source: b1c60b1…` with no uncommitted-source flag. Receipt: `~/.cache/modsync-codex/runs/deploy-fix/probe-b1c60b1.log` (copy in `/tmp/depprobe`). The script exits 1 on any failed probe and 2 on setup errors: with `B=/bin/false` it reports 28/35 and exits 1 (`probe-failclosed.log` in the same directory), and with a missing binary it exits 2. Manifest notation: `path+bk` means a backup exists, `/hl` means hardlink, `/cp` means copy.

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
| P17 `switch --to Two` with One still deployed, no purge | One: `aaaaaaaa[x.txt]` | unchanged | exit 1 "still has 1 deployment(s)"; One stays active; files stay |
| P18 profile Two deploys while One owns the game | One: `aaaaaaaa[x+bk]` | unchanged | Two refused, `ONE` live; after One's purge Two deploys and its purge gives `ORIG` |
| P19 game has `Override/` and `override/`, staged `Override/a.tga` | (none) | (none) | exit 1 "Case-colliding entries"; nothing deployed |
| P20 patcher replaces `dialog.tlk`, which A deployed | `aaaaaaaa[dialog.tlk+bk/hl]` | `aaaaaaaa[dialog.tlk+bk/cp]` | `updated=[dialog.tlk]`; staged copy now `PATCHED`; uninstall restores `ORIG` |
| P21 record on older A after B overwrote it | `aaaaaaaa[x+bk] bbbbbbbb[x+bk]` | A gains `new.2da` | A's `DeployedUtc` unchanged; uninstall A then B gives `ORIG`, no orphan backups |
| P22 another process holds the game lock | (none) | (none) | exit 1 "Another ModSync process"; deploys once free |
| P23 prior copy does not match the snapshot | (none) | (none) | `skipped=[dialog.tlk]`; no backup file left |
| P24 `override/` created after deploying into `Override/` | `aaaaaaaa[Override/a.tga+bk]` | (none) | uninstall restores `ORIG`; no orphan backup |
| P25 record A after the patcher changes B's newer `y.2da` | `aaaaaaaa[x+bk] bbbbbbbb[y+bk]` | unchanged | `skipped=[Override/y.2da]`; nothing recorded in A; no backup made |
| P26 handoff rename fails (B's backup dir mode 555) | `aaaaaaaa[x+bk] bbbbbbbb[x+bk]` | unchanged | exit 1 "Could not hand the original…"; B's slot still `A`, A's still `ORIG`; retry gives `ORIG` |
| P27 record fails on the second backup (dir mode 555) | (none) | (none) | exit 1; the first backup was removed; retry records both; uninstall restores both |
| P28 uninstall with another `--game-dir` | `aaaaaaaa[a.txt]` | unchanged | exit 1 "These deployments belong to game directory…"; marker unchanged |
| P15 snapshot, "patcher" edits `dialog.tlk` and adds `new.2da`, record with `--backup-source` | (none) | `aaaaaaaa[Override/new.2da/cp, dialog.tlk+bk/cp]` | uninstall restores `DIALOG0` and removes `new.2da`; without a backup source the changed `dialog.tlk` is skipped |

### C# differential (built DLL, nothing compiled)

`dotnet …/ModSync.GUI/bin/Debug/net10.0/ModSync.Core.dll install --managed --profile One -y --skip-validation --no-checkpoint` was run on the host with `XDG_CONFIG_HOME=/tmp/csprobe/cfg`. Script: `docs/parity/deployment-csharp-differential.sh`. Rust's `profile --action create` wrote the profile. The DLL is a Sep 15 build from the main ModSync checkout, not the pinned oracle. Its current `DeploymentService.cs` and `ManagedInstallSession.cs` sources are byte-identical to `d31ccbd3`, but whether the DLL was built from exactly those sources is not proven. The evidence is the observed behavior below.

| Probe | Result |
|---|---|
| D1 C# writes, Rust reads and uninstalls | C# staged into `profiles/One/staging/<guid>/Override` and deployed 2 hardlinks, 1 overwrite. Manifest: `DeployedUtc "…49.3313133Z"`, `BackupRelativePath: null`. Rust `status --profile One` parsed it. Rust `uninstall --component AAAAAAAA-…-00A` gave `deleted=[x.2da, added.tga] restored=[x.2da]`, and the live file is `ORIG`. |
| D2 mixed writers, Rust purge | Rust deployed `RustB` into the same profile, then C# installed A over it. C# logged `Conflict: 'Override/x.2da' … already deployed by component 'RustB'`, so C# parsed the Rust-written manifest. It backed up the Rust hardlink. Rust purge order was A (C#), then B (Rust), and the live file is `ORIG` with no leftovers. |
| D3 C# redeploy of the same component | Run 1: backup=`ORIG`. Run 2: backup=`CSNEW` (C# `DeploymentService.cs:150-155`). Purge leaves `CSNEW`, so the original is lost. Rust refuses this case (P3). |

Re-run at `b1c60b1` on the host: 8 assertions, `TOTAL fail=0`, exit 0. DLL sha256 `2c8cb17a6048127f051faf941b840f2e43c65476ac7a018247f383c5bffac270`; Rust binary sha256 `838c24e6…` as above. Receipt: `~/.cache/modsync-codex/runs/deploy-fix/differential-b1c60b1.log`. The C# DLL neither writes nor reads the `game-directory` marker; D2 shows both still share the same roots.

Not covered: a C# purge or uninstall of a Rust-written manifest, because C# exposes no CLI verb for it (GUI only). Windows and macOS filesystem behavior was not checked.

## 6. Engine-facing API added after review of `68adc4d`

These are library-only additions, with no CLI verb yet. They are **unverified at runtime**: no existing verb reaches them, and the test freeze rules out tests. The walkthrough below is from the code.

### `Service::deploy_append(component, name)`

This is the barrier deploy for rule 5. It deploys what is staged so far for a component. It leaves the component open, so later steps can stage more and call it again.

1. **No manifest yet.** It delegates to `deploy_report`, so the first barrier behaves like a normal deploy.
2. **Preflight the whole staged tree** before the game tree changes. The checks match `deploy_report`: staged symlinks are refused, destinations are resolved case-aware, case-duplicate staged files are refused, and symlinked ancestors are refused.
3. **Owned path with an unchanged staged hash** is skipped. This covers files already deployed by an earlier barrier.
4. **Owned path with a changed staged hash.** The live file is replaced only if its bytes still equal the manifest hash. Otherwise the call fails before anything changes. The entry keeps its original backup. This covers:
   - a later Copy over a file staged earlier;
   - a staged Copy over patcher output that `record_live` recorded.
   In both cases uninstall still restores the pre-install original.
5. **Unowned path** is backed up and deployed exactly as in `deploy_report`, with the same conflict warning and stale-backup guard. A path owned by a component deployed *after* this one is refused: this manifest keeps its older `DeployedUtc`, so taking the path would invert that owner's backup chain.
6. **Execution.** Replaced live files are parked under `<manifest root>/append-pending/<id>` before they are overwritten. A leftover parking directory from an interrupted run blocks the call.
7. **Failure.** Any error, including a failed manifest save, rolls back the batch:
   - new files are removed and their backups restored (`roll_back`);
   - parked files are moved back.
   Earlier batches stay deployed and recorded.
8. **Save.** The manifest is saved once per batch:
   - replaced entries get the new hash, size and method;
   - new entries are appended;
   - `DeployedUtc` is kept. It is ownership and purge order, and moving it broke the handoff (review finding 4).
   Then the parking directory is removed.

### `Service::reinstall_blockers(component)` and `prepare_reinstall(component)`

- **`reinstall_blockers`** is read-only. It uses the same resolution as `uninstall_report` and returns owned paths that uninstall would skip:
  - unsafe paths, and paths that cannot be resolved safely (case collisions, symlink or file ancestors);
  - paths that are no longer regular files;
  - files with a hash mismatch that no later deployment has backed up (`later_owner`).
- **`prepare_reinstall`**:
  1. Fails closed on any blocker, before any change.
  2. Runs `uninstall_report`. As a defensive check, it errors if the tree changed in between and something was still skipped.
  3. Removes the component's staging directory, only if it is a real directory, so the reinstall stages from scratch.
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

## 7. Independent review of `68adc4d` and what changed

Review: `/home/brunner56/.cache/modsync-codex/runs/review-deploy/report.md` (CHANGES_REQUIRED). It approved these deviations: redeploy refusal, backup-required provenance, stable-tree symlink refusal, single-variant case reuse, lowercase manifests with legacy discovery, and the same-profile handoff happy path. They stay. Fixes landed in `b1c60b1`.

| # | Finding | Answer | Change and referent |
|---|---|---|---|
| 1 | Two live profiles strand bytes | concede | Each manifest root records its game directory in a `game-directory` marker. Deploy, append and record refuse while another profile has manifests for this game, or has manifests and no marker (`ensure_exclusive_game`, P18). A root refuses any other game directory, for uninstall and purge too (`ensure_own_game`, P28). `switch` without `--purge-current` is refused while the current profile is live (P17). The manifest shape is unchanged. |
| 2 | Rollback not race-safe | concede for scenarios 1, 3 and 4; defend the residual of scenario 2 | (1) A write never replaces an existing file: it hardlinks, or publishes a temp copy with `persist_noclobber`. A file that appears after preflight makes the write fail and is left alone (`place`). (3) Undo deletes a file only while its hash equals the bytes this run deployed. It moves originals back with no clobber (`undo_one`, `move_no_clobber`). (4) Undo reports every failure. Entries it could not undo are saved in the manifest so uninstall can finish (`abort`). CLI mutations take the per-game lock that `recovery::Lock` takes, so ModSync writers are serialized (P22). Both paths are canonical (`paths::Roots::new`, `Service::new`). (2) Ancestors are re-checked right before each write. A write whose parent resolves outside the game tree is removed and reported (`place`). **Defended residual:** a non-ModSync process that swaps an ancestor for a symlink between that re-check and `create_dir_all`/`hard_link` can still cause a brief write outside the tree. Closing it needs descriptor-relative `openat`/`O_NOFOLLOW` walking. That means a new crate in `Cargo.toml`, which this lane does not own, and std has no Windows equivalent. No shell probe can hit these windows reliably, so the evidence for scenarios 1-4 is the code above, not a probe. |
| 3 | Provenance skips owned paths | concede | `record_live` adopts patcher output for an owned path when the pre-patcher bytes were this component's. It updates the entry hash, keeps the original backup, and syncs the staged copy before saving, so `deploy_append` does not redeploy stale bytes. If something else changed the path before the patcher, it skips with a warning. P20. |
| 4 | `record_live` rewrites ordering | concede | `DeployedUtc` is set only when a manifest is created. `record_live` and `deploy_append` (`fa3eab3` had the same bug) keep it (P21). Both also refuse to take a path a *newer* component owns, which would invert the chain through a different door (`deploy_append` refuses; `record_live` skips with a warning, P25). |
| 5 | Handoff not failure-atomic | concede | The original moves into the later owner's slot with one `rename` inside the manifest root, which replaces the slot atomically. The slot is never empty. A failed rename leaves both backups and both manifests unchanged (P26). When A displaced nothing, B's manifest is saved before B's backup is deleted. |
| 6 | Provenance backup not transactional | concede | The copy goes to a temp file. The hash of that exact file must equal the snapshot hash (P23). It is synced, then published with no clobber. Any later error, including a failed manifest save, removes every backup this call created, and names any it could not remove (P27). |
| 7 | Exact spelling hides case collisions | concede | `resolve_case` always scans the directory and refuses more than one case-equivalent entry (P19). Uninstall and `reinstall_blockers` resolve the recorded spelling first (`locate`), so a variant created after deploy cannot make uninstall drop an entry (P24). An entry that cannot be resolved safely stays in the manifest. |
| 8 | Probes are observations | concede | Both scripts run under `bash` with `set -u`. They check their setup, print the binary hash and source commit, assert every result, and exit nonzero on any mismatch (1 on a failed assertion, 2 on setup). With `B=/bin/false` the probe exits 1. The C# DLL is identified by hash only; its source provenance stays unresolved without a pinned rebuild. |

`fa3eab3` re-check against findings 3 and 5:

- `deploy_append` shared finding 4 (it moved `DeployedUtc`); now fixed.
- For finding 3, `deploy_append` relies on `record_live` keeping staging in step. It still refuses to replace an owned live file whose bytes changed.
- It does not use the handoff (finding 5). Its parking and undo use the no-clobber move and hash-guarded undo. It now refuses paths a newer component owns.
- `prepare_reinstall` runs `reinstall_blockers` first. That refuses when a patcher changed an owned file that `record_live` did not adopt (finding 3). It then runs `uninstall_report`, which uses the atomic handoff (finding 5).
- Neither `deploy_append` nor `prepare_reinstall` has a CLI verb. Both are unverified at runtime.
