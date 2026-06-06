#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-windowed-smoke.XXXXXX")"
KEEP_ARTIFACTS=0

while [ $# -gt 0 ]; do
    case "$1" in
        --keep-artifacts)
            KEEP_ARTIFACTS=1
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

cleanup() {
    if [ "$KEEP_ARTIFACTS" -ne 1 ]; then
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT INT TERM

cd "$ROOT"

capture() {
    name="$1"
    shift
    screenshot="$TMP_DIR/$name.png"
    cargo run -p seatshell-shell -- "$@" "--screenshot=$screenshot" >/dev/null 2>&1
    [ -s "$screenshot" ] || {
        echo "windowed shell smoke failed: missing screenshot $screenshot" >&2
        exit 1
    }
}

echo "-> windowed screenshot: desktop"
capture desktop --windowed --desktop --window-size=1280x760

echo "-> windowed screenshot: launcher"
capture launcher --windowed --launcher --window-size=1180x760

echo "-> windowed screenshot: system-center"
capture system-center --windowed --system-center --window-size=1440x500

echo "Windowed shell smoke passed"
if [ "$KEEP_ARTIFACTS" -eq 1 ]; then
    echo "Kept screenshots in $TMP_DIR"
fi
