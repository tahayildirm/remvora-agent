# Agent protocol v1

WSS `/ws/agent`: first message within ten seconds is `agent.authenticate` with a P-256 proof. Fields are `protocolVersion`, UUID `messageId`, ISO8601 `timestamp`, nullable UUID `sessionId`, `type`, and object `payload`. Bound challenge text is UTF-8 `remvora:v1:{activate|connect}:{deviceUuid}:{nonce}`. Public key is base64 DER SPKI; signature is base64 64-byte ECDSA P1363, SHA-256.

`agent.connected` confirms identity. Send `agent.heartbeat` every twenty seconds. `session.request` grants one `Terminal` or `Desktop` session; local capability opt-in is mandatory. Send accept/reject. Browser offers and agent answers contain complete gathered SDP. No TURN/relay. Signaling loss terminates peers.

Terminal channel `remvora.terminal.v1`: binary PTY bytes, input at most 16 KiB, output chunks 8 KiB, text `{type:"resize",cols,rows}`. Desktop input channel `remvora.input.v1`: text JSON at most 20,000 bytes, controls `move`, `down`, `up`, `keyDown`, `keyUp`, `scroll`. Coordinates are normalized. Unknown named keys/controls are ignored; no execution is triggered by unknown input. Video uses H.264.

The server owns authorization and a one-hour session maximum. Agent identity never uses a device ID as a password. Reconnect always obtains and signs a fresh one-use challenge.

## Optional explicit clipboard text access

The authorized desktop input channel accepts `{"type":"clipboard","text":"..."}` only with the local `--allow-clipboard` capability. UTF-8 text is at most 16 KiB and the JSON frame at most 20,000 bytes. No clipboard polling occurs. An explicit `{"type":"clipboard.read"}` request returns `{"type":"clipboard.text","text":"..."}` only with the same local opt-in; denied/unavailable reads return a clipboard.result code. Responses are also capped at 20,000 JSON bytes. The agent acknowledges with `{"type":"clipboard.result","code":"clipboardApplied"}` or `clipboardDenied`. Clipboard data travels only inside the encrypted peer channel and is never placed in audit or signaling logs.

## Reboot

Authenticated agent signaling accepts `device.reboot` with a UUID `commandId` and `expiresAt`. The server binds the pending command to device/organization for 30 seconds. Agent requires local `--allow-reboot`, validates expiry and duplicate IDs, and invokes a fixed OS command. `device.reboot.result` carries the same commandId and accepted/denied/failed status; accepted means scheduled/requested, not proof the machine restarted. No arbitrary command arguments are accepted.
