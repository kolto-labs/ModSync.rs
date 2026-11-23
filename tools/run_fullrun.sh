#!/usr/bin/env bash
# Also runnable directly on an operator-controlled Linux machine.
set -euo pipefail
umask 077

# Do not echo configuration values or diagnostics to Actions. Keep the entire
# run, including failed lane copies, in a new directory on the evidence mount.
root="${MODSYNC_FULLRUN_ROOT:-}"
if [[ "$root" != /* || ! -d "$root" || -L "$root" || ! -O "$root" ]] ||
   [[ "$(stat -c '%a' -- "$root" 2>/dev/null)" != 700 ]]; then
  echo 'Configure a runner-owned, mode-0700 MODSYNC_FULLRUN_ROOT directory.' >&2
  exit 1
fi
root="$(realpath -- "$root" 2>/dev/null)" || {
  echo 'Could not resolve the local evidence directory.' >&2
  exit 1
}
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
case "$root/" in
  "$repo/"*) echo 'Evidence must be kept outside the checkout.' >&2; exit 1 ;;
esac
for name in MODSYNC_FULLRUN_GAME MODSYNC_FULLRUN_MODS; do
  if [[ -n "${!name:-}" ]]; then
    baseline="$(realpath -m -- "${!name}" 2>/dev/null)" || {
      echo 'Could not resolve a configured baseline directory.' >&2
      exit 1
    }
    case "$root/" in
      "$baseline/"*) echo 'Evidence must not be written inside a baseline.' >&2; exit 1 ;;
    esac
  fi
done
run="$(mktemp -d -- "$root/run.XXXXXXXX" 2>/dev/null)" || {
  echo 'Could not create the local evidence directory.' >&2
  exit 1
}
cd -- "$repo"
stage=configuration
trap 'printf "Full run failed at %s; details are on the runner.\n" "$stage" >&3' ERR
exec 3>&2
{
  for name in MODSYNC_FULLRUN_GUIDE MODSYNC_FULLRUN_GAME MODSYNC_FULLRUN_MODS MODSYNC_FULLRUN_DRIVER; do
    if [[ -z "${!name:-}" ]]; then
      printf 'Missing runner configuration: %s\n' "$name" >&2
      false
    fi
  done
  selection="${MODSYNC_FULLRUN_SELECTION:-}"
  if [[ -z "$selection" ]]; then selection='["--use-file-selection"]'; fi
  holo="${MODSYNC_FULLRUN_HOLOPATCHER:-}"

  stage=build
  # Resolve the binary deterministically, independently of the runner's cache settings.
  CARGO_TARGET_DIR="$run/target" cargo build --locked --no-default-features --lib --bin modsync -j2
  binary="$run/target/debug/modsync"

  lane() {
    python3 tools/fullrun.py --lane "$1" --binary "$binary" \
      --guide "$MODSYNC_FULLRUN_GUIDE" --game-root "$MODSYNC_FULLRUN_GAME" \
      --mods-root "$MODSYNC_FULLRUN_MODS" --selection-args-json "$selection" \
      --supplied-runner-json "$MODSYNC_FULLRUN_DRIVER" --holopatcher "$holo" \
      --work-dir "$run/work-$1" --output "$run/evidence-$1"
  }
  compare() {
    python3 tools/compare_filesystem_traces.py "$@"
  }

  stage=prediction
  lane predict
  lanes=(supplied ody-patcher)
  if [[ -n "$holo" ]]; then lanes+=(holo-patcher); fi
  for backend in "${lanes[@]}"; do
    stage="$backend-install"
    lane "$backend"
    stage="$backend-vs-prediction"
    compare --final-only "$run/evidence-predict/trace" "$run/evidence-$backend/trace"
    if [[ "$backend" != supplied ]]; then
      stage="$backend-vs-supplied"
      compare "$run/evidence-supplied/trace" "$run/evidence-$backend/trace"
    fi
  done
} >"$run/fullrun.log" 2>&1
echo 'All selected real-asset lanes matched. Detailed evidence stayed on the runner.'
