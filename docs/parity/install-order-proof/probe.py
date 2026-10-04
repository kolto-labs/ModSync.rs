#!/usr/bin/env python3
"""Install order proof: drives the production `modsync validate` and `install`.

C# validate and install load the guide (`FileLoadingService.LoadFromFileAsync`), then run
`HandleDependencyResolutionErrors` (ModBuildConverter.cs:3092, :3418, :823-877) over
`DependencyResolverService.ResolveDependencies`: every top-level component, selected or
not, is ordered by InstallBefore/InstallAfter with a depth-first post-order, and a
missing, self or circular reference anywhere in the guide stops the run (exit 1) unless
`--ignore-errors`. Install then orders that list again with
`InstallCoordinator.GetOrderedInstallList` (Installation/InstallCoordinator.cs:97-186),
a breadth-first (Kahn) sort over every component by Dependencies, InstallAfter and
InstallBefore, and installs the selected ones in that order
(Services/InstallationService.cs:1219). A Markdown input goes through guide
ingestion, which drops InstallBefore and InstallAfter. `--direct-markdown` keeps the
list order (`preserveInputOrder`, ModBuildConverter.cs:3638).

Each case is a disposable mod/game fixture plus a guide. Component X copies `X.txt` and
`X/z.txt` (holding "X") into Override, so `Override/z.txt` names the last installer.
The Rust binary and the pinned C# CLI (`ModSync.Core.dll`, d31ccbd3) run the same
command line on a fresh copy each, with `--use-file-selection` except in `direct`:

  install    `install -i G -s mods -g game --use-file-selection`; C# also gets
             `-y --skip-validation --no-checkpoint` (it asks for confirmation, and its
             environment check needs a HoloPatcher), Rust `--no-checkpoint`.
  direct     the install with `--direct-markdown --patcher-engine OdyPatcher` (Rust
             `ody-patcher`) and without `--use-file-selection`.
  phase_base Rust only: the install with `--phase base`, checked against EXPECTED.
  validate   `validate -i G -s mods -g game --output json --dry-run --use-file-selection`.
  rootless   `validate -i G --output json --use-file-selection`.

Checks per install case:
  decision  exit code is zero or not;
  order     the components in the order they were installed (C# `[i/n] Installing: X`,
            Rust `X / n: Action` progress lines), null when none was;
  files     the game folder's files outside `.modsync`, with their contents.
Checks per validate/rootless case:
  decision  exit code and `success`;
  shape     `report` (stages were written) or `error` (the run stopped before them);
  errors    the N of `Dependency resolution failed with N errors`, else null;
  listing   the ComponentValidation messages in report order (null without a report);
  dryrun    the DryRun stage messages (null without one).

Known difference (counted separately): validate's dry run simulates the install order
in Rust; C# simulates the list order and installs in another one.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major; --record
also rewrites csharp-oracle.json. Without it the recorded results are used. See
../install-order-contract.md.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
G = {k: f"{k * 8}-{k * 4}-{k * 4}-{k * 4}-{k * 12}" for k in "abcde"}
G["u"] = "12345678-1234-1234-1234-1234567890ab"
G["f"] = "f" * 8 + "-ffff-ffff-ffff-" + "f" * 12  # names no component
KEYS = "abcdeu"


def copy(source):
    return ("Copy", f"<<modDirectory>>/{source}", "<<kotorDirectory>>/Override")


def steps(key):
    return [copy(f"{key}.txt"), copy(f"{key}/z.txt")]


def c(key, selected=1, before=(), after=(), deps=(), instructions=None, widescreen=False):
    return {"key": key, "selected": bool(selected), "before": list(before), "after": list(after),
            "deps": list(deps), "instructions": instructions or steps(key), "widescreen": widescreen}


def fields(comp):
    return (("InstallBefore", comp["before"]), ("InstallAfter", comp["after"]), ("Dependencies", comp["deps"]))


def toml_guide(spec):
    out = []
    for comp in spec:
        key = comp["key"]
        lines = ["[[thisMod]]", f'Guid = "{G[key]}"', f'Name = "{key.upper()}"',
                 f"IsSelected = {'true' if comp['selected'] else 'false'}"]
        for field, keys in fields(comp):
            if keys:
                lines.append(f"{field} = [" + ", ".join(f'"{G[x]}"' for x in keys) + "]")
        if comp["widescreen"]:
            lines.append("WidescreenOnly = true")
        lines.append("")
        for action, source, destination in comp["instructions"]:
            lines += ["[[thisMod.Instructions]]", f'Action = "{action}"', f'Source = ["{source}"]',
                      f'Destination = "{destination}"', ""]
        out.append("\n".join(lines))
    return "\n".join(out)


def json_guide(spec):
    return json.dumps([{
        "Guid": G[comp["key"]], "Name": comp["key"].upper(), "IsSelected": comp["selected"],
        **{field: [G[x] for x in keys] for field, keys in fields(comp) if keys},
        "Instructions": [{"Action": a, "Source": [s], "Destination": d} for a, s, d in comp["instructions"]],
    } for comp in spec], indent=1)


def yaml_guide(spec):
    docs = []
    for comp in spec:
        key = comp["key"]
        lines = [f"Guid: {G[key]}", f"Name: {key.upper()}", f"IsSelected: {'true' if comp['selected'] else 'false'}"]
        for field, keys in fields(comp):
            if keys:
                lines.append(f"{field}:")
                lines += [f"- {G[x]}" for x in keys]
        lines.append("Instructions:")
        for action, source, destination in comp["instructions"]:
            lines += [f"- Action: {action}", "  Source:", f"  - '{source}'", f"  Destination: '{destination}'"]
        docs.append("---\n" + "\n".join(lines) + "\n")
    return "".join(docs)


def markdown_guide(spec):
    out = ["# Example build", "", "## Mod List", ""]
    for comp in spec:
        key = comp["key"]
        out += [f"### {key.upper()}", f"**Name:** [{key.upper()}](https://example.com/{key}.zip)",
                "**Author:** Author", "**Description:** Text.",
                "**Category & Tier:** Textures / Essential", "<!--<<ModSync>>", f"Guid: {G[key]}",
                f"IsSelected: {'true' if comp['selected'] else 'false'}"]
        for field, keys in fields(comp):
            if keys:
                out.append(f"{field}:")
                out += [f"- {G[x]}" for x in keys]
        out.append("Instructions:")
        for action, source, destination in comp["instructions"]:
            out += [f"- Action: {action}", f"  Source: '{source}'", f"  Destination: '{destination}'"]
        out += ["-->", "___", ""]
    return "\n".join(out)


# "txt" and "" hold Markdown text under a name that does not say Markdown.
BUILD = {"toml": toml_guide, "json": json_guide, "yaml": yaml_guide, "md": markdown_guide,
         "txt": markdown_guide, "": markdown_guide}

AFTER_ONE = [c("a"), c("b", after="a"), c("c")]
MISSING_AFTER_UNSELECTED = [c("a"), c("b"), c("c", 0, after="f")]
CYCLE_UNSELECTED = [c("a"), c("b", 0, after="c"), c("c", 0, after="b")]
# B moves the file C creates: fine in install order (A, C, B), not in list order.
MOVE_FROM_C = [c("a"), c("b", after="a", instructions=[copy("b.txt"), ("Move", "<<kotorDirectory>>/Override/c.txt",
                                                                       "<<kotorDirectory>>")]),
               c("c")]

# name: (mode, guide extension, spec)
CASES = {
    # Install order: C# installs breadth-first; Rust took the earliest ready component.
    "after_one": ("install", "toml", AFTER_ONE),
    "dependency": ("install", "toml", [c("a"), c("b", deps="a"), c("c")]),
    "before": ("install", "toml", [c("a"), c("b"), c("c", before="a")]),
    "unselected_link": ("install", "toml", [c("a"), c("u", 0), c("b", after="u"), c("c")]),
    "chain": ("install", "toml", [c("a"), c("b", after="a"), c("c", after="b"), c("d")]),
    "resolver_reorder": ("install", "toml", [c("b", after="a"), c("a"), c("c")]),
    "mixed_edges": ("install", "toml", [c("a"), c("b", deps="a"), c("c", after="a"), c("d", before="b")]),
    "two_after": ("install", "toml", [c("a"), c("b"), c("c", after="ab"), c("d", after="a"), c("e")]),
    "json_after": ("install", "json", AFTER_ONE),
    "yaml_after": ("install", "yaml", AFTER_ONE),
    # Widescreen-only components take their breadth-first place, as in the C# CLI.
    "widescreen_root": ("install", "toml", [c("a"), c("b", after="a"), c("e", widescreen=True)]),
    "widescreen_before_base": ("install", "toml", [c("a"), c("e", widescreen=True), c("b", after="e"), c("c")]),
    "ws_first": ("install", "toml", [c("e", widescreen=True), c("a")]),
    "ws_two": ("install", "toml", [c("d", widescreen=True), c("a"), c("e", widescreen=True), c("b", after="a")]),
    # Controls: the same order before and after.
    "control_no_refs": ("install", "toml", [c("c"), c("a"), c("b")]),
    "control_ordered": ("install", "toml", [c("a"), c("b", after="a")]),
    "control_dependency_first": ("install", "toml", [c("b", deps="a"), c("a")]),
    # Validate lists components in the resolver's order.
    "listing_reorder": ("validate", "toml", [c("b", after="a"), c("a"), c("c")]),
    "listing_before": ("validate", "toml", [c("a"), c("b"), c("c", before="a")]),
    "listing_control": ("validate", "toml", AFTER_ONE),
    # Refusal of bad references anywhere in the guide, before validation or install.
    "missing_after_unselected": ("validate", "toml", MISSING_AFTER_UNSELECTED),
    "missing_before_unselected": ("validate", "toml", [c("a"), c("b", 0, before="f")]),
    "self_unselected": ("validate", "toml", [c("a"), c("b", 0, after="b")]),
    "cycle_unselected": ("validate", "toml", CYCLE_UNSELECTED),
    "cycle_one_unselected": ("validate", "toml", [c("a", after="u"), c("u", 0, after="a")]),
    "missing_after_selected": ("validate", "toml", [c("a"), c("b", after="f")]),
    "cycle_selected": ("validate", "toml", [c("a", after="b"), c("b", after="a")]),
    "missing_json_unselected": ("validate", "json", MISSING_AFTER_UNSELECTED),
    "rootless_missing_unselected": ("rootless", "toml", MISSING_AFTER_UNSELECTED),
    "rootless_cycle_unselected": ("rootless", "toml", CYCLE_UNSELECTED),
    "install_missing_unselected": ("install", "toml", MISSING_AFTER_UNSELECTED),
    "install_cycle_unselected": ("install", "toml", CYCLE_UNSELECTED),
    # Markdown: C# drops the references, so it neither orders by them nor refuses them.
    "markdown_missing_unselected": ("validate", "md", MISSING_AFTER_UNSELECTED),
    "markdown_no_refs": ("install", "md", [c("a"), c("b", deps="a"), c("c")]),
    "markdown_after": ("install", "md", AFTER_ONE),
    "markdown_backward": ("install", "md", [c("b", after="a"), c("a"), c("c")]),
    "markdown_before": ("install", "md", [c("a"), c("b"), c("c", before="a")]),
    "markdown_txt": ("install", "txt", [c("b", after="a"), c("a"), c("c")]),
    "markdown_no_extension": ("install", "", [c("b", after="a"), c("a"), c("c")]),
    # --direct-markdown keeps the source order (C# `preserveInputOrder`).
    "direct_dependency": ("direct", "md", [c("a"), c("b", deps="a"), c("c")]),
    "direct_after": ("direct", "md", [c("b", after="a"), c("a"), c("c")]),
    # Rust-only: --phase base refuses before any write when a base component must
    # follow a widescreen-only one (the C# CLI has no phases; see EXPECTED).
    "phase_base_blocked": ("phase_base", "toml", [c("e", widescreen=True), c("u", 0, after="e"), c("b", after="u")]),
    "phase_base_control": ("phase_base", "toml", [c("a"), c("b", after="a"), c("e", widescreen=True)]),
    # Validate's dry run: Rust simulates the install order, C# the list order.
    "dry_run_order": ("validate", "toml", MOVE_FROM_C),
}
CHECKS = {"install": ("decision", "order", "files"), "direct": ("decision", "order", "files"),
          "phase_base": ("decision", "order", "files"),
          "validate": ("decision", "shape", "errors", "listing", "dryrun"),
          "rootless": ("decision", "shape", "errors", "listing", "dryrun")}
KNOWN = {
    "dry_run_order": {"dryrun"},
}
# Rust-only cases: the documented Rust result, not a C# one.
EXPECTED = {
    "phase_base_blocked": {"decision": False, "order": None, "files": [["chitin.key", "KEY"]]},
    "phase_base_control": {"decision": True, "order": ["A", "B"],
                           "files": [["Override/a.txt", "a"], ["Override/b.txt", "b"], ["Override/z.txt", "b"],
                                     ["chitin.key", "KEY"]]},
}


def fixture(root, extension, spec):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "home"))
    with open(os.path.join(root, "game", "chitin.key"), "w") as handle:
        handle.write("KEY\n")
    for key in KEYS:
        os.makedirs(os.path.join(root, "mods", key))
        with open(os.path.join(root, "mods", f"{key}.txt"), "w") as handle:
            handle.write(key + "\n")
        with open(os.path.join(root, "mods", key, "z.txt"), "w") as handle:
            handle.write(key + "\n")
    path = os.path.join(root, "guide." + extension if extension else "guide")
    with open(path, "w") as handle:
        handle.write(BUILD[extension](spec))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def errors_of(text):
    found = re.search(r"Dependency resolution failed with (\d+) errors", text)
    return int(found.group(1)) if found else None


def report_view(code, stdout, root):
    report = json.loads(stdout[stdout.index("{"):])
    stages = report.get("stages")
    listing = dryrun = None
    for stage in stages or []:
        messages = [m.replace(root, "<root>") for m in stage.get("messages") or []]
        if stage["stage"] == "ComponentValidation":
            listing = messages
        elif stage["stage"] == "DryRun":
            dryrun = messages
    return {
        "decision": [code, report.get("success")],
        "shape": "report" if stages else "error",
        "errors": errors_of(json.dumps(report)),
        "listing": listing,
        "dryrun": dryrun,
    }


def install_view(code, output, game, csharp):
    if csharp:
        names = re.findall(r"\[\d+/\d+\] Installing: (\S+)", output)
    else:
        names = []
        for name in re.findall(r"^(\S+) / \d+: ", output, re.M):
            if name not in names:
                names.append(name)
    files = []
    for directory, folders, entries in os.walk(game):
        folders[:] = [f for f in folders if f != ".modsync"]
        for entry in entries:
            with open(os.path.join(directory, entry), encoding="utf-8", errors="replace") as handle:
                files.append([os.path.relpath(os.path.join(directory, entry), game), handle.read().strip()])
    return {"decision": code == 0, "order": names or None, "files": sorted(files)}


def run(argv_head, case, work, label, csharp, extra_env=None):
    mode, extension, spec = case
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, extension, spec)
    if mode == "rootless":
        argv = argv_head + ["validate", "-i", path, "--output", "json", "--use-file-selection"]
    elif mode == "validate":
        argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game, "--output", "json", "--dry-run",
                            "--use-file-selection"]
    else:
        side = ["-y", "--skip-validation", "--no-checkpoint"] if csharp else ["--no-checkpoint"]
        if mode == "direct":
            side += ["--direct-markdown", "--patcher-engine", "OdyPatcher" if csharp else "ody-patcher"]
        elif mode == "phase_base":
            side += ["--phase", "base"]
        # Direct Markdown installs the whole guide and refuses --use-file-selection.
        selection = [] if mode == "direct" else ["--use-file-selection"]
        argv = argv_head + ["install", "-i", path, "-s", mods, "-g", game] + selection + side
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode in ("install", "direct", "phase_base"):
        return install_view(done.returncode, done.stdout + done.stderr, game, csharp)
    return report_view(done.returncode, done.stdout, root)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="install-order-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            head = ["unshare", "-rn", "dotnet", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run(head, case, work, "csharp", True, env) for name, case in CASES.items()
                      if name not in EXPECTED}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual = run([args.bin], case, work, "rust", False)
            expected = EXPECTED.get(name) or oracle[name]
            for check in CHECKS[case[0]]:
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
