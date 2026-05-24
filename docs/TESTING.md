# Testing

Start with nested development testing:

```sh
cargo check
cargo check --workspace
cargo test --workspace
cargo run -p seatshell-shell
cargo run -p seatshell-session -- --dry-run
cargo run -p seatshell-session -- --dev-dry-run
cargo run -p seatshell-user-agent -- --dry-run
cargo run -p seatshell-admin-daemon -- --dry-run
scripts/run-seatshell.sh --dry-run
scripts/run-seatshell.sh --windowed --dry-run
scripts/install-seatshell.sh --debug --prefix /tmp/seatshell-install
PREFIX=/tmp/seatshell-install scripts/validate-seathell-install.sh
PREFIX=/tmp/seatshell-install scripts/validate-display-manager-session.sh
```

`seatshell-shell` uses a Slint build script, so macOS checks and tests link a host build binary. If Cargo reports that the Xcode license has not been accepted, run `sudo xcodebuild -license` in Terminal before running full workspace checks.

The local release gate is:

```sh
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo run -p seatshell-session -- --dry-run
cargo run -p seatshell-session -- --dev-dry-run
cargo run -p seatshell-user-agent -- --dry-run
cargo run -p seatshell-admin-daemon -- --dry-run
scripts/run-seatshell.sh --dry-run
scripts/run-seatshell.sh --windowed --dry-run
scripts/install-seatshell.sh --debug --prefix /tmp/seatshell-install
PREFIX=/tmp/seatshell-install scripts/validate-seathell-install.sh
PREFIX=/tmp/seatshell-install scripts/validate-display-manager-session.sh
```

Then move to a Linux VM with labwc:

```sh
dbus-run-session labwc
seatshell-shell &
seatshell-user-agent &
```

Before tagging a desktop release, verify in that Linux/labwc session that:

- `seatshell-shell` starts without compositor or Slint backend errors.
- the panel can toggle desktop, launcher, and overview.
- the desktop shows quick-launch entries and recent launches after opening apps.
- launcher search filters application rows and Enter launches the first match.
- launcher entries from `resources/applications` appear and launch expected commands available in the VM.
- `seatshell-user-agent` owns `org.seatshell.UserAgent.u<uid>` on the session bus.
- `seatshell-admin-daemon` owns `org.seatshell.Admin` and returns users/sessions over D-Bus.
- logs are written under `~/.local/state/seatshell/logs` and capture session child output.

On a host that actually runs a display manager, also validate the system-visible session entry:

```sh
PREFIX=/usr/local scripts/validate-display-manager-session.sh --strict-host
```

That check should confirm the generated `seatshell.desktop` is installed into a Wayland session directory visible to the greeter and that its `Exec` and `TryExec` launcher still run with `--dry-run`.

Launcher development app discovery can be tested with:

```sh
mkdir -p resources/applications
cargo run -p seatshell-shell -- --launcher
```
