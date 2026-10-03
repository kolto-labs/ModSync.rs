#!/usr/bin/env python3
"""Guide lastModified save proof: drives the production `modsync convert`.

Every case in csharp-oracle.json (recorded from the reference build) is
written as a TOML or YAML guide, converted by the Rust binary to TOML and YAML
under the recorded TZ, and its lastModified output compared with the reference.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll]

With --csharp, the reference probe (Program.cs/probe.csproj in this directory,
built against ModSync.Core) is run live for each case instead of using the
recorded results. See ../lastmodified-contract.md.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
GUID = "11111111-1111-1111-1111-111111111111"

# The bounded Rust date model does not reproduce these reference parses or
# failures; Rust keeps the authored value and logs a warning (contract
# "Unsupported forms").
RETAINED = {
    "time_only",  # reference infers the current date
    "toml_native_10:20:30",  # TOML local time: reference infers the current date
    "2024-02-30",  # reference rejects an impossible calendar date
    "2024-03-05T24:00:00",  # reference fallback rejects 24:00
}


def toml_doc(literal):
    return (
        f'[metadata]\nfileFormatVersion = "2.0"\nlastModified = {literal}\n\n'
        f'[[thisMod]]\nGuid = "{GUID}"\nName = "A"\n'
    )


def yaml_doc(literal):
    return (
        f'---\n# Metadata\nfileFormatVersion: "2.0"\nlastModified: {literal}\n'
        f"---\nGuid: {GUID}\nName: A\n"
    )


def last_modified_value(text, fmt):
    """Return the parsed lastModified value, or None when absent."""
    if fmt == "toml":
        return tomllib.loads(text).get("metadata", {}).get("lastModified")
    for line in text.splitlines():
        if line.startswith("lastModified:"):
            value = line.split(":", 1)[1].strip()
            if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
                value = value[1:-1]
            return value
    return None


def csharp_value(line, fmt):
    """Parse the reference's recorded lastModified output line."""
    if line is None:
        return None
    if fmt == "toml":
        return tomllib.loads("[metadata]\n" + line)["metadata"]["lastModified"]
    return last_modified_value(line, fmt)


def run_rust(binary, fmt, doc, tz, out_fmt, work):
    source = os.path.join(work, "in." + fmt)
    with open(source, "w", encoding="utf-8") as handle:
        handle.write(doc)
    env = dict(os.environ, TZ=tz)
    done = subprocess.run(
        [binary, "convert", source, "-f", out_fmt, "-v"],
        capture_output=True,
        text=True,
        env=env,
        timeout=120,
    )
    if done.returncode != 0:
        raise RuntimeError(f"convert failed: {done.stderr[-2000:]}")
    return done.stdout, done.stderr


def run_csharp(dll, fmt, doc, tz, work):
    source = os.path.join(work, "cs." + fmt)
    with open(source, "w", encoding="utf-8") as handle:
        handle.write(doc)
    env = dict(os.environ, TZ=tz, DOTNET_ROLL_FORWARD="Major")
    out = subprocess.run(
        ["dotnet", dll, fmt, source, "toml", "yaml"],
        capture_output=True,
        text=True,
        env=env,
        timeout=120,
    ).stdout
    data = json.loads(next(l for l in out.splitlines() if l.startswith("@@JSON@@"))[8:])
    if "error" in data:
        return {"error": data["error"]}

    def line(text):
        return next(
            (l for l in text.splitlines() if l.lower().startswith("lastmodified")), None
        )

    return {"kind": data["kind"], "toml": line(data["toml"]), "yaml": line(data["yaml"])}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    args = parser.parse_args()
    with open(os.path.join(HERE, "csharp-oracle.json"), encoding="utf-8") as handle:
        oracle = json.load(handle)["entries"]
    counts = {"match": 0, "retained": 0, "fail": 0, "skipped": 0}
    failures = []
    with tempfile.TemporaryDirectory() as work:
        for entry in oracle:
            fmt, tz, case, literal = (entry[k] for k in ("format", "tz", "case", "literal"))
            doc = toml_doc(literal) if fmt == "toml" else yaml_doc(literal)
            reference = run_csharp(args.csharp, fmt, doc, tz, work) if args.csharp else entry["csharp"]
            if "error" in reference:
                counts["skipped"] += 1
                continue
            for out_fmt in ("toml", "yaml"):
                stdout, stderr = run_rust(args.bin, fmt, doc, tz, out_fmt, work)
                actual = last_modified_value(stdout, out_fmt)
                expected = csharp_value(reference[out_fmt], out_fmt)
                label = f"{tz}|{fmt}->{out_fmt}|{case}"
                if case in RETAINED:
                    ok = actual is not None and "retains its authored value" in stderr
                    counts["retained" if ok else "fail"] += 1
                    if not ok:
                        failures.append((label, "authored value + warning", actual))
                    continue
                if actual == expected:
                    counts["match"] += 1
                else:
                    counts["fail"] += 1
                    failures.append((label, expected, actual))
        # Formats without metadata never carry the field in either implementation.
        doc = toml_doc('"2024-03-05T10:20:30Z"')
        for out_fmt in ("json", "xml", "md"):
            stdout, _ = run_rust(args.bin, "toml", doc, "UTC", out_fmt, work)
            if "lastmodified" in stdout.lower():
                counts["fail"] += 1
                failures.append((f"UTC|toml->{out_fmt}", None, "lastModified present"))
            else:
                counts["match"] += 1
    for label, expected, actual in failures:
        print(f"FAIL {label}: expected {expected!r}, got {actual!r}")
    print(json.dumps(counts))
    return 1 if counts["fail"] else 0


if __name__ == "__main__":
    sys.exit(main())
