#!/usr/bin/env python3
"""Exact-head production binary producer/consumer. Exit 3 means unavailable only.

No Actions cache is an authority. fetch needs the built-in read-only GITHUB_TOKEN;
produce runs cargo with a clean environment; only seal/fetch receive the existing game-cache key.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from datetime import datetime, timezone

REPOSITORY = "kolto-labs/ModSync.rs"
REFS = {"refs/heads/fix/fullrun-ci-credentials", "refs/heads/feat/ci-fullruns-testing", "refs/heads/main"}
WORKFLOW = ".github/workflows/production-build.yml"
TOOLCHAIN = "1.95.0"
MAX_BINARY = 256 * 1024 * 1024
MAX_ZIP = MAX_BINARY + 16 * 1024 * 1024
MAX_JSON = 12 * 1024 * 1024
ROOT = Path(__file__).resolve().parents[2]


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def stable(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def regular(path, limit):
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
        metadata = os.fstat(stream.fileno())
        require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1
                and metadata.st_size <= limit, "Unsafe or oversized input file")
        body = stream.read(limit + 1)
        require(len(body) == metadata.st_size, "Input changed while reading")
        return body


def identity():
    env = os.environ
    require(env.get("GITHUB_REPOSITORY") == REPOSITORY and env.get("GITHUB_REF") in REFS,
            "Only the exact owned repository and branch are admitted")
    head = env.get("GITHUB_SHA", "")
    require(re.fullmatch(r"[0-9a-f]{40}", head), "Invalid exact head")
    return {"repository": REPOSITORY, "ref": env["GITHUB_REF"], "head_sha": head}


def recipe(mode, root=ROOT):
    root = Path(root)
    require(mode in ("cli", "gui"), "Unknown production build mode")
    # Includes the transitive checked-in source/resource roots used by this app,
    # including embedded guide plans, patcher provenance and GUI font resources.
    # Cargo.lock fixes registry/git dependencies; local patched crates are below vendor.
    files = set()
    for name in ("src", "vendor", "tools", "packaging", "docs", "assets", "resources", ".cargo"):
        base = root / name
        if not base.exists():
            continue
        require(not base.is_symlink(), "Source root cannot be a symlink")
        for parent, dirs, names in os.walk(base, followlinks=False):
            dirs[:] = sorted(d for d in dirs if d not in ("__pycache__", "target", ".git"))
            require(all(not (Path(parent) / d).is_symlink() for d in dirs), "Source directory link")
            for filename in names:
                if not filename.endswith((".pyc", ".pyo")):
                    files.add((Path(parent) / filename).relative_to(root).as_posix())
    for name in ("Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain", "rust-toolchain.toml", WORKFLOW):
        if (root / name).exists():
            files.add(name)
    require("Cargo.lock" in files and WORKFLOW in files and len(files) <= 30000,
            "Incomplete or oversized source inventory")
    inventory = []
    for name in sorted(files):
        body = regular(root / name, 128 * 1024 * 1024)
        inventory.append({"path": name, "bytes": len(body), "sha256": digest(body)})
    flags = ["build", "--locked", "--release", "--lib", "--bin", "modsync", "-j2"]
    if mode == "cli":
        flags.append("--no-default-features")
    return {"schema": 1, "mode": mode, "runner": "ubuntu-24.04", "architecture": "x86_64",
            "toolchain": TOOLCHAIN, "cargo_arguments": flags,
            "build_environment": {"CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS": "true",
                                  "CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS": "true"},
            "cargo_lock_sha256": digest(regular(root / "Cargo.lock", MAX_JSON)),
            "workflow_sha256": digest(regular(root / WORKFLOW, MAX_JSON)),
            "source_inventory": inventory, "source_inventory_sha256": digest(stable(inventory))}


def elf(body):
    require(0 < len(body) <= MAX_BINARY and body[:7] == b"\x7fELF\x02\x01\x01"
            and int.from_bytes(body[18:20], "little") == 62
            and int.from_bytes(body[16:18], "little") in (2, 3), "Expected x86_64 ELF application")


def artifact_name(mode, head, run, attempt):
    return f"production-{mode}-{head}-{run}-{attempt}"


def produce(args):
    ident = identity()
    require(not os.environ.get("GITHUB_TOKEN") and not os.environ.get("CACHE_ENCRYPTION_KEY"),
            "Producer must not receive acquisition credentials")
    require(os.environ["GITHUB_WORKFLOW_SHA"] == ident["head_sha"]
            and os.environ["GITHUB_WORKFLOW_REF"] == REPOSITORY + "/" + WORKFLOW + "@" + ident["ref"],
            "Producer workflow identity differs")
    before = recipe(args.mode)
    clean = {"HOME": os.environ["HOME"], "PATH": os.environ["PATH"], "RUSTUP_TOOLCHAIN": TOOLCHAIN,
             **before["build_environment"]}
    compiler = subprocess.check_output(["rustc", "--version", "--verbose"], env=clean, text=True)
    cargo = subprocess.check_output(["cargo", "--version"], env=clean, text=True).strip()
    require(compiler.startswith("rustc 1.95.0 ") and "host: x86_64-unknown-linux-gnu" in compiler
            and cargo.startswith("cargo 1.95.0 "), "Unexpected production toolchain")
    subprocess.run(["cargo", *before["cargo_arguments"]], cwd=ROOT, env=clean, check=True)
    require(recipe(args.mode) == before, "Sources changed during build")
    # Cargo hardlinks its final executable to the hashed deps output. Accept
    # that build-owned inode only here, then publish independent bundle bytes.
    with os.fdopen(os.open(ROOT / "target/release/modsync",
                           os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
        before_binary = os.fstat(stream.fileno())
        require(stat.S_ISREG(before_binary.st_mode)
                and 0 < before_binary.st_size <= MAX_BINARY, "Unsafe Cargo output")
        body = stream.read(MAX_BINARY + 1)
        after_binary = os.fstat(stream.fileno())
        require(len(body) == before_binary.st_size
                and all(getattr(before_binary, field) == getattr(after_binary, field)
                        for field in ("st_dev", "st_ino", "st_mode", "st_nlink", "st_uid", "st_gid",
                                      "st_size", "st_mtime_ns", "st_ctime_ns")),
                "Cargo output changed while reading")
    elf(body)
    record = {"schema": 1, "mode": args.mode, **ident,
              **{key: os.environ[env] for key, env in (
                  ("repository_id", "GITHUB_REPOSITORY_ID"), ("repository_owner_id", "GITHUB_REPOSITORY_OWNER_ID"),
                  ("run_id", "GITHUB_RUN_ID"), ("run_attempt", "GITHUB_RUN_ATTEMPT"),
                  ("workflow_ref", "GITHUB_WORKFLOW_REF"), ("workflow_sha", "GITHUB_WORKFLOW_SHA"),
                  ("job", "GITHUB_JOB"), ("event", "GITHUB_EVENT_NAME"))},
              "recipe": before, "recipe_sha256": digest(stable(before)),
              "compiler": compiler, "cargo": cargo,
              "binary": {"filename": "modsync", "bytes": len(body), "sha256": digest(body)}}
    require(record["job"] == "production" and record["event"] in ("push", "workflow_dispatch"),
            "Unexpected producer context")
    out = Path(args.output)
    out.mkdir(parents=True, exist_ok=False)
    with (out / "modsync").open("xb") as stream:
        stream.write(body)
    (out / "build-provenance.json").write_bytes(stable(record))
    with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
        stream.write("artifact_name=" + artifact_name(args.mode, ident["head_sha"], record["run_id"], record["run_attempt"]) + "\n")


def seal_bundle(args):
    import seal
    ident = identity()
    require(not os.environ.get("GITHUB_TOKEN") and os.environ.get("CACHE_ENCRYPTION_KEY"),
            "Only the existing cache key may enter post-build sealing")
    source = Path(args.input)
    require(source.is_dir() and not source.is_symlink()
            and sorted(p.name for p in source.iterdir()) == ["build-provenance.json", "modsync"],
            "Only the fixed public application bundle may be sealed")
    body = regular(source / "modsync", MAX_BINARY)
    elf(body)
    manifest = regular(source / "build-provenance.json", MAX_JSON)
    record = json.loads(manifest)
    require(all(record[k] == v for k, v in ident.items())
            and record["recipe"] == recipe(args.mode)
            and record["recipe_sha256"] == digest(stable(recipe(args.mode)))
            and record["binary"] == {"filename": "modsync", "bytes": len(body), "sha256": digest(body)},
            "Post-build bundle changed")
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=False)
    seal.seal(source, output / "production.seal")
    cipher = regular(output / "production.seal", MAX_ZIP)
    outer = {key: record[key] for key in ("schema", "repository", "ref", "head_sha", "repository_id", "repository_owner_id",
             "run_id", "run_attempt", "workflow_ref", "workflow_sha", "job", "event", "recipe_sha256")}
    outer.update(mode=args.mode, ciphertext_sha256=digest(cipher), ciphertext_bytes=len(cipher),
                 inner_provenance_sha256=digest(manifest), encryption="modsync-seal-v1")
    (output / "build-provenance.json").write_bytes(stable(outer))


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args):
        return None


class WaitExpired(Exception):
    pass


class ProducerAbsent(Exception):
    pass


class API:
    def __init__(self):
        self.token = os.environ.get("GITHUB_TOKEN", "")
        require(self.token, "Scoped built-in token required for production build discovery")
        self.base = "https://api.github.com/repos/" + REPOSITORY
        self.deadline = None
        self.identities = {}

    def request(self, suffix):
        return urllib.request.Request(self.base + suffix, headers={
            "Authorization": "Bearer " + self.token, "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "modsync-production-build/1"})

    def get(self, suffix):
        if suffix in self.identities:
            return self.identities[suffix]
        remaining = self.deadline - time.monotonic() if self.deadline is not None else 30
        if remaining <= 0:
            raise WaitExpired()
        with urllib.request.build_opener(NoRedirect()).open(self.request(suffix), timeout=min(30, remaining)) as response:
            body = response.read(MAX_JSON + 1)
        require(len(body) <= MAX_JSON, "API response exceeds bound")
        value = json.loads(body)
        if suffix in ("", "/actions/workflows/production-build.yml"):
            self.identities[suffix] = value
        return value

    def download(self, aid):
        try:
            with urllib.request.build_opener(NoRedirect()).open(self.request(f"/actions/artifacts/{aid}/zip"), timeout=30):
                raise ValueError("Expected artifact storage redirect")
        except urllib.error.HTTPError as error:
            require(error.code == 302, "Artifact download API rejected request")
            url = error.headers["Location"]
        require(urllib.parse.urlsplit(url).scheme == "https", "Unsafe artifact redirect")
        # Never attach API authorization to storage or forward it across redirects.
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "modsync-production-build/1"}), timeout=60) as response:
            body = response.read(MAX_ZIP + 1)
        require(len(body) <= MAX_ZIP, "Artifact ZIP exceeds bound")
        return body


def fresh(artifact):
    if artifact.get("expired"):
        return False
    expires = datetime.fromisoformat(artifact["expires_at"].replace("Z", "+00:00"))
    return expires > datetime.now(timezone.utc)


def fetch_once(api, ident, expected, mode):
    repo = api.get("")
    require(repo["full_name"] == REPOSITORY and str(repo["id"]) == os.environ["GITHUB_REPOSITORY_ID"]
            and str(repo["owner"]["id"]) == os.environ["GITHUB_REPOSITORY_OWNER_ID"], "Repository API identity differs")
    try:
        workflow = api.get("/actions/workflows/production-build.yml")
    except urllib.error.HTTPError as error:
        if error.code == 404:
            raise ProducerAbsent() from None
        raise
    require(workflow["path"] == WORKFLOW, "Producer workflow path differs")
    query = urllib.parse.urlencode({"head_sha": ident["head_sha"], "branch": ident["ref"].removeprefix("refs/heads/"), "per_page": 100})
    runs = api.get(f"/actions/workflows/{workflow['id']}/runs?{query}")["workflow_runs"]
    require(len(runs) < 100, "Ambiguous producer run inventory exceeds bound")
    if not runs:
        raise ProducerAbsent()
    # A newer pending/failed attempt cannot be bypassed by an older successful
    # artifact. A matrix CLI job can finish while its GUI sibling is building.
    for run in sorted(runs, key=lambda row: row["id"], reverse=True)[:1]:
        require(run["workflow_id"] == workflow["id"] and run["path"] == WORKFLOW
                and run["head_sha"] == ident["head_sha"] and run["head_branch"] == ident["ref"].removeprefix("refs/heads/")
                and run["event"] in ("push", "workflow_dispatch") and run["repository"]["id"] == repo["id"]
                and run["head_repository"]["id"] == repo["id"], "Producer run identity differs")
        attempt = run["run_attempt"]
        current = api.get(f"/actions/runs/{run['id']}/attempts/{attempt}")
        require(all(current[key] == run[key] for key in ("id", "run_attempt", "head_sha", "workflow_id", "path", "event")), "Producer attempt differs")
        name = artifact_name(mode, ident["head_sha"], run["id"], attempt)
        jobs = api.get(f"/actions/runs/{run['id']}/attempts/{attempt}/jobs?per_page=100")
        require(jobs["total_count"] <= 100, "Producer job inventory exceeds bound")
        selected = [job for job in jobs["jobs"] if job["name"] == "production-" + mode]
        require(len(selected) <= 1, "Duplicate producer job")
        if not selected and current["status"] == "completed":
            require(current["conclusion"] == "success", "Known producer failed before mode job")
        if selected and selected[0]["status"] == "completed":
            require(selected[0]["conclusion"] == "success", "Known producer mode job failed")
        if not selected or selected[0]["status"] != "completed":
            continue
        require(selected[0]["conclusion"] == "success" and selected[0]["head_sha"] == ident["head_sha"]
                and selected[0]["run_id"] == run["id"], "Producer mode job failed or differs")
        artifacts = api.get(f"/actions/runs/{run['id']}/artifacts?per_page=100")
        require(artifacts["total_count"] <= 100, "Producer artifact inventory exceeds bound")
        matches = [a for a in artifacts["artifacts"] if a["name"] == name]
        require(len(matches) <= 1, "Duplicate production artifact")
        if not matches or not fresh(matches[0]):
            continue
        artifact = api.get(f"/actions/artifacts/{matches[0]['id']}")
        require(artifact == matches[0], "Artifact API records differ")
        require(artifact["workflow_run"]["id"] == run["id"]
                and artifact["workflow_run"]["head_sha"] == ident["head_sha"]
                and artifact["workflow_run"]["head_branch"] == ident["ref"].removeprefix("refs/heads/")
                and artifact["workflow_run"]["repository_id"] == repo["id"]
                and artifact["workflow_run"]["head_repository_id"] == repo["id"]
                and 0 < artifact["size_in_bytes"] <= MAX_ZIP
                and re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", "")), "Artifact provenance differs")
        body = api.download(artifact["id"])
        require(digest(body) == artifact["digest"][7:], "Artifact API ZIP digest mismatch")
        with zipfile.ZipFile(io.BytesIO(body)) as archive:
            entries = archive.infolist()
            require(sorted(i.filename for i in entries) == ["build-provenance.json", "production.seal"], "Unexpected artifact ZIP members")
            for item in entries:
                kind = stat.S_IFMT(item.external_attr >> 16)
                require(not item.is_dir() and kind in (0, stat.S_IFREG) and not item.flag_bits & 1
                        and item.file_size <= (MAX_ZIP if item.filename == "production.seal" else MAX_JSON), "Unsafe artifact ZIP member")
            cipher = archive.read("production.seal")
            outer = json.loads(archive.read("build-provenance.json"))
        outer_identity = {**ident, "schema": 1, "mode": mode,
                          "repository_id": str(repo["id"]), "repository_owner_id": str(repo["owner"]["id"]),
                          "run_id": str(run["id"]), "run_attempt": str(attempt), "event": run["event"],
                          "workflow_ref": REPOSITORY + "/" + WORKFLOW + "@" + ident["ref"],
                          "workflow_sha": ident["head_sha"], "job": "production",
                          "recipe_sha256": digest(stable(expected)), "encryption": "modsync-seal-v1"}
        require(all(outer.get(k) == v for k, v in outer_identity.items())
                and outer.get("ciphertext_sha256") == digest(cipher)
                and outer.get("ciphertext_bytes") == len(cipher), "Outer build identity or ciphertext differs")
        import seal
        with tempfile.TemporaryDirectory(prefix="production-verified-") as scratch:
            sealed = Path(scratch) / "production.seal"
            sealed.write_bytes(cipher)
            # Existing HMAC envelope is verified before decryption. The OpenSSL
            # child receives the derived password, never the API bearer token.
            token = os.environ.pop("GITHUB_TOKEN", None)
            try:
                seal.unseal(sealed, Path(scratch) / "inner")
            finally:
                if token is not None:
                    os.environ["GITHUB_TOKEN"] = token
            inner = Path(scratch) / "inner"
            require(sorted(p.name for p in inner.iterdir()) == ["build-provenance.json", "modsync"], "Unexpected decrypted bundle members")
            binary = regular(inner / "modsync", MAX_BINARY)
            record_body = regular(inner / "build-provenance.json", MAX_JSON)
            require(digest(record_body) == outer.get("inner_provenance_sha256"), "Inner build manifest digest differs")
            record = json.loads(record_body)
        wanted = {**ident, "schema": 1, "mode": mode, "repository_id": str(repo["id"]), "repository_owner_id": str(repo["owner"]["id"]),
                  "run_id": str(run["id"]), "run_attempt": str(attempt), "workflow_ref": REPOSITORY + "/" + WORKFLOW + "@" + ident["ref"],
                  "workflow_sha": ident["head_sha"], "job": "production", "event": run["event"], "recipe": expected,
                  "recipe_sha256": digest(stable(expected))}
        require(all(record.get(k) == v for k, v in wanted.items()), "Inner build identity/source recipe differs")
        require(set(record) == set(wanted) | {"compiler", "cargo", "binary"}, "Unexpected inner provenance fields")
        require(record["compiler"].startswith("rustc 1.95.0 ") and "host: x86_64-unknown-linux-gnu" in record["compiler"]
                and record["cargo"].startswith("cargo 1.95.0 "), "Inner toolchain differs")
        elf(binary)
        require(record["binary"] == {"filename": "modsync", "bytes": len(binary), "sha256": digest(binary)}, "Binary digest/size differs")
        return binary, {"producer_run_id": run["id"], "producer_run_attempt": attempt,
                        "artifact_id": artifact["id"], "artifact_zip_sha256": artifact["digest"][7:],
                        "ciphertext_sha256": digest(cipher), "build_provenance": record}
    return None


def fetch(args):
    ident = identity()
    source_root = Path(args.source_root).absolute()
    require(source_root.is_dir() and not source_root.is_symlink()
            and source_root.resolve() == source_root, "Unsafe source checkout root")
    require(regular(source_root / WORKFLOW, MAX_JSON) == regular(ROOT / WORKFLOW, MAX_JSON)
            and regular(source_root / "tools/modsync-ci/production_build.py", MAX_JSON)
            == regular(Path(__file__), MAX_JSON), "Source checkout producer/helper differs from trusted workflow checkout")
    expected = recipe(args.mode, root=source_root)
    api = API()
    require(0 <= args.wait_seconds <= 3600, "Wait bound must be between 0 and 3600 seconds")
    deadline = time.monotonic() + args.wait_seconds
    # Allow registration/scheduling to settle on a concurrent push. A manual
    # consumer with no producer must not spend the entire build budget waiting
    # before falling back to its own credential-free build.
    absence_deadline = min(deadline, time.monotonic() + 120)
    api.deadline = deadline if args.wait_seconds else None
    while True:
        next_deadline = deadline
        try:
            result = fetch_once(api, ident, expected, args.mode)
        except WaitExpired:
            result = None
        except ProducerAbsent:
            result = None
            next_deadline = absence_deadline
            if time.monotonic() >= absence_deadline:
                print("No exact-head production run scheduled; local build permitted")
                return 3
        if result:
            binary, verified = result
            require(recipe(args.mode, root=source_root) == expected, "Local sources changed while awaiting build")
            destination = Path(args.destination)
            if not destination.is_absolute():
                destination = source_root / destination
            require(destination == source_root / "target/release/modsync", "Only target/release/modsync may be published")
            parent = source_root
            for name in ("target", "release"):
                parent = parent / name
                if not parent.exists():
                    parent.mkdir(mode=0o700)
                require(parent.is_dir() and not parent.is_symlink(), "Unsafe binary destination directory")
            require(not destination.is_symlink(), "Binary destination is a symlink")
            with tempfile.NamedTemporaryFile(dir=parent, prefix=".production-", delete=False) as stream:
                temporary = Path(stream.name)
                try:
                    stream.write(binary)
                    stream.flush()
                    os.fsync(stream.fileno())
                    os.fchmod(stream.fileno(), 0o755)
                    os.replace(temporary, destination)
                finally:
                    temporary.unlink(missing_ok=True)
            if args.receipt:
                receipt = {"schema": 1, "status": "verified", **ident, "mode": args.mode,
                           "binary_path": str(destination), "binary_sha256": digest(binary),
                           "binary_bytes": len(binary), **verified}
                # The caller may retain this bounded metadata privately or pass
                # an already verified read-only descriptor; it is not a secret.
                # A bare JSON receipt is never standalone artifact authority.
                receipt_path = Path(args.receipt)
                with os.fdopen(os.open(receipt_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), "wb") as stream:
                    stream.write(stable(receipt))
            print(f"Verified production {args.mode} binary: run={verified['producer_run_id']} artifact={verified['artifact_id']} sha256={digest(binary)}")
            return 0
        if time.monotonic() >= deadline:
            print("Exact-head production build unavailable (absent, expired or not ready); local build permitted")
            return 3
        time.sleep(min(45, max(0, next_deadline - time.monotonic())))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    producer = commands.add_parser("produce")
    producer.add_argument("--mode", choices=("cli", "gui"), required=True)
    producer.add_argument("--output", required=True)
    sealing = commands.add_parser("seal")
    sealing.add_argument("--mode", choices=("cli", "gui"), required=True)
    sealing.add_argument("--input", required=True)
    sealing.add_argument("--output", required=True)
    consumer = commands.add_parser("fetch")
    consumer.add_argument("--mode", choices=("cli", "gui"), required=True)
    consumer.add_argument("--source-root", default=str(ROOT), help="Separate exact-head engine checkout, if used")
    consumer.add_argument("--destination", default="target/release/modsync")
    consumer.add_argument("--wait-seconds", type=int, default=3300)
    consumer.add_argument("--receipt", help="Optional new private metadata file for the acquiring caller")
    args = parser.parse_args()
    try:
        if args.command == "produce":
            produce(args)
            return 0
        if args.command == "seal":
            seal_bundle(args)
            return 0
        return fetch(args)
    except Exception as error:
        # Never print HTTP exceptions containing signed artifact storage URLs.
        print(f"Production build verification failed ({type(error).__name__})", file=sys.stderr)
        if isinstance(error, ValueError):
            print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
