#!/usr/bin/env python3
"""Private-source, fail-closed full-guide port runner (Python 3.11+)."""

import argparse
from contextlib import nullcontext
from cache import link_key
import gui_diag
import live_output
from fullrun_contract import require_gui_phases
import fullrun_guide_choices
import k1_widescreen_inputs
import install_profiles
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import tomllib
from urllib.parse import unquote, urldefrag, urlsplit, urlunsplit
import uuid


class Blocked(Exception):
    pass


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


FAILURE_TAIL_LINES = 80
GUI_STAGE = "gui"
STAGE_DURATIONS_KEY = "stage_durations"
GUI_REPORT_NAME = "gui-report.json"
GUI_INSTALL_FAILURE_REASON = "backend_install_gate_or_failure"
GUI_INSTALL_FAILURES_KEY = "gui_install_failures"
GUI_TAIL_LINES = 200
GUI_TAIL_GROUP = "::group::gui worker tail"
DIAGNOSTICS_MAX_DEPTH = 4
DIAGNOSTICS_MAX_ITEMS = 400


def sanitize_diagnostics(value, depth=0):
    """Redact every string again and bound the shape of driver-supplied diagnostics."""
    if isinstance(value, str):
        redacted = gui_diag.redact(value)
        return redacted if len(redacted) <= gui_diag.LINE_LIMIT else redacted[:gui_diag.LINE_LIMIT] + "..."
    if value is None or isinstance(value, (bool, int, float)):
        return value
    if depth >= DIAGNOSTICS_MAX_DEPTH:
        return None
    if isinstance(value, list):
        return [sanitize_diagnostics(item, depth + 1) for item in value[-DIAGNOSTICS_MAX_ITEMS:]]
    if isinstance(value, dict):
        return {sanitize_diagnostics(str(key), depth + 1): sanitize_diagnostics(item, depth + 1)
                for key, item in list(value.items())[:DIAGNOSTICS_MAX_ITEMS]}
    return None


def load_worker_diagnostics(path):
    """The driver report's `worker_diagnostics`, sanitized, or None when absent or unreadable."""
    try:
        if path.is_symlink() or not path.is_file():
            return None
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    diagnostics = data.get("worker_diagnostics") if isinstance(data, dict) else None
    if not isinstance(diagnostics, dict) or not diagnostics:
        return None
    return sanitize_diagnostics(diagnostics)


def gui_stage_name(attempt):
    """`gui` for the first GUI attempt, `gui_retry_N` for the quarantine retries."""
    return GUI_STAGE if attempt == 0 else f"{GUI_STAGE}_retry_{attempt}"


def is_gui_stage(stage):
    return stage == GUI_STAGE or stage.startswith(GUI_STAGE + "_retry_")


def gui_report_name(stage):
    return GUI_REPORT_NAME if stage == GUI_STAGE else f"{stage}-report.json"


def reemit_gui_failure(text, driver_report, report):
    """Make a failed GUI driver visible in the plain CI log and copy its diagnostics into the lane report.

    The driver prints its own `::group::gui worker tail` block (already redacted) just before
    its final JSON line; it is re-emitted without an enclosing group because Actions groups do
    not nest. Every line is redacted again before it reaches the log.
    """
    lines = [gui_diag.CONTROL.sub("", gui_diag.redact(line))
             for line in text.splitlines()[-GUI_TAIL_LINES:]]
    print(f"gui output (last {GUI_TAIL_LINES} lines)", file=sys.stderr)
    live_output.emit_lines("\n".join(lines))
    diagnostics = load_worker_diagnostics(driver_report)
    if diagnostics is None:
        return
    report["worker_diagnostics"] = diagnostics
    if not any(line.startswith(GUI_TAIL_GROUP) for line in lines):
        live_output.emit_lines("\n".join(gui_diag.render_group(diagnostics)))


def progress(message):
    """One progress line in the CI log, flushed so it shows while the lane is still running."""
    print("ports: " + message, file=sys.stderr, flush=True)


def format_duration(seconds):
    seconds = max(float(seconds), 0.0)
    if seconds < 60:
        return f"{seconds:.1f}s"
    minutes, rest = divmod(round(seconds), 60)
    hours, minutes = divmod(minutes, 60)
    return f"{hours}h{minutes:02d}m{rest:02d}s" if hours else f"{minutes}m{rest:02d}s"


def finish_stage(report, stage, started, outcome):
    """Record a stage's wall-clock seconds in report["stage_durations"] and log the finish line."""
    seconds = time.monotonic() - started
    report.setdefault(STAGE_DURATIONS_KEY, {})[stage] = round(seconds, 3)
    progress(f"{stage}: finished in {format_duration(seconds)} ({outcome})")
    return seconds


def run(command, source, private, stage, report, env):
    report["stage"] = stage
    log = private / (stage + ".log")
    started = time.monotonic()
    progress(f"{stage}: started")
    interaction = nullcontext(None)
    if ("--patcher-engine" in command
            and command[command.index("--patcher-engine") + 1] == "mod-packaged"
            and (len(command) > 1 and command[1] == "install"
                 or any(Path(str(arg)).name == "gui_rust.py" for arg in command[:2]))):
        import packaged_patcher_ui
        request_dir = command[command.index("--execute-request-dir") + 1]
        game_dir = command[command.index("--game-dir") + 1]
        interaction = packaged_patcher_ui.driver(request_dir, game_dir)
    try:
        with interaction as driver, log.open("xb") as output:
            process = live_output.Popen(
                command, cwd=source, env=env, stdin=subprocess.DEVNULL,
                stdout=output, stderr=subprocess.STDOUT, start_new_session=True,
            )
            try:
                if driver is None:
                    code = process.wait(timeout=7200)
                else:
                    deadline = time.monotonic() + 7200
                    while True:
                        if driver["failures"]:
                            raise Blocked("packaged_patcher_ui_failed")
                        if time.monotonic() >= deadline:
                            raise subprocess.TimeoutExpired(command, 7200)
                        try:
                            code = process.wait(timeout=0.5)
                            break
                        except subprocess.TimeoutExpired:
                            continue
            except (subprocess.TimeoutExpired, KeyboardInterrupt, Blocked):
                os.killpg(process.pid, signal.SIGINT)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                finish_stage(report, stage, started, "interrupted or timed out")
                raise Blocked("command_interrupted_or_timed_out") from None
    except RuntimeError as error:
        finish_stage(report, stage, started, "packaged installer UI failed")
        raise Blocked("packaged_patcher_ui_failed") from error
    process.check_output()
    finish_stage(report, stage, started, f"exit {code}")
    report["commands"].append({"stage": stage, "exit_code": code})
    if code:
        text = live_output.private_tail(log)
        if is_gui_stage(stage):
            reemit_gui_failure(text, private / gui_report_name(stage), report)
        else:
            lines = text.splitlines()[-FAILURE_TAIL_LINES:]
            print(f"::group::{stage} output (last {FAILURE_TAIL_LINES} lines)", file=sys.stderr)
            live_output.emit_lines("\n".join(lines))
            print("::endgroup::", file=sys.stderr)
        record_failure(text, stage, code, report, print_excerpt=True)
        raise Blocked(stage + "_failed")


ANSI_ESCAPE = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
EVIDENCE_LINES_BEFORE = 60
EVIDENCE_LINE_CAP = 200
EVIDENCE_LINE_WIDTH = 600
ERROR_LINE = re.compile(r"(?:\bError|\bException):|\[Error\]")
STOP_MARKER = re.compile(r"(?:install(?:ation)?|cli command)\s+stopped", re.IGNORECASE)
FAILURE_WORDS = re.compile(r"error|failed", re.IGNORECASE)
VALIDATE_EVIDENCE_MAX_BYTES = 128 * 1024
VALIDATE_DOWNLOAD_KINDS = frozenset(("missing-source", "missing-archive"))
VALIDATE_SECTION = re.compile(r"^(?P<severity>[A-Za-z]+) \((?P<count>\d+)\)\s*$")
VALIDATE_ERROR_TAG = re.compile(r"^\s*\[(?:patcher-error|missing-source|missing-archive)\]\s")
VALIDATE_COMPONENT_ERROR = re.compile(
    r"^\s*\[(?P<kind>patcher-error|missing-source|missing-archive)\]\s+"
    r"(?P<component>.+?): (?P=component)"
    r"(?: / (?P<action>[^\s(:]+)(?: \((?P<detail>.*?)\))?)?: (?P<message>.*?)\s*$")
COMPONENT_FAILURE = re.compile(
    r"(?:Error|Exception): (?:Install stopped\. Restore before retrying; snapshot: .+?: )?"
    r"(?P<component>.+?) / step (?P<step>\d+) "
    r"\((?P<action>[^)]*)\): (?P<message>.*?)\s*$")
TS_LOG_PREFIX = r"(?:\d{4}-\d\d-\d\dT[\d:.]+Z\s+)?"
TS_MOD_PROGRESS = re.compile(
    r"^" + TS_LOG_PREFIX + r"\[(?P<index>\d+)/(?P<total>\d+)\]\s+(?P<name>\S.*?)\s*$")
TS_STEP_PROGRESS = re.compile(r"^" + TS_LOG_PREFIX + r"\s+(?P<step>\d+)\.\s+(?P<action>[A-Za-z]+):")
TS_INSTRUCTION_FAILURE = re.compile(
    r"\bERROR: Instruction (?P<guid>[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12})"
    r" failed \((?P<code>[^)]*)\)(?:; installation stopped with recovery pending)?(?:: (?P<message>.*?))?\s*$")
TS_CHECKPOINT_ID = re.compile(r"\[checkpoint (?P<id>[0-9A-Za-z-]+)\]")
TS_CHECKPOINT_SESSION = re.compile(r"^" + TS_LOG_PREFIX + r"Checkpoint session: (?P<id>[0-9A-Za-z-]+)\s*$")
SECRET_PATTERNS = (
    (re.compile(r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,}"), r"\1 ***"),
    (re.compile(r"(?i)\b([\w-]*(?:token|secret|passw(?:or)?d|api[_-]?key|access[_-]?key|authorization"
                r"|credential|signature)[\w-]*)\s*[=:]\s*\S+"), r"\1=***"),
    (re.compile(r"://[^/\s:@]+:[^/\s@]+@"), "://***@"),
    (re.compile(r"(?i)([?&](?:token|key|sig|signature|x-amz-[a-z-]+)=)[^&\s]+"), r"\1***"),
    (re.compile(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b"), "***"),
)


def clean_line(line):
    """One log line without terminal escapes, secrets, or text a CI runner would read as a command."""
    line = ANSI_ESCAPE.sub("", line).rstrip()
    for pattern, replacement in SECRET_PATTERNS:
        line = pattern.sub(replacement, line)
    if len(line) > EVIDENCE_LINE_WIDTH:
        line = line[:EVIDENCE_LINE_WIDTH] + " ...[truncated]"
    if line.startswith(("::", "##[")):
        line = " " + line
    return line


def failure_excerpt(text, before=EVIDENCE_LINES_BEFORE, cap=EVIDENCE_LINE_CAP):
    """A bounded excerpt of a failed tool's output: its Error/Exception lines with context.

    Keeps each Error:/Exception: line with up to `before` lines ahead of it and, after the first
    install-stopped marker (kept too), every line that mentions an error or a failure. At most `cap` lines
    (the last ones win); gaps are marked and the result is secret-scrubbed.
    """
    lines = [ANSI_ESCAPE.sub("", line) for line in text.splitlines()]
    keep = set()
    for index, line in enumerate(lines):
        if ERROR_LINE.search(line):
            keep.update(range(max(0, index - before), index + 1))
    marker = next((index for index, line in enumerate(lines) if STOP_MARKER.search(line)), None)
    if marker is not None:
        keep.add(marker)
        keep.update(index for index in range(marker, len(lines)) if FAILURE_WORDS.search(lines[index]))
    ordered = sorted(keep)
    omitted = max(0, len(ordered) - cap)
    ordered = ordered[omitted:]
    excerpt = []
    if omitted:
        excerpt.append(f"[{omitted} earlier excerpt line(s) omitted]")
    previous = None
    for index in ordered:
        if previous is not None and index != previous + 1:
            excerpt.append("[...]")
        excerpt.append(clean_line(lines[index]))
        previous = index
    return excerpt


def component_failure_fields(match):
    return {"component": match.group("component").strip(), "step": int(match.group("step")),
            "action": match.group("action").strip(),
            "message": clean_line(match.group("message")),
            "error_line": clean_line(match.group(0))}


def parse_typescript_install_failures(text):
    """Every `ERROR: Instruction <guid> failed (<Code>)...` line of a TypeScript CLI install log.

    The TypeScript CLI names the failing instruction by GUID, not the component. The component and
    step are the ones in force on that line: the last `[i/N] <Component>` progress line and the last
    `  N. <Action>: ...` step line after it. `component` is None when no progress line precedes the
    error; `action` is "" when no step line follows the progress line. A line repeated counts once.
    """
    found = []
    seen = set()
    component = step = action = None
    for raw in text.splitlines():
        line = ANSI_ESCAPE.sub("", raw).rstrip()
        progress_line = TS_MOD_PROGRESS.match(line)
        if progress_line:
            component, step, action = progress_line.group("name").strip(), None, ""
            continue
        step_line = TS_STEP_PROGRESS.match(line)
        if step_line:
            step, action = int(step_line.group("step")), step_line.group("action")
            continue
        match = TS_INSTRUCTION_FAILURE.search(line)
        if match is None:
            continue
        fields = {"component": component, "step": step, "action": action or "",
                  "message": clean_line(match.group("message") or match.group("code")),
                  "error_line": clean_line(match.group(0)),
                  "instruction_guid": normalized_reference(match.group("guid")),
                  "result_code": match.group("code")}
        key = (fields["instruction_guid"], fields["component"], fields["message"])
        if key not in seen:
            seen.add(key)
            found.append(fields)
    return found


def parse_component_failures(text):
    """Every distinct `Error: <Component> / step N (<Action>): <message>` line of a tool's output.

    A line repeated (the log and the final error) counts once, at its last position. A log with no
    such line is read as a TypeScript CLI install log (`parse_typescript_install_failures`).
    """
    found = []
    seen = set()
    for line in text.splitlines():
        match = COMPONENT_FAILURE.search(ANSI_ESCAPE.sub("", line))
        if match:
            fields = component_failure_fields(match)
            key = (fields["component"], fields["step"], fields["action"], fields["message"])
            if key in seen:
                found = [item for item in found
                         if (item["component"], item["step"], item["action"], item["message"]) != key]
            seen.add(key)
            found.append(fields)
    return found or [item for item in parse_typescript_install_failures(text) if item["component"]]


def parse_component_failure(text):
    """The last `Error: <Component> / step N (<Action>): <message>` line of a tool's output, or None.

    Falls back to the last TypeScript `ERROR: Instruction <guid> failed` line that has a progress line.
    """
    found = None
    for line in text.splitlines():
        match = COMPONENT_FAILURE.search(ANSI_ESCAPE.sub("", line))
        if match:
            found = match
    if found is not None:
        return component_failure_fields(found)
    attributed = [item for item in parse_typescript_install_failures(text) if item["component"]]
    return attributed[-1] if attributed else None


def validate_error_lines(text):
    """Every error-severity line of a TypeScript `validate` report, in order.

    The report groups lines under `ERROR (n)` / `INFO (n)` headings: everything under an ERROR
    heading is an error line, and a tagged line (`[patcher-error]`, `[missing-source]`,
    `[missing-archive]`) is one wherever no other severity heading is in force. The `FAILED:`
    trailer closes the section. Nothing is dropped by count, only by the caller's byte cap.
    """
    lines = []
    severity = None
    for raw in text.splitlines():
        line = ANSI_ESCAPE.sub("", raw).rstrip()
        section = VALIDATE_SECTION.match(line)
        if section:
            severity = section.group("severity").upper()
            continue
        if line.startswith("FAILED"):
            severity = None
            continue
        if not line.strip():
            continue
        if VALIDATE_ERROR_TAG.match(line):
            if severity is None or severity.startswith("ERROR"):
                lines.append(line)
        elif severity is not None and severity.startswith("ERROR"):
            lines.append(line)
    return lines


def validate_error_excerpt(text, max_bytes=VALIDATE_EVIDENCE_MAX_BYTES):
    """All validate error-severity lines, scrubbed, capped by bytes (not by line count)."""
    lines = [clean_line(line) for line in validate_error_lines(text)]
    kept = []
    used = 0
    for index, line in enumerate(lines):
        size = len(line.encode("utf-8")) + 1
        if used + size > max_bytes:
            kept.append(f"[{len(lines) - index} further error line(s) omitted: {max_bytes} byte evidence cap]")
            break
        kept.append(line)
        used += size
    return kept


def parse_validate_errors(text):
    """(failures, unattributed) of a validate report's error lines.

    A failure is a `[kind] <Component>: <Component> / <Action> (<detail>): <message>` line; its step
    is None. An error line of any other shape (a global or ambiguous error) is returned as-is
    in `unattributed`.
    """
    failures = []
    unattributed = []
    for line in validate_error_lines(text):
        match = VALIDATE_COMPONENT_ERROR.match(line)
        if match is None:
            unattributed.append(clean_line(line))
            continue
        failures.append({
            "component": match.group("component").strip(), "step": None,
            "action": match.group("action"), "kind": match.group("kind"),
            "message": clean_line(match.group("message")),
            "error_line": clean_line(line.strip())})
    return failures, unattributed


def distinct_failures(failures):
    """The first failure of each distinct component name (case-insensitive), in order."""
    found = {}
    for item in failures:
        found.setdefault(item["component"].casefold(), item)
    return list(found.values())


def failure_location(failure):
    """Where a recorded failure happened, for the CI log."""
    if failure.get("component"):
        step = f", step {failure['step']}" if failure.get("step") is not None else ""
        action = f" ({failure['action']})" if failure.get("action") else ""
        return f"component '{failure['component']}'{step}{action}"
    names = failure.get("components")
    if names:
        shown = ", ".join(f"'{clean_line(name)}'" for name in names[:5])
        more = f" and {len(names) - 5} more" if len(names) > 5 else ""
        return f"{len(names)} component(s) attributed ({shown}{more})"
    return "no component could be attributed"


def record_failure(text, stage, code, report, print_excerpt=False):
    """Write the failing component/step into the lane report and, optionally, the excerpt into the CI log."""
    failure = {"stage": stage, "exit_code": code, "component": None, "step": None,
               "action": None, "error_line": None}
    parsed = parse_component_failure(text)
    tagged, unattributed = parse_validate_errors(text) if stage.startswith("validate") else ([], [])
    if parsed:
        failure.update(parsed)
    if tagged:
        names = [item["component"] for item in distinct_failures(tagged)]
        failure["components"] = names
        failure["error_line_count"] = len(tagged) + len(unattributed)
        if not parsed and len(names) == 1:
            first = tagged[0]
            failure.update({"component": first["component"], "action": first["action"],
                            "error_line": first["error_line"]})
    report["failure"] = failure
    if print_excerpt:
        excerpt = failure_excerpt(text)
        if stage.startswith("validate"):
            excerpt += validate_error_excerpt(text)
        print(f"::group::{stage} failure evidence (bounded excerpt, {len(excerpt)} lines)",
              file=sys.stderr)
        live_output.emit_lines("\n".join(excerpt) if excerpt
              else "(no Error/Exception line or install-stopped marker in the output)")
        print("::endgroup::", file=sys.stderr)
        if parsed:
            print(f"ports: {stage} failed at component '{parsed['component']}', step {parsed['step']} "
                  f"({parsed['action']})", file=sys.stderr)
        elif failure.get("component") or failure.get("components"):
            print(f"ports: {stage} failed at {failure_location(failure)}", file=sys.stderr)
    return failure


SKIP_CAUSES = (
    ("already_had_instructions", ("already has instructions",)),
    ("no_mod_links", ("no mod links",)),
    ("mapped_file_missing", ("was not found in the source directory",)),
    ("no_archive_matched", ("no matching archive",)),
    ("patcher_path_refusal", ("patcher tree", "installer", "namespace")),
    ("reader_rejection", ("no supported", "exceeds", "unsafe", "does not exist",
                          "no extension", "not utf-8", "invalid utf-8", "non-utf8",
                          "stream did not contain valid utf-8",
                          "archive links and special files")),
)
POLICY_CAUSE = "skipped_by_reviewed_policy"


def skip_cause(reason):
    lowered = reason.lower()
    for cause, needles in SKIP_CAUSES:
        if any(needle in lowered for needle in needles):
            return cause
    return "other"


def record_convert_skips(log, report):
    """Keep the converter's per-component skip report (a JSON array on stderr, merged into the log)."""
    try:
        text = live_output.private_tail(log)
    except OSError:
        return
    decoder = json.JSONDecoder()
    for start in (match.start() for match in re.finditer(r"^\[$", text, re.MULTILINE)):
        try:
            results, _ = decoder.raw_decode(text, start)
        except ValueError:
            continue
        if (isinstance(results, list) and results
                and all(isinstance(item, dict) and "ComponentGuid" in item for item in results)):
            break
    else:
        return
    skipped = [{"component_guid": str(item["ComponentGuid"]).lower(),
                "name": item.get("ComponentName"),
                "cause": skip_cause(str(item.get("SkipReason") or "")),
                "reason": item.get("SkipReason")}
               for item in results if item.get("Success") is not True]
    causes = {}
    for item in skipped:
        causes[item["cause"]] = causes.get(item["cause"], 0) + 1
    report["convert_skips"] = {
        "total": len(results), "generated": len(results) - len(skipped), "by_cause": causes,
        "components": [item for item in skipped if item["cause"] != "already_had_instructions"]}


REVIEW_FLAG = ("DRAFT INSTRUCTIONS: parsed from guide prose by the natural-language importer. "
               "Review before installing - never auto-trusted.")
DRAFT_REVIEW_STAGE = "review_verified_drafts"


def record_draft_review(log, report):
    """Keep the converter's {cleared, skipped} report (a JSON object on stderr, merged into the log)."""
    try:
        text = live_output.private_tail(log)
    except OSError:
        return
    decoder = json.JSONDecoder()
    for start in (match.start() for match in re.finditer(r"^\{$", text, re.MULTILINE)):
        try:
            result, _ = decoder.raw_decode(text, start)
        except ValueError:
            continue
        if (isinstance(result, dict) and isinstance(result.get("cleared"), list)
                and isinstance(result.get("skipped"), list)
                and all(isinstance(item, dict) and "guid" in item for item in result["skipped"])):
            break
    else:
        return
    report.setdefault("draft_review", {}).update(
        cleared=len(result["cleared"]),
        skipped=[{"component_guid": str(item["guid"]).lower(),
                  "name": item.get("name"), "reason": item.get("reason")}
                 for item in result["skipped"]])


BIND_STAGE = "bind_archives"
BIND_GENERATE_STAGE = "bind_generate_local"
BIND_GENERATOR = "resolved-filenames"
MOVIE_BIND_GENERATOR = "resolved-movie-archives"
VENDOR_BIND_GENERATOR = "resolved-vendor-installer"
MANUAL_BIND_GENERATOR = "reviewed-manual-file-effects"
MANUAL_PATCHER_GENERATOR = "reviewed-manual-native-patcher"
MANUAL_BOOK_SHA256 = "f85240122267d088f7b52c1746c2edb2fdb8795846889f4158dcf59b4fd91d60"
MANUAL_BOOKS = {
    "k1": [("reviewed_manual_k1.json", "b04b7331b5001f81698feb5571460ea9e2e486d032956abb22967d704254c338", 66),
           ("reviewed_manual_raw_inputs_k1.json", "f3dadc5ad04f7c501753ca38bf50492e067c54b8ed1df401910780d3ca836d93", 2),
           ("reviewed_manual_community_patch_k1.json", "923c0a5ebe52615247717bd3bab6ce48bad98cc664825a840465025b2b7e1fbc", 1)],
    "k2": [("reviewed_manual_k2.json", MANUAL_BOOK_SHA256, 57),
           ("reviewed_manual_single_ini_k2.json", "2eee9ca0321722919306743903b18a872c1be1954c547963b3a737a270679fbe", 4),
           ("reviewed_manual_baodur_k2.json", "f71e9fbf5c3e0bcf300d604c453b44fcdb9f98c8ff82c6f5f568a06a07fad577", 1),
           ("reviewed_manual_log_delete_k2.json", "b4850e572859cf4c22616be584a0ca02411fb2ddcd3ae4f23fcb05d7a97af13a", 2),
           ("reviewed_manual_installer_choice_k2.json", "e50408f7108f9c0a6b1829d9cfa70b1004721de6f9162437271aa46e59df9b9b", 2),
           ("reviewed_manual_community_patch_k2.json", "c8f3877165f3da1adc8d5d76cc86ef8e803cc5d2a012aa22ef1f1fb25723db74", 1)],
}
BIND_SUFFIXES = (".zip", ".7z", ".rar")
BIND_UNSAFE_NAME = re.compile(r"[/\\*?\[\]\x00-\x1f\x7f]")
INTERRUPTED = "command_interrupted_or_timed_out"


def canonical_link(url):
    """Guide link as cache.py keys it (cache.link_key): query, fragment and userinfo dropped, host lowercased;
    a mega link identified only by its fragment keeps a digest of it."""
    return link_key(url)


def mapped_archives(path):
    """archive_map.json as {canonical link: file name}; a link with several or conflicting files maps to None."""
    mapping = {}
    for link, value in json.loads(path.read_text(encoding="utf-8")).items():
        if isinstance(value, list):
            value = value[0] if len(value) == 1 else None
        name = value if isinstance(value, str) else None
        key = canonical_link(link)
        mapping[key] = name if mapping.get(key, name) == name else None
    return mapping


def components_of(path):
    document = tomllib.loads(path.read_text(encoding="utf-8"))
    return [component for component in document.get("thisMod", document.get("Components", []))
            if isinstance(component, dict)]


def component_archive(component, mapping):
    """The one archive file all of a component's links map to; None if unmapped, partly mapped or several."""
    registry = component.get("ResourceRegistry")
    names = set()
    for link in registry if isinstance(registry, dict) else ():
        names.add(mapping.get(canonical_link(link)))
    return names.pop() if len(names) == 1 and None not in names else None


def alias_key(text):
    """A heading or name as alias rules compare it: whitespace collapsed, case-insensitive; None if blank."""
    folded = " ".join(text.split()).casefold() if isinstance(text, str) else ""
    return folded or None


def component_alias_keys(component):
    """The normalized guide heading and component Name of a converted component, each once.

    The Rust converter writes `Heading` only when the guide `###` heading differs from the Name
    field, so a component can be known by either one; a reviewed rule may be keyed by both.
    """
    keys = []
    for field in ("Heading", "Name"):
        key = alias_key(component.get(field))
        if key is not None and key not in keys:
            keys.append(key)
    return keys


def declared_archive(component, counts, rules, archives):
    """The one file of the reviewed `archive_aliases` rule for this component, else None.

    The Rust converter derives GUIDs differently from the TypeScript one, so the rule (keyed by
    the TypeScript GUID) is found by the component's guide heading or its Name field, normalized,
    and exactly one rule must match. Each of the component's keys that a rule names must identify
    exactly one component (`counts`). Only a single-file rule for a component without
    instructions applies, so the binding keeps its sha256/size check; the file must be a restored
    archive.
    """
    matched = {}
    for key in component_alias_keys(component):
        for rule in rules.get(key, ()):
            matched[rule["guid"]] = rule
            if counts.get(key) != 1:
                return None
    if len(matched) != 1:
        return None
    rule = next(iter(matched.values()))
    if (component.get("Instructions") or len(rule["files"]) != 1
            or alias_file_problem(archives, rule["files"][0])):
        return None
    return rule["files"][0]


def load_reviewed_manual_recipes(source, game="k2"):
    if game not in MANUAL_BOOKS:
        raise Blocked("reviewed_manual_databook_game_unknown")
    recipes = {}
    for filename, checksum, count in MANUAL_BOOKS[game]:
        path = source / "src/archive_generation" / filename
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 16 * 1024 * 1024:
            raise Blocked("reviewed_manual_databook_missing_or_unsafe")
        if digest(path) != checksum:
            raise Blocked("reviewed_manual_databook_identity_changed")
        book = json.loads(path.read_text(encoding="utf-8"))
        if book.get("schema") != 1 or len(book.get("recipes", [])) != count:
            raise Blocked("reviewed_manual_databook_scope_changed")
        for recipe in book["recipes"]:
            if recipe["component_guid"] in recipes:
                raise Blocked("reviewed_manual_databook_duplicate_component")
            recipes[recipe["component_guid"]] = recipe
    return recipes


def mapped_archive_groups(path):
    groups = {}
    for link, value in json.loads(path.read_text(encoding="utf-8")).items():
        names = [value] if isinstance(value, str) else value
        names = tuple(names) if isinstance(names, list) and all(
            isinstance(name, str) and name for name in names) else ()
        key = canonical_link(link)
        groups[key] = names if groups.get(key, names) == names else ()
    return groups


def load_source_plan_archive_pins(source, game):
    """Transport exact admitted filenames; native derivation remains the authority."""
    if game not in ("k1", "k2"):
        return {}
    authority_path = source / "src/archive_generation/source_plan_patcher_authority.json"
    if (authority_path.is_symlink() or not authority_path.is_file()
            or authority_path.stat().st_size > 256 * 1024
            or digest(authority_path) != "88aac6db9661452a9a64e230ec4e795131117dd9a3d246262323214e2fa26700"):
        raise Blocked("source_plan_archive_mapping_identity_changed")
    authority = json.loads(authority_path.read_text(encoding="utf-8"))
    rows = authority.get("rows", [])
    if len(rows) != 46 or sum(row.get("disposition") == "admitted" for row in rows) != 40:
        raise Blocked("source_plan_archive_mapping_scope_changed")
    expected = authority["sources"][game]
    plan_path = source / "tools/modsync-ci/fixtures/ci" / (game + "-manual-plan.json")
    if (plan_path.is_symlink() or not plan_path.is_file()
            or plan_path.stat().st_size > 8 * 1024 * 1024
            or digest(plan_path) != expected["plan_sha256"]):
        raise Blocked("source_plan_archive_plan_identity_changed")
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    if plan.get("guide_sha256") != expected["guide_sha256"]:
        raise Blocked("source_plan_archive_guide_identity_changed")
    pins = {}
    for row in rows:
        if row["game"] != game or row["disposition"] != "admitted":
            continue
        step = plan["steps"][row["step_index"]]
        operations = step.get("operations", [])
        if (step.get("id") != row["step_id"] or step.get("source_heading") != row["heading"]
                or len(operations) != 2 or operations[0].get("type") != "extract"):
            raise Blocked("source_plan_archive_step_identity_changed")
        archive_source = operations[0].get("source", "")
        filename = archive_source.removeprefix("{archives}/")
        checksum = plan["archive_sha256"].get(archive_source)
        if (not archive_source.startswith("{archives}/") or not 0 < len(filename) <= 255
                or filename in (".", "..") or re.search(r"[/\\*?\x00-\x1f\x7f]", filename)
                or not filename.lower().endswith(BIND_SUFFIXES)
                or not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum)
                or row["guid"] in pins):
            raise Blocked("source_plan_archive_pin_invalid")
        pins[row["guid"]] = (filename, checksum)
    return pins


SOURCE_SUPPLEMENTAL_SHA256 = "92fad6add3641d53416effbf8a1360c51fb9b64fc340273c2baa33b06672ee74"


def source_context_equal(actual, expected):
    """Compare the complete pinned owner, accepting omitted empty serialization defaults."""
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or set(actual) - set(expected):
            return False
        return all(source_context_equal(actual[key], value) if key in actual
                   else (value is None or value == "" or value == [] or value == {}
                         or value is False)
                   for key, value in expected.items())
    if isinstance(expected, list):
        return (isinstance(actual, list) and len(actual) == len(expected)
                and all(source_context_equal(a, e) for a, e in zip(actual, expected)))
    return type(actual) is type(expected) and actual == expected


def load_source_program_groups(source, game, converted):
    """Closed filename/hash transport from full pinned plans and supplemental choices.

    This grants no native selection, instruction approval or installation authority:
    Rust reconstructs original context and authenticates every actual input again.
    """
    if game not in ("k1", "k2"):
        return {}
    # Keep the original forty-owner authority and both source hash pairs anchored.
    load_source_plan_archive_pins(source, game)
    authority = json.loads((source / "src/archive_generation/source_plan_patcher_authority.json")
                           .read_text(encoding="utf-8"))
    expected = authority["sources"][game]
    guide_path = source / "src/archive_generation" / ("source_plan_" + game + ".md")
    if (guide_path.is_symlink() or not guide_path.is_file()
            or digest(guide_path) != expected["guide_sha256"]):
        raise Blocked("source_program_guide_identity_changed")
    plan = json.loads((source / "tools/modsync-ci/fixtures/ci" / (game + "-manual-plan.json"))
                      .read_text(encoding="utf-8"))
    components = components_of(converted)
    count = 197 if game == "k1" else 151
    if (len(components) != count or len({c.get("Guid") for c in components}) != count
            or len(plan["steps"]) > count):
        raise Blocked("source_program_complete_owner_inventory_changed")
    groups = {}
    for index, step in enumerate(plan["steps"]):
        component = components[index]
        heading = component.get("Heading") or component.get("Name")
        if (not isinstance(heading, str) or step.get("id") != f"step-{index + 1:03}"
                or step.get("source_heading") != "### " + heading):
            raise Blocked("source_program_ordered_owner_heading_changed")
        operations = step.get("operations", [])
        native = any(operation.get("type") in ("patcher", "gui_patcher")
                     for operation in operations)
        file_owner = index in ({40, 149} if game == "k1" else {94, 135})
        if not (native or file_owner) or component.get("Guid") == "bee5a72b-d748-8f83-8261-81758a9bfc56":
            continue
        pins = {}
        for operation in step.get("operations", []):
            archive_source = operation.get("source", "")
            if not isinstance(archive_source, str) or not archive_source.startswith("{archives}/"):
                continue
            filename = archive_source.removeprefix("{archives}/")
            checksum = plan["archive_sha256"].get(archive_source)
            if (not 0 < len(filename) <= 255 or filename in (".", "..")
                    or re.search(r"[/\\*?\x00-\x1f\x7f]", filename)
                    or not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum)):
                raise Blocked("source_program_input_pin_invalid")
            if filename in pins and pins[filename] != checksum:
                raise Blocked("source_program_input_pin_conflict")
            pins[filename] = checksum
        if pins:
            groups[component["Guid"]] = [{"filename": name, "sha256": checksum}
                                         for name, checksum in pins.items()]
    supplemental_path = source / "src/archive_generation/source_plan_namespace/supplemental_authority.json"
    if (supplemental_path.is_symlink() or not supplemental_path.is_file()
            or supplemental_path.stat().st_size > 8 * 1024 * 1024
            or digest(supplemental_path) != SOURCE_SUPPLEMENTAL_SHA256):
        raise Blocked("source_program_supplemental_identity_changed")
    supplemental = json.loads(supplemental_path.read_text(encoding="utf-8"))
    if (supplemental.get("schema") != 1 or supplemental.get("policy") != "closed-supplemental-source-choices-v2"
            or supplemental.get("source_kind") != "authored-supplemental"
            or supplemental.get("original_manual_plan_steps") is not False
            or len(supplemental.get("rows", [])) != 11
            or supplemental["source_guide_sha256"][game] != expected["guide_sha256"]):
        raise Blocked("source_program_supplemental_scope_changed")
    for row in supplemental["rows"]:
        if row["game"] != game:
            continue
        component = components[row["index"]]
        raw = row["raw_component"]
        if (row["guid"] != component.get("Guid") or raw.get("Guid") != row["guid"]
                or raw.get("Name") != row["name"] or row["source_kind"] != "authored-supplemental"
                # Initial guide conversion emits the authenticated raw draft.
                # Reloading that model canonicalizes instruction Source paths.
                # Both complete representations are pinned in this authority.
                or not any(source_context_equal(component, row[representation])
                           for representation in ("raw_component", "canonical_component"))):
            raise Blocked("source_program_supplemental_owner_context_changed")
        groups[row["guid"]] = [{"filename": pin["filename"], "sha256": pin["sha256"],
                                "source_url": pin["source_url"]}
                               for pin in row["inputs"] if pin["active"]]
    return groups


# Original pinned plans explicitly consume these five compatibility inputs.
# The four Mega keys are one-way digests of the canonical archive-map keys;
# no share/decryption fragment is embedded here. Metadata provenance is the
# retained run 43710676465 and independently matching historical run 43638810389.
SOURCE_COMPATIBILITY_DONORS = {
    ('07416fb0-1f9b-88ba-952f-4029575ad036', 'Vurt-Yavin Compatch.7z', 'df398a85af50791954e11986c90583a145a2e7a00cd47efa30e07f15f69dc1b8'): {
        'provider_url': 'https://mega.nz/file/QAhhFTzD',
        'source_id': '8fee01d1de2105d1d4cdd442e4b736515ff6da5b42d641bbc8e5bb12f0021065',
        'archive_map_key_sha256': 'cae11643464680b67eefac82b9acf8bd2e94d19655f6641ee093fe4210febdd3',
        'donor_guid': None,
        'provenance': 'explicit-original-pinned-plan-compatibility-input',
    },
    ('83226d49-f919-82a5-923e-c1d00f727c40', 'SAWL Patch.rar', '681ff0da13422831fb006d810fef5733473c866900589aadbce049cc1f58bdcd'): {
        'provider_url': 'https://mega.nz/',
        'source_id': 'b406fedc0738bd8df09ae3d59ac25ca6ffca222590f2856b3dcba033f03e318b',
        'archive_map_key_sha256': '7e34cad6b1f05e3493269eb7bff416ca4dd36b3e58ad84360abd1ac50e0796ae',
        'donor_guid': None,
        'provenance': 'explicit-original-pinned-plan-compatibility-input',
    },
    ('7696c38c-9a45-8510-bda2-328f29eab2c1', 'Czerka Business Attire - Dark Hope Ithorian Compatch.7z', '72ff641a033230673e2f6bf38c8bbae9e9b49c86108364d2eb375ed07db9c68e'): {
        'provider_url': 'https://mega.nz/file/wUYRUTAD',
        'source_id': '6d3324683057692bb2e726f7054cc39e717b9275900484a417014351f22bf90a',
        'archive_map_key_sha256': '0101ac0d1afcdd0c8742fdfba5ef07a3d5d7a6fe322eebc8e331c53ce8d4f252',
        'donor_guid': None,
        'provenance': 'explicit-original-pinned-plan-compatibility-input',
    },
    ('351de0e9-d36f-8bb2-ba08-003d9123e25a', 'K2CP_v1.6.3.zip', '6e734ed7e81ad8169d12870ce9fcf829bd56b88f22e7ea20fb37fcb85b8580c1'): {
        'provider_url': 'https://deadlystream.com/files/file/1280-kotor-2-community-patch/',
        'source_id': '78025cdaf6b1e9ac10062ed95795a58ddb581e7c6fe01772203086814d81fac7',
        'archive_map_key_sha256': '78025cdaf6b1e9ac10062ed95795a58ddb581e7c6fe01772203086814d81fac7',
        'donor_guid': '216eb8b6-3076-8a92-bd7d-012862b10b32',
        'provenance': 'selected-donor-original-guide-owner',
    },
    ('8f97eab8-9ee5-859f-a86f-bb4d0476c34b', 'FS_Fit_Handmaiden Patch.rar', 'dc294dbdfbdf83718fd720a84ebab27a8804f075ce41723faacf2ea57d52b03b'): {
        'provider_url': 'https://mega.nz/',
        'source_id': '517af3e204370d15bf0b5dc29e7ad187a2c7920ec2d0103b5dc82a78dde68f57',
        'archive_map_key_sha256': '0af188c99c157bf4439fd5a395e7aeea1609777618aca3ef88f47b72f3e682b2',
        'donor_guid': None,
        'provenance': 'explicit-original-pinned-plan-compatibility-input',
    },
}


def compiled_compatibility_source_association(component, pin, components, archive_groups):
    """An exact compiled cross-source input, never a global filename fallback."""
    donor = SOURCE_COMPATIBILITY_DONORS.get(
        (component.get("Guid"), pin["filename"], pin["sha256"]))
    if donor is None or component.get("IsSelected", True) is not True:
        return False
    if donor["donor_guid"] is not None:
        selected = [owner for owner in components if owner.get("Guid") == donor["donor_guid"]
                    and owner.get("IsSelected", True) is True
                    and canonical_link(donor["provider_url"]) in {
                        canonical_link(link) for link in owner.get("ResourceRegistry", {})}]
        if len(selected) != 1:
            return False
    return any(hashlib.sha256(canonical_link(link).encode("utf-8")).hexdigest()
               == donor["archive_map_key_sha256"] and pin["filename"] in names
               for link, names in (archive_groups or {}).items())


def resolved_archive_bindings(converted, mapping, archives, rules=None, vendor_languages=None,
                              reviewed_recipes=None, archive_groups=None, source_plan_pins=None,
                              source_program_groups=None, excluded_guids=()):
    """--resolved-archives entries for components whose links map to exactly one archive present in `archives`.

    A component the map cannot resolve falls back to its reviewed `archive_aliases` rule
    (`load_archive_aliases`), matched by heading or Name (`declared_archive`).
    """
    components = components_of(converted)
    seen, counts = {}, {}
    for component in components:
        seen[component.get("Guid")] = seen.get(component.get("Guid"), 0) + 1
        for key in component_alias_keys(component):
            counts[key] = counts.get(key, 0) + 1
    by_key = {}
    for rule in (rules or {}).values():
        by_key.setdefault(alias_key(rule["heading"]), []).append(rule)
    bindings = []
    for component in components:
        guid, name = component.get("Guid"), component_archive(component, mapping)
        if guid in excluded_guids:
            continue
        recipe = (reviewed_recipes or {}).get(guid)
        if recipe is not None and seen.get(guid) == 1:
            pins = recipe["archives"]
            selected = {filename for link in component.get("ResourceRegistry", {})
                        for filename in (archive_groups or {}).get(canonical_link(link), ())}
            names = [pin["filename"] for pin in pins]
            if len(set(names)) != len(names) or not set(names) <= selected:
                continue
            group = []
            for pin in pins:
                filename = pin["filename"]
                if (not 0 < len(filename) <= 255
                        or re.search(r"[/\\*?\x00-\x1f\x7f]", filename)
                        or (not filename.lower().endswith(BIND_SUFFIXES)
                            and (filename, pin["sha256"]) not in {
                                ("N_DarthMalak01.tga", "04160ed9e2eef2c88a3ed644ec2be24e5cd765f67a7cca35106be062e20358cd"),
                                ("tar02_duelorg021.dlg", "6ebb0b909c4ac7496971e3cc1320ce14b04fc67aec167f0feeb0e332181aff57")})):
                    break
                path = archives / filename
                if path.is_symlink() or not path.is_file() or digest(path) != pin["sha256"]:
                    break
                group.append(filename)
            if len(group) == len(pins):
                binding = {"componentGuid": guid, "filenames": group}
                if len(group) == 1:
                    binding.update(sha256=pins[0]["sha256"], sizeBytes=(archives / group[0]).stat().st_size)
                bindings.append(binding)
            continue
        program_pins = (source_program_groups or {}).get(guid)
        # Preserve the separately reviewed vendor and exact revision routes.
        if program_pins is not None and guid not in {
                "bee5a72b-d748-8f83-8261-81758a9bfc56", "14d77038-49c0-882b-83a1-0a424efa0f60",
                "ed488679-bc04-892d-973b-107511862024", "e015887a-be0f-86d1-830e-92d82c8fc46c"}:
            if seen.get(guid) != 1 or not program_pins:
                continue
            registry = component.get("ResourceRegistry", {})
            selected = {filename for link in registry
                        for filename in (archive_groups or {}).get(canonical_link(link), ())}
            names = [pin["filename"] for pin in program_pins]
            if (len(set(names)) != len(names) or not all(
                    pin["filename"] in selected or compiled_compatibility_source_association(
                        component, pin, components, archive_groups) for pin in program_pins)):
                continue
            group = []
            for pin in program_pins:
                filename = pin["filename"]
                # Literal brackets are permitted only inside this exact compiled
                # filename/hash group; the generic filename policy is unchanged.
                if (not 0 < len(filename) <= 255 or filename in (".", "..")
                        or re.search(r"[/\\*?\x00-\x1f\x7f]", filename)
                        or not filename.lower().endswith(BIND_SUFFIXES)):
                    break
                if ("source_url" in pin and filename not in (archive_groups or {}).get(
                        canonical_link(pin["source_url"]), ())):
                    break
                path = archives / filename
                if path.is_symlink() or not path.is_file():
                    break
                with path.open("rb") as handle:
                    actual_sha256 = hashlib.file_digest(handle, "sha256").hexdigest()
                if actual_sha256 != pin["sha256"]:
                    raise Blocked("source_program_archive_identity_changed")
                group.append(filename)
            if len(group) == len(program_pins):
                binding = {"componentGuid": guid, "filenames": group}
                if len(group) == 1:
                    binding.update(sha256=program_pins[0]["sha256"],
                                   sizeBytes=(archives / group[0]).stat().st_size)
                bindings.append(binding)
            continue
        if name is None and by_key:
            name = declared_archive(component, counts, by_key, archives)
        vendor = (guid == "bee5a72b-d748-8f83-8261-81758a9bfc56"
                  and name == "tslrcm2022.exe"
                  and (vendor_languages or {}).get(guid) == "english")
        source_plan_pin = (source_plan_pins or {}).get(guid)
        literal_brackets = (source_plan_pin is not None and name == source_plan_pin[0]
                            and isinstance(name, str) and ("[" in name or "]" in name)
                            and not re.search(r"[/\\*?\x00-\x1f\x7f]", name))
        if (not isinstance(guid, str) or not guid or seen[guid] != 1 or name is None
                or not 0 < len(name) <= 255 or name in (".", "..")
                or (BIND_UNSAFE_NAME.search(name) and not literal_brackets)
                or not (name.lower().endswith(BIND_SUFFIXES) or vendor)):
            continue
        path = archives / name
        if path.is_symlink() or not path.is_file():
            continue
        with path.open("rb") as handle:
            sha256 = hashlib.file_digest(handle, "sha256").hexdigest()
        # This publisher reused its filename for the separately reviewed 1.0.2
        # package. Transport either explicit body identity; Rust independently
        # selects and replays the original or revision authority from fresh bytes.
        if guid == "14d77038-49c0-882b-83a1-0a424efa0f60" and (name, sha256) not in {
                ("Trask Ulgo Without Tutorials.7z",
                 "b9e4b5f4934594400cbc4873cbba12b78444174d5239b92f0049d746f31207cc"),
                ("Trask Ulgo Without Tutorials.7z",
                 "39d1da3da9476dafaa86e2568adae522ee95686b9adb51bff5e5d98c9c9e83ee")}:
            raise Blocked("trask_source_revision_archive_identity_changed")
        # The public revision has a distinct container name and body identity.
        # It never aliases the historical ZIP pin; Rust selects the corresponding
        # original or separately authored revision program from fresh bytes.
        if guid == "ed488679-bc04-892d-973b-107511862024" and (name, sha256) not in {
                ("Heyorange_s_Sith Uniform_Reformation_Revised.zip",
                 "0ac82a9a19b137e58d3ed2f3fcc28e2beab40dce09214f852d520b9f90577ab3"),
                ("Heyorange_s_Sith Uniform_Reformation_Revised.7z",
                 "231ca858a5c354c3a76f8408b06256376930bcc999e8967491b1bc750547abe0")}:
            raise Blocked("uniform_source_revision_archive_identity_changed")
        if guid == "e015887a-be0f-86d1-830e-92d82c8fc46c" and (name, sha256) not in {
                ("Improved Enlightenment and Corruption Feats.zip",
                 "0c6db43f218d6d5bf73b1be8e3f7dc469254f3168920b6af6fa8513313617597"),
                ("Improved_Enlightenment_and_Corruption_Feats_v1_0_1.zip",
                 "b3730c98a05a12ee0a6610a57570be62ff14a7217abaef667ced9b4b541f7eed")}:
            raise Blocked("enlightenment_source_revision_archive_identity_changed")
        if literal_brackets and sha256 != source_plan_pin[1]:
            raise Blocked("source_plan_literal_archive_identity_changed")
        bindings.append({"componentGuid": guid, "filenames": [name], "sha256": sha256,
                         "sizeBytes": path.stat().st_size})
    return bindings


SOURCE_BOUND_GENERATORS = {
    "source-plan-native-patcher-v1": ("SourcePlanPatcherBinding", 1,
                                      "closed-normal-nine-action-empty-directions-v1"),
    "source-plan-selected-namespace-v1": ("SourcePlanNamespaceBinding", 1,
                                         "closed-original-source-selected-namespace-v1"),
    "original-pinned-source-program-v1": ("SourcePlanNamespaceBinding", 2,
                                         "original-pinned-ordered-program-v1"),
    "reviewed-guide-duplicate-cleanup-v1": ("GuideDuplicateCleanupBinding", 1,
                                          "delete-tpc-with-same-directory-casefold-stem-tga-or-dds-v1"),
}


def source_bound_envelope(component):
    """Recognize a compiled draft envelope for preservation, never approve metadata.

    The actual Rust review that follows must freshly reproduce the source body,
    owner and effects. A generator string alone never waives the drop guard.
    """
    record = component.get("DraftReview") or {}
    spec = SOURCE_BOUND_GENERATORS.get(record.get("Generator"))
    if spec is None:
        return False
    key, schema, policy = spec
    binding = component.get(key)
    result = record.get("Result") or {}
    if (not isinstance(binding, dict) or binding.get("Schema") != schema
            or binding.get("Policy") != policy
            or not isinstance(record.get("RequiresReview"), bool)
            or result.get("ComponentGuid") != component.get("Guid")
            or result.get("DraftInstructionCount") != len(component.get("Instructions") or [])
            or result.get("UnparsedGaps") != []
            or not isinstance(record.get("Derivations"), list)):
        return False
    if key == "SourcePlanPatcherBinding":
        original = binding.get("OriginalComponent", {})
    elif key == "SourcePlanNamespaceBinding":
        choice = binding.get("OriginalSourceChoice", {})
        original = choice.get("OriginalComponent" if schema == 2 else "canonical_component", {})
    else:
        original = binding.get("OriginalComponent", {})
    return (isinstance(original, dict)
            and all(original.get(field) == component.get(field)
                    for field in ("Guid", "Name", "Author", "Directions", "ResourceRegistry")))


def bound_result(before, after):
    """(components bound by the resolved-filename pass, problem): problem names anything `after` lost from `before`."""
    def loaded_instructions(value):
        # The explicit compact TOML writer omits only these empty defaults.
        # Compare complete loaded records, retaining every supplied field/value.
        if not isinstance(value, list):
            return value
        return [{"Arguments": "", "Dependencies": [], "Restrictions": [], **step}
                if isinstance(step, dict) else step for step in value]

    def loaded_components(values):
        return [{**component,
                 **({"Instructions": loaded_instructions(component["Instructions"])}
                    if "Instructions" in component else {}),
                 **({"Options": loaded_components(component["Options"])}
                    if isinstance(component.get("Options"), list) else {})}
                if isinstance(component, dict) else component for component in values]

    def flatten(components, found):
        for component in components:
            record = component.get("DraftReview") or {}
            found[str(component.get("Guid", "")).lower()] = (
                len(component.get("Instructions") or []), "DraftReview" in component,
                bool(component.get("InstallationWarning")),
                record.get("Generator") == BIND_GENERATOR or source_bound_envelope(component),
                source_bound_envelope(component))
            flatten([option for option in component.get("Options") or [] if isinstance(option, dict)], found)
        return found

    def movie_bindings(components):
        found = {}
        for component in components:
            record = component.get("DraftReview") or {}
            if record.get("Generator") == MOVIE_BIND_GENERATOR:
                # The fallback restarts conversion from the guide. It may not
                # replace a previously generated movie recipe with an unbound
                # or semantically different draft, even at the same step count.
                recipe = {
                    key: component.get(key) for key in
                    ("Instructions", "Options", "ArchiveMovieBinding", "DraftReview")}
                if isinstance(recipe["Instructions"], list):
                    # Guide::from_value canonicalizes instruction Source paths
                    # on reload. Match only that transformation for the typed
                    # movie paths; every other instruction/provenance field
                    # remains exact, including path spelling and order.
                    recipe["Instructions"] = [
                        {**step, "Source": [path.replace("\\", "/") for path in step["Source"]]}
                        if (isinstance(step, dict) and step.get("Action") in ("Extract", "Move")
                            and isinstance(step.get("Source"), list)
                            and all(isinstance(path, str) and path.startswith(
                                ("<<modDirectory>>\\", "<<modDirectory>>/")) for path in step["Source"]))
                        else step for step in recipe["Instructions"]]
                found[str(component.get("Guid", "")).lower()] = recipe
            found.update(movie_bindings([
                option for option in component.get("Options") or [] if isinstance(option, dict)]))
        return found

    def vendor_bindings(components):
        found = {}
        for component in components:
            if (component.get("DraftReview") or {}).get("Generator") in {
                    VENDOR_BIND_GENERATOR, MANUAL_BIND_GENERATOR, MANUAL_PATCHER_GENERATOR}:
                found[str(component.get("Guid", "")).lower()] = {
                    key: component.get(key) for key in
                    ("Instructions", "Options", "VendorInstallerBinding", "AuthoredRecipeBinding", "DraftReview")}
            found.update(vendor_bindings([
                option for option in component.get("Options") or [] if isinstance(option, dict)]))
        return found

    def source_bindings(components):
        found = {}
        for component in components:
            if source_bound_envelope(component):
                key = SOURCE_BOUND_GENERATORS[component["DraftReview"]["Generator"]][0]
                found[str(component.get("Guid", "")).lower()] = {
                    field: component.get(field) for field in
                    ("Instructions", "Options", key, "DraftReview")}
            found.update(source_bindings([
                option for option in component.get("Options") or [] if isinstance(option, dict)]))
        return found

    old, new = loaded_components(components_of(before)), loaded_components(components_of(after))
    if [item.get("Guid") for item in old] != [item.get("Guid") for item in new]:
        return 0, "bound_output_components_changed"
    before_order = tomllib.loads(before.read_text(encoding="utf-8")).get("ModSyncSourceOrder")
    after_order = tomllib.loads(after.read_text(encoding="utf-8")).get("ModSyncSourceOrder")
    if before_order != after_order:
        return 0, "bound_output_source_order_changed"
    new_movies = movie_bindings(new)
    if any(new_movies.get(identity) != binding
           for identity, binding in movie_bindings(old).items()):
        return 0, "bound_output_changes_resolved_movie_recipe"
    new_vendors = vendor_bindings(new)
    if any(new_vendors.get(identity) != binding
           for identity, binding in vendor_bindings(old).items()):
        return 0, "bound_output_changes_resolved_vendor_recipe"
    new_sources = source_bindings(new)
    if any(new_sources.get(identity) != binding
           for identity, binding in source_bindings(old).items()):
        return 0, "bound_output_changes_compiled_source_recipe"
    kept = flatten(new, {})
    for guid, (count, drafted, warned, _, _) in flatten(old, {}).items():
        if guid not in kept:
            if count or drafted or warned:
                return 0, "bound_output_drops_instructions_or_drafts"
            continue
        new_count, new_drafted, new_warned, new_bound, new_source = kept[guid]
        if ((count and not new_count) or (drafted and not new_drafted)
                or (warned and not new_warned and not new_source)
                or (new_count < count and not new_bound)):
            return 0, "bound_output_drops_instructions_or_drafts"
    return sum(1 for item in new
               if (item.get("DraftReview") or {}).get("Generator") in
               (BIND_GENERATOR, MOVIE_BIND_GENERATOR, VENDOR_BIND_GENERATOR,
                MANUAL_BIND_GENERATOR, MANUAL_PATCHER_GENERATOR)
               or source_bound_envelope(item)), None


BIND_DIAGNOSTICS_KEY = "bind_diagnostics"
BIND_DIAGNOSTIC_KINDS = (
    ("loose-file method remains unresolved", "loose_file_unresolved"),
    ("patcher method remains unresolved", "patcher_unresolved"),
)
BIND_DIAGNOSTIC_PRINT_CAP = 50


def component_names(converted):
    """{lowercase GUID: Name} of a converted TOML; empty when it cannot be read."""
    try:
        document = tomllib.loads(Path(converted).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    components = document.get("thisMod", document.get("Components", []))
    return {str(each["Guid"]).lower(): each.get("Name") or each.get("Heading")
            for each in components if isinstance(each, dict) and each.get("Guid")}


def bind_diagnostic_kind(reason):
    lowered = reason.lower()
    for prefix, kind in BIND_DIAGNOSTIC_KINDS:
        if lowered.startswith(prefix):
            return kind
    return "unresolved"


def unresolved_diagnostics(text):
    """(component GUID or None, reason) for each 'unresolved' diagnostic a bind stage log carries.

    The converter prints its per-component reports as pretty JSON arrays on stderr: the resolved
    archive pass as {componentGuid, diagnostics: [...]}, the local generation pass as
    {ComponentGuid, Success, SkipReason}. A log with no such array falls back to its raw
    'remains unresolved' lines.
    """
    found = []
    decoder = json.JSONDecoder()
    for start in (match.start() for match in re.finditer(r"^\[$", text, re.MULTILINE)):
        try:
            results, _ = decoder.raw_decode(text, start)
        except ValueError:
            continue
        for item in results if isinstance(results, list) else ():
            if not isinstance(item, dict):
                continue
            identity = item.get("componentGuid", item.get("ComponentGuid"))
            reasons = list(item["diagnostics"]) if isinstance(item.get("diagnostics"), list) else []
            if item.get("Success") is False:
                reasons.append(item.get("SkipReason"))
            found += [(str(identity).lower() if identity else None, reason) for reason in reasons
                      if isinstance(reason, str) and "unresolved" in reason.lower()]
    if not found:
        found = [(None, line.strip()) for line in ANSI_ESCAPE.sub("", text).splitlines()
                 if "remains unresolved" in line.lower()]
    return found


def record_bind_diagnostics(private, report, names=None):
    """Keep and print, flushed, the unresolved diagnostics of the archive binding stages.

    The converter exits 0 when it refuses to bind a component (for example a loose-file layout it
    cannot place), so the only trace is a diagnostic in the stage log. They go to
    report["bind_diagnostics"] as {stage, component, component_guid, kind, reason}.
    """
    names = names or {}
    entries, seen = [], set()
    for stage in (BIND_STAGE, BIND_GENERATE_STAGE):
        try:
            text = (private / (stage + ".log")).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for identity, reason in unresolved_diagnostics(text):
            if (stage, identity, reason) in seen:
                continue
            seen.add((stage, identity, reason))
            entries.append({"stage": stage, "component": names.get(identity), "component_guid": identity,
                            "kind": bind_diagnostic_kind(reason), "reason": reason})
    report[BIND_DIAGNOSTICS_KEY] = entries
    for entry in entries[:BIND_DIAGNOSTIC_PRINT_CAP]:
        print(f"::error::ports: {entry['stage']}: {entry['component'] or 'unnamed component'} "
              f"[{entry['component_guid'] or 'no guid'}]: {entry['reason'][:600]}",
              file=sys.stderr, flush=True)
    if len(entries) > BIND_DIAGNOSTIC_PRINT_CAP:
        print(f"::error::ports: {len(entries) - BIND_DIAGNOSTIC_PRINT_CAP} more bind diagnostics "
              f"are in the report under {BIND_DIAGNOSTICS_KEY}", file=sys.stderr, flush=True)


def bind_archives(command, source, guide, converted, archives, archive_map, private, report, env,
                  rules=None, vendor_languages=None, reviewed_recipes=None, compact_defaults=False):
    """Bind method-only loose-file components to their one mapped archive (convert --resolved-archives).

    --resolved-archives only touches components without instructions and conflicts with
    --auto-generate-local, so it runs from the guide (--parse-directions) and the local
    generation pass then runs on its output. That branch is used only when both exit 0 and
    nothing the plain convert produced is lost. Legacy callers retain the plain conversion
    when binding fails. Complete source-program callers propagate failed binding so its
    original error remains the stage failure. Returns the TOML the verified review should read.
    """
    review = report.setdefault("draft_review", {})
    review["bound"] = 0
    if archive_map is None:
        return converted
    try:
        bindings = resolved_archive_bindings(
            converted, mapped_archives(archive_map.resolve(strict=True)), archives, rules,
            vendor_languages=vendor_languages, reviewed_recipes=reviewed_recipes,
            archive_groups=mapped_archive_groups(archive_map.resolve(strict=True)),
            source_plan_pins=(load_source_plan_archive_pins(source, guide.parent.name)
                              if compact_defaults else None),
            source_program_groups=(load_source_program_groups(source, guide.parent.name, converted)
                                   if compact_defaults else None),
            excluded_guids=set((report.get("install_profile_scope") or {}).get("excluded_guids", [])))
    except (OSError, ValueError, AttributeError):
        review["bind_failed"] = "archive_map_unreadable"
        return converted
    already_native = set((report.get("native_k1_profile") or {}).get("bound_owners", []))
    bindings = [binding for binding in bindings if binding["componentGuid"] not in already_native]
    if not bindings:
        return converted
    for binding in bindings:
        language = (vendor_languages or {}).get(binding["componentGuid"])
        if language is not None:
            binding["vendorLanguage"] = language
    plan = private / "resolved-archives.json"
    plan.write_text(json.dumps(bindings), encoding="utf-8")
    bound, staged = private / "converted-bound.toml", private / "converted-bound-pending.toml"
    compact = ["--compact-instruction-defaults"] if compact_defaults else []
    binding_input = converted if compact_defaults else guide
    drafting = [] if compact_defaults else ["--parse-directions"]
    try:
        run(command + ["convert", str(binding_input), str(staged), "--format", "toml"] + drafting + [
                       "--source-path", str(archives), "--resolved-archives", str(plan)] + compact,
            source, private, BIND_STAGE, report, env)
        run(command + ["convert", str(staged), str(bound), "--format", "toml",
                       "--source-path", str(archives), "--auto-generate-local",
                       "--archive-map", str(archive_map.resolve(strict=True))] + compact,
            source, private, BIND_GENERATE_STAGE, report, env)
    except Blocked as failure:
        if str(failure) == INTERRUPTED:
            raise
        review["bind_failed"] = str(failure)
        if compact_defaults:
            raise
        return converted
    finally:
        record_bind_diagnostics(private, report, component_names(converted))
    if not bound.is_file() or bound.is_symlink():
        review["bind_failed"] = "bind_output_missing"
        if compact_defaults:
            raise Blocked("bind_archives_output_missing")
        return converted
    try:
        count, problem = bound_result(converted, bound)
    except (OSError, ValueError):
        count, problem = 0, "bound_output_unreadable"
    if problem:
        review["bind_failed"] = problem
        if compact_defaults:
            raise Blocked("bind_archives_invalid_output")
        return converted
    review["bound"] = count
    return bound if count else converted


def bind_native_k1(command, source, guide, converted, archives, private, report, env):
    """Bind only the fixed original three-owner profile; preserve every other model."""
    roles = {"6c3c7165-297e-80eb-b9f8-d47506dab2d5": "hrmenus",
             "7c11497d-de47-8eee-b80a-967b984a2822": "galaxy",
             "267b10d2-bab0-8e3a-a258-f9b6d6f9d71b": "fourgb"}
    canonical, bound = private / "k1-canonical-input.toml", private / "k1-native-bound.toml"
    # Compare the actual native loader's input, including its established path
    # and list normalizations, rather than weakening preservation comparisons.
    run(command + ["convert", str(converted), str(canonical), "--format", "toml",
                   "--compact-instruction-defaults"], source, private, "canonicalize_k1_input", report, env)
    run(command + ["convert", str(canonical), str(bound), "--format", "toml",
                   "--source-path", str(archives), "--k1-widescreen-profile", k1_widescreen_inputs.PROFILE_ID,
                   "--k1-source-guide", str(guide), "--k1-galaxy-namespace", "Vanilla",
                   "--compact-instruction-defaults"], source, private, "bind_k1_widescreen", report, env)
    canonical_model, bound_model = private / "k1-canonical-model.json", private / "k1-bound-model.json"
    for path, target, stage in ((canonical, canonical_model, "observe_k1_input_model"),
                                (bound, bound_model, "observe_k1_bound_model")):
        run(command + ["convert", str(path), str(target), "--format", "json"],
            source, private, stage, report, env)
    before, after = (json.loads(path.read_text(encoding="utf-8")) for path in (canonical_model, bound_model))
    originals, generated = before.get("thisMod"), after.get("thisMod")
    if (not isinstance(originals, list) or not isinstance(generated, list)
            or len(originals) != 197 or [item.get("Guid") for item in originals]
            != [item.get("Guid") for item in generated]):
        raise Blocked("native_k1_source_inventory_changed")
    expected_root = dict(before)
    expected_root["K1WidescreenSchedule"] = after.get("K1WidescreenSchedule")
    expected_root["thisMod"] = generated
    if expected_root != after:
        raise Blocked("native_k1_unexpected_root_metadata_change")
    observed = set()
    for original, current in zip(originals, generated):
        identity = original["Guid"]
        if identity not in roles:
            if current != original:
                raise Blocked("native_k1_changed_other_component")
            continue
        record = current.get("DraftReview", {})
        binding = current.get("K1WidescreenBinding", {})
        steps = current.get("Instructions", [])
        if (binding.get("InputComponent") != original or binding.get("Role") != roles[identity]
                or binding.get("SourceGuideSha256") != k1_widescreen_inputs.GUIDE_SHA256
                or record.get("Generator") != "resolved-k1-widescreen" or record.get("RequiresReview") is not True
                or len(steps) != 1 or steps[0].get("Action") != "K1Widescreen"
                or steps[0].get("Arguments") != roles[identity]):
            raise Blocked("native_k1_binding_does_not_preserve_original_owner")
        observed.add(identity)
    if observed != set(roles):
        raise Blocked("native_k1_owner_missing")
    report["native_k1_profile"] = {"profile": k1_widescreen_inputs.PROFILE_ID,
        "source_guide_sha256": k1_widescreen_inputs.GUIDE_SHA256, "galaxy_namespace": "Vanilla",
        "bound_owners": sorted(observed), "input_sha256": digest(canonical), "output_sha256": digest(bound)}
    return bound


def vendor_language_selection(game, guide, report):
    """Select the reviewed English vendor path only for an actual English K2 baseline."""
    if guide.parent.name != "k2":
        return {}
    tlk, executable = game / "dialog.tlk", game / "swkotor2.exe"
    try:
        if any(path.is_symlink() or not stat.S_ISREG(path.stat().st_mode)
               for path in (tlk, executable)):
            return {}
        with tlk.open("rb") as stream:
            header = stream.read(12)
        with executable.open("rb") as stream:
            signature = stream.read(2)
        if (len(header) != 12 or header[:8] != b"TLK V3.0"
                or int.from_bytes(header[8:], "little") != 0 or signature != b"MZ"):
            return {}
        report["vendor_language_selection"] = {
            "language": "english", "tlk_language_id": 0,
            "tlk_sha256": digest(tlk), "executable_sha256": digest(executable),
            "basis": "verified K2 baseline TLK header; native execution rechecks game and language"}
        return {"bee5a72b-d748-8f83-8261-81758a9bfc56": "english"}
    except OSError:
        return {}


def baseline_applicability(command, source, game, guide, converted, private, report, env):
    """Observe native game detection without archive inspection or input mutation."""
    if guide.parent.name != "k2":
        return None
    # Native selection may resolve a Steam assets subtree. This closed rule
    # binds only the authenticated Windows baseline at the supplied root.
    if (game / "steamassets").exists() or (game / "steamassets").is_symlink():
        return None
    executable = game / "swkotor2.exe"
    binary = Path(command[0])
    if any(path.is_symlink() or not path.is_file()
           for path in (executable, binary, converted)):
        raise Blocked("baseline_applicability_requires_regular_inputs")
    before = {"input": digest(converted), "binary": digest(binary),
              "executable": digest(executable), "size": executable.stat().st_size}
    stage = "baseline_applicability"
    run(command + ["selection", "--input", str(converted), "--game-dir", str(game), "--json"],
        source, private, stage, report, env)
    after = {"input": digest(converted), "binary": digest(binary),
             "executable": digest(executable), "size": executable.stat().st_size}
    if (before != after or (game / "steamassets").exists()
            or (game / "steamassets").is_symlink()
            or any(path.is_symlink() for path in (executable, binary, converted))):
        raise Blocked("baseline_applicability_inputs_changed")
    log = private / (stage + ".log")
    if log.stat().st_size > 8 * 1024 * 1024:
        raise Blocked("baseline_applicability_output_too_large")
    try:
        observed = json.loads(log.read_text(encoding="utf-8"))
    except (ValueError, UnicodeError) as error:
        raise Blocked("baseline_applicability_output_invalid") from error
    detected = observed.get("game") if isinstance(observed, dict) else None
    native = {"command": "selection", "source_dir_supplied": False, "exit_code": 0,
              "game": detected, "input_sha256": before["input"],
              "stdout_sha256": digest(log), "binary_sha256": before["binary"]}
    actual_executable = {"relative_path": "swkotor2.exe", "sha256": before["executable"],
                         "size": before["size"]}
    report["baseline_applicability_observation"] = {
        "native_selection": native, "executable": actual_executable}
    if (detected != "Kotor2Aspyr"
            or before["executable"] != fullrun_guide_choices.ASPYR_EXE_SHA256
            or before["size"] != fullrun_guide_choices.ASPYR_EXE_SIZE):
        return None
    return {"schema_version": 1, "rule": fullrun_guide_choices.FOUR_GB_RULE["rule"],
            "native_selection": native, "executable": actual_executable,
            "baseline_basis": fullrun_guide_choices.ASPYR_BASIS}


def execute_runner_arguments(private, converted, report, force=False):
    """Create one private execution prefix without changing the native Patcher backend."""
    vendors = [component for component in converted_components(converted)
               if component.get("VendorInstallerBinding")]
    if not vendors and not force:
        return []
    programs = {name: shutil.which(name) for name in ("Xvfb", "wine", "wineserver")}
    if not all(programs.values()):
        raise Blocked("reviewed_vendor_requires_installed_wine_and_xvfb")
    programs = {name: str(Path(path).resolve(strict=True)) for name, path in programs.items()}
    requests = private / "windows-execute"
    requests.mkdir(mode=0o700)
    prefix = requests / "wine-prefix"
    prefix.mkdir(mode=0o700)
    report["windows_execute_runner"] = {
        "backend": "Wine/Xvfb", "configured": True,
        "components": [component["Guid"] for component in vendors]}
    return ["--execute-request-dir", str(requests), "--execute-xvfb", programs["Xvfb"],
            "--execute-wine", programs["wine"], "--execute-wineserver", programs["wineserver"],
            "--execute-wine-prefix", str(prefix)]


ALIAS_MOD_DIRECTORY = re.compile(r"<<modDirectory>>[\\/]+", re.IGNORECASE)
ALIAS_UNSAFE_NAME = re.compile(r"[/\\\x00-\x1f\x7f]")
ALIAS_STEM_STRIP = re.compile(r'[<>:"/\\|?*\x00-\x1f\x7f]+')
ALIAS_STEM_LIMIT = 100
ALIAS_COLLISION_LIMIT = 50
ALIAS_FAILED_KEY = "archive_alias_failed"
ALIAS_RULE_KEYS = frozenset({"heading", "guid", "files", "reason"})


def safe_archive_name(name):
    return (isinstance(name, str) and name == name.strip() and name not in ("", ".", "..")
            and len(name) <= 255 and not ALIAS_UNSAFE_NAME.search(name))


def load_archive_aliases(game, path=None):
    """The reviewed archive_aliases rules of one game from archive_selections.json, keyed by GUID.

    The `archive_aliases` table maps k1/k2 to a list of {heading, guid, files, reason}: the
    component with that GUID takes exactly the listed restored files for its Extract glob,
    where the map evidence alone cannot choose (a link that maps to several files, or names
    that share no tokens with the glob). The whole table is validated whatever the game, so a
    malformed entry for the other game still blocks. A missing file or table declares nothing.
    """
    path = Path(path or LANE_SKIPS_PATH)
    if not path.is_file():
        return {}
    document = json.loads(path.read_text(encoding="utf-8"))
    table = document.get("archive_aliases", {}) if isinstance(document, dict) else None
    if not isinstance(table, dict) or not table.keys() <= set(LANE_SKIP_GAMES):
        raise Blocked("archive_aliases_invalid")
    result = {}
    for name, entries in table.items():
        if not isinstance(entries, list):
            raise Blocked("archive_aliases_invalid")
        seen_guids, seen_headings = set(), set()
        for entry in entries:
            if (not isinstance(entry, dict) or entry.keys() != ALIAS_RULE_KEYS
                    or any(not isinstance(entry[key], str) or not entry[key].strip()
                           or entry[key] != entry[key].strip() for key in ("heading", "guid", "reason"))
                    or not canonical_guid(entry["guid"])
                    or not isinstance(entry["files"], list) or not entry["files"]
                    or not all(safe_archive_name(each) for each in entry["files"])):
                raise Blocked("archive_aliases_invalid")
            if (len(set(entry["files"])) != len(entry["files"])
                    or entry["guid"] in seen_guids or entry["heading"] in seen_headings):
                raise Blocked("archive_aliases_duplicate")
            seen_guids.add(entry["guid"])
            seen_headings.add(entry["heading"])
            if name == game:
                result[entry["guid"]] = entry
    return result


def glob_regex(segment):
    """One path segment as the validator matches it: only `*` and `?` are wildcards, case-insensitive.

    ModSync.ts resolveWildcards runs picomatch with nobrace/nobracket/noextglob, so
    `{zip,rar,7z}` and `[x]` are literal characters, not alternatives.
    """
    parts = []
    for char in segment:
        if char == "*":
            if not parts or parts[-1] != "[^/]*":
                parts.append("[^/]*")
        else:
            parts.append("[^/]" if char == "?" else re.escape(char))
    return re.compile("".join(parts), re.IGNORECASE | re.DOTALL)


def archive_pattern_matches(directory, segments, visited=None):
    """True when some path below `directory` matches the glob `segments` (files and folders count)."""
    if not segments:
        return True
    try:
        entries = list(os.scandir(directory))
    except OSError:
        return False
    head, rest = segments[0], segments[1:]
    if head == "**":
        if not rest:
            return bool(entries)
        if archive_pattern_matches(directory, rest):
            return True
        visited = set() if visited is None else visited
        visited.add(os.path.realpath(directory))
        return any(entry.is_dir() and os.path.realpath(entry.path) not in visited
                   and archive_pattern_matches(entry.path, segments, visited)
                   for entry in entries)
    matcher = glob_regex(head)
    return any(matcher.fullmatch(entry.name) and (
        not rest or (entry.is_dir() and archive_pattern_matches(entry.path, rest))) for entry in entries)


def link_basename(link):
    try:
        return unquote(urlsplit(str(link)).path.rsplit("/", 1)[-1])
    except ValueError:
        return ""


def extract_patterns(component):
    """[(raw source, path relative to the mod directory)] of the Extract sources the lane will validate.

    Covers the component's own instructions and those of its options that are selected in the
    converted TOML (`--select` turns options on only through their own GUIDs).
    """
    records = list(component.get("Instructions") or [])
    for option in component.get("Options") or []:
        if isinstance(option, dict) and option.get("IsSelected") is True:
            records += option.get("Instructions") or []
    found = []
    for record in records:
        if not isinstance(record, dict) or str(record.get("Action", "")).lower() != "extract":
            continue
        sources = record.get("Source")
        for raw in [sources] if isinstance(sources, str) else sources if isinstance(sources, list) else ():
            prefix = ALIAS_MOD_DIRECTORY.match(raw) if isinstance(raw, str) else None
            if prefix and all(raw != seen for seen, _ in found):
                found.append((raw, raw[prefix.end():].replace("\\", "/")))
    return found


def component_links(component):
    links = component.get("ModLink")
    return [links] if isinstance(links, str) else [
        link for link in links if isinstance(link, str)] if isinstance(links, list) else []


def alias_stem(name):
    """The mod name as ModSync.ts sanitizeFileName cleans it (used for the guessed archive glob)."""
    cleaned = " ".join(ALIAS_STEM_STRIP.sub("", str(name or "")).split()).strip(" .")
    return cleaned[:ALIAS_STEM_LIMIT].rstrip(" .") or "mod"


def alias_candidates(segment, matcher, stem, source_name):
    """Names that satisfy `segment`, best first: the literal basename, else `<mod name>.<ext>`.

    When the validator reads the pattern literally (a `{zip,rar,7z}` group is not expanded),
    `<mod name>.<ext>` does not match; the pattern with every `*` removed does by construction.
    """
    if "*" not in segment and "?" not in segment:
        yield segment
        return
    extension = os.path.splitext(source_name)[1]
    if re.fullmatch(r"\.[A-Za-z0-9]{1,5}", extension) is None:
        extension = ""
    for base in (stem + extension, segment.replace("*", "").replace("?", "_")):
        if matcher.fullmatch(base):
            root, suffix = os.path.splitext(base)
            yield base
            for number in range(2, ALIAS_COLLISION_LIMIT + 1):
                numbered = f"{root} ({number}){suffix}"
                if not matcher.fullmatch(numbered):
                    break
                yield numbered


ALIAS_TOKEN = re.compile(r"[a-z0-9]+")
ALIAS_NOISE_TOKEN = re.compile(r"[0-9]+|v[0-9]+|zip|rar|7z|exe")
ALIAS_ARCHIVE_SUFFIX = re.compile(r"\.(?:zip|rar|7z|exe)$", re.IGNORECASE)
ARCHIVE_MAGIC = (b"PK\x03\x04", b"PK\x05\x06", b"Rar!\x1a\x07", b"7z\xbc\xaf\x27\x1c", b"MZ")


def mapped_archive_choices(path):
    """archive_map.json as {canonical link: [file names]}: every file a link maps to, in map order."""
    choices = {}
    for link, value in json.loads(path.read_text(encoding="utf-8")).items():
        names = choices.setdefault(canonical_link(link), [])
        for name in value if isinstance(value, list) else [value]:
            if isinstance(name, str) and name not in names:
                names.append(name)
    return choices


def link_slug(link):
    """Last non-empty path segment of a guide link ('.../2277-kiosk-hd/' -> '2277-kiosk-hd')."""
    try:
        segments = [part for part in unquote(urlsplit(str(link)).path).split("/") if part]
    except ValueError:
        return ""
    return segments[-1] if segments else ""


def alias_tokens(text):
    """Lower-case word tokens of a name; versions, numbers, archive extensions and plural 's' ignored."""
    tokens = set()
    for token in ALIAS_TOKEN.findall(str(text).casefold()):
        if not ALIAS_NOISE_TOKEN.fullmatch(token):
            tokens.add(token[:-1] if len(token) > 3 and token.endswith("s") else token)
    return tokens


def alias_reference(segment):
    """The mod-name text of an Extract glob: wildcards, a `{..}` group and the archive extension removed."""
    text = re.sub(r"\{[^{}]*\}", "", segment).replace("*", " ").replace("?", " ")
    return ALIAS_ARCHIVE_SUFFIX.sub("", text.strip(" .")).strip(" .")


def alias_precision(reference, evidence):
    """Share of the evidence tokens that are reference tokens, or None unless every reference token is present."""
    wanted = alias_tokens(reference)
    best = None
    for text in evidence:
        found = alias_tokens(text)
        if wanted and wanted <= found:
            best = max(best or 0.0, len(wanted) / len(found))
    return best


def alias_file_problem(archives, name):
    """None when `name` is a restored regular archive that can be linked; otherwise the reason it cannot."""
    source = archives / name
    if (ALIAS_UNSAFE_NAME.search(name) or name in (".", "..") or not 0 < len(name) <= 255
            or source.is_symlink() or not source.is_file()):
        return "mapped_archive_missing_or_unsafe"
    try:
        with open(source, "rb") as handle:
            header = handle.read(8)
    except OSError:
        return "mapped_archive_missing_or_unsafe"
    return None if header.startswith(ARCHIVE_MAGIC) else "mapped_file_not_archive"


def alias_identical_groups(archives, names):
    """Group `names` by file content, so mirrors of one download count once."""
    by_size = {}
    for name in names:
        by_size.setdefault((archives / name).stat().st_size, []).append(name)
    groups = []
    for same in by_size.values():
        if len(same) == 1:
            groups.append(same)
            continue
        by_content = {}
        for name in same:
            hasher = hashlib.sha256()
            with open(archives / name, "rb") as handle:
                for chunk in iter(lambda: handle.read(1 << 20), b""):
                    hasher.update(chunk)
            by_content.setdefault(hasher.digest(), []).append(name)
        groups.extend(by_content.values())
    return groups


def declared_source_names(pending, literal, declared, archives):
    """([declared file names], None) when every one is a restored archive, else (None, reason)."""
    if pending != 1 or (literal is not None and len(declared) != 1):
        return None, "declared_archives_not_applicable"
    for name in declared:
        problem = alias_file_problem(archives, name)
        if problem:
            return None, "declared_" + problem
    return list(declared), None


def alias_source_names(pending, links, choices, reference, literal, archives, declared=None):
    """([restored file names], None) for one unmatched pattern, or (None, reason).

    `pending` is how many distinct patterns of the component are unmatched, `choices` is
    `mapped_archive_choices`, `reference` the glob's mod-name text. Files that are missing,
    unsafe or not archives are dropped; mirrors (identical content) count once. Then:
    a literal basename is paired with the file or link URL that ends in that name; one file
    serves one pattern; otherwise the files whose name (or, for a link that maps to just
    that file, URL slug) contain every token of `reference` are the matches. A wildcard
    pattern really matches all of them, so each is returned; a literal one needs exactly
    one. Anything else stays unresolved with the reason, never guessed.

    `declared` is the reviewed `archive_aliases` file list of the component: it replaces the
    inference, and every listed file must be a restored archive or nothing is aliased.
    """
    if declared is not None:
        return declared_source_names(pending, literal, declared, archives)
    origins = {}
    multi = False
    for link in links:
        names = choices.get(canonical_link(link), ())
        multi = multi or len(names) > 1
        for name in names:
            origins.setdefault(name, []).append((link, len(names) == 1))
    if not origins:
        return None, "no_mapped_archive"
    problems = {name: alias_file_problem(archives, name) for name in origins}
    usable = [name for name in origins if not problems[name]]
    if not usable:
        return None, problems[next(iter(origins))]
    if literal is not None:
        paired = [name for name in usable
                  if name.casefold() == literal.casefold() or any(
                      single and link_basename(link).casefold() == literal.casefold()
                      for link, single in origins[name])]
        if len(paired) == 1:
            return paired, None
    precision = {name: alias_precision(
        reference, [name] + [link_slug(link) for link, single in origins[name] if single])
        for name in usable}
    ordered = sorted(usable, key=lambda name: (-(precision[name] or 0.0), name))
    groups = alias_identical_groups(archives, ordered)
    representatives = sorted((min(group, key=ordered.index) for group in groups), key=ordered.index)
    if len(representatives) == 1 and pending == 1:
        return representatives, None
    strong = [name for name in representatives if precision[name] is not None]
    if strong and (literal is None or len(strong) == 1):
        return strong, None
    return None, "multi_file_map_entry" if multi else "ambiguous_mapped_archives"


def link_alias(source, alias):
    """Hardlink `alias` to `source` (same directory), falling back to a relative symlink."""
    try:
        os.link(source, alias)
        return "hardlink"
    except FileExistsError:
        raise
    except OSError:
        os.symlink(source.name, alias)
        return "symlink"


def alias_archives(converted, archive_map, archives, selected, report, rules=None):
    """Give restored archives names the converted TOML's Extract globs match (TypeScript lanes).

    ModSync.ts guesses archive globs from mod names or URL basenames while cache.py restore
    stores archives under unrelated names, so `validate` finds nothing. For each selected
    component with an Extract source under `<<modDirectory>>` that matches nothing in
    `archives`, the one restored archive its ModLinks map to (`archive_map`) gets a hardlink
    (symlink if linking fails) in `archives` named so the glob matches. converted.toml is
    never changed. Links that cannot be made are recorded in `alias_unresolved`; validate
    then reports the missing download as before.

    The validator reads `{zip,rar,7z}` literally, so a braced glob is only matched by a name
    that ends in that literal group (`Name.{zip,rar,7z}`); it still opens the file by content,
    which is why the source must be a real archive (`alias_file_problem`).

    `rules` is `load_archive_aliases`: a component with a rule takes exactly its declared files,
    each under its own alias name, independent of what the map says.
    """
    aliases, unresolved = [], []
    rules = rules or {}
    report["archive_aliases"] = {"count": 0, "aliases": aliases}
    report["alias_unresolved"] = unresolved
    choices, map_problem = {}, None
    if archive_map is None:
        map_problem = "no_archive_map"
    else:
        try:
            choices = mapped_archive_choices(archive_map.resolve(strict=True))
        except (OSError, ValueError, AttributeError):
            map_problem = "archive_map_unreadable"
            report[ALIAS_FAILED_KEY] = map_problem
    needs = []
    for component in components_of(converted):
        try:
            identity = guid(component.get("Guid"))
        except (Blocked, ValueError, AttributeError):
            continue
        if identity not in selected:
            continue
        missing = [(raw, relative) for raw, relative in extract_patterns(component)
                   if not archive_pattern_matches(
                       archives, [part for part in relative.split("/") if part not in ("", ".")])]
        if missing:
            needs.append((component, identity, missing))
    taken = {name.casefold() for name in os.listdir(archives)}
    created = {}
    for component, identity, missing in needs:
        links = component_links(component)
        name = component.get("Name", component.get("Heading", ""))
        rule = rules.get(identity)
        declared = None
        if rule is not None:
            declared = rule["files"] if rule["heading"] == name else []
        failures = []
        for raw, relative in missing:
            segments = [part for part in relative.split("/") if part not in ("", ".")]
            if not segments or ".." in segments:
                failures.append({"pattern": raw, "reason": "path_escapes_archives_dir"})
                continue
            if len(segments) > 1:
                failures.append({"pattern": raw, "reason": "nested_pattern_not_aliased"})
                continue
            segment = segments[0]
            wildcard = "*" in segment or "?" in segment
            if declared == []:
                failures.append({"pattern": raw, "reason": "declared_heading_mismatch"})
                continue
            source_names, reason = alias_source_names(
                len(missing), links, choices, alias_reference(segment),
                None if wildcard else segment, archives, declared)
            if source_names is None:
                failures.append({"pattern": raw, "reason": reason if rule is not None else map_problem or reason})
                continue
            matcher = glob_regex(segment)
            for source_name in source_names:
                source = archives / source_name
                placed = None
                for candidate in alias_candidates(segment, matcher, alias_stem(name), source_name):
                    if ALIAS_UNSAFE_NAME.search(candidate) or not 0 < len(candidate) <= 255 \
                            or candidate in (".", ".."):
                        break
                    key = candidate.casefold()
                    if key in taken:
                        if not wildcard and created.get(key) == source_name:
                            placed = ""
                            break
                        continue
                    try:
                        method = link_alias(source, archives / candidate)
                    except FileExistsError:
                        taken.add(key)
                        continue
                    except OSError:
                        break
                    taken.add(key)
                    created[key] = source_name
                    record = {"component": name, "component_guid": identity,
                              "alias": candidate, "source": source_name, "method": method}
                    if rule is not None:
                        record["rule"] = "archive_aliases"
                    aliases.append(record)
                    placed = candidate
                    break
                if placed is None:
                    failures.append({"pattern": raw, "reason": "alias_not_created"})
        if failures:
            unresolved.append({"component": name, "component_guid": identity,
                               "reason": failures[0]["reason"], "patterns": failures})
    report["archive_aliases"]["count"] = len(aliases)


def draft_pending(component):
    """True when this component or option still carries an unreviewed draft.

    Mirrors the structural part of the Rust requires_review: a DraftReview that is
    not RequiresReview=false, or one whose Result still lists gaps, conditions or
    rejected drafts; with no DraftReview, the prose-draft InstallationWarning. The
    reviewed-identity receipt itself is only checked by the Rust validate step.
    """
    if "DraftReview" in component:
        review = component["DraftReview"]
        if not isinstance(review, dict) or review.get("RequiresReview") is not False:
            return True
        result = review.get("Result")
        return (not isinstance(result, dict)
                or str(result.get("ComponentGuid", "")).lower() != str(component.get("Guid", "")).lower()
                or bool(result.get("UnparsedGaps") or result.get("ConditionalDrafts")
                        or result.get("RejectedDrafts")))
    warning = component.get("InstallationWarning")
    return isinstance(warning, str) and REVIEW_FLAG in warning


def pending_drafts(converted, exempt_guids, gui, owners=False):
    """Selected components (and what they install) that still have an unreviewed draft.

    exempt_guids are reviewed-policy skips and GUI widescreen exclusions: they are
    never installed, so their drafts do not matter. The GUI lane installs every other
    component and refuses any nested draft; the CLI lane installs components with
    IsSelected = true (all, when none is selected, as the Rust loader does) and only
    their selected options. With `owners`, each item also names its top-level component
    (`owner_guid`), the only level `convert --review-draft` can accept.
    """
    document = tomllib.loads(converted.read_text(encoding="utf-8"))
    components = [component for component in document.get("thisMod", document.get("Components", []))
                  if isinstance(component, dict)]
    default_selected = not any(component.get("IsSelected") is True for component in components)
    pending = []

    def walk(component, owner, top):
        heading = component.get("Heading", component.get("Name"))
        if owner is not None:
            heading = f"{owner} / {heading}"
        if draft_pending(component):
            pending.append({"component_guid": str(component.get("Guid", "")).lower(),
                            "heading": heading})
            if owners:
                pending[-1]["owner_guid"] = top
        for option in component.get("Options", []):
            if isinstance(option, dict) and (gui or option.get("IsSelected") is True):
                walk(option, heading, top)

    for component in components:
        identity = str(component.get("Guid", "")).lower()
        if identity in exempt_guids:
            continue
        if gui or default_selected or component.get("IsSelected") is True:
            walk(component, None, identity)
    return pending


DRAFT_ACCEPT_STAGE = "accept_drafts"
DRAFT_ACCEPTANCE_KEY = "draft_acceptance"


def draft_refusal(component):
    """Why `convert --review-draft` cannot accept this top-level draft, or None when it can.

    Mirrors the Rust mark_reviewed: it needs a generated DraftReview record whose diagnostics
    belong to the component and list no unparsed gaps, conditional drafts or rejected drafts.
    A draft with a saved Resolution needs a new explicit resolution, so it is refused too.
    """
    review = component.get("DraftReview")
    if not isinstance(review, dict):
        return "prose draft without a generated draft review record"
    if "Resolution" in review:
        return "draft carries a saved resolution that no longer matches; it needs a new explicit resolution"
    result = review.get("Result")
    if not isinstance(result, dict) or str(result.get("ComponentGuid", "")).lower() != str(
            component.get("Guid", "")).lower():
        return "draft diagnostics are missing or belong to another component"
    problems = []
    for key, label in (("UnparsedGaps", "unparsed gap(s)"), ("ConditionalDrafts", "conditional draft(s)")):
        items = result.get(key) or []
        if items:
            problems.append(f"{len(items)} {label}: " + "; ".join(str(item)[:200] for item in items[:3]))
    if result.get("RejectedDrafts"):
        problems.append(f"{result['RejectedDrafts']} rejected draft instruction(s)")
    return "; ".join(problems) or None


def accept_drafts(command, source, converted, private, report, env, exempt, gui):
    """Accept every generated draft the converter can accept, and deselect the rest as errors.

    The converter fences instructions it drafted from the guide's prose until they are reviewed.
    The lane installs the guide as generated: each selected top-level component whose draft
    diagnostics are clean is accepted with `convert --review-draft` (validate and install then
    check it against the real archives and game). A draft that cannot be accepted (gaps,
    conditional or rejected drafts, an option-level draft, or one still fenced afterwards) is a
    generation failure: the component and its dependents are deselected as errors, so the rest
    installs and the lane fails. Returns the GUIDs deselected here.
    """
    components = converted_components(converted)
    by_guid = {normalized_reference(component.get("Guid")): component for component in components}
    grouped = {}
    for item in pending_drafts(converted, exempt, gui, owners=True):
        grouped.setdefault(item["owner_guid"], []).append(item)
    records = report.setdefault(SKIPPED_REPORT_KEY, [])
    start = len(records)

    def refuse(owner, reason):
        component = by_guid[normalized_reference(owner)]
        refused[owner] = reason
        records.append({"status": NOT_INSTALLED, "severity": SEVERITY_ERROR, "section": None,
                        "heading": component.get("Heading", component.get("Name")),
                        "reason": "generated draft not installable: " + reason, "urls": [],
                        "source": DRAFT_REFUSED_SOURCE, "component_guid": normalized_reference(owner)})

    refused = {}
    for owner, items in grouped.items():
        nested = [item["heading"] for item in items if item["component_guid"] != owner]
        problem = (("option draft(s) cannot be accepted per component: " + ", ".join(nested))
                   if nested else draft_refusal(by_guid[normalized_reference(owner)]))
        if problem:
            refuse(owner, problem)
    # Dependents of a refused draft are not installed, so their drafts are not accepted either.
    cascade_dependents(components, records, start)
    deselected = {record["component_guid"] for record in records[start:]}
    accepted = [owner for owner in grouped if owner not in deselected]
    if accepted:
        output = private / "converted-accepted.toml"
        run(command + ["convert", str(converted), str(output), "--format", "toml"]
            + [flag for owner in accepted for flag in ("--review-draft", owner)],
            source, private, DRAFT_ACCEPT_STAGE, report, env)
        if not output.is_file() or output.is_symlink():
            raise Blocked("accept_drafts_output_missing")
        if [normalized_reference(each.get("Guid")) for each in converted_components(output)] != list(by_guid):
            raise Blocked("accept_drafts_changed_component_order")
        output.replace(converted)
        middle = len(records)
        for item in pending_drafts(converted, exempt, gui, owners=True):
            if item["owner_guid"] in accepted:
                accepted.remove(item["owner_guid"])
                refuse(item["owner_guid"], "still fenced after convert --review-draft")
        cascade_dependents(components, records, middle)
        dependents = {record["component_guid"] for record in records[middle:]}
        accepted = [owner for owner in accepted if owner not in dependents]
    report[DRAFT_ACCEPTANCE_KEY] = {
        "accepted": accepted,
        "refused": [{"component_guid": owner, "reason": reason} for owner, reason in refused.items()]}
    return {record["component_guid"] for record in records[start:]}


def reclassify_policy_skips(report, guids):
    """Move converter skips of reviewed-policy components out of their converter cause bucket."""
    skips = report.get("convert_skips")
    if not skips or not guids:
        return
    for item in skips["components"]:
        if item["component_guid"] in guids and item["cause"] != POLICY_CAUSE:
            counts = skips["by_cause"]
            counts[item["cause"]] -= 1
            if not counts[item["cause"]]:
                del counts[item["cause"]]
            item["cause"] = POLICY_CAUSE
            counts[POLICY_CAUSE] = counts.get(POLICY_CAUSE, 0) + 1


QUARANTINE_KEY = "quarantined_validate_failures"
QUARANTINE_MAX_ITERATIONS = 15
QUARANTINE_MAX_FRACTION = 0.15
VALIDATE_FAILURE_KIND = "validate_failure"
QUARANTINE_DEPENDENT_KIND = "dependent_of_quarantined"
QUARANTINE_CAP_REASON = "validate_quarantine_cap_exceeded"
INSTALL_QUARANTINE_KEY = "quarantined_install_failures"
INSTALL_FAILURE_KIND = "install_failure"
INSTALL_QUARANTINE_CAP_REASON = "install_quarantine_cap_exceeded"
# Disk exhaustion is a lane infrastructure failure, not a defective mod.
DISK_EXHAUSTION = re.compile(r"\bENOSPC\b|no space left on device|disk quota exceeded", re.IGNORECASE)
QUARANTINE_FAILURE_KINDS = (VALIDATE_FAILURE_KIND, INSTALL_FAILURE_KIND)
QUARANTINE_PHASES = {
    "validate": {"key": QUARANTINE_KEY, "kind": VALIDATE_FAILURE_KIND,
                 "cap": QUARANTINE_CAP_REASON, "blocked": "validate_failed"},
    "install": {"key": INSTALL_QUARANTINE_KEY, "kind": INSTALL_FAILURE_KIND,
                "cap": INSTALL_QUARANTINE_CAP_REASON, "blocked": "install_failed"},
}
INSTALL_QUARANTINE_ACTIONS = frozenset(
    action.casefold() for action in ("Patcher", "Extract", "Move", "Copy", "Rename", "Delete"))
BASELINE_COPY_TIMEOUT = 7200
GAME_MISSING = re.compile(
    r"\b(?:game|kotor)\s+(?:directory|folder|install(?:ation)?)\b[^:]{0,40}"
    r"\b(?:not found|does not exist|is missing|missing)\b|chitin\.key",
    re.IGNORECASE)


def deselected_guids(flags):
    """The GUIDs named by the --deselect flags of a command line, normalized."""
    return {normalized_reference(value) for flag, value in zip(flags, flags[1:]) if flag == "--deselect"}


def converted_components(converted):
    document = tomllib.loads(converted.read_text(encoding="utf-8"))
    components = document.get("thisMod", document.get("Components", []))
    return [component for component in components if isinstance(component, dict)]


def strip_ocr_path_prefix(value):
    text = " ".join(str(value).split())
    component = text
    for separator in re.finditer(r":\s+", text):
        prefix = text[:separator.start()]
        if re.search(r"/|\\\\|\.\s*modsync|(?:^|\s)[A-Za-z]\s*:", prefix, re.IGNORECASE):
            candidate = text[separator.end():].strip()
            if candidate:
                component = candidate
    return component


def normalized_component_name(value):
    return re.sub(r"[\W_]+", "", strip_ocr_path_prefix(value).casefold())


def find_failing_component(components, name):
    """The one converted component a validate error names, or (None, why it cannot be attributed)."""
    if not name:
        return None, "component_unknown"
    matches = {}
    for component in components:
        if name in (component.get("Name"), component.get("Heading")):
            matches[normalized_reference(component.get("Guid"))] = component
    if not matches:
        folded = normalized_component_name(name)
        for component in components:
            names = (component.get("Name"), component.get("Heading"))
            if any(folded == normalized_component_name(value) for value in names if value):
                matches[normalized_reference(component.get("Guid"))] = component
    if not matches:
        folded = normalized_component_name(name)
        for component in components:
            names = (component.get("Name"), component.get("Heading"))
            if any(folded in normalized_component_name(value) or
                   normalized_component_name(value) in folded for value in names if value):
                matches[normalized_reference(component.get("Guid"))] = component
    if not matches:
        return None, "component_unknown"
    if len(matches) > 1:
        return None, "component_ambiguous"
    return next(iter(matches.items())), None


def instruction_index(components):
    """{normalized instruction GUID: [(component GUID, instruction record)]} over the converted components.

    Covers each component's own instructions and those of its options.
    """
    index = {}
    for component in components:
        identity = normalized_reference(component.get("Guid"))
        records = list(component.get("Instructions") or [])
        for option in component.get("Options") or []:
            if isinstance(option, dict):
                records += option.get("Instructions") or []
        for record in records:
            if isinstance(record, dict) and record.get("Guid"):
                index.setdefault(normalized_reference(record["Guid"]), []).append((identity, record))
    return index


def resolve_instruction_failures(failures, components):
    """(failures pinned to converted components by instruction GUID, or None; why not).

    A failure carrying an `instruction_guid` takes its component from the converted instruction set
    (`component_guid`, converted `component` name, and the instruction's Action when the log had no
    step line). A GUID that several components own, or that belongs to a different component than the
    log's `[i/N]` progress line names, refuses the pass. A GUID the converted set does not know keeps
    the progress-line attribution. A failure with neither is dropped as unattributable.
    """
    index = instruction_index(components)
    by_guid = {normalized_reference(component.get("Guid")): component for component in components}
    resolved = []
    for item in failures:
        guid_ = item.get("instruction_guid")
        if guid_ is None:
            resolved.append(item)
            continue
        owners = index.get(guid_, [])
        if len({identity for identity, _ in owners}) > 1:
            return None, "instruction_ambiguous"
        item = dict(item)
        if owners:
            identity, record = owners[0]
            named = item.get("component")
            if named:
                match, _ = find_failing_component(components, named)
                if match is not None and match[0] != identity:
                    return None, "instruction_component_mismatch"
            component = by_guid[identity]
            item["component_guid"] = identity
            item["component"] = str(component.get("Name", component.get("Heading")))
            item["action"] = item.get("action") or str(record.get("Action") or "")
        if item.get("component"):
            resolved.append(item)
    return resolved, None


def transitive_dependents(components, root, excluded):
    """Every not-yet-excluded component that depends, directly or through others, on `root`."""
    found = {}
    frontier = {root}
    while frontier:
        reached = set()
        for component in components:
            identity = normalized_reference(component.get("Guid"))
            if identity == root or identity in found or identity in excluded:
                continue
            if frontier & {normalized_reference(each) for each in component.get("Dependencies", [])}:
                found[identity] = component
                reached.add(identity)
        frontier = reached
    return found


def quarantined_entries(report):
    """Every quarantine record of the lane, validate failures first, then install failures."""
    return list(report.get(QUARANTINE_KEY) or ()) + list(report.get(INSTALL_QUARANTINE_KEY) or ())


def start_quarantine(components, deselect, report, max_iterations, max_fraction):
    """Open the quarantine budget that validate and install share; returns the policy record."""
    taken = deselected_guids(deselect)
    selected = sum(normalized_reference(component.get("Guid")) not in taken for component in components)
    max_components = min(selected, math.floor(selected * max_fraction + 1e-9)) if max_fraction > 0 else 0
    report["quarantine"] = {
        "max_iterations": max_iterations, "max_fraction": max_fraction,
        "selected_components": selected, "max_components": max_components,
        "attempts": 0, "install_attempts": 0, "iterations_used": 0, "stopped": None}
    return report["quarantine"]


def quarantine_stop(report, phase, stage, reason, parsed, failure, cap=False, details=None):
    """Record why a failure was not quarantined and fail the lane."""
    settings = QUARANTINE_PHASES[phase]
    report["quarantine"]["stopped"] = {
        "phase": phase, "stage": stage, "reason": reason, "component": parsed and parsed["component"],
        "step": parsed and parsed["step"], "error_line": parsed and parsed["error_line"]}
    report["quarantine"]["stopped"].update(details or {})
    raise Blocked(settings["cap"] if cap else settings["blocked"]) from failure


def quarantine_failure(phase, parsed, stage, components, deselect, game, report, failure,
                       before_commit=None, new_pass=True):
    """Quarantine the one component `parsed` names, plus its dependents, or fail the lane with a reason.

    Both stages use it, against the one budget in report["quarantine"]: the iteration cap counts
    passes (validate runs and install attempts together) and the component cap counts every
    quarantined component (failing components plus dependents). A pass that attributes several
    components calls this once per component and passes `new_pass=False` after the first, so it
    consumes one iteration and one component each. `before_commit` runs once every check has
    passed and before anything is recorded or deselected; the install stage restores the game there.
    """
    settings = QUARANTINE_PHASES[phase]
    policy = report["quarantine"]

    def stop(reason, cap=False):
        quarantine_stop(report, phase, stage, reason, parsed, failure, cap)

    entries = quarantined_entries(report)
    if policy["max_iterations"] <= 0:
        stop("quarantine_disabled")
    if new_pass and policy.get("iterations_used", 0) >= policy["max_iterations"]:
        stop("iteration_cap_exceeded", True)
    if not game.is_dir() or not any(game.iterdir()) or GAME_MISSING.search(parsed["message"]):
        stop("game_missing")
    hinted = parsed.get("component_guid")
    by_guid = {normalized_reference(each.get("Guid")): each for each in components}
    if hinted in by_guid:
        match, problem = (hinted, by_guid[hinted]), None
    else:
        match, problem = find_failing_component(components, parsed["component"])
    if problem:
        stop(problem)
    identity, component = match
    taken = deselected_guids(deselect)
    if identity in taken:
        stop("component_already_deselected")
    dependents = transitive_dependents(components, identity, taken)
    if len(entries) + 1 + len(dependents) > policy["max_components"]:
        stop("component_cap_exceeded", True)
    if before_commit is not None:
        before_commit()
    if new_pass:
        policy["iterations_used"] = policy.get("iterations_used", 0) + 1
    quarantined = report.setdefault(settings["key"], [])
    quarantined.append({
        "kind": settings["kind"], "name": component.get("Name", component.get("Heading")),
        "component_guid": identity, "step": parsed["step"], "action": parsed["action"],
        "error_line": parsed["error_line"], "stage": stage, "caused_by": None})
    if parsed.get("instruction_guid"):
        quarantined[-1]["instruction_guid"] = parsed["instruction_guid"]
    for dependent_guid, dependent in dependents.items():
        quarantined.append({
            "kind": QUARANTINE_DEPENDENT_KIND,
            "name": dependent.get("Name", dependent.get("Heading")),
            "component_guid": dependent_guid, "step": None, "action": None,
            "error_line": None, "stage": stage, "caused_by": identity})
    for each in (identity, *dependents):
        deselect += ["--deselect", each]


def read_stage_log(private, stage, report):
    """The text of a failed stage's log and its exit code."""
    try:
        text = (private / (stage + ".log")).read_text(encoding="utf-8", errors="replace")
    except OSError:
        text = ""
    code = report["commands"][-1].get("exit_code") if report.get("commands") else None
    return text, code


def attribute_validate_failures(text, components, deselect, report, stage, failure):
    """The distinct failing components a validate log names, or a recorded quarantine stop.

    A log with no tagged lines keeps the one-component rule (the last `Error: <Component> /
    step N (<Action>)` line). A log with tagged lines (`[patcher-error]`, `[missing-source]`,
    `[missing-archive]`) names every failing component in one pass. Nothing is attributed, and
    nothing quarantined, when any error line has another shape (a global or ambiguous error), when a
    missing download is not already covered by the reviewed policy, or when a tagged line's action
    is not in INSTALL_QUARANTINE_ACTIONS.
    """
    tagged, unattributed = parse_validate_errors(text)
    if not tagged:
        parsed = parse_component_failure(text)
        if parsed is None:
            quarantine_stop(report, "validate", stage, "error_not_attributable", None, failure)
        return [parsed]
    if unattributed:
        quarantine_stop(report, "validate", stage, "unattributed_error_lines", None, failure)
    taken = deselected_guids(deselect)
    candidates = []
    for item in tagged:
        if item["kind"] in VALIDATE_DOWNLOAD_KINDS:
            match, _ = find_failing_component(components, item["component"])
            if match is not None and match[0] in taken:
                continue
            quarantine_stop(report, "validate", stage, "missing_download_not_quarantinable", item, failure)
        if (item["action"] or "").casefold() not in INSTALL_QUARANTINE_ACTIONS:
            quarantine_stop(report, "validate", stage, "action_not_quarantinable", item, failure)
        candidates.append(item)
    candidates += parse_component_failures(text)
    if not candidates:
        quarantine_stop(report, "validate", stage, "error_not_attributable", None, failure)
    return distinct_failures(candidates)


def validate_with_quarantine(command, directories, deselect, converted, game, source, private, report,
                             env, max_iterations=QUARANTINE_MAX_ITERATIONS,
                             max_fraction=QUARANTINE_MAX_FRACTION, arguments=None, selectable=None):
    """Run `validate`; quarantine each component it names as failing and re-run, visibly and bounded.

    `deselect` is the --deselect flag list shared with install and the GUI driver; quarantined
    components (and their dependents) are appended to it in place. A failure is only
    quarantined when its `Error: <Component> / step N (<Action>): ...` line names exactly one
    converted component that is still selected, the game directory is populated, and neither
    the iteration cap nor max_fraction of the selected components would be exceeded. Anything
    else leaves the lane failed. Every quarantined component is listed in
    report["quarantined_validate_failures"].

    When the log is a TypeScript validate report, every distinct component its tagged error lines
    name is quarantined in the same pass, one `quarantine_failure` per component against the same
    budget, all or none (a refused pass rolls back what it had recorded and deselected). A pass
    consumes one iteration (the iteration cap counts validate passes) and one unit of the
    component budget per quarantined component or dependent. A pass over the component cap stops
    with `component_cap_exceeded` and records every attributed component, their count and the cap
    in report["quarantine"]["stopped"]. The pass is refused as a whole for global or ambiguous
    errors, for missing downloads, and for actions outside INSTALL_QUARANTINE_ACTIONS (see
    `attribute_validate_failures`).

    `arguments(deselect)` replaces `directories + deselect` as the validate arguments (TypeScript
    selects with --select, not --deselect) and `selectable` limits the components the budget and
    the name lookup see to those GUIDs.
    """
    components = converted_components(converted)
    if selectable is not None:
        components = [component for component in components
                      if normalized_reference(component.get("Guid")) in selectable]
    report.setdefault(QUARANTINE_KEY, [])
    start_quarantine(components, deselect, report, max_iterations, max_fraction)
    policy = report["quarantine"]
    iterations = 0
    while True:
        stage = "validate" if iterations == 0 else f"validate_retry_{iterations}"
        policy["attempts"] = iterations + 1
        try:
            run(command + ["validate"] + (arguments(deselect) if arguments else directories + deselect),
                source, private, stage, report, env)
        except Blocked as failure:
            if str(failure) != stage + "_failed":
                raise
            text, code = read_stage_log(private, stage, report)
            record_failure(text, stage, code, report)
            failing = attribute_validate_failures(text, components, deselect, report, stage, failure)
            recorded = list(report[QUARANTINE_KEY])
            flags = list(deselect)
            already = deselected_guids(deselect)
            used = policy["iterations_used"]
            counted = False
            try:
                for parsed in failing:
                    match, _ = find_failing_component(components, parsed["component"])
                    if match is not None and match[0] in deselected_guids(deselect) - already:
                        continue
                    quarantine_failure("validate", parsed, stage, components, deselect, game, report, failure,
                                       new_pass=not counted)
                    counted = True
            except Blocked:
                report[QUARANTINE_KEY][:] = recorded
                deselect[:] = flags
                policy["iterations_used"] = used
                stopped = policy.get("stopped")
                if stopped and stopped["reason"] == "component_cap_exceeded":
                    stopped.update({
                        "attributed_count": len(failing), "component_cap": policy["max_components"],
                        "already_quarantined": len(quarantined_entries(report)),
                        "attributed_components": [item["component"] for item in failing]})
                raise
            iterations += 1
            continue
        report.pop("failure", None)
        return


def game_is_populated(game):
    """The game directory exists, is not empty and still has its chitin.key."""
    try:
        return game.is_dir() and any(child.name.casefold() == "chitin.key" for child in game.iterdir())
    except OSError:
        return False


def tree_manifest(root):
    """Every entry under `root`: relative path -> (kind, size or link target)."""
    manifest = {}
    pending = [root]
    while pending:
        with os.scandir(pending.pop()) as entries:
            for entry in entries:
                relative = Path(entry.path).relative_to(root).as_posix()
                if entry.is_symlink():
                    manifest[relative] = ("link", os.readlink(entry.path))
                elif entry.is_dir(follow_symlinks=False):
                    manifest[relative] = ("dir", 0)
                    pending.append(entry.path)
                else:
                    manifest[relative] = ("file", entry.stat(follow_symlinks=False).st_size)
    return manifest


def copy_tree(source, destination):
    """Copy the contents of `source` into `destination` as independent files (never hardlinks)."""
    destination.mkdir(parents=True, exist_ok=True)
    try:
        subprocess.run(["cp", "-a", "--reflink=auto", "--", str(source) + "/.", str(destination) + "/"],
                       check=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, timeout=BASELINE_COPY_TIMEOUT)
    except FileNotFoundError:
        shutil.copytree(source, destination, symlinks=True, dirs_exist_ok=True)


def game_file_state(root):
    """Every regular file under `root`: relative path -> (size, mtime_ns)."""
    state = {}
    for directory, _, filenames in os.walk(root, followlinks=False):
        for name in filenames:
            path = Path(directory) / name
            info = path.lstat()
            state[path.relative_to(root).as_posix()] = (info.st_size, info.st_mtime_ns)
    return state


def take_baseline(game, private, report):
    """Copy the game directory to the lane's private directory before the first install.

    Returns (baseline directory, manifest) or None when the copy failed or does not match the
    game; the outcome is recorded under report["quarantine"]["baseline"] either way.
    """
    baseline = private / "game-baseline"
    try:
        copy_tree(game, baseline)
        manifest = tree_manifest(baseline)
        if manifest != tree_manifest(game):
            raise OSError("baseline copy differs from the game directory")
    except (OSError, subprocess.SubprocessError, shutil.Error):
        shutil.rmtree(baseline, ignore_errors=True)
        report["quarantine"]["baseline"] = {"status": "copy_failed"}
        return None
    report["quarantine"]["baseline"] = {
        "status": "copied", "files": sum(kind == "file" for kind, _ in manifest.values()),
        "bytes": sum(size for kind, size in manifest.values() if kind == "file")}
    return baseline, manifest


def restore_game(game, baseline, manifest):
    """Put the game directory back to the baseline in place and verify the file list and sizes."""
    for child in game.iterdir():
        if child.is_symlink() or not child.is_dir():
            child.unlink()
        else:
            shutil.rmtree(child)
    copy_tree(baseline, game)
    if tree_manifest(game) != manifest:
        raise OSError("restored game directory differs from the baseline")


ENGINE_STATE_NAME = re.compile(r"\.modsync-[0-9a-f]{16}")
UNFINISHED_INSTALL = "an unfinished install exists"
TS_UNFINISHED_INSTALL = "unfinished checkpoint"
UNFINISHED_INSTALL_MARKERS = (UNFINISHED_INSTALL, TS_UNFINISHED_INSTALL)
TYPESCRIPT_INSTALL_PHASES = ("base", "widescreen")
TYPESCRIPT_FIRST_CHECKPOINT = "0"
NOTHING_TO_RESTORE = "no unfinished install to restore"
UNCAPTURED_MUTATION = "pre-mutation content was never captured"
ENGINE_RESTORE_SKIP_KEY = "engine_restore_skip"
ENGINE_RESTORE_KEY = "engine_restores"
ENGINE_RESTORE_TIMEOUT = BASELINE_COPY_TIMEOUT
UNFINISHED_INSTALL_REASON = "unfinished_install_not_recovered"
GUI_LANE_GATE_MARKER = "physical installation is unavailable"
GUI_LANE_CONFIGURATION_REASON = "lane_configuration_error"


def engine_state_dir(game):
    """The engine's recovery directory for `game`: a `.modsync-<16 hex of sha256(game path)>` sibling."""
    key = hashlib.sha256(str(game).encode("utf-8", "replace")).hexdigest()
    return game.parent / (".modsync-" + key[:16])


def remove_engine_state(game, protected=()):
    """Delete only this game's `.modsync-<hash>` sibling (snapshot + pending marker); returns what happened.

    Refuses (OSError) anything but a real directory, directly beside the game directory, named
    exactly as the engine names it, that is not one of `protected` or inside/around one.
    """
    state = engine_state_dir(game)
    if state.parent != game.parent or not ENGINE_STATE_NAME.fullmatch(state.name):
        raise OSError("recovery directory is not a sibling of the game directory")
    for path in (game, *protected):
        if state == path or state.is_relative_to(path) or path.is_relative_to(state):
            raise OSError("recovery directory overlaps a protected directory")
    try:
        info = state.lstat()
    except FileNotFoundError:
        return "absent"
    if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
        raise OSError("recovery path is not a plain directory")
    shutil.rmtree(state)
    return "removed"


def restore_arguments(directories):
    """`--game-dir X --source-dir Y` taken from an install's directory arguments, or None."""
    found = []
    for flag in ("--game-dir", "--source-dir"):
        if flag not in directories or directories.index(flag) + 1 >= len(directories):
            return None
        found += [flag, directories[directories.index(flag) + 1]]
    return found


def engine_restore(command, directories, source, private, stage, report, env, invocation=None):
    """Run the CLI's own `restore` for the game; returns the record appended to engine_restores.

    `status` is restored, nothing_pending (the engine had no unfinished install), failed or
    unavailable (no binary or no directory arguments). `invocation` replaces the Rust
    `restore --game-dir X --source-dir Y` arguments (the TypeScript CLI's `checkpoints restore ...`).
    """
    record = {"stage": stage, "status": "unavailable", "seconds": 0.0}
    report["quarantine"].setdefault(ENGINE_RESTORE_KEY, []).append(record)
    arguments = restore_arguments(directories) if invocation is None else invocation
    if arguments is None:
        return record
    log = private / (stage + ".log")
    started = time.monotonic()
    progress(f"{stage}: started")
    status = "failed"
    try:
        completed = live_output.run(
            command + (["restore"] + arguments if invocation is None else arguments), log,
            cwd=source, env=env, stdin=subprocess.DEVNULL, timeout=ENGINE_RESTORE_TIMEOUT)
        text = live_output.private_tail(log)
        if completed.returncode == 0:
            status = "restored"
        elif NOTHING_TO_RESTORE in ANSI_ESCAPE.sub("", text).lower():
            status = "nothing_pending"
        else:
            if invocation is not None and UNCAPTURED_MUTATION in ANSI_ESCAPE.sub("", text).lower():
                record["uncaptured_mutation"] = True
            lines = text.splitlines()[-FAILURE_TAIL_LINES:]
            print(f"::group::{stage} output (last {FAILURE_TAIL_LINES} lines)", file=sys.stderr)
            live_output.emit_lines("\n".join(lines))
            print("::endgroup::", file=sys.stderr)
    except (FileNotFoundError, PermissionError):
        status = "unavailable"
    except (OSError, subprocess.SubprocessError):
        status = "failed"
    record["status"] = status
    record["seconds"] = round(finish_stage(report, stage, started, status), 3)
    return record


def recover_engine_state(command, directories, game, source, private, stage, report, env, protected=()):
    """Clear the engine's unfinished-install state so the next `install` is allowed to start.

    The engine keeps its pre-install snapshot and `pending` marker in `.modsync-<hash>` beside the
    game directory, outside what `restore_game` empties and refills. The engine's own `restore`
    is run first (it also puts the source directory back); when it is unavailable or fails, only
    this game's recovery sibling is removed. Raises OSError when the marker is still there after.
    """
    record = engine_restore(command, directories, source, private, stage, report, env)
    if record["status"] not in ("restored", "nothing_pending"):
        record["fallback"] = "state_" + remove_engine_state(game, protected)
    if (engine_state_dir(game) / "pending").exists():
        raise OSError("the engine's unfinished-install marker is still present")


def checkpoint_session(text):
    """The last `Checkpoint session: <id>` a TypeScript install log printed, or None."""
    found = None
    for line in text.splitlines():
        match = TS_CHECKPOINT_SESSION.match(ANSI_ESCAPE.sub("", line).rstrip())
        if match:
            found = match.group("id")
    return found


def recover_typescript_state(command, game, text, source, private, stage, report, env):
    """Ask the TypeScript CLI to roll the game back to its session's anchor before the baseline restore.

    The TypeScript engine records a pending mutation in `<game>/.modsync/checkpoints` and refuses to
    start another session until it is resolved ("... requires recovery"). Its own recovery is
    `checkpoints restore <session> 0 --kotor <game> --ci-physical-install`, with the session printed
    in the failed install's log. It is best effort, recorded in engine_restores: when it is unavailable
    (no session in the log) or fails, `fallback` says the baseline restore carries the recovery, which
    empties the game directory, and with it the checkpoint store and any restore intent in it.

    When a restore fails because the install changed paths through a patcher or external program whose
    pre-mutation content was never captured, the engine cannot restore this game: the decision is
    cached in `quarantine.engine_restore_skip` and every later restore of the run goes straight to the
    baseline restore (record status `skipped`) after a one-line note. The failed install step itself
    is still quarantined or reported as before; only the restore strategy changes.
    """
    skip = report["quarantine"].get(ENGINE_RESTORE_SKIP_KEY)
    if skip is not None:
        print(f"{stage}: skipped, the engine cannot restore this game ({skip['reason']}, first seen at "
              f"{skip['stage']}); using baseline_restore", file=sys.stderr)
        record = {"stage": stage, "status": "skipped", "seconds": 0.0, "fallback": "baseline_restore"}
        report["quarantine"].setdefault(ENGINE_RESTORE_KEY, []).append(record)
        return record
    session = checkpoint_session(text)
    invocation = None if session is None else [
        "checkpoints", "restore", session, TYPESCRIPT_FIRST_CHECKPOINT, "--kotor", str(game),
        TYPESCRIPT_PHYSICAL_INSTALL_FLAG]
    record = engine_restore(command, [], source, private, stage, report, env, invocation)
    if record["status"] != "restored":
        record["fallback"] = "baseline_restore"
    if record.pop("uncaptured_mutation", False):
        reason = "checkpoint modified paths whose pre-mutation content was never captured"
        report["quarantine"][ENGINE_RESTORE_SKIP_KEY] = {"reason": reason, "stage": stage}
        print(f"::notice::ports: {stage}: {reason}; skipping the engine restore for the rest of this run "
              "and using baseline_restore", file=sys.stderr)
    return record


def install_with_quarantine(command, directories, deselect, converted, game, source, private, report,
                            env, max_iterations=QUARANTINE_MAX_ITERATIONS,
                            max_fraction=QUARANTINE_MAX_FRACTION, implementation="rust",
                            arguments=None, selectable=None, extra=(), before_phase=None):
    """Run `install`; when one component's step stops it, restore the game, omit that component and retry.

    TypeScript (`implementation="typescript"`): each attempt runs `install --phase base` then
    `--phase widescreen` (`install_base`, `install_widescreen`; retries `install_base_retry_N`, ...)
    with `arguments(deselect)` as the directory/selection arguments and `extra` after the phase.
    The TypeScript CLI names the failing instruction by GUID
    (`ERROR: Instruction <guid> failed (PatcherError); installation stopped with recovery pending`)
    after its `[i/N] <Component>` and `  N. <Action>:` progress lines: the component is the one
    owning that instruction GUID in the converted TOML (see `resolve_instruction_failures`), the
    step and action come from the progress lines. The retry restore is `recover_typescript_state`
    then the baseline restore; a failure in either phase restores the whole baseline and reruns
    both phases. `before_phase()` runs before every phase command.

    A copy of the game directory is taken before the first install. A failed install is only
    quarantined when its output names exactly one (component, step) whose action is in
    INSTALL_QUARANTINE_ACTIONS, shows no panic or failed rollback, the game directory is
    populated, the baseline copy exists, and the budget shared with validate still allows it.
    Before every retry the engine's own `restore` runs for the game (its snapshot and `pending`
    marker live in a `.modsync-<hash>` sibling that the game restore does not touch; see
    recover_engine_state), then the game directory is restored in place and verified against the
    baseline, the component and its dependents are appended to `deselect` and the install re-runs
    as `install_retry_N`. A retry the engine still refuses as an unfinished install is reported as
    `unfinished_install_not_recovered` together with the original attributed failure. Anything
    else leaves the lane failed. Every quarantined component is
    listed in report["quarantined_install_failures"] and fails the lane (status "incomplete")
    once the rest is installed.
    """
    typescript = implementation == "typescript"
    components = converted_components(converted)
    if selectable is not None:
        components = [component for component in components
                      if normalized_reference(component.get("Guid")) in selectable]
    policy = report.get("quarantine") or start_quarantine(
        components, deselect, report, max_iterations, max_fraction)
    report.setdefault(INSTALL_QUARANTINE_KEY, [])
    # An exit code alone does not prove an install: the game tree must change.
    before = game_file_state(game)
    started = time.monotonic()
    if policy["max_iterations"] > 0:
        progress("baseline_copy: started")
        baseline = take_baseline(game, private, report)
        finish_stage(report, "baseline_copy", started, (report["quarantine"].get("baseline") or {}).get("status"))
    else:
        baseline = None
    source_dir = directories[directories.index("--source-dir") + 1] if "--source-dir" in directories else None
    protected = (Path(source_dir),) if source_dir else ()
    attempt = 0
    while True:
        suffix = "" if attempt == 0 else f"_retry_{attempt}"
        policy["install_attempts"] = attempt + 1
        stage = "install" + suffix
        try:
            for phase in (TYPESCRIPT_INSTALL_PHASES if typescript else (None,)):
                stage = ("install" if phase is None else "install_" + phase) + suffix
                if before_phase is not None:
                    before_phase()
                if typescript:
                    phase_arguments = arguments(deselect) + ["--phase", phase] + list(extra)
                else:
                    phase_arguments = directories + deselect + ["--non-interactive", "--phase", "all"]
                    if "--patcher-engine" in directories:
                        phase_arguments += ["--component-checkpoints"]
                run(command + ["install"] + phase_arguments, source, private, stage, report, env)
        except Blocked as failure:
            if str(failure) != stage + "_failed":
                raise
            text, code = read_stage_log(private, stage, report)
            record_failure(text, stage, code, report)
            problem = None
            if typescript:
                failures, problem = resolve_instruction_failures(
                    parse_typescript_install_failures(text), components)
                failures = failures or []
            else:
                failures = parse_component_failures(text)
            parsed = failures[-1] if failures else None

            def stop(reason, parsed=parsed, stage=stage, failure=failure):
                quarantine_stop(report, "install", stage, reason, parsed, failure)

            lowered = ANSI_ESCAPE.sub("", text).lower()
            if DISK_EXHAUSTION.search(lowered):
                stop("disk_exhaustion")
            if policy["max_iterations"] <= 0:
                stop("quarantine_disabled")
            if any(marker in lowered for marker in UNFINISHED_INSTALL_MARKERS) and not failures:
                original = next((item for item in reversed(report.get(INSTALL_QUARANTINE_KEY) or ())
                                 if item["kind"] == INSTALL_FAILURE_KIND), None)
                unfinished = next((clean_line(line) for line in text.splitlines()
                                   if any(marker in line.lower() for marker in UNFINISHED_INSTALL_MARKERS)),
                                  None)
                report["failure"].update({
                    "unfinished_install": True, "error_line": unfinished,
                    "original_failure": original and {
                        key: original[key] for key in ("name", "component_guid", "step", "action",
                                                       "error_line", "stage")}})
                quarantine_stop(
                    report, "install", stage, UNFINISHED_INSTALL_REASON, None, failure,
                    details={"error_line": unfinished,
                             "original_failure": report["failure"]["original_failure"],
                             "recovery": (policy.get(ENGINE_RESTORE_KEY) or [None])[-1]})
            if code == 101 or "panicked at" in lowered or "stack backtrace" in lowered:
                stop("panic_detected")
            if "rollback failed" in lowered:
                stop("rollback_failed")
            if problem:
                stop(problem)
            if not failures:
                stop("error_not_attributable")
            if len({(item["component"].casefold(), item["step"]) for item in failures}) > 1:
                stop("multiple_components_failed")
            if parsed["action"].casefold() not in INSTALL_QUARANTINE_ACTIONS:
                stop("action_not_quarantinable")
            if not game_is_populated(game):
                stop("game_missing")
            if baseline is None:
                stop("baseline_copy_failed")

            def restore(parsed=parsed, stage=stage, failure=failure, text=text):
                number = attempt + 1
                try:
                    if typescript:
                        recover_typescript_state(command, game, text, source, private,
                                                 f"engine_restore_{number}", report, env)
                    else:
                        recover_engine_state(command, directories, game, source, private,
                                             f"engine_restore_{number}", report, env, protected)
                    started = time.monotonic()
                    progress(f"baseline_restore_{number}: started")
                    restore_game(game, baseline[0], baseline[1])
                    finish_stage(report, f"baseline_restore_{number}", started, "verified")
                except (OSError, subprocess.SubprocessError, shutil.Error):
                    quarantine_stop(report, "install", stage, "restore_failed", parsed, failure)

            quarantine_failure("install", parsed, stage, components, deselect, game, report, failure,
                               before_commit=restore)
            attempt += 1
            continue
        report.pop("failure", None)
        if baseline is not None:
            shutil.rmtree(baseline[0], ignore_errors=True)
        after = game_file_state(game)
        changed = sum(after.get(path) != state for path, state in before.items()) + len(
            after.keys() - before.keys())
        report["game_files_changed"] = changed
        if not changed:
            raise Blocked("install_succeeded_but_changed_no_game_files")
        return


def load_gui_driver_failure(path):
    """(driver reason, sanitized install failure) of a failed TypeScript GUI driver report.

    `(None, None)` when the report is absent, a symlink or unreadable. The install failure is the
    one `gui_typescript.py` captured from `/api/install/status` and `/api/logs` (error text,
    recovery details, failed instruction and mod, last log entries); it is None for a driver
    failure that did not happen during installation.
    """
    try:
        if path.is_symlink() or not path.is_file():
            return None, None
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None, None
    if not isinstance(data, dict):
        return None, None
    return gui_diag.bounded_text(data.get("reason"), 120), gui_diag.sanitize_install_failure(
        data.get("install_failure"))


GUI_ACTION_NAME = re.compile(r"[A-Za-z]+")
GUI_INSTALLATION_STOPPED = "installation_stopped"
RUST_GUI_STEP = re.compile(r"\s/ step (?P<step>\d+)(?:\s*\((?P<action>[^)]*)\))?")
RUST_GUI_NUMBER = re.compile(r"\d+")
RUST_GUI_PARENTHESIZED = re.compile(r"\(([^)]*)\)")
RUST_GUI_FIELD_LIMIT = 200


def gui_failure_log(install_failure):
    """The lines the TypeScript CLI would have printed for a failure the GUI worker captured.

    `[i/N] <Mod>`, `  N. <Action>:` and `ERROR: <summary.error>` (the web server's install error is
    the CLI's `Instruction <guid> failed (<Code>); installation stopped with recovery pending: ...`
    text), so the GUI failure goes through `parse_typescript_install_failures` unchanged.
    """
    lines = []
    name = install_failure.get("mod_name")
    if name:
        lines.append("[%d/%d] %s" % (int(install_failure.get("mod_index") or 0),
                                     int(install_failure.get("mod_total") or 0), name))
        action = install_failure.get("action") or ""
        if install_failure.get("step") is not None and GUI_ACTION_NAME.fullmatch(action):
            lines.append("  %d. %s: " % (int(install_failure["step"]), action))
    if install_failure.get("error"):
        lines.append("ERROR: " + install_failure["error"])
    return lines


def attribute_gui_install_failure(install_failure, components):
    """(failures, problem): a GUI install failure pinned to a converted component, like the CLI's.

    The failure is attributed only when the web server's install error names an
    `Instruction <guid> failed (<Code>)`: the component is the one that owns that instruction GUID
    in the converted TOML (`resolve_instruction_failures`: ambiguous or mismatching owners refuse),
    and the web UI's own mod GUID must agree with it (`instruction_component_mismatch` otherwise).
    Anything else (no error text, a checkpoint or lock error, a prompt, a missing mod) returns
    ([], None): not attributable, so the caller fails the lane.
    """
    if not install_failure:
        return [], None
    failures = parse_typescript_install_failures("\n".join(gui_failure_log(install_failure)))
    resolved, problem = resolve_instruction_failures(failures, components)
    if problem:
        return [], problem
    mod_guid = install_failure.get("mod_guid")
    for item in resolved:
        if mod_guid and item.get("component_guid") and normalized_reference(mod_guid) != item["component_guid"]:
            return [], "instruction_component_mismatch"
    return resolved, None


def flatten_strings(value, depth=0):
    """Every string in a nested driver report, in order, within the diagnostics depth bound."""
    if isinstance(value, str):
        return [value]
    if depth >= DIAGNOSTICS_MAX_DEPTH:
        return []
    if isinstance(value, list):
        return [text for item in value for text in flatten_strings(item, depth + 1)]
    if isinstance(value, dict):
        return [text for item in value.values() for text in flatten_strings(item, depth + 1)]
    return []


def load_rust_gui_driver_failure(path):
    """Read the bounded Rust GUI failure fields and redacted evidence text."""
    try:
        if path.is_symlink() or not path.is_file():
            return None, None
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None, None
    if not isinstance(data, dict):
        return None, None
    return gui_diag.bounded_text(data.get("reason"), 120), {
        "failed_component": gui_diag.bounded_text(data.get("failed_component"), RUST_GUI_FIELD_LIMIT),
        "failed_step": gui_diag.bounded_text(data.get("failed_step"), RUST_GUI_FIELD_LIMIT),
        "text": "\n".join(flatten_strings(sanitize_diagnostics(data)))}


def rust_gui_failure_fields(failure, components):
    """Return a Rust GUI failure in the shape consumed by quarantine_failure, or None."""
    named = strip_ocr_path_prefix(failure.get("failed_component") or "").strip() or None
    step_field = failure.get("failed_step") or ""
    number = RUST_GUI_NUMBER.search(step_field)
    step = int(number.group()) if number else None
    parenthesized = RUST_GUI_PARENTHESIZED.search(step_field)
    action = parenthesized.group(1).strip() if parenthesized else ""
    names = sorted({str(each.get("Name", each.get("Heading"))) for each in components},
                   key=len, reverse=True)
    error_line = None
    for line in reversed(ANSI_ESCAPE.sub("", failure.get("text") or "").splitlines()):
        match = RUST_GUI_STEP.search(line)
        if match is None:
            continue
        prefix = line[:match.start()].casefold()
        owner = named or next((name for name in names if prefix.endswith(name.casefold())), None)
        if owner is None or not prefix.endswith(owner.casefold()):
            continue
        if step is not None and int(match.group("step")) != step:
            continue
        named, step = owner, int(match.group("step"))
        action = action or (match.group("action") or "").strip()
        error_line = clean_line(" ".join(line.split()))
        break
    if named is None or step is None:
        return None
    if not action:
        match, _ = find_failing_component(components, named)
        records = (match[1].get("Instructions") or []) if match is not None else []
        if 0 < step <= len(records) and isinstance(records[step - 1], dict):
            action = str(records[step - 1].get("Action") or "")
    error_line = error_line or clean_line(f"{named} / step {step}")
    return {"component": named, "step": step, "action": action,
            "message": error_line, "error_line": error_line}


def install_rust_gui_with_quarantine(command, source, private, game, archives, converted, policy_guids,
                                     deselect, report, env, max_iterations=QUARANTINE_MAX_ITERATIONS,
                                     max_fraction=QUARANTINE_MAX_FRACTION, complete_guide=False,
                                     execute_runner=(), profile_receipt=None, patcher=()):
    """Run the Rust GUI driver with the shared bounded install quarantine policy."""
    components = converted_components(converted)
    profile_arguments = []
    if profile_receipt:
        profile_path = private / "gui-install-profile.json"
        profile_path.write_text(json.dumps(profile_receipt, indent=2) + "\n", encoding="utf-8")
        profile_arguments = ["--install-profile-receipt", str(profile_path)]
    policy = report.get("quarantine") or start_quarantine(
        components, deselect, report, max_iterations, max_fraction)
    report.setdefault(INSTALL_QUARANTINE_KEY, [])
    directories = ["--input", str(converted), "--game-dir", str(game), "--source-dir", str(archives)]
    baseline = None
    if policy["max_iterations"] > 0:
        started = time.monotonic()
        progress("baseline_copy: started")
        baseline = take_baseline(game, private, report)
        finish_stage(report, "baseline_copy", started, (policy.get("baseline") or {}).get("status"))
    attempt = 0
    while True:
        stage = gui_stage_name(attempt)
        policy["install_attempts"] = attempt + 1
        skipped = policy_guids | {item["component_guid"] for item in quarantined_entries(report)}
        driver_report = private / gui_report_name(stage)
        gui_work = private / ("gui-work" if attempt == 0 else f"gui-work-{stage}")
        try:
            run([sys.executable, str(Path(__file__).resolve().with_name("gui_rust.py")),
                 "--binary", command[0], "--input", str(converted),
                 "--game-dir", str(game), "--archives", str(archives),
                 "--work-dir", str(gui_work), "--report", str(driver_report),
                 "--timeout", "7000"]
                + (["--complete-guide"] if complete_guide else ["--exclude-widescreen"])
                + list(execute_runner) + list(patcher) + profile_arguments
                + [flag for guid_ in sorted(skipped) for flag in ("--deselect", guid_)],
                source, private, stage, report, env)
        except Blocked as failure:
            if str(failure) != stage + "_failed":
                raise
            recorded = report.setdefault("failure", {
                "stage": stage, "exit_code": None, "component": None, "step": None,
                "action": None, "error_line": None})
            reason, driver_failure = load_rust_gui_driver_failure(driver_report)
            if reason:
                recorded["driver_reason"] = reason
            if driver_failure is None or reason != GUI_INSTALLATION_STOPPED:
                raise
            parsed = rust_gui_failure_fields(driver_failure, components)
            if parsed:
                recorded.update({key: parsed[key] for key in ("component", "step", "action", "error_line")})
            else:
                recorded["error_line"] = clean_line(driver_failure["text"]) or None

            def stop(reason, parsed=parsed, stage=stage, failure=failure):
                quarantine_stop(report, "install", stage, reason, parsed, failure)

            lowered = ANSI_ESCAPE.sub("", driver_failure["text"]).lower()
            if DISK_EXHAUSTION.search(lowered):
                stop("disk_exhaustion")
            if policy["max_iterations"] <= 0:
                stop("quarantine_disabled")
            if "panicked at" in lowered or "stack backtrace" in lowered:
                stop("panic_detected")
            if "rollback failed" in lowered:
                stop("rollback_failed")
            if not parsed:
                stop("error_not_attributable")
            if parsed["action"].casefold() not in INSTALL_QUARANTINE_ACTIONS:
                stop("action_not_quarantinable")
            if not game_is_populated(game):
                stop("game_missing")
            if baseline is None:
                stop("baseline_copy_failed")

            def restore(parsed=parsed, stage=stage, failure=failure):
                number = attempt + 1
                try:
                    recover_engine_state(command, directories, game, source, private,
                                         f"engine_restore_{number}", report, env, (archives,))
                    started = time.monotonic()
                    progress(f"baseline_restore_{number}: started")
                    restore_game(game, baseline[0], baseline[1])
                    finish_stage(report, f"baseline_restore_{number}", started, "verified")
                except (OSError, subprocess.SubprocessError, shutil.Error):
                    quarantine_stop(report, "install", stage, "restore_failed", parsed, failure)

            quarantine_failure("install", parsed, stage, components, deselect, game, report, failure,
                               before_commit=restore)
            print(f"::error::ports: {stage}: installation stopped at '{parsed['component']}', step "
                  f"{parsed['step']} ({parsed['action']}); the game was restored and the component "
                  f"is set aside, so the rest is retried and the lane fails once it is installed",
                  file=sys.stderr)
            attempt += 1
            continue
        report.pop("failure", None)
        if baseline is not None:
            shutil.rmtree(baseline[0], ignore_errors=True)
        return driver_report


def verify_typescript_gui_result(driver_report, selected_input, gui_work, game, reviewed_inputs_unchanged):
    """The TypeScript GUI driver's completion report, after every receipt and output hash is checked."""
    if not driver_report.is_file() or driver_report.is_symlink():
        raise Blocked("gui_completion_report_missing")
    result = json.loads(driver_report.read_text(encoding="utf-8"))
    if (not isinstance(result, dict)
            or result.get("schema_version") != 1
            or result.get("implementation") != "typescript"
            or result.get("mode") != "gui"
            or result.get("runtime") != "playwright-web"
            or result.get("status") != "installed"
            or result.get("reason") != "real_ui_install_and_checkpoint_hashes_verified"
            or result.get("ui_completed") is not True
            or result.get("input_sha256") != digest(selected_input)
            or type(result.get("verified_game_files")) is not int
            or result["verified_game_files"] <= 0):
        raise Blocked("gui_completion_not_verified")
    receipts = result.get("checkpoint_receipts")
    if (not isinstance(receipts, dict) or "session.json" not in receipts
            or len(receipts) < 3 or any(
                not isinstance(checksum, str)
                or re.fullmatch(r"[0-9a-f]{64}", checksum) is None
                for checksum in receipts.values())):
        raise Blocked("gui_checkpoint_receipts_missing")
    evidence = Path(result["evidence_directory"])
    if evidence.is_symlink() or not evidence.resolve().is_relative_to(gui_work.resolve()):
        raise Blocked("gui_evidence_directory_invalid")
    for name, key in (("install-status.json", "receipt_sha256"),
                      ("verified-output-hashes.json", "output_manifest_sha256")):
        artifact = evidence / name
        if (artifact.is_symlink() or not artifact.is_file()
                or digest(artifact) != result.get(key)):
            raise Blocked("gui_completion_hash_mismatch")
    outputs = json.loads((evidence / "verified-output-hashes.json").read_text())
    if not isinstance(outputs, dict) or len(outputs) != result["verified_game_files"]:
        raise Blocked("gui_output_manifest_invalid")
    for relative, checksum in outputs.items():
        target = game / relative
        if (Path(relative).is_absolute() or ".." in Path(relative).parts
                or target.is_symlink() or not target.resolve().is_relative_to(game)
                or not target.is_file() or digest(target) != checksum):
            raise Blocked("gui_output_hash_mismatch")
    reviewed_inputs_unchanged()
    return result


def install_gui_with_quarantine(command, source, private, game, archives, converted, selected_input,
                                policy_guids, deselect, selectable, report, env,
                                max_iterations=QUARANTINE_MAX_ITERATIONS,
                                max_fraction=QUARANTINE_MAX_FRACTION, reviewed_inputs_unchanged=None,
                                choices=None):
    """Run the TypeScript GUI driver; when one component's install step stops it, restore, omit it and retry.

    The same loop as `install_with_quarantine`, with the GUI driver as the install: stages `gui`,
    `gui_retry_1`, ... against the one budget shared with validate and the CLI lane, one baseline
    copy of the game taken before the first attempt, and the same refusals. The failure is read
    from the driver report's `install_failure` (the web server's install error, recovery details,
    failed instruction and mod, last log entries; see `attribute_gui_install_failure`), so only a
    driver failure with reason `backend_install_gate_or_failure` whose error names an
    `Instruction <guid> failed (<Code>)` of exactly one converted component, with an action in
    INSTALL_QUARANTINE_ACTIONS, is quarantined. Before each retry the CLI's own `checkpoints
    restore` for the failed session runs, then the game directory is restored to the baseline and
    verified, and the component plus its dependents are unselected in a new copy of the input
    (`converted-gui_retry_N.toml`). Any other driver failure (no install failure captured, a
    prompt, a missing game, a failed rollback, a budget cap) is re-raised or recorded in
    report["quarantine"]["stopped"]; nothing is skipped silently. Every quarantined component is
    listed in report["quarantined_install_failures"] and fails the lane (status "incomplete")
    once the rest is installed. Returns the
    verified driver report of the attempt that installed.

    `choices` is the reviewed manifest's flat `["--choice", "GUID=ANSWER", ...]` list; it is appended
    to every driver command, and the driver answers only a prompt whose instruction GUID has an
    entry matching one of its choices (see `gui_typescript.py`); any other prompt stops it.
    """
    components = [component for component in converted_components(converted)
                  if normalized_reference(component.get("Guid")) in selectable]
    policy = report.get("quarantine") or start_quarantine(
        components, deselect, report, max_iterations, max_fraction)
    report.setdefault(INSTALL_QUARANTINE_KEY, [])
    reviewed_inputs_unchanged = reviewed_inputs_unchanged or (lambda: None)
    baseline = None
    if policy["max_iterations"] > 0:
        started = time.monotonic()
        progress("baseline_copy: started")
        baseline = take_baseline(game, private, report)
        finish_stage(report, "baseline_copy", started, (policy.get("baseline") or {}).get("status"))
    attempt = 0
    while True:
        stage = gui_stage_name(attempt)
        policy["install_attempts"] = attempt + 1
        if attempt:
            omitted = policy_guids | {item["component_guid"] for item in quarantined_entries(report)}
            selected_input = private / f"converted-{stage}.toml"
            report["selected_after_policy_count"] = select_base_components(
                converted, omitted, selected_input)
        reviewed_inputs_unchanged()
        driver_report = private / gui_report_name(stage)
        gui_work = private / ("gui-work" if attempt == 0 else f"gui-work-{stage}")
        try:
            run([sys.executable, str(Path(__file__).resolve().with_name("gui_typescript.py")),
                 "--source", str(source), "--input", str(selected_input),
                 "--game-dir", str(game), "--archives", str(archives),
                 "--work-dir", str(gui_work), "--report", str(driver_report),
                 "--timeout", "7000", *(choices or [])],
                source, private, stage, report, env)
        except Blocked as failure:
            if str(failure) != stage + "_failed":
                raise
            recorded = report.setdefault("failure", {
                "stage": stage, "exit_code": None, "component": None, "step": None,
                "action": None, "error_line": None})
            reason, install_failure = load_gui_driver_failure(driver_report)
            if reason:
                recorded["driver_reason"] = reason
            if install_failure is None or reason != GUI_INSTALL_FAILURE_REASON:
                progress(f"{stage}: driver failure ({reason or 'no driver report'}) is not an "
                         f"attributable install failure; not quarantined")
                raise
            report.setdefault(GUI_INSTALL_FAILURES_KEY, []).append({"stage": stage, **install_failure})
            failures, problem = attribute_gui_install_failure(install_failure, components)
            parsed = failures[-1] if failures else None
            error_line = clean_line(" ".join((install_failure.get("error") or "").split()))
            if parsed:
                recorded.update({key: parsed[key] for key in ("component", "step", "action", "error_line")})
                recorded["instruction_guid"] = parsed.get("instruction_guid")
                progress(f"{stage} failed at component '{parsed['component']}', step {parsed['step']} "
                         f"({parsed['action']}), instruction {parsed.get('instruction_guid')}")
            else:
                recorded["error_line"] = error_line or None
                progress(f"{stage} failed with an install error that names no converted component: "
                         f"{error_line or 'no error text captured'}")

            def stop(reason, parsed=parsed, stage=stage, failure=failure, error_line=error_line):
                quarantine_stop(report, "install", stage, reason, parsed, failure,
                                details=None if parsed else {"error_line": error_line})

            lowered = ANSI_ESCAPE.sub("", install_failure.get("error") or "").lower()
            if DISK_EXHAUSTION.search(lowered):
                stop("disk_exhaustion")
            if policy["max_iterations"] <= 0:
                stop("quarantine_disabled")
            if any(marker in lowered for marker in UNFINISHED_INSTALL_MARKERS) and not failures:
                original = next((item for item in reversed(report.get(INSTALL_QUARANTINE_KEY) or ())
                                 if item["kind"] == INSTALL_FAILURE_KIND), None)
                recorded.update({
                    "unfinished_install": True, "error_line": error_line,
                    "original_failure": original and {
                        key: original[key] for key in ("name", "component_guid", "step", "action",
                                                       "error_line", "stage")}})
                quarantine_stop(
                    report, "install", stage, UNFINISHED_INSTALL_REASON, None, failure,
                    details={"error_line": error_line,
                             "original_failure": recorded["original_failure"],
                             "recovery": (policy.get(ENGINE_RESTORE_KEY) or [None])[-1]})
            if "rollback failed" in lowered:
                stop("rollback_failed")
            if problem:
                stop(problem)
            if not failures and GUI_LANE_GATE_MARKER in " ".join(lowered.split()):
                stop(GUI_LANE_CONFIGURATION_REASON)
            if not failures:
                stop("error_not_attributable")
            if parsed["action"].casefold() not in INSTALL_QUARANTINE_ACTIONS:
                stop("action_not_quarantinable")
            if not game_is_populated(game):
                stop("game_missing")
            if baseline is None:
                stop("baseline_copy_failed")

            def restore(parsed=parsed, stage=stage, failure=failure, install_failure=install_failure):
                number = attempt + 1
                session = install_failure.get("checkpoint_session_id") or ""
                text = f"Checkpoint session: {session}\n" if re.fullmatch(r"[0-9A-Za-z-]+", session) else ""
                try:
                    recover_typescript_state(command, game, text, source, private,
                                             f"engine_restore_{number}", report, env)
                    started = time.monotonic()
                    progress(f"baseline_restore_{number}: started")
                    restore_game(game, baseline[0], baseline[1])
                    finish_stage(report, f"baseline_restore_{number}", started, "verified")
                except (OSError, subprocess.SubprocessError, shutil.Error):
                    quarantine_stop(report, "install", stage, "restore_failed", parsed, failure)

            quarantine_failure("install", parsed, stage, components, deselect, game, report, failure,
                               before_commit=restore)
            attempt += 1
            continue
        result = verify_typescript_gui_result(
            driver_report, selected_input, gui_work, game, reviewed_inputs_unchanged)
        report.pop("failure", None)
        if baseline is not None:
            shutil.rmtree(baseline[0], ignore_errors=True)
        return result


LANE_ERRORS_KEY = "lane_errors"
OMITTED_KEY = "omitted_components"


def lane_errors(report):
    """What keeps an otherwise finished lane from passing.

    Every component that was not installed for a reason other than an unobtainable archive or a
    guide-sanctioned skip (error-severity deselections and their dependents, and validate or install
    quarantines), plus every component the archive binding left unresolved that the lane still
    installed: such a component may install from its prose draft, but not from a layout the
    converter resolved. A component that was not installed at all (deselected with any severity,
    quarantined, or a widescreen component the Rust GUI lane leaves out) is never counted as an
    unresolved binding.
    """
    skipped = [record for record in report.get(SKIPPED_REPORT_KEY) or () if record.get("component_guid")]
    deselected = [record for record in skipped if record.get("severity", SEVERITY_ERROR) == SEVERITY_ERROR]
    quarantined = quarantined_entries(report)
    excluded = {record["component_guid"] for record in deselected} | {
        item["component_guid"] for item in quarantined}
    gui_excluded = (report.get("gui_not_covered") or {}).get("excluded_widescreen") or ()
    not_installed = excluded | {record["component_guid"] for record in skipped} | {
        item["component_guid"] for item in gui_excluded if item.get("component_guid")}
    unresolved = sorted({item["component_guid"] for item in report.get(BIND_DIAGNOSTICS_KEY) or ()
                         if item.get("component_guid") and item["component_guid"] not in not_installed})
    return {"count": len(excluded) + len(unresolved), "not_installed": len(excluded),
            "deselected": len(deselected), "quarantined": len(quarantined),
            "bind_unresolved": len(unresolved), "bind_unresolved_guids": unresolved}


def omitted_components(report):
    """Components left out, with a warning, because their archive is not obtainable.

    Warning-severity deselections from the restore report (cache.py restore: reviewed
    uncached-by-policy steps and --deselect-unavailable steps) and their dependents. Leaving them
    out does not fail the lane, but the result is PARTIAL, not a full install: report status
    "partial" and report["partial"] true, as in manual.py. A warning lane skip (a step the guide
    itself says not to apply in this setup) is to spec and is not counted.
    """
    return [{"component_guid": record["component_guid"], "heading": record["heading"],
             "source": record["source"], "reason": record["reason"]}
            for record in report.get(SKIPPED_REPORT_KEY) or ()
            if record.get("component_guid")
            and record.get("severity", SEVERITY_ERROR) == SEVERITY_WARNING
            and record.get("source") in OMISSION_SOURCES]


def print_diagnostics(report):
    """Put the skip causes and every uncovered step in the CI log so one read is enough."""
    durations = report.get(STAGE_DURATIONS_KEY)
    if durations:
        print("ports: stage durations: " + ", ".join(
            f"{stage} {format_duration(seconds)}" for stage, seconds in durations.items())
            + f"; total {format_duration(sum(durations.values()))}", file=sys.stderr)
    aliased = report.get("archive_aliases")
    if aliased and aliased.get("count"):
        print(f"ports: archive aliases created for {aliased['count']} Extract source(s) "
              f"(restored archives linked under the names the converted globs match)", file=sys.stderr)
    for item in report.get("alias_unresolved") or ():
        print(f"::warning::ports: no archive alias for {item['component']} [{item['component_guid']}]: "
              f"{item['reason']}; validate will report its download as missing", file=sys.stderr)
    review = report.get("draft_review")
    if review and (review.get("bound") or review.get("bind_failed")):
        print(f"ports: archive binding bound {review.get('bound', 0)} component(s)"
              + (f"; not used: {review['bind_failed']}" if review.get("bind_failed") else ""),
              file=sys.stderr)
    if review and ("cleared" in review or "unreviewed" in review):
        print(f"ports: verified-draft review cleared {review.get('cleared', 0)} component(s); "
              f"left fenced: {len(review.get('skipped', []))}", file=sys.stderr)
        for item in review.get("unreviewed", []):
            print(f"ports: unreviewed draft still selected: {item['component_guid']} "
                  f"({item['heading']})", file=sys.stderr)
    skips = report.get("convert_skips")
    if skips:
        causes = ", ".join(f"{cause}={count}" for cause, count in sorted(skips["by_cause"].items())
                           if cause != POLICY_CAUSE)
        policy = skips["by_cause"].get(POLICY_CAUSE)
        suffix = f"; reviewed-policy-skipped={policy}" if policy else ""
        print(f"ports: convert generated instructions for {skips['generated']} of {skips['total']}"
              f" components; not generated: {causes or 'none'}{suffix}", file=sys.stderr)
    acceptance = report.get(DRAFT_ACCEPTANCE_KEY)
    if acceptance:
        print(f"ports: generated drafts accepted for install: {len(acceptance['accepted'])}; "
              f"not installable: {len(acceptance['refused'])}", file=sys.stderr)
    for record in report.get(SKIPPED_REPORT_KEY) or ():
        if not record.get("component_guid"):
            continue
        detail = record["reason"]
        if record.get("bind_diagnostic_reason"):
            detail += f"; converter diagnostic: {record['bind_diagnostic_reason']}"
        if record.get("severity", SEVERITY_ERROR) == SEVERITY_ERROR:
            print(f"::error::ports: NOT INSTALLED {record['heading']} [{record['component_guid']}] "
                  f"({record['source']}): {detail}", file=sys.stderr)
        else:
            print(f"::warning::ports: DESELECTED {record['heading']} [{record['component_guid']}] "
                  f"({record['source']}): {detail}", file=sys.stderr)
    for item in report.get(UNAPPLIED_BIND_SKIPS_KEY) or ():
        outcome = ("the converter now gives it instructions, so it is NOT skipped; remove the entry"
                   if item["state"] == "stale"
                   else "no matching bind diagnostic was recorded, so the coverage gap stands")
        print(f"::warning::ports: lane skip for {item['heading']} [{item['component_guid']}] "
              f"({item['bind_diagnostic']}) not applied: {outcome}", file=sys.stderr)
    for phase, label, outcome in (
            ("validate", "validate failures", "not installed and not verified"),
            ("install", "install failures", "omitted from the retried install and not verified")):
        settings = QUARANTINE_PHASES[phase]
        quarantined = report.get(settings["key"])
        if not quarantined:
            continue
        failing = [item for item in quarantined if item["kind"] == settings["kind"]]
        print(f"::error::ports: {len(quarantined)} component(s) QUARANTINED after {label} "
              f"({len(failing)} failing, {len(quarantined) - len(failing)} dependent(s)): they are "
              f"{outcome}; the rest of the guide was installed", file=sys.stderr)
        for item in quarantined:
            if item["kind"] == settings["kind"]:
                at_step = f" at step {item['step']}" if item["step"] is not None else ""
                print(f"::error::ports: quarantined {item['name']} [{item['component_guid']}]: "
                      f"{phase} failed{at_step} ({item['action']}): {item['error_line']}",
                      file=sys.stderr)
            else:
                print(f"::error::ports: quarantined {item['name']} [{item['component_guid']}]: "
                      f"depends on quarantined {item['caused_by']}", file=sys.stderr)
    for item in report.get(GUI_INSTALL_FAILURES_KEY) or ():
        print("\n".join(gui_diag.render_install_failure(
            {key: value for key, value in item.items() if key not in ("stage", "log_tail")},
            title=f"{item['stage']} install failure (summary; the log tail is in the gui worker output above)")),
            file=sys.stderr)
    for item in (report.get("quarantine") or {}).get(ENGINE_RESTORE_KEY) or ():
        if item["status"] not in ("restored", "nothing_pending", "skipped"):
            print(f"::warning::ports: {item['stage']}: engine restore {item['status']}"
                  + (f", fell back to {item['fallback']}" if item.get("fallback") else ""), file=sys.stderr)
    baseline = (report.get("quarantine") or {}).get("baseline")
    if baseline and baseline.get("status") == "copy_failed":
        print("::warning::ports: the game directory baseline copy failed, so an install failure "
              "could not be quarantined (no restore point)", file=sys.stderr)
    stopped = (report.get("quarantine") or {}).get("stopped")
    if stopped:
        print(f"::error::ports: {stopped.get('phase', 'validate')} failure not quarantined "
              f"({stopped['reason']})"
              + (f": {stopped['error_line']}" if stopped.get("error_line") else ""), file=sys.stderr)
        if stopped.get("attributed_components"):
            print(f"::error::ports: the {stopped.get('stage', 'validate')} pass attributed "
                  f"{stopped['attributed_count']} component(s) against a component cap of "
                  f"{stopped['component_cap']} ({stopped['already_quarantined']} already quarantined); "
                  f"none were quarantined: " + ", ".join(repr(name) for name in stopped["attributed_components"]),
                  file=sys.stderr)
    failure = report.get("failure")
    if failure and report.get("status") not in PASSING_STATUSES:
        where = failure_location(failure)
        if failure.get("driver_reason"):
            where += f" [driver reason: {failure['driver_reason']}]"
        if failure.get("error_line") and not failure.get("component"):
            where += f" [error: {failure['error_line']}]"
        print(f"::error::ports: {failure['stage']} failed (exit {failure['exit_code']}): {where}",
              file=sys.stderr)
        if failure.get("unfinished_install"):
            original = failure.get("original_failure")
            cause = (f"the original failure was component '{original['name']}', step {original['step']} "
                     f"({original['action']}) in {original['stage']}: {original['error_line']}"
                     if original else "no earlier attributed failure was recorded")
            print(f"::error::ports: {failure['stage']} was refused as an unfinished install, not a new "
                  f"component failure; the engine's recovery state was not cleared before the retry; "
                  f"{cause}", file=sys.stderr)
    uncovered = report.get("steps_without_instructions")
    if uncovered:
        print(f"::error::ports: {len(uncovered)} guide steps have no instructions", file=sys.stderr)
        for step in uncovered:
            print(f"  - {step['heading']} [{step['component_guid']}] "
                  f"{step['skip_reason'] or 'no convert skip reason recorded'} "
                  f"<{' '.join(step['urls'])}>", file=sys.stderr)
            if step.get("bind_diagnostic"):
                print(f"    bind diagnostic: {step['bind_diagnostic']}", file=sys.stderr)
    errors = report.get(LANE_ERRORS_KEY)
    if errors and errors["count"]:
        print(f"::error::ports: the guide was NOT installed to spec: {errors['not_installed']} component(s) "
              f"not installed ({errors['deselected']} deselected because of errors, {errors['quarantined']} "
              f"quarantined), {errors['bind_unresolved']} component(s) with an unresolved archive binding; "
              f"every other component was installed", file=sys.stderr)
    if report.get("status") == "partial":
        print(f"::warning title=partial install::ports: finished PARTIAL: "
              f"{len(report.get(OMITTED_KEY) or ())} component(s) not installed because their "
              f"archive is not obtainable (see report {OMITTED_KEY}); every other component was "
              f"installed, but this is not a full install of the guide", file=sys.stderr)


IMPLEMENTATION_SECTIONS = {
    "rust": frozenset(),
    "typescript": frozenset({"CRITICAL: PC Version Differences & Fixes"}),
}


TYPESCRIPT_PHYSICAL_INSTALL_FLAG = "--ci-physical-install"
TYPESCRIPT_GAMES = {"k1": "kotor1", "k2": "kotor2"}
SKIPPED_BY_POLICY = "skipped-by-reviewed-policy"
SKIPPED_REPORT_KEY = "skipped_by_reviewed_policy"
LANE_SKIPS_PATH = Path(__file__).with_name("archive_selections.json")
LANE_SKIP_GAMES = ("k1", "k2")
LANE_SKIP_KEYS = frozenset({"section", "heading", "reason"})
LANE_SKIP_LANES = ("typescript", "rust")
LANE_SKIP_BIND_DIAGNOSTICS = ("loose_file_unresolved", "patcher_unresolved")
UNAPPLIED_BIND_SKIPS_KEY = "unapplied_bind_skips"
# Severity of a deselected component. Only an archive no source can provide (or one the cache
# never holds by policy) and a step the guide itself says not to apply in this setup are warnings;
# every other component that is not installed fails the lane once everything else is installed.
SEVERITY_WARNING = "warning"
SEVERITY_ERROR = "error"
SEVERITIES = (SEVERITY_WARNING, SEVERITY_ERROR)
UNAVAILABLE_SOURCE = "archive_unavailable"
RESTORE_POLICY_SOURCE = "restore_report"
# Sources whose (warning) deselections make a passing lane "partial" (omitted_components).
OMISSION_SOURCES = (RESTORE_POLICY_SOURCE, UNAVAILABLE_SOURCE)
# Lane statuses that exit 0: everything installed, or everything but unobtainable archives.
PASSING_STATUSES = ("installed", "partial")
NO_INSTRUCTIONS_SOURCE = "no_instructions"
DRAFT_REFUSED_SOURCE = "draft_not_acceptable"
DEPENDENT_KIND = "dependent_of_deselected"
NOT_INSTALLED = "not-installed"


def load_policy_skips(path, guide):
    """Read cache.py restore --report and return the guide steps it says cannot be installed.

    `skipped_steps` are the reviewed uncached-by-policy steps; `unavailable_steps` (cache.py
    restore --deselect-unavailable) are steps whose archive no source could provide. Each step is
    {guide, section, heading, url, reason}; steps naming the same section and heading are merged
    into one record carrying every URL. Both are warnings: the archive is not obtainable.
    """
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict) or document.get("ok") is not True:
        raise Blocked("restore_report_not_ok")
    merged = {}
    for key, source in (("skipped_steps", RESTORE_POLICY_SOURCE), ("unavailable_steps", UNAVAILABLE_SOURCE)):
        steps = document.get(key, [])
        if not isinstance(steps, list):
            raise Blocked("restore_report_skipped_steps_invalid")
        for step in steps:
            if (not isinstance(step, dict)
                    or any(not isinstance(step.get(name), str) or not step[name].strip()
                           for name in ("guide", "section", "heading", "url", "reason"))):
                raise Blocked("restore_report_skipped_step_invalid")
            if Path(step["guide"]).resolve() != guide:
                raise Blocked("restore_report_guide_mismatch")
            record = merged.setdefault((step["section"], step["heading"]), {
                "section": step["section"], "heading": step["heading"],
                "reason": step["reason"], "urls": [], "source": source,
                "severity": SEVERITY_WARNING})
            if step["url"] not in record["urls"]:
                record["urls"].append(step["url"])
    return list(merged.values())


def load_lane_skips(game, path=None, implementation="rust"):
    """Return the reviewed lane skips for one game from archive_selections.json.

    The `lane_skips` table maps k1/k2 to a list of {section, heading, reason} objects. An
    optional `severity` (SEVERITIES, default "error") says whether leaving the step out fails the
    lane: "warning" is only for a step the guide itself says not to apply to this game setup.
    An error-severity skip still deselects the component (and its dependents) so the rest of the
    guide installs, and then fails the lane. An entry with `"lane": "rust"` and a `bind_diagnostic`
    (see LANE_SKIP_BIND_DIAGNOSTICS) is a Rust-only skip that coverage applies only while the
    archive binding stage recorded that diagnostic for the component and the component still has
    no instructions (see bind_skip_state). The whole table is validated whatever the game, so a
    malformed entry for the other game still blocks. A missing file or table skips nothing. An
    entry with `"lane": "typescript"` applies to the TypeScript lanes alone.
    """
    path = Path(path or LANE_SKIPS_PATH)
    if not path.is_file():
        return []
    document = json.loads(path.read_text(encoding="utf-8"))
    table = document.get("lane_skips", {}) if isinstance(document, dict) else None
    if not isinstance(table, dict) or not table.keys() <= set(LANE_SKIP_GAMES):
        raise Blocked("lane_skips_invalid")
    result = []
    for name, entries in table.items():
        if not isinstance(entries, list):
            raise Blocked("lane_skips_invalid")
        seen = {"rust": set(), "typescript": set()}
        for entry in entries:
            if (not isinstance(entry, dict) or not LANE_SKIP_KEYS <= entry.keys()
                    or not entry.keys() <= LANE_SKIP_KEYS | {"lane", "bind_diagnostic", "severity"}
                    or any(not isinstance(entry[key], str) or not entry[key].strip()
                           or entry[key] != entry[key].strip()
                           for key in entry.keys() & LANE_SKIP_KEYS)):
                raise Blocked("lane_skips_invalid")
            if "lane" in entry and entry["lane"] not in LANE_SKIP_LANES:
                raise Blocked("lane_skips_invalid")
            if entry.get("severity", SEVERITY_ERROR) not in SEVERITIES:
                raise Blocked("lane_skips_invalid")
            if "bind_diagnostic" in entry and (entry["bind_diagnostic"] not in LANE_SKIP_BIND_DIAGNOSTICS
                                               or entry.get("lane") != "rust"):
                raise Blocked("lane_skips_invalid")
            lanes = {entry["lane"]} if "lane" in entry else {"rust", "typescript"}
            key = (entry["section"], entry["heading"])
            if any(key in seen[lane] for lane in lanes):
                raise Blocked("lane_skips_duplicate")
            for lane in lanes:
                seen[lane].add(key)
            if name == game and implementation in lanes:
                skip = {"section": entry["section"], "heading": entry["heading"],
                        "reason": entry["reason"], "urls": [], "source": "lane_skips",
                        "severity": entry.get("severity", SEVERITY_ERROR)}
                if "bind_diagnostic" in entry:
                    skip["bind_diagnostic"] = entry["bind_diagnostic"]
                result.append(skip)
    return result


def normalized_reference(value):
    """A GUID reference in the lowercase hyphenated form, whether written {BRACED} or bare."""
    try:
        return str(uuid.UUID(str(value)))
    except ValueError:
        return str(value).lower()


def canonical_guid(value):
    """True when value is a lowercase hyphenated GUID, the form the reports and --deselect use."""
    try:
        return isinstance(value, str) and str(uuid.UUID(value)) == value
    except ValueError:
        return False


def bind_skip_state(report, identity, kind, component):
    """Whether a lane skip bound to a bind diagnostic applies, and the diagnostic that confirms it.

    "confirmed": the binding stage recorded this diagnostic kind for the component and it still has
    no instructions. "stale": it has instructions (the converter now binds it, so the skip would drop
    an installable component). "unconfirmed": no instructions and no such diagnostic, so the gap is
    not the one that was reviewed and coverage must still fail.
    """
    if isinstance(component, dict) and component.get("Instructions"):
        return "stale", None
    for item in (report or {}).get(BIND_DIAGNOSTICS_KEY) or ():
        if item.get("component_guid") == identity and item.get("kind") == kind:
            return "confirmed", item["reason"]
    return "unconfirmed", None


def merge_lane_skips(restore_skips, lane_skips):
    """Restore-report skips first; a lane skip naming the same step adds nothing."""
    known = {(skip["section"], skip["heading"]) for skip in restore_skips}
    return list(restore_skips) + [skip for skip in lane_skips
                                  if (skip["section"], skip["heading"]) not in known]


def uncovered_steps(report, headings, components, indexes):
    """Heading, GUID, links and the converter's skip reason for each step lacking instructions."""
    reasons = {item["component_guid"]: item["reason"]
               for item in report.get("convert_skips", {}).get("components", [])}
    diagnostics = {}
    for item in report.get(BIND_DIAGNOSTICS_KEY) or ():
        if item.get("component_guid"):
            diagnostics.setdefault(item["component_guid"], item["reason"])
    steps = []
    for index in indexes:
        component = components[index] if isinstance(components[index], dict) else {}
        identity = str(component.get("Guid", "")).lower()
        step = {"heading": headings[index], "component_guid": identity,
                "urls": [urldefrag(url).url for url in component.get("ResourceRegistry", {})],
                "skip_reason": reasons.get(identity)}
        if identity in diagnostics:
            step["bind_diagnostic"] = diagnostics[identity]
        steps.append(step)
    return steps


def guide_step_list(text, implementation="rust"):
    """The counted steps of a guide: (section, heading) in order, every step, and per-section counts."""
    if implementation not in IMPLEMENTATION_SECTIONS:
        raise Blocked("implementation_unknown")
    allowed_sections = {
        "Mod List",
        "Optional Widescreen & Extended Widescreen Support Mods",
        "Enhanced Widescreen Support Mods",
    } | IMPLEMENTATION_SECTIONS[implementation]
    section = None
    steps = []
    guide_steps = set()
    sections = {}
    for line in text.splitlines():
        if line.startswith("# "):
            section = None
        elif line.startswith("## "):
            section = line[3:].strip()
            if section in allowed_sections:
                sections.setdefault(section, 0)
        elif line.startswith("### "):
            guide_steps.add((section, line[4:].strip()))
            if section in allowed_sections:
                steps.append((section, line[4:].strip()))
                sections[section] += 1
    return steps, guide_steps, sections


def coverage(guide, converted, report=None, implementation="rust", skipped=()):
    text = guide.read_text(encoding="utf-8")
    steps, guide_steps, sections = guide_step_list(text, implementation)
    headings = [heading for _, heading in steps]
    if "Mod List" not in sections:
        raise Blocked("unsupported_guide_structure")
    document = tomllib.loads(converted.read_text(encoding="utf-8"))
    components = document.get("thisMod", document.get("Components", []))
    if report is not None:
        report["source_component_count"] = len(headings)
        report["source_section_counts"] = sections
        report["converted_component_count"] = (
            len(components) if isinstance(components, list) else None
        )
    if not headings or not isinstance(components, list) or len(components) != len(headings):
        raise Blocked("guide_component_coverage_mismatch")
    exempt = set()
    records = []
    unapplied = []
    for skip in skipped:
        key = (skip["section"], skip["heading"])
        if key not in guide_steps:
            raise Blocked("policy_skipped_step_not_in_guide")
        record = {"status": SKIPPED_BY_POLICY, "severity": SEVERITY_ERROR, **skip,
                  "component_guid": None}
        if key in steps:
            if steps.count(key) != 1:
                raise Blocked("policy_skipped_step_ambiguous")
            index = steps.index(key)
            component = components[index]
            if not isinstance(component, dict) or not component.get("Guid"):
                raise Blocked("policy_skipped_component_unidentified")
            identity = guid(component["Guid"])
            record["component_guid"] = identity
            if "bind_diagnostic" in skip:
                state, reason = bind_skip_state(report, identity, skip["bind_diagnostic"], component)
                if state != "confirmed":
                    unapplied.append({"section": skip["section"], "heading": skip["heading"],
                                      "component_guid": identity,
                                      "bind_diagnostic": skip["bind_diagnostic"], "state": state})
                    continue
                record["bind_diagnostic_reason"] = reason
            exempt.add(index)
        elif "bind_diagnostic" in skip:
            raise Blocked("policy_skipped_guid_unmatched")
        records.append(record)
    skipped_guids = {record["component_guid"] for record in records
                     if record["component_guid"] is not None}
    uncovered = [index for index, component in enumerate(components)
                 if index not in exempt and (not isinstance(component, dict)
                                             or not component.get("Instructions"))]
    if uncovered:
        details = uncovered_steps(report or {}, headings, components, uncovered)
        if report is not None:
            report["steps_without_instructions"] = details
        for index, step in zip(uncovered, details):
            if not step["component_guid"]:
                raise Blocked("guide_step_without_instructions_unidentified")
            records.append({"status": NOT_INSTALLED, "severity": SEVERITY_ERROR,
                            "section": steps[index][0], "heading": step["heading"],
                            "reason": "the converter generated no instructions"
                                      + (f": {step['skip_reason']}" if step["skip_reason"] else ""),
                            "urls": step["urls"], "source": NO_INSTRUCTIONS_SOURCE,
                            "component_guid": guid(step["component_guid"])})
            exempt.add(index)
    cascade_dependents(components, records)
    if any(component.get("Heading", component.get("Name")) != heading
           for component, heading in zip(components, headings)):
        raise Blocked("guide_component_order_or_heading_mismatch")
    if report is not None:
        report[SKIPPED_REPORT_KEY] = records
        if unapplied:
            report[UNAPPLIED_BIND_SKIPS_KEY] = unapplied
        reclassify_policy_skips(report, skipped_guids)
    return len(components)


def cascade_dependents(components, records, start=0):
    """Deselect, in place, every component that depends on a component records[start:] deselects.

    The guide's Dependencies form a DAG; a component whose dependency is not installed cannot be
    installed either. Each dependent is appended once (to the first root that reaches it,
    directly or through other dependents) with that root's severity, so a dependent of a missing
    download is a warning and a dependent of a failure is an error.
    """
    components = [component for component in components if isinstance(component, dict)]
    taken = {record["component_guid"] for record in records if record.get("component_guid")}
    for record in list(records[start:]):
        root = record.get("component_guid")
        if not root:
            continue
        for identity, dependent in transitive_dependents(components, root, taken).items():
            taken.add(identity)
            records.append({
                "status": record["status"], "severity": record["severity"],
                "section": None, "heading": dependent.get("Heading", dependent.get("Name")),
                "reason": f"depends on {record['heading']}, which is not installed ({record['reason']})",
                "urls": [], "source": record["source"], "kind": DEPENDENT_KIND,
                "caused_by": root, "component_guid": identity})


def gui_widescreen_scope(converted, policy_guids):
    """Widescreen-only components the Rust GUI lane does not install, and whether any exist.

    The GUI driver has a single install phase. Widescreen-only components that
    are not already reviewed-policy skips are excluded and reported as not
    covered; this mirrors gui_rust.plan_selection, which re-derives it.
    """
    document = tomllib.loads(converted.read_text(encoding="utf-8"))
    components = document.get("thisMod", document.get("Components", []))
    widescreen = [component for component in components
                  if isinstance(component, dict) and component.get("WidescreenOnly") is True]
    excluded = [{"component_guid": str(component.get("Guid", "")).lower(),
                 "heading": component.get("Heading", component.get("Name"))}
                for component in widescreen
                if str(component.get("Guid", "")).lower() not in policy_guids]
    return excluded, bool(widescreen)


def component_phase(component):
    """The install phase a converted component belongs to: widescreen or base."""
    return "widescreen" if component.get("IsWidescreen", False) else "base"


def select_base_components(converted, deselect=(), destination=None):
    """Write IsSelected=true for every base-phase, platform-neutral component.

    Widescreen and Aspyr-only components stay unselected: the GUI driver
    refuses them, and the CLI installs them through its own phase flow.
    Components named in `deselect` (GUIDs) are also written unselected. The
    result replaces `converted`, or is written to `destination` when given.
    """
    deselect = {normalized_reference(each) for each in deselect}
    text = converted.read_text(encoding="utf-8")
    document = tomllib.loads(text)
    key = "thisMod" if "thisMod" in document else "Components"
    components = document.get(key)
    if not isinstance(components, list) or not components:
        raise Blocked("selection_components_missing")
    expected = [not (component.get("IsWidescreen", False)
                     or component.get("AspyrOnly", False)
                     or normalized_reference(component.get("Guid")) in deselect)
                for component in components]
    header = "[[" + key + "]]"
    index = -1
    inside = False
    seen = 0
    lines = []
    for line in text.splitlines(keepends=True):
        stripped = line.strip()
        if stripped.startswith("["):
            inside = stripped == header
            if inside:
                index += 1
        elif inside and re.fullmatch(r"IsSelected\s*=\s*(true|false)", stripped):
            line = "IsSelected = " + ("true" if expected[index] else "false") + "\n"
            seen += 1
        lines.append(line)
    if index + 1 != len(components) or seen != len(components):
        raise Blocked("selection_rewrite_incomplete")
    rewritten = "".join(lines)
    updated = tomllib.loads(rewritten)
    if [component.get("IsSelected") for component in updated[key]] != expected:
        raise Blocked("selection_rewrite_mismatch")
    for document_part in (document, updated):
        for component in document_part[key]:
            component.pop("IsSelected", None)
    if updated != document:
        raise Blocked("selection_rewrite_changed_other_fields")
    (destination or converted).write_text(rewritten, encoding="utf-8")
    return sum(expected)


def typescript_import_command(command, guide, output):
    """The ModSync.ts import-md invocation whose output the lanes and the review hash."""
    name = guide.parent.name
    if name not in TYPESCRIPT_GAMES:
        raise Blocked("guide_target_game_unknown")
    return command + ["import-md", str(guide), "--output", str(output),
                      "--game", TYPESCRIPT_GAMES[name],
                      "--source-url", "content/" + name + "/full.md"]


def guid(value):
    if not isinstance(value, str):
        raise Blocked("review_manifest_guid_invalid")
    return str(uuid.UUID(value))


def review_manifest(path, guide, converted):
    """Require a separately authored JSON review, never manufacture approval.

    Version 1: schema_version=1, reviewer (nonempty string), unresolved=[],
    guide_sha256, converted_sha256, components (in converted order).
    Each component: guid, instructions (exact TOML Instructions records),
    options (exact TOML Options records, or []), phase (base/widescreen),
    assessment (nonempty string), unresolved=[], choices ({instruction GUID:
    CLI answer}). Choose/Patcher/Execute instructions require explicit answers,
    including instructions in options. Other answers may be supplied as needed.
    """
    if path is None:
        raise Blocked("review_manifest_required")
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        raise Blocked("review_manifest_schema_invalid")
    reviewer = manifest.get("reviewer")
    if not isinstance(reviewer, str) or not reviewer.strip():
        raise Blocked("review_manifest_reviewer_required")
    if manifest.get("unresolved") != []:
        raise Blocked("review_manifest_unresolved")
    stale = [name for name, path in (("guide_sha256", guide), ("converted_sha256", converted))
             if manifest.get(name) != digest(path)]
    if stale:
        raise Blocked(
            "review_manifest_hash_mismatch: " + ", ".join(stale) + " differ from this run. The converted "
            "TOML depends on the guide and on the ModSync.ts ref (MODSYNC_TS_REF) that built it. Regenerate with: "
            "python3 tools/modsync-ci/gen_ts_review.py --source <ModSync.ts checkout of that ref, built with "
            "`pnpm install --frozen-lockfile && pnpm build`> --game " + guide.parent.name
            + ", then review the diff of ci/" + guide.parent.name + "-typescript-review.json")
    document = tomllib.loads(converted.read_text(encoding="utf-8"))
    components = document.get("thisMod", document.get("Components", []))
    reviews = manifest.get("components")
    if not isinstance(reviews, list) or len(reviews) != len(components):
        raise Blocked("review_manifest_component_coverage_mismatch")
    seen = set()
    instruction_ids = set()
    answers = []
    for component, review in zip(components, reviews):
        if not isinstance(review, dict):
            raise Blocked("review_manifest_component_invalid")
        component_id = guid(component["Guid"])
        if component_id in seen or guid(review["guid"]) != component_id:
            raise Blocked("review_manifest_component_order_or_guid_mismatch")
        seen.add(component_id)
        if (review.get("instructions") != component.get("Instructions", [])
                or review.get("options") != component.get("Options", [])):
            raise Blocked("review_manifest_instructions_mismatch")
        if (review.get("phase") != component_phase(component) or review.get("unresolved") != []
                or not isinstance(review.get("assessment"), str)
                or not review["assessment"].strip()):
            raise Blocked("review_manifest_phase_or_assessment_unresolved")
        choices = review.get("choices")
        if not isinstance(choices, dict):
            raise Blocked("review_manifest_choices_required")
        normalized = {}
        for key, answer in choices.items():
            key = guid(key)
            if key in normalized or not isinstance(answer, str) or not answer.strip():
                raise Blocked("review_manifest_choice_invalid")
            normalized[key] = answer
        instructions = list(component.get("Instructions", []))
        for option in component.get("Options", []):
            instructions.extend(option.get("Instructions", []))
        local_ids = set()
        for instruction in instructions:
            instruction_id = guid(instruction["Guid"])
            if instruction_id in instruction_ids:
                raise Blocked("review_manifest_duplicate_instruction_guid")
            instruction_ids.add(instruction_id)
            local_ids.add(instruction_id)
            if (instruction.get("Action") in ("Choose", "Patcher", "Execute")
                    and instruction_id not in normalized):
                raise Blocked("review_manifest_prompt_choice_missing")
        if not normalized.keys() <= local_ids:
            raise Blocked("review_manifest_unknown_choice_guid")
        for key, answer in normalized.items():
            answers.extend(["--choice", key + "=" + answer])
    return answers



def consume_verified_build(fd, source, mode, private, report):
    """Consume Stage's authenticated parent capability, never a guide-supplied receipt.

    Stage acquires and verifies the encrypted producer artifact before its
    credential scrub, then passes only this anonymous read-only descriptor.
    Recheck the selected checkout and copy the exact binary into owned storage.
    """
    import fcntl
    import production_build

    def require(value, reason):
        if not value:
            raise Blocked("verified_build_" + reason)

    def unchanged(before, after):
        # Reading may update atime. Identity, ownership, contents and mode must stay fixed.
        return all(getattr(before, field) == getattr(after, field) for field in (
            "st_dev", "st_ino", "st_mode", "st_nlink", "st_uid", "st_gid", "st_size",
            "st_mtime_ns", "st_ctime_ns"))

    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate_json_key")
            result[key] = value
        return result

    require(type(fd) is int and fd >= 3, "descriptor_invalid")
    try:
        metadata = os.fstat(fd)
        require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 0
                and 0 < metadata.st_size <= production_build.MAX_JSON,
                "descriptor_not_bounded_anonymous_regular")
        require(fcntl.fcntl(fd, fcntl.F_GETFL) & os.O_ACCMODE == os.O_RDONLY,
                "descriptor_not_read_only")
        os.lseek(fd, 0, os.SEEK_SET)
        with os.fdopen(os.dup(fd), "rb") as stream:
            body = stream.read(production_build.MAX_JSON + 1)
        require(len(body) == metadata.st_size and unchanged(metadata, os.fstat(fd)),
                "descriptor_changed")
    finally:
        os.close(fd)
    payload = json.loads(body, object_pairs_hook=unique_object)
    require(isinstance(payload, dict)
            and set(payload) == {"schema_version", "source_path", "receipt"}
            and type(payload["schema_version"]) is int and payload["schema_version"] == 1
            and payload["source_path"] == str(source), "parent_context_differs")
    receipt = payload["receipt"]
    require(isinstance(receipt, dict) and set(receipt) == {
        "schema", "status", "repository", "ref", "head_sha", "mode", "binary_path",
        "binary_sha256", "binary_bytes", "producer_run_id", "producer_run_attempt",
        "artifact_id", "artifact_zip_sha256", "ciphertext_sha256", "build_provenance"},
        "receipt_shape_differs")
    require(type(receipt["schema"]) is int and receipt["schema"] == 1
            and receipt["status"] == "verified" and receipt["mode"] == mode
            and receipt["repository"] == production_build.REPOSITORY
            and receipt["ref"] in production_build.REFS
            and isinstance(receipt["head_sha"], str)
            and re.fullmatch(r"[0-9a-f]{40}", receipt["head_sha"]), "identity_differs")
    for key in ("binary_sha256", "artifact_zip_sha256", "ciphertext_sha256"):
        require(isinstance(receipt[key], str) and re.fullmatch(r"[0-9a-f]{64}", receipt[key]),
                "digest_invalid")
    for key in ("producer_run_id", "producer_run_attempt", "artifact_id", "binary_bytes"):
        require(type(receipt[key]) is int and receipt[key] > 0, "size_or_identifier_invalid")
    require(receipt["binary_bytes"] <= production_build.MAX_BINARY, "binary_oversized")
    clean = {key: os.environ[key] for key in ("PATH", "HOME") if key in os.environ}
    clean.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL="/dev/null", GIT_OPTIONAL_LOCKS="0")
    try:
        head = subprocess.check_output(["git", "rev-parse", "--verify", "HEAD"],
                                       cwd=source, env=clean, timeout=30, text=True,
                                       stderr=subprocess.DEVNULL).strip()
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise Blocked("verified_build_checkout_identity_unavailable") from error
    require(head == receipt["head_sha"], "checkout_head_differs")
    expected = production_build.recipe(mode, root=source)
    provenance = receipt["build_provenance"]
    require(isinstance(provenance, dict), "provenance_missing")
    wanted = {"schema": 1, "mode": mode, "repository": receipt["repository"],
              "ref": receipt["ref"], "head_sha": head, "workflow_sha": head,
              "workflow_ref": receipt["repository"] + "/" + production_build.WORKFLOW + "@" + receipt["ref"],
              "job": "production", "run_id": str(receipt["producer_run_id"]),
              "run_attempt": str(receipt["producer_run_attempt"]), "recipe": expected,
              "recipe_sha256": production_build.digest(production_build.stable(expected)),
              "binary": {"filename": "modsync", "bytes": receipt["binary_bytes"],
                         "sha256": receipt["binary_sha256"]}}
    require(all(provenance.get(key) == value for key, value in wanted.items())
            and provenance.get("event") in ("push", "workflow_dispatch")
            and isinstance(provenance.get("compiler"), str)
            and provenance["compiler"].startswith("rustc 1.95.0 ")
            and "host: x86_64-unknown-linux-gnu" in provenance["compiler"]
            and isinstance(provenance.get("cargo"), str)
            and provenance["cargo"].startswith("cargo 1.95.0 "), "source_recipe_differs")
    binary = source / "target" / "release" / "modsync"
    require(receipt["binary_path"] == str(binary), "binary_path_differs")
    require(all(parent.is_dir() and not parent.is_symlink()
                for parent in (source / "target", source / "target" / "release")),
            "binary_parent_unsafe")
    destination = private / "target" / "release" / "modsync"
    destination.parent.mkdir(parents=True, mode=0o700)
    checksum = hashlib.sha256()
    size = 0
    prefix = b""
    try:
        with os.fdopen(os.open(binary, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as original:
            before = os.fstat(original.fileno())
            require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                    and stat.S_IMODE(before.st_mode) == 0o755
                    and before.st_size == receipt["binary_bytes"], "binary_shape_differs")
            with destination.open("xb") as output:
                while chunk := original.read(4 * 1024 * 1024):
                    size += len(chunk)
                    require(size <= receipt["binary_bytes"], "binary_grew")
                    if not prefix:
                        prefix = chunk[:64]
                    checksum.update(chunk)
                    output.write(chunk)
                require(unchanged(before, os.fstat(original.fileno()))
                        and size == receipt["binary_bytes"]
                        and checksum.hexdigest() == receipt["binary_sha256"], "binary_changed")
                require(prefix[:7] == b"\x7fELF\x02\x01\x01"
                        and int.from_bytes(prefix[18:20], "little") == 62
                        and int.from_bytes(prefix[16:18], "little") in (2, 3), "binary_not_expected_elf")
                output.flush()
                os.fsync(output.fileno())
                os.fchmod(output.fileno(), 0o755)
        require(production_build.recipe(mode, root=source) == expected, "source_changed_during_copy")
    except BaseException:
        destination.unlink(missing_ok=True)
        raise
    report["production_build"] = {
        "source": "verified-production-artifact", "head_sha": head, "mode": mode,
        "producer_run_id": receipt["producer_run_id"], "producer_run_attempt": receipt["producer_run_attempt"],
        "artifact_id": receipt["artifact_id"], "artifact_zip_sha256": receipt["artifact_zip_sha256"],
        "ciphertext_sha256": receipt["ciphertext_sha256"], "binary_sha256": receipt["binary_sha256"],
        "binary_bytes": size, "recipe_sha256": wanted["recipe_sha256"],
        "source_inventory_sha256": expected["source_inventory_sha256"]}
    progress("verified exact-head production binary copied into private execution storage")
    return destination


def gui_diag_runner_arguments(args):
    # Share the production driver's canonical path and complete-runner checks.
    import gui_rust
    try:
        return gui_rust.execute_runner_arguments(args)
    except gui_rust.Blocked as error:
        raise Blocked(str(error)) from error


def selected_patcher_arguments(args):
    backend = getattr(args, "patcher_engine", None)
    executable = getattr(args, "patcher", None)
    if backend is not None and args.implementation != "rust":
        raise Blocked("patcher_backend_selection_requires_rust")
    if executable is not None and backend != "holo-patcher":
        raise Blocked("patcher_path_requires_holo_patcher_backend")
    if backend == "holo-patcher" and executable is None:
        raise Blocked("holo_patcher_requires_explicit_executable")
    arguments = [] if backend is None else ["--patcher-engine", backend]
    if executable is not None:
        path = Path(executable).resolve(strict=True)
        if not (path.is_file() or (path.is_dir() and (path / "PyKotor/Tools/HoloPatcher/src/holopatcher/__main__.py").is_file())):
            raise Blocked("patcher_executable_missing")
        arguments += ["--patcher", str(path)]
    return arguments


def record_patcher_comparison(args, report, source, private, command, converted, game, archives,
                               runner, gui_result=None):
    """Bind the exporter to the observed successful launch and completion receipt."""
    if getattr(args, "patcher_engine", None) is None:
        return
    if not getattr(args, "complete_guide", False):
        return
    if gui_result is not None:
        launched = gui_result.get("launch_command")
    else:
        path = private / "install.log.command.json"
        if not path.is_file() or path.is_symlink() or path.stat().st_size > 1024 * 1024:
            raise Blocked("comparison_install_command_receipt_missing")
        launched = json.loads(path.read_text(encoding="utf-8")).get("argv")
    state = engine_state_dir(game)
    completion = state / "completion.json"
    if not completion.is_file() or completion.is_symlink() or completion.stat().st_size > 64 * 1024 ** 2:
        raise Blocked("comparison_completion_receipt_missing_or_oversized")
    receipt = json.loads(completion.read_text(encoding="utf-8"))
    identity = (receipt.get("installReport") or {}).get("checkpoint") or {}
    if not identity.get("sessionId") or not identity.get("checkpointId"):
        raise Blocked("comparison_requires_committed_checkpoint_completion")
    git = subprocess.run(["git", "rev-parse", "HEAD"], cwd=source,
                         capture_output=True, text=True, check=True)
    head = git.stdout.strip()
    if re.fullmatch(r"[0-9a-f]{40}", head) is None:
        raise Blocked("comparison_engine_revision_invalid")
    request_dir = runner[runner.index("--execute-request-dir") + 1] if "--execute-request-dir" in runner else None
    report["patcher_comparison_execution"] = {
        "schema_version": 1, "backend": args.patcher_engine, "mode": args.mode,
        "command": launched, "binary_sha256": digest(Path(command[0])), "engine_sha": head,
        "guide_digest": report["guide_sha256"], "input_digest": digest(converted),
        "profile": report["install_profile"]["profile"],
        "selected_guid_order": [component["Guid"] for component in receipt["components"]],
        "session_id": identity["sessionId"], "checkpoint_id": identity["checkpointId"],
        "request_dir": request_dir, "binary": str(command[0]), "converted_guide": str(converted),
        "game_dir": str(game), "archives": str(archives),
        "output_dir": str(getattr(args, "comparison_output_dir", None) or private / "patcher-comparison")}


def export_patcher_comparison(args, report):
    execution = report.get("patcher_comparison_execution")
    if not execution or report.get("status") != "installed":
        return
    output = Path(execution["output_dir"])
    command = [sys.executable, str(Path(__file__).with_name("patcher_comparison.py")), "export",
               "--backend", execution["backend"], "--mode", args.mode,
               "--binary", execution["binary"], "--game-dir", execution["game_dir"],
               "--archives", execution["archives"], "--converted-guide", execution["converted_guide"],
               "--lane-report", str(args.report.absolute()), "--output-dir", str(output)]
    result = live_output.run(command, output.parent / (output.name + "-export.log"),
                             stdin=subprocess.DEVNULL, timeout=7200)
    if result.returncode:
        raise Blocked("patcher_comparison_export_failed")


def execute(args, report):
    patcher = selected_patcher_arguments(args)
    external_backend = getattr(args, "patcher_engine", None) in ("holo-patcher", "mod-packaged")
    verified_fd = getattr(args, "verified_build_fd", None)
    if verified_fd is not None and (args.implementation != "rust" or not getattr(args, "complete_guide", False)):
        raise Blocked("verified_build_requires_complete_rust_lane")
    source = args.source.resolve(strict=True)
    guide = args.guide.resolve(strict=True)
    game = args.game_dir.resolve(strict=True)
    archives = args.archives.resolve(strict=True)
    work = args.work_dir.resolve()
    if not source.is_dir() or not game.is_dir() or not archives.is_dir():
        raise Blocked("required_directory_missing")
    if not guide.is_file() or guide.name != "full.md":
        raise Blocked("full_markdown_guide_required")
    protected = (source, game, archives, guide.parent)
    if any(work == path or work.is_relative_to(path) or path.is_relative_to(work)
           for path in protected):
        raise Blocked("work_directory_overlaps_inputs")
    if any(game == path or game.is_relative_to(path) or path.is_relative_to(game)
           for path in (source, archives, guide.parent)):
        raise Blocked("game_directory_overlaps_inputs")
    work.mkdir(parents=True, exist_ok=True)
    private = Path(tempfile.mkdtemp(prefix="ports-", dir=work))
    private.chmod(0o700)
    report["guide_sha256"] = digest(guide)
    if getattr(args, "complete_guide", False):
        try:
            scope = install_profiles.profile_scope(guide.parent.name, args.install_profile)
        except ValueError as error:
            raise Blocked(str(error)) from error
        if report["guide_sha256"] != scope["source_guide_sha256"]:
            raise Blocked("install_profile_requires_pinned_complete_guide")
        report["install_profile_scope"] = scope
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(private / "target")
    converted = private / "converted.toml"
    if args.implementation == "rust":
        if not (source / "Cargo.toml").is_file():
            raise Blocked("rust_source_manifest_missing")
        build = ["cargo", "build", "--locked", "--bin", "modsync"]
        profile = "debug"
        if getattr(args, "complete_guide", False):
            build += ["--lib", "-j2", "--release"]
            profile = "release"
            # Repeated full archive validation needs optimized decoders. Keep
            # the dev build's arithmetic/assertion guards in this CI profile.
            env["CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS"] = "true"
            env["CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS"] = "true"
            if args.mode == "cli":
                build.append("--no-default-features")
        if verified_fd is None:
            run(build, source, private, "build", report, env)
            report["production_build"] = {"source": "local-build", "profile": profile,
                                          "mode": args.mode, "cargo_arguments": build[1:]}
            executable = private / "target" / profile / "modsync"
        else:
            executable = consume_verified_build(verified_fd, source, args.mode, private, report)
        command = [str(executable)]
        archive_map = []
        if args.archive_map:
            archive_map = ["--archive-map", str(args.archive_map.resolve(strict=True))]
        complete = getattr(args, "complete_guide", False)
        local_generation = [] if complete else ["--auto-generate-local"]
        preparation = []
        if complete and guide.parent.name == "k2":
            preparation = ["--k2-preparation-profile", "steam-windows-source-3cfd-v1"]
            report["native_k2_preparation_profile"] = "steam-windows-source-3cfd-v1"
        try:
            run(command + ["convert", str(guide), str(converted),
                           "--format", "toml", "--parse-directions"] + local_generation
                + ["--source-path", str(archives)] + archive_map + preparation,
                source, private, "convert", report, env)
        finally:
            record_convert_skips(private / "convert.log", report)
        if complete:
            original_guids = [component.get("Guid") for component in components_of(converted)]
            converted.write_text(fullrun_guide_choices.declare_source_order(
                converted.read_text(encoding="utf-8"), original_guids), encoding="utf-8")
            report.setdefault("draft_review", {})["binding_source"] = {
                "kind": "original-guide-draft-before-local-inference", "sha256": digest(converted),
                "source_order_policy": fullrun_guide_choices.source_order_policy(original_guids)}
            if guide.parent.name == "k1" and report["guide_sha256"] == k1_widescreen_inputs.GUIDE_SHA256:
                converted = bind_native_k1(command, source, guide, converted, archives, private, report, env)
        reviewed = private / "converted-reviewed.toml"
        review_input = bind_archives(command, source, guide, converted, archives,
                                     args.archive_map, private, report, env,
                                     load_archive_aliases(guide.parent.name),
                                     vendor_languages=(vendor_language_selection(game, guide, report)
                                                       if getattr(args, "complete_guide", False) else None),
                                     reviewed_recipes=(load_reviewed_manual_recipes(source, guide.parent.name)
                                                       if getattr(args, "complete_guide", False) else None),
                                     compact_defaults=getattr(args, "complete_guide", False))
        try:
            run(command + ["convert", str(review_input), str(reviewed), "--format", "toml",
                           "--source-path", str(archives), "--review-verified-drafts"]
                + (["--compact-instruction-defaults"] if getattr(args, "complete_guide", False) else []),
                source, private, DRAFT_REVIEW_STAGE, report, env)
        finally:
            record_draft_review(private / (DRAFT_REVIEW_STAGE + ".log"), report)
        if not reviewed.is_file() or reviewed.is_symlink():
            raise Blocked("review_verified_drafts_output_missing")
        reviewed.replace(converted)
    else:
        if not (source / "packages/cli/src/main.ts").is_file():
            raise Blocked("typescript_source_entrypoint_missing")
        run(["pnpm", "build"], source, private, "build", report, env)
        command = ["node", str(source / "packages/cli/dist/main.js")]
        run(typescript_import_command(command, guide, converted),
            source, private, "convert", report, env)
        report["selected_component_count"] = select_base_components(converted)
    if digest(guide) != report["guide_sha256"]:
        raise Blocked("guide_changed_during_conversion")
    report["converted_sha256"] = digest(converted)
    report["stage"] = "coverage"
    skipped = []
    if args.restore_report is not None:
        skipped = load_policy_skips(args.restore_report.resolve(strict=True), guide)
    lane_skips = load_lane_skips(guide.parent.name, implementation=args.implementation)
    if args.implementation == "rust" and report.get("native_k1_profile"):
        retired = [item for item in lane_skips if item.get("section") == "Mod List"
                   and item.get("heading") == "4GB Patcher" and item.get("severity") == "warning"]
        report["superseded_lane_skips"] = retired
        lane_skips = [item for item in lane_skips if item not in retired]
    skipped = merge_lane_skips(skipped, lane_skips)
    report["component_count"] = coverage(
        guide, converted, report, args.implementation, skipped)
    gui_rust = args.implementation == "rust" and args.mode == "gui"
    complete_rust = args.implementation == "rust" and getattr(args, "complete_guide", False)
    complete_gui = gui_rust and complete_rust

    def deselected_now():
        return {record["component_guid"] for record in report.get(SKIPPED_REPORT_KEY, [])
                if record["component_guid"] is not None}

    if args.implementation == "rust":
        report["stage"] = "draft_review"
        exempt = deselected_now()
        exempt |= set((report.get("install_profile_scope") or {}).get("excluded_guids", []))
        if gui_rust and not complete_gui:
            exempt |= {item["component_guid"] for item in gui_widescreen_scope(converted, exempt)[0]}
        accept_drafts(command, source, converted, private, report, env, exempt, gui_rust)
        if digest(guide) != report["guide_sha256"]:
            raise Blocked("guide_changed_during_conversion")
        report["converted_sha256"] = digest(converted)
    if complete_rust:
        if args.restore_report is None or args.archive_map is None:
            raise Blocked("complete_guide_requires_current_restore_report_and_archive_map")
        applicability = baseline_applicability(
            command, source, game, guide, converted, private, report, env)
        report["stage"] = "guide_selection"
        try:
            movie_alternative = None
            if args.install_profile == "exhaustive":
                movie_alternative = fullrun_guide_choices.verify_alternative(
                    guide.parent.name, archives, args.restore_report.resolve(strict=True),
                    args.archive_map.resolve(strict=True),
                    Path(__file__).resolve().with_name("seed_pins.json"))
            receipt = install_profiles.apply(converted, guide.parent.name,
                args.install_profile, baseline_applicability=applicability)
        except (ValueError, OSError, subprocess.SubprocessError) as error:
            raise Blocked("install_profile_selection_refused: " + str(error)) from error
        if movie_alternative is not None:
            receipt["selected_movie_alternative"] = movie_alternative
        report["install_profile"] = receipt
        report["complete_guide"] = True
        report["installation_scope"] = receipt["claim"]
        omitted = set(receipt["excluded_guids"]) | set(receipt["platform_inactive_guids"])
        report["profile_selection_omissions"] = [item for item in report.get(SKIPPED_REPORT_KEY, [])
            if item.get("component_guid") in omitted]
        report[SKIPPED_REPORT_KEY] = [item for item in report.get(SKIPPED_REPORT_KEY, [])
            if item.get("component_guid") not in omitted]
        report["original_converted_component_count"] = receipt["source_definition_count"]
        report["converted_component_count"] = receipt["retained_definition_count"]
        report["admitted_component_count"] = receipt["selected_count"]
        report["component_count"] = receipt["selected_count"]
        report["source_order"] = fullrun_guide_choices.source_order_policy(receipt["original_guids"])
        report["converted_sha256"] = receipt["admitted_input_sha256"]
        inactive = omitted
        # Keep original conversion diagnostics, separately attributed to the two
        # inactive choices; they are not required admitted recipe components.
        diagnostics = report.get(BIND_DIAGNOSTICS_KEY, [])
        report["inactive_choice_bind_diagnostics"] = [
            item for item in diagnostics if item.get("component_guid") in inactive]
        report[BIND_DIAGNOSTICS_KEY] = [
            item for item in diagnostics if item.get("component_guid") not in inactive]
        # A complete run cannot recover missing required instructions by
        # validating and installing the remaining subset. Preserve every
        # refusal in the report and stop before expensive archive simulation.
        # A policy label does not establish platform applicability. After the
        # independently checked projection, every remaining component GUID is
        # required; guide-only applicability records have no component GUID.
        if any(item.get("component_guid") is not None
               and item.get("status") in {NOT_INSTALLED, SKIPPED_BY_POLICY}
               for item in report.get(SKIPPED_REPORT_KEY, [])):
            raise Blocked("complete_guide_unresolved_components")
    deselect = []
    for record in report.get(SKIPPED_REPORT_KEY, []):
        if record["component_guid"] is not None:
            deselect += ["--deselect", record["component_guid"]]
    profile_receipt = report.get("install_profile") or {}
    profile_omitted = set(profile_receipt.get("excluded_guids", [])) | set(profile_receipt.get("platform_inactive_guids", []))
    deselect += [flag for identity in sorted(profile_omitted) for flag in ("--deselect", identity)]
    policy_guids = deselected_now()
    execution_excluded = policy_guids | profile_omitted
    excluded, has_widescreen = ([], False)
    if gui_rust:
        excluded, has_widescreen = gui_widescreen_scope(converted, execution_excluded)
        if complete_gui:
            excluded = []
        report["gui_not_covered"] = {
            "widescreen_phase": ("pending" if complete_gui else "not_covered_by_gui_lane") if has_widescreen else "none_in_input",
            "excluded_widescreen": excluded}
        deselect += [flag for item in excluded for flag in ("--deselect", item["component_guid"])]
    if args.implementation == "rust":
        report["stage"] = "draft_review"
        pending = pending_drafts(
            converted, execution_excluded | {item["component_guid"] for item in excluded}, gui_rust)
        if pending:
            # accept_drafts accepted or deselected every pending draft; one left is a bug.
            report.setdefault("draft_review", {})["unreviewed"] = pending
            raise Blocked("unreviewed_draft_instructions_remain: " + "; ".join(
                f"{item['component_guid']} ({item['heading']})" for item in pending))
    if args.implementation == "typescript":
        report["stage"] = "review"
        choices = review_manifest(args.review_manifest, guide, converted)
        selected_input = converted
        if execution_excluded:
            selected_input = private / "converted-policy.toml"
            report["selected_after_policy_count"] = select_base_components(
                converted, execution_excluded, selected_input)
        document = tomllib.loads(converted.read_text(encoding="utf-8"))
        installable = [guid(component["Guid"])
                       for component in document.get("thisMod", document.get("Components", []))
                       if guid(component["Guid"]) not in execution_excluded]
        report["installable_component_count"] = len(installable)
        all_installable = list(installable)
        alias_archives(converted, args.archive_map, archives, set(installable), report,
                       load_archive_aliases(guide.parent.name))
        def validate_arguments(taken):
            blocked = deselected_guids(taken)
            chosen = [each for each in installable if normalized_reference(each) not in blocked]
            return [str(converted), "--kotor", str(game),
                    "--mods", str(archives), "--select", ",".join(chosen)]

        directories = validate_arguments(deselect)
        validate_with_quarantine(
            command, directories, deselect, converted, game, source, private, report, env,
            getattr(args, "max_quarantine", QUARANTINE_MAX_ITERATIONS),
            getattr(args, "max_quarantine_fraction", QUARANTINE_MAX_FRACTION),
            arguments=validate_arguments,
            selectable={normalized_reference(each) for each in installable})
        quarantined_guids = {item["component_guid"] for item in quarantined_entries(report)}
        if quarantined_guids:
            report["installable_before_quarantine_count"] = len(installable)
            installable = [each for each in installable
                           if normalized_reference(each) not in quarantined_guids]
            report["installable_component_count"] = len(installable)
            directories = validate_arguments(deselect)
            selected_input = private / "converted-policy.toml"
            report["selected_after_policy_count"] = select_base_components(
                converted, policy_guids | quarantined_guids, selected_input)
        def reviewed_inputs_unchanged():
            if (digest(guide) != report["guide_sha256"]
                    or digest(converted) != report["converted_sha256"]):
                raise Blocked("reviewed_inputs_changed")

        if args.mode == "gui":
            result = install_gui_with_quarantine(
                command, source, private, game, archives, converted, selected_input,
                policy_guids, deselect, {normalized_reference(each) for each in all_installable},
                report, env, getattr(args, "max_quarantine", QUARANTINE_MAX_ITERATIONS),
                getattr(args, "max_quarantine_fraction", QUARANTINE_MAX_FRACTION),
                reviewed_inputs_unchanged, choices)
            if report.get(INSTALL_QUARANTINE_KEY):
                omitted = {item["component_guid"] for item in quarantined_entries(report)}
                report.setdefault("installable_before_quarantine_count", len(all_installable))
                report["installable_component_count"] = len(
                    [each for each in all_installable if normalized_reference(each) not in omitted])
            report["gui"] = result
            report["proof"] = "native_gui_and_verified_completion_receipt"
            report["status"] = "installed"
            report["reason"] = result["reason"]
            return
        install_with_quarantine(
            command, directories, deselect, converted, game, source, private, report, env,
            getattr(args, "max_quarantine", QUARANTINE_MAX_ITERATIONS),
            getattr(args, "max_quarantine_fraction", QUARANTINE_MAX_FRACTION),
            implementation="typescript", arguments=validate_arguments,
            selectable={normalized_reference(each) for each in all_installable},
            extra=choices + [TYPESCRIPT_PHYSICAL_INSTALL_FLAG], before_phase=reviewed_inputs_unchanged)
        if report.get(INSTALL_QUARANTINE_KEY):
            omitted = {item["component_guid"] for item in quarantined_entries(report)}
            report.setdefault("installable_before_quarantine_count", len(all_installable))
            report["installable_component_count"] = len(
                [each for each in all_installable if normalized_reference(each) not in omitted])
        report["status"] = "installed"
        report["reason"] = "native_install_commands_succeeded"
        return
    directories = ["--input", str(converted), "--game-dir", str(game),
                   "--source-dir", str(archives), "--use-file-selection"]
    explicit_runner = gui_diag_runner_arguments(args)
    execute_runner = explicit_runner or (execute_runner_arguments(private, converted, report, force=True)
        if external_backend else execute_runner_arguments(private, converted, report) if complete_rust else [])
    directories += patcher
    if complete_rust or external_backend:
        # Complete runs authorize the reviewed admitted external instructions.
        # Desktop trust is independently enabled through its native checkbox.
        directories += ["--allow-processes"] + execute_runner
    validate_with_quarantine(
        command, directories, deselect, converted, game, source, private, report, env,
        getattr(args, "max_quarantine", QUARANTINE_MAX_ITERATIONS),
        getattr(args, "max_quarantine_fraction", QUARANTINE_MAX_FRACTION))
    if args.mode == "gui":
        if (digest(guide) != report["guide_sha256"]
                or digest(converted) != report["converted_sha256"]):
            raise Blocked("reviewed_inputs_changed")
        driver_report = install_rust_gui_with_quarantine(
            command, source, private, game, archives, converted, policy_guids, deselect, report, env,
            getattr(args, "max_quarantine", QUARANTINE_MAX_ITERATIONS),
            getattr(args, "max_quarantine_fraction", QUARANTINE_MAX_FRACTION),
            complete_guide=complete_gui, execute_runner=execute_runner,
            profile_receipt=profile_receipt or None,
            **({"patcher": patcher} if patcher else {}))
        gui_skipped = policy_guids | {item["component_guid"] for item in quarantined_entries(report)}
        if not driver_report.is_file() or driver_report.is_symlink():
            raise Blocked("gui_completion_report_missing")
        result = json.loads(driver_report.read_text(encoding="utf-8"))
        if not isinstance(result, dict):
            raise Blocked("gui_completion_not_verified")
        expected_completed = (len(set(profile_receipt["selected_guids"]) - gui_skipped)
            if profile_receipt else report["component_count"] - len(gui_skipped) - len(excluded))
        if complete_gui:
            expected_ids = ((set(profile_receipt["selected_guids"]) if profile_receipt else
                             {str(component.get("Guid", "")).lower()
                              for component in converted_components(converted)}) - gui_skipped)
            if set(result.get("completed_guids", [])) != expected_ids:
                raise Blocked("gui_complete_guide_component_set_mismatch")
            if has_widescreen:
                try:
                    require_gui_phases(result, expected_completed, definition_count=(
                        report["converted_component_count"] if profile_receipt else None))
                except ValueError as error:
                    raise Blocked("gui_complete_guide_phase_evidence_invalid: " + str(error)) from error
            selected_widescreen = gui_widescreen_scope(converted, gui_skipped | profile_omitted)[0]
            expected_phase = ("completed_and_verified" if selected_widescreen else
                              "not_installed_due_to_policy") if has_widescreen else "none_in_input"
            if result.get("widescreen_phase") != expected_phase:
                raise Blocked("gui_complete_guide_widescreen_phase_not_verified")
            report["gui_not_covered"]["widescreen_phase"] = expected_phase
        if (not isinstance(result, dict)
                or result.get("schema_version") != 1
                or result.get("implementation") != "rust"
                or result.get("mode") != "gui"
                or result.get("status") != "installed"
                or result.get("reason") != "native_gui_completed_and_receipt_verified"
                or result.get("ui_completed") is not True
                or result.get("input_sha256") != report["converted_sha256"]
                or result.get("binary_sha256") != digest(Path(command[0]))
                or result.get("component_count") != (report["converted_component_count"] if profile_receipt else report["component_count"])
                or result.get("completed_components") != expected_completed
                or result.get("expected_components") != expected_completed
                or result.get("skipped_by_reviewed_policy") != sorted(gui_skipped)
                or (profile_receipt and (
                    result.get("install_profile") != profile_receipt
                    or result.get("profile_excluded_guids") != profile_receipt["excluded_guids"]
                    or result.get("platform_inactive_guids") != profile_receipt["platform_inactive_guids"]))
                or result.get("excluded_widescreen") != sorted(
                    item["component_guid"] for item in excluded)
                or result.get("widescreen_phase") != report["gui_not_covered"]["widescreen_phase"]
                or result.get("coverage") != ("subset" if gui_skipped or excluded else "full")
                or not isinstance(result.get("verified_game_files"), int)
                or result["verified_game_files"] <= 0
                or not isinstance(result.get("receipt_sha256"), str)
                or re.fullmatch(r"[0-9a-f]{64}", result["receipt_sha256"]) is None):
            raise Blocked("gui_completion_not_verified")
        if (digest(guide) != report["guide_sha256"]
                or digest(converted) != report["converted_sha256"]):
            raise Blocked("reviewed_inputs_changed")
        report["gui"] = result
        report["gui_covered_components"] = expected_completed
        report["proof"] = "native_gui_and_verified_completion_receipt"
        report["status"] = "installed"
        report["reason"] = result["reason"]
        record_patcher_comparison(args, report, source, private, command, converted, game, archives,
                                  execute_runner, gui_result=result)
        return
    install_with_quarantine(
        command, directories, deselect, converted, game, source, private, report, env,
        getattr(args, "max_quarantine", QUARANTINE_MAX_ITERATIONS),
        getattr(args, "max_quarantine_fraction", QUARANTINE_MAX_FRACTION))
    report["status"] = "installed"
    report["reason"] = "native_install_command_succeeded"
    record_patcher_comparison(args, report, source, private, command, converted, game, archives,
                              execute_runner)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--implementation", choices=("rust", "typescript"), required=True)
    parser.add_argument("--mode", choices=("cli", "gui"), required=True)
    parser.add_argument("--patcher-engine", choices=("ody-patcher", "holo-patcher", "mod-packaged"))
    parser.add_argument("--patcher", type=Path)
    parser.add_argument("--comparison-output-dir", type=Path)
    for name in ("execute-request-dir", "execute-xvfb", "execute-wine", "execute-wineserver", "execute-wine-prefix"):
        parser.add_argument("--" + name, type=Path)
    parser.add_argument("--install-profile", choices=install_profiles.PROFILES, default="minimal",
                        help="closed selections on the complete pinned source guide; definitions stay intact")
    parser.add_argument("--complete-guide", action="store_true",
                        help="drive the Rust desktop through base and remaining phases with full receipt verification")
    parser.add_argument("--verified-build-fd", type=int, help=argparse.SUPPRESS)
    parser.add_argument("--review-manifest", type=Path,
                        help="separately authored hash-bound JSON review (TypeScript required)")
    parser.add_argument("--archive-map", type=Path,
                        help="cache.py restore --archive-map JSON passed to the Rust convert; TypeScript lanes use it "
                             "to link restored archives under the names the converted Extract globs match")
    parser.add_argument("--restore-report", type=Path,
                        help="cache.py restore --report JSON; its reviewed skipped_steps and its "
                             "unavailable_steps (restore --deselect-unavailable) are left out of the install "
                             "(Rust --deselect, TypeScript --select of the rest) with their dependents and "
                             "recorded as warnings. Both implementations also apply the lane_skips of "
                             "archive_selections.json for their lane (error severity unless the entry says "
                             "warning)")
    parser.add_argument("--max-quarantine", type=int, default=QUARANTINE_MAX_ITERATIONS,
                        help="most validate and install failures together to quarantine (Rust), or validate "
                             "failures (TypeScript) "
                             "(deselect and re-run) before the lane fails; 0 disables quarantine "
                             "(default %(default)s)")
    parser.add_argument("--max-quarantine-fraction", type=float, default=QUARANTINE_MAX_FRACTION,
                        help="most quarantined components (validate and install failures plus "
                             "dependents) as a fraction of the selected components (default %(default)s)")
    for name in ("source", "guide", "game-dir", "archives", "work-dir", "report"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args(argv)
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(line_buffering=True)
        except (AttributeError, ValueError, OSError):
            pass
    report = {"schema_version": 1, "implementation": args.implementation,
              "mode": args.mode, "status": "blocked", "stage": "preflight",
              "commands": [], "proof": "native_commands_only"}
    previous_umask = os.umask(0o077)
    try:
        destination = args.report.absolute()
        inputs = [path.resolve() for path in
                  (args.source, args.game_dir, args.archives, args.guide.parent)]
        if destination.is_symlink() or destination.exists() or any(
            destination.resolve().is_relative_to(path) for path in inputs
        ):
            print("ports: report destination must be new and outside inputs", file=sys.stderr)
            return 2
        try:
            execute(args, report)
            errors = lane_errors(report)
            report[LANE_ERRORS_KEY] = errors
            omitted = omitted_components(report)
            report[OMITTED_KEY] = omitted
            report["partial"] = bool(omitted)
            if report["status"] == "installed" and errors["count"]:
                # Everything installable was installed, but the guide was not installed to spec.
                report["status"] = "incomplete"
                report["install_reason"] = report.get("reason")
                report["reason"] = (f"install_incomplete: {errors['not_installed']} component(s) not "
                                    f"installed and {errors['bind_unresolved']} installed from an "
                                    "unresolved archive binding")
            elif report["status"] == "installed" and omitted:
                # Everything obtainable was installed to spec; unobtainable archives were left out.
                report["status"] = "partial"
        except Blocked as error:
            report["reason"] = str(error)
        except (OSError, ValueError, TypeError, KeyError):
            report["reason"] = "input_tool_or_document_error"
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open("x", encoding="utf-8") as output:
            json.dump(report, output, indent=2)
            output.write("\n")
        try:
            export_patcher_comparison(args, report)
        except (Blocked, OSError, ValueError, subprocess.SubprocessError) as error:
            report["status"] = "blocked"
            report["reason"] = str(error) if isinstance(error, Blocked) else "patcher_comparison_export_failed"
            destination.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report), flush=True)
        print_diagnostics(report)
        sys.stderr.flush()
        return 0 if report["status"] in PASSING_STATUSES else 1
    except OSError:
        print("ports: unable to write sanitized report", file=sys.stderr)
        return 2
    finally:
        os.umask(previous_umask)


if __name__ == "__main__":
    sys.exit(main())
