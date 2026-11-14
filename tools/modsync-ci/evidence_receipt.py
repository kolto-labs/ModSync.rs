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
from pathlib import Path


def identity():
    # Fixed public fields only: never serialize a caller's environment wholesale.
    fields = ("GITHUB_REPOSITORY", "GITHUB_SHA", "GITHUB_REF", "GITHUB_RUN_ID",
              "GITHUB_RUN_ATTEMPT", "GITHUB_WORKFLOW_REF", "GITHUB_WORKFLOW_SHA",
              "GITHUB_JOB")
    return {"schema_version": 1,
            **{field.removeprefix("GITHUB_").lower(): os.environ.get(field, "") for field in fields}}


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
