# SeatShell Roadmap

SeatShell is a Rust/Slint Wayland shell centered on a built-in SingleSeat Overview for managing local sessions without turning into a full KDE or GNOME clone. The goal is not feature parity with the large desktop environments. The goal is a focused shell with a clear point of view, reliable session behavior, and a signature user/session-management experience.

This roadmap is ordered by product value and engineering risk. Each phase should leave the project in a shippable state.

## Product Direction

SeatShell should become:

- a real Wayland desktop session, not just a nested demo window
- a keyboard-friendly shell with launcher, desktop, panel, and overview workflows
- a trustworthy local session-management surface
- a visually distinctive desktop environment with a strong conceptual point of view
- a small, understandable codebase with strong packaging and test discipline

## Current Baseline

The repository already has:

- a `labwc`-backed session launcher
- a Slint shell window with desktop, launcher, panel, and overview surfaces
- a first-pass shared shell design system for spacing, color, and component styling
- D-Bus services for admin discovery and user-side app launch
- desktop file discovery and launching
- release/install scripts and a session desktop entry

## Phase 1: Production Session

Objective: make SeatShell boot and behave like a real desktop session.

### Milestones

- [x] Start `labwc`, SeatShell services, and shell from a coordinated session runner
- [x] Export session-scoped environment for child processes
- [x] Prevent duplicate autostarted processes when the session runner already owns lifecycle
- [x] Support nested windowed testing
- [x] Generate an installable Wayland session launcher and validation script
- [x] Validate generated Wayland session metadata and greeter-visible install paths
- [ ] Validate start-to-desktop flow from a display manager
- [x] Add restart/recovery behavior when a child process exits unexpectedly
- [x] Add persistent runtime logs under `~/.local/state/seatshell`
- [x] Add explicit health checks for D-Bus services during session startup

### Definition of done

- login manager launch works
- shell appears consistently
- compositor shortcuts target the live shell, not extra shell instances
- logs make failures diagnosable

## Phase 2: Daily-Usable Shell

Status: closed for the current shell UX scope. The daily shell workflow, first-run docs, empty states, and responsive windowed layouts are now in place; Linux login-session validation remains tracked under Phase 1.

Objective: turn the shell from a proof-of-concept into something someone can live in.

### Milestones

- [x] Desktop quick-launch surface
- [x] Recent application recall
- [x] Launcher search with desktop metadata
- [x] Panel counts and view toggles
- [x] Keyboard selection and focus movement in launcher
- [x] Pinned favorites managed from config/state
- [x] Running-app task strip merged with pinned favorites
- [x] Better panel information architecture for power, network, audio, notifications
- [x] Desktop context menu or command surface
- [x] Better empty/error states when no apps are discoverable
- [x] First-run user guidance in docs, not in-app clutter

### Definition of done

- apps can be launched comfortably with mouse and keyboard
- common tasks are discoverable without reading source code
- the shell looks intentional rather than accidental

## Phase 3: Signature SingleSeat Overview

Objective: make the overview the feature that justifies SeatShell existing.

### Milestones

- [x] Read-only session discovery
- [ ] Session previews with privacy boundaries
- [ ] Real lock/logout/message actions behind authorization
- [ ] Policy-aware admin controls and audit trail
- [ ] Consent and visibility model for viewing another session
- [ ] Clear distinction between current session, inactive session, and locked session
- [x] Overview keyboard navigation scaffolding
- [ ] Overview bulk actions

### Definition of done

- users understand who can see what
- admins understand what actions are allowed
- the overview is clearly more than a decorative dashboard

## Phase 4: System Integration

Objective: behave like a desktop environment instead of an app launcher with a panel.

### Milestones

- [x] Make the home surface feel like a real desktop canvas instead of a launcher/dashboard stack
- [x] Move full app browsing into launcher/overlay surfaces so the desktop itself can prioritize workspace, active windows, files, and live system state
- [ ] Replace static desktop mock objects with real desktop interactions
- [ ] Draggable desktop icons with saved positions across restarts
- [ ] Snap-to-grid, auto-arrange, and collision rules for desktop items
- [ ] Desktop selection model: single-select, marquee selection, and keyboard focus movement
- [ ] Desktop file objects that can be opened, highlighted, and handed off to default apps
- [ ] Dock interaction parity with user expectations: focus, minimize/restore, running state, and active-window indication
- [ ] Real active-window presence on the desktop surface instead of quiet-state placeholders when apps are running
- [ ] Multi-monitor-aware desktop layout rules so icon placement and chrome do not break on different outputs
- [ ] Notifications daemon integration
- [x] Audio status/control
- [ ] Power/battery status
- [x] NetworkManager status
- [ ] Portal integration
- [ ] Polkit integration
- [ ] `systemd --user` and `loginctl` session awareness
- [ ] Clipboard lifecycle behavior
- [ ] Default application and file-open handoff

### Definition of done

- browsers, editors, portals, and core desktop flows behave normally
- desktop objects can be moved, selected, relaunched, and trusted to stay where the user put them
- the dock and desktop surface reflect real application state instead of static placeholders
- SeatShell can be a primary session on a developer workstation

## Phase 5: Quality and Packaging

Objective: make contributors and testers trust the project.

### Milestones

- [ ] VM-based integration tests under `dbus-run-session`
- [ ] labwc smoke tests
- [ ] shell D-Bus control tests
- [ ] packaging for Debian, Arch, and Fedora
- [ ] reproducible release checklist
- [ ] architecture docs for services and UI boundaries
- [ ] issue templates and contribution guide

### Definition of done

- a new contributor can build, test, and run the shell quickly
- regressions are caught before release

## Phase 6: Design and Accessibility

Objective: bring the shell up to “serious project” standards.

### Milestones

- [x] first-pass coherent spacing, color, and typography system
- [ ] second-pass visual polish across all shell states and interactions
- [ ] icon strategy beyond placeholder initials
- [ ] Interaction polish for desktop manipulation: drag thresholds, drop affordances, hover feedback, and non-janky motion
- [ ] Keyboard parity for desktop objects: selection, repositioning, launching, and context actions without a mouse
- [ ] focus rings and keyboard accessibility audit
- [ ] high-DPI and scaling validation
- [ ] localization-ready strings
- [ ] performance pass for startup and UI responsiveness

### Definition of done

- the shell feels calm, readable, and robust
- accessibility is part of the build, not an afterthought

## Phase 7: Most Beautiful and Conceptual Desktop Env

Status: in progress. A first full conceptual art-direction pass now exists in the shell UI, theme primitives, and design notes; remaining work is icon-system depth, motion, and broader usability validation.

Objective: push SeatShell from “clean and usable” into a desktop environment with unmistakable authorship, atmosphere, and emotional pull.

### Milestones

- [x] Define a signature art direction for typography, color, light, texture, and motion
- [ ] Replace placeholder letter-icons with a coherent icon and illustration system
- [x] Design a more conceptual desktop home surface with stronger hierarchy and fewer generic dashboard patterns
- [ ] Add animation and transition rules that feel deliberate without harming responsiveness
- [x] Introduce a wallpaper, background, or scene treatment that gives the shell a recognisable identity
- [x] Create polished states for empty, loading, error, and success surfaces so they feel designed rather than incidental
- [ ] Validate beauty against usability: preserve keyboard speed, legibility, and low cognitive load
- [x] Produce a visual reference set or design spec so contributors can extend the same language consistently

### Definition of done

- the shell is immediately recognisable from a screenshot
- the design feels intentional, memorable, and cohesive across every surface
- aesthetic ambition improves the product instead of fighting clarity or performance

## Release Milestones

## v0.2

- stable `labwc` session startup
- remote shell commands for launcher/overview/desktop
- recent-app persistence
- roadmap and contributor-facing project direction

## v0.3

- keyboard-first launcher
- pinned favorites
- persistent logs
- display-manager validation

## v0.5

- real overview actions
- basic system integration
- package install story for at least one distro

## v1.0

- production-quality session launch
- polished daily shell workflow
- trustworthy overview permissions
- integration tests and release discipline

## What SeatShell Should Not Try To Do

SeatShell should not try to out-KDE KDE or out-GNOME GNOME.

Avoid:

- cloning every desktop feature before the core idea is mature
- tightly coupling the shell to one compositor-specific UI trick too early
- adding privileged controls before privacy and audit behavior exist
- growing a settings surface faster than the shell’s operational quality

## Next Build Targets

The next concrete engineering targets for this repository are:

1. Validate display-manager session startup end to end on a real login manager
2. Implement draggable desktop icons with persisted positions
3. Add snap-to-grid and selection behavior so desktop objects feel reliable rather than decorative
4. Add dock interaction parity for focus, minimize/restore, and running-state indication
5. Add notification daemon integration beyond shell-local storage
6. Start Phase 3 overview differentiation work for current, inactive, and locked sessions
7. Add VM-based integration tests under `dbus-run-session`
8. Add portal and polkit integration
