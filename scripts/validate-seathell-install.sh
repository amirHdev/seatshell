#!/usr/bin/env sh
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
SESSION_DIR="$PREFIX/share/wayland-sessions"
SHARE_DIR="$PREFIX/share/seatshell"
SESSION_FILE="$SESSION_DIR/seatshell.desktop"
SESSION_LAUNCHER="$BIN_DIR/seatshell-start"

fail() {
    echo "validation failed: $1" >&2
    exit 1
}

for path in \
    "$BIN_DIR/seatshell-admin-daemon" \
    "$BIN_DIR/seatshell-session" \
    "$BIN_DIR/seatshell-shell" \
    "$BIN_DIR/seatshell-user-agent" \
    "$SESSION_LAUNCHER" \
    "$SESSION_FILE"
do
[ -e "$path" ] || fail "missing $path"
done

grep -q "^Exec=$SESSION_LAUNCHER\$" "$SESSION_FILE" || fail "session Exec does not point to $SESSION_LAUNCHER"
[ "$(sed -n 's/^TryExec=//p' "$SESSION_FILE" | head -n 1)" = "$SESSION_LAUNCHER" ] || fail "session TryExec does not point to $SESSION_LAUNCHER"
[ -d "$SHARE_DIR/labwc" ] || fail "missing $SHARE_DIR/labwc"

command -v labwc >/dev/null 2>&1 || fail "labwc is not installed"
command -v dbus-run-session >/dev/null 2>&1 || fail "dbus-run-session is not installed"

SEATSHELL_STATE_DIR="${SEATSHELL_STATE_DIR:-/tmp/seatshell-validate-state}" \
SEATSHELL_LOG_DIR="${SEATSHELL_LOG_DIR:-/tmp/seatshell-validate-state/logs}" \
SEATSHELL_SHARE_DIR="$SHARE_DIR" \
SEATSHELL_BIN_DIR="$BIN_DIR" \
"$SESSION_LAUNCHER" --dry-run >/dev/null 2>&1 || fail "session launcher dry-run failed"

echo "SeatShell install looks valid"
echo "  binaries: $BIN_DIR"
echo "  session:  $SESSION_FILE"
echo "  launcher: $SESSION_LAUNCHER"
echo "  share:    $SHARE_DIR"
