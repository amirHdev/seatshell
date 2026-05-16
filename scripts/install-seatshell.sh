#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
SESSION_DIR="$PREFIX/share/wayland-sessions"
SEATSHELL_SHARE="$PREFIX/share/seatshell"

for binary in seatshell-admin-daemon seatshell-session seatshell-shell seatshell-user-agent; do
    if [ ! -x "$ROOT/target/release/$binary" ]; then
        echo "Missing $ROOT/target/release/$binary"
        echo "Build first with: cargo build --workspace --release"
        exit 1
    fi
done

mkdir -p "$BIN_DIR" "$APP_DIR" "$SESSION_DIR" "$SEATSHELL_SHARE/labwc"

install -m 0755 "$ROOT/target/release/seatshell-admin-daemon" "$BIN_DIR/seatshell-admin-daemon"
install -m 0755 "$ROOT/target/release/seatshell-session" "$BIN_DIR/seatshell-session"
install -m 0755 "$ROOT/target/release/seatshell-shell" "$BIN_DIR/seatshell-shell"
install -m 0755 "$ROOT/target/release/seatshell-user-agent" "$BIN_DIR/seatshell-user-agent"

install -m 0644 "$ROOT"/resources/applications/*.desktop "$APP_DIR/"
install -m 0644 "$ROOT/resources/sessions/seatshell.desktop" "$SESSION_DIR/seatshell.desktop"
install -m 0644 "$ROOT"/resources/labwc/* "$SEATSHELL_SHARE/labwc/"

echo "Installed SeatShell into $PREFIX"
echo "Make sure $BIN_DIR is on PATH before starting the SeatShell session."
echo "A display manager may require the session file in /usr/share/wayland-sessions."
