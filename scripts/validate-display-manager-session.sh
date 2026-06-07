#!/usr/bin/env sh
set -eu

PREFIX="${PREFIX:-}"
STRICT_HOST=0
SELF_TEST=0
SKIP_HOST=0

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)
            PREFIX="$2"
            shift
            ;;
        --strict-host)
            STRICT_HOST=1
            ;;
        --self-test)
            SELF_TEST=1
            ;;
        --skip-host)
            SKIP_HOST=1
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

require_desktop_key() {
    key="$1"
    expected="$2"
    file="$3"
    actual="$(read_desktop_key "$key" "$file")"
    [ "$actual" = "$expected" ] || fail "$file has $key=$actual, expected $expected"
}

launcher_plan_must_include() {
    plan="$1"
    expected="$2"
    printf '%s\n' "$plan" | grep -F "$expected" >/dev/null 2>&1 \
        || fail "launcher plan did not include expected entry: $expected"
}

strict_host_path_allowed() {
    path="$1"
    home_dir="$2"
    case "$path" in
        "$home_dir"/.local/share/wayland-sessions/*)
            return 1
            ;;
        *)
            return 0
            ;;
    esac
}

validate_session_file() {
    file="$1"
    [ -f "$file" ] || fail "missing session file $file"

    exec_path="$(read_desktop_key Exec "$file")"
    try_exec_path="$(read_desktop_key TryExec "$file")"

    [ -n "$exec_path" ] || fail "session file $file is missing Exec"
    case "$exec_path" in
        /*) ;;
        *) fail "session Exec target must be an absolute path: $exec_path" ;;
    esac
    [ -x "$exec_path" ] || fail "session Exec target is not executable: $exec_path"
    [ -n "$try_exec_path" ] || fail "session file $file is missing TryExec"
    case "$try_exec_path" in
        /*) ;;
        *) fail "session TryExec target must be an absolute path: $try_exec_path" ;;
    esac
    [ "$try_exec_path" = "$exec_path" ] || fail "TryExec does not match Exec in $file"
    require_desktop_key Name "SeatShell" "$file"
    require_desktop_key Type "Application" "$file"
    [ -n "$(read_desktop_key Comment "$file")" ] || fail "session file $file is missing Comment"
    require_desktop_key X-LightDM-DesktopName "SeatShell" "$file"

    if command -v desktop-file-validate >/dev/null 2>&1; then
        desktop-file-validate "$file" >/dev/null 2>&1 || fail "desktop-file-validate rejected $file"
    fi

    share_dir="${SEATSHELL_SHARE_DIR:-$(dirname "$(dirname "$exec_path")")/share/seatshell}"
    state_dir="${SEATSHELL_STATE_DIR:-/tmp/seatshell-dm-validate-state}"
    log_dir="${SEATSHELL_LOG_DIR:-$state_dir/logs}"
    [ -d "$share_dir/labwc" ] || fail "missing $share_dir/labwc for launcher validation"
    [ -f "$share_dir/wallpapers/default.png" ] || fail "missing $share_dir/wallpapers/default.png for launcher validation"

    dry_run_plan="$(SEATSHELL_SHARE_DIR="$share_dir" \
    SEATSHELL_STATE_DIR="$state_dir" \
    SEATSHELL_LOG_DIR="$log_dir" \
    "$exec_path" --dry-run 2>/dev/null)" \
        || fail "session launcher dry-run failed from $file"

    dev_dry_run_plan="$(SEATSHELL_SHARE_DIR="$share_dir" \
    SEATSHELL_STATE_DIR="$state_dir" \
    SEATSHELL_LOG_DIR="$log_dir" \
    "$exec_path" --dev-dry-run 2>/dev/null)" \
        || fail "session launcher dev dry-run failed from $file"

    launcher_plan_must_include "$dry_run_plan" "labwc"
    launcher_plan_must_include "$dry_run_plan" "$(dirname "$exec_path")/seatshell-admin-daemon"
    launcher_plan_must_include "$dry_run_plan" "$(dirname "$exec_path")/seatshell-user-agent"
    launcher_plan_must_include "$dry_run_plan" "$(dirname "$exec_path")/seatshell-shell"
    launcher_plan_must_include "$dev_dry_run_plan" "$(dirname "$exec_path")/seatshell-admin-daemon --dry-run"
    launcher_plan_must_include "$dev_dry_run_plan" "$(dirname "$exec_path")/seatshell-user-agent --dry-run"
    launcher_plan_must_include "$dev_dry_run_plan" "$(dirname "$exec_path")/seatshell-shell"
}

self_test() {
    tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-dm-selftest.XXXXXX")"
    trap 'rm -rf "$tmp_dir"' EXIT INT TERM

    fake_prefix="$tmp_dir/prefix"
    fake_bin="$fake_prefix/bin"
    fake_share="$fake_prefix/share/seatshell"
    fake_session_dir="$fake_prefix/share/wayland-sessions"
    fake_launcher="$fake_bin/seatshell-start"
    fake_session="$fake_session_dir/seatshell.desktop"

    mkdir -p "$fake_bin" "$fake_share/labwc" "$fake_share/wallpapers" "$fake_session_dir"
    : >"$fake_share/labwc/rc.xml"
    : >"$fake_share/wallpapers/default.png"

    for binary in seatshell-admin-daemon seatshell-user-agent seatshell-shell; do
        printf '#!/usr/bin/env sh\nexit 0\n' >"$fake_bin/$binary"
        chmod +x "$fake_bin/$binary"
    done

    cat >"$fake_launcher" <<EOF
#!/usr/bin/env sh
set -eu
case "\${1:-}" in
    --dry-run)
        printf '%s\n' "labwc"
        printf '%s\n' "$fake_bin/seatshell-admin-daemon"
        printf '%s\n' "$fake_bin/seatshell-user-agent"
        printf '%s\n' "$fake_bin/seatshell-shell"
        ;;
    --dev-dry-run)
        printf '%s\n' "labwc"
        printf '%s\n' "$fake_bin/seatshell-admin-daemon --dry-run"
        printf '%s\n' "$fake_bin/seatshell-user-agent --dry-run"
        printf '%s\n' "$fake_bin/seatshell-shell"
        ;;
    *)
        exit 1
        ;;
esac
EOF
    chmod +x "$fake_launcher"

    cat >"$fake_session" <<EOF
[Desktop Entry]
Name=SeatShell
Comment=SeatShell self-test
Exec=$fake_launcher
TryExec=$fake_launcher
Type=Application
X-LightDM-DesktopName=SeatShell
EOF

    validate_session_file "$fake_session"

    if strict_host_path_allowed "$HOME/.local/share/wayland-sessions/seatshell.desktop" "$HOME"; then
        fail "strict_host_path_allowed should reject ~/.local session paths"
    fi
    strict_host_path_allowed "/usr/local/share/wayland-sessions/seatshell.desktop" "$HOME" \
        || fail "strict_host_path_allowed should allow system session paths"

    if (launcher_plan_must_include "one\ntwo" "missing") >/dev/null 2>&1; then
        fail "launcher_plan_must_include should fail when the entry is absent"
    fi

    broken_session="$tmp_dir/broken.desktop"
    sed "s#TryExec=$fake_launcher#TryExec=/missing#" "$fake_session" >"$broken_session"
    if (validate_session_file "$broken_session") >/dev/null 2>&1; then
        fail "validate_session_file should reject mismatched TryExec"
    fi

    info "display-manager validation self-test passed"
}

if [ "$SELF_TEST" -eq 1 ]; then
    self_test
    exit 0
fi

DM_UNIT="$(detect_display_manager)"
info "Detected display manager: $DM_UNIT"

if [ -n "$PREFIX" ]; then
    SESSION_FILE="$(session_file_for_prefix "$PREFIX")"
    validate_session_file "$SESSION_FILE"
    info "Prefix session file validated: $SESSION_FILE"
fi

if [ "$SKIP_HOST" -eq 0 ]; then
    HOST_SESSION_FILE="$(first_existing_session_file || true)"

    if [ -n "$HOST_SESSION_FILE" ]; then
        if [ "$STRICT_HOST" -eq 1 ] && ! strict_host_path_allowed "$HOST_SESSION_FILE" "$HOME"; then
            fail "strict host validation requires a system-visible session file outside ~/.local/share/wayland-sessions"
        fi
        validate_session_file "$HOST_SESSION_FILE"
        info "Host-visible session file validated: $HOST_SESSION_FILE"
    else
        info "No host-visible SeatShell session file found in /usr/local/share/wayland-sessions, /usr/share/wayland-sessions, or ~/.local/share/wayland-sessions."
        if [ "$STRICT_HOST" -eq 1 ]; then
            fail "SeatShell is not installed into a display-manager-visible session directory"
        fi
    fi
fi

if [ -n "$PREFIX" ]; then
    info "To expose SeatShell to the login screen, install the generated session file under /usr/local/share/wayland-sessions or /usr/share/wayland-sessions."
fi
