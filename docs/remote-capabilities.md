> Current bilingual overview: [English](GUIDE.en.md) / [Türkçe](GUIDE.tr.md). Platform-specific limits below remain applicable.

# Remote desktop capabilities — 0.3.0

Every capability requires a server-authorized desktop session and explicit local configuration. Example flags: `--allow-desktop --allow-clipboard --allow-audio --file-root /absolute/protected/shared` before `run`. A capability message enables only the available controls in the panel.

## Audio

System playback is encoded as stereo 48 kHz Opus on the existing WebRTC peer. The browser starts muted; use its audio button and volume control. No microphone fallback or recording is implemented. macOS 13+ uses an embedded ScreenCaptureKit helper compiled with Swift/Xcode tools; OS screen/audio capture permission is required. Linux uses `/usr/bin/parec` and the default PulseAudio/PipeWire monitor. Windows uses WASAPI output loopback through CPAL. macOS playback-to-browser energy was verified with a known generated tone. Selected Linux runtime flows have been exercised; full Windows/Linux audio acceptance remains incomplete. Production signing must include the embedded macOS helper; current packages are unsigned development builds.

## Keyboard and clipboard

Native edit/navigation keys, modifiers, F1–F12 and CapsLock are mapped. Blur and disconnect release held keys. Ctrl/Cmd+C/X on the focused remote video explicitly requests copy/cut and reads text only with local clipboard permission. Ctrl/Cmd+V handles a browser paste event and invokes native paste remotely. Buttons provide fallbacks for browser-reserved combinations and clipboard permissions. Clipboard text is bounded to 16 KiB and protocol messages to 20,000 bytes. No background clipboard polling occurs. Browser permission may prevent writing the local clipboard; the panel retains the explicit response for manual copying. Secure OS sequences such as Ctrl+Alt+Delete are not implemented.

## Files

A separate `remvora.files.v1` WebRTC data channel (or the authorized WSS relay file channel) transfers files only when `--file-root` names an existing protected directory. Picker, video drop and browser-provided pasted files upload into that flat shared folder. Files do not enter the native OS file clipboard or the active Explorer/Finder folder. Browser file paste support varies; picker/drop remain available.

Limits: 100 MiB per file, 500 MiB requested transfer budget per session, one transfer at a time, 12 KiB acknowledged chunks, at most 200 displayed files. The browser buffers each bounded file in memory. Both directions verify SHA-256. Upload uses a private temporary file, exact offsets, and atomic publication without overwriting an existing name. Cancel/disconnect removes partial uploads. Paths, symlinks, special files and reserved names are rejected. The shared folder and its parents must be protected from hostile local modification; this is not a sandbox against a privileged local attacker. Contents are not logged.

## Displays

The panel lists native displays and switches capture and input together. `--monitor-id` selects the initial display. A single-display selection was accepted locally; multiple monitors and HiDPI combinations remain unverified.

## Linux graphical service

After enrolling and activating under the intended desktop account, use `python3 scripts/install-linux-user.py --help`. The installer creates a user systemd unit without replacing an existing unit or changing another kiosk service. Supply the actual graphical session environment. `--enable` starts the unit; omit it for inspection first. OS capture/input permissions still apply. A selected Raspberry Pi graphical user service has been installed and exercised. This does not prove all Raspberry OS/compositor combinations.
