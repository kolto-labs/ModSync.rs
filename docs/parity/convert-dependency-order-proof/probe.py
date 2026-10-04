#!/usr/bin/env python3
"""Convert dependency order proof: drives the production `modsync convert`.

C# `convert` loads the guide, then runs `HandleDependencyResolutionErrors`
(ModBuildConverter.cs:2046, :823-877) over `DependencyResolverService.ResolveDependencies`
(DependencyResolverService.cs:48-109). That orders every top-level component by
InstallBefore/InstallAfter with a depth-first post-order (:350-403) and writes the guide
in that order. Missing, self and circular references fail the conversion (exit 1, no
output) unless `--ignore-errors` is given. Markdown input keeps its file order and is
never refused: the C# Markdown ingest drops InstallBefore/InstallAfter.

Each case is a guide file (or stdin). The Rust binary and the pinned C# CLI
(`ModSync.Core.dll`, d31ccbd3) run `convert -i G -f FMT -o OUT [flags]` in a fresh
folder each.

Checks per case:
  decision  exit code is zero or not, and whether OUT was written;
  order     the top-level component names in OUT, in file order (null when not written);
  refs      per component name, its InstallBefore and InstallAfter GUIDs as written
            (lowercase without braces, null when not written), so ordering does not drop
            references;
  errors    the N of `Dependency resolution failed with N errors` when the run fails
            (null when it succeeds).

Known differences (counted separately): with --ignore-errors and a cycle, C# writes a
guide with no components (its sort throws and the CLI keeps the empty result); Rust
refuses (exit 1, nothing written). From Markdown input C# drops InstallBefore and
InstallAfter; Rust keeps them in the output (`refs`).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major; --record
also rewrites csharp-oracle.json. Without it the recorded results are used. See
../convert-dependency-order-contract.md.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
G = {k: f"{k * 8}-{k * 4}-{k * 4}-{k * 4}-{k * 12}" for k in "abcdef"}
MISSING = "f" * 8 + "-ffff-ffff-ffff-" + "f" * 12


def ref(name):
    return G.get(name, name)


def toml_guide(spec, option=None):
    """spec: (key, InstallBefore keys, InstallAfter keys, selected)."""
    out = []
    for key, before, after, selected in spec:
        lines = ["[[thisMod]]", f'Guid = "{G[key]}"', f'Name = "{key.upper()}"',
                 f"IsSelected = {'true' if selected else 'false'}"]
        if before:
            lines.append("InstallBefore = [" + ", ".join(f'"{ref(x)}"' for x in before) + "]")
        if after:
            lines.append("InstallAfter = [" + ", ".join(f'"{ref(x)}"' for x in after) + "]")
        lines += ["", "[[thisMod.Instructions]]", 'Action = "Copy"',
                  f'Source = ["<<modDirectory>>/{key}.txt"]', 'Destination = "<<kotorDirectory>>/Override"', ""]
        if option and option[0] == key:
            lines += ["[[thisMod.Options]]", f'Guid = "{G["e"]}"', 'Name = "Option"', "IsSelected = true",
                      f'InstallAfter = ["{MISSING}"]', ""]
        out.append("\n".join(lines))
    return "\n".join(out)


def json_guide(spec):
    return json.dumps([{
        "Guid": G[key], "Name": key.upper(), "IsSelected": bool(selected),
        "InstallBefore": [ref(x) for x in before], "InstallAfter": [ref(x) for x in after],
        "Instructions": [{"Action": "Copy", "Source": [f"<<modDirectory>>/{key}.txt"],
                          "Destination": "<<kotorDirectory>>/Override"}],
    } for key, before, after, selected in spec], indent=1)


def yaml_guide(spec):
    docs = []
    for key, before, after, selected in spec:
        lines = [f"Guid: {G[key]}", f"Name: {key.upper()}", f"IsSelected: {'true' if selected else 'false'}"]
        for field, keys in (("InstallBefore", before), ("InstallAfter", after)):
            if keys:
                lines.append(f"{field}:")
                lines += [f"- {ref(x)}" for x in keys]
        lines += ["Instructions:", "- Action: Copy", "  Source:", f"  - <<modDirectory>>/{key}.txt",
                  "  Destination: <<kotorDirectory>>/Override"]
        docs.append("---\n" + "\n".join(lines) + "\n")
    return "".join(docs)


def markdown_guide(spec):
    out = ["# Example build", "", "## Mod List", ""]
    for key, before, after, selected in spec:
        out += [f"### {key.upper()}", f"**Name:** [{key.upper()}](https://example.com/{key}.zip)",
                "**Author:** Author", "**Description:** Text.",
                "**Category & Tier:** Textures / Essential", "<!--<<ModSync>>", f"Guid: {G[key]}",
                f"IsSelected: {'true' if selected else 'false'}"]
        for field, keys in (("InstallBefore", before), ("InstallAfter", after)):
            if keys:
                out.append(f"{field}:")
                out += [f"- {ref(x)}" for x in keys]
        out += ["Instructions:", "- Action: Copy", f"  Source: '<<modDirectory>>/{key}.txt'",
                "  Destination: '<<kotorDirectory>>/Override'", "-->", "___", ""]
    return "\n".join(out)


AFTER = [("b", [], ["a"], 1), ("a", [], [], 1)]
TIE = [("b", [], ["a"], 1), ("c", [], [], 1), ("a", [], [], 1)]
CYCLE = [("b", [], ["a"], 1), ("a", [], ["b"], 1), ("c", [], [], 1)]
SELF = [("a", [], ["a"], 1), ("b", [], [], 1)]
MISS = [("b", [], ["f"], 1), ("a", [], [], 1)]

# name: (input text, input extension, output format, flags, stdin)
CASES = {
    "after": (toml_guide(AFTER), "toml", "toml", [], False),
    "before": (toml_guide([("a", [], [], 1), ("b", ["a"], [], 1)]), "toml", "toml", [], False),
    "tie": (toml_guide(TIE), "toml", "toml", [], False),
    "multi_after": (toml_guide([("c", [], ["b", "a"], 1), ("a", [], [], 1), ("b", [], [], 1)]), "toml", "toml", [], False),
    "before_and_after": (toml_guide([("d", [], ["c"], 1), ("a", ["d", "b"], [], 1), ("c", [], [], 1), ("b", [], [], 1)]),
                         "toml", "toml", [], False),
    "chain": (toml_guide([("c", [], ["b"], 1), ("d", [], [], 1), ("b", [], ["a"], 1), ("a", [], [], 1)]),
              "toml", "toml", [], False),
    "unselected_target": (toml_guide([("b", [], ["a"], 1), ("a", [], [], 0)]), "toml", "toml", [], False),
    "unselected_both": (toml_guide([("b", [], ["a"], 0), ("c", [], [], 1), ("a", [], [], 0)]), "toml", "toml", [], False),
    "braced_reference": (toml_guide([("b", [], ["{" + G["a"].upper() + "}"], 1), ("a", [], [], 1)]),
                         "toml", "toml", [], False),
    "json_input": (json_guide(TIE), "json", "toml", [], False),
    "yaml_input": (yaml_guide(TIE), "yaml", "toml", [], False),
    "json_output": (toml_guide(TIE), "toml", "json", [], False),
    "yaml_output": (toml_guide(TIE), "toml", "yaml", [], False),
    "markdown_output": (toml_guide(TIE), "toml", "markdown", [], False),
    "stdin": (toml_guide(TIE), "toml", "toml", [], True),
    "option_reference": (toml_guide([("c", [], [], 1), ("a", [], [], 1)], option=("c",)), "toml", "toml", [], False),
    "ignore_errors_valid": (toml_guide(TIE), "toml", "toml", ["--ignore-errors"], False),
    "control_no_order": (toml_guide([("c", [], [], 1), ("a", [], [], 1), ("b", [], [], 1)]), "toml", "toml", [], False),
    "control_ordered": (toml_guide([("a", [], [], 1), ("b", [], ["a"], 1), ("c", [], [], 1)]), "toml", "toml", [], False),
    "missing_after": (toml_guide(MISS), "toml", "toml", [], False),
    "missing_before": (toml_guide([("b", ["f"], [], 1), ("a", [], [], 1)]), "toml", "toml", [], False),
    "missing_ignored": (toml_guide([("b", [], ["f", "c"], 1), ("a", [], [], 1), ("c", [], [], 1)]),
                        "toml", "toml", ["--ignore-errors"], False),
    "missing_unselected": (toml_guide([("b", [], ["f"], 0), ("a", [], [], 1)]), "toml", "toml", [], False),
    "self_reference": (toml_guide(SELF), "toml", "toml", [], False),
    "cycle": (toml_guide(CYCLE), "toml", "toml", [], False),
    "cycle_three": (toml_guide([("a", [], ["c"], 1), ("b", [], ["a"], 1), ("c", [], ["b"], 1), ("d", [], [], 1)]),
                    "toml", "toml", [], False),
    "cycle_json": (json_guide(CYCLE), "json", "toml", [], False),
    "markdown_input": (markdown_guide([("c", [], [], 1), ("b", [], ["a"], 1), ("a", [], [], 1)]), "md", "toml", [], False),
    "markdown_input_ignored": (markdown_guide([("c", [], [], 1), ("b", [], ["a"], 1), ("a", [], [], 1)]), "md", "toml", ["--ignore-errors"], False),
    "markdown_missing": (markdown_guide(MISS), "md", "toml", [], False),
    "markdown_missing_ignored": (markdown_guide(MISS), "md", "toml", ["--ignore-errors"], False),
    "markdown_cycle": (markdown_guide(CYCLE), "md", "toml", [], False),
    "markdown_stdin": (markdown_guide(MISS), "md", "toml", [], True),
    "self_reference_ignored": (toml_guide(SELF), "toml", "toml", ["--ignore-errors"], False),
    "cycle_ignored": (toml_guide(CYCLE), "toml", "toml", ["--ignore-errors"], False),
}
CHECKS = ("decision", "order", "refs", "errors")
KNOWN = {
    # C# Markdown ingest drops InstallBefore/InstallAfter; Rust keeps them in the output.
    **{name: {"refs"} for name in ("markdown_input", "markdown_input_ignored", "markdown_missing",
                                   "markdown_missing_ignored", "markdown_cycle", "markdown_stdin")},
    "self_reference_ignored": {"decision", "order", "refs", "errors"},
    "cycle_ignored": {"decision", "order", "refs", "errors"},
}
EXT = {"toml": "toml", "json": "json", "yaml": "yaml", "markdown": "md"}


def components(text, fmt):
    """Top-level (name, InstallBefore, InstallAfter) in file order."""
    if fmt == "toml":
        return [(c.get("Name"), c.get("InstallBefore", []), c.get("InstallAfter", []))
                for c in tomllib.loads(text).get("thisMod", [])]
    if fmt == "json":
        data = json.loads(text)
        data = data.get("thisMod", data.get("Components", [])) if isinstance(data, dict) else data
        return [(c.get("Name"), c.get("InstallBefore", []), c.get("InstallAfter", [])) for c in data]
    if fmt == "yaml":
        out, field = [], None
        for line in text.splitlines():
            if line.startswith("---") or line.startswith("- Guid:"):
                field = None
            name = re.match(r"^(?:- )?Name: ?['\"]?(.*?)['\"]?$", line)
            if name:
                out.append([name.group(1), [], []])
                continue
            key = re.match(r"^(?:  )?(InstallBefore|InstallAfter):(.*)$", line)
            if key:
                field = 1 if key.group(1) == "InstallBefore" else 2
                inline = key.group(2).strip()
                if inline.startswith("[") and out:
                    out[-1][field] += [x.strip(" '\"") for x in inline.strip("[]").split(",") if x.strip()]
                    field = None
                continue
            item = re.match(r"^(?:  )?- (.*)$", line)
            if item and field and out and re.fullmatch(r"[{(]?[0-9a-fA-F-]{32,36}[})]?", item.group(1).strip(" '\"")):
                out[-1][field].append(item.group(1).strip(" '\""))
                continue
            if not line.startswith(" ") and not line.startswith("- "):
                field = None
        return [tuple(c) for c in out]
    return [(m, None, None) for m in re.findall(r"^### (.+)$", text, re.M)]


def guid(text):
    """C# writes a parsed Guid; Rust keeps the authored spelling (see Known differences)."""
    return text.strip("{}()").lower()


def run(argv_head, name, case, work, side, env_extra=None):
    text, in_ext, fmt, flags, stdin = case
    root = os.path.join(work, side, name)
    os.makedirs(os.path.join(root, "home"))
    guide = os.path.join(root, f"guide.{in_ext}")
    with open(guide, "w") as handle:
        handle.write(text)
    out = os.path.join(root, f"out.{EXT[fmt]}")
    argv = argv_head + ["convert"] + (["--stdin"] if stdin else ["-i", guide]) + ["-f", fmt, "-o", out] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "home", ".config"),
               **(env_extra or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300,
                          input=text if stdin else None, stdin=None if stdin else subprocess.DEVNULL)
    written = os.path.exists(out)
    view = {"decision": {"exit0": done.returncode == 0, "written": written},
            "order": None, "refs": None, "errors": None}
    if written:
        with open(out, encoding="utf-8") as handle:
            parsed = components(handle.read(), fmt)
        view["order"] = [c[0] for c in parsed]
        if fmt != "markdown":
            view["refs"] = {c[0]: [[guid(x) for x in c[1]], [guid(x) for x in c[2]]] for c in parsed}
    if done.returncode != 0:
        found = re.search(r"Dependency resolution failed with (\d+) errors", done.stdout + done.stderr)
        view["errors"] = int(found.group(1)) if found else "other: " + (done.stderr or done.stdout).strip()[-300:]
    return view


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="convert-dependency-order-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            head = ["unshare", "-rn", "dotnet", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run(head, name, case, work, "csharp", env) for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual = run([args.bin], name, case, work, "rust")
            expected = oracle[name]
            for check in CHECKS:
                same = actual[check] == expected[check]
                if same:
                    label = "match"
                    matched += 1
                elif check in KNOWN.get(name, ()):
                    label = "known"
                    known += 1
                else:
                    label = "FAIL"
                    failed += 1
                detail = "" if same else f" | rust={actual[check]} | reference={expected[check]}"
                print(f"{label:5} {name}/{check}{detail}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
