#!/usr/bin/env python3
"""Explicit production resource compilation; no Cargo hook and no binary execution."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", required=True)
    parser.add_argument("--kind", choices=["windres", "msvc"], required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--icon", type=Path, required=True)
    parser.add_argument("--version", required=True, help="Four decimal version components")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    matrix = json.loads((Path(__file__).parent / "targets.json").read_text())["targets"]
    if args.target not in matrix or matrix[args.target]["os"] != "windows":
        raise ValueError("A retained Windows target is required")
    components = args.version.split(".")
    if len(components) != 4 or not all(value.isascii() and value.isdecimal() and int(value) <= 65535 for value in components):
        raise ValueError("Version requires four decimal components in 0..65535")
    if args.icon.is_symlink() or not args.icon.is_file() or args.icon.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("Icon must be a regular .ico no larger than 4 MiB")
    icon = args.icon.read_bytes()
    if len(icon) < 6 or icon[:4] != b"\0\0\1\0":
        raise ValueError("Invalid ICO header")
    template = (Path(__file__).parent / "windows_resources.rc.in").read_bytes()
    receipt = args.output.with_suffix(args.output.suffix + ".json")
    if args.output.exists() or receipt.exists():
        raise ValueError("Resource object or receipt already exists")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".modsync-resources-", dir=args.output.parent) as directory:
        directory = Path(directory)
        (directory / "icon.ico").write_bytes(icon)
        rendered = template.decode().replace("@ICON@", "icon.ico").replace("@VERSION_NUMERIC@", ",".join(components)).replace("@VERSION_TEXT@", args.version)
        (directory / "resources.rc").write_text(rendered, encoding="utf-8")
        if args.kind == "windres":
            coff = {"x86": "pe-i386", "x86_64": "pe-x86-64", "aarch64": "pe-aarch64-little"}[matrix[args.target]["architecture"]]
            command = [args.compiler, "--input-format=rc", "--output-format=coff", "--target=" + coff, "resources.rc", "resources.obj"]
        else:
            command = [args.compiler, "/nologo", "/fo", "resources.obj", "resources.rc"]
        subprocess.run(command, cwd=directory, check=True)
        compiled = directory / "resources.obj"
        if not compiled.is_file() or compiled.stat().st_size == 0:
            raise ValueError("Resource compiler emitted no object")
        payload = compiled.read_bytes()
        record = {"schemaVersion": 1, "target": args.target, "version": args.version, "objectSha256": hashlib.sha256(payload).hexdigest(), "iconSha256": hashlib.sha256(icon).hexdigest(), "templateSha256": hashlib.sha256(template).hexdigest(), "renderedRcSha256": hashlib.sha256(rendered.encode()).hexdigest(), "compiler": Path(args.compiler).name, "compilerKind": args.kind}
        os.link(compiled, args.output)
        with receipt.open("x", encoding="utf-8") as output:
            json.dump(record, output, indent=2, sort_keys=True)
            output.write("\n")
    print(str(args.output))


if __name__ == "__main__":
    main()
