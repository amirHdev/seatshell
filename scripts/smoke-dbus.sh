#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
BIN_DIR="${SEATSHELL_BIN_DIR:-$ROOT/target/debug}"
TIMEOUT="${SEATSHELL_SMOKE_TIMEOUT:-20}"

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
        echo "Build it first with: cargo build -p seatshell-admin-daemon -p seatshell-user-agent"
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
require_binary seatshell-admin-daemon
require_binary seatshell-user-agent

if [ "${1:-}" != "--inside-dbus" ]; then
    TIMEOUT_CMD="$(timeout_command)"
    TMP_OUT="$(mktemp "${TMPDIR:-/tmp}/seatshell-dbus-smoke-output.XXXXXX")"
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

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-dbus-smoke.XXXXXX")"
cleanup() {
    if [ -n "${ADMIN_PID:-}" ]; then
        kill "$ADMIN_PID" >/dev/null 2>&1 || true
    fi
    if [ -n "${AGENT_PID:-}" ]; then
        kill "$AGENT_PID" >/dev/null 2>&1 || true
    fi
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

"$BIN_DIR/seatshell-admin-daemon" >"$TMP_DIR/admin.log" 2>&1 &
ADMIN_PID="$!"
"$BIN_DIR/seatshell-user-agent" >"$TMP_DIR/user-agent.log" 2>&1 &
AGENT_PID="$!"

wait_for_name() {
    name="$1"
    i=0
    while [ "$i" -lt 100 ]; do
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
    echo "--- admin log ---"
    sed -n '1,120p' "$TMP_DIR/admin.log" || true
    echo "--- user-agent log ---"
    sed -n '1,120p' "$TMP_DIR/user-agent.log" || true
    return 1
}

UID_VALUE="$(id -u)"
USER_AGENT_NAME="org.seatshell.UserAgent.u$UID_VALUE"

wait_for_name org.seatshell.Admin
wait_for_name "$USER_AGENT_NAME"

gdbus call --session \
    --dest org.seatshell.Admin \
    --object-path /org/seatshell/Admin \
    --method org.seatshell.Admin.ListUsers >/dev/null

gdbus call --session \
    --dest org.seatshell.Admin \
    --object-path /org/seatshell/Admin \
    --method org.seatshell.Admin.ListSessions >/dev/null

gdbus call --session \
    --dest "$USER_AGENT_NAME" \
    --object-path /org/seatshell/UserAgent \
    --method org.seatshell.UserAgent.GetSessionInfo >/dev/null

echo "SeatShell D-Bus smoke test passed"
