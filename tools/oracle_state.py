#!/usr/bin/env -S python3 -I
"""Rebuild checkpoint manifests and compare file content, not installation proof.

See tools/ORACLE_STATE.md for formats, compatibility, and proof boundaries.
"""
import argparse
import hashlib
import os
from pathlib import Path
import re
import stat
import sys


SEEDS = {"k1_manual": "k1_seed.sha256", "tsl_manual": "tsl_seed_src.sha256"}


def directory(path):
    path = Path(path).absolute()
    for parent in (*reversed(path.parents), path):
        if not stat.S_ISDIR(parent.lstat().st_mode):
            raise ValueError(f"not a real directory (symlinks forbidden): {parent}")
    return path


def signature(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def regular_bytes(path, consume):
    """Read a fresh regular file and reject observable replacement or mutation."""
    path = directory(Path(path).parent) / Path(path).name
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError(f"not a regular file (symlinks forbidden): {path}")
    if not hasattr(os, "O_NOFOLLOW") or not hasattr(os, "O_NONBLOCK"):
        raise ValueError("fresh scanning requires POSIX O_NOFOLLOW and O_NONBLOCK")
    flags = os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW
    with os.fdopen(os.open(path, flags), "rb") as stream:
        if signature(before) != signature(os.fstat(stream.fileno())):
            raise ValueError(f"file changed before reading: {path}")
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            consume(chunk)
        if signature(before) != signature(os.fstat(stream.fileno())):
            raise ValueError(f"file changed while reading: {path}")
    if signature(before) != signature(path.lstat()):
        raise ValueError(f"file changed after reading: {path}")
    return signature(before)


def lines(path):
    content = bytearray()
    regular_bytes(path, content.extend)
    return [line.removesuffix("\r") for line in content.decode("utf-8").split("\n")]


def relative_path(name):
    if (not name.startswith("./") or "\\" in name
            or any(ord(char) < 32 or ord(char) == 127 for char in name)
            or any(part in ("", ".", "..") for part in name[2:].split("/"))):
        raise ValueError(f"expected normalized ./relative/path, got {name!r}")
    return name


def read(path):
    state = {}
    for number, line in enumerate(lines(path), 1):
        if not line:
            continue
        digest, separator, name = line.partition("  ")
        if not separator or not re.fullmatch(r"[0-9a-fA-F]{64}", digest):
            raise ValueError(f"{path}:{number}: malformed SHA-256 record")
        relative_path(name)
        if name in state:
            raise ValueError(f"{path}:{number}: duplicate path {name!r}")
        state[name] = digest.lower()
    return state


def path_list(path):
    names = set()
    for number, name in enumerate(lines(path), 1):
        if not name:
            continue
        relative_path(name)
        if name in names:
            raise ValueError(f"{path}:{number}: duplicate path {name!r}")
        names.add(name)
    return names


def steps(root, lane):
    if lane not in SEEDS:
        raise ValueError(f"invalid lane: {lane!r}")
    manifests = directory(directory(root) / "manifests" / lane)
    found, companions = set(), set()
    for path in manifests.iterdir():
        match = re.fullmatch(r"step_([0-9]{3})(\.sha256|_full\.sha256|\.added|\.deleted)", path.name)
        if match:
            if not stat.S_ISREG(path.lstat().st_mode):
                raise ValueError(f"not a regular manifest: {path}")
            number = int(match[1])
            (found if match[2] == ".sha256" else companions).add(number)
    missing = sorted(companions - found)
    if missing:
        raise ValueError(f"checkpoint companions without step manifests: {missing}")
    return sorted(found)


def rebuild(root, lane, upto):
    available = steps(root, lane)
    if not 0 <= upto <= 999 or upto not in available:
        raise ValueError(f"missing requested checkpoint: step_{upto:03d}")
    first = 0 if 0 in available else 1
    missing = sorted(set(range(first, upto + 1)) - set(available))
    if missing:
        raise ValueError(f"checkpoint gaps through step_{upto:03d}: {missing}")
    manifests = Path(root, "manifests")
    state = read(manifests / SEEDS[lane])
    anchors = {}
    for number in range(first, upto + 1):
        base = manifests / lane / f"step_{number:03d}"
        deleted = base.with_suffix(".deleted")
        if os.path.lexists(deleted):
            for name in path_list(deleted):
                state.pop(name, None)
        delta = read(base.with_suffix(".sha256"))
        added = base.with_suffix(".added")
        if os.path.lexists(added):
            if not path_list(added).issubset(delta):
                raise ValueError(f"added paths missing from delta: {added}")
        state.update(delta)
        full = manifests / lane / f"step_{number:03d}_full.sha256"
        if os.path.lexists(full):
            anchors[number] = (dict(state), read(full))
    return state, anchors


def scan(directory_path):
    """Hash every regular file; never read or write a scan cache."""
    root = directory(directory_path)
    state, observed = {}, {}

    def walk_error(error):
        raise error

    for base, directories, files in os.walk(root, followlinks=False, onerror=walk_error):
        base = Path(base)
        info = base.lstat()
        if not stat.S_ISDIR(info.st_mode):
            raise ValueError(f"directory changed or is a symlink: {base}")
        observed[base] = signature(info)
        for name in directories:
            child = base / name
            if not stat.S_ISDIR(child.lstat().st_mode):
                raise ValueError(f"not a real directory (symlinks forbidden): {child}")
        for name in files:
            full = base / name
            relative = relative_path("./" + full.relative_to(root).as_posix())
            digest = hashlib.sha256()
            observed[full] = regular_bytes(full, digest.update)
            state[relative] = digest.hexdigest()
    for path, before in observed.items():
        if signature(path.lstat()) != before:
            raise ValueError(f"tree changed during scan: {path}")
    return state


def keep(state, exclusions, label):
    filtered = {name: digest for name, digest in state.items()
                if not any(name == excluded or name.startswith(excluded + "/")
                           for excluded in exclusions)}
    if exclusions:
        print(f"{label}: excluded {len(state) - len(filtered)} files", file=sys.stderr)
    return filtered


def report(label_a, state_a, label_b, state_b, tag, exclusions):
    state_a = keep(state_a, exclusions, label_a)
    state_b = keep(state_b, exclusions, label_b)
    only_a = sorted(set(state_a) - set(state_b))
    only_b = sorted(set(state_b) - set(state_a))
    changed = sorted(name for name in set(state_a) & set(state_b)
                     if state_a[name] != state_b[name])
    for name in only_a:
        print(f"only in {label_a}: {name}")
    for name in only_b:
        print(f"only in {label_b}: {name}")
    for name in changed:
        print(f"differs: {name}")
    print(f"{tag}: {len(only_a)} only-{label_a}, {len(only_b)} only-{label_b}, {len(changed)} differing")
    return 1 if (only_a or only_b or changed) else 0


def exclusion(value):
    name = value.removesuffix("/")
    if not name.startswith("./"):
        name = "./" + name
    try:
        return relative_path(name)
    except ValueError as error:
        raise argparse.ArgumentTypeError(str(error)) from error


def checkpoint(value):
    try:
        number = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("checkpoint must be an integer from 0 to 999") from error
    if not 0 <= number <= 999:
        raise argparse.ArgumentTypeError("checkpoint must be from 0 to 999")
    return number


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("verify", "diff", "dump", "tree", "trees"):
        subparser = commands.add_parser(command)
        subparser.add_argument("--exclude", action="append", default=[], type=exclusion,
                               metavar="RELATIVE_PATH", help="omit this exact file/subtree (repeatable; no globs)")
        if command == "trees":
            subparser.add_argument("dir_a")
            subparser.add_argument("dir_b")
        else:
            subparser.add_argument("root")
            subparser.add_argument("lane", choices=SEEDS)
            if command != "verify":
                subparser.add_argument("checkpoint", type=checkpoint)
            if command == "diff":
                subparser.add_argument("manifest")
            if command == "tree":
                subparser.add_argument("directory")
    args = parser.parse_args(argv[1:])
    if args.exclude:
        print("FILTERED scope; exclusions: " + ", ".join(args.exclude), file=sys.stderr)
    else:
        print("EXACT scope; no exclusions", file=sys.stderr)
    try:
        if args.command == "trees":
            return report("a", scan(args.dir_a), "b", scan(args.dir_b), "trees", args.exclude)
        if args.command == "verify":
            available = steps(args.root, args.lane)
            if not available:
                raise ValueError("no checkpoint manifests; nothing to verify")
            _, anchors = rebuild(args.root, args.lane, available[-1])
            if not anchors:
                raise ValueError("no full anchors; reconstruction is NOT verified")
            bad = 0
            for number, (mine, anchor) in sorted(anchors.items()):
                bad += report("rebuilt", mine, "anchor", anchor, f"step {number}", args.exclude)
            print("FAILED" if bad else f"{len(anchors)} anchors reproduce (not installation proof)")
            return 1 if bad else 0
        state, _ = rebuild(args.root, args.lane, args.checkpoint)
        if args.command == "dump":
            for name, digest in sorted(keep(state, args.exclude, args.lane).items()):
                print(f"{digest}  {name}")
            return 0
        other = read(args.manifest) if args.command == "diff" else scan(args.directory)
        return report(args.lane, state, "other", other, f"step {args.checkpoint}", args.exclude)
    except (OSError, ValueError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    sys.exit(main(sys.argv))
