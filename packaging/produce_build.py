#!/usr/bin/env python3
"""Explicit offline production build and provenance receipt; never runs app binaries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True

from build_package import ROOT, binary, digest, git, regular, require, validate_windows_resource


def capture(command, environment):
    return subprocess.check_output(command, cwd=ROOT, env=environment, text=True).strip()


def source_state():
    return {"commit": git("rev-parse", "HEAD"),
            "status": git("status", "--porcelain", "--untracked-files=all"),
            "cargoLockSha256": digest(ROOT / "Cargo.lock")}


def write_new(path, value):
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output-dir", type=Path, required=True, help="New directory outside checkout; retained on failure")
    parser.add_argument("--toolchain", required=True, help="Installed exact Rust version, for example 1.95.0")
    parser.add_argument("--windows-resource-object", type=Path)
    parser.add_argument("--windows-resource-receipt", type=Path)
    args = parser.parse_args()
    require(args.toolchain.isascii() and len(args.toolchain.split(".")) == 3 and all(part.isdecimal() for part in args.toolchain.split(".")), "Use an exact installed Rust release")
    target = json.loads((ROOT / "packaging/targets.json").read_text())["targets"].get(args.target)
    require(target is not None, "Unknown retained target")
    initial = source_state()
    require(initial["commit"] == args.commit and not initial["status"], "Producing build requires the exact clean committed checkout")
    output = args.output_dir.resolve()
    require(not output.is_relative_to(ROOT), "Build output must be outside the source checkout")
    output.mkdir(parents=True, exist_ok=False)
    build = output / "buildtree"
    environment = os.environ.copy()
    for name in list(environment):
        if name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER") or name.startswith("CARGO_TARGET_") and name.endswith("_RUSTFLAGS"):
            environment.pop(name, None)
    environment["CARGO_TARGET_DIR"] = str(build)
    environment["CARGO_NET_OFFLINE"] = "true"
    rustup = shutil.which("rustup")
    require(rustup is not None, "An installed rustup toolchain is required; this helper never installs one")
    cargo = [rustup, "run", args.toolchain, "cargo"]
    rustc = [rustup, "run", args.toolchain, "rustc"]
    receipt = {"schemaVersion": 1, "sourceCommit": args.commit, "target": args.target,
               "cargoLockSha256": initial["cargoLockSha256"], "sourceStart": initial,
               "host": {"os": platform.system(), "architecture": platform.machine()},
               "nativeAcceptance": "unverified", "publication": False, "offline": True,
               "startedUnix": int(time.time()), "commands": [], "binaries": {}, "status": "building"}
    try:
        for name, command in (("rustc", rustc + ["--version", "--verbose"]), ("cargo", cargo + ["--version", "--verbose"])):
            receipt["commands"].append(command)
            receipt[name] = capture(command, environment)
        if target["os"] == "windows":
            require(args.windows_resource_object and args.windows_resource_receipt, "Windows builds require the explicit compiled resource object and receipt")
            require(regular(args.windows_resource_receipt).stat().st_size <= 1024 * 1024, "Resource receipt exceeds 1 MiB")
            resource_bytes = args.windows_resource_receipt.read_bytes()
            require(len(resource_bytes) <= 1024 * 1024, "Resource receipt exceeds 1 MiB")
            resource = json.loads(resource_bytes)
            validate_windows_resource(resource, args.target)
            require(digest(args.windows_resource_object) == resource["objectSha256"], "Resource object digest mismatch")
            copied = output / "resources.obj"
            shutil.copyfile(regular(args.windows_resource_object), copied)
            require(digest(copied) == resource["objectSha256"], "Resource changed while copying")
            (output / "resources.obj.json").write_bytes(resource_bytes)
            # JSON-valued Cargo config preserves spaces in the single linker arg
            # and applies only to this target, not host build-script executables.
            resource_config = "target." + args.target + ".rustflags=" + json.dumps(["-C", "link-arg=" + str(copied)])
            receipt["windowsResources"] = {"receiptSha256": hashlib.sha256(resource_bytes).hexdigest(), "objectSha256": resource["objectSha256"], "iconSha256": resource["iconSha256"]}
        else:
            require(not args.windows_resource_object and not args.windows_resource_receipt, "Windows resource inputs only apply to Windows")
            resource_config = None
        command = cargo + ["build", "--locked", "--offline", "--release", "--target", args.target, "--bin", "modsync", "--message-format=json-render-diagnostics", "-j2"]
        if target["os"] == "windows":
            command += ["--bin", "modsync-gui", "--config", resource_config]
        receipt["commands"].append(command)
        artifacts = set()
        with subprocess.Popen(command, cwd=ROOT, env=environment, stdout=subprocess.PIPE, text=True) as process:
            try:
                while True:
                    line = process.stdout.readline(16 * 1024 * 1024 + 1)
                    if not line:
                        break
                    require(len(line) <= 16 * 1024 * 1024, "Cargo message exceeds bound")
                    event = json.loads(line)
                    if event.get("reason") == "compiler-artifact":
                        artifacts.update(event.get("filenames", []))
                        require(len(artifacts) <= 100_000, "Build artifact inventory exceeds bound")
                    elif event.get("reason") == "compiler-message":
                        print(event.get("message", {}).get("rendered", ""), file=sys.stderr, end="")
                require(process.wait() == 0, "Production Cargo build failed")
            except BaseException:
                process.kill()
                process.wait()
                raise
        records = []
        for filename in sorted(artifacts):
            path = Path(filename).resolve()
            require(path.is_relative_to(build), "Compiler artifact is outside isolated target directory")
            records.append({"path": path.relative_to(build).as_posix(), "sha256": digest(path), "size": path.stat().st_size})
        write_new(output / "buildtree.json", {"schemaVersion": 1, "artifacts": records})
        receipt["buildtreeSha256"] = digest(output / "buildtree.json")
        names = ["modsync", "modsync-gui"] if target["os"] == "windows" else ["modsync"]
        for name in names:
            filename = name + (".exe" if target["os"] == "windows" else "")
            source = build / args.target / "release" / filename
            expected = digest(source)
            require(any(record["path"] == source.relative_to(build).as_posix() and record["sha256"] == expected for record in records), "Executable differs from compiler artifact inventory")
            binary(source, target, "gui" if name == "modsync-gui" else "console")
            destination = output / filename
            shutil.copyfile(source, destination)
            destination.chmod(0o755)
            require(digest(destination) == expected, "Built executable changed while copying")
            receipt["binaries"][name] = expected
        if resource_config is not None:
            require(digest(output / "resources.obj") == receipt["windowsResources"]["objectSha256"], "Linked resource changed during build")
        final = source_state()
        receipt["sourceEnd"] = final
        require(final == initial, "Source or lockfile changed during the producing build")
        receipt["status"] = "built"
        receipt["finishedUnix"] = int(time.time())
        write_new(output / "build-receipt.json", receipt)
    except BaseException as error:
        receipt["status"] = "failed"
        receipt["errorType"] = type(error).__name__
        receipt["finishedUnix"] = int(time.time())
        write_new(output / "build-failure.json", receipt)
        raise
    print(output / "build-receipt.json")


if __name__ == "__main__":
    main()
