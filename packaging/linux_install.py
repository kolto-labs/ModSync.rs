#!/usr/bin/env python3
"""Per-user Linux package launcher/MIME install and compare-before-remove uninstall."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent


def require(value, message):
    if not value:
        raise ValueError(message)


def sha(path):
    require(path.is_file() and not path.is_symlink(), f"Unsafe file: {path}")
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def tool(*arguments):
    return subprocess.check_output(arguments, text=True, timeout=30).strip()


def quoted(value):
    text = str(value)
    require(not any(ord(c) < 32 or ord(c) == 127 for c in text), "Control characters in launcher path")
    escaped = text.replace("\\", "\\\\").replace('"', '\\"').replace("`", "\\`").replace("$", "\\$").replace("%", "%%")
    return '"' + escaped.replace("\\", "\\\\") + '"'


def directory(path, private=False):
    require(path.is_absolute(), "XDG roots must be absolute")
    cursor = Path(path.anchor)
    for part in path.parts[1:]:
        cursor = cursor / part
        require(not cursor.is_symlink(), "Symlink in installation directory")
        if cursor.exists():
            require(cursor.is_dir(), "Installation ancestor is not a directory")
    path.mkdir(parents=True, exist_ok=True, mode=0o700 if private else 0o755)
    require(path.stat().st_uid == os.geteuid(), "Installation directory belongs to another user")
    if private:
        require(path.stat().st_mode & 0o077 == 0, "Installation state must be private")


def protocol(action, token, replace=False):
    command = [str(ROOT / "modsync"), "protocol", action, "--executable", str(ROOT / "modsync"), "--package-token", token, "--channel", "stable", "--scheme", "both"]
    if replace:
        command.append("--replace")
    return tool(*command)


def write_new(path, data, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["install", "uninstall"])
    parser.add_argument("--register", action="store_true", help="Register both MIME schemes through the shared protocol service")
    parser.add_argument("--replace", action="store_true", help="Explicitly authorize replacing an existing handler")
    args = parser.parse_args()
    require(os.geteuid() != 0, "Run per-user installation without sudo/root")
    require(not any(ord(c) < 32 or ord(c) == 127 for c in str(ROOT)), "Control characters in package directory")
    manifest_path = ROOT / "modsync-package.json"
    require(manifest_path.stat().st_size <= 4 * 1024 * 1024, "Manifest too large")
    manifest = json.loads(manifest_path.read_text())
    require(manifest["schemaVersion"] == 1 and "linux" in manifest["target"], "Not a Linux package")
    package_hash = sha(manifest_path)
    token = manifest["registrationToken"]
    require(token and all(c.isascii() and (c.isalnum() or c in ".-_") for c in token), "Invalid package registration token")
    desktop_id = f"modsync-rust-stable-{package_hash[:16]}.desktop"
    data = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "modsync-packages"
    directory(data)
    directory(state, private=True)
    directory(data / "applications")
    desktop = data / "applications" / desktop_id
    record_path = state / (package_hash + ".json")
    entry = "[Desktop Entry]\nType=Application\nName=ModSync (pre-release)\nComment=ModSync guide installer\nExec=" + quoted(sys.executable) + " " + quoted(ROOT / "launch.py") + " %u\nIcon=" + str(ROOT / "Resources/modsync.svg") + "\nTerminal=false\nCategories=Game;Utility;" + "\n"
    if args.action == "install":
        require(not record_path.exists() and not record_path.is_symlink(), "Package installation record already exists")
        require(not desktop.exists() and not desktop.is_symlink(), "Desktop entry already exists")
        for item in manifest["files"]:
            relative = Path(item["path"])
            require(not relative.is_absolute() and ".." not in relative.parts, "Unsafe manifest path")
            path = ROOT / relative
            cursor = ROOT
            for part in relative.parts:
                cursor = cursor / part
                require(not cursor.is_symlink(), "Package contains a symlink")
            require(sha(path) == item["sha256"] and path.stat().st_size == item["size"], "Package file differs: " + item["path"])
        require(shutil.which("update-desktop-database"), "update-desktop-database is required")
        # Record intended ownership before touching launch/association state so a
        # failed registration can be reversed by the same explicit uninstall.
        record = {"schemaVersion": 1, "packageRoot": str(ROOT), "manifestSha256": package_hash, "registrationToken": token, "desktop": str(desktop), "desktopSha256": hashlib.sha256(entry.encode()).hexdigest(), "protocolRegistrationRequested": args.register}
        write_new(record_path, json.dumps(record, indent=2) + "\n", 0o600)
        write_new(desktop, entry)
        tool("update-desktop-database", str(desktop.parent))
        if args.register:
            protocol("register", token, args.replace)
        print(f"Installed launcher for {ROOT}. Keep this extracted directory in place. Record: {record_path}")
    else:
        require(not record_path.is_symlink(), "Unsafe installation record")
        record = json.loads(record_path.read_text())
        require(record["packageRoot"] == str(ROOT) and record["manifestSha256"] == package_hash and record["desktop"] == str(desktop) and record["registrationToken"] == token, "Installation ownership differs")
        if desktop.exists():
            require(sha(desktop) == record["desktopSha256"], "Desktop entry changed; refusing to remove another owner's launcher")
        if record["protocolRegistrationRequested"]:
            protocol("unregister", token)
        if desktop.exists():
            desktop.unlink()
        tool("update-desktop-database", str(desktop.parent))
        record_path.unlink()
        print("Removed this package's launcher. Package files remain for explicit removal.")


if __name__ == "__main__":
    main()
