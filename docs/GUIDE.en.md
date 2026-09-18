# Remvora Agent — device installation

[English](GUIDE.en.md) · [Türkçe](GUIDE.tr.md)

## What runs on a device?

This Rust process connects a device you are authorized to administer to your own Remvora Server. Operators use Remvora Web; the agent supplies terminal, desktop/input and locally permitted clipboard/audio/files/reboot. It is not the API, does not create operator accounts, and is not a hidden/unattended monitoring product. IT teams, schools, kiosk administrators, support teams and home labs can use it. Server and Web are separate repositories; install/configure them first. Public source repositories: [Server](https://github.com/tahayildirm/remvora-server) · [Web](https://github.com/tahayildirm/remvora-web) · [Agent](https://github.com/tahayildirm/remvora-agent).

The Server guide explains database choice and first Owner bootstrap (`--migrate`, then `--bootstrap` on an empty installation). Subsequent users are created in Web → Users with email, initial password (8–256 characters) and role. There are no default credentials. Operator accounts differ from the local OS account running this agent.

## Platform and runtime prerequisites

Rust stable (edition 2024) and native build toolchains are required to build from source. The current development build used Rust 1.98; use the repository lockfile and validate your toolchain. An archive is not guaranteed to contain every OS library.

- **Linux / Raspberry Pi:** build for the actual architecture and ABI. Development prerequisites used by CI include clang, libclang-dev, libpipewire-0.3-dev, libxcb1-dev, libxrandr-dev, libxtst-dev, libxkbcommon-dev, libgbm-dev, libdrm-dev, libegl1-mesa-dev, libasound2-dev, libwayland-dev and cmake. Distribution names vary. Desktop needs an active accessible graphical session; Wayland backend permissions/compositor support matter. Linux audio needs `/usr/bin/parec` and a PulseAudio/PipeWire monitor. PTY-only use needs no active desktop but the linked binary still needs its runtime libraries.
- **macOS:** Xcode command-line tools; Screen Recording and Accessibility permission for desktop/input. Output audio uses a Swift/ScreenCaptureKit helper (macOS 13+). Run in the intended logged-in user session, not a headless LaunchDaemon.
- **Windows:** Visual Studio C++ build tools and suitable Rust target. Desktop requires an interactive session; a Session 0 service is not a way to control the logged-in desktop. Identity protection uses account-bound DPAPI. Windows runtime/installer acceptance remains incomplete.

Selected macOS ARM64 and Linux ARM64/Raspberry flows were exercised, not the entire platform matrix. Signed/notarized public installers are not yet provided. Native OS permission prompts must be handled by the device administrator. Do not disable OS safeguards just to make a test pass.

```sh
cargo build --release --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

The executable is `target/release/remvora-agent` (Windows adds `.exe`). Build locally or use an independently verified matching release when published. Check checksums/signatures, target CPU and native dependencies. Source builds can be slow on Raspberry Pi; subsequent compatible binary updates do not need to repeat the entire source toolchain build. Missing runtime packages must still be installed once. No universal all-dependencies-included installer is claimed.

## Enroll → approve → activate → run

Use a dedicated least-privilege OS account and an absolute protected state directory. The following is a Unix example; adapt executable and state paths on Windows. Run every step as the same account:

```sh
mkdir -p "$HOME/.local/state/remvora"
chmod 700 "$HOME/.local/state/remvora"
export REMVORA_SERVER=https://remvora.example/
export REMVORA_STATE_DIR="$HOME/.local/state/remvora"
```

In Web, create a device and issue an enrollment token. Transfer the token privately to `REMVORA_ENROLLMENT_TOKEN` in the device process environment; do not paste real tokens into public instructions or shell-history commands.

```sh
./target/release/remvora-agent enroll
unset REMVORA_ENROLLMENT_TOKEN
# Administrator approves the pending device in the panel, then:
./target/release/remvora-agent activate
./target/release/remvora-agent --allow-terminal run
```

Token expiry is 10 minutes by default. If it expires, issue a fresh token and repeat the applicable step. Do not repeatedly erase identity state. Device private keys are generated locally; preserve the directory during upgrades. Never clone one enrolled identity onto multiple machines. On Unix the agent enforces private permissions and rejects direct symlinks; on Windows enroll and run under the same account for DPAPI.

The server URL is the **API origin**, not the panel origin when separate subdomains are used. Use a trailing `/` and trusted HTTPS. Agent redirects are refused. `--allow-loopback-http` is exclusively for isolated loopback tests; it cannot make arbitrary public HTTP acceptable. For a private CA use `--ca-certificate /absolute/path/ca.pem` or `REMVORA_CA_CERTIFICATE`; normal hostname/certificate validation remains enabled.

## Choose capabilities explicitly

Flags precede `run`:

| Flag | Effect and boundary |
|---|---|
| `--allow-terminal` | Real shell/PTY under agent OS privileges; default Unix shell `/bin/sh`, configurable `--shell /absolute/path` |
| `--allow-desktop` | Screen capture and keyboard/mouse input, subject to OS permission and interactive display |
| `--allow-clipboard` | Explicit text clipboard reads/writes only, requires desktop; 16 KiB text limit |
| `--file-root /absolute/shared` | Existing protected flat shared directory, requires desktop; no arbitrary filesystem access |
| `--allow-audio` | System-output audio, requires desktop; platform dependencies, no microphone mode |
| `--allow-reboot` | Permits fixed native reboot only after server authorization/confirmation, no privilege elevation |
| `--monitor-id ID` | Initial display ID; missing display fails rather than silently selecting a different one |

Example after creating/protecting the shared folder:

```sh
./target/release/remvora-agent --allow-terminal --allow-desktop \
  --allow-clipboard --file-root /absolute/shared --allow-audio run
./target/release/remvora-agent list-displays
```

Stop the foreground agent before changing its flags; do not start a duplicate service. Local opt-ins do not bypass server roles/device scope. One active remote session per agent is the current model. Losing authorization/signaling ends sessions; reconnect uses backoff.

## Persistent services

Linux graphical-session install (enroll/activate first):

```sh
python3 scripts/install-linux-user.py \
  --server https://remvora.example/ \
  --binary /absolute/path/remvora-agent \
  --state /absolute/private/state \
  --allow-terminal --allow-desktop --enable
systemctl --user status remvora-agent
journalctl --user -u remvora-agent -n 100
```

Run from the intended graphical account/session so DISPLAY, WAYLAND_DISPLAY, XDG_RUNTIME_DIR and DBUS environment match. The installer refuses to overwrite an existing unit. Review/update existing units manually; do not replace unrelated kiosk services. Boot/start before login and headless capture require platform-specific design, not just enabling linger. `deploy/remvora-agent.service.example` is a system-service template for reviewed use.

macOS: adapt `deploy/com.remvora.agent.plist.example` with actual executable, state and server; enroll under the same user, load as that user’s LaunchAgent and grant OS permissions. Windows: `--service` integrates with SCM `RemvoraAgent`; use a dedicated account with matching enrollment/DPAPI identity and reviewed service arguments. Service mode does not remove Session 0 desktop limits. Native installers/signing are separate release gates; inspect `scripts/` and deployment templates before use.

## Connectivity and operating the device

Set `REMVORA_STUN_URL` to an operator-approved STUN/STUNS URI and configure the server’s `WebRtc:StunServers` for browsers. STUN is discovery, not media relay. Automatic WebRTC uses ICE candidates, then WSS relay if direct connection fails. No built-in TURN deployment exists. Both endpoints need outgoing HTTPS/WSS; P2P also depends on UDP/NAT policies. WSS relay works through the application server and costs its bandwidth; TLS is per hop, not server-blind end-to-end encryption.

Web shows connecting/P2P/relay and actual presence separately from enrollment. Desktop includes adaptive quality, device/browser-saved choices, display selection, optional audio, explicit clipboard and file controls. Mobile has direct touch, optional touchpad, pinch/pan, collapsible tools and fullscreen where supported. Terminal offers mobile text entry and special keys, with viewport-driven rows/columns.

Video is H.264, audio Opus. Clipboard never polls in the background. File transfer is a flat shared folder: 100 MiB per file, 500 MiB session budget, single transfer, SHA-256 check, no overwrite/path traversal/symlinks; browser file paste is not native OS file-clipboard transfer. Browser-reserved shortcuts and secure attention sequences may not work. See [capability details](remote-capabilities.md).

## Update, recovery and removal

Keep the agent identity/private keys, chosen permissions and shared-folder contents across updates. Stop only Remvora, verify target/checksum/signature, back up the executable, replace with a compatible binary, restart and verify presence plus actual session. `stage-update`/`install-update` accept a manifest, signature, artifact and trusted key; they do not discover/download updates automatically. Review `--help` and [release/update details](release-and-update.md). Do not invent signing trust or treat unsigned development archives as production installers.

If a service fails, inspect logs and roll back the executable/configuration; do not delete state as a first action. For retirement, revoke/disable server trust, stop/disable the Remvora service and remove only its installed executable/unit after backups. Private identity destruction is a separate irreversible step. OS reinstallation/lost private state generally requires fresh enrollment. Never restore a key onto a second active device.

Troubleshooting: offline → service/WSS/TLS/state; black desktop → login session/display permissions; no audio → local flag/monitor/backend/browser mute; transfer rejected → protected folder/name/size; busy → close previous session; slow → both networks, CPU/encoding and relay capacity. Check SD/storage health and native package integrity if Raspberry builds fail; do not format storage as a routine fix.

## License and release limits

MIT applies to Remvora’s original source, not a blanket relicensing of dependencies. Preserve OpenH264, Opus, xcap, Enigo and other upstream notices. Complete license/SBOM review, native platform tests, long-duration reliability/security review and code-signing/notarization remain public binary release gates. Read [status](STATUS.md), [release checklist](PUBLIC_RELEASE.md), SECURITY and CONTRIBUTING. Hosting/support costs and guaranteed service levels are not included.
