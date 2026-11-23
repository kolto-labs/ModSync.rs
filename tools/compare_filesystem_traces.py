#!/usr/bin/env python3
"""Validate and compare complete version-1 ModSync filesystem traces.

By default compare every reached instruction and the final filesystem. Use
--final-only for prediction versus installation, then compare installation lanes
against the supplied-installer trace without that flag. No paths are excluded.
"""

import argparse
import json
import re
import sys
from pathlib import Path


class TraceError(ValueError):
    """Incomplete, malformed, or unequal trace evidence."""


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise TraceError(f"duplicate JSON key {key!r}")
        result[key] = value
    return result


def read_json(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    except (OSError, UnicodeError, ValueError) as error:
        raise TraceError(f"{path}: {error}") from error


def identity(manifest):
    return tuple(manifest.get(key) for key in ("owner_guid", "instruction", "action", "skipped"))


def validate_manifest(path, boundary=False):
    manifest = read_json(path)
    fields = {"version", "owner_guid", "instruction", "action", "skipped", "entries"}
    if not isinstance(manifest, dict) or set(manifest) != fields:
        raise TraceError(f"{path}: expected a version-1 manifest object with known fields")
    if type(manifest.get("version")) is not int or manifest["version"] != 1:
        raise TraceError(f"{path}: unsupported manifest version")
    if type(manifest.get("skipped")) is not bool:
        raise TraceError(f"{path}: missing or invalid skipped flag")
    if boundary:
        if identity(manifest) != (None, None, None, False):
            raise TraceError(f"{path}: baseline/final identity must be null with skipped=false")
    else:
        for key in ("owner_guid", "action"):
            if not isinstance(manifest.get(key), str) or not manifest[key].strip():
                raise TraceError(f"{path}: reached instruction requires {key}")
        if type(manifest.get("instruction")) is not int or manifest["instruction"] < 1:
            raise TraceError(f"{path}: invalid instruction index")
    entries = manifest.get("entries")
    if not isinstance(entries, dict) or not entries:
        raise TraceError(f"{path}: empty or missing filesystem entries")
    for key, entry in entries.items():
        parts = key.split("/")
        if parts[0] not in ("game", "mods") or any(part in ("", ".", "..") for part in parts) or "\\" in key:
            raise TraceError(f"{path}: invalid root-relative entry {key!r}")
        if not isinstance(entry, dict) or set(entry) != {"directory", "bytes", "sha256"}:
            raise TraceError(f"{path}: invalid entry shape for {key!r}")
        if type(entry["directory"]) is not bool:
            raise TraceError(f"{path}: invalid directory flag for {key!r}")
        if entry["directory"]:
            valid = entry["bytes"] is None and entry["sha256"] is None
        else:
            valid = type(entry["bytes"]) is int and entry["bytes"] >= 0
            valid = valid and isinstance(entry["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])
        if not valid:
            raise TraceError(f"{path}: invalid size/hash for {key!r}")
        if len(parts) > 1:
            parent = entries.get("/".join(parts[:-1]))
            if not isinstance(parent, dict) or parent.get("directory") is not True:
                raise TraceError(f"{path}: missing directory parent for {key!r}")
    for root in ("game", "mods"):
        if root not in entries or entries[root]["directory"] is not True:
            raise TraceError(f"{path}: missing {root} directory root")
    return manifest


def trace_files(directory):
    if not directory.is_dir() or directory.is_symlink():
        raise TraceError(f"{directory}: expected an ordinary trace directory")
    files = sorted(directory.iterdir())
    if any(not path.is_file() or path.is_symlink() for path in files):
        raise TraceError(f"{directory}: trace contains a non-regular file")
    steps = sorted((path for path in files if re.fullmatch(r"step_[0-9]{6,}\.json", path.name)), key=lambda path: int(path.stem[5:]))
    if len(steps) < 2:
        raise TraceError(f"{directory}: require a baseline and at least one reached instruction")
    expected = [f"step_{index:06d}.json" for index in range(len(steps))]
    if [path.name for path in steps] != expected:
        raise TraceError(f"{directory}: step sequence must start at zero with no gaps")
    if {path.name for path in files} != set(expected) | {"final.json"}:
        raise TraceError(f"{directory}: missing final.json or unexpected trace files")
    return [*steps, directory / "final.json"]


def load_trace(directory):
    # Keep instruction identities only. Full inventories may contain millions of
    # entries across a guide, so callers must consume one inventory at a time.
    manifests = {}
    for path in trace_files(directory):
        manifest = validate_manifest(path, path.name in ("step_000000.json", "final.json"))
        manifests[path.name] = {key: value for key, value in manifest.items() if key != "entries"}
    return manifests


def compare_entries(left, right, label, limit=20):
    differences = []
    count = 0
    for key in sorted(left.keys() | right.keys()):
        if key in left and key in right and left[key] == right[key]:
            continue
        count += 1
        if len(differences) >= limit:
            continue
        if key not in left:
            differences.append(f"{label}: extra in actual {key!r}")
        elif key not in right:
            differences.append(f"{label}: missing from actual {key!r}")
        else:
            differences.append(f"{label}: changed {key!r}: expected {left[key]!r}, actual {right[key]!r}")
    if count:
        if count > limit:
            differences.append(f"{label}: {count - limit} further differences")
        raise TraceError("\n".join(differences))


def compare_traces(expected, actual, final_only=False):
    left_paths = trace_files(expected)
    right_paths = trace_files(actual)
    if not final_only and len(left_paths) != len(right_paths):
        raise TraceError(f"step count differs: {expected} has {len(left_paths) - 2} instructions, {actual} has {len(right_paths) - 2}")
    if final_only:
        # Even a final-only comparison rejects incomplete or malformed steps.
        load_trace(expected)
        load_trace(actual)
        left_paths = [expected / "step_000000.json", expected / "final.json"]
        right_paths = [actual / "step_000000.json", actual / "final.json"]
    for left_path, right_path in zip(left_paths, right_paths):
        name = left_path.name
        boundary = name in ("step_000000.json", "final.json")
        left = validate_manifest(left_path, boundary)
        right = validate_manifest(right_path, boundary)
        if identity(left) != identity(right):
            raise TraceError(f"{name}: step identity differs: expected {identity(left)!r}, actual {identity(right)!r}")
        compare_entries(left["entries"], right["entries"], name)
    return len(trace_files(actual)) - 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("expected", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--final-only", action="store_true")
    args = parser.parse_args()
    try:
        count = compare_traces(args.expected, args.actual, args.final_only)
    except TraceError as error:
        print(f"Filesystem trace comparison failed: {error}", file=sys.stderr)
        return 1
    print(f"Filesystem traces match ({count} actual instructions, {'baseline and final' if args.final_only else 'every step and final'}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
