#!/usr/bin/env python3
"""Export verified production checkpoint manifests and compare six real lanes."""

import argparse
from collections import Counter
import hashlib
import itertools
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys

import fullrun_contract
import live_output
import ports


LIMIT = 128 * 1024 * 1024
BACKENDS = ("ody-patcher", "holo-patcher", "mod-packaged")
MODES = ("cli", "gui")
# The lane that runs each mod's own packaged patcher is the reference result.
# When another lane's tree differs from it, that other lane is the wrong one.
AUTHORITY = "mod-packaged"
SCHEMA = "production-patcher-comparison-v1"
RECEIPT_KEYS = {
    "schema", "game", "backend", "mode", "engine_sha", "binary_sha256", "guide_sha256",
    "input_sha256", "profile", "selected_guid_order", "session_chain", "boundaries",
    "final_game_tree_sha256", "baseline_game_tree_sha256", "patcher_operations",
    "external_requests_sha256", "lane_report_sha256", "checkpoint_commands",
    "checkpoint_commit_verified",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False, allow_nan=False).encode("utf-8")


def value_hash(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def file_hash(path):
    with regular(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def regular(path):
    path = Path(path)
    require(stat.S_ISREG(path.lstat().st_mode), "Expected a regular nonsymlink file")
    return path


def load(path):
    path = regular(path)
    require(path.stat().st_size <= LIMIT, "JSON document exceeds 128 MiB")

    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "Duplicate JSON key")
            result[key] = value
        return result

    return json.loads(path.read_bytes(), object_pairs_hook=unique,
                      parse_constant=lambda _: require(False, "Nonfinite JSON value"))


def write(path, value):
    body = canonical(value) + b"\n"
    require(len(body) <= LIMIT, "Export document exceeds 128 MiB")
    # An existing export is never overwritten, including through a link.
    with Path(path).open("xb") as stream:
        os.chmod(path, 0o600)
        stream.write(body)


def sha(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def tree_valid(tree):
    require(set(tree) == {"rootMode", "entries"}, "Unexpected tree schema")
    modes = [tree["rootMode"]]
    require(isinstance(tree["entries"], dict), "Missing tree entries")
    for name, entry in tree["entries"].items():
        path = PurePosixPath(name)
        require(name and not path.is_absolute() and "\\" not in name
                and all(part not in ("", ".", "..") for part in name.split("/")),
                "Nonportable game path")
        expected = {"kind", "mode", "sha256", "size"} if entry.get("kind") == "file" else {"kind", "mode"}
        require(set(entry) == expected and entry["kind"] in ("file", "directory"),
                "Unsupported game tree entry")
        if entry["kind"] == "file":
            require(sha(entry["sha256"]) and type(entry["size"]) is int and entry["size"] >= 0,
                    "Invalid file identity")
        modes.append(entry["mode"])
    for mode in modes:
        require(set(mode) == {"readonly", "unix"} and type(mode["readonly"]) is bool
                and (mode["unix"] is None or type(mode["unix"]) is int), "Invalid game mode")


class Checkpoints:
    def __init__(self, binary, archives, game, output):
        self.binary = regular(binary).resolve()
        self.roots = ["--source-dir", str(archives), "--game-dir", str(game)]
        self.output = output
        self.calls = []
        self.documents = {}
        self.sessions = {}

    def batch(self, session, checkpoint):
        bundle = self.output / "verified-history"
        command = [str(self.binary), "checkpoint", "export-history", *self.roots,
                   "--session", session, "--checkpoint", checkpoint, "--output-dir", str(bundle)]
        log = self.output.parent / (self.output.name + "-checkpoint-batch.log")
        process = live_output.run(command, log, stdin=subprocess.DEVNULL, timeout=7200)
        require(process.returncode == 0, "Production batch history verification failed")
        report = load(log)
        require(report.get("schemaVersion") == 1 and report.get("operation") == "exportHistory"
                and report.get("sessionId") == session and report.get("checkpointId") == checkpoint,
                "Unexpected batch history command identity")
        index = retained(bundle, "index.json", report["indexSha256"])
        require(index.get("schemaVersion") == 1 and index.get("operation") == "exportHistory"
                and index.get("sessionId") == session and index.get("checkpointId") == checkpoint
                and index.get("verifiedObjects") == report.get("verifiedObjects")
                and isinstance(index.get("documents"), list)
                and len(index["documents"]) == report.get("documents")
                and 0 < len(index["documents"]) <= 50_000,
                "Batch history index differs from authenticated command")
        names = {"index.json"}
        for row in index["documents"]:
            name = row["document"]
            require(isinstance(name, str) and Path(name).name == name and name.endswith(".json")
                    and "\\" not in name and name not in names and sha(row.get("sha256")),
                    "Unsafe or duplicate batch history document")
            names.add(name)
            value = retained(bundle, name, row["sha256"])
            key = (row["operation"], row["sessionId"], row["checkpointId"])
            require(row["operation"] in ("inspect", "verify") and key not in self.documents
                    and value.get("schemaVersion") == 1 and value.get("operation") == row["operation"]
                    and value.get("sessionId") == row["sessionId"]
                    and value.get("checkpointId") == row["checkpointId"],
                    "Batch document identity differs from its index")
            target = self.output / name
            require(not target.exists() and not target.is_symlink(), "Batch destination already exists")
            (bundle / name).rename(target)
            self.documents[key] = target
            if row["operation"] == "inspect":
                self.sessions.setdefault(row["sessionId"], []).append(row["checkpointId"])
            self.calls.append({"command": command, "document": name, "sha256": row["sha256"]})
        require({path.name for path in bundle.iterdir()} == {"index.json"},
                "Batch export contains unreferenced documents")
        (bundle / "index.json").unlink()
        bundle.rmdir()

    def call(self, operation, session, checkpoint):
        key = (operation, session, checkpoint)
        require(key in self.documents, "Batch export lacks a required verified boundary")
        return load(self.documents[key])

    def checkpoint(self, boundary):
        return load(boundary["document"])["checkpoint"]


def collect_history(reader, session_id, checkpoint_id, seen=None):
    seen = set() if seen is None else seen
    require(session_id not in seen and len(seen) < 128, "Checkpoint lineage cycles or exceeds bound")
    seen.add(session_id)
    require(session_id in reader.sessions, "Session is missing from verified batch ancestry")
    head = reader.call("inspect", session_id, checkpoint_id)
    require(not head["pendingOperations"] and head["headCheckpointId"] == checkpoint_id,
            "Session has pending work or changed HEAD")
    session = head["session"]
    head.pop("checkpoint")
    require(session["sessionId"] == session_id and session["installScope"] == "install"
            and session.get("origin") == "rust" and not session.get("participants"),
            "Session is not an independent Rust physical install")
    require(head["checkpointId"] == checkpoint_id and head.get("finalizationReceipt"),
            "Missing finalization receipt")
    typed = head["finalizationReceipt"].get("installReport") or {}
    identity = typed.get("checkpoint") or {}
    require(identity.get("sessionId") == session_id and identity.get("checkpointId") == checkpoint_id,
            "Finalization receipt has a different install identity")
    checkpoints = []
    for cp_id in reader.sessions[session_id]:
        reader.call("verify", session_id, cp_id)
        inspected = reader.call("inspect", session_id, cp_id)
        require(inspected["session"] == session and not inspected["pendingOperations"]
                and inspected["headCheckpointId"] == checkpoint_id,
                "Session changed during checkpoint export")
        cp = inspected["checkpoint"]
        require(cp["sessionId"] == session_id and cp["checkpointId"] == cp_id
                and value_hash(cp["completedComponents"]) == cp["completedSha256"],
                "Checkpoint identity changed")
        tree_valid(cp["game"])
        checkpoints.append({key: cp[key] for key in ("checkpointId", "sequence", "kind", "component")})
        checkpoints[-1]["document"] = reader.documents[("inspect", session_id, cp_id)]
    checkpoints.sort(key=lambda cp: cp["sequence"])
    require(checkpoints and checkpoints[0]["kind"] == "Baseline"
            and checkpoints[-1]["checkpointId"] == checkpoint_id, "Incomplete committed ancestry")
    ancestors, chain = [], []
    if session.get("parentSessionId"):
        ancestors, chain = collect_history(reader, session["parentSessionId"],
                                           session["parentCheckpointId"], seen)
        parent_boundary = reader.checkpoint(ancestors[-1])
        child_baseline = reader.checkpoint(checkpoints[0])
        require(parent_boundary["game"] == child_baseline["game"]
                and parent_boundary["completedComponents"] == child_baseline["completedComponents"],
                "Child session baseline differs from its verified parent boundary")
        checkpoints = checkpoints[1:]
    else:
        require(not session.get("initialCompletedComponents"),
                "Inherited prefix has no verified parent session")
    chain.append({"session_id": session_id, "checkpoint_id": checkpoint_id,
                  "definitions_sha256": session["definitionsSha256"],
                  "finalization_receipt": head["finalizationReceipt"]})
    return ancestors + checkpoints, chain


def validate_requests(requests, backend, definitions, game, guide_sha):
    expected = {(component["Guid"].lower(), index) for component in definitions
                for index, step in enumerate(component.get("Instructions", []))
                if step.get("Action") == "Patcher"}
    if backend == "ody-patcher":
        require(not requests, "Native lane contains external Patcher requests")
        return sorted(expected)
    owners = {component["Guid"].lower(): component for component in definitions}
    observed = set()
    for request in requests:
        require(request.get("schemaVersion") == 1 and request["backend"] == backend,
                "External request backend differs")
        key = (str(request.get("componentGuid", "")).lower(), request.get("operationIndex"))
        require(type(key[1]) is int and key in expected and key not in observed,
                "Unexpected or repeated external Patcher operation")
        observed.add(key)
        authority = request.get("originalSourceAuthority") or {}
        require(authority.get("Policy") == "fresh-original-external-package-v1"
                and str(authority.get("ComponentGuid", "")).lower() == key[0]
                and authority.get("InstructionIndex") == key[1]
                and authority.get("GuideSha256") == guide_sha
                and request.get("instruction") == owners[key[0]]["Instructions"][key[1]],
                "External request differs from installed source operation")
        require(request.get("status") == "processAndLogVerified"
                and request.get("writerQuiescenceVerified") is True
                and Path(request.get("gameTarget", "")).resolve() == game,
                "External process was not verified for this game")
        process = request.get("processReceipt") or {}
        require(type(process.get("exitCode")) is int and process["exitCode"] == 0
                and process.get("cancelled") is False
                and process.get("failure") is None and process.get("processGroupQuiescent") is True
                and process.get("noChildStarted") is False,
                "External backend did not execute successfully and quiesce")
        require(request.get("backendIdentity") and request.get("originalSourceAuthority")
                and request.get("launch") and request.get("checkpointOperations")
                and request.get("rawPackage") and request.get("submittedPackage")
                and isinstance(request.get("outputArtifacts"), dict)
                and sha((request.get("installLog") or {}).get("sha256")),
                "External backend identity, original inputs, artifacts or log evidence missing")
        identity = request["backendIdentity"]
        require(identity.get("kind") in (("originalPackagedExecutable",) if backend == "mod-packaged"
                                         else ("python", "executable")), "Backend executable identity differs")
        program = identity.get("entrypoint") if identity["kind"] == "python" else identity.get("program")
        require(isinstance(program, dict) and sha(program.get("sha256")), "Backend executable digest missing")
        require(backend != "mod-packaged" or program["sha256"] == request.get("executableSha256"),
                "Packaged executable differs from original source package")
        require(hashlib.sha256(request["installLog"].get("text", "").encode()).hexdigest()
                == request["installLog"]["sha256"], "External install log digest differs")
    require(observed == expected, "External requests do not cover every selected Patcher operation")
    return sorted(expected)


def request_evidence(directory, backend, definitions, game, guide_sha):
    requests = []
    if directory is not None:
        directory = Path(directory)
        require(directory.is_dir() and not directory.is_symlink(), "Unsafe external request directory")
        for path in sorted(directory.glob("*.json")):
            request = load(path)
            if "backend" in request:
                requests.append(request)
    operations = validate_requests(requests, backend, definitions, game, guide_sha)
    return requests, operations


def execution_command(execution):
    command = execution.get("command")
    require(isinstance(command, list) and command and all(isinstance(arg, str) for arg in command),
            "Missing actual installed launch command")
    require(len(command) > 1 and command[1] == ("gui" if execution["mode"] == "gui" else "install"),
            "Installed launch used a different CLI/GUI route")
    require(command.count("--patcher-engine") == 1, "Installed launch has no unique backend selection")
    index = command.index("--patcher-engine") + 1
    require(index < len(command) and command[index] == execution["backend"],
            "Installed launch selected a different backend")
    return command


def export(args):
    output = Path(args.output_dir)
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    game, archives = Path(args.game_dir).resolve(strict=True), Path(args.archives).resolve(strict=True)
    require(not output.resolve().is_relative_to(game) and not output.resolve().is_relative_to(archives),
            "Export output must be outside installed roots")
    binary_digest = file_hash(args.binary)
    report = load(args.lane_report)
    fullrun_contract.require_complete(report, args.mode)
    execution = report.get("patcher_comparison_execution") or {}
    require(execution.get("schema_version") == 1 and execution.get("backend") == args.backend
            and execution.get("mode") == args.mode and execution.get("binary_sha256") == binary_digest,
            "Installed lane execution identity differs from requested binary/backend/mode")
    command = execution_command(execution)
    require(Path(command[0]).resolve() == Path(args.binary).resolve(),
            "Installed launch used a different binary")
    require(re.fullmatch(r"[0-9a-f]{40}", execution.get("engine_sha", "")) is not None,
            "Missing exact engine commit")
    require(execution.get("input_digest") == file_hash(args.converted_guide)
            == report.get("converted_sha256")
            and execution.get("guide_digest") == report.get("guide_sha256"),
            "Lane input/guide identity differs")
    profile = report.get("install_profile")
    require(isinstance(profile, dict) and execution.get("profile") == profile["profile"],
            "Missing admitted installation profile")
    selected = execution.get("selected_guid_order")
    require(isinstance(selected, list) and len(selected) == len(set(selected))
            and set(selected) == set(profile["selected_guids"]), "Selected execution order differs from profile")
    reader = Checkpoints(args.binary, archives, game, output)
    reader.batch(execution["session_id"], execution["checkpoint_id"])
    checkpoints, chain = collect_history(reader, execution["session_id"], execution["checkpoint_id"])
    definitions = reader.checkpoint(checkpoints[-1])["completedComponents"]
    require([c["Guid"] for c in definitions] == selected, "Store completed order differs from admitted profile")
    require([cp["component"]["guid"] for cp in checkpoints if cp["kind"] == "Component"] == selected,
            "Missing, repeated or out-of-order component boundary")
    components = {c["Guid"]: c for c in ports.components_of(Path(args.converted_guide))}
    require(set(selected) <= components.keys(), "Converted input lacks installed owners")
    for definition in definitions:
        # Guide reload canonicalizes instruction Source separators before Store capture.
        admitted = components[definition["Guid"]]
        normalized = {**admitted, "Instructions": [
            {**step, "Source": [source.replace("\\", "/") for source in step["Source"]]}
            if isinstance(step.get("Source"), list) else step
            for step in admitted.get("Instructions", [])]}
        require(ports.source_context_equal(admitted, definition)
                or ports.source_context_equal(normalized, definition),
                "Store definition differs from admitted converted owner")
    requests, operations = request_evidence(execution.get("request_dir"), args.backend, definitions, game,
                                            execution["guide_digest"])
    boundaries, occurrences = [], Counter()
    for index, boundary in enumerate(checkpoints):
        cp = reader.checkpoint(boundary)
        if cp["kind"] == "Component":
            guid = cp["component"]["guid"]
            occurrences[guid] += 1
            key = f"component/{guid}/{occurrences[guid]}"
        else:
            key = "sessionBaseline" if cp["kind"] == "Baseline" else "preparation"
        name = f"boundary-{index:04}.json"
        value = {"key": key, "session_id": cp["sessionId"], "checkpoint_id": cp["checkpointId"],
                 "definition_prefix_sha256": cp["completedSha256"], "game": cp["game"]}
        require(sha(value["definition_prefix_sha256"]), "Missing Store definition prefix digest")
        write(output / name, value)
        boundaries.append({"key": key, "document": name, "sha256": file_hash(output / name),
                           "game_tree_sha256": value_hash(cp["game"]),
                           "definition_prefix_sha256": cp["completedSha256"]})
    write(output / "external-requests.json", requests)
    write(output / "lane-report.json", report)
    receipt = {"schema": SCHEMA, "game": profile["game"], "backend": args.backend, "mode": args.mode,
               "engine_sha": execution["engine_sha"], "binary_sha256": binary_digest,
               "guide_sha256": execution["guide_digest"], "input_sha256": execution["input_digest"],
               "profile": profile["profile"], "selected_guid_order": selected,
               "session_chain": chain, "boundaries": boundaries,
               "final_game_tree_sha256": boundaries[-1]["game_tree_sha256"],
               "baseline_game_tree_sha256": boundaries[0]["game_tree_sha256"],
               "checkpoint_commit_verified": True,
               "patcher_operations": operations, "external_requests_sha256": file_hash(output / "external-requests.json"),
               "lane_report_sha256": file_hash(output / "lane-report.json"), "checkpoint_commands": reader.calls}
    require(file_hash(args.binary) == binary_digest, "Production binary changed during export")
    write(output / "receipt.json", receipt)
    print(json.dumps({"event": "patcher_comparison_export", "backend": args.backend, "mode": args.mode,
                      "boundaries": len(boundaries), "external_processes": len(requests),
                      "receipt": str(output / "receipt.json")}))


def retained(root, name, checksum):
    require(isinstance(name, str) and Path(name).name == name, "Unsafe retained document path")
    path = root / name
    require(file_hash(path) == checksum, "Retained document digest differs")
    return load(path)


def differences(left, right):
    tree_valid(left)
    tree_valid(right)
    changes = []
    if left["rootMode"] != right["rootMode"]:
        changes.append({"path": ".", "change": "changed", "before": left["rootMode"], "after": right["rootMode"]})
    for path in sorted(left["entries"].keys() | right["entries"].keys()):
        before, after = left["entries"].get(path), right["entries"].get(path)
        if before != after:
            changes.append({"path": path, "change": "added" if before is None else "removed" if after is None else "changed",
                            "before": before, "after": after})
    return changes


def comparison_pairs():
    """(reference, candidate) cells. Each backend is held against the packaged-patcher
    lane of the same route; the packaged GUI route is held against its CLI route."""
    pairs = [((AUTHORITY, mode), (backend, mode)) for mode in MODES
             for backend in BACKENDS if backend != AUTHORITY]
    pairs.append(((AUTHORITY, "cli"), (AUTHORITY, "gui")))
    return pairs


def patcher_boundaries(keys, operations):
    """Boundaries worth hashing: preparation and each component that ran a Patcher
    operation. Components without a patcher cannot differ between patcher lanes
    except by carrying an earlier difference, which the earlier boundary already names."""
    guids = {str(operation[0]).lower() for operation in operations}
    return [key for key in keys if key == "preparation"
            or (key.startswith("component/") and key.split("/")[1].lower() in guids)]


def judge(trees, operations, load_tree):
    """Compare every candidate lane with its reference after each patcher component.

    trees maps (backend, mode) to an ordered {boundary key: handle}; load_tree(handle)
    returns that boundary's game tree. Returns (comparisons, wrong_lanes)."""
    comparisons, wrong = [], []
    for reference, candidate in comparison_pairs():
        keys = patcher_boundaries(list(trees[reference]), operations) + ["final"]
        result = {"reference": list(reference), "candidate": list(candidate), "boundaries": []}
        first, count = None, 0
        for key in keys:
            ref_handle = list(trees[reference].values())[-1] if key == "final" else trees[reference][key]
            cand_handle = list(trees[candidate].values())[-1] if key == "final" else trees[candidate][key]
            changes = differences(load_tree(ref_handle), load_tree(cand_handle))
            result["boundaries"].append({"key": key, "changes": changes})
            if changes and first is None:
                first = key
            count += len(changes)
        comparisons.append(result)
        if count:
            wrong.append({"lane": list(candidate), "reference": list(reference),
                          "first_differing_boundary": first, "differences": count})
    return comparisons, wrong


def compare(args):
    require(len(args.receipt) == 6, "Exactly six independently exported receipts are required")
    lanes = []
    cells = set()
    for path in map(Path, args.receipt):
        receipt = load(path)
        require(set(receipt) == RECEIPT_KEYS and receipt.get("schema") == SCHEMA,
                "Unsupported comparison receipt schema")
        require(isinstance(receipt["session_chain"], list) and receipt["session_chain"]
                and isinstance(receipt["boundaries"], list) and receipt["boundaries"]
                and isinstance(receipt["selected_guid_order"], list) and receipt["selected_guid_order"],
                "Empty session, boundary or selected owner coverage")
        require(receipt["checkpoint_commit_verified"] is True,
                "Component checkpoint commits have not been verified")
        require(all(sha(receipt[key]) for key in ("binary_sha256", "guide_sha256", "input_sha256",
                                                "baseline_game_tree_sha256", "final_game_tree_sha256"))
                and re.fullmatch(r"[0-9a-f]{40}", receipt["engine_sha"]) is not None,
                "Invalid source or engine digest")
        cell = (receipt["backend"], receipt["mode"])
        require(cell in set(itertools.product(BACKENDS, MODES)) and cell not in cells,
                "Repeated or unsupported comparison cell")
        cells.add(cell)
        root = path.parent
        report = retained(root, "lane-report.json", receipt["lane_report_sha256"])
        fullrun_contract.require_complete(report, receipt["mode"], receipt["guide_sha256"], receipt["profile"])
        require((report.get("install_profile") or {}).get("game") == receipt["game"],
                "Exported game differs from admitted profile")
        execution = report.get("patcher_comparison_execution", {})
        execution_command(execution)
        require(execution.get("backend") == receipt["backend"] and execution.get("mode") == receipt["mode"]
                and execution.get("binary_sha256") == receipt["binary_sha256"]
                and execution.get("engine_sha") == receipt["engine_sha"]
                and execution.get("input_digest") == receipt["input_sha256"]
                and execution.get("guide_digest") == receipt["guide_sha256"]
                and execution.get("profile") == receipt["profile"]
                and execution.get("selected_guid_order") == receipt["selected_guid_order"],
                "Exported execution identity differs from installed lane")
        inspections, verified = {}, set()
        for command in receipt["checkpoint_commands"]:
            document = retained(root, command["document"], command["sha256"])
            require(document.get("schemaVersion") == 1 and document.get("operation") in
                    ("list", "history", "inspect", "verify"), "Unexpected retained Store command")
            if document["operation"] in ("inspect", "verify"):
                identity = (document["sessionId"], document["checkpointId"])
                if document["operation"] == "inspect":
                    require(not document["pendingOperations"], "Retained inspection has pending work")
                    inspections[identity] = command
                else:
                    verified.add(identity)
        final = receipt["session_chain"][-1]
        final_id = (final["session_id"], final["checkpoint_id"])
        require(final_id in inspections and final_id in verified, "Final Store verification is missing")
        final_command = inspections[final_id]
        inspection = retained(root, final_command["document"], final_command["sha256"])
        require(inspection["finalizationReceipt"] == final["finalization_receipt"]
                and inspection["session"]["definitionsSha256"] == final["definitions_sha256"],
                "Exported session differs from Store inspection")
        previous = None
        for session in receipt["session_chain"]:
            identity = (session["session_id"], session["checkpoint_id"])
            require(identity in inspections and identity in verified, "Session lineage verification missing")
            command = inspections[identity]
            original = retained(root, command["document"], command["sha256"])
            parent = original["session"]
            require(original["finalizationReceipt"] == session["finalization_receipt"]
                    and parent["definitionsSha256"] == session["definitions_sha256"],
                    "Session lineage finalization/definitions differ")
            if previous is None:
                require(not parent.get("parentSessionId") and not parent.get("initialCompletedComponents"),
                        "Original session baseline has an inherited prefix")
            else:
                require(parent.get("parentSessionId") == previous["session_id"]
                        and parent.get("parentCheckpointId") == previous["checkpoint_id"],
                        "Exported session lineage is disconnected")
            previous = session
        definitions = inspection["checkpoint"]["completedComponents"]
        require([c["Guid"] for c in definitions] == receipt["selected_guid_order"],
                "Retained Store completed order differs")
        requests = retained(root, "external-requests.json", receipt["external_requests_sha256"])
        operations = validate_requests(requests, receipt["backend"], definitions,
                                       Path(inspection["session"]["roots"]["game"]), receipt["guide_sha256"])
        require([list(op) for op in operations] == receipt["patcher_operations"],
                "Retained backend operation coverage differs")
        trees = {}
        for boundary in receipt["boundaries"]:
            value = retained(root, boundary["document"], boundary["sha256"])
            identity = (value["session_id"], value["checkpoint_id"])
            require(identity in inspections and identity in verified,
                    "Boundary lacks production Store inspection and verification")
            command = inspections[identity]
            checkpoint = retained(root, command["document"], command["sha256"])["checkpoint"]
            require(checkpoint["game"] == value["game"]
                    and checkpoint["completedSha256"] == value["definition_prefix_sha256"]
                    and value_hash(checkpoint["completedComponents"]) == checkpoint["completedSha256"],
                    "Exported game tree or definition prefix differs from Store")
            require(value["key"] == boundary["key"] and value_hash(value["game"]) == boundary["game_tree_sha256"]
                    and value["definition_prefix_sha256"] == boundary["definition_prefix_sha256"]
                    and value["key"] not in trees, "Exported boundary identity differs")
            tree_valid(value["game"])
            trees[value["key"]] = (root, boundary)
        require(list(trees) == ["sessionBaseline"] + (["preparation"] if "preparation" in trees else [])
                + [f"component/{guid}/1" for guid in receipt["selected_guid_order"]],
                "Exported boundary coverage/order is incomplete")
        lanes.append((receipt, trees))
    first = lanes[0][0]
    for receipt, trees in lanes:
        for key in ("game", "engine_sha", "guide_sha256", "profile", "selected_guid_order", "baseline_game_tree_sha256"):
            require(receipt[key] == first[key], "Six-lane comparison identity/baseline differs")
        require(list(trees) == list(lanes[0][1]), "Preparation/component boundary sets differ")
        require(trees["sessionBaseline"][1]["game_tree_sha256"] == receipt["baseline_game_tree_sha256"]
                and list(trees.values())[-1][1]["game_tree_sha256"] == receipt["final_game_tree_sha256"],
                "Baseline or final tree digest differs")
    require(len({lane[0]["session_chain"][-1]["session_id"] for lane in lanes}) == 6,
            "Six cells reused an installation session")
    for mode in MODES:
        require(len({receipt["binary_sha256"] for receipt, _ in lanes if receipt["mode"] == mode}) == 1,
                "Backend cells used different binaries for the same route")
    output = Path(args.output_dir)
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    cells = {(receipt["backend"], receipt["mode"]): (receipt, trees) for receipt, trees in lanes}
    authority = cells[(AUTHORITY, MODES[0])][0]
    require(all(receipt["patcher_operations"] == authority["patcher_operations"] for receipt, _ in lanes),
            "Lanes disagree on which components run a Patcher operation")
    comparisons, wrong = judge({cell: trees for cell, (_, trees) in cells.items()},
                               authority["patcher_operations"],
                               lambda handle: retained(handle[0], handle[1]["document"],
                                                       handle[1]["sha256"])["game"])
    total, pairs = 0, []
    for pair_index, result in enumerate(comparisons):
        pair = {"reference": result["reference"], "candidate": result["candidate"], "boundaries": []}
        for boundary_index, boundary in enumerate(result["boundaries"]):
            changes = boundary["changes"]
            total += len(changes)
            observed = Counter(change["change"] for change in changes)
            counts = {kind: observed[kind] for kind in ("added", "removed", "changed")}
            name = f"diff-pair-{pair_index:02}-boundary-{boundary_index:04}.json"
            write(output / name, {"reference": pair["reference"], "candidate": pair["candidate"],
                                  "key": boundary["key"], "changes": changes})
            pair["boundaries"].append({"key": boundary["key"], "counts": counts, "document": name,
                                       "sha256": file_hash(output / name)})
            # left/right are kept for existing log consumers: left is the reference lane.
            print(json.dumps({"event": "patcher_comparison_diff", "left": pair["reference"],
                              "right": pair["candidate"], "boundary": boundary["key"], "counts": counts,
                              "differences": len(changes)}), flush=True)
        pairs.append(pair)
    write(output / "private-diff.json", {"schema": SCHEMA, "game": first["game"], "authority": AUTHORITY,
                                         "pairs": pairs})
    write(output / "summary.json", {"schema": SCHEMA, "game": first["game"], "cells": 6,
                                    "pairs": len(pairs), "differences": total, "equal": total == 0,
                                    "authority": AUTHORITY, "wrong_lanes": wrong,
                                    "patcher_components": len({str(op[0]).lower() for op in
                                                               authority["patcher_operations"]}),
                                    "engine_sha": first["engine_sha"], "guide_sha256": first["guide_sha256"],
                                    "profile": first["profile"],
                                    "component_count": len(first["selected_guid_order"])})
    for lane in wrong:
        print(f"::error title=Patcher lane differs from {AUTHORITY}::{first['game']}: "
              f"{lane['lane'][0]} ({lane['lane'][1]}) differs from {lane['reference'][0]} "
              f"({lane['reference'][1]}) first at {lane['first_differing_boundary']} "
              f"({lane['differences']} file differences); {lane['lane'][0]} is the incorrect lane",
              file=sys.stderr, flush=True)
    # Admission and execution failures raise above; a completed comparison fails when
    # any lane disagrees with the packaged-patcher reference.
    return 1 if wrong else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    capture = sub.add_parser("export")
    for name in ("binary", "game-dir", "archives", "converted-guide", "lane-report", "output-dir"):
        capture.add_argument("--" + name, required=True)
    capture.add_argument("--backend", required=True, choices=BACKENDS)
    capture.add_argument("--mode", required=True, choices=MODES)
    comparison = sub.add_parser("compare")
    comparison.add_argument("--receipt", action="append", required=True)
    comparison.add_argument("--output-dir", required=True)
    args = parser.parse_args()
    try:
        return compare(args) if args.command == "compare" else export(args) or 0
    except (ValueError, OSError, KeyError, TypeError) as error:
        print(json.dumps({"event": "patcher_comparison_failed", "reason": str(error)}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
