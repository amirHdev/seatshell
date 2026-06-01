# Architecture Overview

SeatShell is a Rust workspace that combines a Wayland session runner, a Slint shell UI, and a small D-Bus service layer around session and application behavior.

## What the Project Is

At the moment, SeatShell is best understood as four things working together:

1. a session launcher that starts and supervises the desktop stack
2. a shell UI that renders the desktop, panel, launcher, overview, and notifications
3. a user-side launch agent that starts apps and exposes session-local information
4. an admin-side discovery daemon that reports users and sessions in a read-only way

Version `1.0.0` uses `labwc` as the compositor backend for the full session path.

## Workspace Layout

### Runtime crates

- `crates/seatshell-session`
  Starts `labwc`, `seatshell-admin-daemon`, `seatshell-user-agent`, and `seatshell-shell`, waits for readiness on D-Bus, writes logs, and restarts recoverable child processes when possible.

- `crates/seatshell-shell`
  The main Slint application. It owns the visible desktop experience: panel, launcher, home surface, SingleSeat Overview, command surface, and notification center. It also exposes a shell D-Bus interface for remote view toggles and notification posting.

- `crates/seatshell-user-agent`
  A per-user session D-Bus service used for launching commands or desktop files in the current user context and for reporting simple session-local state such as running apps.

- `crates/seatshell-admin-daemon`
  A read-only admin discovery service. It currently lists users, sessions, and policy-group information. Mutating privileged behavior is intentionally deferred until authorization and audit design are stronger.

### Shared crates

- `crates/seatshell-common`
  Shared domain types such as config structures, user/session models, and core error types.

- `crates/seatshell-config`
  Config loading and layering behavior for system and user configuration.

- `crates/seatshell-protocol`
  D-Bus bus names, object paths, and method-name constants shared across services.

- `crates/seatshell-notifications`
  Notification models and storage logic used by the shell runtime.

## Runtime Model

The intended session flow is:

1. a display manager or developer script launches `seatshell-session`
2. `seatshell-session` resolves runtime paths and initializes process logs
3. it starts `labwc` unless running in nested `--windowed` mode
4. it starts the admin daemon, user agent, and shell
5. it waits for required D-Bus names to appear before declaring startup successful
6. it supervises children and attempts bounded restarts for recoverable failures

This separation keeps lifecycle control in one place instead of scattering process startup across shell UI code or compositor autostart hooks.

## UI Boundaries

The shell UI is implemented in Slint under `crates/seatshell-shell/src/ui/`.

Important surfaces include:

- `desktop.slint`
  The home surface and desktop scene

- `launcher.slint`
  Search, filtering, pinned apps, and browsing

- `overview.slint`
  The SingleSeat Overview concept and session list

- `panel.slint`
  The top-level shell controls and system status strip

- `notification-center.slint`
  In-shell notification history and dismissal

- `command-surface.slint`
  A lightweight quick-action overlay

- `system-center.slint`
  Quick settings for network, sound, brightness scaffold, notifications, and sessions

- `settings.slint`
  A desktop settings scaffold for system integration work

- `power-menu.slint`
  Session and power-action scaffolds that remain non-operative until privileged Linux services are connected

Shared colors, spacing, and shape primitives are centralized in:

- `shell-theme.slint`
- `shell-widgets.slint`

The shell's Rust `main.rs` acts as the orchestration layer that:

- loads config
- discovers applications
- derives desktop/launcher models
- polls clock and lightweight system status
- mirrors notification-store state into the UI
- listens for remote shell commands over D-Bus

The shell can route directly to scaffold surfaces for development and future compositor bindings:

- `seatshell-shell --system-center`
- `seatshell-shell --settings`
- `seatshell-shell --power-menu`

## D-Bus Boundaries

SeatShell uses D-Bus as the seam between session components.

### Admin daemon

- bus name: `org.seatshell.Admin`
- object path: `/org/seatshell/Admin`
- current scope: read-only discovery

Methods currently implemented include:

- `ListUsers`
- `ListSessions`
- `GetPolicyGroup`

### User agent

- bus name pattern: `org.seatshell.UserAgent.u<uid>`
- object path: `/org/seatshell/UserAgent`
- current scope: launching and local session reporting

Methods currently implemented include:

- `LaunchCommand`
- `LaunchDesktopFile`
- `ListRunningApps`
- `GetSessionInfo`

### Shell service

- bus name: `org.seatshell.Shell`
- object path: `/org/seatshell/Shell`
- current scope: view toggles and notification intake

View methods include desktop, launcher, overview, notifications, system center, settings, and power-menu routing. Power actions exposed by the scaffold do not perform privileged mutations yet.

## Security Model

SeatShell treats the overview and future cross-user actions as privileged-shell territory, not ordinary launcher behavior.

That means:

- privileged actions should not bypass user agents
- read-only discovery can ship earlier than mutation
- previews, lock/logout controls, and cross-user launch must be authorization-aware
- audit logging matters before admin controls become real

See [docs/SECURITY.md](SECURITY.md) for the current guardrails.

## Key Scripts and Resources

- `scripts/run-seatshell.sh`
  Convenience runner for dry-run, nested, and full-session development paths

- `scripts/install-seatshell.sh`
  Local-prefix installer for binaries, session metadata, and launch helpers

- `scripts/validate-seathell-install.sh`
  Install-tree validation for generated artifacts

- `scripts/validate-display-manager-session.sh`
  Session-entry and launcher validation for display-manager scenarios

- `resources/labwc/`
  `labwc` session resources such as environment, menu, and autostart files

- `resources/sessions/seatshell.desktop`
  Wayland session entry

## Architectural Priorities

The next architecture-level improvements that would raise project quality are:

- real display-manager startup validation on a Linux host or VM
- stronger integration tests around D-Bus and session startup
- clearer desktop-state ownership as interactions move from mock objects to real ones
- portal, polkit, and login-session integration before privileged overview actions ship
