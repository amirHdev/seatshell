#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
TMP_PREFIX="$(mktemp -d "${TMPDIR:-/tmp}/seatshell-macos-smoke.XXXXXX")"
KEEP_PREFIX=0

while [ $# -gt 0 ]; do
    case "$1" in
        --keep-prefix)
            KEEP_PREFIX=1
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

cleanup() {
    if [ "$KEEP_PREFIX" -ne 1 ]; then
        rm -rf "$TMP_PREFIX"
    fi
}
trap cleanup EXIT INT TERM

cd "$ROOT"

echo "== SeatShell macOS smoke =="
echo "Workspace: $ROOT"
echo "Temp prefix: $TMP_PREFIX"

echo "-> cargo fmt --all --check"
cargo fmt --all --check

echo "-> cargo check --workspace"
cargo check --workspace

echo "-> cargo test --workspace"
cargo test --workspace

echo "-> cargo run -p seatshell-admin-daemon -- --dry-run"
cargo run -p seatshell-admin-daemon -- --dry-run

echo "-> cargo run -p seatshell-user-agent -- --dry-run"
cargo run -p seatshell-user-agent -- --dry-run

echo "-> cargo run -p seatshell-session -- --dry-run"
cargo run -p seatshell-session -- --dry-run

echo "-> cargo run -p seatshell-session -- --dev-dry-run"
cargo run -p seatshell-session -- --dev-dry-run

echo "-> scripts/install-seatshell.sh --debug --prefix $TMP_PREFIX"
scripts/install-seatshell.sh --debug --prefix "$TMP_PREFIX"

echo "-> PREFIX=$TMP_PREFIX scripts/validate-seathell-install.sh"
PREFIX="$TMP_PREFIX" scripts/validate-seathell-install.sh

echo "-> PREFIX=$TMP_PREFIX scripts/validate-display-manager-session.sh"
PREFIX="$TMP_PREFIX" scripts/validate-display-manager-session.sh

echo "-> scripts/validate-display-manager-session.sh --self-test"
scripts/validate-display-manager-session.sh --self-test

echo "-> scripts/smoke-shell-dbus.sh"
scripts/smoke-shell-dbus.sh

echo "-> scripts/smoke-windowed-shell.sh"
scripts/smoke-windowed-shell.sh

echo "SeatShell macOS smoke passed"
if [ "$KEEP_PREFIX" -eq 1 ]; then
    echo "Kept temp prefix: $TMP_PREFIX"
fi
