# ADR 0001: Device-local identity and peer transport
## Context
Agents need unattended device identity and direct browser connectivity without depending on TURN.
## Decision
Rust owns local P-256 keys, signs purpose-bound server challenges, keeps authenticated WSS signaling and uses WebRTC DTLS data channels/video. PTY, screen capture and input are separate modules; xcap/enigo provide native platform implementations. Terminal/desktop require explicit local flags.
## Alternatives
Device passwords and unsigned auto-update are rejected. A relay is deferred until direct peer authorization and platform acceptance are complete.
## Consequences
Some NATs cannot connect; errors must be visible. Desktop requires user-granted OS permissions. A one-session cap bounds initial resource use. Stable protocol versions coordinate separate repositories.
## Security Considerations
Unix permissions and Windows DPAPI protect key persistence. The control server is trusted. Session disconnect must kill its shell and stop capture. Service accounts define the maximum OS authority exposed to operators. Native Windows service/runtime validation is still required.

Native CI labels are based on https://docs.github.com/en/actions/reference/runners/github-hosted-runners ; the matrix is authored but has not been dispatched to GitHub.
