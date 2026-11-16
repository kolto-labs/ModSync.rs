#!/usr/bin/env python3
"""Run a production prediction/install lane against explicitly provisioned assets.

Every invocation creates fresh game/mod copies under --work-dir, which must not
exist. Inputs are absolute paths on a trusted Linux runner. --output must also be
fresh and stores only traces, logs and provenance, leaving copied assets local.

The mandatory supplied lane uses actual executables selected by each guide
Patcher instruction. --supplied-runner-json configures their GUI interaction
driver, for example
  {"command":["/opt/modsync/gui-driver","--requests","{request_dir}"],
   "xvfb":"/usr/bin/Xvfb","wine":"/usr/bin/wine",
   "wineserver":"/usr/bin/wineserver"}
The driver command is an explicitly installed automation consumer, with its own
CLI. It watches engine request JSON files in request_dir and interacts with the
request's headlessDisplay and stagedExecutable, selecting namespaceName/index
and gameTarget. The engine starts the original executable with no invented CLI,
waits for process quiescence and verifies its installation log. The consumer must
remain alive until terminated after the lane. It must fail on unknown dialogs.
Command tokens may contain {request_dir}, {game_dir}, {source_dir}, {output_dir}.
Wine and wineserver are configured together. Wine prefixes are created privately
inside the disposable request directory. Native installers can omit both.

The runner must already have Python 3.10+, Rust/system build dependencies, Xvfb,
any configured patchers/driver and legally obtained complete game/mod baselines.
No asset download, original-installer substitution or exclusion is performed.
"""

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import time
from pathlib import Path

from compare_filesystem_traces import TraceError, load_trace, read_json


def absolute_file(value):
    path = Path(value)
    if not path.is_absolute() or not path.is_file():
        raise ValueError(f"expected an existing absolute file path: {value!r}")
    return path.resolve()


def absolute_directory(value):
    path = Path(value)
    if not path.is_absolute() or not path.is_dir():
        raise ValueError(f"expected an existing absolute directory path: {value!r}")
    return path.resolve()


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def require_disjoint(paths):
    for index, left in enumerate(paths):
        for right in paths[index + 1:]:
            if left == right or left in right.parents or right in left.parents:
                raise ValueError(f"paths must be disjoint: {left} and {right}")


def copy_root(source, destination):
    # Reject links and devices before copying so installed programs cannot reach
    # original assets through a copied link. Each lane owns every copied inode.
    for root, directories, files in os.walk(source, followlinks=False):
        for name in directories + files:
            path = Path(root) / name
            mode = path.lstat().st_mode
            if not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                raise ValueError(f"baseline contains a link or special file: {path}")
    shutil.copytree(source, destination)
    # Baselines may deliberately be read-only. Only private copies become writable.
    for root, directories, files in os.walk(destination):
        directory = Path(root)
        directory.chmod(directory.stat().st_mode | stat.S_IRUSR | stat.S_IWUSR | stat.S_IXUSR)
        for name in files:
            path = directory / name
            path.chmod(path.stat().st_mode | stat.S_IRUSR | stat.S_IWUSR)


def selection_arguments(value):
    arguments = json.loads(value)
    if not isinstance(arguments, list) or any(not isinstance(item, str) for item in arguments):
        raise ValueError("selection arguments must be a JSON array of strings")
    switches = {"--use-file-selection", "--spoiler-free", "--fomod-skip", "--direct-markdown"}
    selectors = {"--select", "--option", "--deselect", "--deselect-option", "--select-through-tier", "--add-category", "--select-visible-search"}
    accepting_values = False
    for argument in arguments:
        if argument in switches:
            accepting_values = False
        elif argument in selectors:
            accepting_values = True
        elif argument.startswith("-") or not accepting_values:
            raise ValueError(f"unsupported selection argument: {argument!r}")
    return arguments


def supplied_configuration(value, work, output):
    config = json.loads(value)
    if not isinstance(config, dict) or set(config) - {"command", "xvfb", "wine", "wineserver"}:
        raise ValueError("supplied runner requires command/xvfb and optional wine/wineserver")
    command = config.get("command")
    if not isinstance(command, list) or not command or any(not isinstance(item, str) for item in command):
        raise ValueError("supplied command must be a nonempty JSON string array")
    executable = absolute_file(command[0])
    requests = work / "requests"
    requests.mkdir()
    replacements = {"request_dir": str(requests), "game_dir": str(work / "game"), "source_dir": str(work / "mods"), "output_dir": str(output)}
    driver = [str(executable)]
    for argument in command[1:]:
        for key, replacement in replacements.items():
            argument = argument.replace("{" + key + "}", replacement)
        driver.append(argument)
    flags = ["--provided-request-dir", str(requests), "--provided-xvfb", str(absolute_file(config.get("xvfb", "")))]
    if bool(config.get("wine")) != bool(config.get("wineserver")):
        raise ValueError("wine and wineserver must be configured together")
    if config.get("wine"):
        prefix = requests / "wine-prefix"
        prefix.mkdir()
        flags += ["--provided-wine", str(absolute_file(config["wine"])), "--provided-wineserver", str(absolute_file(config["wineserver"])), "--provided-wine-prefix", str(prefix)]
    return driver, flags


def stop_process(process):
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=15)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def run_lane(args):
    binary = absolute_file(args.binary)
    guide = absolute_file(args.guide)
    game = absolute_directory(args.game_root)
    mods = absolute_directory(args.mods_root)
    work = args.work_dir.absolute().resolve()
    output = args.output.absolute().resolve()
    require_disjoint([game, mods, work, output])
    if work.exists() or output.exists():
        raise ValueError("work and output directories must both be fresh")
    if work in guide.parents or output in guide.parents:
        raise ValueError("guide must be outside work and output directories")
    extra = selection_arguments(args.selection_args_json)
    work.mkdir(parents=True)
    output.mkdir(parents=True)
    metadata = {"version": 1, "lane": args.lane, "guide_sha256": digest(guide), "binary_sha256": digest(binary), "selection_arguments": extra, "status": "preparing"}
    receipt = output / "run.json"
    receipt.write_text(json.dumps(metadata, indent=2) + "\n")
    driver = None
    process = None
    try:
        copy_root(game, work / "game")
        copy_root(mods, work / "mods")
        copied_guide = work / ("guide" + guide.suffix)
        shutil.copy2(guide, copied_guide)
        settings = work / "settings"
        scratch = work / "scratch"
        settings.mkdir()
        scratch.mkdir()
        patcher_engine = "holo-patcher" if args.lane == "holo-patcher" else "ody-patcher"
        command = [str(binary), "predict" if args.lane == "predict" else "install", "--input", str(copied_guide), "--game-dir", str(work / "game"), "--source-dir", str(work / "mods"), "--settings-dir", str(settings), "--scratch-dir", str(scratch), "--filesystem-trace", str(output / "trace"), "--non-interactive", "--patcher-engine", patcher_engine, *extra]
        driver_command = None
        if args.lane == "supplied":
            driver_command, flags = supplied_configuration(args.supplied_runner_json, work, output)
            command += ["--allow-processes", "--component-checkpoints", *flags]
        elif args.lane == "holo-patcher":
            patcher = absolute_file(args.holopatcher)
            command += ["--allow-processes", "--patcher", str(patcher)]
            metadata["patcher_sha256"] = digest(patcher)
        metadata["command"] = command
        metadata["status"] = "running"
        receipt.write_text(json.dumps(metadata, indent=2) + "\n")
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr, (output / "driver.log").open("wb") as driver_log:
            if driver_command:
                driver = subprocess.Popen(driver_command, stdout=driver_log, stderr=subprocess.STDOUT)
            process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
            deadline = time.monotonic() + args.timeout_seconds
            while process.poll() is None:
                if driver is not None and driver.poll() is not None:
                    raise RuntimeError(f"supplied GUI driver exited early ({driver.returncode}); see driver.log")
                if time.monotonic() >= deadline:
                    raise RuntimeError("lane timeout reached; disposable roots retained for recovery")
                time.sleep(0.25)
            if process.returncode != 0:
                raise RuntimeError(f"production CLI failed ({process.returncode}); see stderr.log")
            if driver is not None and driver.poll() is not None:
                raise RuntimeError(f"supplied GUI driver exited early ({driver.returncode}); see driver.log")
        trace = load_trace(output / "trace")
        if args.lane == "supplied":
            requests = [read_json(path) for path in (work / "requests").glob("*.json") if path.name != "wine-prefix-active.json"]
            patchers = [item for name, item in trace.items() if name.startswith("step_") and item.get("action") == "Patcher" and not item["skipped"]]
            verified = [item for item in requests if isinstance(item, dict) and item.get("status") == "processAndLogVerified"]
            if not patchers or len(verified) != len(patchers):
                raise RuntimeError(f"supplied lane requires verified real installer requests for every reached Patcher ({len(patchers)} steps, {len(verified)} verified requests)")
            metadata["supplied_requests"] = verified
        metadata["status"] = "completed"
        metadata["instruction_count"] = len(trace) - 2
    except BaseException as error:
        metadata["status"] = "failed"
        metadata["error"] = str(error)
        raise
    finally:
        # Retain copied roots/checkpoints on failure. Never retry on dirty roots.
        stop_process(process)
        stop_process(driver)
        receipt.write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"{args.lane}: complete trace with {metadata['instruction_count']} reached instructions in {output}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--lane", choices=["predict", "supplied", "ody-patcher", "holo-patcher"], required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--guide", required=True)
    parser.add_argument("--game-root", required=True)
    parser.add_argument("--mods-root", required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--supplied-runner-json", default="{}")
    parser.add_argument("--holopatcher", default="")
    parser.add_argument("--selection-args-json", default="[]")
    parser.add_argument("--timeout-seconds", type=int, default=18000)
    args = parser.parse_args()
    try:
        if args.timeout_seconds <= 0:
            raise ValueError("timeout must be positive")
        run_lane(args)
    except (OSError, ValueError, RuntimeError, TraceError) as error:
        print(f"Full run failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
