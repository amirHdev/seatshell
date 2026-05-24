#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
PROFILE="${PROFILE:-release}"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
SESSION_DIR="$PREFIX/share/wayland-sessions"
SEATSHELL_SHARE="$PREFIX/share/seatshell"
SESSION_LAUNCHER="$BIN_DIR/seatshell-start"

while [ $# -gt 0 ]; do
    case "$1" in
        --debug)
            PROFILE="debug"
            ;;
        --release)
            PROFILE="release"
            ;;
        --prefix)
            PREFIX="$2"
            BIN_DIR="$PREFIX/bin"
            APP_DIR="$PREFIX/share/applications"
            SESSION_DIR="$PREFIX/share/wayland-sessions"
            SEATSHELL_SHARE="$PREFIX/share/seatshell"
            SESSION_LAUNCHER="$BIN_DIR/seatshell-start"
            shift
            ;;
    esac
    shift
done

TARGET_DIR="$TARGET_ROOT/$PROFILE"

for binary in seatshell-admin-daemon seatshell-session seatshell-shell seatshell-user-agent; do
    if [ ! -x "$TARGET_DIR/$binary" ]; then
        echo "Missing $TARGET_DIR/$binary"
        if [ "$PROFILE" = "debug" ]; then
            echo "Build first with: ${CARGO_TARGET_DIR:+CARGO_TARGET_DIR=$CARGO_TARGET_DIR }cargo build --workspace"
        else
            echo "Build first with: ${CARGO_TARGET_DIR:+CARGO_TARGET_DIR=$CARGO_TARGET_DIR }cargo build --workspace --release"
        fi
        exit 1
    fi
done

mkdir -p "$BIN_DIR" "$APP_DIR" "$SESSION_DIR" "$SEATSHELL_SHARE/labwc"

install -m 0755 "$TARGET_DIR/seatshell-admin-daemon" "$BIN_DIR/seatshell-admin-daemon"
install -m 0755 "$TARGET_DIR/seatshell-session" "$BIN_DIR/seatshell-session"
install -m 0755 "$TARGET_DIR/seatshell-shell" "$BIN_DIR/seatshell-shell"
install -m 0755 "$TARGET_DIR/seatshell-user-agent" "$BIN_DIR/seatshell-user-agent"

install -m 0644 "$ROOT"/resources/applications/*.desktop "$APP_DIR/"
install -m 0644 "$ROOT"/resources/labwc/* "$SEATSHELL_SHARE/labwc/"

cat >"$SESSION_LAUNCHER" <<EOF
#!/usr/bin/env sh
set -eu

export SEATSHELL_BIN_DIR="$BIN_DIR"
export SEATSHELL_SHARE_DIR="$SEATSHELL_SHARE"
export SEATSHELL_STATE_DIR="\${SEATSHELL_STATE_DIR:-\${XDG_STATE_HOME:-\$HOME/.local/state}/seatshell}"
export SEATSHELL_LOG_DIR="\${SEATSHELL_LOG_DIR:-\$SEATSHELL_STATE_DIR/logs}"
export PATH="$BIN_DIR:\$PATH"

if [ -z "\${DBUS_SESSION_BUS_ADDRESS:-}" ] && command -v dbus-run-session >/dev/null 2>&1; then
    exec dbus-run-session "$BIN_DIR/seatshell-session" "\$@"
fi

exec "$BIN_DIR/seatshell-session" "\$@"
EOF
chmod 0755 "$SESSION_LAUNCHER"

cat >"$SESSION_DIR/seatshell.desktop" <<EOF
[Desktop Entry]
Name=SeatShell
Comment=Rust/Slint Wayland desktop shell with SingleSeat Overview
Exec=$SESSION_LAUNCHER
TryExec=$SESSION_LAUNCHER
Type=Application
X-LightDM-DesktopName=SeatShell
EOF

echo "Installed SeatShell into $PREFIX"
echo "Make sure $BIN_DIR is on PATH before starting the SeatShell session."
echo "Display-manager session file: $SESSION_DIR/seatshell.desktop"
echo "Session launcher: $SESSION_LAUNCHER"
echo "Share dir: $SEATSHELL_SHARE"
echo "Build artifacts: $TARGET_DIR"
echo "For a system-wide install, rerun with: PREFIX=/usr/local $0 --release"
