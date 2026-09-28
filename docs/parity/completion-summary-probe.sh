#!/bin/sh
# Probe for `modsync completion-summary` (Task 2b). Run on the host (glibc mismatch
# in the dev container). Hand-writes completion.json/snapshot.json per the
# recovery.rs schema instead of running a full install, per the task's fallback
# instruction. Read-only against the binary: only touches /tmp/modsync-probe.
set -eu

BIN=/tmp/modsync-target-completion/debug/modsync
BASE=/tmp/modsync-probe
GAME="$BASE/gameA"
MODS_A="$BASE/modsA"
MODS_C="$BASE/modsC"

echo "== binary sha256 =="
sha256sum "$BIN"

rm -rf "$BASE"
mkdir -p "$GAME" "$MODS_A" "$MODS_C"
touch "$GAME/.keep" "$MODS_A/.keep" "$MODS_C/.keep"

GAME_ABS=$(cd "$GAME" && pwd)
MODS_A_ABS=$(cd "$MODS_A" && pwd)
MODS_C_ABS=$(cd "$MODS_C" && pwd)

STATE_KEY=$(python3 -c "import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest()[:16])" "$GAME_ABS")
STATE_DIR=$(dirname "$GAME_ABS")/".modsync-$STATE_KEY"
echo "state dir: $STATE_DIR"

echo
echo "== Scenario 1: no receipt =="
echo "-- plain --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS"; echo "exit=$?"
echo "-- json --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS" --json; echo "exit=$?"
echo "-- state dir must not exist --"
if [ -e "$STATE_DIR" ]; then echo "FAIL: state dir was created"; else echo "OK: absent"; fi

echo
echo "== Scenario 2: receipt present, no snapshot, no pending -> delta N/A/null =="
mkdir -p "$STATE_DIR"
COMPONENTS='[{"Guid":"11111111-1111-1111-1111-111111111111","Name":"Base Fix"},{"Guid":"22222222-2222-2222-2222-222222222222","Name":"Widescreen"}]'
cat > "$STATE_DIR/completion.json" <<EOF
{
  "mods": "$MODS_A_ABS",
  "game": "$GAME_ABS",
  "components": $COMPONENTS,
  "files": {"override/a.tpc": "hasha", "override/b.tpc": "hashb"}
}
EOF
echo "-- plain --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS"; echo "exit=$?"
echo "-- json --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS" --json; echo "exit=$?"

echo
echo "== Scenario 3: snapshot present, distinct from receipt -> delta computed =="
cat > "$STATE_DIR/snapshot.json" <<EOF
{
  "mods": "$MODS_A_ABS",
  "game": "$GAME_ABS",
  "files": {"game/override/a.tpc": "hasha", "game/override/c.tpc": "hashc"},
  "completion": null
}
EOF
# receipt has a.tpc (unchanged), b.tpc (added vs snapshot), lacks c.tpc (removed vs snapshot)
echo "-- plain --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS"; echo "exit=$?"
echo "-- json --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS" --json; echo "exit=$?"
echo "expected delta: added=1 (b.tpc), changed=0, removed=1 (c.tpc)"

echo
echo "== Scenario 4: pending marker present -> delta forced to N/A/null =="
touch "$STATE_DIR/pending"
echo "-- plain --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS"; echo "exit=$?"
echo "-- json --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS" --json; echo "exit=$?"
rm -f "$STATE_DIR/pending"

echo
echo "== Scenario 5: restore binding - snapshot.completion equals current receipt -> delta N/A/null =="
RECEIPT_VALUE=$(cat "$STATE_DIR/completion.json")
python3 - "$STATE_DIR/snapshot.json" "$STATE_DIR/completion.json" <<'PY'
import json, sys
snap_path, receipt_path = sys.argv[1], sys.argv[2]
with open(receipt_path) as f:
    receipt = json.load(f)
with open(snap_path) as f:
    snap = json.load(f)
snap["completion"] = receipt
with open(snap_path, "w") as f:
    json.dump(snap, f)
PY
echo "-- plain --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS"; echo "exit=$?"
echo "-- json --"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_A_ABS" --json; echo "exit=$?"

echo
echo "== Scenario 6: roots differ (receipt.mods=modsA, query mods=modsC) -> exit 1, stderr =="
set +e
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_C_ABS"
echo "exit=$?"
"$BIN" completion-summary --game-dir "$GAME_ABS" --source-dir "$MODS_C_ABS" --json
echo "exit=$?"
set -e

echo
echo "== cleanup =="
rm -rf "$BASE"
echo "done"
