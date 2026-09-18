# Changelog
## Unreleased
- Added local P-256 identity, protected Unix key files and Windows DPAPI persistence.
- Added secure enrollment/activation and authenticated WSS with exponential retry.
- Added real WebRTC PTY terminal and H.264 primary-display capture/input implementation.
- Added explicit terminal/desktop capability opt-ins and fail-closed disconnect handling.
- Verified local crypto, codec and WebRTC PTY tests; browser terminal acceptance passes.
- Added native Windows SCM lifecycle and DPAPI key protection; Windows runtime acceptance is pending.
- Recorded the upstream bincode maintenance warning from RustSec.

### Deployment hardening — 2026-09-16
- Private CA trust, held-key cleanup, opt-in text clipboard writes, signed offline staging and local native packaging.
- Real HTTPS/WSS acceptance and update tamper rejection passed. See docs/changes/2026-09-16-private-pki-and-signed-staging.md.

## 0.2.0 — 2026-09-16
- Explicit monitor selection, bidirectional clipboard, local reboot gate.
- Signed executable install and service rollback; native PKG/MSI packaging sources.
- WebRTC 0.17.2 removes the previous bincode dependency. See docs/STATUS.md for runtime gates.

## 0.3.0 — 2026-09-16
- Opt-in Opus system audio; macOS ScreenCaptureKit, Linux monitor and Windows loopback implementations.
- Bounded verified WebRTC file transfer, native clipboard shortcuts and live display selection.
- Linux graphical user-service installer; Raspberry Pi acceptance blocked by storage errors.

- Fixed signaling Pong handling discovered during real Raspberry Pi connection; added a >20-second browser keepalive regression. Added Linux Wayland build dependency and cross-platform tempfile test dependency. Raspberry Pi ARM64 user-service installation and live desktop connection verified.

- Fixed H.264 recovery: enable default WebRTC NACK/report interceptors, honor PLI/FIR feedback with a fresh intra frame, refresh every two seconds, and bound sample writes. Fresh-decoder regression and extended browser video continuity test pass.
