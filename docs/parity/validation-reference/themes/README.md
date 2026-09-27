# Native C# validation themes

This extends the adjacent Light reference with actual KOTOR 1 and KOTOR 2 native
ValidatePage captures. It reuses the existing pinned C# build at commit
`d31ccbd3bfcddb4705917eafa6483715c166de3b`; no build or product edits were made.
It is presentation evidence for the pinned C# app, not Rust parity evidence.

The client window was verified as 1280×800 at (0,0), on an owned 1280×900 Xvfb
display allocated through `-displayfd` (display `:1`). Default 1× scaling was
used; this is not a high-DPI check. The process used private XDG config/data/cache/
runtime directories, a private DBus session, `XDG_SESSION_TYPE=x11`, no
`WAYLAND_DISPLAY`, and explicit `--allow-multiple-instances`.

The runtime log confirms settings resolved to
`/tmp/modsync-validation-themes-20260927-5813/fixture/config/ModSync/settings.json`.
`SettingsManager` uses ApplicationData/ModSync/settings.json, which this launch
redirected through XDG_CONFIG_HOME. Neither HOME nor CODEX_HOME was changed.
The active `WizardHostControl` K1/K2 buttons apply ThemeService and invalidate
visuals without reloading the page. These handlers do not themselves save
settings; any MainWindow persistence resolves the same isolated location.

## Native captures

| State | K1 | K2 |
|---|---|---|
| Warning, top controls | [K1 top](k1-warning-top-final.png) | [K2 top](k2-warning-top.png) |
| Warning card | [K1 card](k1-warning-card.png) | [K2 card](k2-warning-card.png) |
| Warning summary | [K1 summary](k1-warning-summary.png) | [K2 summary](k2-warning-summary.png) |
| Failure, top controls | [K1 top](k1-failure-top.png) | [K2 top](k2-failure-top.png) |
| Failure card and summary | [K1 failure](k1-failure-summary.png) | [K2 failure](k2-failure-summary.png) |

Both themes show 0 errors / 1 warning / 5 passed checks for the missing Copy
source. Restoring the source and removing only the owned runtime copy's patcher
symlink produces 1 error / 0 warnings / 0 passed checks. The patcher symlink was
restored afterward. One warning run occurred under K1, the failure run under K2,
and toggling themes retained the result. A second K1 warning run produced the
final stable warning captures.

K1 uses bright blue text for headings, descriptions, result cards and counters;
K2 uses green text throughout. This is not just a hyperlink/accent change.
Warning card borders remain orange and failure borders remain red, while their
text stays blue or green. Cards share the page background. The log expander
header is a solid colored panel. The active host still clips its nested content
and the download sidebar at this geometry; scrolling was needed to capture
cards and summary.

## Palette and geometry evidence

`pixel-palettes.json` records dominant exact RGB values read from the unedited
native screenshots. Source styles establish control tokens and hover states;
the screenshots establish the rendered colors for these states.

| Surface | K1 | K2 | Light source/reference |
|---|---|---|---|
| Page/window background | `#000116` | `#02110D` | `#FFFFFF` |
| Main foreground | `#3AAAFF` | `#18AE88` | `#000000` |
| Normal button fill | `#010116` | `#02110D` | `#F0F0F0` |
| Button border | `#062766`, 2px | Source `#C00D5945` / template `#0D5945`; observed blend `#094737`, 2px | `#ADADAD`, 1px |
| Log expander header | `#062766` | `#0A2A22` | `#F5F5F5` |
| Output panel | `#031332` | `#0A2A22` | White |
| Warning border | `#FB8C00` | `#FB8C00` | Same severity border |
| Failure border | `#E53935` | `#E53935` | Same severity border |

Dark theme default TextBlock/Menu font size is 14 with margin `0,5,0,2`, versus
Light's 13 and `0,3,0,2`. Dark default buttons use font size 14, padding 10 and
2px borders, versus Light's 13, padding `16,8` and 1px borders. ValidatePage's
explicit sizes still apply: heading 20, status 14, summary 16, counts 20, log 11.
In these captures the dark Run button begins around y340 compared with y331 in
the Light reference; the bottom output divider is around y634 versus y640.
These positions are observations for the captured state, not layout guarantees.

K1 button hover source uses `#A8B348` foreground/border and `#000116` fill.
K2 button hover source uses light foreground (`#F2F2F2`) and light border
(`#E0F2F1`). Pointer hover can produce a transient light gray log header through
the Fluent template; the stable final top captures move the pointer outside
the content so normal-theme colors are visible. Native theme repaint is
asynchronous; early stale frames were not retained as the final K2 captures.

## Fonts: source, fallback, and actual loaded files

The theme files declare Verdana for TextBox/ComboBox in K1/K2 and Segoe UI for
those controls in Light. They do **not** declare Verdana globally for all
TextBlocks. ValidatePage explicitly requests
`Consolas,Courier New,monospace` for its 11px log.

Host `fc-match` selected Noto Sans for Verdana, Inter Variable for Segoe UI,
and Noto Sans Mono for Consolas. These results alone are not proof of the fonts
Avalonia actually used.

`runtime-font-maps.json` supplies stronger, bounded runtime evidence: the native
C# process mapped these font files after rendering the three themes:

- Bitstream Vera regular and bold (`Vera.ttf`, `VeraBd.ttf`)
- DejaVu Sans
- Symbola
- Liberation Mono regular
- Font Awesome 6 Free regular

This identifies loaded files, not the precise font used for each control/glyph.
The magnifier, clipboard and issue symbols visibly render in the screenshots.
Host `fc-match` for U+1F50D and U+1F4CB lists Noto Sans Symbols 2 before Symbola;
Symbola was actually mapped in this C# process. No runtime Noto Sans Symbols 2
mapping was observed. Do not substitute fontconfig guesses for the mapped-font
or visual evidence.

## Input and safety record

1. Allocated private Xvfb and a separate hardlinked copy of immutable runtime
   files; patcher symlink changes affected only this copy, not the shared build.
2. A Homebrew DBus launch initially stalled on disk I/O. Switched to explicit
   `/usr/bin/dbus-run-session --dbus-daemon=/usr/bin/dbus-daemon`; terminated
   the transient owned first app before any navigation/theme actions.
3. Verified native app PID through `_NET_WM_PID`, settings path in its log, and
   1280×800 client geometry. Traversed the normal wizard with Next.
4. Clicked active wizard K1 (1199,87), ran warning validation, captured controls,
   cards and counters. Clicked K2 (1234,87), retained/captured that result.
5. Changed only disposable fixture source and private runtime patcher symlink,
   ran failure in K2, then toggled K1 and K2 to capture failure presentation.
6. Restored patcher, removed disposable source again, reran K1 warning and
   captured stable full warning card/top/summary.
7. Saved game/mod manifests, source/binary/runtime hashes, log and captures.
   Warning and failure checkpoint manifests showed no validation writes. No
   installation or download was started.
8. Stopped only the owned native app, Xvfb and associated private bus lifecycle.
   The owned app did not exit after SIGTERM/display closure and was finally
   stopped by SIGKILL. No user display or user-owned application was operated.

No clipboard test, exact issue scroll offset, splitter drag, keyboard coverage,
screen-reader behavior, minimum geometry, alternate DPI or other platform is
claimed by this theme extension. The prior Light package and receipt were not
modified.
