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
