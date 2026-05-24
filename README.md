# SeatShell

SeatShell is a Rust/Slint Wayland shell focused on a built-in SingleSeat Overview for managing local user sessions.

Version 1.0.0 uses labwc as the compositor backend. The shell UI starts as normal Slint windows, while the service layer exposes D-Bus interfaces for user-agent launch requests and read-only admin discovery.

## Version 1.0.0

This repository is at a first working release:

- Cargo workspace packages are versioned at 1.0.0.
- Shared config, session, protocol, and notification models build.
- Slint shell window renders a desktop surface, panel, launcher, and SingleSeat Overview.
- Desktop and launcher views expose clickable application rows with search/filter support, quick-launch picks, and recent-app recall.
- Launcher discovers `.desktop` files and launches parsed commands without brittle whitespace splitting.
- Launcher parses comment/category metadata so the shell can present richer desktop-friendly app cards.
- Overview is fed from runtime session data through the admin D-Bus service, with a local fallback for development.
- `seatshell-user-agent` registers launch and session-info D-Bus methods.
- `seatshell-admin-daemon` registers read-only `ListUsers`, `ListSessions`, and policy-group D-Bus methods.
- `seatshell-session` starts labwc, the admin daemon, the user agent, and the shell from colocated binaries or an installed prefix.
- Session logs are written under `~/.local/state/seatshell/logs` by default.
- `scripts/run-seatshell.sh` now launches a standalone labwc-backed session by default and supports `--windowed` for nested desktop testing.
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
scripts/run-seatshell.sh --dry-run
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

Install into a temporary or custom prefix for validation:

```sh
scripts/install-seatshell.sh --debug --prefix /tmp/seatshell-install
PREFIX=/tmp/seatshell-install scripts/validate-seathell-install.sh
PREFIX=/tmp/seatshell-install scripts/validate-display-manager-session.sh
```

The installer now generates:

- an absolute-path Wayland session file under `share/wayland-sessions`
- a `seatshell-start` launcher that exports SeatShell runtime/share paths
- a login-session path that can use `dbus-run-session` when needed
- a display-manager-friendly session entry with `TryExec` and LightDM desktop naming

To validate a real login-manager install on the host:

```sh
PREFIX=/usr/local scripts/validate-display-manager-session.sh --strict-host
```

That host validator detects the active display manager, checks whether `seatshell.desktop` is installed in a display-manager-visible Wayland session directory, validates the generated launcher, and runs it with `--dry-run`.

The default config is loaded from `/etc/seatshell/config.toml`, then `~/.config/seatshell/config.toml` when those files exist. Missing files are fine; built-in defaults are used, and partial user config now layers cleanly over system defaults instead of replacing them wholesale.

Runtime state defaults to `~/.local/state/seatshell`, with per-process logs under `~/.local/state/seatshell/logs`. You can override those with `SEATSHELL_STATE_DIR` and `SEATSHELL_LOG_DIR`.

On macOS, Rust builds that link binaries require the Xcode license to be accepted first. If linking fails with an SDK/license error, run `sudo xcodebuild -license` in Terminal and then rerun the Cargo command.
