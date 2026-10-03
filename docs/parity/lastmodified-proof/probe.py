#!/usr/bin/env python3
"""Guide lastModified save proof: drives the production `modsync convert`.

Every case below is written as a TOML or YAML guide, converted by the Rust
binary to TOML and YAML under the case's TZ, and its lastModified output
compared with the reference result recorded in csharp-oracle.json.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll [--record]]

With --csharp, the reference probe (Program.cs/probe.csproj in this directory,
built against ModSync.Core) is run live for each case instead of using the
recorded results; --record also rewrites csharp-oracle.json from that run.
See ../lastmodified-contract.md.
"""
import argparse
import datetime
import json
import os
import subprocess
import sys
import tempfile
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
GUID = "11111111-1111-1111-1111-111111111111"
ZONES = ["UTC", "America/New_York", "Asia/Kolkata"]
NEW_YORK = ["America/New_York"]


def lm(literal):
    return [["lastModified", literal]]


# (zones, source format, case name, metadata fields [[key, literal], ...])
CASES = []
for name, literal in [
    ("iso_z", '"2024-03-05T10:20:30Z"'),
    ("iso_offset", '"2024-03-05T10:20:30+02:00"'),
    ("iso_local", '"2024-03-05T10:20:30"'),
    ("iso_frac7", '"2024-03-05T10:20:30.1234567"'),
    ("iso_frac_z", '"2024-03-05T10:20:30.5Z"'),
    ("date_only", '"2024-03-05"'),
    ("space_clock", '"2024-03-05 10:20:30"'),
    ("month_name", '"March 5, 2024"'),
    ("rfc1123", '"Tue, 05 Mar 2024 10:20:30 GMT"'),
    ("invalid", '"not a date"'),
    ("empty", '""'),
    ("time_only", '"10:20"'),
]:
    CASES += [(ZONES, "toml", name, lm(literal)), (ZONES, "yaml", name, lm(literal))]
for name, literal in [
    ("toml_native_odt", "2024-03-05T10:20:30Z"),
    ("toml_native_ldt", "2024-03-05T10:20:30"),
    ("toml_native_ld", "2024-03-05"),
    ("toml_native_frac", "2024-03-05T10:20:30.1234567+05:30"),
]:
    CASES.append((ZONES, "toml", name, lm(literal)))
for text in [
    "March", "Tuesday", "GMT", "Z", "AM", "PM", "now", "today", "-", ".", "   ",
    "T", "Mar Tue", "2024", "2024-02-30", "2024-03-05T10:20:30+15:00",
    "2024-03-05T10:20:30.0001", "2024-03-05T10:20:30.9999999",
    "2024-03-05T10:20:30.0009999Z", "  2024-03-05T10:20:30  ", "2024-03-05T24:00:00",
    "Wed, 05 Mar 2024 10:20:30 GMT", "0001-01-01T00:00:00Z", "9999-12-31T23:59:59Z",
    "2024-03-05T10:20:30-14:00", "2024-03-05 10:20:30 +0530", "3/5/2024 10:20 PM",
    "2024/03/05",
]:
    CASES.append((NEW_YORK, "yaml", text, lm(json.dumps(text))))
for literal in ["true", "20240305", "10:20:30", "2024-03-05T10:20:30.000100"]:
    CASES.append((NEW_YORK, "toml", "toml_native_" + literal, lm(literal)))
# Key choice: TOML reads only `lastModified`; YAML prefers `LastModified` with
# no fallback to `lastModified` when it fails to parse.
CASES += [
    (ZONES, "toml", "pascal_only", [["LastModified", '"2024-03-05T10:20:30Z"']]),
    (NEW_YORK, "toml", "pascal_only_native", [["LastModified", "2024-03-05T10:20:30"]]),
    (ZONES, "toml", "pascal_and_camel", [["LastModified", '"2024-03-05"'], ["lastModified", '"2025-01-02T03:04:05Z"']]),
    (ZONES, "yaml", "pascal_only", [["LastModified", '"2024-03-05T10:20:30Z"']]),
    (ZONES, "yaml", "pascal_and_camel", [["lastModified", '"2025-01-02"'], ["LastModified", '"2024-03-05T10:20:30Z"']]),
    (NEW_YORK, "yaml", "pascal_invalid_no_fallback", [["LastModified", '"not a date"'], ["lastModified", '"2025-01-02"']]),
    (NEW_YORK, "yaml", "pascal_null_no_fallback", [["LastModified", "null"], ["lastModified", '"2025-01-02"']]),
]
# More than seven fraction digits: a native TOML value fails the whole
# reference TOML load (Tomlyn); the YAML string goes through TryParse.
CASES += [
    (NEW_YORK, "toml", "toml_native_frac8", lm("2024-03-05T10:20:30.12345678")),
    (NEW_YORK, "yaml", "frac8", lm('"2024-03-05T10:20:30.12345678"')),
]

# The bounded Rust date model does not reproduce these reference parses or
# failures; Rust keeps the authored value and logs a warning (contract
# "Unsupported forms").
RETAINED = {
    "time_only",  # reference infers the current date
    "toml_native_10:20:30",  # TOML local time: reference infers the current date
    "2024-02-30",  # reference rejects an impossible calendar date
    "2024-03-05T24:00:00",  # reference fallback rejects 24:00
}


def toml_doc(fields):
    lines = "".join(f"{key} = {literal}\n" for key, literal in fields)
    return (
        f'[metadata]\nfileFormatVersion = "2.0"\n{lines}\n'
        f'[[thisMod]]\nGuid = "{GUID}"\nName = "A"\n'
    )


def yaml_doc(fields):
    lines = "".join(f"{key}: {literal}\n" for key, literal in fields)
    return (
        f'---\n# Metadata\nfileFormatVersion: "2.0"\n{lines}'
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


def has_pascal_key(text, fmt):
    if fmt == "toml":
        return "LastModified" in tomllib.loads(text).get("metadata", {})
    return any(line.startswith("LastModified:") for line in text.splitlines())


def authored_value(fmt, fields, out_fmt):
    """The authored lastModified value as it reads back from `out_fmt`."""
    literal = dict(fields)["lastModified"]
    if fmt == "toml":
        value = tomllib.loads(toml_doc(lm(literal)))["metadata"]["lastModified"]
    else:
        value = json.loads(literal)
    if out_fmt == "yaml" and isinstance(value, (datetime.date, datetime.time)):
        return literal  # a native TOML date-time is written as its text
    return value


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
        ["dotnet", dll, fmt, source, "toml", "yaml", "json", "xml", "md"],
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

    return {
        "value": data["value"],
        "kind": data["kind"],
        "toml": line(data["toml"]),
        "yaml": line(data["yaml"]),
        "others_have_lastModified": [
            f for f in ("json", "xml", "md") if "lastmodified" in data[f].lower()
        ],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    if args.record and not args.csharp:
        parser.error("--record needs --csharp")
    with open(ORACLE, encoding="utf-8") as handle:
        oracle = json.load(handle)
    recorded = {(e["tz"], e["format"], e["case"]): e["csharp"] for e in oracle["entries"]}
    entries = []
    counts = {"match": 0, "retained": 0, "reference_error": 0, "fail": 0}
    failures = []
    with tempfile.TemporaryDirectory() as work:
        for zones, fmt, case, fields in CASES:
            for tz in zones:
                doc = toml_doc(fields) if fmt == "toml" else yaml_doc(fields)
                if args.csharp:
                    reference = run_csharp(args.csharp, fmt, doc, tz, work)
                else:
                    reference = recorded.get((tz, fmt, case))
                    if reference is None:
                        counts["fail"] += 1
                        failures.append((f"{tz}|{fmt}|{case}", "oracle entry", "missing"))
                        continue
                entries.append(
                    {"tz": tz, "format": fmt, "case": case, "fields": fields, "csharp": reference}
                )
                for out_fmt in ("toml", "yaml"):
                    stdout, stderr = run_rust(args.bin, fmt, doc, tz, out_fmt, work)
                    actual = last_modified_value(stdout, out_fmt)
                    label = f"{tz}|{fmt}->{out_fmt}|{case}"
                    if has_pascal_key(stdout, out_fmt):
                        counts["fail"] += 1
                        failures.append((label, "no LastModified key", "LastModified present"))
                        continue
                    if "error" in reference:
                        # The reference cannot load this guide; there is
                        # nothing to compare. Report it explicitly.
                        counts["reference_error"] += 1
                        print(
                            f"REFERENCE-ERROR {label}: {reference['error']}; "
                            f"Rust wrote {actual!r}"
                        )
                        continue
                    if case in RETAINED:
                        expected = authored_value(fmt, fields, out_fmt)
                        ok = actual == expected and "retains its authored value" in stderr
                        counts["retained" if ok else "fail"] += 1
                        if not ok:
                            failures.append((label, expected, actual))
                        continue
                    expected = csharp_value(reference[out_fmt], out_fmt)
                    if actual == expected:
                        counts["match"] += 1
                    else:
                        counts["fail"] += 1
                        failures.append((label, expected, actual))
        # Formats without metadata never carry the field in either implementation.
        doc = toml_doc(lm('"2024-03-05T10:20:30Z"'))
        for out_fmt in ("json", "xml", "md"):
            stdout, _ = run_rust(args.bin, "toml", doc, "UTC", out_fmt, work)
            if "lastmodified" in stdout.lower():
                counts["fail"] += 1
                failures.append((f"UTC|toml->{out_fmt}", None, "lastModified present"))
            else:
                counts["match"] += 1
    if args.record:
        oracle["entries"] = entries
        oracle["reference"] = (
            "ModOrchestration d31ccbd3 ModSync.Core (Tomlyn 0.17.0, YamlDotNet 16.2.0) on "
            ".NET runtime 10.0.12 via DOTNET_ROLL_FORWARD=Major; captured "
            + datetime.date.today().isoformat()
        )
        with open(ORACLE, "w", encoding="utf-8") as handle:
            json.dump(oracle, handle, indent=1)
            handle.write("\n")
    for label, expected, actual in failures:
        print(f"FAIL {label}: expected {expected!r}, got {actual!r}")
    print(json.dumps(counts))
    return 1 if counts["fail"] else 0


if __name__ == "__main__":
    sys.exit(main())
