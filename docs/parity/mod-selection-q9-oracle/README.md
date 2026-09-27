# Q9 oracle: does C# Mod Selection cascade dependencies and restrictions?

Captured 2026-09-27 against pinned C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`. The run used the existing validation-reference build without compiling. `ModSync.dll` and `ModSync.Core.dll` hashes match [the validation reference](../validation-reference/receipt.json). [receipt.json](receipt.json) holds the commands, hashes, PIDs and cleanup record.

## Answer

It depends on whether the hidden editor sidebar has realized a `ModListItem` for the mod you click.

| | Run A: sidebar items realized first | Run B: straight into wizard (control) |
|---|---|---|
| Tick a mod with a dependency | **Cascades.** The dependency is selected. | **No cascade.** |
| Untick a dependency | **Cascades.** The dependent mod is deselected. | not exercised |
| Tick one side of a Restrictions pair (option level) | **Cascades.** The partner option is deselected. | **No cascade.** Both options stay selected. |
| Wizard display right after the click | Stale. The page count and the dependency checkbox do not update until the list is rebuilt. | Correct, since nothing else changed. |

The cascade is a side effect, not wizard behaviour. It fires only through the sidebar `ModListItem` TwoWay binding into `MainWindow.OnCheckBoxChanged`. Only realized sidebar items take part. In Run A, Deselect All cleared 24 selected mods but logged exactly one `[OnCheckBoxChanged] START`, and that one was for the single realized item, High-Poly Grenades (log line 20277). In Run B the sidebar never realized an item, and the whole session logged zero `[OnCheckBoxChanged]` lines.

## Fixture

- Guide: `KOTOR1_Full.toml` copied unchanged to a disposable dir per run (sha256 `7a4417963c62ec8205b1d219ecad10ea6cd3419897face92df04ea1aa8aecd3d`).
- Game dir: empty `swkotor.exe` and `dialog.tlk`. Mod dir: empty. There were no downloads, validation runs or installs.
- Dependency target: **Grenades and Mines HD** (TOML index 124), `Dependencies = [High-Poly Grenades]`. High-Poly Grenades has no dependencies of its own.
- Restrictions target: the guide has **no component-level Restrictions pair**. All 21 `Restrictions =` lines sit under `[[thisMod.Options]]`. The pair used is **KOTOR Dialogue Fixes → Standard / Revised**. Those two options restrict each other, and Revised is selected by default.

## Run A: realize the sidebar, then enter the wizard

1. Launch with `--instructionFile/--kotorPath/--modDirectory`. Decline telemetry ([00-launch](run-a/00-launch.png), [01](run-a/01-after-telemetry.png)). The app opens in the wizard ([03-loaded](run-a/03-loaded.png)).
2. Switch Wizard Mode off to reach the main view, where the sidebar lists the guide ([04-main-view](run-a/04-main-view.png), 189 mods and 24 selected). **Route note:** the main view was reached by toggling out of the wizard. The guide was not opened through a file picker.
3. Filter the sidebar with `Grenade`, so the High-Poly Grenades and Grenades and Mines HD items are realized ([05](run-a/05-sidebar-grenade-realized.png)). The sidebar is virtualized. Scrolling realizes items, and realizing a selected item already fires `OnCheckBoxChanged` (see the log after the toggle-to-main marker).
4. Switch Wizard Mode on and click Next ×4 to reach Mod Selection, which shows 24 of 189 ([08](run-a/08-modselection-initial.png)).
5. **Deselect All** gives 0 of 189 ([09](run-a/09-after-deselect-all.png)). The same click also fired the sidebar handler for the realized item. Log line 20277: `ModComponent: 'High-Poly Grenades' … IsChecked=False`, then `Unchecking component`.
6. Search `Grenade` in the wizard and click Expand All. Both mods show unticked ([12](run-a/12-before-tick-dep.png)).
7. **Tick Grenades and Mines HD.**
   - Right after the click the page shows **1 of 189**, and High-Poly Grenades still shows unticked ([14](run-a/14-after-tick-dep-5s.png), [15](run-a/15-after-tick-dep-count-stale.png)).
   - The log shows the cascade. Line 20284 has `ModComponent: 'Grenades and Mines HD' … IsChecked=True`. Line 20291 has `ModComponent: 'High-Poly Grenades' … IsChecked=True` / `Checking component 'High-Poly Grenades'`.
   - Both senders' visual path is `Grid('MainModInfo') -> … -> Border('RootBorder')`. Those names belong to `ModListItem.axaml:36,44`, the sidebar control, not the wizard page.
   - Rebuild the list by appending and then deleting one character in the search box. The page then shows **2 of 189**, with High-Poly Grenades ticked ([16](run-a/16-after-tick-dep-rebuilt-count.png), [17](run-a/17-after-tick-dep-rebuilt-list.png)).
8. **Untick High-Poly Grenades** ([18](run-a/18-before-untick-parent.png)).
   - Log lines 20298 and 20305: High-Poly Grenades goes `IsChecked=False`, then `Grenades and Mines HD … IsChecked=False` / `Unchecking component`.
   - The stale page shows 1 of 189 ([20](run-a/20-after-untick-count-stale.png)). After the rebuild it shows **0 of 189**, and Grenades and Mines HD is unticked ([21](run-a/21-after-untick-rebuilt.png), [22](run-a/22-after-untick-rebuilt-count.png)).
9. For the restriction leg, return to the main view and filter the sidebar with `Dialogue Fixes` ([24](run-a/24-sidebar-dialogue-realized.png)).
   - **Side effect:** realizing that sidebar item re-selected KOTOR Dialogue Fixes, which Deselect All had cleared. The Revised option was still `IsSelected`, so the realized option checkbox fired `HandleOptionChecked` and that re-checked the parent. The sidebar count went from 0 to 1 selected with no user click on that mod.
   - Re-enter the wizard and search `Dialogue Fixes`. The page shows the component ticked, Standard unticked and Revised ticked ([27](run-a/27-before-tick-standard.png)).
10. **Tick Standard.**
    - Log line 20911: `[ComponentSelectionService] Option 'Standard' conflicts with component 'Revised', deselecting it`. Line 20916 has `Option: 'Revised' … IsChecked=False`. The sender path runs through `ItemsControl('OptionsContainer')`, which is `ModListItem.axaml:218`.
    - The stale page shows both ticked ([29](run-a/29-after-tick-standard-5s.png)). After the rebuild, Standard is ticked and **Revised is unticked** ([31](run-a/31-after-tick-standard-rebuilt-options.png)). The count stays 1 of 189, because this is an option-level change ([30](run-a/30-after-tick-standard-rebuilt-count.png)).

## Run B: launch straight into the wizard (control)

Run B used the same launch arguments and a fresh private XDG/bus. It never visited the main view.

1. Decline telemetry ([00](run-b/00-telemetry.png)), reach the Welcome page ([01](run-b/01-loaded.png)), then Next ×4 to Mod Selection, which shows 24 of 189 ([02](run-b/02-modselection-initial.png)).
2. **Deselect All** gives 0 of 189 ([03](run-b/03-after-deselect-all.png)).
3. Search `Grenade`, then Expand All ([04](run-b/04-before-tick-dep.png)).
4. **Tick Grenades and Mines HD.** The page shows 1 of 189 ([07](run-b/07-after-tick-dep-count.png)). After the same search-edit rebuild it still shows **1 of 189**, and High-Poly Grenades stays **unticked** ([08](run-b/08-after-tick-dep-rebuilt-count.png), [09](run-b/09-after-tick-dep-rebuilt-list.png)). **No dependency cascade.**
5. Search `Dialogue Fixes`. The component is unticked, Standard unticked and Revised ticked ([10](run-b/10-before-tick-standard.png)). Tick the component to match Run A's precondition ([11](run-b/11-after-tick-component.png)), then **tick Standard**. After the rebuild, **Standard and Revised are both ticked** and the page shows 2 of 189 ([13](run-b/13-after-tick-standard-rebuilt-count.png), [14](run-b/14-after-tick-standard-rebuilt-options.png)). **No restriction cascade.**
6. Across the whole Run B log, `grep -c OnCheckBoxChanged` = 0 and `grep -c ComponentSelectionService` = 0 ([run-b/app-trimmed.log](run-b/app-trimmed.log)).

## Logs

The full app logs (about 21k and 20k lines of verbose stdout) are not copied. Their line counts and sha256 are in the trimmed-log headers and in receipt.json.

- [run-a/app-trimmed.log](run-a/app-trimmed.log) keeps every `OnCheckBoxChanged`, `ComponentSelectionService` and `NavigateToPage] COMPLETE` line with its original line number. It also lists the log line count taken before each action, so every event can be tied to the click that caused it.
- [run-b/app-trimmed.log](run-b/app-trimmed.log) has the same filter and is empty apart from navigation.

## Deviations and limits

- **Input and capture:** `xdotool` is not installed on this host. Input used python-xlib XTEST (`uvx --with python-xlib`, python-xlib 0.33) on the private display only. Screenshots used `ffmpeg -f x11grab`.
- **Runtime host:** the host from the earlier reference (`3fd639f3…`) was not on PATH. This run used `~/.dotnet-sandbox/dotnet` (sha256 `f0c9838e…`, Microsoft.NETCore.App 9.0.19). The app DLLs are identical to the reference.
- **Display:** both runs used private Xvfb `:57` at 1280×900, run one after the other with no window manager. The client sat at +10,+10, so the right edge is clipped in captures, as in earlier references.
- **"Rebuild" method:** typing one character into the wizard search and deleting it again, which calls the page's list rebuild and recount. Screens right after each click are kept alongside the rebuilt screens.
- **Restrictions:** only option-level Restrictions could be exercised with this guide. A component-level Restrictions pair was not tested, and no derived guide was used.
- Run B did not exercise the untick direction, because its dependency was never selected by cascade.
- Run A's restriction precondition came from the realization side effect in step 9. Run B's came from an explicit component tick. In both runs the component, Standard=off and Revised=on were in place before Standard was ticked.
- No tests, compilation, repo edits or instrumentation. Telemetry was declined in both runs.
