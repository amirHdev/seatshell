# Security Notes

SeatShell must treat SingleSeat management as an explicit privileged shell mode, not as an ordinary app feature.

Initial rules:

- live previews are disabled by default
- locked sessions are hidden or blurred
- admin actions must be logged before they control another user session
- GUI app launches must route through the target user's `seatshell-user-agent`
- D-Bus methods must check caller identity before they mutate session state

The 1.0.0 admin daemon only exposes read-only user/session discovery. Mutating methods such as cross-user app launch, lock, and logout remain intentionally unimplemented until caller authorization and audit logging are in place.
