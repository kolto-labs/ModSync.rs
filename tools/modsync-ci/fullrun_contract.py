#!/usr/bin/env python3
"""Repository-owned full-run request, provenance and coverage contract.

No network access, credential disclosure, game mutation or Rust test compilation happens here.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys

from seal import MIN_KEY_BYTES, validate_recipient
import fullrun_guide_choices

OPERATIONS = ("check", "full", "games", "archives", "install", "seed", "retain-inputs", "p2p")
SHA = re.compile(r"[0-9a-f]{40}")
REPO = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def exact_sha(value):
    if not isinstance(value, str) or SHA.fullmatch(value) is None:
        raise ValueError("an exact lowercase 40-character commit SHA is required")
    return value


def repository(value):
    if not isinstance(value, str) or REPO.fullmatch(value) is None:
        raise ValueError("invalid repository identity")
    return value


def plan(config, execution_repository, own_sha, counterpart_sha="", operation="check",
         game="both", mode="both", write_cache=False, event="workflow_dispatch", trusted_pull_request=False):
    if config.get("schema_version") != 1 or config.get("role") not in ("engine", "guides"):
        raise ValueError("unsupported repository-owned CI configuration")
    owner = repository(config["repository"])
    if repository(execution_repository).lower() != owner.lower():
        raise ValueError("this CI variant must execute in its owning repository")
    own_sha = exact_sha(own_sha)
    counterpart_sha = exact_sha(counterpart_sha or config["counterpart_sha"])
    engine_repo = repository(config["engine_repository"])
    guide_repo = repository(config["guide_repository"])
    if operation not in OPERATIONS or game not in ("both", "k1", "k2") or mode not in ("both", "cli", "gui"):
        raise ValueError("unsupported operation, game or mode")
    if operation == "retain-inputs" and event != "workflow_dispatch":
        raise ValueError("public-input retention requires explicit manual dispatch")
    if type(write_cache) is not bool:
        raise ValueError("write_cache must be boolean")
    if (event != "workflow_dispatch" and operation != "check"
            and not (config["role"] == "engine" and (event == "push" or (event == "pull_request" and trusted_pull_request)))):
        raise ValueError("full-run operations require an owned branch, a same-repository PR, or manual dispatch")
    # The dispatch form shares this checkbox across operations. Non-producer
    # operations are always read-only, regardless of the form's default value.
    write_cache = write_cache and operation in ("full", "games", "archives")
    if config["role"] == "engine":
        if engine_repo.lower() != owner.lower():
            raise ValueError("engine variant must test its own commit")
        engine_sha, guide_sha = own_sha, counterpart_sha
    else:
        if guide_repo.lower() != owner.lower():
            raise ValueError("guide variant must test its own commit")
        engine_sha, guide_sha = counterpart_sha, own_sha
    result = {
        "schema_version": 1, "owner_repository": owner, "owner_sha": own_sha,
        "engine_repository": engine_repo, "engine_sha": engine_sha,
        "guide_repository": guide_repo, "guide_sha": guide_sha,
        "operation": operation, "games": ["k1", "k2"] if game == "both" else [game],
        "modes": ["cli", "gui"] if mode == "both" else [mode],
        "cache_writer": write_cache,
        "claim": "workflow-contract-only" if operation == "check" else "single-lane-installation",
        "cross_lane_parity": "unverified",
    }
    canonical = json.dumps(result, sort_keys=True, separators=(",", ":")).encode()
    result["plan_id"] = hashlib.sha256(canonical).hexdigest()
    return result



DEFAULT_WORKFLOW_POLICY = {
    "push": "quick", "pull_request": "full", "workflow_dispatch": "minimal",
    "release_candidate": "exhaustive", "default_download_mode": "cache-first",
    "release_download_mode": "refresh", "release_target_branches": ["main", "master"],
    "release_branch_prefixes": ["release/", "release-please--"],
    "release_labels": ["release", "release-candidate"],
}


def workflow_policy(event, payload=None, overrides=None, requested_profile="", requested_download=""):
    """Resolve optional workflow-file defaults; release gates cannot be downgraded by inputs."""
    payload, policy = payload or {}, {**DEFAULT_WORKFLOW_POLICY, **(overrides or {})}
    pull = payload.get("pull_request") or {}
    labels = {label.get("name", "").lower() for label in pull.get("labels", [])}
    head, base = pull.get("head", {}).get("ref", ""), pull.get("base", {}).get("ref", "")
    release = bool(pull) and (base in policy["release_target_branches"]
        or any(head.startswith(prefix) for prefix in policy["release_branch_prefixes"])
        or bool(labels.intersection(policy["release_labels"])))
    profile = policy["release_candidate"] if release else (requested_profile or policy.get(event, "minimal"))
    download = policy["release_download_mode"] if release else (requested_download or policy["default_download_mode"])
    if profile not in ("quick", "minimal", "full", "exhaustive") or download not in ("cache-first", "refresh"):
        raise ValueError("unsupported installation profile or download policy")
    if release and (profile != "exhaustive" or download != "refresh"):
        raise ValueError("release candidates require exhaustive installation and downloader refresh")
    return {"install_profile": profile, "download_mode": download,
            "release_candidate": release, "require_downloader_attempts": release}

def require_gui_phases(gui, count, definition_count=None):
    """Admit a complete native two-phase report only with cumulative receipt evidence."""
    def identities(value):
        if (not isinstance(value, list) or not value
                or any(not isinstance(each, str) or re.fullmatch(
                    r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", each) is None
                    for each in value) or len(value) != len(set(value))):
            raise ValueError("GUI phase component identities missing or duplicate")
        return set(value)

    def checksum(value):
        return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None

    if (not isinstance(gui, dict) or gui.get("complete_guide") is not True
            or gui.get("coverage") != "full" or gui.get("excluded_widescreen") != []
            or gui.get("skipped_by_reviewed_policy") != []
            or gui.get("status") != "installed" or gui.get("ui_completed") is not True
            or gui.get("reason") != "native_gui_completed_and_receipt_verified"
            or gui.get("completed_components") != count or gui.get("expected_components") != count
            or gui.get("component_count") != (count if definition_count is None else definition_count)
            or not checksum(gui.get("receipt_sha256"))):
        raise ValueError("GUI full-guide final receipt evidence missing")
    phases = gui.get("phases")
    if not isinstance(phases, list) or len(phases) != 2:
        raise ValueError("GUI base and remaining phase receipts required")
    cumulative = set()
    for phase, name in zip(phases, ("base", "remaining")):
        if (not isinstance(phase, dict) or phase.get("phase") != name
                or phase.get("validation_passed") is not True
                or phase.get("snapshot_confirmed") is not True
                or phase.get("ui_completed") is not True
                or not checksum(phase.get("receipt_sha256"))
                or type(phase.get("verified_game_files")) is not int
                or phase["verified_game_files"] <= 0):
            raise ValueError("GUI phase validation, snapshot or receipt evidence missing")
        expected = identities(phase.get("expected_guids"))
        if cumulative & expected:
            raise ValueError("GUI phase component sets overlap")
        cumulative |= expected
        if identities(phase.get("completed_guids")) != cumulative:
            raise ValueError("GUI phase receipt does not cover cumulative component set")
    if (len(cumulative) != count or identities(gui.get("completed_guids")) != cumulative
            or gui["receipt_sha256"] != phases[-1]["receipt_sha256"]
            or gui.get("verified_game_files") != phases[-1]["verified_game_files"]):
        raise ValueError("GUI final receipt differs from verified remaining-phase receipt")
    return cumulative


def require_complete(report, mode, expected_guide_sha256=None, install_profile=None):
    if not isinstance(report, dict) or report.get("status") != "installed" or report.get("implementation") != "rust":
        raise ValueError("required Rust lane did not complete installation")
    if report.get("mode") != mode:
        raise ValueError("lane mode mismatch")
    if report.get("partial") is not False or report.get("omitted_components") != []:
        raise ValueError("partial or missing coverage cannot satisfy a full install")
    count = report.get("component_count")
    if type(count) is not int or count <= 0:
        raise ValueError("missing/nonpositive component coverage")
    errors = report.get("lane_errors")
    if not isinstance(errors, dict) or type(errors.get("count")) is not int or errors["count"] != 0:
        raise ValueError("missing or failing lane error reconciliation")
    if expected_guide_sha256 is not None and report.get("guide_sha256") != expected_guide_sha256:
        raise ValueError("report guide digest does not match the admitted guide")
    selection = report.get("guide_selection")
    scoped_profile = report.get("install_profile")
    if isinstance(scoped_profile, dict):
        import install_profiles
        admitted = install_profiles.validate_receipt(scoped_profile, expected_profile=install_profile)
        if (report.get("complete_guide") is not True
                or report.get("source_component_count") != scoped_profile["source_definition_count"]
                or report.get("original_converted_component_count") != scoped_profile["source_definition_count"]
                or report.get("converted_component_count") != scoped_profile["retained_definition_count"]
                or count != scoped_profile["selected_count"] or len(admitted) != count
                or report.get("admitted_component_count") != count
                or report.get("guide_sha256") != scoped_profile["source_guide_sha256"]
                or report.get("converted_sha256") != scoped_profile["admitted_input_sha256"]):
            raise ValueError("profile selection does not match actual input and completed coverage")
        if mode == "gui" and set((report.get("gui") or {}).get("completed_guids", [])) != admitted:
            raise ValueError("GUI receipt does not cover every selected profile component")
    elif install_profile is not None:
        raise ValueError("requested installation profile has no validated coverage receipt")
    elif report.get("complete_guide") is True or selection is not None:
        admitted = fullrun_guide_choices.validate_receipt(selection)
        if (report.get("complete_guide") is not True
                or report.get("installation_scope") != fullrun_guide_choices.CLAIM
                or report.get("guide_sha256") != selection["source_guide_sha256"]
                or report.get("source_component_count") != selection["original_component_count"]
                or report.get("original_converted_component_count") != selection["original_component_count"]
                or report.get("converted_component_count") != count
                or report.get("admitted_component_count") != count or len(admitted) != count
                or report.get("converted_sha256") != selection["admitted_input_sha256"]):
            raise ValueError("guide selection recipe does not match installed lane input or source coverage")
        if mode == "gui" and set((report.get("gui") or {}).get("completed_guids", [])) != admitted:
            raise ValueError("GUI receipt does not cover every admitted recipe component")
        if ("source_order_policy" in selection or "source_order" in report) and (
                report.get("source_order") != selection.get("source_order_policy")):
            raise ValueError("source-order provenance differs from admitted lane input")
    if mode == "gui":
        gui = report.get("gui")
        scope = report.get("gui_not_covered")
        if (not isinstance(gui, dict) or gui.get("coverage") != "full"
                or not isinstance(scope, dict) or scope.get("excluded_widescreen") != []):
            raise ValueError("GUI subset or unverified widescreen phase is incomplete")
        phase = scope.get("widescreen_phase")
        if phase == "completed_and_verified":
            require_gui_phases(gui, count, definition_count=(
                scoped_profile["retained_definition_count"] if isinstance(scoped_profile, dict) else None))
            if gui.get("widescreen_phase") != phase:
                raise ValueError("GUI widescreen phase evidence mismatch")
        elif phase != "none_in_input":
            raise ValueError("GUI subset or unverified widescreen phase is incomplete")
    return {"status": "complete", "component_count": count,
            "installation_scope": (scoped_profile["profile"] + "-installation") if isinstance(scoped_profile, dict)
                else fullrun_guide_choices.CLAIM if selection is not None else "all-input-components",
            "cross_lane_parity": "unverified"}


def aggregate(operation, results, require_downloader=False, require_comparison=False):
    required = {"check": ("plan", "toolkit"), "full": ("plan", "toolkit", "games", "archives", "install", "p2p"),
                "games": ("plan", "toolkit", "games"), "archives": ("plan", "toolkit", "archives"),
                "install": ("plan", "toolkit", "install"), "seed": ("plan", "toolkit", "seed"),
                "p2p": ("plan", "toolkit", "p2p"),
                "retain-inputs": ("plan", "toolkit", "retain_inputs")}
    if operation not in required or not isinstance(results, dict):
        raise ValueError("unknown requested coverage")
    if require_comparison:
        required[operation] += ("comparison",)
    missing = [name for name in required[operation]
               if not isinstance(results.get(name), dict) or results[name].get("result") != "success"]
    if missing:
        raise ValueError("required jobs missing, skipped, cancelled or failed: " + ", ".join(missing))
    if require_downloader and (operation != "full" or
            results.get("archives", {}).get("outputs", {}).get("download_validation") != "passed"):
        raise ValueError("release gate requires exhaustive downloader attempts with verified results or cache fallback")
    return {"operation": operation, "status": "passed", "required_jobs": list(required[operation]),
            "cross_lane_parity": "compared" if require_comparison else "unverified"}


def require_stage_credentials(stage):
    required = {"games": ("CACHE_ENCRYPTION_KEY",),
                "archives": ("DIAGNOSTICS_ENCRYPTION_KEY",),
                "install": ("CACHE_ENCRYPTION_KEY", "DIAGNOSTICS_ENCRYPTION_KEY"),
                "seed": ("DIAGNOSTICS_ENCRYPTION_KEY",)}
    if stage not in required:
        raise ValueError("unknown credential-bearing stage")
    # GitHub resolves repository-over-organization secrets before passing them.
    # Diagnostics can be encrypted using only an offline custodian's public key.
    recipient = os.environ.get("DIAGNOSTICS_ENCRYPTION_RECIPIENT", "")
    if stage != "games" and recipient:
        validate_recipient(recipient)  # age also verifies its checksum before sealing.
        required[stage] = tuple(key for key in required[stage] if key != "DIAGNOSTICS_ENCRYPTION_KEY")
    missing = [key for key in required[stage] if not os.environ.get(key)]
    short = [key for key in required[stage]
             if key not in missing and len(os.environ[key].encode()) < MIN_KEY_BYTES]
    if missing or short:
        problems = []
        if missing:
            problems.append("missing: " + ", ".join(missing))
        if short:
            problems.append(f"shorter than {MIN_KEY_BYTES} bytes: " + ", ".join(short))
        raise ValueError(f"{stage} credential setup is incomplete ({'; '.join(problems)}). "
                         "Set or expose the named Actions secrets in the owning repository "
                         "(organization secrets are supported). Diagnostics may instead use the "
                         "DIAGNOSTICS_ENCRYPTION_RECIPIENT public Actions variable; "
                         "see docs/ci-fullruns.md#minimum-setup.")
    return {"stage": stage, "status": "credentials-present"}


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def emit_outputs(value):
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as stream:
            for key, item in value.items():
                text = json.dumps(item, separators=(",", ":")) if not isinstance(item, str) else item
                if "\n" in text or "\r" in text:
                    raise ValueError("unsafe workflow output")
                stream.write(key + "=" + text + "\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("plan")
    p.add_argument("--config", default=".github/fullrun.json")
    p.add_argument("--output", required=True)
    k = sub.add_parser("check-credentials")
    k.add_argument("--stage", choices=("games", "archives", "install", "seed"), required=True)
    c = sub.add_parser("complete")
    c.add_argument("--report", required=True)
    c.add_argument("--guide", required=True)
    c.add_argument("--mode", choices=("cli", "gui"), required=True)
    c.add_argument("--output", required=True)
    a = sub.add_parser("aggregate")
    a.add_argument("--operation", required=True)
    a.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "plan":
            raw = os.environ.get("REQUEST_WRITE_CACHE", "false").lower()
            if raw not in ("true", "false"):
                raise ValueError("write_cache must be boolean")
            event = os.environ["GITHUB_EVENT_NAME"]
            payload_path = os.environ.get("GITHUB_EVENT_PATH")
            payload = json.loads(Path(payload_path).read_text()) if payload_path else {}
            own_sha = os.environ["GITHUB_SHA"]
            trusted_pull = False
            if event == "pull_request":
                pull = payload.get("pull_request") or {}
                trusted_pull = (pull.get("head", {}).get("repo", {}).get("full_name", "").lower()
                                == os.environ["GITHUB_REPOSITORY"].lower())
                own_sha = exact_sha(pull.get("head", {}).get("sha", ""))
            policy = workflow_policy(event, payload,
                json.loads(os.environ.get("FULLRUN_DEFAULTS", "{}")),
                os.environ.get("REQUEST_INSTALL_PROFILE", ""), os.environ.get("REQUEST_DOWNLOAD_MODE", ""))
            operation = os.environ.get("REQUEST_OPERATION") or ("check" if policy["install_profile"] == "quick" else "full")
            value = plan(json.loads(Path(args.config).read_text()), os.environ["GITHUB_REPOSITORY"],
                         own_sha, os.environ.get("REQUEST_COUNTERPART_SHA", ""),
                         operation, os.environ.get("REQUEST_GAME", "both"),
                         os.environ.get("REQUEST_MODE", "both"), raw == "true", event, trusted_pull)
            value.update(policy)
            value["claim"] = ("workflow-contract-only" if operation == "check" else
                              "verified-public-input-retention" if operation == "retain-inputs" else
                              policy["install_profile"] + "-installation")
            value.pop("plan_id", None)
            value["plan_id"] = hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
            emit_outputs(value)
        elif args.command == "check-credentials":
            value = require_stage_credentials(args.stage)
        elif args.command == "complete":
            digest = hashlib.sha256(Path(args.guide).read_bytes()).hexdigest()
            value = require_complete(json.loads(Path(args.report).read_text()), args.mode, digest)
        else:
            value = aggregate(args.operation, json.loads(os.environ.get("NEEDS_JSON", "{}")),
                              require_downloader=os.environ.get("REQUIRE_DOWNLOADER", "false") == "true",
                              require_comparison=os.environ.get("REQUIRE_COMPARISON", "false") == "true")
        if args.command != "check-credentials":
            write_json(args.output, value)
        print(json.dumps(value, sort_keys=True))
        return 0
    except (KeyError, OSError, TypeError, ValueError) as error:
        if args.command == "aggregate":
            write_json(args.output, {"operation": args.operation, "status": "failed", "reason": str(error),
                                     "cross_lane_parity": "unverified"})
        print("::error::full-run contract: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
