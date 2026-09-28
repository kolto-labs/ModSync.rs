# Downloads GUI step 1: desktop job adapter and safe initialization

Plan: [downloads-gui-plan.md](../downloads-gui-plan.md), serialized boundary 1 only
(G1–G6 plus G10's launch ordering and probe isolation). Step 2 (settings, guarded
FOMOD queue, navigation) and step 3 (full native acceptance) are not started.
Nothing here is accepted: independent SPEC and QUALITY reviews have not run.

Source: commit `6ac257f` on `baio/nice-archimedes-fmxu3k` plus an uncommitted
review follow-up in `src/ui.rs` and `src/ui/downloads_panel.rs` (a test-only
`record_nxm_download` restored for a frozen test; proxy secrets merged into the
redactor before a changed context freezes the rows; Stop during planning skips the
transfer; the default settings directory compared through its nearest existing
ancestor; the binding context derived from the input stamp instead of a second
per-frame guide serialization; unreachable Fetch/NXM arms made explicit). Step 1
changes only `src/ui.rs`, new `src/ui/downloads_panel.rs` and `src/main.rs` (GUI
arguments). [receipt.json](receipt.json) binds git HEAD, porcelain, the SHA-256 of
every `src/**/*.rs`, `Cargo.toml` and `Cargo.lock`, and the desktop binary
`80f4286c2ff527953fdc518dd80bf95784468dc5f39f22fe71b6de5dc049cd06`.

## What changed

- **Fetch Downloads** (Downloads panel, from DownloadsExplain onward and in the
  utility view). Needs a loaded guide and an existing workspace; the game folder is
  not required. A run captures the guide, normalized workspace, archive options and
  Nexus key once, then off the UI thread reads Peer settings (sharing=false wins),
  builds `selection::download_candidates(.., Scope::All)`, and calls the shared
  `download_orchestrator::plan` / `run_with_loopback_ca` with workers 4, retries 1 and
  a per-run `PeerAcquire` built exactly as the CLI builds it. Nothing starts until
  the button is pressed.
- **nxm handoff** now goes `nxm_target` → `AcquisitionPlan::single` (Peer Off) →
  shared run → shared `record`, re-resolved against the guide bound at dispatch.
  The old NexusClient + `download::fetch` bypass and its per-app record helper are gone.
- **Run identity and binding.** Every run takes a new identity from one counter
  before any asynchronous work, bound to the guide revision, workspace text and
  canonical path, game folder and completed-base context. Every worker message carries
  it. Only the active run can change rows or settle; the context is checked every
  frame and again at the terminal. A changed context freezes the rows, and its
  terminal records nothing and says which folder holds the files.
- **Terminal.** The report is authoritative. On a matching context `record` runs on
  a clone of the bound guide, which replaces the guide only if `record` succeeds
  (partial runs record their completed and present archives only). The guide file is
  never saved; the panel says to use Save guide as…. Validation is invalidated at
  start and when records change the guide. No download creates install receipts or
  validation.
- **Presentation.** Rows per plan target (keyed by run + target index), with a line
  per source that has actually reported (a Peer that never started never appears),
  phase, measured bytes, a percent only for a positive known total, peer count when
  supplied, the orchestrator's own Superseded, and manual reason with an Open
  download page button (explicit click only). Final counts come from outcomes and
  plan skips, in archive-target units, separate from the selected component count.
  FOMOD follow-ups and inspection errors of an accepted, noncancelled Fetch are
  listed (the guarded editor queue is step 2).
- **Stop.** Stop Downloads (and the Activity Stop) opens a download-specific
  confirmation that explains no install rollback is involved. It cancels that run's
  own token; the panel shows Stopping… until the workers join and the terminal
  arrives; a new Fetch cannot start before that. Retry plans afresh.
  **Known limitation:** planning (including existing-archive hash verification) is
  not interruptible. A Stop pressed while Fetch is preparing takes effect only when
  planning returns, so Stopping… can last as long as that verification; the run then
  transfers nothing and records only archives already present.
- **Redaction.** API keys, the raw nxm link, peer proxy endpoints, URL user info,
  queries and fragments are removed from panel text and activity lines, including
  rejected protocol links.
- **Safe initialization.** `App::with_launch` fixes the settings directory and any
  probe key before the stored key, telemetry, guide load and (in `launch`) protocol
  dispatch. This also fixes the ordinary `--settings-dir` launch, which previously
  read the key and telemetry from the default directory. Hidden `gui` options
  `--nexus-mods-api-key`, `--nexus-api-base`, `--extra-ca-file` form a probe launch:
  it needs an existing settings directory that is neither the default settings
  directory nor nested with it (after resolving symlinks), an API base needs an
  explicit key, the CA is loaded per invocation, the instance socket is neither
  claimed nor forwarded to, the stored key is never read, the key field cannot save,
  and the Nexus update check is refused.

## Native proof

`probe.py` runs the real desktop binary on a private Xvfb display (`:93`,
1280×800 and 600×400) inside `unshare -Urn` with no routes, private HOME/XDG/
runtime roots, loopback HTTPS origin (per-run CA and leaf) and loopback HTTP Nexus
API. It clicks with xdotool and reads only server logs, the private filesystem and
the process table. A listener sits on the ordinary per-user socket path; stored
and environment sentinel keys are planted.

```sh
cargo build --locked --lib --bin modsync
DOWNLOADS_GUI_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/downloads-gui/probe.py
```

Needs `Xvfb`, `xdotool`, ImageMagick `import`/`convert`, `openssl`, `iproute2`.
Result: **46/46 checks** ([receipt.json](receipt.json), [screenshots](screenshots/)).

| Screenshot | Shows |
| --- | --- |
| 01 | nxm link given at launch finished through the shared path |
| 02 | DownloadsExplain idle; no request before the click |
| 03 | Fetch preparing (slow existing-archive verification), no request yet |
| 04 | two live transfer rows at once |
| 05 | status after navigating to Validate; Run Validation disabled while busy |
| 06, 07 | terminal summary; manual reason and link; 404 failure |
| 08, 09 | the same at 600×400 |
| 10–13 | large transfer, stop confirmation, Stopping…, stopped summary |
| 14, 15 | retry makes a new request after the first ended, then stops again |
| 16 | wrong loopback CA: nxm archive refused |

Checks cover: nxm archive bytes; API saw only the explicit key and the link key;
no sentinel on any request; no request before Fetch or during preparation; both
slow transfers live together; unselected and present archives never requested;
archive bytes; guide file, game and settings unchanged; no partial files; the same
fixture through `fetch-downloads --use-file-selection --record` makes the same
requests, writes the same bytes, reports statuses completed/completed/manual/failed/
completed, records 4 files (the GUI summary, screenshot 06, also says 4) and exits 1;
Stop ends the request incomplete with no residue while the GUI keeps running; retry
starts a new request only after the first ended; six refused probe launches exit 1
with the exact message and no window (no settings dir, the default dir, a symlink
to it, a dir nested in it, API base without key, unreadable CA); wrong CA gives no
archive and no CDN request; nothing was forwarded to the user socket and the runtime
directory holds only that socket.

**Found (not changed here):** every Origin `Transferring` event carries `total: 0`
although the server sends Content-Length, in the CLI's `--progress-json` too
(check `cli_transfer_totals_are_zero_not_content_length`): `src/download.rs:453`
reads `content_length()` after the sniff read at `:382`. The panel treats 0 as
unknown and never shows a percent; fixing it belongs to the transport owner.

## Not proven here (pending)

- Guide record diff through the GUI: Save guide as… uses the XDG portal file dialog,
  which bare Xvfb does not provide. Records are shown only as the panel count.
- Peer: Origin-win/Peer-win races, zero-connection gates and sharing-off bootstrap
  (the cloud kernel has no IPv6 for the seeder; see the fetch-shared-url README).
- Cancel mid-race; close-during-work (no window manager to send a close request);
  context changes through real UI operations; late/stale messages (unreachable
  without synthetic messages, so source review only).
- GUI nxm versus CLI `nxm-download --input`: the CLI verb has no loopback CA option.
- FOMOD follow-up queue/editor, settings controls, completion-page sidebar, and
  everything else assigned to steps 2 and 3.
- At 600×400 the fixed 320 px Downloads sidebar leaves about 280 px for the page
  body (screenshots 08, 09); usable but cramped, a layout item for step 3.

The present fixture archive is marked `configured`, so its large payload slows only
preparation's verification (screenshot 03), not a post-run FOMOD inspection. With it
unmarked, an earlier run showed the GUI staying busy (all targets settled, Stop still
offered) until that inspection finished, as intended.
