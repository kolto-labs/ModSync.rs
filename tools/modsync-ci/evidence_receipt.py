#!/usr/bin/env python3
"""Bind encrypted diagnostics to public GitHub execution metadata.

The receipt is not a signature. Consumers must obtain the artifact through the
expected GitHub run, check its API digest and compare its run/head identities.
Public-key encryption authenticates ciphertext integrity, not the sender.
"""
import argparse
import hashlib
import json
import os
import re
import stat
from pathlib import Path


def source_identity(environment):
    """Keep the tested source distinct from GitHub's merge/workflow execution."""
    repository = environment.get("GITHUB_REPOSITORY", "")
    source = {"source_repository": repository,
              "source_sha": environment.get("GITHUB_SHA", ""),
              "source_ref": environment.get("GITHUB_REF", ""),
              "event_name": environment.get("GITHUB_EVENT_NAME", "")}
    if source["event_name"] != "pull_request":
        return source
    # The event file is supplied by Actions, never by the retained evidence.
    # Only a same-repository PR may identify source in the licensed full graph.
    path = environment.get("GITHUB_EVENT_PATH", "")
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 8 * 1024 * 1024:
            raise ValueError("Unsafe or oversized GitHub event payload")
        body = stream.read(8 * 1024 * 1024 + 1)
        if len(body) != metadata.st_size:
            raise ValueError("GitHub event payload changed while reading")
    payload = json.loads(body)
    pull = payload.get("pull_request") or {}
    head, base = pull.get("head") or {}, pull.get("base") or {}
    number = payload.get("number")
    if (not repository or not isinstance(number, int) or isinstance(number, bool) or number <= 0
            or pull.get("number") != number
            or payload.get("repository", {}).get("full_name", "").casefold() != repository.casefold()
            or head.get("repo", {}).get("full_name", "").casefold() != repository.casefold()
            or base.get("repo", {}).get("full_name", "").casefold() != repository.casefold()
            or environment.get("GITHUB_REF") != "refs/pull/%d/merge" % number
            or not re.fullmatch(r"[0-9a-f]{40}", head.get("sha", ""))
            or not re.fullmatch(r"[0-9a-f]{40}", base.get("sha", ""))
            or not isinstance(head.get("ref"), str) or not head["ref"]):
        raise ValueError("PR source identity requires the exact same-repository event")
    source.update(source_sha=head["sha"], source_ref="refs/heads/" + head["ref"],
                  pull_request_number=number, pull_request_base_sha=base["sha"])
    return source


def identity():
    # Fixed public fields only: never serialize a caller's environment wholesale.
    # `sha` and `workflow_sha` retain their original Actions meaning. Readers of
    # v2 compare the API run head to source_sha; v1 records keep their old shape.
    fields = ("GITHUB_REPOSITORY", "GITHUB_SHA", "GITHUB_REF", "GITHUB_RUN_ID",
              "GITHUB_RUN_ATTEMPT", "GITHUB_WORKFLOW_REF", "GITHUB_WORKFLOW_SHA",
              "GITHUB_JOB")
    value = {"schema_version": 1,
             **{field.removeprefix("GITHUB_").lower(): os.environ.get(field, "") for field in fields}}
    if os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        value.update(schema_version=2, **source_identity(os.environ))
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("identity", "receipt"))
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    value = identity()
    if args.command == "receipt":
        digest = hashlib.sha256()
        with (args.directory / "evidence.seal").open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        value["ciphertext_sha256"] = digest.hexdigest()
        value["encryption"] = "age-x25519" if os.environ.get("DIAGNOSTICS_ENCRYPTION_RECIPIENT") else "legacy-v1"
    # Do not follow an evidence-supplied link or overwrite a pre-existing file.
    with (args.directory / "fullrun-provenance.json").open("x", encoding="utf-8") as stream:
        json.dump(value, stream, sort_keys=True, indent=2)
        stream.write("\n")


if __name__ == "__main__":
    main()
