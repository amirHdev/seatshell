#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
BIN_DIR="${SEATSHELL_BIN_DIR:-$ROOT/target/debug}"
TIMEOUT="${SEATSHELL_SHELL_DBUS_TIMEOUT:-20}"

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
        echo "Build it first with: cargo build -p seatshell-shell"
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

require_command gdbus
require_binary seatshell-shell

if [ "${1:-}" != "--inside-dbus" ]; then
    TIMEOUT_CMD="$(timeout_command)"
    TMP_OUT="$(mktemp "${TMPDIR:-/tmp}/seatshell-shell-dbus-output.XXXXXX")"
    BUS_TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-shell-dbus-bus.XXXXXX")"
    trap 'rm -f "$TMP_OUT"; rm -rf "$BUS_TMP_DIR"; if [ -n "${BUS_PID:-}" ]; then kill "$BUS_PID" >/dev/null 2>&1 || true; fi' EXIT INT TERM

    if command -v dbus-daemon >/dev/null 2>&1; then
        bus_path="$BUS_TMP_DIR/bus"
        bus_output="$(dbus-daemon --session --address="unix:path=$bus_path" --fork --print-address=1 --print-pid=1)"
        BUS_ADDRESS="$(printf '%s\n' "$bus_output" | sed -n '1p')"
        BUS_PID="$(printf '%s\n' "$bus_output" | sed -n '2p')"
        if [ -n "$TIMEOUT_CMD" ]; then
            set +e
            DBUS_SESSION_BUS_ADDRESS="$BUS_ADDRESS" "$TIMEOUT_CMD" "$TIMEOUT" "$0" --inside-dbus >"$TMP_OUT" 2>&1
            status="$?"
            set -e
        else
            set +e
            DBUS_SESSION_BUS_ADDRESS="$BUS_ADDRESS" "$0" --inside-dbus >"$TMP_OUT" 2>&1
            status="$?"
            set -e
        fi
    else
        require_command dbus-run-session
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
    fi

    cat "$TMP_OUT"
    if [ "$status" -eq 127 ] && grep -q "Failed to start message bus" "$TMP_OUT"; then
        echo "SKIP: dbus-run-session could not start a session bus on this host"
        exit 0
    fi
    exit "$status"
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-shell-dbus.XXXXXX")"
cleanup() {
    if [ -n "${SHELL_PID:-}" ]; then
        kill "$SHELL_PID" >/dev/null 2>&1 || true
        wait "$SHELL_PID" 2>/dev/null || true
    fi
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

SEATSHELL_STATE_DIR="$TMP_DIR/state" \
SEATSHELL_LOG_DIR="$TMP_DIR/logs" \
SEATSHELL_BIN_DIR="$BIN_DIR" \
"$BIN_DIR/seatshell-shell" --windowed --window-size=960x640 >"$TMP_DIR/shell.log" 2>&1 &
SHELL_PID="$!"

wait_for_name() {
    name="$1"
    i=0
    while [ "$i" -lt 120 ]; do
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
    echo "--- shell log ---"
    sed -n '1,160p' "$TMP_DIR/shell.log" || true
    return 1
}

wait_for_name org.seatshell.Shell
wait_for_name org.freedesktop.Notifications

gdbus call --session \
    --dest org.seatshell.Shell \
    --object-path /org/seatshell/Shell \
    --method org.seatshell.Shell.ShowLauncher >/dev/null

gdbus call --session \
    --dest org.seatshell.Shell \
    --object-path /org/seatshell/Shell \
    --method org.seatshell.Shell.ShowOverview >/dev/null

gdbus call --session \
    --dest org.seatshell.Shell \
    --object-path /org/seatshell/Shell \
    --method org.seatshell.Shell.PostNotification "Shell smoke" "posted from smoke-shell-dbus.sh" >/dev/null

gdbus call --session \
    --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.GetCapabilities >/dev/null

server_info="$(gdbus call --session \
    --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.GetServerInformation)"
printf '%s\n' "$server_info" | grep -q "SeatShell" \
    || {
        echo "Unexpected notification server info: $server_info"
        exit 1
    }

notify_output="$(gdbus call --session \
    --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.Notify \
    "SeatShell smoke" \
    0 \
    "" \
    "Smoke title" \
    "Smoke body" \
    "[]" \
    "{}" \
    5000)"

notify_id="$(printf '%s\n' "$notify_output" | sed -n 's/.*uint32 \([0-9][0-9]*\).*/\1/p')"
[ -n "$notify_id" ] || {
    echo "Could not parse notification id from: $notify_output"
    exit 1
}

gdbus call --session \
    --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.CloseNotification "$notify_id" >/dev/null

gdbus call --session \
    --dest org.seatshell.Shell \
    --object-path /org/seatshell/Shell \
    --method org.seatshell.Shell.ClearNotifications >/dev/null

echo "SeatShell shell D-Bus smoke test passed"
