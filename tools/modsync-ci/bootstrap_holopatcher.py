#!/usr/bin/env python3
"""Materialize pinned public HoloPatcher and PyKotor source for a real CI backend."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

import live_output


SOURCES = (
    ("https://github.com/vrifftech/HoloPatcher.git",
     "a0213be373e741d1b434f24f291ec87027ecbb5d", "holo"),
    ("https://github.com/vrifftech/PyKotor.git",
     "d226c0198872ec65a5b376bdda3fb8fa4331d626", "pykotor"),
)


def environment():
    result = {name: os.environ[name] for name in ("PATH", "LANG", "TMPDIR")
              if name in os.environ}
    result.update(GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1",
                  GIT_CONFIG_COUNT="0", GIT_TERMINAL_PROMPT="0")
    return result


def checkout(url, revision, destination, log):
    destination.mkdir()
    commands = (
        ["git", "init", "--quiet", str(destination)],
        ["git", "-C", str(destination), "fetch", "--depth=1", "--no-tags", url, revision],
        ["git", "-C", str(destination), "checkout", "--quiet", "--detach", "FETCH_HEAD"],
    )
    for index, command in enumerate(commands):
        result = live_output.run(command, log / f"{destination.name}-{index}.log",
                                 env=environment(), timeout=600)
        if result.returncode:
            raise ValueError("Pinned public patcher source acquisition failed")
    actual = subprocess.check_output(
        ["git", "-C", str(destination), "rev-parse", "HEAD"], env=environment(), text=True)
    if actual.strip() != revision:
        raise ValueError("Pinned patcher source commit differs")


def copy_sources(source, destination):
    # Git symlinks and special files cannot introduce dependencies outside the pin.
    for path in source.rglob("*"):
        if path.is_symlink() or not (path.is_dir() or path.is_file()):
            raise ValueError("Patcher source contains a link or special file")
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))


def install(destination, logs):
    destination = destination.absolute()
    if destination.exists() or destination.is_symlink():
        raise ValueError("Patcher runtime destination must be new")
    logs.mkdir(parents=True, exist_ok=True)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".holopatcher-", dir=destination.parent) as temporary:
        temporary = Path(temporary)
        for url, revision, name in SOURCES:
            checkout(url, revision, temporary / name, logs)
        resources = temporary / "resources"
        distribution = resources / "PyKotor"
        copy_sources(temporary / "holo/src", distribution / "Tools/HoloPatcher/src")
        for library in ("PyKotor", "Utility"):
            copy_sources(temporary / f"pykotor/Libraries/{library}/src",
                         distribution / f"Libraries/{library}/src")
        for name in ("holo", "pykotor"):
            shutil.copyfile(temporary / name / "LICENSE", resources / f"LICENSE-{name}")
        (resources / "source-provenance.json").write_text(json.dumps(
            {"schema_version": 1,
             "sources": [{"repository": url, "commit": revision} for url, revision, _ in SOURCES]},
            indent=2) + "\n")
        resources.rename(destination)
    print(f"Pinned HoloPatcher Python resources ready at {destination}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--logs", type=Path, required=True)
    args = parser.parse_args()
    install(args.directory, args.logs)


if __name__ == "__main__":
    main()
