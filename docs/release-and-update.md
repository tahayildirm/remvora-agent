# Signed release and update procedure

Protect an offline P-256 private key outside source control. Independently provision the DER SPKI public key to each device. A key downloaded beside a release is not a trust anchor.

Build and test the native artifact, then sign a version newer than the installed agent:

```sh
python3 scripts/sign-release.py --artifact target/release/remvora-agent --key /protected/release-key.pem --version 0.3.0 --os macos --arch aarch64 --output .artifacts/0.3.0
```

`stage-update` validates the signature, semantic version, OS, architecture, copied artifact SHA-256 and bounded metadata without executing the artifact. `install-update` additionally requires an absolute protected target, a helper copied from that exact installed executable and a stopped service. It re-verifies the destination copy, preserves the previous executable and restores it on rename failure. Use the wrapper to coordinate an installed service:

```sh
python3 scripts/update-service.py --manager launchd --server https://remvora.example/ --installed /usr/local/lib/remvora/remvora-agent --manifest /protected/release/manifest.json --signature /protected/release/manifest.sig --artifact /protected/release/remvora-agent --trusted-key /protected/release-public.der
```

Managers are `launchd`, `systemd`, and `windows`, with fixed Remvora service names. Run under the existing service owner/administrator. No privilege elevation, download or trust-key replacement occurs. Verification runs before stopping the service; failed startup restores the previous binary. Keep the printed previous binary path for a controlled rollback. Protect the installation parent and trust key from unprivileged writes. Crash/power-loss recovery and native service paths require platform drills; health currently verifies service-manager running state, not remote connectivity.

Two local tests exercise genuine signed installation and file restoration with a service-controller stand-in. Native Windows/Linux/macOS service update acceptance is still pending. Updates are explicitly operator-triggered, not an unattended download daemon.

## Local distributions

`python3 scripts/package-release.py --version 0.2.0` creates a native archive and SHA-256 file. `python3 scripts/build-macos-pkg.py --version 0.2.0` creates a binary PKG; optional `--application-identity` and `--installer-identity` sign locally. Notarization is not performed. Windows `scripts/build-windows-msi.ps1` uses WiX and optional Authenticode signing. MSI source has not been built on this macOS host. These binary installers do not automatically enroll or configure a service. Follow README service instructions under the correct identity. No artifacts are uploaded by these scripts.
