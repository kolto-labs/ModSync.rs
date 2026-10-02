#!/usr/bin/env python3
"""Offline production package assembly. Never builds, downloads, installs, or publishes."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def regular(path):
    path = Path(path)
    require(path.is_file() and not path.is_symlink(), f"Not a regular input file: {path}")
    return path


def digest(path):
    with regular(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def git(*arguments):
    return subprocess.check_output(["git", "-C", str(ROOT), *arguments], text=True).strip()


def source_state():
    return {"commit": git("rev-parse", "HEAD"),
            "status": git("status", "--porcelain", "--untracked-files=all"),
            "cargoLockSha256": digest(ROOT / "Cargo.lock")}


def binary(path, target, role):
    """Inspect file headers, not host execution. Thin Mach-O only; no silent universal slice selection."""
    with regular(path).open("rb") as source:
        header = source.read(64)
        arch = None
        if target["os"] == "linux":
            require(len(header) == 64 and header[:4] == b"\x7fELF" and header[5] == 1 and struct.unpack_from("<H", header, 16)[0] in (2, 3), "Expected executable little-endian ELF")
            arch = {62: "x86_64", 183: "aarch64", 3: "x86"}.get(struct.unpack_from("<H", header, 18)[0])
        elif target["os"] == "windows":
            require(header[:2] == b"MZ" and len(header) == 64, "Expected Windows PE")
            source.seek(struct.unpack_from("<I", header, 60)[0])
            pe = source.read(94)
            require(len(pe) == 94 and pe[:4] == b"PE\0\0", "Invalid PE header")
            arch = {0x14C: "x86", 0x8664: "x86_64", 0xAA64: "aarch64"}.get(struct.unpack_from("<H", pe, 4)[0])
            if role in ("console", "gui"):
                require(struct.unpack_from("<H", pe, 92)[0] == (3 if role == "console" else 2), f"Wrong PE subsystem for {role}")
        else:
            require(len(header) == 64 and header[:4] == b"\xcf\xfa\xed\xfe" and struct.unpack_from("<I", header, 12)[0] == 2, "Expected executable thin little-endian Mach-O 64")
            arch = {0x1000007: "x86_64", 0x100000C: "aarch64"}.get(struct.unpack_from("<I", header, 4)[0])
        require(arch == target["architecture"], f"Binary architecture {arch!r} differs from target")


def sha256_text(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def validate_build_receipt(build, commit, target, lock_digest):
    require(build.get("schemaVersion") == 1 and build.get("status") == "built", "A successful producing build receipt is required")
    require(build.get("sourceCommit") == commit and build.get("target") == target and build.get("cargoLockSha256") == lock_digest, "Build receipt source/lock/target differs")
    expected = {"commit": commit, "status": "", "cargoLockSha256": lock_digest}
    require(build.get("sourceStart") == expected and build.get("sourceEnd") == expected, "Build receipt must prove unchanged clean source at both boundaries")
    require(build.get("offline") is True and build.get("publication") is False and build.get("nativeAcceptance") == "unverified", "Unsupported producing build receipt policy")
    require(sha256_text(build.get("buildtreeSha256")), "Build receipt lacks its artifact inventory binding")
    started, finished = build.get("startedUnix"), build.get("finishedUnix")
    require(type(started) is int and type(finished) is int and 0 <= started <= finished, "Build receipt completion time is invalid")
    require(isinstance(build.get("binaries"), dict) and all(sha256_text(value) for value in build["binaries"].values()), "Invalid producing binary hashes")


def validate_windows_resource(resource, target, package_version=None):
    require(resource.get("schemaVersion") == 1 and resource.get("target") == target, "Resource receipt target differs")
    version = resource.get("version")
    require(isinstance(version, str), "Resource version is missing")
    parts = version.split(".")
    require(len(parts) == 4 and all(part.isascii() and part.isdecimal() and int(part) <= 65535 for part in parts), "Invalid Windows resource version")
    if package_version is not None:
        release = package_version.split(".")
        require(len(release) == 3 and all(part.isascii() and part.isdecimal() and int(part) <= 65535 for part in release), "Windows package version requires three decimal components in 0..65535")
        require(version == package_version + ".0", "Resource version differs from package version")
    template = regular(ROOT / "packaging/windows_resources.rc.in").read_bytes()
    require(resource.get("templateSha256") == hashlib.sha256(template).hexdigest(), "Resource template differs from committed source")
    rendered = template.decode().replace("@ICON@", "icon.ico").replace("@VERSION_NUMERIC@", ",".join(parts)).replace("@VERSION_TEXT@", version)
    require(resource.get("renderedRcSha256") == hashlib.sha256(rendered.encode()).hexdigest(), "Rendered resource source differs from receipt")
    require(sha256_text(resource.get("objectSha256")) and sha256_text(resource.get("iconSha256")), "Invalid resource payload hashes")


def copy(source, destination, executable=False):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(regular(source), destination)
    destination.chmod(0o755 if executable else 0o644)


def json_file(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    path.chmod(0o644)


def archive_tree(root, output, epoch, vehicle):
    files = sorted(path for path in root.rglob("*") if path.is_file())
    if vehicle == "tar.gz":
        with output.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for path in files:
                    entry = archive.gettarinfo(str(path), arcname=str(path.relative_to(root.parent)))
                    entry.uid = entry.gid = 0
                    entry.uname = entry.gname = ""
                    entry.mtime = epoch
                    with path.open("rb") as source:
                        archive.addfile(entry, source)
    else:
        import time
        stamp = time.gmtime(max(epoch, 315532800))[:6]
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for path in files:
                entry = zipfile.ZipInfo(path.relative_to(root.parent).as_posix(), date_time=stamp)
                entry.create_system = 3
                entry.external_attr = (0o100000 | (path.stat().st_mode & 0o777)) << 16
                entry.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(entry, path.read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--gui-launcher", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--build-receipt", type=Path, required=True, help="Commit/lock/target and executable hashes from the producing build")
    parser.add_argument("--version", required=True)
    parser.add_argument("--patcher", type=Path)
    parser.add_argument("--patcher-engine", choices=["OdyPatcher", "HoloPatcher", "KPatcher"])
    parser.add_argument("--patcher-license", type=Path)
    parser.add_argument("--platform-icon", type=Path, help="Reviewed .ico on Windows or .icns on macOS")
    parser.add_argument("--windows-resource-receipt", type=Path, help="Receipt from explicit Windows resource object compilation")
    args = parser.parse_args()
    matrix = json.loads((ROOT / "packaging/targets.json").read_text())
    require(args.target in matrix["targets"], "Target is not retained in packaging/targets.json")
    target = matrix["targets"][args.target]
    initial_source = source_state()
    require(initial_source["commit"] == args.commit, "Package commit must be the checked-out full HEAD")
    require(not initial_source["status"], "Release input checkout must be clean, including untracked files")
    require(args.version and all(c.isalnum() or c in ".-_" for c in args.version), "Unsafe version spelling")
    binary(args.binary, target, "console")
    require(regular(args.build_receipt).stat().st_size <= 1024 * 1024, "Build receipt exceeds 1 MiB")
    build_bytes = args.build_receipt.read_bytes()
    require(len(build_bytes) <= 1024 * 1024, "Build receipt exceeds 1 MiB")
    build = json.loads(build_bytes)
    build_digest = hashlib.sha256(build_bytes).hexdigest()
    registration_token = f"org.modsync.rust.{args.commit}.{build_digest[:16]}"
    validate_build_receipt(build, args.commit, args.target, digest(ROOT / "Cargo.lock"))
    require(build["binaries"]["modsync"] == digest(args.binary), "Console binary differs from build receipt")
    if target["os"] == "windows":
        require(args.gui_launcher is not None, "Windows needs its GUI-subsystem launcher")
        binary(args.gui_launcher, target, "gui")
        require(build["binaries"]["modsync-gui"] == digest(args.gui_launcher), "GUI launcher differs from build receipt")
        require(args.windows_resource_receipt is not None, "Windows requires a resource compilation receipt")
        require(regular(args.windows_resource_receipt).stat().st_size <= 1024 * 1024, "Resource receipt exceeds 1 MiB")
        resource_bytes = args.windows_resource_receipt.read_bytes()
        require(len(resource_bytes) <= 1024 * 1024, "Resource receipt exceeds 1 MiB")
        resource = json.loads(resource_bytes)
        validate_windows_resource(resource, args.target, args.version)
        require(build["windowsResources"] == {"receiptSha256": hashlib.sha256(resource_bytes).hexdigest(), "objectSha256": resource["objectSha256"], "iconSha256": resource["iconSha256"]}, "Build receipt is not bound to this resource object and icon")
    if target["os"] in ("windows", "macos"):
        require(args.platform_icon is not None, "Windows/macOS require a reviewed native icon input")
        require(args.platform_icon.suffix.lower() == (".ico" if target["os"] == "windows" else ".icns"), "Wrong platform icon extension")
        regular(args.platform_icon)
    icon_hash = digest(args.platform_icon) if args.platform_icon else None
    if target["os"] == "windows":
        require(icon_hash == resource["iconSha256"], "Package icon differs from linked resource receipt")
    require(bool(args.patcher) == bool(args.patcher_engine) == bool(args.patcher_license), "Patcher, engine and license must be supplied together")
    patcher = {"available": False, "reason": "No reviewed native backend bundled; explicit external patcher is a separate user trust decision."}
    reviewed = None
    if args.patcher:
        binary(args.patcher, target, "patcher")
        allowlist = json.loads((ROOT / "packaging/patcher-provenance.json").read_text())
        matches = [record for record in allowlist["patchers"] if record["target"] == args.target and record["engine"] == args.patcher_engine and record["sha256"] == digest(args.patcher)]
        require(len(matches) == 1, "Patcher needs exactly one version-controlled reviewed provenance entry")
        reviewed = matches[0]
        require(reviewed["source"] and reviewed["license"], "Reviewed source and license are required")
        require(reviewed["licenseSha256"] == digest(args.patcher_license), "Patcher license differs from reviewed provenance")
    epoch = int(git("show", "-s", "--format=%ct", args.commit))
    args.output_dir.mkdir(parents=True, exist_ok=True)
    name = f"modsync-{args.version}-{args.target}-{args.commit[:12]}-prerelease"
    final = args.output_dir / f"{name}.{target['vehicle']}"
    receipt = args.output_dir / f"{name}.receipt.json"
    require(not final.exists() and not receipt.exists(), "Artifact or receipt already exists")
    with tempfile.TemporaryDirectory(prefix=".modsync-package-", dir=args.output_dir) as temporary:
        scratch = Path(temporary)
        root = scratch / ("ModSync.app" if target["os"] == "macos" else name)
        executables = root / "Contents/MacOS" if target["os"] == "macos" else root
        resources = root / "Contents/Resources" if target["os"] == "macos" else root / "Resources"
        staged_binary = executables / ("modsync.exe" if target["os"] == "windows" else "modsync")
        copy(args.binary, staged_binary, True)
        require(digest(staged_binary) == build["binaries"]["modsync"], "Console binary changed during staging")
        if args.gui_launcher and target["os"] == "windows":
            copy(args.gui_launcher, executables / "modsync-gui.exe", True)
            require(digest(executables / "modsync-gui.exe") == build["binaries"]["modsync-gui"], "GUI launcher changed during staging")
        copy(ROOT / "LICENSE.txt", resources / "LICENSE.txt")
        # Retain the binding and native decoder notices separately, byte for byte.
        # This is a scoped vendored dependency inventory, not a complete license audit.
        for relative in ("README.md", "LICENSE-MIT", "LICENSE-APACHE", "vendor/unrar/license.txt", "vendor/unrar/acknow.txt"):
            source_notice = ROOT / "vendor/unrar_sys" / relative
            expected_notice = digest(source_notice)
            staged_notice = resources / "licenses/unrar_sys" / relative
            copy(source_notice, staged_notice)
            require(digest(staged_notice) == expected_notice, "UnRAR notice changed during staging")
        copy(ROOT / "README.md", resources / "README.md")
        copy(ROOT / "packaging/README.md", resources / "PACKAGE-README.md")
        copy(ROOT / "assets/fonts/NotoEmoji-OFL.txt", resources / "NotoEmoji-OFL.txt")
        copy(ROOT / "packaging/modsync.svg", resources / "modsync.svg")
        if args.platform_icon:
            staged_icon = resources / ("modsync.ico" if target["os"] == "windows" else "modsync.icns")
            copy(args.platform_icon, staged_icon)
            require(digest(staged_icon) == icon_hash, "Icon changed during staging")
        if target["os"] == "macos":
            require(1 <= len(args.version.split('.')) <= 3 and all(part.isascii() and part.isdecimal() for part in args.version.split('.')), "macOS version requires one to three decimal components")
            info = {"CFBundleIdentifier": "org.modsync.rust.stable", "CFBundleName": "ModSync", "CFBundleExecutable": "modsync", "CFBundlePackageType": "APPL", "CFBundleVersion": args.version, "CFBundleShortVersionString": args.version, "CFBundleIconFile": "modsync.icns", "NSHighResolutionCapable": True, "ModSyncPackageToken": registration_token, "ModSyncChannel": "stable", "CFBundleURLTypes": [{"CFBundleURLName": "ModSync and Nexus links", "CFBundleURLSchemes": ["modsync", "nxm"]}]}
            (root / "Contents/Info.plist").write_bytes(plistlib.dumps(info, sort_keys=True))
        if target["os"] == "linux":
            copy(ROOT / "packaging/linux_install.py", root / "install.py", True)
            copy(ROOT / "packaging/linux_launch.py", root / "launch.py", True)
        (resources / "build-receipt.json").write_bytes(build_bytes)
        (resources / "build-receipt.json").chmod(0o644)
        if target["os"] == "windows":
            (resources / "windows-resource-receipt.json").write_bytes(resource_bytes)
            (resources / "windows-resource-receipt.json").chmod(0o644)
        if reviewed:
            leaf = {"OdyPatcher": "odypatcher", "HoloPatcher": "holopatcher", "KPatcher": "kpatcher"}[args.patcher_engine] + (".exe" if target["os"] == "windows" else "")
            payload = resources / leaf
            copy(args.patcher, payload, True)
            copy(args.patcher_license, resources / "PATCHER-LICENSE.txt")
            require(digest(payload) == reviewed["sha256"], "Patcher changed during staging")
            require(digest(resources / "PATCHER-LICENSE.txt") == reviewed["licenseSha256"], "Patcher license changed during staging")
            patcher = {"available": True, "engine": args.patcher_engine, "path": payload.relative_to(root).as_posix(), "sha256": reviewed["sha256"], "source": reviewed["source"], "license": reviewed["license"], "licenseSha256": reviewed["licenseSha256"]}
        manifest_path = resources / "modsync-package.json" if target["os"] == "macos" else root / "modsync-package.json"
        entries = [{"path": p.relative_to(root).as_posix(), "sha256": digest(p), "size": p.stat().st_size, "mode": oct(p.stat().st_mode & 0o777)} for p in sorted(root.rglob("*")) if p.is_file()]
        manifest = {"schemaVersion": 1, "packageId": "org.modsync.rust.stable", "registrationToken": registration_token, "channel": "stable", "version": args.version, "target": args.target, "sourceCommit": args.commit, "cargoLockSha256": digest(ROOT / "Cargo.lock"), "patcherProvenanceSha256": digest(ROOT / "packaging/patcher-provenance.json"), "publication": False, "distributionStatus": "unsigned-unnotarized-prerelease", "nativeAcceptance": "unverified", "patcher": patcher, "files": entries}
        json_file(manifest_path, manifest)
        archive = scratch / final.name
        archive_tree(root, archive, epoch, target["vehicle"])
        record = {"schemaVersion": 1, "sourceCommit": args.commit, "target": args.target, "archive": final.name, "sha256": digest(archive), "manifestSha256": digest(manifest_path), "buildReceiptSha256": build_digest, "publication": False, "nativeAcceptance": "unverified", "binaryBuildProvenance": "supplied build receipt verified for commit/lock/target/binary hashes; build issuer authentication is external"}
        # Source resources and provenance were read over time. Reject drift before
        # publishing the assembled bytes under the captured commit identity.
        require(source_state() == initial_source, "Source or lockfile changed during package assembly")
        # No overwrite: a competing publisher or rerun must not replace an artifact.
        os.link(archive, final)
        try:
            with receipt.open("x", encoding="utf-8") as output:
                json.dump(record, output, indent=2, sort_keys=True)
                output.write("\n")
        except Exception:
            raise RuntimeError(f"Artifact retained at {final}; receipt publication failed")
    print(str(final))


if __name__ == "__main__":
    main()
