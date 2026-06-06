# Security Notes

SeatShell must treat SingleSeat management as an explicit privileged shell mode, not as an ordinary app feature.

Initial rules:

- live previews are disabled by default
- locked sessions are hidden or blurred
- admin actions must be logged before they control another user session
- GUI app launches must route through the target user's `seatshell-user-agent`
- D-Bus methods must check caller identity before they mutate session state

The current repository no longer stops at read-only discovery: it now exposes guarded `LockSession`, `LogoutSession`, `SendMessage`, and `GetSessionState` admin methods, and writes audit entries for those actions. That is still not the same thing as a finished privileged-control model.

Current limitations:

- same-user actions are the best-covered path
- cross-user actions depend on admin policy and optional `pkcheck` re-authentication
- message delivery is currently limited to the current user's active session
- restart, power-off, live previews, and cross-user app launch remain incomplete

The current D-Bus services run on the session bus and reject callers from a different Unix UID. Treat this as a baseline isolation check, not a complete authorization model. Privileged mutations still need broader Linux validation, consent rules, and cross-user delivery design before they should be treated as complete.
