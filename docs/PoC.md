# SeatShell 1.0.0

## Working baseline

- Workspace packages are versioned at 1.0.0.
- Shared config, session, protocol, and notification models build.
- Shell renders a Slint panel, launcher, and SingleSeat Overview.
- Session launcher can show production and development process plans with `--dry-run` and `--dev-dry-run`.
- User agent registers the current user's D-Bus service and exposes launch/session-info methods.
- Admin daemon registers the admin D-Bus service and exposes read-only user/session discovery.
- Launcher apps are provided by Rust through a Slint model.
- Launcher parses development and system `.desktop` files.
- Launcher button clicks spawn the parsed command argv.
- Overview displays runtime session data for the current shell process.

## Next

1. Route the shell overview through `seatshell-admin-daemon` over D-Bus instead of local process environment data.
2. Add privileged admin mutations for lock, logout, and open-app-as-user with caller authorization.
3. Add Linux VM integration tests under `dbus-run-session labwc`.
4. Replace Slint top-level windows with native compositor shell surfaces.
