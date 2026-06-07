#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
BIN_DIR="${SEATSHELL_BIN_DIR:-$ROOT/target/debug}"
TIMEOUT="${SEATSHELL_SESSION_TIMEOUT:-20}"

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
require_command gdbus
require_binary seatshell-session
require_binary seatshell-admin-daemon
require_binary seatshell-user-agent
require_binary seatshell-shell

if [ "${1:-}" != "--inside-dbus" ]; then
    TIMEOUT_CMD="$(timeout_command)"
    TMP_OUT="$(mktemp "${TMPDIR:-/tmp}/seatshell-session-linux-output.XXXXXX")"
    trap 'rm -f "$TMP_OUT"' EXIT INT TERM

    if [ -n "$TIMEOUT_CMD" ]; then
        set +e
        "$TIMEOUT_CMD" "$TIMEOUT" dbus-run-session -- "$0" --inside-dbus >"$TMP_OUT" 2>&1
        status="$?"
        set -e
    else
        set +e
        dbus-run-session -- "$0" --inside-dbus >"$TMP_OUT" 2>&1
        status="$?"
        set -e
    fi

    cat "$TMP_OUT"
    if [ "$status" -eq 127 ] && grep -q "Failed to start message bus" "$TMP_OUT"; then
        echo "SKIP: dbus-run-session could not start a session bus on this host"
        exit 0
    fi
    exit "$status"
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-session-linux.XXXXXX")"
STATE_DIR="$TMP_DIR/state"
LOG_DIR="$STATE_DIR/logs"
cleanup() {
    shutdown_session
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

shutdown_session() {
    if [ -z "${SESSION_PID:-}" ]; then
        return
    fi

    kill -INT "$SESSION_PID" >/dev/null 2>&1 || true

    i=0
    while kill -0 "$SESSION_PID" >/dev/null 2>&1 && [ "$i" -lt 20 ]; do
        i=$((i + 1))
        sleep 0.1
    done

    if kill -0 "$SESSION_PID" >/dev/null 2>&1; then
        kill -TERM "$SESSION_PID" >/dev/null 2>&1 || true
        i=0
        while kill -0 "$SESSION_PID" >/dev/null 2>&1 && [ "$i" -lt 20 ]; do
            i=$((i + 1))
            sleep 0.1
        done
    fi

    if kill -0 "$SESSION_PID" >/dev/null 2>&1; then
        kill -KILL "$SESSION_PID" >/dev/null 2>&1 || true
    fi

    wait "$SESSION_PID" >/dev/null 2>&1 || true
    SESSION_PID=""
}

SEATSHELL_BIN_DIR="$BIN_DIR" \
SEATSHELL_STATE_DIR="$STATE_DIR" \
SEATSHELL_LOG_DIR="$LOG_DIR" \
PATH="$BIN_DIR:$PATH" \
"$BIN_DIR/seatshell-session" --windowed >"$TMP_DIR/session.out" 2>&1 &
SESSION_PID="$!"

wait_for_name() {
    name="$1"
    i=0
    while [ "$i" -lt 200 ]; do
        if gdbus call --session \
            --dest org.freedesktop.DBus \
            --object-path /org/freedesktop/DBus \
            --method org.freedesktop.DBus.NameHasOwner "$name" 2>/dev/null | grep -q true; then
            return 0
        fi
        i=$((i + 1))
        sleep 0.1
    done

    echo "Timed out waiting for D-Bus name: $name"
    echo "--- session stdout/stderr ---"
    sed -n '1,200p' "$TMP_DIR/session.out" || true
    echo "--- session.log ---"
    sed -n '1,200p' "$LOG_DIR/session.log" || true
    echo "--- shell.log ---"
    sed -n '1,200p' "$LOG_DIR/seatshell-shell.log" || true
    echo "--- admin.log ---"
    sed -n '1,200p' "$LOG_DIR/seatshell-admin-daemon.log" || true
    echo "--- user-agent.log ---"
    sed -n '1,200p' "$LOG_DIR/seatshell-user-agent.log" || true
    return 1
}

UID_VALUE="$(id -u)"
wait_for_name org.seatshell.Admin
wait_for_name "org.seatshell.UserAgent.u$UID_VALUE"
wait_for_name org.seatshell.Shell

shutdown_session

[ -f "$LOG_DIR/session.log" ] || {
    echo "SeatShell session smoke failed: missing $LOG_DIR/session.log"
    exit 1
}
[ -f "$LOG_DIR/seatshell-admin-daemon.log" ] || {
    echo "SeatShell session smoke failed: missing $LOG_DIR/seatshell-admin-daemon.log"
    exit 1
}
[ -f "$LOG_DIR/seatshell-user-agent.log" ] || {
    echo "SeatShell session smoke failed: missing $LOG_DIR/seatshell-user-agent.log"
    exit 1
}
[ -f "$LOG_DIR/seatshell-shell.log" ] || {
    echo "SeatShell session smoke failed: missing $LOG_DIR/seatshell-shell.log"
    exit 1
}

echo "SeatShell session smoke test completed"
