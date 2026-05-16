# SeatShell

SeatShell is a Rust/Slint Wayland shell focused on a built-in SingleSeat Overview for managing local user sessions.

Version 1.0.0 uses labwc as the compositor backend. The shell UI starts as normal Slint windows, while the service layer exposes D-Bus interfaces for user-agent launch requests and read-only admin discovery.

## Version 1.0.0

This repository is at a first working release:

- Cargo workspace packages are versioned at 1.0.0.
- Shared config, session, protocol, and notification models build.
- Slint shell window renders a desktop surface, panel, launcher, and SingleSeat Overview.
- Desktop and launcher views expose clickable application rows with search/filter support.
- Launcher discovers `.desktop` files and launches parsed commands without brittle whitespace splitting.
- Overview is fed from runtime session data through the admin D-Bus service, with a local fallback for development.
- `seatshell-user-agent` registers launch and session-info D-Bus methods.
- `seatshell-admin-daemon` registers read-only `ListUsers`, `ListSessions`, and policy-group D-Bus methods.
- `seatshell-session` starts labwc, the admin daemon, the user agent, and the shell.
- labwc/session resources are checked in.

## Development

```sh
cargo check
cargo run -p seatshell-shell
cargo run -p seatshell-session -- --dry-run
cargo run -p seatshell-session -- --dev-dry-run
cargo run -p seatshell-admin-daemon
cargo run -p seatshell-user-agent
cargo run -p seatshell-shell -- --windowed
```

Build and run the desktop shell from release binaries:

```sh
cargo build --workspace --release
scripts/run-seatshell.sh
scripts/run-seatshell.sh --windowed
```

Install the release binaries, application launchers, and SeatShell session file into `~/.local`:

```sh
scripts/install-seatshell.sh
```

The default config is loaded from `/etc/seatshell/config.toml`, then `~/.config/seatshell/config.toml` when those files exist. Missing files are fine; built-in defaults are used.

On macOS, Rust builds that link binaries require the Xcode license to be accepted first. If linking fails with an SDK/license error, run `sudo xcodebuild -license` in Terminal and then rerun the Cargo command.
