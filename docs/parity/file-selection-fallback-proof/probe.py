#!/usr/bin/env python3
"""File selection fallback proof: drives the production `modsync`.

C# `ModComponentSerializationService.DeserializeModComponentFromString`
(ModComponentSerializationService.cs:895-903) selects every component when the file
selects none. The CLI's validate and install load guides through it
(`FileLoadingService.LoadFromFileAsync`, ModBuildConverter.cs:3089 and :3417), so
`--use-file-selection` then reads every component as selected
(ModBuildConverter.cs:3175-3194 and :3431-3434). A Markdown file goes through guide
ingestion instead (FileLoadingService.cs:60-70), which keeps the authored flags.

Each case is a disposable mod/game fixture plus a guide. The Rust binary and the
pinned C# CLI (`ModSync.Core.dll`, d31ccbd3) run the same command line on a fresh
copy each:

  validate   `validate -i G -s mods -g game --output json --dry-run [flags]`
  rootless   `validate -i G --output json [flags]` (the report without roots)
  install    `install -i G -s mods -g game [flags]`; C# also gets `-y --skip-validation`
             (its environment check needs a HoloPatcher and it asks for confirmation),
             Rust `--no-checkpoint` (no checkpoint history in the disposable game).

Checks per validate/rootless case:
  decision  exit code and `success`;
  counts    `errorCount`, `warningCount`, `passedCount`, `componentCount`;
  stages    each stage's name, passed and ComponentValidation's messages
            (or the setup `error` when there is no report).
Checks per install case:
  decision  exit code is zero or not;
  files     the game folder's files outside `.modsync`.

Usage:
  probe.py [--bin target/debug/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major; --record
also rewrites csharp-oracle.json. Without it the recorded results are used. See
../file-selection-fallback-contract.md.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
G1 = "44444444-5555-6666-7777-888888888881"
G2 = "44444444-5555-6666-7777-888888888882"
OPTION = "44444444-5555-6666-7777-8888888888a1"
COPY_M = ('Copy', '<<modDirectory>>/m.txt')
COPY_N = ('Copy', '<<modDirectory>>/n.txt')
COPY_ABSENT = ('Copy', '<<modDirectory>>/absent.txt')


def toml_component(guid, name, selected, steps, option=None):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f'Name = "{name}"']
    if selected is not None:
        lines.append(f"IsSelected = {'true' if selected else 'false'}")
    lines.append("")
    for action, source in steps:
        lines += ["[[thisMod.Instructions]]", f'Action = "{action}"']
        if action == "Choose":
            lines.append(f'Source = ["{OPTION}"]')
        else:
            lines += [f'Source = ["{source}"]', 'Destination = "<<kotorDirectory>>/Override"']
        lines.append("")
    if option is not None:
        option_selected, option_steps = option
        lines += ["[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "Option"',
                  f"IsSelected = {'true' if option_selected else 'false'}", ""]
        for action, source in option_steps:
            lines += ["[[thisMod.Options.Instructions]]", f'Action = "{action}"',
                      f'Source = ["{source}"]', 'Destination = "<<kotorDirectory>>/Override"', ""]
    return "\n".join(lines)


def json_guide(selections):
    components = []
    for (guid, name, selected, source) in selections:
        components.append({"Guid": guid, "Name": name, "IsSelected": selected,
                           "Instructions": [{"Action": "Copy", "Source": [source],
                                             "Destination": "<<kotorDirectory>>/Override"}]})
    return json.dumps(components, indent=1)


def yaml_guide(selections):
    lines = []
    for (guid, name, selected, source) in selections:
        lines += ["---", f"Guid: {guid}", f"Name: {name}", f"IsSelected: {'true' if selected else 'false'}",
                  "Instructions:", "  - Action: Copy", f"    Source: ['{source}']",
                  "    Destination: '<<kotorDirectory>>/Override'"]
    return "\n".join(lines) + "\n"


A_FALSE = toml_component(G1, "A", False, [COPY_M])
B_FALSE_ABSENT = toml_component(G2, "B", False, [COPY_ABSENT])
TWO_FALSE = A_FALSE + "\n" + toml_component(G2, "B", False, [COPY_N])
FALSE_ABSENT = A_FALSE + "\n" + B_FALSE_ABSENT
MIXED = toml_component(G1, "A", True, [COPY_M]) + "\n" + B_FALSE_ABSENT
OMITTED = toml_component(G1, "A", None, [COPY_M]) + "\n" + toml_component(G2, "B", None, [COPY_ABSENT])
OPTION_FALSE = toml_component(G1, "A", False, [("Choose", None)], option=(True, [COPY_N]))
OPTION_UNSELECTED = toml_component(G1, "A", False, [COPY_M, ("Choose", None)],
                                   option=(False, [COPY_ABSENT]))
JSON_FALSE = json_guide([(G1, "A", False, "<<modDirectory>>/m.txt"),
                         (G2, "B", False, "<<modDirectory>>/absent.txt")])
YAML_FALSE = yaml_guide([(G1, "A", False, "<<modDirectory>>/m.txt"),
                         (G2, "B", False, "<<modDirectory>>/absent.txt")])


def markdown_guide(first, second):
    return f"""# Example build

## Mod List

### A
**Name:** [A](https://example.com/a.zip)
**Author:** Author One
**Description:** First.
**Category & Tier:** Textures & Models / Essential
<!--<<ModSync>>
Guid: {G1}
IsSelected: {'true' if first else 'false'}
Instructions:
- Action: Copy
  Source: '<<modDirectory>>/m.txt'
  Destination: '<<kotorDirectory>>/Override'
-->
___

### B
**Name:** [B](https://example.com/b.zip)
**Author:** Author Two
**Description:** Second.
**Category & Tier:** Textures / Optional
<!--<<ModSync>>
Guid: {G2}
IsSelected: {'true' if second else 'false'}
Instructions:
- Action: Copy
  Source: '<<modDirectory>>/n.txt'
  Destination: '<<kotorDirectory>>/Override'
-->
"""


FILE = ["--use-file-selection"]

# name -> (mode, guide extension, guide text, extra flags)
CASES = {
    # The gap: the file selects nothing and --use-file-selection reads its flags.
    "one_false": ("validate", "toml", A_FALSE, FILE),
    "two_false": ("validate", "toml", TWO_FALSE, FILE),
    "two_false_one_absent": ("validate", "toml", FALSE_ABSENT, FILE),
    "omitted_flags": ("validate", "toml", OMITTED, FILE),
    "option_selected_component_false": ("validate", "toml", OPTION_FALSE, FILE),
    "option_unselected_component_false": ("validate", "toml", OPTION_UNSELECTED, FILE),
    "json_false": ("validate", "json", JSON_FALSE, FILE),
    "yaml_false": ("validate", "yaml", YAML_FALSE, FILE),
    "rootless_one_false": ("rootless", "toml", A_FALSE, FILE),
    "rootless_false_absent": ("rootless", "toml", FALSE_ABSENT, FILE),
    "install_two_false": ("install", "toml", TWO_FALSE, FILE),
    # Controls: unchanged before and after.
    "mixed_file_selection": ("validate", "toml", MIXED, FILE),
    "two_false_no_flag": ("validate", "toml", FALSE_ABSENT, []),
    "mixed_no_flag": ("validate", "toml", MIXED, []),
    "rootless_mixed": ("rootless", "toml", MIXED, FILE),
    # Markdown goes through guide ingestion, which keeps the authored flags (both refuse).
    "markdown_false": ("validate", "md", markdown_guide(False, False), FILE),
    "markdown_mixed": ("validate", "md", markdown_guide(True, False), FILE),
    "markdown_false_no_flag": ("validate", "md", markdown_guide(False, False), []),
    "markdown_false_rootless": ("rootless", "md", markdown_guide(False, False), FILE),
    "install_mixed": ("install", "toml", toml_component(G1, "A", True, [COPY_M]) + "\n"
                      + toml_component(G2, "B", False, [COPY_N]), FILE),
}
CHECKS = {"validate": ("decision", "counts", "stages"), "rootless": ("decision", "counts", "stages"),
          "install": ("decision", "files")}


def fixture(root, extension, guide):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "mods"))
    os.makedirs(os.path.join(root, "home"))
    with open(os.path.join(root, "game", "chitin.key"), "w") as handle:
        handle.write("KEY\n")
    for name in ("m.txt", "n.txt"):
        with open(os.path.join(root, "mods", name), "w") as handle:
            handle.write(name + "\n")
    path = os.path.join(root, "guide." + extension)
    with open(path, "w") as handle:
        handle.write(guide)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def report_view(code, stdout, root):
    report = json.loads(stdout[stdout.index("{"):])
    stages = []
    for stage in report.get("stages", []):
        messages = None
        if stage["stage"] == "ComponentValidation":
            messages = [m.replace(root, "<root>") for m in stage.get("messages") or []]
        stages.append([stage["stage"], stage["passed"], messages])
    return {
        "decision": [code, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount"),
                   report.get("componentCount")],
        "stages": stages if "stages" in report else report.get("error"),
    }


def install_view(code, game):
    files = []
    for directory, folders, names in os.walk(game):
        folders[:] = [f for f in folders if f != ".modsync"]
        files += [os.path.relpath(os.path.join(directory, n), game) for n in names]
    return {"decision": code == 0, "files": sorted(files)}


def run(argv_head, case, work, label, csharp, extra_env=None):
    mode, extension, guide, flags = case
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, extension, guide)
    if mode == "rootless":
        argv = argv_head + ["validate", "-i", path, "--output", "json"] + flags
    elif mode == "validate":
        argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game, "--output", "json", "--dry-run"] + flags
    else:
        side = ["-y", "--skip-validation"] if csharp else ["--no-checkpoint"]
        argv = argv_head + ["install", "-i", path, "-s", mods, "-g", game] + flags + side
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode == "install":
        return install_view(done.returncode, game)
    return report_view(done.returncode, done.stdout, root)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="file-selection-fallback-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            head = ["unshare", "-rn", "dotnet", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run(head, case, work, "csharp", True, env) for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = failed = 0
        for name, case in CASES.items():
            actual = run([args.bin], case, work, "rust", False)
            expected = oracle[name]
            for check in CHECKS[case[0]]:
                same = actual[check] == expected[check]
                matched += same
                failed += not same
                detail = "" if same else f" | rust={actual[check]} | reference={expected[check]}"
                print(f"{'match' if same else 'FAIL':5} {name}/{check}{detail}")
        print(f"{matched} match / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
