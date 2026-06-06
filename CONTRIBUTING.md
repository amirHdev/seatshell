# Contributing to SeatShell

SeatShell is trying to become a real desktop project with a clear identity, not just a collection of shell experiments. Contributions are welcome, but the best ones reinforce the product direction instead of pulling it toward feature sprawl.

## Before You Change Code

Start by reading:

- [README.md](README.md) for the current product overview and run/install paths
- [ROADMAP.md](ROADMAP.md) for the ordered build targets
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for crate/runtime boundaries
- [docs/DESIGN.md](docs/DESIGN.md) if your work touches the shell UI
- [docs/SECURITY.md](docs/SECURITY.md) if your work touches session control, authorization, or D-Bus

If you want to help SeatShell feel like a serious project, prioritize work that improves one of these areas:

- session reliability
- desktop usability
- overview trust and privacy model
- contributor experience
- packaging and test discipline

## Development Setup

Primary local checks:

```sh
cargo fmt --all
cargo check --workspace
cargo test --workspace
scripts/smoke-macos.sh
scripts/smoke-shell-dbus.sh
scripts/run-seatshell.sh --dry-run
scripts/run-seatshell.sh --windowed --dry-run
```

For UI iteration, use the nested shell first:

```sh
cargo run -p seatshell-shell -- --windowed
```

For install validation:

```sh
scripts/install-seatshell.sh --debug --prefix /tmp/seatshell-install
PREFIX=/tmp/seatshell-install scripts/validate-seathell-install.sh
PREFIX=/tmp/seatshell-install scripts/validate-display-manager-session.sh
scripts/validate-display-manager-session.sh --self-test
```

More detailed testing guidance lives in [docs/TESTING.md](docs/TESTING.md).

## Contribution Rules

- Keep changes aligned with the roadmap instead of adding unrelated features.
- Prefer small, reviewable pull requests over broad rewrites.
- Preserve SeatShell's visual and product point of view instead of defaulting to generic desktop patterns.
- Treat cross-user actions, previews, and privileged controls as security-sensitive work.
- Add or update tests when you change behavior that can be validated automatically.
- Update docs when user-visible workflows, architecture, or contribution expectations change.

## UI and Product Expectations

SeatShell should not try to out-GNOME GNOME or out-KDE KDE.

Good UI changes usually make the shell:

- calmer
- clearer
- faster to navigate with keyboard and mouse
- more distinctive from a screenshot
- more trustworthy about session state and action scope

Avoid adding visual clutter, placeholder-heavy panels, or settings surfaces that outrun runtime quality.

## Commit and PR Guidance

Aim for PRs that answer three questions clearly:

1. What user or contributor problem does this solve?
2. Why is this the right fit for SeatShell's product direction?
3. How was it tested?

Useful PR descriptions include:

- the user-facing effect
- the crates or scripts touched
- screenshots for shell UI changes
- any security or session-management implications
- follow-up work that remains intentionally out of scope

## Good First Areas

These are high-value contribution areas that help the project mature:

- desktop interaction polish
- overview state differentiation
- packaging and install validation
- VM-based integration testing
- accessibility and keyboard parity
- icon and illustration system work that matches the design language

## When to Open an Issue First

Please open an issue before starting work if the change would:

- add a major new capability
- change D-Bus interfaces
- affect session security or privilege boundaries
- introduce a new runtime dependency or compositor coupling
- significantly alter the shell's visual direction
