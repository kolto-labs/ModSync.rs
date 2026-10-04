#!/usr/bin/env python3
"""Scalar empty Source proof: drives the production `modsync`.

A scalar empty Source (`<Source></Source>` or whitespace in XML, `Source = ""`
or `Source: ''` in Markdown metadata) is a single empty path, not an empty list.
TOML, JSON and YAML guides keep it as `[""]`, and path resolution rejects it.
Before this slice the XML reader and the second Markdown metadata pass turned it
into an empty list, so it took the empty-list behaviour of
../empty-source-contract.md instead.

Each case is a disposable mod/game fixture and a guide that first copies
`a.txt` and `b.txt` into `Override`, then runs the case step. The Rust binary
runs `validate --output json --dry-run`, plain `validate` and `install` on fresh
copies, as in ../empty-source-proof. The expected Rust decision comes from the
reference load result recorded in csharp-oracle.json (C# `convert`, see
../source-normalization-scalar-oracle):

- the reference refuses to load the guide: every Rust verb refuses it;
- the reference loads the case step with an empty Source list: the empty-list
  contract (Copy: dry-run report passes with DryRunSkipped, validate and install
  refuse; Delete with Overwrite false: everything passes, tree unchanged);
- the reference loads the step with a path: everything passes;
- the reference drops the whole Markdown metadata block (its typed parser throws
  and the legacy fallback recovers no instructions): a documented difference,
  Rust refuses with every verb, as for a standalone TOML/YAML guide.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp REF/ModSync.Core.dll [--record]]

With --csharp the reference loads are run live (`unshare -rn`, no network) and
must agree with the recorded ones; --record rewrites csharp-oracle.json.
"""
import argparse
import html
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
GUID = "33333333-3333-3333-3333-333333333333"
OVERRIDE = "<<kotorDirectory>>/Override"
TREE = {"Override/a.txt": "A\n", "Override/b.txt": "B\n"}

# Source spellings: ("scalar", text), ("list", [..]), ("nokey",), ("selfclose",) (XML only)
COPY_AB = ("Copy", ("list", ["<<modDirectory>>/a.txt", "<<modDirectory>>/b.txt"]), OVERRIDE, None)


def copy(source):
    return ("Copy", source, OVERRIDE, None)


def delete(source):
    return ("Delete", source, "", False)


# name -> (format, case step)
CASES = {
    "xml_empty_copy": ("xml", copy(("scalar", ""))),
    "xml_whitespace_copy": ("xml", copy(("scalar", "  "))),
    "xml_empty_delete_false": ("xml", delete(("scalar", ""))),
    "xml_selfclose_copy": ("xml", copy(("selfclose",))),
    "xml_selfclose_delete_false": ("xml", delete(("selfclose",))),
    "xml_nokey_copy": ("xml", copy(("nokey",))),
    "xml_path_copy": ("xml", copy(("scalar", "a.txt"))),
    "md_toml_empty_copy": ("md_toml", copy(("scalar", ""))),
    "md_toml_whitespace_copy": ("md_toml", copy(("scalar", "  "))),
    "md_toml_empty_delete_false": ("md_toml", delete(("scalar", ""))),
    "md_toml_blank_list_copy": ("md_toml", copy(("list", [""]))),
    "md_toml_empty_list_delete_false": ("md_toml", delete(("list", []))),
    "md_toml_nokey_copy": ("md_toml", copy(("nokey",))),
    "md_yaml_empty_copy": ("md_yaml", copy(("scalar", ""))),
    "md_yaml_whitespace_copy": ("md_yaml", copy(("scalar", "  "))),
    "md_yaml_empty_delete_false": ("md_yaml", delete(("scalar", ""))),
    "md_yaml_empty_list_copy": ("md_yaml", copy(("list", []))),
    "md_yaml_path_copy": ("md_yaml", copy(("scalar", "a.txt"))),
    "toml_empty_copy_control": ("toml", copy(("scalar", ""))),
}
EXT = {"xml": "xml", "md_toml": "md", "md_yaml": "md", "toml": "toml"}


def toml_steps(steps, table):
    lines = []
    for action, source, destination, overwrite in steps:
        lines += ["", f"[[{table}]]", f"Action = {json.dumps(action)}"]
        if source[0] == "scalar":
            lines.append(f"Source = {json.dumps(source[1])}")
        elif source[0] == "list":
            lines.append("Source = [" + ", ".join(json.dumps(s) for s in source[1]) + "]")
        if destination:
            lines.append(f"Destination = {json.dumps(destination)}")
        if overwrite is not None:
            lines.append(f"Overwrite = {'true' if overwrite else 'false'}")
    return lines


def yaml_steps(steps):
    lines = ["Instructions:"]
    for action, source, destination, overwrite in steps:
        lines.append(f"  - Action: {action}")
        if source[0] == "scalar":
            lines.append(f"    Source: {json.dumps(source[1])}")
        elif source[0] == "list":
            lines.append("    Source: [" + ", ".join(json.dumps(s) for s in source[1]) + "]")
        if destination:
            lines.append(f"    Destination: {json.dumps(destination)}")
        if overwrite is not None:
            lines.append(f"    Overwrite: {'true' if overwrite else 'false'}")
    return lines


def xml_steps(steps):
    out = ""
    for action, source, destination, overwrite in steps:
        body = f"<Action>{action}</Action>"
        if source[0] == "scalar":
            body += f"<Source>{html.escape(source[1])}</Source>"
        elif source[0] == "selfclose":
            body += "<Source />"
        elif source[0] == "list":
            body += "".join(f"<Source>{html.escape(s)}</Source>" for s in source[1])
        if destination:
            body += f"<Destination>{html.escape(destination)}</Destination>"
        if overwrite is not None:
            body += f"<Overwrite>{'true' if overwrite else 'false'}</Overwrite>"
        out += f"<Instructions>{body}</Instructions>"
    return out


def guide(fmt, step):
    steps = [COPY_AB, step]
    if fmt == "toml":
        lines = ["[[thisMod]]", f'Guid = "{GUID}"', 'Name = "Scalar Source Probe"', "IsSelected = true"]
        return "\n".join(lines + toml_steps(steps, "thisMod.Instructions")) + "\n"
    if fmt == "xml":
        return (
            '<ModSync xmlns:json="http://james.newtonking.com/projects/json"><ModComponents>'
            f"<Guid>{GUID}</Guid><Name>Scalar Source Probe</Name><IsSelected>true</IsSelected>"
            f"{xml_steps(steps)}</ModComponents></ModSync>\n"
        )
    if fmt == "md_toml":
        meta = [f'Guid = "{GUID}"', 'Name = "Scalar Source Probe"'] + toml_steps(steps, "Instructions")
    else:
        meta = [f"Guid: {GUID}", "Name: Scalar Source Probe"] + yaml_steps(steps)
    return (
        "# Guide\n\n## Mod List\n\n### Scalar Source Probe\n\n**Name:** Scalar Source Probe\n\n"
        "Some description text.\n\n<!--<<ModSync>>\n" + "\n".join(meta) + "\n-->\n"
    )


def fixture(root, name):
    fmt, step = CASES[name]
    mods = os.path.join(root, "mods")
    game = os.path.join(root, "game")
    os.makedirs(os.path.join(game, "Override"))
    os.makedirs(mods)
    os.makedirs(os.path.join(root, "scratch"))
    for file, text in (("a.txt", "A\n"), ("b.txt", "B\n")):
        with open(os.path.join(mods, file), "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide." + EXT[fmt])
    with open(path, "w") as handle:
        handle.write(guide(fmt, step))
    return path, mods, game


def tree(game):
    result = {}
    for directory, _, files in os.walk(game):
        for file in files:
            full = os.path.join(directory, file)
            with open(full) as handle:
                result[os.path.relpath(full, game).replace(os.sep, "/")] = handle.read()
    return dict(sorted(result.items()))


def run_rust(binary, mode, name, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, name)
    verb = "install" if mode == "install" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "dryrun":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    result = {
        "accepted": done.returncode == 0,
        "tree": tree(game) if done.returncode == 0 and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if done.returncode else [],
    }
    if mode == "dryrun":
        report = json.loads(done.stdout)
        issues = report.get("dryRun", {}).get("issues", [])
        # A guide that fails to load has errorCount 0 and an "error" field.
        result["accepted"] = done.returncode == 0 and report["errorCount"] == 0
        result["skipped"] = any(issue["category"] == "DryRunSkipped" for issue in issues)
    return result


def run_csharp(dll, name, work):
    """Reference load: C# `convert` to JSON, the scalar-oracle method."""
    root = tempfile.mkdtemp(prefix="csharp-", dir=work)
    path, _, _ = fixture(root, name)
    out = os.path.join(root, "out.json")
    env = dict(
        os.environ,
        HOME=os.path.join(root, "home"),
        DOTNET_CLI_TELEMETRY_OPTOUT="1",
        DOTNET_ROLL_FORWARD="Major",
    )
    argv = ["unshare", "-rn", "dotnet", dll, "convert", "--input", path, "--output", out,
            "--format", "json", "--plaintext", "--non-interactive"]
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
    if done.returncode != 0 or not os.path.exists(out):
        return {"loaded": False, "exit": done.returncode, "instructions": None}
    with open(out) as handle:
        components = json.load(handle)
    steps = [
        {"Action": i.get("Action"), "Source": i.get("Source", [])}
        for c in components
        for i in c.get("Instructions", [])
    ]
    return {"loaded": True, "exit": done.returncode, "instructions": steps}


def expected(name, reference):
    """Rust decision per mode, derived from the reference load."""
    action = CASES[name][1][0]
    refused = {m: {"accepted": False, "tree": None} for m in ("dryrun", "validate", "install")}
    refused["dryrun"]["skipped"] = None  # not compared for a refusal
    if not reference["loaded"]:
        return "reference refuses to load", refused
    steps = reference["instructions"]
    if len(steps) != 2:
        return "reference drops the metadata block (documented difference)", refused
    source = steps[1]["Source"]
    passed = {
        "dryrun": {"accepted": True, "tree": None, "skipped": False},
        "validate": {"accepted": True, "tree": None},
        "install": {"accepted": True, "tree": TREE},
    }
    if source:
        return "reference loads a path", passed
    if action == "Delete":
        return "reference loads an empty list (non-strict Delete)", passed
    skipped = {
        "dryrun": {"accepted": True, "tree": None, "skipped": True},
        "validate": {"accepted": False, "tree": None},
        "install": {"accepted": False, "tree": None},
    }
    return "reference loads an empty list", skipped


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="scalar-empty-source-proof-")
    try:
        with open(ORACLE) if os.path.exists(ORACLE) else open(os.devnull) as handle:
            text = handle.read()
        recorded = json.loads(text) if text else {}
        matched = failed = 0
        if args.csharp:
            live = {name: run_csharp(args.csharp, name, work) for name in CASES}
            for name in CASES:
                same = recorded.get(name) == live[name]
                if not args.record:
                    matched, failed = matched + same, failed + (not same)
                    print(f"{'match' if same else 'FAIL':5} {name}/reference: live {live[name]}")
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(live, handle, indent=1, sort_keys=True)
                    handle.write("\n")
                recorded = live
        for name in CASES:
            basis, wanted = expected(name, recorded[name])
            for mode in ("dryrun", "validate", "install"):
                actual = run_rust(args.bin, mode, name, work)
                want = wanted[mode]
                same = actual["accepted"] == want["accepted"] and actual["tree"] == want["tree"]
                if mode == "dryrun" and want["skipped"] is not None:
                    same = same and actual["skipped"] == want["skipped"]
                matched, failed = matched + same, failed + (not same)
                print(
                    f"{'match' if same else 'FAIL':5} {name}/{mode}: rust accepted={actual['accepted']}"
                    + (f" skipped={actual['skipped']}" if mode == "dryrun" else "")
                    + f" | expected accepted={want['accepted']}"
                    + (f" skipped={want['skipped']}" if mode == "dryrun" else "")
                    + f" ({basis})"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
