# Production checks before checkpoint engine integration

The working source through checkpoint T2, patcher discovery E6, desktop sharing,
provider metadata and Markdown changes passed these production-only checks on
2026-10-02:

| Command | Result |
| --- | --- |
| `cargo build --locked --lib --bin modsync -j2` | Passed |
| `cargo clippy --locked --lib --bin modsync -j2 -- -D warnings` | Passed |
| `cargo build --locked --lib --bin modsync --no-default-features -j2` | Passed |
| `cargo clippy --locked --lib --bin modsync --no-default-features -j2 -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed |

The desktop build found a sharing edit guard in the read-only wizard gate instead
of its game-directory renderer. The guard was moved to the renderer. Strict
Clippy found one nested conditional in the queued-sharing continuation; it was
collapsed before the successful rerun. No test code was changed to obtain these
results. Modified files with inline test modules (`direct_markdown.rs`, `p2p.rs`,
`patcher.rs`, `ui.rs`) retained byte-identical test suffixes relative to HEAD.

Selected source hashes recorded at the boundary before T3 edits:

```text
901b55475e12b241389c2b8eaeca63ba108e4d4ee1cf7c920fe90156d2a15391  src/engine.rs
0842987744182d1d9205c914d3674cfb514d5463d954cc988b6878794d3c4972  src/patcher.rs
87aa7de3beed4a58ced86632352575031e7279d50057b0f5c864c0b3ed464c90  src/peer_sharing.rs
1bab1c7962e4440c545cad5880da9e727d910b1781bd7b01f4d46f4b9a303e59  src/ui.rs
82b9b81033ef837af4b88f6c66af93869180e974625bdc92902724e7e1c1c074  src/ui/sharing.rs
49dd78302b034593aefa3493a3702beb2d729eb1ae7c40ffc9f2b8706a166615  src/checkpoint/journal.rs
1c085aaa29b687f41b4b2e8afe2320b8b70c7161d642d013c8007b6311c43d1e  src/checkpoint/read.rs
9aa8923add33cf99999271afbe09f1fc6c10ae588ed9b37193d7677a14777466  src/checkpoint/history.rs
ce81d265be934efb302381cd1055e88238a5ee9533525afe6cceb91edf1867e9  Cargo.lock
```

These results cover compilation, lint, formatting and source preservation. They
do not establish runtime behavior, crash durability, real transport, packaged
patcher availability, native interaction, full-guide installation or feature
parity. No filesystem tests, Rust tests, custom harnesses or CI runs were used.
T3 and subsequent source changes require fresh production checks.

## T3 managed integration and E5 timestamp boundary

The same four production build/strict-Clippy commands passed after T3 engine,
managed participants, durable run reports, CLI opt-in and archive timestamp
integration. The initial build found a `String`/`Path` mismatch in the game guard;
strict Clippy found two expression-style issues in finalization/history. These
were corrected before successful checks. No-default build and both strict-Clippy
passes include the corrections. Formatting and diff checks also passed. A static
comparison confirmed all four modified inline test modules and test-only helper
methods remain byte-identical to HEAD. No test target or runtime probe ran.

Selected hashes before T4 editing began:

```text
19177b00ff45df3cde4c1e2bdcd32f4ced4975a7f9c186de708c476cb75518f0  src/engine.rs
0a611c7f9b36f3eb990e96c28d0304c2cf9d97b3e619d901ea3bd56934f3a268  src/engine/managed.rs
8695b04c93b10aebcbdf3255b1d2fe9750256194d6b986c9c62c625b24b36a4c  src/engine/checkpoint_coordinator.rs
e535a2032c118204e0c23b8779c1036a6035059014c5c507063bd6253c4f0d2a  src/deployment.rs
02ec01338558977e927ac74058f8ed621c9034517c20c7ea271025cdfcc46746  src/checkpoint/model.rs
3b07b13476ff78387ef8bd3bdfbdffa0051d12f1cf166b4d5dc8c9b57bd56647  src/archive.rs
948323c9725106e92987ee5ccf63728fb2a1aad8629d19013f3a275da85683d1  src/archive/timestamps.rs
90302faccee57f2d59cb818d34a5f38dee37cffc4c7b9b490f1fbd78b09a2e52  src/main.rs
```

T4 restore/resume and later desktop changes are outside this boundary. These
checks do not promote checkpoint crash recovery, managed uninstall semantics,
timestamp fidelity, or any parity domain to accepted runtime behavior.

## T4 restore/resume and desktop FOMOD scope boundary

Both production builds and both strict-Clippy feature modes passed after T4 and
the scoped FOMOD editor corrections. Strict Clippy requested boxing the CLI's
large routing-error variant; the correction is included in both lint passes and
the no-default build. Formatting and diff checks passed. Static comparison of
all five modified inline test modules, including `fomod_ui.rs`, and test-only
helpers confirmed unchanged bytes. No tests or filesystem/runtime probes ran.

Selected source hashes before lifecycle and native history implementation:

```text
2a2bcfa2f7bd4c743fb61fa09972d79a6015dc8e40ab59521db26803bda6d276  src/checkpoint/restore.rs
d6192edfcf99b340b20cb6c5a3597b3beb6a633b15b1df9908744f8bbe5fa395  src/checkpoint/resume.rs
841c6e4032f8597bf6e632892b29507156f9b33da58066183f4cb56b7f247603  src/checkpoint/transition.rs
711ac1785ab5ed629f2d2a590794f444aa160aa334d93afbcdda0bcbfffe1c9f  src/checkpoint/browser.rs
3568899223b27c5983b585342dc71aecca09ef608570badf5d8dd33035823c81  src/engine.rs
37cd925ac99dfa68ae9dec0c6c4d6e13244995aa1481bab8f3cd4d8a4c570a09  src/engine/checkpoint_coordinator.rs
a158925bba3226f7dd81e870eb739322eb82c96c6e8f9b06cea28d33cb15cab5  src/recovery.rs
3c9eabe103083648d7a091a928c13e2c418663029ad2f5ba9dfe4837e5cd571e  src/main.rs
de0fd633c01d77ad3c9b065cf646ef8b0c1549057602eb51880d27fe06dc63f7  src/checkpoint_cli.rs
251eedd322a0fc6349b1e05a2dc85116bca1802b499acfcbd2a37b15fb8fdc6d  src/ui.rs
d8d4b96b4d84516178509e00c57512d449492737c52a11f3ca37438cd0e42c58  src/fomod_ui.rs
d03e73117ec09edeabc5b85f0384aad8ba6cd9daa6b94b434a54836a4ce22a3f  src/fomod.rs
877b354455d7ab94425e51e337edcb45c805f51d1349c4b4f08ab446a9831cc5  src/download_orchestrator.rs
```

This is compilation/static-review evidence only. Component history remains
opt-in. Crash/restart, native behavior, real restore/replay and FOMOD editor
acceptance remain deferred. T5/T6 edits are outside this gate boundary.


## T5/T6 and shared GUI tools boundary

Both production builds and both strict-Clippy feature modes passed after
checkpoint lifecycle, desktop history/recovery, shared editor/import/browser,
conflict presentation and update snapshot integration. Default build was repeated
after the import-preview size correction. Formatting initially reported the new
lifecycle module; targeted formatting resolved it and the full format check
passed. Diff checking passed. Static comparison again confirmed the five modified
inline test modules and their test-only helpers retain HEAD bytes. No test target,
filesystem probe, native application, install operation or CI was run.

Selected source hashes before the next desktop/profile/update-cancellation slices:

```text
c9746171efeaa171c1597cb53816ec9d9729a5a28b99c33eebf05877c68c205a  src/checkpoint/lifecycle.rs
2ba0aafd1423cecf33f074563f8787fb650d19e6f9e6fe39b3a7d556b15761b2  src/checkpoint/mod.rs
9b4854b35dc7fb99a13124a6078c76ac7e018924c6561316077868bbe50ae589  src/checkpoint_cli.rs
c9c88577a240e2d873ab91254098ae6df3705a107a1af94a2110dfa644c3690b  src/ui/checkpoints.rs
a15ad3f45cee4dd67b46bf30cc9018c25219178fecbddeebe0fe10756de33742  src/ui/completion_pages.rs
dd1e1b33846489a0a84852302028a5cc5dadd6906bae9f9d5dd1334a24a7114f  src/ui.rs
6bc9ffca9b2ace8bb5963afbb49dd40bff9891f63326f21c87d32420e3396bf5  src/editor.rs
458f765cde7a65e58620aa8940b23b31571d56b075ef29fcf9545a2d419631ff  src/editor/transaction.rs
22161882d859391d88fd4ffadc3f475da72a20c5c83217d0340bef2b1a0ca26f  src/editor/import.rs
be05349942bc62dc62bb57551a2a3fd59c16cbc688c67660f87f055989c1f456  src/editor/regex_import.rs
83bd6aa298ef64b8ddc10d28b52286ae2f2ab17bdf5a5dd00534c4db0acf2a11  src/editor/browser.rs
f4b925663fded583cade50d39c7ef0f9fcc54f4951e346de4c8127cfbf2054e4  src/updates.rs
ad31df89737f957ef440f333bfb9b57bfdde9099101db7928b50670716e2bc46  src/conflicts.rs
6dd2a303773101d60acded11687bcecc9b78749833cf9315327479caf1e724a7  src/main.rs
```

Independent source reviews resolved lifecycle reachability/repair/trash cleanup,
desktop readiness/sharing, editor identity/draft validation, update snapshot
identity/size, and preview size findings. These results do not establish crash,
restart, native interaction, provider transport or F01–F29 acceptance. Component
checkpoints remain opt-in; .NET-specific custom regex constructs remain an
explicit unsupported syntax boundary.

## Profiles, desktop tools, cancellation and single-instance boundary

Working source on HEAD `eed8089`, 2026-10-02. Default production build passed (41.81s), then final build passed (13.51s). Strict Clippy default passed (4.16s) after two style fixes. No-default production build passed (20.65s), strict Clippy passed (12.52s). Final formatting follows this boundary. Six modified inline test suffixes and test-only helper bodies remained byte-identical to HEAD by static comparison; no tests were compiled or run.

This covers F16/F17 library and CLI, F19/F21/F22 desktop integration, owned cancellable provider/import requests, F23 Unix transport compilation, and shared telemetry lifecycle source. Windows/macOS/native behavior remains unverified. Early package patcher source was included in the latter compilation; independent review found a digest-binding issue still requiring correction. The new GUI launcher was formatted but not built by `--bin modsync`. No package/native/runtime/CI acceptance is claimed.

```text
bb526cf2c0b4e96b150999647bd05be9ebaffc20b482c8877d7118be7924802c  src/main.rs
26eeb97858d874e7e6e79cd8044c211aa5f5030c5a04ea1b45b601127cc9fa0d  src/ui.rs
9b8f5caeb03266a0d02f29a5dd5bca4aee0c268c5e14ac296ddfbceb1e16a72e  src/ui/editor_panel.rs
8cd2374ffcf3fc8b2e01a2f5e2db824444eceb6dcf60110bbd5e0f8eb0e7d399  src/ui/tools_panel.rs
73d7bdb9ceb26d396f9dbedd8b5d75c93668f1a5a80db38c783bcd739e796ccd  src/profiles.rs
5d817f80c046926a61fdca50ea31ae67c9be9d004e13b44f2dc964188c00d823  src/profiles/schema.rs
9111ea56d6093b514ce7489bb2636c401b63fedea9fcdfb48a86e80a11208495  src/profiles/settings.rs
5885c696adc3fc3193984af05e83c2c91826d0b3035c673739cdeb2e0eb07959  src/profile_cli.rs
bdf5596997fb85cedf7c703d6dac5e2081df0ed0ab9b21a9d059c501f48a57fe  src/updates.rs
8cf8df0e0a6e3e313dc1f2b83b6b04b4898ae4b624df8ad4d7878475a5d648fb  src/updates/cancellable.rs
da2b00d1d23a0dbad422215273ed5c30a12021ec828c79062b463a4056f408c7  src/protocol.rs
1444ef6fae53ac50f98ad63c066c9180b10a5aed0940617494a44248439cc458  src/protocol/fetch.rs
7c8608118f1736925aabcfc3d5c91a95495df0713d50a43007f7e94b24c74a11  src/protocol_registration.rs
e1b3d2870fe9cffc698155380b9f7ebccbe580de5664f533e0539e3a798919af  src/single_instance.rs
fc7d661fcc2136f29df5d3d00e438d5e6b2a16bb6a4857ba4edca153eeade19e  src/single_instance/unix.rs
9d9c185431a770e56646de73b1a9c6aa005bac703852f03a3dbab921a1d5af35  src/single_instance/windows.rs
f0a64d714f0298f7031f7e1aadc17962a626ebb19fe9c051f6c7e83f40ddfeba  src/telemetry.rs
2fd4cbe455839ff64b4d450db5046a235ec0a43e7666688e44fdd63640c40b9a  src/telemetry/lifecycle.rs
c39b112e02817bd5ed782afa235e24edb9f8f416f177150285ce735f7cfac585  src/patcher.rs
ea40a352bf57058ad2230f66f474c2b59d2472633805b34ff5f5a396e1c759bd  src/patcher/package.rs
ac2ed89a2b413045c45e7414c834687500443e9148813d4a342628815f168ed7  Cargo.toml
ce81d265be934efb302381cd1055e88238a5ee9533525afe6cceb91edf1867e9  Cargo.lock
```

Final `cargo fmt --all -- --check` passed for the recorded boundary after targeted formatting of the new package/launcher sources. Later CLI session, UI telemetry and packaging-binding changes require fresh gates.

## CLI composition, standalone execution, Nexus and desktop bindings

Production-only gates during continuation on 2026-10-02:

- Default production build passed (exec session 33812, exit 0). Earlier attempts
  exposed missing Unix namespace parameter, managed-copy Read import, watcher
  unused import, and FOMOD choice-file borrow; these were corrected.
- Default strict Clippy passed (73931, exit 0), including later bounded patcher-log
  intake. Prior Clippy attempts found expression/control-layout issues in draft,
  editor, protocol, process cleanup and desktop adapters; corrections preserved
  existing branch conditions.
- No-default-features production build passed (27946, exit 0).
- No-default-features strict Clippy passed (32610, exit 0).

Only --lib --bin modsync targets were requested, with --locked and -j2. No test
compilation/execution, custom harness, filesystem test, application/backend
launch, native acceptance or CI occurred. Static byte comparison confirmed all
six tracked embedded test module suffixes unchanged.

The default build preceded the final log-reader/style corrections; default Clippy
and the no-default build cover those corrections. The new Windows-only engine
restriction is source-reviewed syntax here, not Windows type checking. Windows
checkpoint persistence implementation begins after this boundary and is excluded.
These checks do not close any parity acceptance row.

Selected source hashes at this boundary (subsequent edits require new checks):

```text
39c022ec8f5299efc8007fb39e133d589d4fffc35415b4272f6e9c992cb603c0  src/main.rs
0b4c1e4b794d08e72b765125166c29cf2b2e4eb4e1bba24914c5abc5c2a55631  src/engine.rs
4668a7b114a29979951ae14476cc083be628e869a7c216a3ead774c0540c7b72  src/engine/managed.rs
435d5cc1b243b1572a03e6bf34a77abfb308dc092afac9a77b44b45895af4488  src/patcher/standalone.rs
78f05ca323d2d6595d2b0bf6f6ec41767777228bb7ea15678614f819e240a437  src/patcher/standalone/unix_group.rs
b75069f6c2e6f0362e01adde1d9f06b49fc87caa56dda70d064474f93683995b  src/patcher/standalone/windows.rs
fb82ac95e97cc5ec03a8e02d75bbffa9097ca4900dc79549671373b09511a736  src/download.rs
e812ee642abe9183f1e8daa36ff524675b80d5cae44a2dfd08f3281a20f09fb3  src/download_orchestrator.rs
ea946d300f4d6e7cbc2c260ef4a38163113615c85e4c09773edceaa1a8377921  src/updates.rs
dfd93196acee888ad54594de9ef200bd1b6715d5fe06bf5ba3e066de2d492989  src/updates/cancellable.rs
964640622ad87034420f8468a55f70a16f623fb295d18a70070c8acb5c7be8bc  src/profiles.rs
e4d45287967464492f03b33c2deb40f27ffd54fa41fb8c31390ad6559eb828a8  src/profiles/settings.rs
71ab6196a0926da4a5ef7989f2ad5e7fdfccda03959f5a226545e14dca00d526  src/ui/desktop_settings.rs
e596637c0498ca3247c1832a429f03a2c562eed5250d220907430798d90508f6  src/ui/standalone_patcher.rs
c87573bde26de629626054d06e80d5552a38729d91597d89ae13a0802a171129  src/ui/main_window.rs
d50010ec8e75b3881f97b5fe4cc83bca6850ada0c942cacec1da911e9331bf9b  src/recovery.rs
feff856ef9e4f425d5ced71ed3ae6d33fbe629092d4d77c81796111f08a5394d  Cargo.lock
```

Formatting check also passed (22025, exit 0) after this receipt was captured.

## Windows checkpoint, INI staging and desktop continuation

Intermediate production-only receipts from the next source boundary:

- Default build passed in sessions 93074 (initial Windows library / INI source), 41744 (integrated process guards/facade) and 80850 (staged INI, process integration, desktop resource browser, saved update rehydration and typed log view).
- Default strict Clippy passed in 9619 and 70459 before the final desktop additions. Session 21805 found one collapsible-if in the resource tree renderer; that equivalent control-flow correction was applied and requires the following fresh gate.
- Formatting passed in 19654 and 80570. Session 7738 exposed only module order and narrow production formatting, corrected before 80570.
- Static byte comparison again found all six tracked inline test-module suffixes identical to HEAD. No test targets, harnesses, filesystem tests, applications or native backends ran.

Windows x86_64 GNU production cross-build attempts used only `--lib --bin modsync --no-default-features`. Attempt 19857 stopped because MinGW C++ was absent. Installing Fedora `mingw64-gcc-c++` also updated its matching GCC/libstdc++ packages. Attempt 27102 then found upstream case-sensitive header spellings. Temporary aliases to the actual MinGW headers, without replacement declarations, allowed attempt 43341 to reach Rust compilation. It found job-access constants imported from the wrong windows-sys module, the missing Windows single-instance namespace argument and one platform-unused mode value. Those were corrected. Attempt 76405 compiled Rust source and reached linking, which failed because unrar_sys chose Windows source units using the Linux build-host cfg; the real `isnt.cpp` and Windows system libraries were missing. No successful Windows binary is claimed from these attempts. A durable production dependency patch follows this diagnosis.

These are source-bound compiler receipts, not F01-F29 acceptance. Later dependency and UI changes require fresh gates below.

## Integrated Windows, desktop and CLI production boundary

The combined working source passed the following production-only gates:

- Linux no-default production build: session 65492, exit 0 (67s).
- Linux no-default strict Clippy: 82346, exit 0 (19.89s).
- Linux default production build: 6513, exit 0 (38.84s).
- Linux default strict Clippy: 24244, exit 0 (23.37s).
- Formatting: 50302, exit 0.
- Windows x86_64 GNU no-default production build and link: 28802, exit 0
  (104s), followed by current-source refresh 61027, exit 0 (0.65s).

Build/Clippy commands use `--locked --lib --bin modsync -j2`; strict Clippy
adds `-- -D warnings`. Windows adds `--no-default-features --target
x86_64-pc-windows-gnu`. The successful Windows builds use the durable local
unrar_sys patch, without temporary header aliases or replacement symbols.
The latest CLI preference/key corrections are covered by the final Linux
default build, both Clippy modes and the Windows refresh.

This boundary includes integrated Windows checkpoint process ownership and
explicit recovery, owned patcher payload/INI preparation, resource hierarchy,
update rehydration, typed nonblocking desktop log snapshots and CLI settings
provenance. The separate GUI launcher is outside `--bin modsync`. No binaries,
backend processes, tests, test harnesses, packages, native acceptance or CI were
run. F01–F29 acceptance remains open.

## Observer, NXM desktop and package qualification boundary

Independent source review accepted the observer lifecycle correction, desktop
NXM preference/worker adapter and shared confirmed registration guard. Parent
source review accepted packaging receipt/resource/notice qualification and the
final source-state publication check. The shared registration guard compares the
original displayed scheme/state/handler under the platform mutation lock;
external native tools and content behind unchanged handler strings remain outside
that serialization/fingerprint contract.

Production gates passed on the combined source:

- Default Linux build: 61560, exit 0, 19.33s.
- Default Linux strict Clippy: 83755, exit 0, 13.92s.
- No-default Linux build: 58735, exit 0, 12.05s.
- No-default Linux strict Clippy: 41061, exit 0, 8.32s.
- Windows x86_64 GNU no-default build/link: 79368, exit 0, 17.40s.
- Formatting: 52192, exit 0, after fixing module declaration order only.
- Static diff whitespace check passed. Read-only byte comparison confirmed all
  six tracked inline test modules unchanged; no test targets were compiled or run.

The default Linux build includes desktop adapter code; Windows compilation here
covers the shared CLI/library service, not a Windows desktop/native invocation.
Packaging Python was source-reviewed only; no packaging helper, artifact build,
registration action, application/backend launch, test/harness, or CI ran. The
separate GUI launcher and macOS compilation/native behavior remain unverified.
All parity acceptance rows retain their earlier open status.

Source hashes at this boundary:

```text
92cf5a85c15b283a258f3ead199b3544929b0726e602b3c3bd75733edf10ad30  src/ui.rs
65309f9e54832e719f60fee5bb461d676ae3119464466ec5a2383a29b05957c3  src/ui/resource_watch.rs
4557ef44873c71faccc584b46dc86a71c7b45108ea89bb3b3b3d7dc2c0a7fe33  src/ui/protocol_settings.rs
469743e1dcab36a15f9d5902fbf2136757f3e58a3bfc980de64b1051c66ced7f  src/ui/desktop_settings.rs
bcd7ca39897f93b115f2ff464c824413434e8a9e2230c610682b90cef846303b  src/profiles/settings.rs
d52e8673c631fc2952768ea76495564ac67f23991591f05e76fd7ad6f2e2c213  src/protocol_registration/owned.rs
47bc22836d0c6162edc36ad314ebbad7a32495574caee88d6a5fc109d5ec4d66  src/protocol_registration/linux.rs
8ac4a4b93653e3f4204e23668495c64d724b257d2e533c7eaa3cf4850ce90e93  src/protocol_registration/windows.rs
5d6f7d4b11f2cd9f3df7cb0f5efb7cf98f62b52e3597b1431ff6a247a7760084  src/protocol_registration/macos.rs
80f871b5fb03f374900d24ea2fb7c34a9bbb86585e3ce9ef09464d82aabd1970  packaging/build_package.py
1a43de7304c563b3ccebcce223c5e327a0f1610b5776ec2b1d0679c9206ab0ca  packaging/produce_build.py
44ced1f48cbef168a5b719c95b51a7a94add8384f6f12b6ec8e875747dcc5261  Cargo.toml
14a630b1840bae61999cd9b2dca4215596565e2cce0d8dbaf228fd07237c867f  Cargo.lock
```

## Profile ISO forms, removal confirmation, log highlights and CLI selectors

Source review accepted the additional bounded profile ISO reader against pinned
Newtonsoft.Json13.0.4 source, component/option removal confirmation after fixing
disabled-parent/settings-worker modal gating, independent retained-record highlight
state and focused H action, and case-insensitive validation/backend selectors.
The remaining CLI value-enum audit found no additional established reference case
mismatch. No behavior/native acceptance was inferred from these source reviews.

Final production checks on the combined source passed:

- Default Linux build: 32726, exit0,4.79s (refresh after final selector correction).
- Default strict Clippy: 61634, exit0,3.18s.
- No-default Linux build: 79107, exit0,2.77s.
- No-default strict Clippy: 69585, exit0,8.13s.
- Formatting: 2607, exit0.
- Windows x86_64 GNU no-default build/link: 22463, exit0,12.40s.
- Static diff whitespace check passed; all six tracked inline test modules remain
  byte-identical to HEAD. This was source comparison, not test execution.

Earlier default build69053/default Clippy15922 and no-default build33677 preceded
the last small corrections and are superseded by the final boundary above.
All commands target only production `--lib --bin modsync`, with locked dependencies
and two build jobs. No tests, custom harnesses, runtime/CLI probes, native actions,
filesystem tests, packaging helpers or CI ran. Broader invariant/local-zone date
normalization, numeric culture compatibility, native desktop/platform and F29
acceptance remain open.

Selected source hashes:

```text
5c4abec3b741fd3537825afb7da871c86f9469df335194c890522a50975f76a7  src/profiles/schema.rs
1859d2d10c6588370f2cda081b3c2082c49c715572df7e3fbf5c7ea431d363af  src/profiles/date.rs
7026d54828fc18665c4f47ee9bdface2f5c59f6d7b1c65b936910ec66f80123e  src/ui/editor_panel.rs
e10f99ce369e5c6788f31cd0ad6f8a02b90398027e8f59bcc292f3338764005b  src/ui/structured_log.rs
70607a9bcda7e0e16f28f000b0a970329fb78f92b08c43d3a127f33de815ab5d  src/main.rs
```

## Numeric culture, editor generation and owned shell/editor workers

Reviewed production source adds current-culture settings parsing, ordinary
validation's zero-selection setup report, reachable install source-directory
fallback and reference -g/-s aliases, worker-based Save Selection, raw UUID helper,
drop dismissal/focus, issue/sponsor links, editor section controls, and local/
provider instruction-generation preview/adoption. Numeric review corrected
floating negative zero, exact integer signs and locale-free key removal. Worker
review corrected ownership, terminal-tail teardown and preservation of queued
successful-save receipts. Provider review added a pre-network probe-isolation
guard. All identified findings in these bounded slices were source-rereviewed.

Final production gates on the combined source passed in session21847:

- Linux default build: exit0,9.35s.
- Linux default strict Clippy: exit0,5.75s.
- Linux no-default build: exit0,4.16s.
- Linux no-default strict Clippy: exit0,3.22s.
- Windows x86_64 GNU no-default production build/link: exit0; separate compiler
  output retained at `/home/brunner56/.cache/modsync-windows-production-build.log`.
- Formatting: session94412,exit0. Static whitespace check passed.
- Six tracked inline test modules remain byte-identical to HEAD by read-only
  source comparison. Existing frozen settings/save helpers remain in place.

Build/lint commands use `--locked --lib --bin modsync -j2`, with the documented
feature/target variants and `-- -D warnings` for Clippy. No test targets, custom
harnesses, native app/CLI/backend probes, filesystem tests, provider requests,
packaging helpers or CI ran. Native locale/picker/shutdown, actual generation
results, platform behavior and F01–F29 acceptance remain open. The standalone
GUI launcher is not included in the explicit bin target.

Source-bound hashes:

```text
b6c3c2f99523b93363f5bf6da3c0bcff6ea54ae5de48a3a5e8049eb45c605ac1  Cargo.toml
1ad02aad919589c4fd21635a1e65de319d8453260cfda39540f299442e349bd7  Cargo.lock
b5d5b1907267b41f61f11302a05753446dab3e33e27434e0f2676c2707a676f5  src/main.rs
71c78f830058b4999e3855150c0df115a78c5d3a31f78fa944895201dffe0223  src/profile_cli.rs
ae8a91afe7366a8f8d83059afcc20d48f5b517425db0e72d64375b08b6300b09  src/profiles.rs
482c752dc71c29a4a7efedf87853389abcbfbd6de79ba591fb1ccd62220b9eeb  src/profiles/numeric_culture.rs
e7fbf6f02e4b1523f2a9e2a5abeff169c2288705f02530464ba9a6d5cfc66818  src/profiles/numeric_culture/icu.rs
295200ffb9ae5308e55d8feb29e6665677e9f12ef03f34d60917aa5e912d0f6c  src/profiles/numeric_culture/windows.rs
093e5804a3aaec396d9f8a45107734dfc4c75a2b83864ba4f9994bb7851f176a  src/ui.rs
81472ac62a4a4ae621015518bf2c487509cd49874b8f45cabe79b74340c4ee62  src/ui/main_window.rs
13446755d067ac94fb5c4612bf31090787e2cd01d72d774abc5c96abe0318b9a  src/ui/editor_panel.rs
1dc27563c606c2e008aafbe209c504e12eb33e52dc6741041f03c40578289934  src/ui/editor_panel/generation.rs
```


## Basic validation, legacy key/date reader, maintenance and rooted choices

The reviewed source after consolidation `1fc4251` adds rootless read-only basic
validation and authored selection preservation, TOML conversion/merge defaults,
private legacy Nexus-key import, Microsoft-date grammar, rooted FOMOD choice
scope, and shared Linux maintenance with explicit CLI/desktop authority and
reverse recovery. Independent source reviews accepted the bounded implementation
and follow-up publication, terminal recovery, path, projection and worker fixes.

Final combined production gates passed in session29594, exit0:

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Default Linux `cargo build --locked --lib --bin modsync -j2`: passed,1m37s.
- Default strict Clippy with the same targets and `-- -D warnings`: passed,20.34s.
- No-default Linux production build: passed,1m11s.
- No-default strict Clippy: passed,39.23s.

Earlier attempts stopped on expression/style lint findings. Their inclusive
range, result return, tuple alias, button/guard let-chains and Boolean corrections
were independently source reviewed before the complete successful rerun.
Formatting used edition2024 and preserved existing frozen regions.

Read-only source comparison against HEAD confirmed seven inline test modules
(`direct_markdown`, `fomod_ui`, `game`, `p2p`, `patcher`, `ui`, `validation`) and
the two existing UI test helpers retain identical bytes. No tracked test or
fixture path changed. No Rust test, custom harness, filesystem test, runtime
app/CLI/backend probe, packaging helper, native mutation, install or CI ran.
The previous Windows receipt covers its earlier boundary, not these changes.
Native maintenance, cross-process acquisition admission, broader profile dates,
FOMOD dependency/corpus behavior and all F01-F29 acceptance remain open.

Source-bound hashes:

```text
88f8c8036361837036d2955fadd60f07187bb4cfbfc6ba524018ebd0f2f34276  src/maintenance.rs
f2caccdca077163f78735a3b4220a10760d2a9dddbe1ebdf8427f2cbb29a1804  src/maintenance/plan.rs
00b6f5f082c3e13ab0f7a96dabc169ff4e440efab6131bd30b8b32929b5081e5  src/maintenance/journal.rs
a287e709634487f10f131d8f32c46e729dafbf7a97692714129d9097f3116d44  src/maintenance/platform.rs
00453e55ccab31aad54a3d8ca489d185106cc20f27f6ce669400e43414553b32  src/maintenance_cli.rs
05ee40a493728783a22abe79eb774d8129b0cbd88c0d06b204c2ae0c19ddb356  src/ui/maintenance.rs
217326c4b87fa4bf055ff5fa1d30bb79b64fb438e539a12ff24f487847ae2f10  src/recovery.rs
c702d71552eeba404831bfac4ecc587bead0b9664413d412a3cda249c69b13b9  src/fomod.rs
8244e80f7047af2fbc557e6dcd1bce439332199af98b69ac733a86f238dcacc4  src/fomod/resolved_choices.rs
1280c9cc1634175fcaa6e451a75842a8152ff8e135a67d1d3b405511a13d4a45  src/phase.rs
3a3f98314a111de7a661a2773694bcaae05b02d9c17cbd63bd3238c8cb63bdd9  src/main.rs
040f75a9c51942fed2c10986cd63655fbc43dcdc563dcaa73a3225347b03812a  src/ui.rs
c60e1b7959e1923d2a0af8f9262bdbb53fb6ea6a5ccced6f3ca8e75754b0aece  src/archive.rs
d24f831b937f64897925144d6c05f6050e0b7333c8e8325e704a0339db354237  src/model.rs
02268edee42433cd723b01af97fed271237b7cd3d38eb8c36e147a91c9c2cb7c  src/validation.rs
fdfd334615b12c47fa238d8079c6d50066cf82d8edcd34a8e91aa69a5b02afae  src/validation/basic.rs
e412291dd5fe4873f5c42d3714f42cfdce8ae7422211c188b5400f659cd5aef4  src/profiles.rs
4b89e04cd99acd188d84aac9c2c192ef911358c3eda486c41b9f8fd6b42b015f  src/profiles/date.rs
d41ee6431e5e58b8e19a18569d74409ae6ab8e78dc18325254424f45b2e75d48  src/profiles/schema.rs
a244410913dfd1ec1161ef5bd28e3a521bad3a1b3bf72fb0e6f3400068ef0ce6  src/ui/desktop_settings.rs
```

## Durable maintenance/acquisition and native profile continuation

The reviewed source after `a185ced` implements a durable mod-root fence,
target-dependent terminal cleanup and disposition, shared acquisition root
coordination, existing-file/current-guide revalidation, standalone guards,
worker/retry bounds, keyed execution admission and direct torrent metadata
cancellation. A Darwin native source profile and offline OdyPatcher producer
source are included; Linux gates do not compile Darwin-only code or execute the
producer.

Independent reviews corrected the game-only cleanup crash window, root-only
canonical binding, skip/GUID ownership and work bounds, worker/retry exhaustion,
quadratic execution grouping, torrent pre-session parsing/metadata cancellation,
native capability presentation and producer package/executable binding.

Final allowed Linux production gates passed:

- Formatting: session89887, exit0.
- Default library/binary build: session64755, exit0, 1m30s.
- Default strict Clippy: session19733, 49.63s.
- No-default library/binary build: session19733, 51.16s.
- No-default strict Clippy: session19733, 47.45s. Combined session exit0.
- Whitespace checks passed.

Read-only comparison against `a185ced` confirms seven inline test modules and
the two frozen UI helpers remain byte-identical. No tracked test/fixture path
changed. No tests, custom harnesses, filesystem tests, application/backend/native
probes, package helpers, installs or CI ran. Earlier Windows receipts retain
their older source bindings. Native interruption/platform behavior, Unicode,
broader date normalization, producing/backend obligations and all F01–F29
acceptance remain open.

```text
33302fb20ebb2d62d4f31b9b3bf2dcc0d109351858f2c749d22c9080d20f3740  src/acquisition_guard.rs
a522dc717e052214f8c85743a59852d90f215af982bddd0cdef0532ca1e4b7cf  src/recovery/root_lock.rs
7aa928139b3e92f943cf6b21e6fab7f6888f7bdabf7e0542eb37196f21142c5c  src/download_orchestrator.rs
baef794bdd93674039bda5360af46b2d2649129a4eedab1624f1072e95ec7864  src/download_queue.rs
3c37f3e102d0c87c416c5b25ce70278d12de375c99feedec4472c42e5065b1df  src/maintenance/fence.rs
0bacd982547d9064215d0f85567ac5d109d773d013e5a2a8bd376f9257140210  src/maintenance/journal.rs
6a70e526d0488599209861a60d0bea0cfb17b6310bb2cc419b6b4e063d4b2345  src/maintenance/platform.rs
26ebbd30e54b0593da32df9f17d35022023f46a3314f87d6f6766c7176058d6e  src/maintenance/platform/macos.rs
e466202cedff6a7acf4480693f980dea974d561a53dac82cb1b38bf5f8e529fd  src/p2p.rs
e7bb86bc7b8d9bedc97b63046dc5ea5f2b7e7397fe1befee99f6e5127d547b78  packaging/produce_patcher.py
```

## Versioned casing, Windows maintenance and profile dates

The reviewed source after `8be4382` adds a bounded native Windows NTFS
maintenance profile, versioned Unicode 15.0 lowercase and ordinal uppercase
projections, shared keyed planning/discovery/execution admission, safe empty-only
stale peer cleanup and profile DateTime canonicalization with immutable Unix
zone intake. Independent source reviews accepted the current source after
corrections for named streams, journal truncation/append rights, private-file
archive attributes, readonly flush handles and historical/future date rules.
The Windows profile date document records the next proposed provider, whose
implementation has not started in this source boundary.

Final Linux production gates passed after the last POSIX rule-equivalence fix:

- Formatting: session53046, exit0.
- Default library/binary build: session11196, exit0, 32.92s.
- Default strict Clippy: session25531, exit0, 16.65s.
- No-default library/binary build: session31266, exit0, 36.37s.
- No-default strict Clippy: session88047, exit0, 34.13s.
- Whitespace checks passed.

The first Windows GNU cross-build (session31366) stopped in `aws-lc-sys`
before Rust source compilation: MinGW GCC/G++ were 16.2.1 while the installed
CPP package was 16.1.1, leaving the driver without its `cc1` component. A
targeted `dnf upgrade mingw64-cpp` (session75880, exit0) aligned all three
packages at 16.2.1. The real driver then resolved its matching `cc1` path.
The subsequent production cross-build result is recorded below.

Read-only comparison against `8be4382` confirms seven frozen inline-bearing
source modules and the existing UI helper source remain unchanged as whole
files. No tracked test/fixture path changed. No tests, custom harnesses,
filesystem tests, application/CLI/backend probes, Windows binary execution,
packaging producers, installs or CI ran. Only production Unicode data
generators were invoked. Linux build/lint and Windows cross-compilation do not
establish native behavior or interruption acceptance.

Maintenance Unicode source SHA-256 is
`806e9aed65037197f1ec85e12be6e8cd870fc5608b4de0fffd990f689f376a73`;
the lowercase semantic table SHA-256 is
`cb52d6454792a898d04ae3102a8c227ea351f70f15321589f7d4cf9db6015400`.
The ordinal uppercase semantic table SHA-256 is
`e291cd8564cd0a84c4eb04b5dce57a3948b272f22613096dbbb042454502273c`.
Retained provenance records bind each algorithm, source, generator and table.

Windows GNU production library/CLI build and link passed in session20889,
exit0,4m00s, after the narrow toolchain repair. Its sole deprecated TempPath
call was then replaced by fallible path resolution before exclusive creation;
cleanup is armed only after successful creation, preserving collision ownership.
Independent source review accepted that correction. The final Windows rerun
(session46315) passed exit0,15.80s, without warnings. Final formatting
(session26428) and whitespace checks passed. Windows binaries were not executed.

Source-bound hashes:

```text
ed583bc4f7dc52a378cc78265ff106c0b4f506fa171991871f8e92afa146ad7b  src/download_orchestrator.rs
c3a7c34cf082de65f5e63edefccf11cb386e485f58a3fea595985c9e615bae0a  src/fomod.rs
17ee9f5c5815c60098b6c7a2b91e8c3923d4f3ee4b8fbf44c8b35d8bbd521fc8  src/fomod/ordinal.rs
20694af376521c3a93d23e965c87a8c15c3a4917a000387ebb9519e06cd2903d  src/fomod/ordinal/data.rs
6c879ae71ca376661eb2fbe4884fc5be368fb5f0699b3af3bb285fb47a64d462  src/maintenance.rs
830fa832acb808aecc63a7852a612cfdc3e323eb17ea0b79f59bb2bee49b8ad6  src/maintenance/casing.rs
f127504a65e62eceee3759fbc34ba806ce80a0fa884c724410aa2141d734fad5  src/maintenance/casing/data.rs
9a64be3cb2f8b8098f06a175be82848739af8540706ff15e52d3c9d4112936ea  src/maintenance/platform.rs
42b06765f6a38aca803d5bbb1567e90924bd1bb4415b692c5aa7f0a0747d3ebb  src/maintenance/platform/windows.rs
1343ab0a8d14bbb8c4f1545896aaeaea41ecd81215d2af5c7852c8ba38f04f72  src/maintenance/journal.rs
14a0b2b7b9df1c6f93f1806b302f23110e1b265785c1293dc0dc52f45cd4fdf9  src/profiles/date.rs
b57d3239e8f08bff5b4c2ce1561f0995a594cd76aa7740c76d4427f1e2878ba2  src/profiles/schema.rs
33b5eb4f0c70014c8ae09b0b3c22ef47649710caf8a8b43830954c5aeee80e9e  src/profiles/date/local.rs
8ed0552bc50a6a2690394f25acbda93a63ff078a08ba74c3989d27c556771465  src/profiles/date/local/unix.rs
b9a1599452ace63e1d564566c1c98f3105d42922b2acff83befd6283d94ca84b  src/profiles/date/local/posix.rs
8d1eddb9a1f3804f1c6d082eb3fb6ed437d9e5db029b8beebd39975156bdcca5  src/profiles/date/local/tzif.rs
61513b7ffb72657b157176efcd4f271513f5ae06b27709a5f08dc17c03aa5f3a  src/ui/maintenance.rs
```
