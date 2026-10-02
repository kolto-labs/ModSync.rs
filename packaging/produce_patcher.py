#!/usr/bin/env python3
"""Explicit offline OdyPatcher producing receipt; no payload execution or publication."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time
import tomllib

sys.dont_write_bytecode = True

from build_package import ROOT, binary, digest, regular, require
from produce_build import write_new


def command_output(command, source, environment=None):
    with subprocess.Popen(command, cwd=source, env=environment, stdout=subprocess.PIPE) as process:
        try:
            value = process.stdout.read(16 * 1024 * 1024 + 1)
            require(len(value) <= 16 * 1024 * 1024, "Producer command output exceeds 16 MiB")
            require(process.wait() == 0, "Producer intake command failed")
            return value.decode("utf-8").strip()
        except BaseException:
            process.kill()
            process.wait()
            raise


def git(source, *arguments):
    return command_output(["git", "-C", str(source), *arguments], source)


def state(source):
    return {"commit": git(source, "rev-parse", "HEAD"),
            "status": git(source, "status", "--porcelain", "--untracked-files=all"),
            "cargoLockSha256": digest(source / "Cargo.lock"),
            "cargoManifestSha256": digest(source / "Cargo.toml"),
            "licenseSha256": digest(source / "LICENSE")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend-source", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--toolchain", required=True)
    parser.add_argument("--license-sha256", required=True,
                        help="Previously reviewed backend LICENSE digest")
    parser.add_argument("--output-dir", type=Path, required=True,
                        help="New external directory; all failure evidence is retained")
    args = parser.parse_args()
    require(re.fullmatch(r"[0-9a-f]{40}", args.commit), "Use an exact full source commit")
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.toolchain),
            "Use an exact installed Rust toolchain release")
    require(re.fullmatch(r"[0-9a-f]{64}", args.license_sha256), "Invalid reviewed license digest")
    source = args.backend_source.resolve(strict=True)
    require(source.is_dir(), "Backend source must be a directory")
    require(Path(git(source, "rev-parse", "--show-toplevel")).resolve() == source,
            "Choose the backend repository root")
    initial = state(source)
    require(initial["commit"] == args.commit and not initial["status"],
            "Backend build requires the exact clean committed checkout")
    require(initial["licenseSha256"] == args.license_sha256, "Reviewed backend license differs")
    # An archive of only the parent commit would omit gitlink source. Until a
    # source-complete participant contract exists, reject that input shape.
    entries = git(source, "ls-tree", "-r", args.commit).splitlines()
    require(len(entries) <= 100_000 and not any(line.startswith("160000 ") for line in entries),
            "Backend source inventory is too large or contains unbound submodules")
    require(regular(source / "Cargo.toml").stat().st_size <= 1024 * 1024,
            "Backend Cargo manifest exceeds 1 MiB")
    with (source / "Cargo.toml").open("rb") as input_file:
        manifest_bytes = input_file.read(1024 * 1024 + 1)
    require(len(manifest_bytes) <= 1024 * 1024, "Backend Cargo manifest exceeds 1 MiB")
    manifest = tomllib.loads(manifest_bytes.decode("utf-8"))
    package = manifest.get("package", {})
    require(package.get("name") == "odypatcher" and package.get("license") == "AGPL-3.0-or-later",
            "Producer admits only the reviewed standalone OdyPatcher package/license shape")
    require(isinstance(package.get("version"), str), "Backend package version is missing")
    target = json.loads((ROOT / "packaging/targets.json").read_text())["targets"].get(args.target)
    require(target is not None, "Unknown retained package target")
    output = args.output_dir.resolve()
    require(not output.is_relative_to(source) and not output.is_relative_to(ROOT),
            "Backend output must be outside both source checkouts")
    output.mkdir(parents=True, exist_ok=False)
    build = output / "buildtree"
    environment = os.environ.copy()
    for name in list(environment):
        if name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER") or name.startswith("CARGO_TARGET_") and name.endswith("_RUSTFLAGS"):
            environment.pop(name, None)
    environment["CARGO_TARGET_DIR"] = str(build)
    environment["CARGO_NET_OFFLINE"] = "true"
    rustup = shutil.which("rustup")
    receipt = {"schemaVersion": 1, "engine": "ody", "sourceCommit": args.commit,
               "target": args.target, "packageVersion": package["version"],
               "license": package["license"], "licenseSha256": args.license_sha256,
               "sourceStart": initial, "status": "building", "offline": True,
               "publication": False, "nativeAcceptance": "unverified",
               "hermetic": False, "dependencyLicenseReview": "pending",
               "host": {"os": platform.system(), "architecture": platform.machine()},
               "startedUnix": int(time.time()), "commands": []}
    try:
        require(rustup is not None, "An installed rustup toolchain is required")
        for executable in ("rustc", "cargo"):
            command = [rustup, "run", args.toolchain, executable, "--version", "--verbose"]
            receipt["commands"].append(command)
            receipt[executable] = command_output(command, source, environment)
        metadata_command = [rustup, "run", args.toolchain, "cargo", "metadata", "--locked",
                            "--offline", "--no-deps", "--format-version=1"]
        receipt["commands"].append(metadata_command)
        metadata_text = command_output(metadata_command, source, environment)
        metadata = json.loads(metadata_text)
        owned_packages = [item for item in metadata.get("packages", [])
                          if Path(item["manifest_path"]).resolve() == source / "Cargo.toml"]
        require(len(owned_packages) == 1 and owned_packages[0]["name"] == "odypatcher",
                "Cargo metadata does not identify the reviewed root backend package")
        package_id = owned_packages[0]["id"]
        receipt["packageId"] = package_id
        with (output / "cargo-metadata.json").open("x", encoding="utf-8") as metadata_file:
            metadata_file.write(metadata_text + "\n")
        receipt["cargoMetadataSha256"] = digest(output / "cargo-metadata.json")
        # Retain exact tracked backend source. This archive is evidence for
        # review; dependency/source-distribution obligations remain separate.
        archive = output / "backend-source.tar"
        command = ["git", "-C", str(source), "archive", "--format=tar", "--output=" + str(archive), args.commit]
        receipt["commands"].append(command)
        subprocess.run(command, cwd=source, check=True)
        receipt["sourceArchiveSha256"] = digest(archive)
        license_copy = output / "LICENSE"
        shutil.copyfile(regular(source / "LICENSE"), license_copy)
        require(digest(license_copy) == args.license_sha256, "Backend license changed while copying")
        command = [rustup, "run", args.toolchain, "cargo", "build", "--locked", "--offline",
                   "--release", "--no-default-features", "--target", args.target,
                   "--package", "odypatcher", "--bin", "odypatcher",
                   "--message-format=json-render-diagnostics", "-j2"]
        receipt["commands"].append(command)
        artifacts = set()
        backend_executables = set()
        with subprocess.Popen(command, cwd=source, env=environment, stdout=subprocess.PIPE, text=True) as process:
            try:
                while True:
                    line = process.stdout.readline(16 * 1024 * 1024 + 1)
                    if not line:
                        break
                    require(len(line) <= 16 * 1024 * 1024, "Cargo message exceeds bound")
                    event = json.loads(line)
                    if event.get("reason") == "compiler-artifact":
                        artifacts.update(event.get("filenames", []))
                        require(len(artifacts) <= 100_000, "Backend artifact inventory exceeds bound")
                        emitted_target = event.get("target", {})
                        if (event.get("package_id") == package_id
                                and emitted_target.get("name") == "odypatcher"
                                and emitted_target.get("kind") == ["bin"]
                                and event.get("executable")):
                            backend_executables.add(Path(event["executable"]).resolve())
                    elif event.get("reason") == "compiler-message":
                        print(event.get("message", {}).get("rendered", ""), file=sys.stderr, end="")
                require(process.wait() == 0, "Backend production build failed")
            except BaseException:
                process.kill()
                process.wait()
                raise
        records = []
        for filename in sorted(artifacts):
            path = Path(filename).resolve(strict=True)
            require(path.is_relative_to(build), "Backend artifact escaped isolated build directory")
            records.append({"path": path.relative_to(build).as_posix(),
                            "sha256": digest(path), "size": path.stat().st_size})
        write_new(output / "buildtree.json", {"schemaVersion": 1, "artifacts": records})
        receipt["buildtreeSha256"] = digest(output / "buildtree.json")
        filename = "odypatcher.exe" if target["os"] == "windows" else "odypatcher"
        produced = build / args.target / "release" / filename
        require(backend_executables == {produced},
                "Cargo did not emit exactly the reviewed root backend executable")
        expected = digest(produced)
        require(any(record["path"] == produced.relative_to(build).as_posix()
                    and record["sha256"] == expected for record in records),
                "Backend executable differs from compiler artifact inventory")
        binary(produced, target, "console")
        copied = output / filename
        shutil.copyfile(regular(produced), copied)
        copied.chmod(0o755)
        require(digest(copied) == expected, "Backend executable changed while copying")
        receipt["sha256"] = expected
        receipt["sourceEnd"] = state(source)
        require(receipt["sourceEnd"] == initial, "Backend source or license changed during build")
        receipt["status"] = "built"
        receipt["finishedUnix"] = int(time.time())
        write_new(output / "patcher-build-receipt.json", receipt)
    except BaseException as error:
        receipt["status"] = "failed"
        receipt["errorType"] = type(error).__name__
        receipt["finishedUnix"] = int(time.time())
        write_new(output / "patcher-build-failure.json", receipt)
        raise
    print(output / "patcher-build-receipt.json")


if __name__ == "__main__":
    main()
