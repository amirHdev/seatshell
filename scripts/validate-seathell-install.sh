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
[ -f "$SHARE_DIR/labwc/rc.xml" ] || fail "missing $SHARE_DIR/labwc/rc.xml"
[ -f "$SHARE_DIR/wallpapers/default.png" ] || fail "missing $SHARE_DIR/wallpapers/default.png"

launcher_plan_must_include() {
    plan="$1"
    expected="$2"
    printf '%s\n' "$plan" | grep -F "$expected" >/dev/null 2>&1 \
        || fail "launcher plan did not include expected entry: $expected"
}

DRY_RUN_PLAN="$(SEATSHELL_STATE_DIR="${SEATSHELL_STATE_DIR:-/tmp/seatshell-validate-state}" \
SEATSHELL_LOG_DIR="${SEATSHELL_LOG_DIR:-/tmp/seatshell-validate-state/logs}" \
SEATSHELL_SHARE_DIR="$SHARE_DIR" \
SEATSHELL_BIN_DIR="$BIN_DIR" \
"$SESSION_LAUNCHER" --dry-run 2>/dev/null)" || fail "session launcher dry-run failed"

DEV_DRY_RUN_PLAN="$(SEATSHELL_STATE_DIR="${SEATSHELL_STATE_DIR:-/tmp/seatshell-validate-state}" \
SEATSHELL_LOG_DIR="${SEATSHELL_LOG_DIR:-/tmp/seatshell-validate-state/logs}" \
SEATSHELL_SHARE_DIR="$SHARE_DIR" \
SEATSHELL_BIN_DIR="$BIN_DIR" \
"$SESSION_LAUNCHER" --dev-dry-run 2>/dev/null)" || fail "session launcher dev dry-run failed"

launcher_plan_must_include "$DRY_RUN_PLAN" "labwc"
launcher_plan_must_include "$DRY_RUN_PLAN" "$BIN_DIR/seatshell-admin-daemon"
launcher_plan_must_include "$DRY_RUN_PLAN" "$BIN_DIR/seatshell-user-agent"
launcher_plan_must_include "$DRY_RUN_PLAN" "$BIN_DIR/seatshell-shell"
launcher_plan_must_include "$DEV_DRY_RUN_PLAN" "$BIN_DIR/seatshell-admin-daemon --dry-run"
launcher_plan_must_include "$DEV_DRY_RUN_PLAN" "$BIN_DIR/seatshell-user-agent --dry-run"
launcher_plan_must_include "$DEV_DRY_RUN_PLAN" "$BIN_DIR/seatshell-shell"

echo "SeatShell install looks valid"
echo "  binaries: $BIN_DIR"
echo "  session:  $SESSION_FILE"
echo "  launcher: $SESSION_LAUNCHER"
echo "  share:    $SHARE_DIR"
