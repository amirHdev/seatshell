#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
BIN_DIR=""
SHARE_DIR="${SEATSHELL_SHARE_DIR:-$ROOT/resources}"
STATE_DIR="${SEATSHELL_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/seatshell}"
LOG_DIR="${SEATSHELL_LOG_DIR:-$STATE_DIR/logs}"
WINDOWED=0
DRY_RUN=0
PROFILE_MODE=""

for arg in "$@"; do
    case "$arg" in
        --windowed)
            WINDOWED=1
            ;;
        --dry-run)
            DRY_RUN=1
            ;;
        --debug)
            PROFILE_MODE="debug"
            ;;
        --release)
            PROFILE_MODE="release"
            ;;
    esac
done

choose_bin_dir() {
    if [ -n "${SEATSHELL_BIN_DIR:-}" ]; then
        echo "$SEATSHELL_BIN_DIR"
        return
    fi

    if [ "$PROFILE_MODE" = "debug" ]; then
        echo "$ROOT/target/debug"
        return
    fi

    if [ "$PROFILE_MODE" = "release" ]; then
        echo "$ROOT/target/release"
        return
    fi

    if [ "$WINDOWED" = "1" ] && [ -x "$ROOT/target/debug/seatshell-session" ]; then
        echo "$ROOT/target/debug"
        return
    fi

    echo "$ROOT/target/release"
}

BIN_DIR="$(choose_bin_dir)"

require_binary() {
    binary="$1"
    if [ ! -x "$BIN_DIR/$binary" ]; then
        echo "Missing $BIN_DIR/$binary"
        if [ "$BIN_DIR" = "$ROOT/target/debug" ]; then
            echo "Build it first with: cargo build --workspace"
        else
            echo "Build it first with: cargo build --workspace --release"
        fi
        exit 1
    fi
}

require_binary seatshell-session
require_binary seatshell-admin-daemon
require_binary seatshell-user-agent
require_binary seatshell-shell

export SEATSHELL_BIN_DIR="$BIN_DIR"
export SEATSHELL_SHARE_DIR="$SHARE_DIR"
export SEATSHELL_STATE_DIR="$STATE_DIR"
export SEATSHELL_LOG_DIR="$LOG_DIR"
export PATH="$BIN_DIR:$PATH"

if [ "$DRY_RUN" = "1" ]; then
    if [ "$WINDOWED" = "1" ]; then
        echo "$BIN_DIR/seatshell-session --dry-run --windowed"
    else
        echo "dbus-run-session $BIN_DIR/seatshell-session"
    fi
    exit 0
fi

if [ "$WINDOWED" = "1" ]; then
    echo "Starting SeatShell in windowed mode from $BIN_DIR"
    exec "$BIN_DIR/seatshell-session" --windowed
fi

if ! command -v dbus-run-session >/dev/null 2>&1; then
    echo "dbus-run-session is required for standalone mode"
    echo "Try: scripts/run-seatshell.sh --windowed"
    exit 1
fi

if ! command -v labwc >/dev/null 2>&1; then
    echo "labwc is required for standalone mode"
    echo "Try: scripts/run-seatshell.sh --windowed"
    exit 1
fi

mkdir -p "$LOG_DIR"
echo "Starting standalone SeatShell session"
echo "Using binaries from $BIN_DIR"
echo "Logs: $LOG_DIR"
exec dbus-run-session "$BIN_DIR/seatshell-session"
