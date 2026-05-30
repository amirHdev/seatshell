# Contributing to SeatShell

SeatShell is currently a technical preview. The highest-value contributions are the ones that make the shell safer, easier to run in a VM, and more honest about what works.

## Local Checks

Run the same core checks before opening a pull request:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -q -p seatshell-session -- --dry-run
cargo run -q -p seatshell-user-agent -- --dry-run
cargo run -q -p seatshell-admin-daemon -- --dry-run
scripts/smoke-dbus.sh
```

## Contribution Priorities

- Real session discovery through logind or another documented Linux session source.
- D-Bus caller authorization, audit logging, and narrow command policies.
- labwc integration tests under a Linux VM or nested compositor session.
- Native shell-surface integration instead of ordinary top-level windows.
- Freedesktop-compliant application discovery and launching.

## Review Expectations

Keep changes scoped. If a patch changes security behavior, session startup, process supervision, or D-Bus interfaces, include tests or a manual verification note.
