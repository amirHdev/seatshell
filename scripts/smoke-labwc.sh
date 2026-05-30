#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
BIN_DIR="${SEATSHELL_BIN_DIR:-$ROOT/target/debug}"
TIMEOUT="${SEATSHELL_LABWC_TIMEOUT:-10}"

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "SKIP: missing required command: $1"
        exit 0
    }
}

require_binary() {
    binary="$1"
    if [ ! -x "$BIN_DIR/$binary" ]; then
        echo "Missing $BIN_DIR/$binary"
        echo "Build it first with: cargo build --workspace"
        exit 1
    fi
}

timeout_command() {
    if command -v timeout >/dev/null 2>&1; then
        echo timeout
    elif command -v gtimeout >/dev/null 2>&1; then
        echo gtimeout
    else
        echo ""
    fi
}

require_command dbus-run-session
require_command labwc
require_binary seatshell-session
require_binary seatshell-admin-daemon
require_binary seatshell-user-agent
require_binary seatshell-shell

TIMEOUT_CMD="$(timeout_command)"
if [ -z "$TIMEOUT_CMD" ]; then
    echo "SKIP: missing timeout or gtimeout"
    exit 0
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-labwc-smoke.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

set +e
SEATSHELL_BIN_DIR="$BIN_DIR" \
SEATSHELL_LABWC_CONFIG_DIR="$ROOT/resources/labwc" \
PATH="$BIN_DIR:$PATH" \
"$TIMEOUT_CMD" "$TIMEOUT" dbus-run-session -- "$BIN_DIR/seatshell-session" \
    >"$TMP_DIR/session.log" 2>&1
status="$?"
set -e

case "$status" in
    0|124)
        echo "SeatShell labwc smoke test completed"
        ;;
    *)
        echo "SeatShell labwc smoke test failed with status $status"
        sed -n '1,160p' "$TMP_DIR/session.log" || true
        exit "$status"
        ;;
esac
