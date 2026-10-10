#!/usr/bin/env -S python3 -I
"""Rebuild per-step install state from a seed manifest plus per-step manifests,
and diff two states. Used to prove automated installs match the manual oracle
after every mod.

  oracle_state.py verify <root> <lane>            rebuild; check against every step_NNN_full anchor
  oracle_state.py diff   <root> <lane> <N> <other.sha256>   compare lane state after step N to a manifest
  oracle_state.py dump   <root> <lane> <N>        print state after step N

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


def main(argv):
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
