# Pinned C# installing-page oracle and native reference plan

2026-09-27 source audit. Oracle checkout `/home/brunner56/.codex/worktrees/5813/ModSync`, HEAD `d31ccbd3bfcddb4705917eafa6483715c166de3b`. No builds, tests, native launches, product changes or commits were performed. Source observations below are not new native acceptance. Rust's existing safety/receipt contracts and user freeze on creating/porting/expanding any Rust tests remain binding.

## Call graph: distinguish three similarly named contracts

1. Active `WizardHostControl.axaml.cs:173` creates `InstallingPage` with **all components**. Page `OnNavigatedToAsync` (`InstallingPage.axaml.cs:81-94`) starts an unawaited background task, stopwatch and global logger subscriptions. Reentry is suppressed while installing or after `_installationComplete`.
2. The page calls **`InstallationService.InstallAllSelectedComponentsAsync`**, with `Action<int,int,string>`, at page lines289-294. It does **not** use `InstallationCoordinatorService.InstallProgress`. Service lines1146-1166 wrap a managed-install session; core lines1180-1206 validate source directory and FOMOD configuration before creating `InstallCoordinator`.
3. `InstallCoordinator.InitializeAsync` lines35-88 initializes checkpoint state, optionally creates a full copy+ZIP snapshot and Git baseline, and returns either original input order or `GetOrderedInstallList` (default sorting). `--no-checkpoint` skips snapshots/Git; it does not imply no state bookkeeping.
4. The separate **`InstallationCoordinatorService.ExecuteComponentsWithCheckpointsAsync`** exposes typed progress/events but is not this page's execution path. Do not wire Rust to a second installer simply to resemble that class.

## Exact page composition

`InstallingPage.axaml:9-251`: outer `PageScrollViewer`, horizontal scrolling Disabled, vertical Auto. Inner grid margin16,12, rows `Auto,*,Auto`. No width breakpoint or column stacking exists. Content remains a two-column statistics/showcase layout at small width, with whole-page vertical scrolling. Active main window is1280×800, minimum600×400 (`MainWindow.axaml:15-18`); active host has a persistent download sidebar from Download Process onward. Older modal wizard dimensions/sidebar are not the active oracle.

| Region / source | Exact controls and layout |
|---|---|
| Main border, lines12-85 | row0, padding24,20, radius12, bottom margin16; vertical stack spacing16 |
| Header, lines15-29 | grid `*,Auto`; `PercentText`28 Bold `0%`; `CountText`16 opacity.8 vertically centered `0/0 mods installed` |
| Overall bar, lines32-37 | `MainProgressBar`, height12, minimum0 maximum1, radius6 |
| Current info, lines40-61 | grid `*,Auto`; left stack spacing6: `CurrentModText`16 SemiBold `Preparing installation...`; `DirectionsText`13 opacity.7 Wrap MaxHeight60. Right `CurrentOperationText`13 opacity.6 top-aligned `Initializing...` |
| Per-mod activity, lines64-68 | `CurrentModProgress`, height6 radius3 indeterminate true. No explicit minimum/maximum; do not assume the main bar's0..1 applies |
| Checkpoint line, lines71-83 | grid `Auto,*`, top margin8; disk glyph12 opacity.7 and `CheckpointStatusText`12 opacity.7 |
| Statistics/showcase, lines88-240 | row1 grid columns `*,16,*`, rows `Auto,16,Auto` |
| Statistics border, lines90-187 | row0 col0 padding20,16 radius10; inner equal three columns, rows `Auto,12,Auto,12,Auto` |
| Time row | labels11 opacity.6 centered `ELAPSED`, `REMAINING`, `RATE`; values `ElapsedTimeText`, `RemainingTimeText`, `RateText`18 Bold centered |
| Issues row | full three-column span, label `ISSUES`; two equal columns, centered horizontal stacks spacing6: warning/error glyph14 and `WarningsText`/`ErrorsText`16 Bold |
| Checkpoint count row | full span, label11 `CHECKPOINTS CREATED`; `CheckpointsCreatedText`16 Bold centered |
| Showcase, lines190-212 | col2 border padding20,16 radius10; centered stack spacing12; glyph🎮48; `Installing Your Mods`16 SemiBold; `Sit back and relax while we handle everything`13 opacity.7 Wrap centered |
| Explanation, lines215-239 | row2 all columns, padding20,16 radius10; stack spacing8; heading14 SemiBold `⚙️ What's Happening?`; two equal columns with spacing4,12pt opacity.8 text |
| Explanation text | `✓ Extracting archives`; `✓ Running TSLPatcher/HoloPatcher`; `✓ Moving and copying files`; `✓ Managing dependencies`; `✓ Creating checkpoints`; `✓ Validating installations` — static, not live phase completion indicators |
| Footer, lines243-249 | row2 padding16,12 radius8 top margin16; text12 opacity.7 centered `⚠️ Please do not close the application during installation` |

Title `Installing Mods`; subtitle `Please wait while mods are being installed...` (code24-25). No page-local log, copy-report control, Stop button, operation list or byte-progress control exists here. Preserve useful Rust controls without calling them exact C# controls.

## Initial, callback and terminal state semantics

Runtime `InitializeDefaults` (code129-195) overrides XAML: Rate is **`0.0 mods/min`**, not XAML's `0.0/min`; checkpoint line is **`Checkpoint system enabled`**, not `Checkpoints enabled`. Counts are0, elapsed/remaining `--:--:--`, overall0, per-mod indeterminate/value0. These checkpoint claims are unconditional.

The service callback at `InstallationService.cs:1231` occurs **before** checking completed/skipped/blocked state and before `InstallAsync`. Current index is **zero-based**; total is all ordered selected components. The page calls this `currentIndex/total mods installed`, sets overall fraction `index/total`, rounds percent with .NET Math.Round, displays `Installing: {componentName}`, finds directions by exact first matching name, assigns `_installedCount=index`, updates metrics and says `Creating checkpoint for '{componentName}'...`. This is not a real checkpoint-start event or successful-completion counter. Duplicate names may select the wrong directions.

Actual successful checkpoint creation occurs **after** successful component installation (`InstallationService.cs:1252-1283`). Page counts a global log substring `✓ Checkpoint created:` and displays `Checkpoint created successfully`; no typed receipt identity is supplied. Checkpoint exceptions are logged as warnings and installation continues to snapshot promotion. No-checkpoint mode cannot support the initial optimistic enabled/creating labels.

Warnings/errors count global logger messages containing `[Warning]`/`[Error]`, case-insensitive; ExceptionLogged separately increments errors (page392-431). They are process-wide, not scoped to a run/component, and increments are not synchronized. Logger constructs the event string and prefixes severity through its logging wrappers (`Logger.cs:72-87,294-302,360-376,408-422`). These are log/exception observations, not a typed count of failed operations; concurrent unrelated work and exceptions can skew counts.

Metrics update **only at component callbacks and terminal success rendering**, not on a timer. Elapsed is Stopwatch `hh:mm:ss`; average ticks=elapsed/max(1,index); ETA=average×max(0,total-index); rate=index/max(.001,elapsed minutes), one decimal `mods/min`. Long individual operations leave metrics stale. The stopwatch stops in finally after terminal rendering. No operation-specific progress callback updates `CurrentOperationText`: it stays `Initializing...` until complete/cancel. No determinate per-mod progress exists during execution.

On **any returned exit code** (page296-345), the page sets installed count to locally selected **non-widescreen** count, `_installationComplete=true`, overall100%, countN/N, `✅ Installation complete!`, operation `Complete`, per-mod indeterminate false/value1, checkpoint `✓ {count} checkpoints created`, and `_canNavigateForward=true`. Only afterward does it log a non-success exit code. Thus returned errors/cancellation codes can look successful. The per-mod bar lacks explicit Maximum1, so Value1 is not demonstrated to represent100%. Do not invent a filled-success bar from this value without native observation.

Thrown OperationCanceledException instead shows `Installation cancelled by user`, operation `Cancelled`, checkpoint `Installation was cancelled`; it does not set completed or forward-ready, stop per-mod indeterminate, reset counts or repaint elapsed. Generic exception logs `Error during installation` and changes only checkpoint text. Finally clears installing, stops stopwatch and unsubscribes logger handlers. `ValidateAsync` tests only `_installationComplete`, otherwise returns `Installation is still in progress. Please wait for it to complete.` — misleading after cancellation/failure.

Base-page count excludes widescreen (`selectedMods` line202), but it passes all components to service, whose filter is only IsSelected (1180); selected widescreen can execute in this base path. This mismatch must not replace Rust's correct shared base/remaining phase preparation.

## Separate InstallProgress contract and events

`InstallationCoordinatorService.cs:246-263`: `InstallProgress { InstallPhase Phase; string Message; int Current; int Total; string ComponentName; }`; enum `Initializing, InstallingComponent, CreatingCheckpoint, RollingBack, Completed`. In the inspected implementation, only **InstallingComponent** is emitted (78-85), once before execution, one-based selected iteration count against **components.Count including unselected entries**. It does not emit every enum phase.

Events: ComponentInstallStarted before progress/execution; ComponentInstallCompleted after success with `CheckpointId=null`; InstallationError and ComponentInstallFailed for failures; WidescreenNotificationRequested before the first widescreen start, with cancellation option. Cancellation is checked before components and passed into instruction execution. UserCancelled/UnknownError return early; other non-success codes can continue, and caught per-component exceptions can fall through to final Success (98-168). Despite the class/method name, rollback/session listing/restore methods explicitly report disabled/no-op behavior (178-203). These contracts are not proof of working per-mod checkpoints.

The actual page's InstallationService path does have checkpoint implementations and optional continue-on-missing/continue-on-failure flags (1286-1346). Those best-effort continuations conflict with the user's strict guide-order/no-skip rule. Preserve Rust's strict stop/recovery semantics; do not reproduce the unsafe continuation merely for superficial parity.

## Host navigation, Stop and close

Active `WizardHostControl` assigns Back disabled and Next/Continue enabled from page properties **before activation**, sets label `Continue`, and calls OnNavigatedToAsync (772-792). No completion subscription/property notification refresh is present in the inspected host/page. The page later changes a private boolean; Continue may remain disabled. This is a source-established refresh gap requiring native confirmation, not permission to simulate clicks or bypass readiness. Host Next handler validates the current page before moving (813-845).

There is **no active host wizard Stop/Cancel button**. Cleanup cancels/disposes the token (927-931); constructor-supplied CTS on InstallingPage is only null-checked, not retained. The separate old `InstallWizardDialog` labels its button `Stop Install` and promises `The current mod will finish installing, but no further mods will be installed.` (408-434). It cancels immediately and execution passes the token into instructions; the text is not a guarantee of component-boundary cancellation. Do not mix that older dialog's controls into the active-host oracle.

Main-window closing (`MainWindow.axaml.cs:1920-1970`) asks a save/quit confirmation only in EditorMode, then disposes app services and closes; no active-install-specific safety confirmation appears there. Rust's existing close/Stop confirmation must not be weakened. Record any deliberate safety difference.

## Theme and responsive rules

Page borders set padding/radius but **no explicit background, border thickness or brush**. Theme Border brush alone does not establish a painted card border. Local text font sizes/weights override theme defaults; theme TextBlock margins still apply. No specific theme selector for MainProgressBar or CurrentModProgress appears in the inspected theme files.

| Theme | Text / inherited layout | Window / UserControl | Progress specifics |
|---|---|---|---|
| Light | TextBlock black,#000000; default13,Normal,Wrap,margin0,3,0,2; Border brush#D0D0D0 | #F5F5F5 window and UserControl | generic ProgressBar track#E1E1E1 fill#0078D4; height20 overridden by local12/6 |
| K1 | TextBlock#3AAAFF; default14,Normal,Wrap,margin0,5,0,2; Border#3AAAFF | Window#000116 | named OverallProgressBar track#062766 does **not** match MainProgressBar; download-progress class rules also do not apply |
| K2 | TextBlock#18AE88; same default margin/fontsizes; Border#18AE88 | Window#02110D | named OverallProgressBar track#0D5945 likewise not this page |

Relevant selectors: Light TextBlock/Border near lines30-47, ProgressBar537-545; K1/K2 TextBlock30-36, Border46-48, named bar495-499. TextBox font rules Segoe UI/Verdana are not global TextBlock font assignments. Do not infer exact text family or K1/K2 main-bar colors from unrelated selectors: inspect actual inherited theme/native rendering. No responsive XAML breakpoint; smaller windows retain columns and wrap/scroll. High DPI, emoji/font fallback, overflow and fixed-count/right-operation wrapping require native evidence.

## Existing binary reuse: verified without compiler

Existing reference directory exists: `/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync/debug_net9.0`. Independently rehashed against `docs/parity/validation-reference/receipt.json`:

- ModSync.dll `fbd5164771bceb732018b47215490d94f7e8944689c86c015bbf14819c2bfd2d` — match.
- ModSync.Core.dll `89d9b2361aee4364f95151b41db59a4133122a6ad41bc2703e232cf10a4f5a6b` — match.
- Resources/holopatcher `fdd470570a82df58be8913074133ab13ba94a874e69d66b0592ebe3f5af230c5` — match.

Pinned source HEAD verified; binary reuse is feasible without rebuilding. This checks key artifact continuity, not every dependency or new runtime launch. Rehash the actually executed artifacts when a runtime owner is assigned. Do not alter shared reference artifacts, existing configs, HOME or user displays. No compiler reservation follows from this audit.

## Disposable native reference plan (future authorized runtime owner)

Allocate a new private fixture and XDG config/data/cache/runtime plus owned Xvfb via displayfd. Reuse existing DLL read-only with `--instructionFile=<fixture/guide.json> --kotorPath=<fixture/game> --modDirectory=<fixture/mods> --allow-multiple-instances`; record process/DLL hashes and explicit settings/checkpoint/managed-deployment flags. Use synthetic swkotor.exe/dialog.tlk markers and empty Override; never use or copy a user game tree. Bind guide/source bytes and baseline directory manifests. Capture before validation, after validation, after installation, including expected adjacent checkpoint/state paths.

| State | Reproducible route / boundary |
|---|---|
| Clean terminal | One small real Copy component, required directories and source present; normal load/select/validate/review/Next. Verify exact copied bytes and actual checkpoint artifacts, not text alone. |
| Observed initialization/in-flight | Multiple real Copy components and a bounded set of moderately sized/generated disposable files; retain guide order. Capture initial snapshot, component callbacks and checkpoint messages with screen recording/log timestamps. Tune file count only in owned fixture; do not invent percentages or add delays to source. |
| Returned non-success | Validate a present Copy source, then remove only that owned source before starting install. Record real service result/modal and whether page wrongly displays100%. If input invalidation prevents route, report it; no control bypass or injected result. |
| Checkpoint disabled | Separate fresh fixture/config with actual supported --no-checkpoint option, after verifying parser/preload acceptance; compare misleading static label against absence of real checkpoint creation. Never treat flag recognition as already measured. |
| Cancellation | First establish whether the active host exposes any install-cancel route. Download Stop is not install Stop. If none, record active native Stop unavailable; old modal/CLI cancellation probes are separate surfaces, not equivalent page proof. Never kill a shared/user process to force a screenshot. |
| Hard exception/checkpoint warning | Only if a reproducible real fixture failure can reach the page through ordinary controls; file permission or destination collisions must be confined to owned roots. Label unachieved states unproven rather than adding hooks/test harnesses. |
| Theme/responsive | Light/K1/K2 at1280×800 and600×400, record client geometry/scale and visible navigation, whole-page scroll, title/count wrapping and real progress states. No reference screenshot fabrication. |

The existing core native Copy fallback must survive if richer cases are blocked. No Rust test files, inline modules, temporary test code or custom Rust harnesses may be added under the user freeze. Native C# installs are future work requiring their own assigned session; this document has not launched any.

## Source binding

- `src/ModSync.GUI/Dialogs/WizardPages/InstallingPage.axaml`: `30a7c305b80517a4f13769ab57970f6a4377478d0f766786074fe51ba4e2632f`
- `src/ModSync.GUI/Dialogs/WizardPages/InstallingPage.axaml.cs`: `33556174fee7c465d6009a8eca66164ed18bf0402c3a60424aec24999d92e5db`
- `src/ModSync.GUI/Controls/WizardHostControl.axaml.cs`: `f72cebf61e3dca379710c58f3186e201778032310ee047a215d4c9d4fa4a977b`
- `src/ModSync.Core/Services/InstallationService.cs`: `3989b0e267edcd80eda4a1f8aea9b9c349a5322fbdac6dbd34b8d9651d713230`
- `src/ModSync.Core/Services/InstallationCoordinatorService.cs`: `5b42663b2bf1755994e2ee89698db2f83ab47a33791586d2678b5097e1d7975b`
- `src/ModSync.Core/Installation/InstallCoordinator.cs`: `ee61b77906fc2b4d20bc1c79c4aab58f71ff22d295a11d502f2aff6c3786563c`
