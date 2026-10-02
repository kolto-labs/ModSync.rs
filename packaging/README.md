# Offline pre-release package construction

This directory contains production packaging source. It does not build binaries, download payloads, install packages, change native registrations, or publish releases when imported or inspected. Those actions require an explicit invocation. No package was generated as part of this source slice.

## Inputs and output

`build_package.py` requires a clean committed source checkout, the full checked-out commit ID, an explicit version, a retained target from `targets.json`, caller-supplied production binaries, and `--build-receipt`. It inspects ELF/PE/thin Mach-O headers and rejects wrong architecture; Windows requires both a console executable and a GUI-subsystem launcher. It never runs either binary. The build receipt must carry schemaVersion=1, sourceCommit, target, cargoLockSha256, and binaries mapping `modsync` (plus `modsync-gui` for Windows) to executable SHA256 values. All are compared before assembly and the receipt is retained inside the package. The receipt must also have `status=built`, matching clean `sourceStart` and `sourceEnd` commit/lock/status objects, an artifact-inventory digest, ordered start/end timestamps, and the producer's offline/unpublished/unverified policy fields. Failed-build receipts are rejected. Windows resource receipts must match the tracked RC template and its rendered digest, and their four-component version must equal the three-component package version plus `.0`. Authentication of the producing build/receipt is external; matching supplied hashes alone is not native acceptance.

Examples below describe future authorized artifact construction, not acceptance already performed:

```
python3 packaging/build_package.py --target x86_64-unknown-linux-gnu --binary /build/modsync --build-receipt /build/receipt.json --output-dir /artifacts --commit <full-commit> --version <version>
```

Windows also requires `--gui-launcher /build/modsync-gui.exe --platform-icon packaging/assets/modsync.ico --windows-resource-receipt /build/resources.obj.json`; macOS requires `--platform-icon packaging/assets/modsync.icns`. Direct helper invocations still require an explicit native icon path; the manual workflow defaults to the pinned source copies under `packaging/assets/`. Linux includes the repository original SVG icon. The macOS bundle declares both URL schemes in one array and package/channel ownership markers, but declaration is not implementation proof of AppKit delivery.

### Explicit Windows resource producing step

`windows_resources.py` compiles the checked-in `windows_resources.rc.in` only when invoked. There is no automatic Cargo build script or packaging side effect in ordinary gates. Supply a reviewed native ICO, a four-component numeric version, target, and installed compiler:

```
python3 packaging/windows_resources.py --kind windres --compiler <target-windres> --target x86_64-pc-windows-msvc --icon /reviewed/modsync.ico --version 0.1.0.0 --output /build/resources.obj
```

For MSVC use `--kind msvc --compiler <rc.exe>`. The output is a COFF resource object for windres or an MSVC resource file (accepted directly by link.exe); the suffix does not change that compiler-specific format. The producing build must link that object/resource into **both** the GUI launcher and console executable, using `RUSTFLAGS=-C link-arg=<absolute resource output>` for the explicitly selected Windows target. Keep it out of host build-script/link steps when cross-compiling by using target-specific Cargo rustflags configuration instead of a global flag. The producing build receipt must include `windowsResources` with receiptSha256, objectSha256 and iconSha256 from the helper's receipt alongside both resulting executable hashes. Package assembly checks that binding and the retained icon. Actual compiler/linker behavior, embedded resource inspection and native launch remain acceptance requirements.

Vehicles are a versioned Linux tar.gz, a Windows ZIP with both executable roles, and a ZIP containing ModSync.app. Manifest records include source commit, Cargo.lock SHA256, file hashes/sizes/modes, patcher capability, and explicit unsigned/unnotarized pre-release status. The manifest excludes itself to avoid recursive hashes; the outer artifact receipt records its digest and the archive digest. Archive entry ordering, modes, timestamps, owner metadata and gzip header timestamp are fixed from source commit time. Reproducibility still depends on identical binary/icon inputs and compression implementation.

Outputs use no-clobber publication. A manual artifact-only workflow is included; no publication action is included. The native matrix retains Linux x86_64/aarch64, Windows x86/i686, x86_64 and aarch64, and macOS x86_64/aarch64. Older Windows reference lanes remain explicit unresolved exceptions.

## Patcher authority

`patcher-provenance.json` is intentionally empty until a native payload and its license/source are independently reviewed. A package without such a payload declares patching unavailable. Supplying `--patcher`, `--patcher-engine`, and `--patcher-license` requires an exact target/engine/digest match in that version-controlled allowlist. No hash, license or source URL is invented here.

The allowlist entries carry target, engine, sha256, source, license and licenseSha256. The same allowlist is compiled into the Rust runtime. A manifest-bound patcher must match it and its file digest is checked again when constructing the process command. Resolution captures immutable manifest bytes by digest and the selected path/provenance: deleting or changing the manifest later fails instead of switching trust rules. Target OS/architecture must match the running process. Package lookup precedes development/PATH discovery. A package declaring no backend does not silently acquire a PATH or Python backend. An explicitly configured external backend remains a separate user trust decision and does not establish bundled capability.

### Backend producing receipt source

`produce_patcher.py` provides an explicit offline producing step for a reviewed
OdyPatcher checkout. It requires an exact clean full commit, retained target,
installed exact Rust toolchain and previously reviewed `LICENSE` SHA-256. It
admits the standalone root package `odypatcher` with its declared
`AGPL-3.0-or-later` expression, and refuses unbound submodules. Its output must be
a new directory outside both source checkouts.

The helper builds only that package's CLI binary with `--locked --offline
--release --no-default-features`. Offline Cargo metadata binds the root manifest
to the compiler's emitted package ID and binary target. The receipt retains the
tracked source archive, unchanged start/end commit/manifest/lock/license hashes,
toolchain/host/commands, metadata and compiler artifact inventory digests, copied
license, architecture-checked executable hash and failure evidence. It never
executes the backend, installs dependencies, modifies the checkout or adds an
allowlist entry. No helper invocation or backend build was performed for this
source change.

This is a nonhermetic build receipt: ignored local Cargo configuration, inherited
configuration, external path dependencies, registry/git caches and native tools
can affect compilation beyond the retained tracked source archive. Matching
hashes do not authenticate a producer. Dependency license review and complete
source-distribution obligations remain pending; the archive is retained review
evidence. Successful production and independent provenance review are required
before reviewing any allowlist addition. The currently empty allowlist remains
the runtime authority.

## Linux per-user launcher

After extracting a verified package into its permanent location, run `python3 install.py install` to create a package-specific user desktop launcher. `--register` delegates both schemes to the packaged shared `protocol register` API; replacing another handler additionally requires `--replace`. The package token includes full commit and producing-build receipt digest prefix and matches the manifest/macOS ownership marker. External command failures are fatal. `python3 install.py uninstall` compares the retained package identity and desktop-entry hash before removing its launcher and calls shared `protocol unregister` with the same recorded token. The shared platform API owns association comparison/restoration; the Python installer does not rewrite MIME state. A newer package's association is left alone. The extracted payload remains; the operator can remove it after unregistering.

The installer needs Python 3.11+, desktop-file-utils, and xdg-utils when registering. It does not require elevation and refuses root. An interrupted install retains its ownership record for explicit cleanup. Source inspection is not a native clean-install/uninstall acceptance receipt.

## Remaining acceptance

Native launcher, registration, AppKit delivery, linked Windows PE icon/version resources, package resources, settings migration, patcher invocation and architecture/OS compatibility require commit-bound installed-artifact receipts. macOS signing/notarization, public release publication and any support promotion remain separate work. F23/F28 remain partial.

## Producing build and manual artifact lanes (source only)

`produce_build.py` closes the build-receipt creation boundary. Explicitly invoke it with a clean exact committed checkout, a retained target, an already installed exact Rust toolchain and a **new output directory outside the checkout**. It builds only the production binaries in an isolated Cargo target directory using `--locked --offline --release`. Prepare dependencies/toolchain separately; the helper never installs them or runs the resulting programs.

```text
python3 packaging/produce_build.py --commit <full-commit> --target x86_64-unknown-linux-gnu --toolchain 1.95.0 --output-dir /new/build-output
```

It retains actual command arguments, toolchain verbose versions, host identity, start/end source and lockfile identity, executable hashes, and a digest-bound compiler artifact inventory. Source changes or build failures produce a failure record instead of a usable build receipt. Windows additionally requires the explicit resource object/receipt; the helper snapshots and verifies them, supplies the target-specific linker argument for both binary roles, and rechecks the resource after building. It clears inherited compiler/wrapper and Rust flag overrides; native C toolchain/environment still affects builds and bit reproducibility is not claimed. The output build tree is retained for inspection; this helper never deletes user outputs.

`.github/workflows/package-manual.yml` is manually dispatched or explicitly called, with a required exact commit. Independent target lanes prepare dependencies, invoke this offline producer, assemble packages when required assets exist, and retain binaries/receipts/packages for 14 days. There are no tests, native probes, protocol registration, public-release permissions or publication steps. Every lane remains `nativeAcceptance=unverified`. The native-icon inputs default to the exact pinned same-project assets in `packaging/assets/modsync.ico` and `packaging/assets/modsync.icns`; their hashes, source binding and license limitations are retained in [assets/PROVENANCE.md](assets/PROVENANCE.md). An explicitly empty or unavailable ICO still blocks Windows before producing its resource-bound binaries; an unavailable ICNS blocks macOS package assembly while preserving its completed build receipt. An unavailable runner cannot produce evidence. These limitations keep all seven target rows visible rather than treating them as successful.

Action source pins were resolved read-only from the official repositories: checkout v6 `d23441a48e516b6c34aea4fa41551a30e30af803` ([source](https://github.com/actions/checkout/tree/d23441a48e516b6c34aea4fa41551a30e30af803)) and upload-artifact v6 `b7c566a772e6b6bfb58ed0dc250532a479d7789f` ([source](https://github.com/actions/upload-artifact/tree/b7c566a772e6b6bfb58ed0dc250532a479d7789f)). Their manifests select Node24; their documentation requires runner 2.327.1 or newer. Hosted runner availability and actual artifact production remain unverified. No workflow was dispatched or queried during implementation.

Native asset preparation copied existing files without image conversion or native resource execution. The full current source license is retained in `packaging/assets/LICENSE.txt`; the ordinary package already includes the same root license. This does not assert separate logo permission or authorize publication.
