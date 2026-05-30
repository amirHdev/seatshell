# Security Notes

SeatShell must treat SingleSeat management as an explicit privileged shell mode, not as an ordinary app feature.

Initial rules:

- live previews are disabled by default
- locked sessions are hidden or blurred
- admin actions must be logged before they control another user session
- GUI app launches must route through the target user's `seatshell-user-agent`
- D-Bus methods must check caller identity before they mutate session state

The 0.1.0 technical preview only exposes read-only admin discovery. Mutating methods such as cross-user app launch, lock, and logout remain intentionally unimplemented until Polkit-grade caller authorization and audit logging are in place.

The current D-Bus services run on the session bus and reject callers from a different Unix UID. Treat this as a baseline isolation check, not a complete authorization model. Privileged mutations still need policy, re-authentication, consent rules, and audit logging before they are added.
