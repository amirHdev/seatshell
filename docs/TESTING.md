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
```

Then move to a Linux VM with labwc:

```sh
dbus-run-session labwc
seatshell-shell &
seatshell-user-agent &
```

Before tagging a desktop release, verify in that Linux/labwc session that:

- `seatshell-shell` starts without compositor or Slint backend errors.
- the panel can toggle launcher and overview.
- launcher entries from `resources/applications` appear and launch expected commands available in the VM.
- `seatshell-user-agent` owns `org.seatshell.UserAgent.u<uid>` on the session bus.
- `seatshell-admin-daemon` owns `org.seatshell.Admin` and returns users/sessions over D-Bus.

Launcher development app discovery can be tested with:

```sh
mkdir -p resources/applications
cargo run -p seatshell-shell -- --launcher
```
