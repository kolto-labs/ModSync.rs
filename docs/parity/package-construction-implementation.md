# F28 offline package construction source

Status: source only. No packaging helper was executed, no package/artifact/fixture was generated, no binary was run, and no native registration or CI/release action was performed. F28 remains partial.

The implementation follows platform plan U4/KTD5–KTD9. `packaging/targets.json` retains seven architecture targets and names unresolved older-Windows exceptions. `build_package.py` requires clean committed inputs, a producing build receipt bound to target/commit/Cargo.lock and binary hashes, and actual ELF/PE/Mach-O architecture/subsystem headers. It assembles Linux tar.gz, Windows dual-launcher ZIP and macOS app ZIP vehicles, records every payload's hash/length/mode, fixes archive metadata from commit time, and publishes without overwriting. Supplied build receipt authentication remains external; the package never claims native acceptance.

External binary/icon/patcher/license inputs are verified after staging, against captured receipt/provenance values. The exact verified build/resource receipt bytes are retained, rather than re-reading mutable paths later. Patcher capability is absent by default: the checked-in allowlist contains no invented payload hashes. When a reviewed payload is supplied, both backend and license digests, target, engine, source and license identity must match the allowlist. Runtime copies that authority into `Invocation::Packaged`; it captures manifest digest and provenance and requires both unchanged manifest and selected file digest immediately before command construction. Explicit external `Native` and `Python` invocation shapes remain intact.

The Windows GUI-subsystem wrapper owns only forwarding to the sibling console companion and visible startup errors. The actual macOS application executable remains `modsync` for AppKit callbacks. The resource compiler helper is an explicitly invoked production build step with no automatic Cargo hook; object/icon/template/RC hashes feed the producing receipt and final executable binding.

Linux install/uninstall creates a visible package-specific desktop launcher and delegates MIME registration/ownership to the shared Rust protocol CLI. It rejects nonabsolute XDG roots, symlink ancestors and source control characters; desktop Exec escaping accounts for both parsing layers. Package/channel ownership tokens are shared between manifest, installer receipt and macOS Info.plist.

Frozen inline patcher tests/helpers were preserved. The parent owns production compilation, the new GUI binary target registration, independent review, and all future acceptance decisions. Native behavior, signing/notarization, real payload license review and public release construction/retention workflow U6 remain separate outstanding work.

## Local backend inventory (source and file evidence only)

Read-only inventory on 2026-10-02 examined the pinned C# checkout at `/home/brunner56/.codex/worktrees/5813/ModSync`. No payload was executed, downloaded, or copied into this Rust checkout.

| Pinned payload | Observed file architecture | SHA-256 |
| --- | --- | --- |
| `vendor/bin/HoloPatcher.exe` | PE32 i386 GUI | `b15441bd2d9b7f5ff8a119611cc898d43d5e6921bccfce087e38d20fe374d7a8` |
| `vendor/bin/HoloPatcher_linux` | ELF x86-64, dynamically linked | `fdd470570a82df58be8913074133ab13ba94a874e69d66b0592ebe3f5af230c5` |
| `vendor/bin/holopatcher.app/Contents/MacOS/holopatcher` | Thin Mach-O x86-64 | `2360a960dbd3ac21080cb66d6a5bb42061fb10c2e4a45c6d52837c16eff021cf` |

The macOS bundle reports version `0.0.0`; its executable plist spelling also differs in case from the payload filename. Project copy rules reference this same legacy macOS payload for both x64 and ARM64, which does not establish ARM64 support. The pinned `.gitmodules` names a patcher source repository, but the pinned tree does not contain a corresponding `vendor/KPatcher` gitlink/source binding. No adjacent payload-specific license, exact backend source revision, or producing build receipt was found. The ModSync repository's own BSL license does not establish licenses for bundled backend/dependency bytes.

The separate working C# checkout at `/run/media/brunner56/MyBook/Workspaces/ModSync` additionally contains tracked `vendor/bin/odypatcher`: ELF x86-64, SHA-256 `e694b93c8fcac63d5c6bb8cfcdc1f4eb8d19493f88057d2d96a9f7efba9b2e06`, Git blob `487b8f0f7aa06e7a6149010a655867dbb8c8c16c`. Its latest path-changing commit is `983b0e6592f5c83814f23b9ee0ec823a485ba8a8`. This identifies repository origin and bytes, without establishing a reviewed source/license/build binding.

The compiled provenance allowlist therefore remains empty. Populating it requires the missing source revision, license and dependency obligations, and producing-build provenance review. File architecture and hashes alone do not establish backend compatibility or redistribution authority.

### Additional OdyPatcher source and artifact inventory

A read-only follow-up found `/run/media/brunner56/MyBook/Workspaces/bioware_projects/OdyPatcher` at Git HEAD `859aba157591a27f3dfe71f3db8c0d398bdd5c34`. The checkout has modified source, Cargo manifests/lockfile, workflows and documentation. HEAD declares version `0.1.0`; the working manifest declares `0.2.0`. Its Cargo license expression is `AGPL-3.0-or-later`; the local AGPLv3 `LICENSE` file SHA-256 is `8486a10c4393cee1c25392769ddd3b2d6c242d6ec7928e1414efff7dfb2f07ef`.

Untracked `dist/platforms/github-run-34549487818` contains seven executable targets whose headers confirm the architectures below. Directory names identify retained artifact groups, but are not authenticated producing-build receipts.

| Artifact directory | Observed architecture | Executable SHA-256 |
| --- | --- | --- |
| `odypatcher-linux-x86_64` | ELF x86-64 | `522e476deba29ae097e7ce6fbf72c514803c3c2132dc170f34dde0fd44838528` |
| `odypatcher-linux-arm64` | ELF AArch64 | `fbbe012dd2c184d30ceade50ca6eeb067d7bddf1e6093655bebbc2c99f8a9e77` |
| `odypatcher-macos-x86_64` | Mach-O x86-64 | `10e8eb5633edf5d75e28e12c2a4867ff5c95ec6335c30c49cf1d174390bac449` |
| `odypatcher-macos-arm64` | Mach-O arm64 | `7853d8ebbdeddded7f41c7cece0bfcd6a4fa6cceeb0481ed6fc752619544eb0b` |
| `odypatcher-windows-x86` | PE i386 console | `98a2da3885c329d71379890e5dccfd30ea342109776c83dd9e2a0bff0db444a1` |
| `odypatcher-windows-x86_64` | PE x86-64 console | `4a5f4d3a49aaaca5f419b7a43509cad8d0ca6dd807cee1c92a01b668a4b25390` |
| `odypatcher-windows-arm64` | PE ARM64 console | `ea21429fbe0ad28f9f74a90d4ecdb340eaca5a7d248cb6a59077107d2c93733b` |

The earlier retained run `34548928529` has matching Linux/macOS bytes and different Windows bytes. The local `target/release/odypatcher` has SHA-256 `366f51bd3d3cbec16e89c8c1b9325a70fecfc2109eabcb0bf90a7f837eacffae`; other local static/GUI variants also differ. None of these inspected Ody artifacts matches the working ModSync vendor payload recorded above. Retained artifact directories expose binaries and archives without a source/run receipt. HEAD's workflow archives a bare executable; the dirty working workflow's newer license/checksum staging cannot retrospectively establish the retained artifacts' provenance. These are concrete candidates for a later producing-build/source review, not allowlist entries. Nothing was executed, copied or fetched.

## U6 producing receipt and manual workflow source

`packaging/produce_build.py` now provides the previously missing producing-build receipt path: exact clean source before/after, locked offline production-only build, isolated retained target directory, toolchain/host/argv identity, hashed compiler artifact inventory, and hashes of architecture-checked copied executables. Windows resource snapshots are digest-bound before and after the build and linked through target-specific Cargo flags. Failed attempts retain failure records and cannot produce a successful package input receipt.

The new `package-manual.yml` is manual/workflow-call only, with read-only repository permission, no publication, no tests/native execution, independent target lanes, exact commit binding and 14-day receipt/artifact retention. Official checkout/upload action source pins and Node24 runner requirement are recorded in the packaging README. Toolchain and dependency preparation occurs explicitly before the offline helper. Native Windows/macOS icon prerequisites remain explicit: missing resources fail the respective package capability, with completed build evidence retained where available. No workflow, producing build, package helper, binary, test or native registration was run for this implementation. U6 source exists; actual execution/retention and U7 native acceptance remain outstanding.

## Producing-receipt and Windows resource qualification correction

Package assembly now requires the producing helper's successful `status=built`
receipt. Both source boundaries must equal the requested commit, clean Git status,
and current lockfile hash. The artifact-inventory digest, ordered start/completion
timestamps, binary hash shapes, and offline/unpublished/unverified policy fields
must also be present and valid. A `build-failure.json` with already populated
binary hashes cannot be repurposed as a successful build receipt. The inventory
hash is a retained binding, not independent authentication of the issuer.

Both the producer and package assembler now compare Windows resource receipts
against the tracked RC template and recompute the rendered RC digest using the
recorded four-component version. The assembler additionally requires the resource
version to be exactly the three-component Windows package version plus `.0`.
All version components are bounded to 0..65535. This prevents a differently
versioned or stale-template resource from passing merely because its object and
icon hashes agree with its own supplied receipt. Native PE resource inspection
and issuer authentication remain separate acceptance requirements.

Source-only correction: no helper invocation, binary build, test, harness,
application execution, native registration, or CI action was performed.

The package now also retains the vendored `unrar_sys/README.md` provenance,
binding `LICENSE-MIT` and `LICENSE-APACHE`, and complete native UnRAR
`vendor/unrar/license.txt` and `vendor/unrar/acknow.txt` under
`Resources/licenses/unrar_sys` (macOS: `Contents/Resources/licenses/unrar_sys`).
Their relative layout and bytes are preserved, copied bytes are rehashed, and
all five files enter the existing package manifest inventory. This is a scoped
notice-delivery correction, not a complete dependency-license audit.

## Backend producing source continuation

`packaging/produce_patcher.py` now defines an explicit offline OdyPatcher CLI
producer for an exact clean caller-supplied backend checkout. It retains a source
archive and license, verifies an explicit reviewed license digest, binds the root
Cargo package through metadata and emitted binary records, checks copied
executable architecture/digest, and records clean source boundaries and failures.
It neither runs the backend nor populates the provenance allowlist. The receipt
explicitly reports a nonhermetic build and pending dependency-license review;
tracked source alone does not cover inherited/ignored configuration or external
path dependencies. This source was independently reviewed, including correction
of a workspace default-member/package ambiguity. Actual producing builds,
authenticated source/license obligations, admission and installed patcher
acceptance remain open. No helper or backend was executed.

Assembly also captures full clean Git status (including every untracked file),
HEAD and lockfile digest before intake, then compares the same state immediately
before no-clobber archive publication. Persistent source drift during resource or
provenance staging rejects publication. External input and staged-byte digest
checks remain in place. This checks the start/end states; it does not claim to
detect an adversarial source change that is completely reverted between checks.
