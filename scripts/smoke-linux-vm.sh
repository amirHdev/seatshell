#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
STRICT_HOST=0
RUN_RELEASE=0

while [ $# -gt 0 ]; do
    case "$1" in
        --strict-host)
            STRICT_HOST=1
            ;;
        --release)
            RUN_RELEASE=1
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

run_step() {
    echo
    echo "==> $1"
    shift
    "$@"
}

if [ "$RUN_RELEASE" -eq 1 ]; then
    run_step "Building release workspace" cargo build --workspace --release
    export SEATSHELL_BIN_DIR="$ROOT/target/release"
    install_profile="--release"
else
    run_step "Building debug workspace" cargo build --workspace
    export SEATSHELL_BIN_DIR="$ROOT/target/debug"
    install_profile="--debug"
fi

run_step "Service-pair D-Bus smoke" "$ROOT/scripts/smoke-dbus.sh"
run_step "Windowed session smoke" "$ROOT/scripts/smoke-session-linux.sh"
run_step "Labwc session smoke" "$ROOT/scripts/smoke-labwc.sh"

TMP_PREFIX="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-linux-vm-install.XXXXXX")"
cleanup() {
    rm -rf "$TMP_PREFIX"
}
trap cleanup EXIT INT TERM

run_step \
    "Installing into temporary prefix" \
    "$ROOT/scripts/install-seatshell.sh" "$install_profile" --prefix "$TMP_PREFIX"
run_step \
    "Validating install layout" \
    env PREFIX="$TMP_PREFIX" "$ROOT/scripts/validate-seathell-install.sh"
run_step \
    "Validating generated display-manager session" \
    env PREFIX="$TMP_PREFIX" "$ROOT/scripts/validate-display-manager-session.sh" --skip-host

if [ "$STRICT_HOST" -eq 1 ]; then
    run_step \
        "Validating host-visible display-manager install" \
        env PREFIX=/usr/local "$ROOT/scripts/validate-display-manager-session.sh" --strict-host
fi

echo
echo "SeatShell Linux VM smoke completed"
