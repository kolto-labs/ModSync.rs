"""Live, redacted subprocess output; original files and receipts stay private.

Regular-file followers avoid pipe deadlocks and preserve callers' process groups.
The explicit per-process raw budget is 4 GiB. A line over 64 MiB is withheld in
full and fails the command rather than exposing a potentially split credential.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import threading
import time
import gui_diag

MAX_LINE = 64 * 1024 * 1024
MAX_RAW = 4 * 1024 ** 3
_SECRET = re.compile(r"(?i)(token|secret|key|passw(?:or)?d|credential|authorization|signature|cookie)")
_URL = re.compile(r"https?://[^\s<>\"']+", re.I)
_ASSIGN = re.compile(r"(?i)((?:--)?[\w.-]*(?:token|secret|key|password|credential|signature|cookie)[\w.-]*\s*(?:=|:)\s*)[^\s,;]+")
_AUTH = re.compile(r"(?i)((?:proxy-)?authorization\s*[:=]\s*)[^\r\n]*")
_PRIVATE = re.compile(r"-----BEGIN (?:[A-Z0-9 ]* )?PRIVATE KEY-----")
_WRITE_LOCK = threading.Lock()
_NATIVE_RUN = subprocess.run


def public(text, env=None):
    """Sanitize a complete bounded logical line before splitting display chunks."""
    if _PRIVATE.search(text):
        return "[REDACTED private key material]"
    environment = dict(os.environ)
    environment.update(env or {})
    for name, value in environment.items():
        if value and _SECRET.search(name):
            text = text.replace(value, "[REDACTED]")
            for part in value.splitlines():
                if part:
                    text = text.replace(part, "[REDACTED]")
    # Capability URL query strings are private regardless of parameter spelling.
    text = _URL.sub(lambda m: m.group(0).split("?", 1)[0].split("#", 1)[0]
                    + ("?[REDACTED]" if "?" in m.group(0) or "#" in m.group(0) else ""), text)
    text = _AUTH.sub(r"\1[REDACTED]", text)
    text = _ASSIGN.sub(r"\1[REDACTED]", text)
    text = gui_diag.redact(text, env=environment)
    # Visible escaping prevents CR/ANSI from restoring runner command syntax.
    return "".join(c if c == "\t" or ord(c) >= 32 and ord(c) != 127 else
                   "\\x%02x" % ord(c) for c in text)


def emit(text, env=None):
    safe = public(str(text), env)
    with _WRITE_LOCK:
        for offset in range(0, max(len(safe), 1), 16384):
            # Prefix EVERY physical line/chunk. Child workflow commands are data.
            print("live | " + safe[offset:offset + 16384], flush=True)


def command_text(command, env=None):
    args = [os.fsdecode(arg) for arg in command]
    safe = []
    hide = False
    for arg in args:
        if hide:
            safe.append("[REDACTED]")
            hide = False
        else:
            safe.append(public(arg, env))
            hide = arg.startswith("-") and bool(_SECRET.search(arg)) and "=" not in arg
    return shlex.join(safe)


class Popen:
    """Composition wrapper resolving subprocess.Popen at call time."""
    def __init__(self, command, **options):
        self._live_done = threading.Event()
        self._live_code = None
        self._live_env = options.get("env")
        self._live_started = time.monotonic()
        self._live_error = None
        self._live_files = []
        for key in ("stdout", "stderr"):
            stream = options.get(key)
            if hasattr(stream, "name") and isinstance(stream.name, (str, bytes, os.PathLike)):
                path = Path(stream.name)
                if path not in self._live_files:
                    self._live_files.append(path)
        if not self._live_files:
            raise ValueError("live subprocess requires private file output")
        self._live_receipt = self._live_files[0].with_name(self._live_files[0].name + ".command.json")
        self._live_data = {"argv": [os.fsdecode(arg) for arg in command],
                           "cwd": str(Path(options.get("cwd") or Path.cwd()).resolve()),
                           "status": "starting"}
        self._receipt()
        emit("cwd=" + self._live_data["cwd"] + " $ " + command_text(command, self._live_env), self._live_env)
        try:
            self._process = subprocess.Popen(command, **options)
        except BaseException as error:
            self._live_data.update(status="launch_failed", error_type=type(error).__name__)
            self._receipt()
            emit("launch failed: " + type(error).__name__)
            raise
        self._live_thread = threading.Thread(target=self._follow, daemon=True)
        self._live_thread.start()

    def __getattr__(self, name):
        return getattr(self._process, name)

    @property
    def returncode(self):
        return getattr(self._process, "returncode", self._live_code)

    def poll(self):
        poll = getattr(self._process, "poll", None)
        return poll() if poll is not None else self._live_code

    def communicate(self, *args, **kwargs):
        result = self._process.communicate(*args, **kwargs)
        self._live_code = self._process.returncode
        self._live_done.set()
        self._live_thread.join()
        return result

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.wait()

    def _receipt(self):
        descriptor = os.open(self._live_receipt, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as out:
            json.dump(self._live_data, out, ensure_ascii=True)
            out.write("\n")

    def _follow(self):
        pending = [bytearray() for _ in self._live_files]
        withheld = [False] * len(pending)
        private_key = [False] * len(pending)
        hashes = [hashlib.sha256() for _ in pending]
        sizes = [0] * len(pending)
        def line(index, raw):
            text = raw.decode("utf-8", errors="replace")
            if _PRIVATE.search(text):
                private_key[index] = True
            if private_key[index]:
                emit("[REDACTED private key material]")
                if "-----END " in text and "PRIVATE KEY-----" in text:
                    private_key[index] = False
            else:
                emit(text, self._live_env)
        try:
            streams = [path.open("rb") for path in self._live_files]
            try:
                while True:
                    active = not self._live_done.is_set() and self.poll() is None
                    received = False
                    for i, stream in enumerate(streams):
                        data = stream.read(65536)
                        if not data:
                            continue
                        received = True
                        hashes[i].update(data)
                        sizes[i] += len(data)
                        if sum(sizes) > MAX_RAW:
                            raise ValueError("raw output exceeds explicit 4 GiB process budget")
                        for part in data.splitlines(keepends=True):
                            ended = part.endswith(b"\n")
                            if not withheld[i]:
                                pending[i].extend(part)
                                if len(pending[i]) > MAX_LINE:
                                    pending[i].clear()
                                    withheld[i] = True
                                    self._live_error = "logical output line exceeds 64 MiB safe display bound"
                                    emit("OUTPUT WITHHELD: " + self._live_error + "; complete bytes remain private")
                            if ended:
                                if not withheld[i]:
                                    line(i, bytes(pending[i][:-1]))
                                pending[i].clear()
                                withheld[i] = False
                    if not active and not received:
                        break
                    if not received:
                        time.sleep(0.05)
                for i, raw in enumerate(pending):
                    if raw:
                        line(i, bytes(raw))
            finally:
                for stream in streams:
                    stream.close()
        except BaseException as error:
            self._live_error = "live output follower failed: " + type(error).__name__
            emit(self._live_error)
            # Terminate the existing group when the caller created one.
            try:
                if os.name == "posix" and os.getpgid(self.pid) == self.pid:
                    os.killpg(self.pid, signal.SIGKILL)
                else:
                    self.kill()
            except ProcessLookupError:
                pass
        self._live_data.update(status="finished", exit_code=self.poll(),
                               duration_seconds=round(time.monotonic() - self._live_started, 3),
                               output_error=self._live_error,
                               logs=[{"path": str(path), "bytes": size, "sha256": h.hexdigest()}
                                     for path, size, h in zip(self._live_files, sizes, hashes)])
        try:
            self._receipt()
        except OSError:
            self._live_error = "private command receipt could not be written"
        emit("exit=" + str(self.returncode) + " duration=" + str(self._live_data["duration_seconds"]) + "s")

    def wait(self, timeout=None):
        try:
            code = self._process.wait(timeout=timeout)
            self._live_code = code
            self._live_done.set()
        except subprocess.TimeoutExpired:
            self._live_data["timeout_seconds"] = timeout
            self._receipt()
            raise
        self._live_thread.join()
        # Preserve timeout cleanup (which also calls wait) and report output
        # policy errors as a failed result through check_output().
        return code

    def check_output(self):
        self._live_thread.join()
        if self._live_error:
            raise ValueError(self._live_error + "; inspect encrypted evidence")


def run(command, log, **options):
    """Run with merged live output and byte-exact private capture."""
    # Retain the caller's call-time subprocess.run adapter. Production uses the
    # Popen follower, while injected runners keep their established file handles,
    # timeout and check semantics without launching an additional child.
    runner = options.pop("runner", None) or subprocess.run
    if runner is not _NATIVE_RUN:
        with Path(log).open("wb") as stream:
            return runner(command, stdout=stream, stderr=subprocess.STDOUT, **options)
    timeout = options.pop("timeout", None)
    with Path(log).open("wb") as stream:
        process = Popen(command, stdout=stream, stderr=subprocess.STDOUT, **options)
        try:
            process.wait(timeout=timeout)
        except BaseException:
            process.kill()
            process.wait()
            raise
        process.check_output()
    return subprocess.CompletedProcess(command, process.returncode)


def private_tail(path, limit=16 * 1024 ** 2):
    """Bound analysis memory and suppress PEM even when its start predates the tail."""
    marker = re.compile(rb"-----BEGIN (?:[A-Z0-9 ]* )?PRIVATE KEY-----|"
                        rb"-----END (?:[A-Z0-9 ]* )?PRIVATE KEY-----")
    private_key = False
    with Path(path).open("rb") as source:
        source.seek(0, os.SEEK_END)
        start = max(0, source.tell() - limit)
        source.seek(0)
        remaining = start
        carry = b""
        while remaining:
            chunk = source.read(min(65536, remaining))
            if not chunk:
                break
            remaining -= len(chunk)
            combined = carry + chunk
            for match in marker.finditer(combined):
                private_key = match.group(0).startswith(b"-----BEGIN ")
            carry = combined[-256:]
        # Resolve a marker crossing the exact tail boundary before deciding
        # whether its first bytes are private key material.
        boundary = carry + source.read(256)
        for match in marker.finditer(boundary):
            if match.start() < len(carry):
                private_key = match.group(0).startswith(b"-----BEGIN ")
        source.seek(start)
        data = source.read(limit)
    text = data.decode("utf-8", errors="replace")
    if private_key:
        text = "-----BEGIN PRIVATE KEY-----\n" + text
    return re.sub(r"-----BEGIN (?:[A-Z0-9 ]* )?PRIVATE KEY-----.*?"
                  r"(?:-----END (?:[A-Z0-9 ]* )?PRIVATE KEY-----|\Z)",
                  "[REDACTED private key material]", text, flags=re.DOTALL)


def emit_lines(text, env=None, file=None):
    """Safely repeat a failure summary on its established diagnostic channel."""
    destination = sys.stderr if file is None else file
    private_key = False
    for line in str(text).splitlines():
        if _PRIVATE.search(line):
            private_key = True
        if private_key:
            safe = "[REDACTED private key material]"
            if "-----END " in line and "PRIVATE KEY-----" in line:
                private_key = False
        else:
            safe = public(line, env)
        for offset in range(0, max(len(safe), 1), 16384):
            print("live | " + safe[offset:offset + 16384], file=destination, flush=True)
