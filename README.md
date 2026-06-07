# SeatShell

SeatShell is a Rust/Slint Wayland shell focused on a built-in SingleSeat Overview for managing local user sessions.

Version 0.1.0 uses labwc as the compositor backend. The shell UI starts as normal Slint windows, while the service layer exposes D-Bus interfaces for user-agent launch requests, desktop notification intake, and admin session discovery/control.

## Project Overview

SeatShell is not trying to beat GNOME or KDE at feature count. Its chance to become a memorable project is to be smaller, clearer, and more opinionated:

- a recognizable desktop shell with strong visual authorship
- a trustworthy local-session overview instead of a generic control center
- a keyboard-friendly launcher and daily workflow
- a session/runtime stack that is understandable to contributors

The current architecture is a compact Rust workspace:

- `seatshell-session` supervises the session lifecycle
- `seatshell-shell` renders the UI in Slint
- `seatshell-user-agent` launches apps in the user session
- `seatshell-admin-daemon` exposes session discovery plus guarded session-control actions

For more detail, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [ROADMAP.md](ROADMAP.md), and [CONTRIBUTING.md](CONTRIBUTING.md).

## Version 0.1.0

This repository is at a first working release:

- Cargo workspace packages are versioned at 0.1.0.
- Shared config, session, protocol, and notification models build.
- Slint shell window renders a desktop surface, panel, launcher, and SingleSeat Overview.
- Desktop and launcher views expose clickable application rows with search/filter support, quick-launch picks, and recent-app recall.
- Launcher discovers `.desktop` files and launches parsed commands without brittle whitespace splitting.
- Launcher parses comment/category metadata so the shell can present richer desktop-friendly app cards.
- Overview is fed from runtime session data through the admin D-Bus service, with a local fallback for development.
- The running shell owns `org.seatshell.Shell`, so labwc hotkeys and menu actions control the existing shell instead of spawning extra overview/launcher windows.
- The running shell also owns `org.freedesktop.Notifications`, so desktop notifications can flow into the SeatShell notification center.
- Configured panel position is applied, so `panel.position = "top"` moves the panel to the top edge.
- `seatshell-user-agent` registers launch and session-info D-Bus methods.
- `seatshell-admin-daemon` registers `ListUsers`, `ListSessions`, `GetPolicyGroup`, `LockSession`, `LogoutSession`, `SendMessage`, and `GetSessionState`.
- Overview/power flows can hand off `lock` and `sign out` to the admin daemon, with audit logs under the SeatShell runtime log directory.
- `seatshell-session` starts labwc, the admin daemon, the user agent, and the shell from colocated binaries or an installed prefix.
- Session logs are written under `~/.local/state/seatshell/logs` by default.
- `scripts/run-seatshell.sh` now launches a standalone labwc-backed session by default and supports `--windowed` for nested desktop testing.
- `scripts/smoke-macos.sh` validates the macOS-safe contributor path, including install/session metadata and windowed screenshot smoke coverage.
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
scripts/smoke-macos.sh
scripts/smoke-shell-dbus.sh
scripts/smoke-session-linux.sh
scripts/smoke-linux-vm.sh
scripts/run-seatshell.sh --dry-run
```

Contributor docs:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/TESTING.md](docs/TESTING.md)
- [docs/DESIGN.md](docs/DESIGN.md)
- [docs/SECURITY.md](docs/SECURITY.md)

## First Run

For a first pass on macOS, stay in the windowed shell path instead of trying to boot the full Wayland session:

```sh
cargo run -p seatshell-shell -- --windowed
```

That path is the best place to validate the shell UI on macOS:

- inspect the panel, desktop, launcher, overview, command surface, and notifications
- resize the window to catch spacing or clipping issues
- confirm launcher search, pinned apps, recents, and overview keyboard navigation still behave correctly

For the broader macOS-safe contributor check, prefer:

```sh
scripts/smoke-macos.sh
```

That path covers workspace checks/tests, temporary install validation, display-manager validator self-tests, and windowed screenshot smoke coverage without requiring a Linux login manager.

If Cargo fails during linking on macOS, accept the Xcode license first:

```sh
sudo xcodebuild -license
```

For the first real Linux session pass, use the session runner or nested script:

```sh
scripts/run-seatshell.sh --windowed
scripts/run-seatshell.sh
```

For the current Linux smoke gate without a display manager, prefer:

```sh
scripts/smoke-session-linux.sh
scripts/smoke-linux-vm.sh
```

`scripts/smoke-session-linux.sh` verifies that `seatshell-session --windowed`
brings up `org.seatshell.Admin`, `org.seatshell.UserAgent.u<uid>`, and
`org.seatshell.Shell` on a private session bus and then shuts down cleanly.

`scripts/smoke-linux-vm.sh` is the broader Linux/VM path: it builds the
workspace, runs D-Bus and session smokes, exercises the `labwc` session when
available, installs into a temporary prefix, and validates the generated
session metadata.

With a shell already running, control that process through D-Bus:

```sh
seatshell-shell --toggle-launcher
seatshell-shell --toggle-overview
seatshell-shell --show-desktop
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
PREFIX=/tmp/seatshell-install scripts/validate-display-manager-session.sh --skip-host
```

The installer now generates:

- an absolute-path Wayland session file under `share/wayland-sessions`
- a `seatshell-start` launcher that exports SeatShell runtime/share paths
- a login-session path that can use `dbus-run-session` when needed
- a display-manager-friendly session entry with `TryExec` and LightDM desktop naming

To validate a real login-manager install on the host:

```sh
PREFIX=/usr/local scripts/validate-display-manager-session.sh --strict-host
scripts/smoke-linux-vm.sh --strict-host
```

That host validator detects the active display manager, checks whether `seatshell.desktop` is installed in a display-manager-visible Wayland session directory, validates the generated launcher, and runs it with `--dry-run` and `--dev-dry-run`.

The default config is loaded from `/etc/seatshell/config.toml`, then `~/.config/seatshell/config.toml` when those files exist. Missing files are fine; built-in defaults are used, and partial user config now layers cleanly over system defaults instead of replacing them wholesale.

Runtime state defaults to `~/.local/state/seatshell`, with per-process logs under `~/.local/state/seatshell/logs`. You can override those with `SEATSHELL_STATE_DIR` and `SEATSHELL_LOG_DIR`.

On macOS, Rust builds that link binaries require the Xcode license to be accepted first. If linking fails with an SDK/license error, run `sudo xcodebuild -license` in Terminal and then rerun the Cargo command.
