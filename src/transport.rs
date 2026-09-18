use crate::terminal::{self, Killer};
use anyhow::Result;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use webrtc::{
    api::APIBuilder,
    ice_transport::ice_server::RTCIceServer,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        sdp::session_description::RTCSessionDescription,
    },
};

pub struct DesktopOptions {
    pub clipboard: bool,
    pub monitor_id: Option<u32>,
    pub file_root: Option<PathBuf>,
    pub audio: bool,
    pub state: PathBuf,
}
pub struct RemotePeer {
    connection: Arc<RTCPeerConnection>,
    killer: Killer,
    stop: Arc<AtomicBool>,
}
impl RemotePeer {
    pub async fn answer(
        sdp: &str,
        shell: PathBuf,
        desktop: bool,
        options: DesktopOptions,
        candidates: Option<(
            uuid::Uuid,
            tokio::sync::mpsc::Sender<(uuid::Uuid, serde_json::Value)>,
        )>,
    ) -> Result<(Self, String)> {
        let DesktopOptions {
            clipboard,
            monitor_id,
            file_root,
            audio,
            state: audio_directory,
        } = options;
        let stun = std::env::var("REMVORA_STUN_URL")
            .ok()
            .filter(|x| x.starts_with("stun:") || x.starts_with("stuns:"));
        let configuration = RTCConfiguration {
            ice_servers: stun
                .map(|url| {
                    vec![RTCIceServer {
                        urls: vec![url],
                        ..Default::default()
                    }]
                })
                .unwrap_or_default(),
            ..Default::default()
        };
        let mut media = webrtc::api::media_engine::MediaEngine::default();
        media.register_default_codecs()?;
        let registry = webrtc::api::interceptor_registry::register_default_interceptors(
            webrtc::interceptor::registry::Registry::new(),
            &mut media,
        )?;
        let connection = Arc::new(
            APIBuilder::new()
                .with_media_engine(media)
                .with_interceptor_registry(registry)
                .build()
                .new_peer_connection(configuration)
                .await?,
        );
        if let Some((session, output)) = candidates {
            connection.on_ice_candidate(Box::new(move |candidate| {
                let output = output.clone();
                Box::pin(async move {
                    let value = match candidate {
                        Some(candidate) => candidate
                            .to_json()
                            .ok()
                            .and_then(|c| serde_json::to_value(c).ok()),
                        None => Some(serde_json::json!({"candidate":""})),
                    };
                    if let Some(value) = value {
                        let _ = output.try_send((session, value));
                    }
                })
            }));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let quality = Arc::new(std::sync::Mutex::new(
            crate::desktop::VideoSettings::default(),
        ));
        let selection = Arc::new(AtomicU32::new(monitor_id.unwrap_or(u32::MAX)));
        if desktop {
            let track = Arc::new(
                webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample::new(
                    webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability {
                        mime_type: "video/H264".into(),
                        clock_rate: 90000,
                        sdp_fmtp_line:
                            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                                .into(),
                        ..Default::default()
                    },
                    "screen".into(),
                    "remvora".into(),
                ),
            );
            let sender = connection.add_track(track.clone()).await?;
            let keyframe = Arc::new(AtomicBool::new(true));
            let requested = keyframe.clone();
            tokio::spawn(async move {
                while let Ok((packets, _)) = sender.read_rtcp().await {
                    if packets.iter().any(|packet| {
                        packet.as_any().is::<webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication>()
                            || packet.as_any().is::<webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest>()
                    }) {
                        requested.store(true, Ordering::Relaxed);
                    }
                }
            });
            let audio_track = if audio {
                let track=Arc::new(webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample::new(
                    webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability {mime_type:"audio/opus".into(),clock_rate:48000,channels:2,sdp_fmtp_line:"minptime=10;useinbandfec=1".into(),..Default::default()},"system-audio".into(),"remvora".into()));
                let sender = connection.add_track(track.clone()).await?;
                tokio::spawn(async move {
                    let mut buffer = [0u8; 1500];
                    while sender.read(&mut buffer).await.is_ok() {}
                });
                Some(track)
            } else {
                None
            };
            let capture_quality = quality.clone();
            let capture_selection = selection.clone();
            let state_stop = stop.clone();
            let started = Arc::new(AtomicBool::new(false));
            connection.on_peer_connection_state_change(Box::new(move |state| {
                let quality = capture_quality.clone();
                let keyframe = keyframe.clone();
                let audio_state = audio_directory.clone();
                let selection = capture_selection.clone();
                let audio_track = audio_track.clone();
                let stop = state_stop.clone();
                let track = track.clone();
                let started = started.clone();
                Box::pin(async move {
                    use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
                    if state == RTCPeerConnectionState::Connected
                        && !started.swap(true, Ordering::SeqCst)
                    {
                        crate::desktop::capture(
                            Arc::new(crate::relay::MediaTrack::Rtc(track)),
                            stop.clone(),
                            selection,
                            keyframe,
                            quality,
                        );
                        if let Some(track) = audio_track {
                            crate::audio::start(
                                Arc::new(crate::relay::MediaTrack::Rtc(track)),
                                stop.clone(),
                                audio_state,
                            );
                        }
                    } else if matches!(
                        state,
                        RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
                    ) {
                        stop.store(true, Ordering::Relaxed);
                    }
                })
            }));
        }
        let input_stop = stop.clone();
        let killer: Killer = Arc::new(Mutex::new(None));
        let terminal_killer = killer.clone();
        let files_used = Arc::new(AtomicBool::new(false));
        let used = Arc::new(std::sync::atomic::AtomicBool::new(false));
        connection.on_data_channel(Box::new(move |channel| {
            let quality = quality.clone();
            let selection = selection.clone();
            let file_root = file_root.clone();
            let files_used = files_used.clone();
            let shell = shell.clone();
            let killer = terminal_killer.clone();
            let used = used.clone();
            let stop = input_stop.clone();
            Box::pin(async move {
                if desktop && channel.label() == "remvora.files.v1" {
                    if let Some(root) = file_root
                        && !files_used.swap(true, Ordering::SeqCst)
                    {
                        crate::files::attach(crate::relay::Channel::rtc(channel), root, stop);
                    } else {
                        let _ = channel.close().await;
                    }
                    return;
                }
                if channel.label()
                    != if desktop {
                        "remvora.input.v1"
                    } else {
                        "remvora.terminal.v1"
                    }
                    || used.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    let _ = channel.close().await;
                    return;
                }
                if desktop {
                    let opened=channel.clone();let files=file_root.is_some();
                    channel.on_open(Box::new(move || Box::pin(async move {let _=opened.send_text(serde_json::json!({"type":"capabilities","audio":audio,"clipboard":clipboard,"files":files,"videoSettings":true}).to_string()).await;})));
                    crate::desktop::input(crate::relay::Channel::rtc(channel), stop, clipboard, selection, quality);
                    return;
                }
                let open = channel.clone();
                let close_killer = killer.clone();
                channel.on_close(Box::new(move || {
                    let killer = close_killer.clone();
                    Box::pin(async move {
                        if let Some(mut child) = killer.lock().unwrap().take() {
                            let _ = child.kill();
                        }
                    })
                }));
                channel.on_open(Box::new(move || {
                    Box::pin(async move {
                        if terminal::start(crate::relay::Channel::rtc(open.clone()), shell, killer).is_err() {
                            let _ = open.close().await;
                        }
                    })
                }));
            })
        }));
        // A slow/unreachable STUN server must not tear down the authenticated
        // control socket. Use candidates gathered so far after a bounded wait.
        let negotiation = async {
            connection
                .set_remote_description(RTCSessionDescription::offer(sdp.to_owned())?)
                .await?;
            let answer = connection.create_answer(None).await?;
            let mut gathering = connection.gathering_complete_promise().await;
            connection.set_local_description(answer).await?;
            if tokio::time::timeout(Duration::from_secs(6), gathering.recv())
                .await
                .is_err()
            {
                tracing::warn!(
                    "ICE gathering deadline reached; answering with available candidates"
                );
            }
            connection
                .local_description()
                .await
                .ok_or_else(|| anyhow::anyhow!("Missing local SDP"))
        };
        let description = match tokio::time::timeout(Duration::from_secs(10), negotiation).await {
            Ok(Ok(description)) => description,
            result => {
                stop.store(true, Ordering::Relaxed);
                let _ = connection.close().await;
                return Err(match result {
                    Ok(Err(error)) => error,
                    Err(_) => anyhow::anyhow!("ICE negotiation deadline reached"),
                    Ok(Ok(_)) => unreachable!(),
                });
            }
        };
        let watched = Arc::downgrade(&connection);
        let watched_stop = stop.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let Some(peer) = watched.upgrade() else {
                    break;
                };
                if watched_stop.load(Ordering::Relaxed) {
                    let _ = peer.close().await;
                    break;
                }
            }
        });
        Ok((
            Self {
                connection,
                killer,
                stop,
            },
            description.sdp,
        ))
    }
    pub async fn add_candidate(&self, value: serde_json::Value) -> Result<()> {
        let candidate = serde_json::from_value(value)?;
        self.connection.add_ice_candidate(candidate).await?;
        Ok(())
    }
    pub async fn close(self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.killer.lock().unwrap().take() {
            let _ = child.kill();
        }
        let _ = self.connection.close().await;
    }
}
impl Drop for RemotePeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.killer.lock().unwrap().take() {
            let _ = child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn real_webrtc_data_channel_reaches_pty() {
        verify_pty(false).await;
    }
    #[tokio::test]
    async fn late_trickle_candidates_connect_without_sdp_candidates() {
        verify_pty(true).await;
    }
    async fn verify_pty(trickle: bool) {
        let client = Arc::new(
            APIBuilder::new()
                .build()
                .new_peer_connection(RTCConfiguration::default())
                .await
                .unwrap(),
        );
        let channel = client
            .create_data_channel("remvora.terminal.v1", None)
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        channel.on_message(Box::new(move |message| {
            let tx = tx.clone();
            Box::pin(async move {
                let _ = tx.send(message.data).await;
            })
        }));
        let input = channel.clone();
        channel.on_open(Box::new(move || {
            Box::pin(async move {
                if cfg!(windows) {
                    return;
                }
                let _ = input
                    .send(&bytes::Bytes::from_static(if cfg!(windows) {
                        b"Write-Output ('REMVORA_PTY_' + 'VERIFIED')\r"
                    } else {
                        b"printf 'REMVORA_PTY_%s\\n' VERIFIED\n"
                    }))
                    .await;
            })
        }));
        let mut gathering = client.gathering_complete_promise().await;
        client
            .set_local_description(client.create_offer(None).await.unwrap())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(15), gathering.recv())
            .await
            .unwrap();
        let offer = client.local_description().await.unwrap();
        let shell = if cfg!(windows) {
            PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
        } else {
            PathBuf::from("/bin/sh")
        };
        let without_candidates = |sdp: &str| {
            sdp.lines()
                .filter(|line| {
                    !line.starts_with("a=candidate:") && !line.starts_with("a=end-of-candidates")
                })
                .map(|line| format!("{line}\r\n"))
                .collect::<String>()
        };
        let (candidate_tx, mut candidate_rx) = tokio::sync::mpsc::channel(64);
        let (agent, answer) = RemotePeer::answer(
            &if trickle {
                without_candidates(&offer.sdp)
            } else {
                offer.sdp.clone()
            },
            shell,
            false,
            DesktopOptions {
                clipboard: false,
                monitor_id: None,
                file_root: None,
                audio: false,
                state: PathBuf::new(),
            },
            if trickle {
                Some((uuid::Uuid::new_v4(), candidate_tx))
            } else {
                None
            },
        )
        .await
        .unwrap();
        client
            .set_remote_description(
                RTCSessionDescription::answer(if trickle {
                    without_candidates(&answer)
                } else {
                    answer
                })
                .unwrap(),
            )
            .await
            .unwrap();
        if trickle {
            for candidate in offer
                .sdp
                .lines()
                .filter_map(|line| line.strip_prefix("a=candidate:"))
            {
                agent.add_candidate(serde_json::json!({"candidate":format!("candidate:{candidate}"),"sdpMid":"0","sdpMLineIndex":0})).await.unwrap();
            }
            let mut count = 0;
            while let Ok((_, candidate)) = candidate_rx.try_recv() {
                client
                    .add_ice_candidate(serde_json::from_value(candidate).unwrap())
                    .await
                    .unwrap();
                count += 1;
            }
            assert!(count > 0, "agent must emit trickled candidates");
        }
        let mut output = String::new();
        let output_result = tokio::time::timeout(Duration::from_secs(20), async {
            let mut cursor_requests = 0;
            let mut command_sent = !cfg!(windows);
            while let Some(bytes) = rx.recv().await {
                output.push_str(&String::from_utf8_lossy(&bytes));
                // Emulate the cursor-position reply sent by browser terminal emulators.
                let requests = output.matches("\x1b[6n").count();
                while cursor_requests < requests {
                    channel
                        .send(&bytes::Bytes::from_static(b"\x1b[1;1R"))
                        .await
                        .unwrap();
                    cursor_requests += 1;
                }
                if !command_sent && output.contains("> ") {
                    channel
                        .send(&bytes::Bytes::from(crate::terminal::test_input(
                            &output,
                            "Write-Output ('REMVORA_PTY_' + 'VERIFIED')\r",
                        )))
                        .await
                        .unwrap();
                    command_sent = true;
                }
                if output.contains("REMVORA_PTY_VERIFIED") {
                    return;
                }
            }
        })
        .await;
        assert!(
            output_result.is_ok(),
            "Terminal timed out; output: {output:?}"
        );
        assert!(output.contains("REMVORA_PTY_VERIFIED"));
        agent.close().await;
        client.close().await.unwrap();
    }
}
