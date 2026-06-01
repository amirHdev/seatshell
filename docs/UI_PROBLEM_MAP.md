# SeatShell UI Problem Map

This document keeps the current shell problems in one place so design and engineering work stay aligned.

## Resolution status

The May 31, 2026 windowed validation pass is complete for the shell UI layer. The desktop, panel, launcher, and overview now use one calmer hierarchy and share one primary navigation model: Desktop, Apps, and Seats. The default surface is wallpaper-first, with desktop objects, one compact glance card, optional recent files, and one taskbar for pinned and running apps.

The shell now distinguishes what the UI can resolve locally from what requires compositor integration on Linux. Running-app presence is real process-derived state. Focus, minimize, restore, and live window previews remain compositor work and are tracked as integration tasks rather than represented by misleading visual state.

The completed UI pass resolves these concrete issues:

- desktop shortcut positions are persisted, statically clamped, dynamically bounded to the live desktop lane, and clipped as a final guard
- the panel is sized to its real controls instead of squeezing a 48-52 px dock into a 34-38 px bar
- the desktop now preserves open wallpaper space instead of presenting a permanent dashboard
- launcher and overview helper areas use quieter supporting surfaces instead of stacked tutorial cards
- seat rows visually distinguish current, locked, inactive, and selected states
- supported icon formats are filtered before Slint loads them, with consistent glyph medallions as fallback
- shared controls now use short hover/state transitions
- system center, settings, and power-menu scaffolds establish the next shell integration surfaces without pretending privileged actions already work

## Resolved problem map

### 1. Shell hierarchy

- Resolved in the UI layer: Desktop, Apps, and Seats form the primary navigation pattern.
- Shared shell controls, code-drawn system icons, medallions, pills, state colors, and spacing keep the surfaces in one family.

### 2. Real state and visual state

- Resolved for available runtime data: desktop and dock presence come from discovered running processes, desktop shortcuts launch and persist positions, and recent files open through the host handoff.
- Linux compositor integration still needs to add true focus, minimize, restore, and live-window preview behavior.

### 3. GNOME-class layout discipline

- Resolved in the visual layer: the desktop has open wallpaper space, one desktop lane, one compact glance card, optional recent files, and one shell taskbar.

### 4. Desktop information density

- Resolved for the current runtime model: pinned and running apps stay visible in one taskbar without taking over the wallpaper or pretending to be live compositor windows.

### 5. Desktop-object positioning

- Resolved and regression-tested. Saved positions are clamped in Rust, live drop coordinates are bounded against the rendered lane, and the lane remains clipped.

### 6. Wrapper chrome

- Resolved for this pass. The desktop glance card, launcher helper area, and overview support rail are flatter and quieter.

### 7. Icon system

- Resolved for runtime consistency: SeatShell loads supported PNG/SVG app icons, uses one glyph-medallion fallback language for missing app assets, and draws shell-system icons directly in Slint.
- A custom icon asset pack remains an art-direction enhancement, not a correctness blocker.

### 8. Motion and interaction polish

- Resolved for core hover and state transitions. Broader scene transitions and accessibility motion preferences remain a later polish pass.

### 9. Responsive behavior

- Resolved for the windowed UI: desktop, launcher, overview, panel, and shortcut placement have explicit compact, medium, and wide behavior.

### 10. Shell scaffolds

- Resolved for the UI layer: quick settings, settings, and session/power surfaces are reachable from the panel and direct shell routes.
- Privileged lock, sign-out, restart, and power-off actions remain explicit scaffolds until the authorized Linux service is connected.

## Linux integration follow-up

- Connect the dock to compositor window focus, minimize, and restore operations.
- Replace process-derived app presence with compositor window identity where available.
- Add privacy-aware live previews in the Seat Overview.
- Validate multi-monitor placement, scale factors, and full session behavior under Linux/labwc.
- Add keyboard positioning and collision rules for desktop objects.
