# Pinned C# completion-page oracle: Base, Widescreen and Finished pages

2026-09-27 source audit. Oracle checkout `/home/brunner56/.codex/worktrees/5813/ModSync`, HEAD `d31ccbd3bfcddb4705917eafa6483715c166de3b`. No builds, tests, native launches, product changes or commits were performed. Everything below comes from reading source. None of it is new native acceptance. The user freeze on creating, porting, expanding or running Rust tests still applies. Paths are relative to `src/ModSync.GUI` unless they start with `src/`.

## Call graph and where the page data comes from

1. The active host builds the base route once in `Controls/WizardHostControl.axaml.cs:124-184`. Line176 constructs **`new BaseInstallCompletePage(0, TimeSpan.Zero, 0, 0)`** before any install has run. Line181 appends `FinishedPage`. The old `Dialogs/InstallWizardDialog.axaml.cs` does the same thing: Base with zeros at :122, Finished at :127, and the WidescreenComplete insert at :169. No other call site passes real values (grep of `BaseInstallCompletePage(`), and no setter exists. **Every statistic on the Base page is therefore a constant.**
2. The Next handler (`WizardHostControl.axaml.cs:813-845`) runs `AddWidescreenPages()` (lines187-217) when the current page is `BaseInstallCompletePage` and `_hasWidescreenMods` is false. That method detects `WidescreenOnly` components and inserts, before Finished, an optional `WidescreenNoticePage(widescreenWarningContent)` (only when the content is nonblank), then `WidescreenModSelectionPage`, `WidescreenInstallingPage` and `WidescreenCompletePage`. When there are no widescreen components nothing is inserted and Next goes to Finished.
3. `Finish` (`WizardHostControl.axaml.cs:871-875`) sets `InstallationCompleted = true` and raises `WizardCompleted`. `MainWindow.axaml.cs:7711-7715` handles that by calling `ExitWizardMode()` (starts at 7676), which unsubscribes the events and calls `wizardHost.Cleanup()` (927-953). Cleanup cancels the CTS, clears page content and resets download status. The loaded guide is not unloaded.
4. The only dynamic data on any completion page is the **managed-deployment summary**. Base (`BaseInstallCompletePage.axaml.cs:53-57`), WidescreenComplete (`WidescreenCompletePage.axaml.cs:29-33`) and Finished (`FinishedPage.axaml.cs:31-35`) all call `ManagedDeploymentUiHelper.TryApplySummary` (`Services/ManagedDeploymentUiHelper.cs:16-33`) when they are shown. It formats the **process-global static** `InstallationService.LastManagedInstallResult` (`src/ModSync.Core/Services/InstallationService.cs:36-37`). A `finally` block overwrites that value at the end of every managed-session call, on success or failure (lines75-85). The summary text comes from `ManagedInstallSummaryFormatter.FormatWizardSummary` (`src/ModSync.Core/Services/Installation/ManagedInstallSummaryFormatter.cs:16-44`). It is empty and hidden unless `ManagedModeUsed`, and `ModSyncSettings.ManagedDeploymentEnabled` defaults to false (`src/ModSync.Core/Services/Settings/ModSyncSettings.cs:29`).
5. `epilogueContent` is parsed from Markdown (`src/ModSync.Core/Parsing/MarkdownParser.cs:269-284`) and stored (`Services/FileLoadingService.cs:414`). The only GUI consumer is Markdown export (`MainWindow.axaml.cs:4216-4221`). **No completion page displays the epilogue.** `MainConfig.cs:312` also defines a nonempty default epilogue (launch options and troubleshooting), and the wizard never shows it either.

## Host chrome on completion pages

The following comes from `WizardHostControl.axaml.cs:700-786` and `Controls/WizardHostControl.axaml:103-186`:

- The title and subtitle come from the page. The step label is `Step {index+1} of {_pages.Count}`, and the wizard progress bar max equals the page count (714-716). Both are renumbered after widescreen pages are inserted.
- Back is enabled only when `pageIndex > 0 && CanNavigateBack`. Every completion page returns `CanNavigateBack => false`.
- `NextButton.IsVisible = pageIndex < Count-1` and `FinishButton.IsVisible = pageIndex == Count-1`. Next is labelled `Next →`, except `Continue` on the two installing pages. Finish is labelled `Finish`, has MinWidth100 and padding12,8, and stays enabled (no gate). Next is enabled from `CanNavigateForward`, which is true by default (`WizardPageBase.cs:23`). Finished overrides it to false, but Next is hidden there anyway.
- The download status bar and sidebar stay visible on every page from `DownloadsExplainPage` onward, completion pages included (182, 701-706). That contract belongs to the download-orchestration slice and has been forwarded to it.
- The host content area already wraps the page in a vertical ScrollViewer with padding16 (axaml103-108). Each page adds its own ScrollViewer inside it, so pages are nested in two scroll viewers.
- Main window: 1280×800, minimum 600×400 (`MainWindow.axaml:15-18`).

## BaseInstallCompletePage — exact composition

`Dialogs/WizardPages/BaseInstallCompletePage.axaml:9-244`. The outer ScrollViewer has horizontal scrolling Disabled and vertical Auto. The grid has margin24,16 and rows `Auto,Auto,*,Auto`. There is no responsive breakpoint.

| Region / lines | Controls, measurements and copy |
|---|---|
| Success header, 12-29 | Border row0, padding32,24, radius16, bottom margin24. Centered stack, spacing16: `✅` at 64pt; `Base Installation Complete!` at 32pt Bold; `Your base KOTOR mods have been successfully installed` at 16pt, opacity .8. All centered. |
| Statistics, 32-96 | Grid row1, columns `*,16,*,16,*,16,*`, bottom margin24. Four Borders, each padding24,20 and radius12. Each holds a centered stack, spacing8, with a label at 12pt opacity .6 and a value at 36pt Bold. Labels and XAML defaults: `MODS INSTALLED` `0` (`ModsInstalledText`); `CHECKPOINTS` `0` (`CheckpointsCreatedText`); `TIME ELAPSED` `--:--` (`TimeElapsedText`); `FILES MODIFIED` `0` (`FilesModifiedText`). |
| Next steps grid, 99-232 | Row2, columns `*,16,*`, rows `*,16,Auto`. |
| What's Next, 101-139 | Row0 col0, padding24,20, radius12, VerticalAlignment Stretch. Stack spacing16. Heading `📋 What's Next?` at 20pt Bold. Item stack spacing12, each item a stack with spacing6: title at 14pt SemiBold, body at 13pt opacity .8 with Wrap. Items: `1. Widescreen Support` / `If you're playing on a widescreen monitor, continue to install widescreen-specific mods`; `2. Test Your Game` / `Launch KOTOR and verify everything is working correctly before starting a new playthrough`; `3. Save Your Progress` / `Your installation is checkpointed - you can always roll back if needed`. |
| Important Notes, 142-181 | Row0 col2, same border. Heading `⚠️ Important Notes` at 20pt Bold. Stack spacing12. Bullets at 13pt, opacity .8, Wrap: `• Start a NEW GAME - Don't load old save files`; `• Check the log files if you encounter any issues`; `• Join our community for support and discussion`; `• Remember to credit mod authors if you share content`. After them, hidden `ManagedDeploymentSummaryText` at 13pt with Wrap and no opacity. |
| Quick Actions, 184-231 | Row2 spanning all columns, padding24,20, radius12. Stack spacing12. Heading `🚀 Quick Actions` at 18pt SemiBold. A 2×2 grid `*,*`. Each button stretches horizontally with padding16,12. Row0 margins 0,0,4,0 and 4,0,0,0. Row1 margins 0,8,4,0 and 4,8,0,0. Buttons: `📄 View Logs`, `🔄 Manage Checkpoints`, `📁 Open Game Folder`, `📦 Open Mod Folder`. |
| Footer, 235-242 | Row3, padding16,12, radius8, top margin24. Text at 13pt, opacity .7, centered, Wrap: `Click 'Next' to continue with widescreen-specific mods (if applicable), or 'Finish' to exit`. |

Code-behind (`BaseInstallCompletePage.axaml.cs`):

- Title `Base Installation Complete`. Subtitle `The base mod installation has finished successfully` (48-49). Back and Cancel are false (50-51). Validate always passes (60-61).
- Statistics formatting (77-100): mods is `int.ToString()`. Checkpoints is `int.ToString()`. Time is `{(int)TotalMinutes}:{Seconds:D2}` when elapsed is at least one minute, otherwise `{Seconds}s`. Files is the number when above 0, otherwise **`N/A`**. With the constructor values fixed at line176, the page always renders **`0` / `0` / `0s` / `N/A`**. The XAML defaults `--:--` and `0` are replaced before the page is ever seen.
- `View Logs` (125-147) opens `AppDomain.BaseDirectory/logs` through shell-execute when that directory exists. Otherwise it only writes a warning to the log.
- `Open Game Folder` (149-170) opens `MainConfig.DestinationPath`. `Open Mod Folder` (172-193) opens `MainConfig.SourcePath`. When a directory is missing, both only log a warning, and neither shows any UI feedback.
- `Manage Checkpoints` (195-221) opens the modal `GitCheckpointBrowserDialog(DestinationPath)` (`Dialogs/GitCheckpointBrowserDialog.axaml(.cs)`: title `Checkpoint Browser`, history list, details, `🔄 Restore to Selected Checkpoint` at .cs:126-129 and 269-314). It is a whole Git-history restore browser.

## WidescreenNoticePage

`WidescreenNoticePage.axaml:9-16`: grid with margin16,12 and rows `Auto,*`. Heading TextBlock `🖥️ Widescreen Mods`, bottom margin12, theme default size. Border row1 with MinHeight100, padding12, radius8, containing a named ScrollViewer (horizontal Disabled, vertical Auto) and `ContentText` with Wrap. The code sets `ContentText.Text = widescreenWarningContent` **raw**: the Markdown/admonition source text is not rendered (`.axaml.cs:42-48`). Title `Widescreen Support`. Subtitle `Information about widescreen mod installation`. No Back, no Cancel, Validate always passes. This page has no outer page ScrollViewer. The separate modal `Dialogs/WidescreenNotificationDialog` is used only by the editor-mode path (`MainWindow.axaml.cs:7313-7338`), not by the wizard.

## WidescreenCompletePage

`WidescreenCompletePage.axaml:6-18`: ScrollViewer, then a StackPanel with margin16,12, centered, spacing12. `✅ Widescreen Installation Complete!` (centered, Wrap). `All widescreen mods have been successfully installed.` (centered, Wrap). Hidden managed summary, MaxWidth600, centered, Wrap. **No font sizes, cards, statistics or actions**: the text takes theme defaults (Light TextBlock 13 Normal; K1/K2 14, per the theme table in [install-progress-csharp-audit.md](install-progress-csharp-audit.md)). Title `Widescreen Installation Complete`. Subtitle `All widescreen mods have been installed`. No Back, no Cancel. Next is visible (Finished follows).

## FinishedPage

`FinishedPage.axaml:6-34`: ScrollViewer, then a StackPanel with margin16,12, centered, spacing14. No explicit font sizes anywhere.

1. `🎉 Installation Complete!` (centered)
2. `All mods have been successfully installed.` (centered, Wrap)
3. hidden `ManagedDeploymentSummaryText`, MaxWidth600
4. Border MaxWidth600, padding16,14, radius8, stack spacing8: `🎮 Next Steps:` (centered); `• Launch the game and enjoy your modded experience!`; `• If you encounter issues, you can rollback to previous checkpoints.`; `• Check the output log for detailed installation information.` (Wrap)
5. Border MaxWidth500, padding14,12, radius8, stack spacing6: `💝 Support the Project`; `If you found ModSync helpful, consider supporting the project!`; `Visit our sponsor page to learn more.` at opacity .8. No link or button exists.
6. `Click 'Finish' to close the wizard.` at opacity .7

Title `Installation Complete`. Subtitle `Thank you for using ModSync!`. Back, Forward and Cancel are all false (`FinishedPage.axaml.cs:21-29`).

## Borders and themes

As on the Installing page, the completion Borders set padding and radius but **no background, brush or thickness**. Whether a card appears painted depends on the theme's Border rule (Light `#D0D0D0`; K1 `#3AAAFF`; K2 `#18AE88`, per the installing audit's theme table), and that needs native confirmation. Emoji glyph fallback (✅ 📋 ⚠️ 🚀 📄 🔄 📁 📦 🎉 🎮 💝 🖥️) also needs native evidence. Earlier Rust native receipts already record missing emoji glyphs (`native-selection/receipt.json`).

## C# defects (source-proven; not parity targets)

| # | Defect | Referent |
|---|---|---|
| D1 | Base statistics are constants: always `0` mods, `0` checkpoints, `0s`, `N/A`. They are never bound to the run. | `WizardHostControl.axaml.cs:176`; `BaseInstallCompletePage.axaml.cs:77-100` |
| D2 | The Base page says "successfully installed" even when the Installing page reached it after a returned non-success exit code, which that page renders as 100% and forward-ready. | `BaseInstallCompletePage.axaml:26`, `.axaml.cs:49`; [install-progress-csharp-audit.md](install-progress-csharp-audit.md) "any returned exit code" |
| D3 | The footer tells the user to click `'Finish' to exit`, but Finish is hidden on the Base page. Only Next is shown there. | `BaseInstallCompletePage.axaml:239`; `WizardHostControl.axaml.cs:774-775` |
| D4 | `View Logs` opens `<basedir>/logs`, but NLog writes to `<basedir>/Logs/kotormodsync_<date>.log`. On a case-sensitive filesystem (Linux) the button only logs "Log directory not found" and shows nothing. | `BaseInstallCompletePage.axaml.cs:129`; `src/ModSync.Core/NLog.config:12` |
| D5 | Folder and log buttons report failure only to the log. The user sees no feedback. | `.axaml.cs:138-146,161-169,184-192` |
| D6 | WidescreenInstalling treats cancellation as success. The loop `break`s on a cancel token, then sets `_installationComplete = true` and shows "Widescreen installation complete!", which leads to "All widescreen mods have been successfully installed." | `WidescreenInstallingPage.axaml.cs:90-93,129-141`; `WidescreenCompletePage.axaml:9` |
| D7 | With zero widescreen mods selected the loop runs zero times, and WidescreenComplete and Finished still claim that all widescreen mods / all mods were installed. | `WidescreenInstallingPage.axaml.cs:86-129`; `WidescreenCompletePage.axaml:9`; `FinishedPage.axaml:9` |
| D8 | The managed summary is a process-global "last session". Each widescreen component opens its own managed session (`InstallSingleComponentAsync` → `RunWithManagedInstallSessionAsync`, `src/ModSync.Core/Services/InstallationService.cs:1079-1099`), so Finished shows only the **last widescreen component's** manifests and not the whole run. A failed session also overwrites it. | `InstallationService.cs:75-85`; `ManagedDeploymentUiHelper.cs:23` |
| D9 | Copy promises rollback: "you can always roll back if needed" and "rollback to previous checkpoints". Whether a checkpoint exists depends on the no-checkpoint flag, Git and snapshot settings, and the Installing page's own checkpoint messages are unconditional (see the installing audit). | `BaseInstallCompletePage.axaml:134`; `FinishedPage.axaml:21` |
| D10 | "Join our community" and "Visit our sponsor page" have no link or action. | `BaseInstallCompletePage.axaml:167`; `FinishedPage.axaml:29` |
| D11 | The guide epilogue (post-install notes) is parsed and never shown on any completion page. | `MarkdownParser.cs:269-284`; `MainWindow.axaml.cs:4219` is the only consumer |
| D12 | The four equal statistic columns have 36pt values and 24px padding, and there is no breakpoint. At the 600×400 minimum the columns are about 100px wide or less, so clipping or wrapping is likely. This needs native proof. | `BaseInstallCompletePage.axaml:32-96` |
| D13 | Widescreen notice content is shown as raw Markdown/admonition source (`:::note` …), not rendered. | `WidescreenNoticePage.axaml.cs:46`; default at `src/ModSync.Core/MainConfig.cs:320` |

## Source binding (sha256)

- `Dialogs/WizardPages/BaseInstallCompletePage.axaml` `64add352b6aa44c889d2b4c3d223e489618b2be7115d40f1e5a1820361a22f44`
- `Dialogs/WizardPages/BaseInstallCompletePage.axaml.cs` `fd4b8b40e072201f9b33e3435b42e0b8ccdd27e4d0d5adab3368d9a0c144d1df`
- `Dialogs/WizardPages/FinishedPage.axaml` `4615c4b825afd192d14d3a882965d99ef308b1c963a51496ab6bf85d73a8033d`
- `Dialogs/WizardPages/FinishedPage.axaml.cs` `5495d504660dd6f01963ae746059abff1906741f43b28c522703bc3c0a419418`
- `Dialogs/WizardPages/WidescreenCompletePage.axaml` `ff77f1d113b3ebd0cd35b2ff154c0ec7c3623f328e2d345a61251724a9107a76`
- `Dialogs/WizardPages/WidescreenCompletePage.axaml.cs` `8f82ed183ab4a1b77a440e39cf99242417f9673fedbca941e111253c09e26152`
- `Dialogs/WizardPages/WidescreenNoticePage.axaml` `ba216ecd96efe084245a3b6ba78205286d78733911d189da1e5afa10c05cba23`
- `Dialogs/WizardPages/WidescreenNoticePage.axaml.cs` `de8ed041ee31d7a5a7858e135ab0ae00b70b24d6398c0e3febf3b674f290d8df`
- `Dialogs/WizardPages/WidescreenInstallingPage.axaml.cs` `ef7c1eab5e128da9457ccda123310c4b52fe83ca585919591a59eb028341f933`
- `Controls/WizardHostControl.axaml` `6687077f0ab308ee0d3aa7c880179f98d5bfc8209d465c9dfa3ca75a45e9b5db`
- `Controls/WizardHostControl.axaml.cs` `f72cebf61e3dca379710c58f3186e201778032310ee047a215d4c9d4fa4a977b` (same as the installing audit)
- `Services/ManagedDeploymentUiHelper.cs` `205f24624ec00452ac06d313f8f8fe163dc432c3b6dc76b6386050e5a0e2e02d`
- `src/ModSync.Core/Services/Installation/ManagedInstallSummaryFormatter.cs` `2e1354faacc23036dc0e0ea5973e62912323e4e702a552fd6ae86e1e3454c612`
- `src/ModSync.Core/NLog.config` `0d18997875668cce9f68f158ccd2bccc9b45b782489dd00d72b304f8afa7c9b5`
