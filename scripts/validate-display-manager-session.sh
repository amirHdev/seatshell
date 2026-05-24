#!/usr/bin/env sh
set -eu

PREFIX="${PREFIX:-}"
STRICT_HOST=0

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)
            PREFIX="$2"
            shift
            ;;
        --strict-host)
            STRICT_HOST=1
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

fail() {
    echo "display-manager validation failed: $1" >&2
    exit 1
}

info() {
    echo "$1"
}

detect_display_manager() {
    if [ -L /etc/systemd/system/display-manager.service ]; then
        basename "$(readlink -f /etc/systemd/system/display-manager.service)"
        return
    fi

    for unit in gdm.service sddm.service lightdm.service ly.service; do
        if systemctl list-unit-files "$unit" 2>/dev/null | grep -q "^$unit"; then
            echo "$unit"
            return
        fi
    done

    echo "unknown"
}

session_file_for_prefix() {
    prefix="$1"
    echo "$prefix/share/wayland-sessions/seatshell.desktop"
}

first_existing_session_file() {
    for path in \
        /usr/local/share/wayland-sessions/seatshell.desktop \
        /usr/share/wayland-sessions/seatshell.desktop \
        "$HOME/.local/share/wayland-sessions/seatshell.desktop"
    do
        if [ -f "$path" ]; then
            echo "$path"
            return
        fi
    done

    return 1
}

read_desktop_key() {
    key="$1"
    file="$2"
    sed -n "s/^$key=//p" "$file" | head -n 1
}

validate_session_file() {
    file="$1"
    [ -f "$file" ] || fail "missing session file $file"

    exec_path="$(read_desktop_key Exec "$file")"
    try_exec_path="$(read_desktop_key TryExec "$file")"

    [ -n "$exec_path" ] || fail "session file $file is missing Exec"
    [ -x "$exec_path" ] || fail "session Exec target is not executable: $exec_path"
    [ -n "$try_exec_path" ] || fail "session file $file is missing TryExec"
    [ "$try_exec_path" = "$exec_path" ] || fail "TryExec does not match Exec in $file"

    if command -v desktop-file-validate >/dev/null 2>&1; then
        desktop-file-validate "$file" >/dev/null 2>&1 || fail "desktop-file-validate rejected $file"
    fi

    share_dir="${SEATSHELL_SHARE_DIR:-$(dirname "$(dirname "$exec_path")")/share/seatshell}"
    state_dir="${SEATSHELL_STATE_DIR:-/tmp/seatshell-dm-validate-state}"
    log_dir="${SEATSHELL_LOG_DIR:-$state_dir/logs}"

    SEATSHELL_SHARE_DIR="$share_dir" \
    SEATSHELL_STATE_DIR="$state_dir" \
    SEATSHELL_LOG_DIR="$log_dir" \
    "$exec_path" --dry-run >/dev/null 2>&1 || fail "session launcher dry-run failed from $file"
}

DM_UNIT="$(detect_display_manager)"
info "Detected display manager: $DM_UNIT"

if [ -n "$PREFIX" ]; then
    SESSION_FILE="$(session_file_for_prefix "$PREFIX")"
    validate_session_file "$SESSION_FILE"
    info "Prefix session file validated: $SESSION_FILE"
fi

HOST_SESSION_FILE="$(first_existing_session_file || true)"

if [ -n "$HOST_SESSION_FILE" ]; then
    validate_session_file "$HOST_SESSION_FILE"
    info "Host-visible session file validated: $HOST_SESSION_FILE"
else
    info "No host-visible SeatShell session file found in /usr/local/share/wayland-sessions, /usr/share/wayland-sessions, or ~/.local/share/wayland-sessions."
    if [ "$STRICT_HOST" -eq 1 ]; then
        fail "SeatShell is not installed into a display-manager-visible session directory"
    fi
fi

if [ -n "$PREFIX" ]; then
    info "To expose SeatShell to the login screen, install the generated session file under /usr/local/share/wayland-sessions or /usr/share/wayland-sessions."
fi
