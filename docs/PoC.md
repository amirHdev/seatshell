# SeatShell 0.1.0 Technical Preview

## Working baseline

- Workspace packages are versioned at 0.1.0.
- Shared config, session, protocol, and notification models build.
- Shell renders a Slint desktop surface, panel, launcher, and SingleSeat Overview.
- Session launcher starts labwc, the admin daemon, the user agent, and the shell; it can show production and development process plans with `--dry-run` and `--dev-dry-run`.
- User agent registers the current user's D-Bus service and exposes launch/session-info methods.
- Admin daemon registers the admin D-Bus service and exposes read-only user/session discovery.
- Launcher apps are provided by Rust through a Slint model.
- Launcher parses development and system `.desktop` files.
- Desktop and launcher app rows spawn the parsed command argv.
- Launcher search filters by application name, command, and desktop id.
- Overview loads logind-backed session data from `seatshell-admin-daemon` over D-Bus and falls back to the current shell process when logind or the daemon is unavailable.
- Shell hotkeys and menu actions use the running `org.seatshell.Shell` D-Bus service for desktop, launcher, and overview control.
- The shell applies configured top or bottom panel placement.
- D-Bus service calls reject callers from a different Unix UID.

## Next

1. Add privileged admin mutations for lock, logout, and open-app-as-user with Polkit-grade caller authorization.
2. Add Linux VM integration tests under `dbus-run-session labwc`.
3. Replace Slint top-level windows with native compositor shell surfaces.
4. Expand logind session-state handling and validate it in a Linux VM.
5. Add distribution packaging and release artifacts.
