# Active C# wizard reference run

Reference commit: `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Native Linux X11 run on Xvfb `:913`, screen 1280x900, with the reference MainWindow at its default 1280x800 size. Screenshots include the desktop background outside the window. This is reference evidence, not a Rust equivalence claim.

Built `src/ModSync.GUI/ModSync.csproj`, Debug/net9.0, with artifacts under `/tmp/modsync-gui-reference-build`. Build completed with 0 errors and 20 existing warnings. Launched the built DLL with `--instructionFile`, `--kotorPath`, and `--modDirectory`; the guide was `../validation-pipeline-oracle/guide.json`. All game/mod paths were disposable under `/tmp/modsync-wizard-reference-proof`. A zero-byte `swkotor.exe` marker made the game directory detectable; this was not a real game installation. Telemetry was declined.

The run used actual mouse input through xdotool. It traversed Welcome, Before You Begin, Mod Workspace Directory, KOTOR Game Directory, Mod Selection, Download Process, Validation, Ready to Install, and Installing Mods. The active app embeds `Controls/WizardHostControl`; the older `InstallWizardDialog` is not the active shell. There is no table-of-contents sidebar, and a download sidebar appears from Download Process onward.

Initial validation reported the missing bundled HoloPatcher. The repository helper `scripts/agents/ensure_linux_holopatcher.sh` linked the bundled Linux executable into the disposable build output. The next full validation succeeded and left the game directory with only the original zero-byte marker. The subsequent one-component Copy installation produced `Override/sample.tga`, whose SHA-256 matched the source: `eb3a5fb11d4b50febbbb25d373c55fe6bdcc760640a18ce3c01e3f92ab4ef7be`.

## Observed reference defects and unresolved differences

- After the successful install, the page displayed 100%, 1/1 mods installed, and Installation complete, but Continue remained disabled. Clicking it did not advance from Step 9 of 11. The Rust implementation must refresh Continue from the successful worker result rather than reproduce this dead end.
- The metadata-free JSON fixture inherited the nonblank default `MainConfig.PreambleContent`, producing eleven pages. The TOML parser instead clears missing metadata. Rust's typed route currently consumes the guide's explicit metadata; format-specific defaults/global-state behavior remains a shared-loader parity gap.
- The preamble screenshot has a blank content area despite the included route. It proves the observed route, not successful markdown rendering.
- No widescreen route, completion pages, theme variants, keyboard traversal, screen-reader behavior, or real mod build was exercised in this reference run.

The screenshot names identify the observed screen. `reference-validation-missing-patcher.png` is the first failed validation; `reference-validation-success.png` follows the bundled-patcher setup. `reference-continue-disabled.png` was captured after clicking the disabled Continue button following the successful Copy install.
