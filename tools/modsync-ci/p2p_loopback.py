#!/usr/bin/env python3
"""Production ModSync P2P proof between two isolated Linux devices.

The historical filename remains for workflow compatibility. Requires passwordless
sudo, iproute2 and bubblewrap; isolation failures are fatal. No external torrent
implementation authors, seeds or downloads data. The controller compares results
but is never mounted into either device. JSON receipts contain no fixture bytes.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import subprocess
import tempfile
import time
import uuid
import zipfile

DIRECT = ["--no-proxy", "--peer-proxy-list-url", "", "--tracker-list-url", ""]


def run(command, **kwargs):
    return subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True,
                          text=True, timeout=kwargs.pop("timeout", 30), **kwargs)


def checked(command):
    result = run(command)
    if result.returncode:
        raise RuntimeError(f"{command[:4]} failed: {result.stderr[-2000:]}")
    return result.stdout


def admin(*args):
    return checked(["sudo", "-n", *map(str, args)])


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class Devices:
    def __init__(self, binary, root):
        self.binary, self.root = binary, root
        self.names = []
        self.processes = []
        self.prefix = "modsync-p2p-" + uuid.uuid4().hex[:10]
        # Expose individual ELF dependencies, never host /usr, /home or /tmp.
        self.mounts = []
        libs = set()
        for source, target in [(binary, "/modsync"),
                               (Path("/bin/sh").resolve(), "/probe-shell"),
                               (Path("/usr/bin/readlink").resolve(), "/probe-readlink"),
                               (Path("/usr/bin/cat").resolve(), "/probe-cat"),
                               (Path(shutil.which("setpriv")).resolve(), "/drop-privileges")]:
            self.mounts += ["--ro-bind", str(source), target]
            listing = checked(["ldd", str(source)])
            libs.update(re.findall(r"(?:=>\s+|^\s*)(/\S+)", listing, re.M))
        certificates = next((Path(p) for p in (
            "/etc/ssl/certs/ca-certificates.crt", "/etc/pki/tls/certs/ca-bundle.crt")
            if Path(p).is_file()), None)
        if certificates is None:
            raise RuntimeError("Production HTTP client requires a system CA bundle")
        self.mounts += ["--ro-bind", str(certificates.resolve()), "/ca-certificates.crt"]
        if Path("/etc/ld.so.cache").is_file():
            self.mounts += ["--ro-bind", "/etc/ld.so.cache", "/etc/ld.so.cache"]
        self.library_path = ":".join(sorted({str(Path(lib).parent) for lib in libs}))
        for lib in sorted(libs):
            self.mounts += ["--ro-bind", lib, lib]
        parents = {str(parent) for target in self.mounts[2::3]
                   for parent in Path(target).parents if str(parent) != "/"}
        directories = []
        for parent in sorted(parents, key=lambda p: (p.count("/"), p)):
            directories += ["--perms", "0755", "--dir", parent]
        self.mounts = directories + self.mounts

    def setup(self):
        for number in (1, 2):
            name = f"{self.prefix}-{number}"
            admin("ip", "netns", "add", name)
            self.names.append(name)
        left, right = "mp" + uuid.uuid4().hex[:8], "mp" + uuid.uuid4().hex[:8]
        try:
            admin("ip", "link", "add", left, "type", "veth", "peer", "name", right)
            for number, interface in enumerate((left, right), 1):
                name = self.names[number - 1]
                admin("ip", "link", "set", interface, "netns", name)
                admin("ip", "-n", name, "link", "set", interface, "name", "peer0")
                admin("ip", "-n", name, "addr", "add", f"192.0.2.{number}/30", "dev", "peer0")
                admin("ip", "-n", name, "link", "set", "peer0", "up")
                admin("ip", "-n", name, "link", "set", "lo", "up")
                routes = json.loads(admin("ip", "-n", name, "-j", "route", "show", "table", "all"))
                if any(r.get("dst") == "default" for r in routes):
                    raise RuntimeError("Device unexpectedly has an Internet route")
        finally:
            # Only needed if link creation succeeded but assignment failed.
            run(["sudo", "-n", "ip", "link", "del", left])

    def workspace(self, label):
        path = self.root / label
        path.mkdir()
        for name in ("home", "config", "cache", "data", "runtime", "files"):
            (path / name).mkdir(mode=0o700)
        return path

    def command(self, device, workspace, args):
        return ["sudo", "-n", "ip", "netns", "exec", self.names[device - 1],
                "bwrap", "--die-with-parent", "--unshare-pid",
                "--unshare-ipc", "--unshare-uts", "--hostname", f"emulateddevice{device}",
                "--new-session", "--clearenv", *self.mounts,
                "--bind", str(workspace), "/device", "--proc", "/proc", "--dev", "/dev",
                "--perms", "1777", "--tmpfs", "/tmp", "--chdir", "/device",
                "--setenv", "LD_LIBRARY_PATH", self.library_path,
                "--setenv", "SSL_CERT_FILE", "/ca-certificates.crt",
                "--setenv", "HOME", "/device/home",
                "--setenv", "XDG_CONFIG_HOME", "/device/config",
                "--setenv", "XDG_CACHE_HOME", "/device/cache",
                "--setenv", "XDG_DATA_HOME", "/device/data",
                "--setenv", "XDG_RUNTIME_DIR", "/device/runtime", "--",
                "/drop-privileges", "--reuid", str(os.getuid()), "--regid", str(os.getgid()),
                "--clear-groups", "--bounding-set=-all", "--inh-caps=-all",
                "--ambient-caps=-all", "--no-new-privs", *args]

    def probe(self, device, workspace, forbidden):
        script = ('for p do [ ! -e "$p" ] || exit 41; done; '
                  '/probe-readlink /proc/self/ns/net /proc/self/ns/mnt /proc/self/ns/pid; '
                  '/probe-cat /proc/self/status')
        result = checked(self.command(device, workspace,
                         ["/probe-shell", "-c", script, "probe", *map(str, forbidden)]))
        for capability in ("CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"):
            if not re.search(rf"{capability}:\s+0+\s", result):
                raise RuntimeError(f"Device retained {capability}")
        for field, identifier in (("Uid", os.getuid()), ("Gid", os.getgid())):
            if identifier == 0 or not re.search(rf"{field}:\s+{identifier}\s+{identifier}\s+{identifier}\s+{identifier}\s", result):
                raise RuntimeError(f"Device did not drop {field}")
        if not re.search(r"NoNewPrivs:\s+1\s", result):
            raise RuntimeError("Device lacks no-new-privileges protection")
        return {"device": f"emulateddevice{device}", "namespace": self.names[device - 1],
                "network_namespace": result.splitlines()[0],
                "mount_namespace": result.splitlines()[1], "pid_namespace": result.splitlines()[2],
                "source_inaccessible": True, "all_capability_sets": "0", "no_new_privs": True, "uid": os.getuid(), "gid": os.getgid(),
                "routes": json.loads(admin("ip", "-n", self.names[device - 1], "-j", "route")),
                "private_workspace": workspace.name}

    def start(self, device, workspace, archive, magnet=None, torrent_info=None):
        args = ["/modsync", "torrent-seed", archive, *DIRECT]
        if magnet:
            args += ["--authored-magnet", magnet, "--share-ratio", "0", "--share-hours", "0",
                     "--upload-kibps", "1048576"]
        if torrent_info:
            args += ["--torrent-info", torrent_info]
        log = tempfile.TemporaryFile(mode="w+")
        process = subprocess.Popen(self.command(device, workspace, args),
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=log, text=True, start_new_session=True)
        self.processes.append((process, log))
        ready, _, _ = select.select([process.stdout], [], [], 60)
        line = process.stdout.readline() if ready else ""
        if not line:
            log.seek(0)
            raise RuntimeError("Seeder readiness missing: " + log.read()[-2000:])
        info = json.loads(line)
        peer = "192.0.2." + str(device) + ":" + info["peer"].rsplit(":", 1)[1]
        return process, info, peer

    def seed_namespaces(self, device):
        for pid in admin("ip", "netns", "pids", self.names[device - 1]).split():
            command = admin("cat", f"/proc/{pid}/cmdline").split("\0")
            if command[:2] == ["/modsync", "torrent-seed"]:
                return {name + "_namespace": admin("readlink", f"/proc/{pid}/ns/{kind}").strip()
                        for name, kind in (("network", "net"), ("mount", "mnt"), ("pid", "pid"))}
        raise RuntimeError("No production seed found in its owned namespace")

    def stop(self, device, process, authored=True):
        # sudo/bwrap are wrappers. Signal the actual production seed in its own
        # network namespace, so it can emit the cancelled outcome/upload count.
        pids = admin("ip", "netns", "pids", self.names[device - 1]).split()
        for pid in pids:
            try:
                descriptor = os.pidfd_open(int(pid))
            except ProcessLookupError:
                continue
            try:
                command = admin("cat", f"/proc/{pid}/cmdline").split("\0")
                if command[:2] == ["/modsync", "torrent-seed"]:
                    # A pidfd cannot accidentally signal a recycled host PID.
                    signal.pidfd_send_signal(descriptor, signal.SIGINT)
            finally:
                os.close(descriptor)
        output, _ = process.communicate(timeout=30)
        lines = [json.loads(line) for line in output.splitlines() if line.strip()]
        outcome = lines[-1] if lines else {}
        if not authored and process.returncode == 0:
            return 0
        if process.returncode or outcome.get("outcome") != "stopped" or outcome.get("reason") != "cancelled":
            raise RuntimeError(f"Seed failed to stop cleanly: {process.returncode}, {outcome}")
        return int(outcome["uploadedBytes"])

    def download(self, device, workspace, magnet, expected, peer, torrent_info):
        args = ["/modsync", "torrent-download", magnet, "/device/files/fixture.zip",
                "--sha256", expected, "--peer", peer, *DIRECT]
        if torrent_info:
            args += ["--torrent-info", torrent_info]
        log = tempfile.TemporaryFile(mode="w+")
        process = subprocess.Popen(self.command(device, workspace, args),
                                   stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                   start_new_session=True)
        self.processes.append((process, log))
        process.wait(timeout=180)
        log.seek(0)
        # Bound retained diagnostics; fixture bytes never enter the receipt.
        log.seek(max(0, log.seek(0, 2) - 4000))
        return process.returncode, log.read()

    def close(self):
        for name in self.names:
            pids = admin("ip", "netns", "pids", name).split()
            if pids:
                # Cleanup includes privileged bwrap supervisors, strictly from
                # this run's randomly named network namespace.
                run(["sudo", "-n", "kill", "-KILL", "--", *pids])
            deadline = time.monotonic() + 5
            while admin("ip", "netns", "pids", name).split():
                if time.monotonic() >= deadline:
                    raise RuntimeError("Owned namespace still contains live processes")
                time.sleep(0.05)
            admin("ip", "netns", "del", name)
        remaining = admin("ip", "netns", "list")
        if any(name in remaining for name in self.names):
            raise RuntimeError("Owned network namespace cleanup failed")
        for process, log in self.processes:
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
            log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--size-mb", type=int, default=3)
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args()
    if not 1 <= args.size_mb <= 64:
        parser.error("--size-mb must be between 1 and 64")
    binary = args.binary.resolve(strict=True)
    receipt = {"schema": 1, "proof": "isolated-modsync-two-device-p2p", "status": "failed",
               "binary_sha256": digest(binary), "devices": [], "trials": []}
    sandbox = None
    previous = {}
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"signal {signum}")
    for sig in (signal.SIGINT, signal.SIGTERM):
        previous[sig] = signal.signal(sig, interrupted)
    try:
        for tool in ("sudo", "ip", "bwrap", "setpriv", "ldd"):
            if not shutil.which(tool):
                raise RuntimeError(f"Required isolation tool missing: {tool}")
        admin("true")
        with tempfile.TemporaryDirectory(prefix="modsync-two-devices-") as temporary:
            root = Path(temporary)
            sandbox = Devices(binary, root)
            try:
                sandbox.setup()
                sender = sandbox.workspace("device1-source")
                receiver = sandbox.workspace("device2-received")
                archive = sender / "files" / "fixture.zip"
                with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_STORED) as bundle:
                    bundle.writestr("payload/random.bin", os.urandom(args.size_mb * 1024 * 1024))
                expected, size = digest(archive), archive.stat().st_size
                receipt.update(payload_sha256=expected, payload_bytes=size)
                receipt["devices"] += [sandbox.probe(1, sender, [receiver, binary]),
                                       sandbox.probe(2, receiver, [sender, archive, binary])]
                if receipt["devices"][0]["network_namespace"] == receipt["devices"][1]["network_namespace"]:
                    raise RuntimeError("Devices share a network namespace")
                minted, info, _ = sandbox.start(1, sender, "/device/files/fixture.zip")
                sandbox.stop(1, minted, authored=False)
                magnet, torrent_info = info.get("magnet", ""), info.get("torrentInfo")
                if not magnet.startswith("magnet:?") or info.get("sha256") != expected:
                    raise RuntimeError("Production authoring returned an invalid identity")

                def trial(label, source_device, source, destination_device, destination, wrong=False):
                    seed, ready, peer = sandbox.start(source_device, source, "/device/files/fixture.zip", magnet, torrent_info)
                    if ready.get("sha256") != expected or ready.get("magnet") != magnet:
                        raise RuntimeError("Reseeder content identity changed")
                    isolation = sandbox.probe(destination_device, destination, [source, source / "files" / "fixture.zip", root / "controller-original.zip"])
                    live_sender = sandbox.seed_namespaces(source_device)
                    if any(live_sender[key] == isolation[key] for key in
                           ("network_namespace", "mount_namespace", "pid_namespace")):
                        raise RuntimeError("Sender and receiver share an isolation namespace")
                    started = time.monotonic()
                    code, diagnostic = sandbox.download(destination_device, destination, magnet,
                            "0" * 64 if wrong else expected, peer, torrent_info)
                    uploaded = sandbox.stop(source_device, seed)
                    output = destination / "files" / "fixture.zip"
                    result = {"trial": label, "sender": f"emulateddevice{source_device}",
                              "receiver": f"emulateddevice{destination_device}", "peer": peer,
                              "receiver_isolation": isolation, "live_sender_namespaces": live_sender, "exit_code": code,
                              "uploaded_bytes": uploaded, "elapsed_seconds": round(time.monotonic() - started, 3),
                              "output_published": output.exists()}
                    receipt["trials"].append(result)
                    if uploaded < size:
                        raise RuntimeError(f"{label}: insufficient real peer upload: {uploaded}/{size}; {diagnostic}")
                    if wrong:
                        if not code or output.exists() or "SHA-256 mismatch" not in diagnostic:
                            raise RuntimeError(f"{label}: wrong hash was not rejected after transfer: {diagnostic}")
                        result["sha256_mismatch_after_transfer"] = True
                    else:
                        if code or not output.exists() or digest(output) != expected or output.read_bytes() != (source / "files" / "fixture.zip").read_bytes():
                            raise RuntimeError(f"{label}: received bytes differ or transfer failed: {diagnostic}")
                        result.update(exact_bytes=True, sha256=digest(output))
                    result["status"] = "passed"
                    print(json.dumps(result, sort_keys=True), flush=True)

                trial("device1-to-device2", 1, sender, 2, receiver)
                refused = sandbox.workspace("device2-wrong-sha-fresh")
                trial("wrong-sha-fresh-device2", 1, sender, 2, refused, wrong=True)
                # No seed remains. Move the original outside both mount views;
                # device2 must now serve its downloaded bytes to fresh device1.
                archive.rename(root / "controller-original.zip")
                returned = sandbox.workspace("device1-return-fresh")
                trial("device2-reseeds-to-fresh-device1", 2, receiver, 1, returned)
                receipt["status"] = "passed"
            finally:
                # Finish bounded cleanup even if a second cancellation arrives.
                signal.signal(signal.SIGINT, signal.SIG_IGN)
                signal.signal(signal.SIGTERM, signal.SIG_IGN)
                sandbox.close()
                receipt["cleanup"] = "completed"
                sandbox = None
    except (Exception, KeyboardInterrupt) as error:
        receipt["error"] = str(error)[-4000:]
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        if args.receipt:
            args.receipt.parent.mkdir(parents=True, exist_ok=True)
            args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
        print(json.dumps(receipt, sort_keys=True), flush=True)
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
