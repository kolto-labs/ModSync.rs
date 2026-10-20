#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: tools/build.sh [--ui qt|egui|headless] [cargo arguments...]

Selects the GUI toolkit for a cargo build. The toolkit comes from --ui, else the
MODSYNC_UI environment variable, else qt (the default).

  qt        cargo build                                            (feature ui-qt)
  egui      cargo build --no-default-features --features ui-egui   (small binary)
  headless  cargo build --no-default-features                      (CLI only)

Remaining arguments pass to cargo build unchanged, for example:
  MODSYNC_UI=egui tools/build.sh --release --locked --bin modsync
USAGE
}

ui="${MODSYNC_UI:-qt}"
if [[ "${1:-}" == "--ui" ]]; then
    [[ $# -ge 2 ]] || { usage >&2; exit 2; }
    ui="$2"
    shift 2
elif [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

case "${ui,,}" in
    qt | "") features=() ;;
    egui) features=(--no-default-features --features ui-egui) ;;
    headless | none) features=(--no-default-features) ;;
    *)
        echo "MODSYNC_UI must be qt, egui or headless, not '${ui}'" >&2
        exit 2
        ;;
esac

cd "$(dirname "$0")/.."
exec cargo build ${features[@]+"${features[@]}"} "$@"
