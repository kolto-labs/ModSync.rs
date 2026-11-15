#!/usr/bin/env python3
"""Authenticate six current-run encrypted comparison exports and compare them."""

import argparse
import contextlib
from datetime import datetime, timezone
import gzip
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import time
import zipfile

from artstore_client import ArtifactError, ArtifactsClient
import evidence_receipt
import patcher_comparison as comparison
from production_build import API, REPOSITORY
import seal


WORKFLOW = ".github/workflows/fullrun.yml"
MAX_ZIP = 2 * 1024 * 1024 * 1024
MAX_PRIVATE = 8 * 1024 * 1024 * 1024
MAX_FILES = 50_000
DISK_RESERVE = 512 * 1024 * 1024
PACK_NAME = "comparison.tar.gz"
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")
require = comparison.require


def expected_identity(environment):
    require(environment.get("GITHUB_REPOSITORY") == REPOSITORY, "Comparison requires its owning repository")
    require(environment.get("GITHUB_TOKEN"), "Built-in Actions token is required")
    require(len(environment.get("CACHE_ENCRYPTION_KEY", "").encode()) >= seal.MIN_KEY_BYTES,
            "Existing game-cache encryption key is required")
    value = evidence_receipt.identity()
    source = evidence_receipt.source_identity(environment)
    require(all(re.fullmatch(r"[0-9a-f]{40}", environment.get(field, "")) is not None
                for field in ("GITHUB_SHA", "GITHUB_WORKFLOW_SHA")), "Missing exact execution/workflow SHA")
    require(all(re.fullmatch(r"[1-9][0-9]*", environment.get(field, "")) is not None
                for field in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_REPOSITORY_ID",
                              "GITHUB_REPOSITORY_OWNER_ID")), "Missing Actions run/repository identity")
    require(value["workflow_ref"] == REPOSITORY + "/" + WORKFLOW + "@" + value["ref"],
            "Unexpected workflow execution reference")
    value["job"] = "install"
    return value, source


def validate_run(api, expected, source, environment):
    repository = api.get("")
    require(repository.get("full_name") == REPOSITORY
            and str(repository.get("id")) == environment["GITHUB_REPOSITORY_ID"]
            and str((repository.get("owner") or {}).get("id")) == environment["GITHUB_REPOSITORY_OWNER_ID"],
            "Repository API identity differs")
    workflow = api.get("/actions/workflows/fullrun.yml")
    require(workflow.get("path") == WORKFLOW, "Workflow API path differs")
    run_id, attempt = int(expected["run_id"]), int(expected["run_attempt"])
    run = api.get(f"/actions/runs/{run_id}")
    current = api.get(f"/actions/runs/{run_id}/attempts/{attempt}")
    for value in (run, current):
        require(value.get("id") == run_id and value.get("run_attempt") == attempt
                and value.get("workflow_id") == workflow["id"] and value.get("path") == WORKFLOW
                and value.get("head_sha") == source["source_sha"]
                and value.get("event") == source["event_name"]
                and (value.get("repository") or {}).get("id") == repository["id"]
                and (value.get("head_repository") or {}).get("id") == repository["id"],
                "Current workflow run/attempt/source API identity differs")
    return repository, run


def artifact_name(game, backend, mode, expected):
    return f"comparison-{game}-{backend}-{mode}-{expected['run_id']}-{expected['run_attempt']}"


def validate_artifact(artifact, name, repository, expected, source):
    require(artifact.get("name") == name and type(artifact.get("id")) is int and artifact["id"] > 0,
            "Comparison artifact name/ID differs")
    require(artifact.get("expired") is False, "Comparison artifact expired")
    expires = datetime.fromisoformat(artifact["expires_at"].replace("Z", "+00:00"))
    require(expires > datetime.now(timezone.utc), "Comparison artifact expiry elapsed")
    require(DIGEST.fullmatch(artifact.get("digest", "")) is not None
            and type(artifact.get("size_in_bytes")) is int and 0 < artifact["size_in_bytes"] <= MAX_ZIP,
            "Comparison artifact lacks a bounded API ZIP digest/size")
    run = artifact.get("workflow_run") or {}
    require(run.get("id") == int(expected["run_id"]) and run.get("repository_id") == repository["id"]
            and run.get("head_repository_id") == repository["id"] and run.get("head_sha") == source["source_sha"],
            "Comparison artifact belongs to another source/run/repository")


def selected_artifacts(api, repository, expected, source, game):
    found = []
    for page in range(1, 21):
        response = api.get(f"/actions/runs/{expected['run_id']}/artifacts?per_page=100&page={page}")
        batch = response.get("artifacts")
        total = response.get("total_count")
        require(isinstance(batch, list) and type(total) is int and 0 <= total <= 2000,
                "Run artifact inventory exceeds bound")
        found.extend(batch)
        if len(found) >= total:
            require(len(found) == total, "Run artifact inventory changed during listing")
            break
        require(len(batch) == 100, "Incomplete run artifact page")
    else:
        raise ValueError("Run artifact pagination exceeded bound")
    selected = []
    for backend, mode in itertools.product(comparison.BACKENDS, comparison.MODES):
        name = artifact_name(game, backend, mode, expected)
        matches = [artifact for artifact in found if artifact.get("name") == name]
        require(len(matches) == 1, "Missing or duplicate current comparison cell artifact")
        artifact = matches[0]
        validate_artifact(artifact, name, repository, expected, source)
        fresh = api.get(f"/actions/artifacts/{artifact['id']}")
        validate_artifact(fresh, name, repository, expected, source)
        require(all(fresh[key] == artifact[key] for key in ("id", "name", "digest", "size_in_bytes", "workflow_run")),
                "Comparison artifact metadata changed before download")
        selected.append((backend, mode, fresh))
    require(len({artifact["id"] for _, _, artifact in selected}) == 6,
            "Comparison cells reused an artifact")
    return selected


def unpack_envelope(archive_path, destination):
    with zipfile.ZipFile(archive_path) as archive:
        members = archive.infolist()
        require(len(members) == 2 and {member.filename for member in members}
                == {"evidence.seal", "fullrun-provenance.json"}, "Unexpected comparison artifact ZIP members")
        for member in members:
            mode = stat.S_IFMT(member.external_attr >> 16)
            limit = MAX_ZIP if member.filename == "evidence.seal" else 1024 * 1024
            require(not member.is_dir() and mode in (0, stat.S_IFREG) and not member.flag_bits & 1
                    and 0 < member.file_size <= limit, "Unsafe comparison artifact ZIP member")
            with archive.open(member) as incoming, (destination / member.filename).open("xb") as outgoing:
                shutil.copyfileobj(incoming, outgoing, 1024 * 1024)


def validate_outer_provenance(outer, expected, ciphertext_sha256):
    require(set(outer) == set(expected) | {"ciphertext_sha256", "encryption"}
            and outer.get("encryption") == "legacy-v1"
            and outer.get("ciphertext_sha256") == ciphertext_sha256,
            "Comparison encryption/ciphertext provenance differs")
    require({key: outer[key] for key in expected} == expected,
            "Outer comparison execution identity differs")


def private_inventory(directory, *, documents=False):
    total, count = 0, 0
    for parent, directories, files in os.walk(directory, followlinks=False):
        for name in directories:
            try:
                mode = (Path(parent) / name).lstat().st_mode
            except FileNotFoundError:
                require(not documents, "Decrypted directory disappeared")
                continue
            require(stat.S_ISDIR(mode), "Unsafe decrypted directory")
        for name in files:
            path = Path(parent) / name
            try:
                metadata = path.lstat()
            except FileNotFoundError:
                require(not documents, "Decrypted file disappeared")
                continue
            require(stat.S_ISREG(metadata.st_mode), "Unsafe decrypted file")
            require(metadata.st_size <= (comparison.LIMIT if documents else MAX_ZIP), "Private file exceeds bound")
            total += metadata.st_size
            count += 1
            require(total <= MAX_PRIVATE and count <= MAX_FILES, "Private comparison export exceeds resource bounds")


def decrypt_envelope(envelope, destination, workspace):
    with envelope.open("rb") as stream:
        require(stream.read(len(seal.MAGIC)) == seal.MAGIC,
                "Comparison export must use the existing symmetric game key")
    environment = dict(os.environ)
    for key in ("GITHUB_TOKEN", "ARTSTORE_TOKEN", "DIAGNOSTICS_ENCRYPTION_KEY",
                "DIAGNOSTICS_ENCRYPTION_RECIPIENT"):
        environment.pop(key, None)
    environment["TMPDIR"] = str(workspace)

    def resource_limits():
        resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_ZIP, MAX_ZIP))

    # The existing seal CLI invokes seal.unseal. Its process group also owns OpenSSL.
    command = [sys.executable, str(Path(seal.__file__).resolve()), "unseal", "--source", str(envelope),
               "--destination", str(destination)]
    process = subprocess.Popen(command, env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               start_new_session=True, preexec_fn=resource_limits)
    try:
        deadline = time.monotonic() + 900
        while process.poll() is None:
            require(time.monotonic() <= deadline, "Comparison decryption timed out")
            private_inventory(workspace)
            time.sleep(0.5)
        require(process.returncode == 0, "Comparison decryption/authentication failed")
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
        process.wait()
    private_inventory(destination)


def export_documents(directory):
    receipt_path = directory / "receipt.json"
    receipt = comparison.load(receipt_path)
    require(set(receipt) == comparison.RECEIPT_KEYS and receipt.get("schema") == comparison.SCHEMA,
            "Unsupported packed comparison receipt")
    documents = {"receipt.json": comparison.file_hash(receipt_path),
                 "lane-report.json": receipt["lane_report_sha256"],
                 "external-requests.json": receipt["external_requests_sha256"]}
    for row in receipt["boundaries"] + receipt["checkpoint_commands"]:
        name, digest = row["document"], row["sha256"]
        require(isinstance(name, str) and Path(name).name == name and name.endswith(".json")
                and "\\" not in name and comparison.sha(digest) and name not in documents,
                "Unsafe or repeated packed comparison document")
        documents[name] = digest
    require({path.name for path in directory.iterdir()} == set(documents),
            "Export contains unreferenced files or logs")
    return receipt, documents


def disk_space(directory, required):
    require(shutil.disk_usage(directory).free >= required + DISK_RESERVE,
            "Insufficient private disk space for comparison transport")


class HashingReader:
    def __init__(self, stream):
        self.stream = stream
        self.digest = hashlib.sha256()

    def read(self, size=-1):
        body = self.stream.read(size)
        self.digest.update(body)
        return body


class TarReader:
    """Keep only bounded read-ahead so tar padding can be checked after iteration."""
    def __init__(self, stream):
        self.stream, self.position, self.tail = stream, 0, b""

    def read(self, size):
        body = self.stream.read(size)
        self.position += len(body)
        self.tail = (self.tail + body)[-32768:]
        return body


def pack_export(source, output):
    require(source.is_dir() and not source.is_symlink(), "Unsafe comparison export directory")
    private_inventory(source, documents=True)
    _, documents = export_documents(source)
    output.mkdir(mode=0o700, parents=True)
    disk_space(output, 0)
    path = output / PACK_NAME
    total = 0
    try:
        with path.open("xb") as destination:
            with gzip.GzipFile(filename="", mode="wb", fileobj=destination, compresslevel=1, mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w|") as archive:
                    for name, digest in sorted(documents.items()):
                        flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
                        with os.fdopen(os.open(source / name, flags), "rb") as stream:
                            metadata = os.fstat(stream.fileno())
                            require(stat.S_ISREG(metadata.st_mode) and metadata.st_size <= comparison.LIMIT,
                                    "Unsafe comparison export member")
                            total += metadata.st_size
                            require(total <= MAX_PRIVATE, "Comparison export exceeds 8 GiB")
                            member = tarfile.TarInfo(name)
                            member.size, member.mode, member.mtime = metadata.st_size, 0o600, 0
                            incoming = HashingReader(stream)
                            archive.addfile(member, incoming)
                            require(not incoming.read(1) and incoming.digest.hexdigest() == digest,
                                    "Comparison document changed during packing")
                        require(destination.tell() <= MAX_ZIP - 1024 * 1024,
                                "Compressed comparison transport exceeds 2 GiB")
                        disk_space(output, 0)
        require(path.stat().st_size <= MAX_ZIP - 1024 * 1024,
                "Compressed comparison transport exceeds 2 GiB")
    except BaseException:
        path.unlink(missing_ok=True)
        raise
    return {"documents": len(documents), "uncompressed_bytes": total, "compressed_bytes": path.stat().st_size}


def unpack_export(source, destination):
    comparison.regular(source)
    require(source.stat().st_size <= MAX_ZIP, "Compressed comparison transport exceeds bound")
    destination.mkdir(mode=0o700)
    seen, total, end_offset = set(), 0, 0
    with source.open("rb") as stream, gzip.GzipFile(fileobj=stream, mode="rb") as compressed:
        observed = TarReader(compressed)
        with tarfile.open(fileobj=observed, mode="r|") as archive:
            for member in archive:
                name = member.name
                require(member.isreg() and Path(name).name == name and name.endswith(".json")
                        and "\\" not in name and not member.pax_headers and name not in seen,
                        "Unsafe packed comparison member")
                require(0 <= member.size <= comparison.LIMIT, "Packed comparison member exceeds 128 MiB")
                seen.add(name)
                total += member.size
                require(len(seen) <= MAX_FILES and total <= MAX_PRIVATE,
                        "Packed comparison export exceeds resource bounds")
                disk_space(destination, member.size)
                with archive.extractfile(member) as incoming, (destination / name).open("xb") as outgoing:
                    shutil.copyfileobj(incoming, outgoing, 1024 * 1024)
                end_offset = member.offset_data + ((member.size + 511) // 512) * 512
        # Include bytes already read ahead by tarfile, then validate gzip through EOF.
        start = observed.position - len(observed.tail)
        require(start <= end_offset <= observed.position, "Tar read-ahead exceeded padding validation bound")
        padding = observed.tail[end_offset - start:]
        require(not padding.strip(b"\0"), "Unexpected data after packed comparison archive")
        trailing = len(padding)
        for chunk in iter(lambda: compressed.read(1024 * 1024), b""):
            trailing += len(chunk)
            require(trailing <= 11264 and not chunk.strip(b"\0"), "Unexpected data after packed comparison archive")
        require(1024 <= trailing <= 11264, "Invalid packed comparison tar terminator")
    private_inventory(destination, documents=True)
    _, documents = export_documents(destination)
    for name, digest in documents.items():
        require(comparison.file_hash(destination / name) == digest, "Unpacked comparison document digest differs")


def validate_export(directory, game, backend, mode, source):
    receipt = comparison.load(directory / "receipt.json")
    require(receipt.get("schema") == comparison.SCHEMA and receipt.get("game") == game
            and receipt.get("backend") == backend and receipt.get("mode") == mode
            and receipt.get("engine_sha") == source["source_sha"], "Decrypted comparison cell/source identity differs")
    export_documents(directory)
    return directory / "receipt.json"


def private_target(value):
    runner = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    path = Path(value).absolute()
    require(path == path.resolve() and path.is_relative_to(runner) and path != runner
            and not path.exists() and not path.is_symlink(), "Output must be a new private RUNNER_TEMP directory")
    return path


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "pack":
        return pack_main()
    parser = argparse.ArgumentParser(description=__doc__, epilog=
        "Use %(prog)s pack --source EXPORT --output-dir TRANSPORT to prepare producer exports.")
    parser.add_argument("--game", required=True, choices=("k1", "k2"))
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args()
    os.umask(0o077)
    try:
        expected, source = expected_identity(os.environ)
        work, output = private_target(args.work_dir), private_target(args.output_dir)
        require(work != output and not work.is_relative_to(output), "Overlapping comparison output roots")
        work.mkdir(mode=0o700, parents=True)
        api = API()
        repository, _ = validate_run(api, expected, source, os.environ)
        artifacts = selected_artifacts(api, repository, expected, source, args.game)
        client = ArtifactsClient(REPOSITORY, os.environ["GITHUB_TOKEN"])
        receipts, retained = [], []
        for backend, mode, artifact in artifacts:
            cell = work / f"{backend}-{mode}"
            cell.mkdir(mode=0o700)
            disk_space(cell, artifact["size_in_bytes"] * 2)
            archive = cell / "artifact.zip"
            client.download_zip(artifact["id"], archive, expect_digest=artifact["digest"])
            require(archive.stat().st_size == artifact["size_in_bytes"], "Comparison artifact ZIP size differs")
            unpack_envelope(archive, cell)
            archive.unlink()
            outer = comparison.load(cell / "fullrun-provenance.json")
            ciphertext_sha256 = comparison.file_hash(cell / "evidence.seal")
            validate_outer_provenance(outer, expected, ciphertext_sha256)
            decrypted = cell / "transport"
            disk_space(cell, (cell / "evidence.seal").stat().st_size * 2)
            decrypt_envelope(cell / "evidence.seal", decrypted, cell)
            inner = comparison.load(decrypted / "fullrun-provenance.json")
            validate_outer_provenance(outer, expected, ciphertext_sha256)
            require(inner == expected, "Inner comparison execution identity differs")
            require({path.name for path in decrypted.iterdir()} == {PACK_NAME, "fullrun-provenance.json"},
                    "Unexpected encrypted comparison transport members")
            (cell / "evidence.seal").unlink()
            exported = cell / "export"
            unpack_export(decrypted / PACK_NAME, exported)
            receipts.append(str(validate_export(exported, args.game, backend, mode, source)))
            (decrypted / PACK_NAME).unlink()
            retained.append({"backend": backend, "mode": mode, "artifact_id": artifact["id"],
                             "zip_digest": artifact["digest"], "zip_bytes": artifact["size_in_bytes"],
                             "ciphertext_sha256": ciphertext_sha256})
        comparison.write(work / "artifact-receipts.json", {"identity": expected, "artifacts": retained})
        with (work / "comparison-events.jsonl").open("x", encoding="utf-8") as events:
            with contextlib.redirect_stdout(events):
                result = comparison.compare(argparse.Namespace(receipt=receipts, output_dir=str(output)))
        with (work / "comparison-events.jsonl").open(encoding="utf-8") as events:
            for line in events:
                value = json.loads(line)
                if value.get("event") == "patcher_comparison_diff":
                    print(json.dumps({key: value[key] for key in
                                      ("event", "left", "right", "boundary", "counts", "differences")}),
                          flush=True)
        summary = comparison.load(output / "summary.json")
        print(json.dumps({key: summary[key] for key in
                          ("game", "cells", "pairs", "differences", "equal", "component_count")},
                         sort_keys=True), flush=True)
        return result
    except (ValueError, OSError, KeyError, TypeError, ArtifactError,
            zipfile.BadZipFile, tarfile.TarError, EOFError, subprocess.SubprocessError) as error:
        # SDK exceptions can contain signed storage locations. Keep public failure output fixed.
        print(json.dumps({"event": "patcher_comparison_artifacts_failed", "error_type": type(error).__name__}),
              file=sys.stderr)
        return 1


def pack_main():
    parser = argparse.ArgumentParser(description="Compress only an authenticated comparison export's JSON documents")
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args(sys.argv[2:])
    os.umask(0o077)
    try:
        output = private_target(args.output_dir)
        source = args.source.resolve(strict=True)
        require(not output.is_relative_to(source), "Transport must be outside its source export")
        counts = pack_export(source, output)
        print(json.dumps({"event": "patcher_comparison_packed", **counts}), flush=True)
        return 0
    except (ValueError, OSError, KeyError, TypeError, tarfile.TarError) as error:
        print(json.dumps({"event": "patcher_comparison_pack_failed", "error_type": type(error).__name__}),
              file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
