# Native desktop route receipt

This directory records a disposable Rust native run against the active-host route
from C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`. It establishes functional route
integration for one two-component fixture, not exact C# GUI equivalence or a real
KOTOR mod-build installation.

`receipt.json` binds the source, binary, guide, captures, and file hashes.
`guide.json` is the exact loaded fixture; `activity.txt` was copied through the
native Copy report button; `completion.json` is the shared engine's final receipt.
`after-base.json` was captured after base installation and before remaining
validation. Initial hashes derive from the explicit fixture creation writes in the
agent transcript. All PNGs in this directory were recaptured in the current run;
none rely on the interrupted worker's inherited screenshots. The executable was
hashed through `/proc/<pid>/exe` before driving the final run and matched the
desktop build artifact. A previous run was repeated because the no-default test
had replaced the executable path before its hash was recorded; that run is not
the binary binding for these captures.

The fixture creates `swkotor.exe` solely for game detection, writes `base fixture`
and `wide fixture` as source bytes, then loads two selected components: a base Move
and a widescreen Copy. Native mouse input drove Welcome → directories → selection
→ downloads → full validation → review/confirmation → base installation → base
completion → widescreen notice/selection → remaining validation/confirmation →
widescreen installation/completion → Finished → loaded project. Start wizard
returned to Welcome with Step 1 of 11. Widescreen insertion changed the total to 15.

The base source was absent before remaining validation. The base output hash stayed
`db27e40baf145ac9d69e3304fbce46e43931a71078135b591723358563dee7ea`;
the final wide output hash is
`ec5c16eb005762e90a94a5858fb34c516fbf9e2b9812944b7c7d898d8de386d6`.
The copied activity has one base Move in base validation, one in base installation,
and no base Move in remaining validation/install. The engine receipt records both
component IDs. Together these establish no base replay for this native fixture.

Launch used a private Xvfb `:915`, 1280×800×24, `WAYLAND_DISPLAY` unset,
`WINIT_UNIX_BACKEND=x11`, and software OpenGL. Command:

```bash
env -u WAYLAND_DISPLAY DISPLAY=:915 WINIT_UNIX_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 \
  /tmp/modsync-rs-target/debug/modsync gui \
  --input /tmp/modsync-task2-native/guide.json \
  --game-dir /tmp/modsync-task2-native/game \
  --source-dir /tmp/modsync-task2-native/mods \
  --settings-dir /tmp/modsync-task2-native/settings --allow-multiple-instances
```

The minimum-size capture uses a 600×400 client window. Header/progress/navigation
remain visible, but the Activity/header/footer leave very little content height.
This is evidence of the accepted geometry and a remaining usability limitation,
not minimum-size visual parity. Default-size capture comparison, all themes,
keyboard/focus/accessibility, native cancellation/failure, network downloads,
provider acquisition, and full reference validation-card/report equivalence remain
open. Welcome download copy intentionally states the automatic queue is unavailable.

Required checks have captured output here: all-targets/all-features **212 passed,
1 ignored**; no-default-features **187 passed, 1 ignored**; strict all-targets
Clippy passed; format check passed. A successful format check has an empty log.
Focused desktop tests cover stale inputs, unknown game, failed worker, full selected
widescreen validation, consumed base source, remaining phase, empty widescreen
phase, replacement guide, and FIFO-safe metadata stamping. Test results support
those contracts only; they do not substitute for native proof of other states.
