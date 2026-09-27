# Task 2b native evidence, round 2

- Commit: ac9cf3a (HEAD). Binary /tmp/modsync-2b-native/modsync sha256 9429089da7c9fd7e8d4b023d0aa46084dccea20e13b127d63873b6ba196dcce6, copied from /home/brunner56/.cache/modsync-rs-target/debug/modsync after the final default build.
- No Rust tests, inline tests or Rust harnesses were created, changed or run. Only production builds, clippy and the native GUI/CLI were used.
- Display: private Xvfb :93, root 1280x800x24, -dpi 96, -nolisten tcp, -noreset (pid 3180221; started with setsid). No window manager. App window 1280x800 at +0+0; 600x400 via xdotool windowsize. Scale 1:1, no high-DPI claim.
- Discarded: first Xvfb (pid 3161633, without -noreset) reset when its launching shell ended; that run is kept in discarded-light-small-xreset/ and makes no claim.
- Private HOME/XDG_*/dbus per fixture (see /tmp/modsync-2b-native/launch.sh). App launch argv: modsync gui --settings-dir R/settings --input R/guide.json --game-dir R/game --source-dir R/mods --allow-multiple-instances.
- Owned app process groups (all terminated with kill -TERM -PGID): light-small 3182446, k2-close-request 3190343, k2-cancel 3198314, k1-f1 3206229. Xvfb 3180221 terminated. After cleanup: no process with a modsync-2b-native environment; X93 socket gone.
- Exit codes: every scenario script exited 0; the G2 CLI install exited 1 (intended fixture failure), recorded in k1-f1/transcript.txt; the Xvfb/app processes were ended by TERM.
- Fixture generator: /tmp/modsync-2b-native/make_fixture.sh (empty swkotor.exe/dialog.tlk markers, Copy + Run steps; fail mode runs fail.sh exit 3). Scenario scripts: r2/lib.sh, r2/scenario-*.sh. Interaction transcripts with timestamps: r2/*/transcript.txt.

## Scenarios
- light-small (Light): 600x400 while active shows Stop Install in footer (10-small-active-stop-install.png); 600x400 after success shows Continue enabled (20-small-success-continue.png).
- k2-close-request (KOTOR2): a real WM_DELETE_WINDOW during the install (11-close-attempt.png) opens the Stop modal with pending-snapshot text at 67%/2-of-3. The install committed at elapsed 18s (~10:26:28) while the modal was open; the terminal cleared the modal. The later click at 10:26:31.187 hit empty space. Result stayed 100%/Complete (20-cancelled.png): a late Stop cannot rewrite committed success.
- k2-cancel (KOTOR2): install confirmed 10:27:25.199; Stop Install 10:27:28.594; modal confirm 10:27:29.303. Terminal elapsed froze at 00:00:04 (start + 4 s = ~10:27:29.2), inside Alpha Textures step 2 (Run), the external program loop polls the token every 100 ms. 0/3 installed, snapshot active (20-cancelled.png, 22-install-steps.png). Footer Restore: after-restore manifest equals before (RESTORE_EXACT).
- k1-f1 (KOTOR1): G2 got its own pending snapshot from the CLI (exit 1). G1 failed in the GUI at 33% (20-g1-failed.png). Folders repointed at G2 (31c-settings-g2.png); page shows the folders-changed notice (32-folders-changed.png). Footer Restore restored G1 exactly (after-restore == before), G2 bytes unchanged and G2 pending file still present, G1 pending gone (transcript). Page then says restored; Retry disabled while folders differ (40-restored-g1.png); enabled after setting them back (41-folders-back-retry.png).

## Earlier E2 explanation
In round 1 (final/k2) the modal was captured at 00:00:10, but the confirm click was issued in a separate tool call several seconds later. The install kept running while the modal was open, so the token was set around 16 s and the stop landed in the second Beta Patch (c3). The round-2 run clicks confirm 0.7 s after Stop and the stop lands within 0.1 s of the click.

## Hashes
```
f3bdcc59f1a08b8f599bc2d653ce1bdeaafc2bfdc23eaa6d15fe20b7bd6f5101  discarded-light-small-xreset/02b-trust.png
f0227871d8279433b1238a8a2cdf89f80b842128f8992bd6d36500444b04502b  discarded-light-small-xreset/02-trust.png
f3bdcc59f1a08b8f599bc2d653ce1bdeaafc2bfdc23eaa6d15fe20b7bd6f5101  discarded-light-small-xreset/06b-confirm.png
4a7abec0d91308ec13235c4854404bbb8805d0a6f5f3ea2ed9b3efc610abf748  discarded-light-small-xreset/06-confirm.png
f3bdcc59f1a08b8f599bc2d653ce1bdeaafc2bfdc23eaa6d15fe20b7bd6f5101  discarded-light-small-xreset/10-small-active-stop-install.png
f3bdcc59f1a08b8f599bc2d653ce1bdeaafc2bfdc23eaa6d15fe20b7bd6f5101  discarded-light-small-xreset/20-small-success-continue.png
f3bdcc59f1a08b8f599bc2d653ce1bdeaafc2bfdc23eaa6d15fe20b7bd6f5101  discarded-light-small-xreset/21-success-large.png
190397ee10e351e9c83c3e5b4d695b37b2768ba4c13ea58844ab9336283ae0d6  k1-f1/02-trust.png
f00c86a3433b08f441ade2230123269e1cfb4e5f2ae32ca485b4d0eff6d05bd7  k1-f1/06-confirm.png
c6864dc03e662ed20eb521618f6ec56cc76c84e304005d3021ff71ba8352b203  k1-f1/20-g1-failed.png
0696ff50695f98e220033299c19b223ae690789a9f58f44c86c7c059d0f7879f  k1-f1/30-settings.png
2cdc8fddf636da7c5a89914039977d72bb9570cdf9a4cac2d4f8fc5879188064  k1-f1/31b-typing.png
31ffdb063a611a28a6f5cdcae129efac2bd1f99b2526e219e193da69ed62685e  k1-f1/31c-settings-g2.png
d886914d3654a24c3d1935e0d5cb82f5b50ad1a994fbb4e30bd1f3c1ed0f9dca  k1-f1/31-settings-g2.png
abbddff02e0ca83827952d82188e916eda73540a15dd01352cbc3892d6d70df9  k1-f1/32-folders-changed.png
3aaebfe71436d29715b881a74649ca04b83f2ff53c9b4ed7566399763a7665f7  k1-f1/40-restored-g1.png
1110bf9cd0f65a1301fcf1d232de08827e521721442dc3b6381538f2c87c5f70  k1-f1/41-folders-back-retry.png
9de8ab90349349a4616f9e911e611ff9bd74d45d26005b76a93f590516761eb1  k2-cancel/02-trust.png
ec1cdf671b80d055ed31cb2ff33f2d6d094d47ed77c369bcaa92f9f5bda9c23e  k2-cancel/06-confirm.png
f5c50bb42b89a63e762909c42503ec3fe06a6cf28c05d9733d27513370879df5  k2-cancel/10-active.png
497ef361f396972c5ccfe0d906894788fe47707704dc90756d04e1be17636ba7  k2-cancel/11-stop-confirm.png
8778bab55aa67c1d4b8c954b1bd6c715e92b4654c837dc62cf7eb915255690f8  k2-cancel/20-cancelled.png
fe67b2366659dab4368dd7c3f99feabd2f827990b48f0b26ee31cdb4938e8500  k2-cancel/21-scrolled.png
4c3b2331859e04dfb5275b6f127c25cf881d02c23ce3b483c47bfb71023d9317  k2-cancel/22-install-steps.png
a59f62c9ece7a1a9e4dc8b44b19d82fbfa2540a24a3034a7ba1b95d5c3134db4  k2-cancel/30-restored.png
9de8ab90349349a4616f9e911e611ff9bd74d45d26005b76a93f590516761eb1  k2-close-request/02-trust.png
ec1cdf671b80d055ed31cb2ff33f2d6d094d47ed77c369bcaa92f9f5bda9c23e  k2-close-request/06-confirm.png
6d9c9a5dd91e3f53bc33064e8422aad7a64fbd88834ac24eb6660de5be3e8b57  k2-close-request/10-active.png
6221219cc5c52b0c67763373a02310617580c71c3af23e04aa0bfedfa7cc988e  k2-close-request/11-close-attempt.png
3614d1e754d0ba3acc4e0daf84f814f1cc808beb27286e001bc44c269b366581  k2-close-request/20-cancelled.png
4eb97affba1188959fcc069156d51c2c7984ac57f92df4d1b3028c622c403f73  k2-close-request/21-scrolled.png
c64258016db5336bcfe77ebddd570b2d06f5e6dcaddb67e1ac69451c82724321  light-small/02-trust.png
7e4509e7e57156a499e41c57901fc96c00067a424c6808d6e53c1564f4ab4a2d  light-small/06-confirm.png
d92e8a1dab99b5572a6e5af9794ad4bb0553733b78bb94cc37c69e006b2587f2  light-small/10-small-active-stop-install.png
c3ae56c9bbf1ca1606d2043f3ced3803c1e4efc043874d8cd17b21def4f461f5  light-small/20-small-success-continue.png
f030f721bf64d1fc001f76486c1954e3b81ffb39c14a666560698db9f6e6df5a  light-small/21-success-large.png
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  discarded-light-small-xreset/after.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  discarded-light-small-xreset/before.sha256
79bad9a0a209cee6e685c9e3051a58cdd4db6e6b8f5f5d9f37ff29070fcea69d  k1-f1/after-failure.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k1-f1/after-restore.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k1-f1/before.sha256
79bad9a0a209cee6e685c9e3051a58cdd4db6e6b8f5f5d9f37ff29070fcea69d  k1-f1/g2-after-fail.sha256
79bad9a0a209cee6e685c9e3051a58cdd4db6e6b8f5f5d9f37ff29070fcea69d  k1-f1/g2-after-restore.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k1-f1/g2-before.sha256
85b79c989476721a4eb0a21014a92b24b6b48c5509bbd70131b51384013b2f5e  k2-cancel/after-cancel.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k2-cancel/after-restore.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k2-cancel/before.sha256
9a79386919e17a9b6fe4b5485e036dbc0cee3d69a48f27acbf0620eb5d0e0c70  k2-close-request/after-cancel.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k2-close-request/before.sha256
9a79386919e17a9b6fe4b5485e036dbc0cee3d69a48f27acbf0620eb5d0e0c70  light-small/after.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  light-small/before.sha256
f1deb40ffb845138c3092bad4d686ce040919389a4e8ad4adbebf8d03eaadcd2  discarded-light-small-xreset/transcript.txt
3478b98108fec84f370659400bf1b74099e1083cd6ed3d9c8720e8bf0ed9d41b  k1-f1/transcript.txt
7f9a10d6662558adc71034f4cefdc986a1f173048d33314a21e1e0f99e9e1c89  k2-cancel/transcript.txt
ef63d947e34008fa17db2315b046d16969cded0cffe0221f1584af056bded697  k2-close-request/transcript.txt
6b348c7c63340ade002bb0c5ac2bb1ab2ed4122a7b0a976223e0d8c4ec9701e8  light-small/transcript.txt
78c17ad6fef728d7b6176f853215f8f3f0ec469aa27d0da5239c44111e44fa36  discarded-light-small-xreset/guide.json
d09e8571d44d1a42eb801cfbd8217ba059474c37a9b98f9b43ba6b7d517a877e  k1-f1-g2/guide.json
d09e8571d44d1a42eb801cfbd8217ba059474c37a9b98f9b43ba6b7d517a877e  k1-f1/guide.json
78c17ad6fef728d7b6176f853215f8f3f0ec469aa27d0da5239c44111e44fa36  k2-cancel/guide.json
78c17ad6fef728d7b6176f853215f8f3f0ec469aa27d0da5239c44111e44fa36  k2-close-request/guide.json
78c17ad6fef728d7b6176f853215f8f3f0ec469aa27d0da5239c44111e44fa36  light-small/guide.json
81d36a091df878499f9adf3471cc2acdccd57b1078bfee44961769fd279d6704  k1-f1/g2-cli.out
90e9d7d324994e261ac0b2a2e42fc282508a466ea1fb8d2802e31a501a4da43a  lib.sh
cba06da66f47e0dfc6c684c05141330c18d2cc95f9de33ba6c77ee7b8c8c6994  scenario-cancel2.sh
990081009f820dca7bd169240782bd0ef1bbbf616b75ee1504f987c9f7958545  scenario-cancel.sh
a1452c9a641036a44b93cebb179cfac29ed4b031ae11076dffaa8634328dcf30  scenario-f1.sh
da8ae4a4a07845d9bdc1806557032a55cb68d11a411fdf97911d79b167ee0fac  scenario-small.sh
477e4c5b8cb8787262be81add6e9a4e19e0dd94eec7dde1c1f490fff848e7d22  ../make_fixture.sh
fbc3289112b4cf018830eb4b1a5bf2177829aeffaac737fdd0698f3f912dcd6d  ../launch.sh
08a87e9c94664449b77acde97687a8446be9553aa8e255713c65fb1ac0fad38b  ../wm_delete.py
```

## Whole-package review fixes (#2, #9, #10)

- Commits: a24e96b (restore routing, Restart wizard, bind folders early), 9df6c3b (stale-success rate boundary).
- Binary for #2 and #10: f233685ffb7ec1de689b02f2fb15d399afa0cb2d81fb36acc3af39cbf7050026 (a24e96b). Binary for #9: 4c9d49d321da9e4003d565547f82e9e2bd8b8c973c813d06f322f95d1393dd6f (9df6c3b, final). 9df6c3b changes only the rate boundary on an Ok terminal, which #2 and #10 do not reach.
- Xvfb :93 pid 3602536 (setsid, -noreset, -dpi 96), terminated. App PGIDs terminated: k1-review2 3614850, light-review9 3760829, discarded-k1-review2-slow-start 3604333 (the window appeared after the scripted clicks; no claim), discarded-light-review9-rate-bug 3638482 (shows the rate defect fixed in 9df6c3b; no claim). The launcher now waits for the window.
- #2 (k1-review2): G2 gets its own pending snapshot from the CLI (exit 1); G1 fails in the GUI; the folders are repointed at G2 (30-settings-g2). The utility 'Restore failed install' hover names G1's game folder (32-utility-restore-hover). After clicking it, G1's manifest equals before (G1_RESTORE_EXACT), G2 is unchanged (G2_UNCHANGED), G2's pending file is still present, and G1's pending file is gone (transcript). The page says restored, with Retry disabled because the folders differ (34-page-after-restore).
- #10 (k1-review2): folders set back to G1 (40-folders-back). mods moved away, then Retry: 'Install stopped before changing files / Check directories', Retry still enabled (41-bad-dir-retry-enabled). mods moved back (42-dir-fixed), then Retry runs at 0/3 with the recovery snapshot active (43-retry-running).
- #9 (light-review9): FOMOD choices file set; install started; choices.json touched mid-install, so the input stamp changed. The terminal shows 'Installed, but not applied here' at 100%, rate 9.6 mods/min, Continue blocked, and 'Restart wizard' in the footer (20-stale-success). Clicking it goes to Welcome, step 1 of 10 (30-restarted); installed_phase was not applied. Engine bytes are present in game/Override (after.sha256).
- Launch clicks were lost twice (window slow to accept input). Those runs recovered from the Validate page with the recovery steps logged in the transcript (review9-recover.sh).

```
d92f40578e5b3104d94a47dfd951b77ef838d45fa3377b17e33376bc62a2d127  k1-review2/02-trust.png
7ce9cb6972eff6c55c546c09243c2cfd8b8cc0bd286d241d6b5265398dbb245d  k1-review2/06-confirm.png
bf1639ce3e83f27c8a4148627c542cdfd043a2db1c9f981fb6bdcef114c94c09  k1-review2/20-g1-failed.png
7ba8c5e38edde95d923a8ecde056796e9667efaaf31f7a878744a6fee7cf7dc0  k1-review2/30-settings-g2.png
5c021d8dd203e44e4bd6c14ae3f25f27cb61796ab48794e4d1dbdb1d1c5dcb24  k1-review2/31-utility-page.png
ab32eb6c52e887dfb3769089f5abb327f68c6362f6af20f7a139e92dd22098dc  k1-review2/32-utility-restore-hover.png
e2d819815ab963864b35fb51c2500e9775556824445245bd65af3bf2bfd44e89  k1-review2/33-after-utility-restore.png
9afb8aae9721004103fc3d7dd0aebe986189c9fbef80d71e934f531a0d921388  k1-review2/34-page-after-restore.png
b689c015b1e38bf258b6711ce5be51a5dc338b1bea4d1571bc3406188d81342f  k1-review2/40-folders-back.png
3b30794dceb7c8d8531ed4e082c152ee194989e288cb847bdca3ac82541e465d  k1-review2/41-bad-dir-retry-enabled.png
3b30794dceb7c8d8531ed4e082c152ee194989e288cb847bdca3ac82541e465d  k1-review2/42-dir-fixed.png
90fcf4ce95ffe087da739fca8eab77042f1208ae7247203df2bb78afeae330f6  k1-review2/43-retry-running.png
5c4026235c4274fd8e0d8103b48f128b55e4c3b34b0b143fa9414572274f51d7  k1-review2/44-retry-terminal.png
79bad9a0a209cee6e685c9e3051a58cdd4db6e6b8f5f5d9f37ff29070fcea69d  k1-review2/after-failure.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k1-review2/after-restore.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k1-review2/before.sha256
79bad9a0a209cee6e685c9e3051a58cdd4db6e6b8f5f5d9f37ff29070fcea69d  k1-review2/g2-after-fail.sha256
79bad9a0a209cee6e685c9e3051a58cdd4db6e6b8f5f5d9f37ff29070fcea69d  k1-review2/g2-after-restore.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  k1-review2/g2-before.sha256
fbb6474997a5a3fab55818f054511bd5d13ea542e9ee3a0c00618b9df0af7397  k1-review2/transcript.txt
6780b6a9cf8d09690f7e9d93648bcfc849b7bca05330ae6523af3b9f292bff02  light-review9/02b-trust-choices.png
9237f7f10513f2404f1648aba4012fd82217dff840974f7dc6adc20f7013f6b4  light-review9/02-trust-choices.png
bb9b5834897fbe729cb1a0d1af6b835266127e6d3a57cc9b30fb60b09b32a042  light-review9/06b-confirm.png
1a2ac94a6ca84f4d72a623006423ff5fe2977a163730f80dbb1778663f2261ac  light-review9/06-confirm.png
b901a6ec72bf8adf0378cd28b9da279f0d1c06da828a22317a6fb353b571b1bd  light-review9/10-active.png
1898153ab4dbc705230217e2ae95d5ba88fde5c880b2af5f89e8eb333ccbeefb  light-review9/20-stale-success.png
e4a997297424f42f44d4dfb88437f6325672075bda54d17eb711398b88ad56ba  light-review9/30-restarted.png
9a79386919e17a9b6fe4b5485e036dbc0cee3d69a48f27acbf0620eb5d0e0c70  light-review9/after.sha256
96fb8930cbed3f552af064f9c6ab5c00c0811404f228abb6090df5799ab63345  light-review9/before.sha256
e05391f73dc8b40c32fbbdc9222f74f336098722b2d1c660f9e36c0c6ec3d73a  light-review9/transcript.txt
585d895b8fe66a74532ee58784813f7f553c9bb4204053c81bcb1e6245aab88a  scenario-review2.sh
268f4e9a2350d43b592e11dea3530e03efb22c9103a7b92f97521cec168ebe7a  scenario-review9.sh
1c2b8bb9808c517455b79359e63aef606e25a1fda9fa5f544f1cea891e20fd8e  review9-recover.sh
5af056fdadfa8bb080b7971689f73894a981037ee2369b15302a78eb9385535b  lib.sh
```
