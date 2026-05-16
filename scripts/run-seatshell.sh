#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
BIN_DIR="${SEATSHELL_BIN_DIR:-$ROOT/target/release}"
LOG_DIR="${SEATSHELL_LOG_DIR:-/tmp}"
WINDOWED="${SEATSHELL_WINDOWED:-0}"

for arg in "$@"; do
    if [ "$arg" = "--windowed" ]; then
        WINDOWED=1
    fi
done

require_binary() {
    binary="$1"
    if [ ! -x "$BIN_DIR/$binary" ]; then
        echo "Missing $BIN_DIR/$binary"
        echo "Build it first with: cargo build --workspace --release"
        exit 1
    fi
}

start_once() {
    process_name="$1"
    shift

    if pgrep -u "$(id -u)" -f "$BIN_DIR/$process_name" >/dev/null 2>&1; then
        echo "$process_name already running"
        return
    fi

    log_file="$LOG_DIR/$process_name.log"
    setsid -f "$@" >"$log_file" 2>&1
    echo "started $process_name (log: $log_file)"
}

require_binary seatshell-admin-daemon
require_binary seatshell-user-agent
require_binary seatshell-shell

start_once seatshell-admin-daemon "$BIN_DIR/seatshell-admin-daemon"
start_once seatshell-user-agent "$BIN_DIR/seatshell-user-agent"
if [ "$WINDOWED" = "1" ]; then
    start_once seatshell-shell "$BIN_DIR/seatshell-shell" --windowed --launcher
else
    start_once seatshell-shell "$BIN_DIR/seatshell-shell" --launcher
fi

echo "SeatShell is running. Stop it with: pkill -f '$BIN_DIR/seatshell-shell'; pkill -f '$BIN_DIR/seatshell-user-agent'; pkill -f '$BIN_DIR/seatshell-admin-daemon'"
