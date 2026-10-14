#!/usr/bin/env -S python3 -I
"""Rebuild per-step install state from a seed manifest plus per-step manifests,
and diff two states. Used to prove automated installs match the manual oracle
after every mod.

  oracle_state.py verify <root> <lane>            rebuild; check against every step_NNN_full anchor
  oracle_state.py diff   <root> <lane> <N> <other.sha256>   compare lane state after step N to a manifest
  oracle_state.py dump   <root> <lane> <N>        print state after step N
  oracle_state.py tree   <root> <lane> <N> <dir>  scan <dir> (cached) and diff it against the lane's state after step N
  oracle_state.py trees  <dir_a> <dir_b>          scan both (cached) and diff them, e.g. odypatcher vs providedpatcher

<root> holds manifests/<lane>/step_NNN.{sha256,added,deleted} and the seed manifest.
Manifest lines are `<sha256>  ./relative/path`. Exclusions below are patcher
bookkeeping, not mod content, and are dropped from every comparison.
"""
import re
import sys
from pathlib import Path

SEEDS = {"k1_manual": "k1_seed.sha256", "tsl_manual": "tsl_seed_src.sha256"}
EXCLUDE = re.compile(r"(^|/)(backup|tslpatchdata)(/|$)|(^|/)installlog\.(rtf|txt)$", re.I)


def read(path):
    state = {}
    for line in Path(path).read_text(errors="replace").splitlines():
        if not line.strip():
            continue
        digest, _, name = line.partition("  ")
        state[name] = digest
    return state


def keep(state):
    return {k: v for k, v in state.items() if not EXCLUDE.search(k)}


def steps(root, lane):
    found = sorted(Path(root, "manifests", lane).glob("step_[0-9][0-9][0-9].sha256"))
    return [int(p.stem.split("_")[1]) for p in found]


def rebuild(root, lane, upto):
    manifests = Path(root, "manifests")
    state = read(manifests / SEEDS[lane])
    anchors = {}
    for n in steps(root, lane):
        if n > upto:
            break
        base = manifests / lane / f"step_{n:03d}"
        deleted = base.with_suffix(".deleted")
        if deleted.exists():
            for name in deleted.read_text().splitlines():
                state.pop(name, None)
        state.update(read(base.with_suffix(".sha256")))
        full = manifests / lane / f"step_{n:03d}_full.sha256"
        if full.exists():
            anchors[n] = (dict(state), read(full))
    return state, anchors


def scan(directory):
    """sha256 every file under directory. Results are cached beside the tree
    (<dir>.scan-cache.json) keyed by size and mtime, so only files touched since
    the last checkpoint are rehashed."""
    import hashlib, json, os
    root = Path(directory)
    cache_file = root.parent / (root.name + ".scan-cache.json")
    cache = json.loads(cache_file.read_text()) if cache_file.exists() else {}
    state, fresh = {}, {}
    for base, _, files in os.walk(root):
        for name in files:
            full = Path(base, name)
            rel = "./" + str(full.relative_to(root))
            st = full.lstat()
            key = [st.st_size, st.st_mtime_ns]
            hit = cache.get(rel)
            if hit and hit[0] == key:
                digest = hit[1]
            else:
                h = hashlib.sha256()
                with open(full, "rb") as fh:
                    for chunk in iter(lambda: fh.read(1 << 20), b""):
                        h.update(chunk)
                digest = h.hexdigest()
            fresh[rel] = [key, digest]
            state[rel] = digest
    cache_file.write_text(json.dumps(fresh))
    return state


def report(label_a, a, label_b, b, tag):
    a, b = keep(a), keep(b)
    only_a = sorted(set(a) - set(b))
    only_b = sorted(set(b) - set(a))
    changed = sorted(k for k in set(a) & set(b) if a[k] != b[k])
    for k in only_a:
        print(f"only in {label_a}: {k}")
    for k in only_b:
        print(f"only in {label_b}: {k}")
    for k in changed:
        print(f"differs: {k}")
    print(f"{tag}: {len(only_a)} only-{label_a}, {len(only_b)} only-{label_b}, {len(changed)} differing")
    return 1 if (only_a or only_b or changed) else 0


def main(argv):
    if len(argv) >= 4 and argv[1] == "trees":
        return report("a", scan(argv[2]), "b", scan(argv[3]), "trees")
    if len(argv) >= 6 and argv[1] == "tree":
        state = rebuild(argv[2], argv[3], int(argv[4]))[0]
        return report("manual", state, "auto", scan(argv[5]), f"step {argv[4]}")
    if len(argv) < 4:
        print(__doc__)
        return 2
    cmd, root, lane = argv[1:4]
    if cmd == "verify":
        last = steps(root, lane)[-1]
        _, anchors = rebuild(root, lane, last)
        bad = 0
        for n, (mine, anchor) in sorted(anchors.items()):
            if mine != anchor:
                bad += 1
                miss = len(set(anchor) ^ set(mine))
                diff = sum(1 for k in set(anchor) & set(mine) if anchor[k] != mine[k])
                print(f"step {n}: MISMATCH ({miss} path differences, {diff} hash differences)")
            else:
                print(f"step {n}: ok ({len(mine)} files)")
        print("no anchors yet" if not anchors else ("FAILED" if bad else "all anchors reproduce"))
        return 1 if bad else 0
    n = int(argv[4])
    state = keep(rebuild(root, lane, n)[0])
    if cmd == "dump":
        for k in sorted(state):
            print(f"{state[k]}  {k}")
        return 0
    if cmd == "diff":
        other = keep(read(argv[5]))
        only_a = sorted(set(state) - set(other))
        only_b = sorted(set(other) - set(state))
        changed = sorted(k for k in set(state) & set(other) if state[k] != other[k])
        for k in only_a:
            print(f"only in {lane}: {k}")
        for k in only_b:
            print(f"only in other: {k}")
        for k in changed:
            print(f"differs: {k}")
        print(f"step {n}: {len(only_a)} only-oracle, {len(only_b)} only-other, {len(changed)} differing")
        return 1 if (only_a or only_b or changed) else 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
