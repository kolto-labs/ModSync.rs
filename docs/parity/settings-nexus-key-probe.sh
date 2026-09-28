#!/bin/sh
# Durable copy of the G-e probe (formerly /tmp/msprobe/nk2.sh).
#
# Exercises `update-check` / `nxm-download` against a corrupt legacy
# KOTORModSync settings file, with and without an explicit API key /
# env var / --settings-dir current file.
#
# Cases B and D reach the network (Nexus's download-link resolver), so they
# run under `unshare -rn` (network namespace unshare, loopback only) to keep
# the probe from making live calls to api.nexusmods.com. That turns the
# outcome from "real HTTP 401 from Nexus" into a local DNS/connection
# failure, which is what the case is meant to demonstrate: rc=1 with no
# settings error, regardless of *why* the network call failed.
#
# Requires: `unshare` from util-linux, unprivileged CLONE_NEWUSER/CLONE_NEWNET
# (rootless unshare). If `unshare -rn` fails in your sandbox, try
# `host-spawn unshare -rn` instead (drop-in prefix on the two isolated
# commands below).
set -u

MODSYNC_CLI="${MODSYNC_CLI:-/tmp/msprobe/modsync-cli}"
FIXTURE_TOML="${FIXTURE_TOML:-$(cd "$(dirname "$0")/../.." && pwd)/tests/fixtures/csharp/csharp.toml}"

B=/tmp/msprobe/nk; rm -rf "$B"; mkdir -p "$B/x/KOTORModSync" "$B/d"
echo corrupt > "$B/x/KOTORModSync/settings.json"
cp "$FIXTURE_TOML" "$B/guide.toml"
export HOME="$B/home" XDG_CONFIG_HOME="$B/x"; mkdir -p "$HOME"
unset KOTOR_MODSYNC_NEXUS_API_KEY NEXUS_MODS_API_KEY
M="$MODSYNC_CLI"
echo "binary: $M sha256=$(sha256sum "$M" | cut -d' ' -f1)"

r() { echo "rc=$?  legacy_warnings=$(grep -c 'failed to read legacy settings' "$B/o")  parse_errors=$(grep -ci 'pars' "$B/o")"; cat "$B/o" | head -6; echo; }

echo "### A update-check --nexus-mods-api-key X"
"$M" update-check "$B/guide.toml" "$B/out.toml" --nexus-mods-api-key X > "$B/o" 2>&1; r

echo "### B nxm-download --nexus-mods-api-key X (network-isolated: unshare -rn)"
unshare -rn "$M" nxm-download 'nxm://kotor/mods/1/files/2?key=k&expires=9999999999&user_id=1' "$B/f.bin" --nexus-mods-api-key X > "$B/o" 2>&1; r

echo "### C update-check, no flag, NEXUS_MODS_API_KEY=E"
NEXUS_MODS_API_KEY=E "$M" update-check "$B/guide.toml" "$B/out2.toml" > "$B/o" 2>&1; r

echo "### D nxm-download, no flag, KOTOR_MODSYNC_NEXUS_API_KEY=E (network-isolated: unshare -rn)"
unshare -rn env KOTOR_MODSYNC_NEXUS_API_KEY=E "$M" nxm-download 'nxm://kotor/mods/1/files/2?key=k&expires=9999999999&user_id=1' "$B/f2.bin" > "$B/o" 2>&1; r

echo "### E update-check, no flag, no env"
"$M" update-check "$B/guide.toml" "$B/out3.toml" > "$B/o" 2>&1; r

echo "### F update-check, current file 'bad', no flag"
echo bad > "$B/d/settings.json"
"$M" update-check "$B/guide.toml" "$B/out4.toml" --settings-dir "$B/d" > "$B/o" 2>&1; r
echo "current file now: $(cat "$B/d/settings.json")"

echo "### G nxm-download, current file 'bad', no flag"
"$M" nxm-download 'nxm://kotor/mods/1/files/2?key=k&expires=9999999999&user_id=1' "$B/f3.bin" --settings-dir "$B/d" > "$B/o" 2>&1; r

echo "### H update-check, current file 'bad', with flag"
"$M" update-check "$B/guide.toml" "$B/out5.toml" --settings-dir "$B/d" --nexus-mods-api-key X > "$B/o" 2>&1; r

echo "legacy file after all runs: $(cat "$B/x/KOTORModSync/settings.json")"
