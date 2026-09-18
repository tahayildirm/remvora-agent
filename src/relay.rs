//! Bounded WSS fallback. Reuses the same capability-gated desktop, PTY and file handlers.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use bytes::Bytes;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;
use uuid::Uuid;
use webrtc::{
    data_channel::{OnMessageHdlrFn, RTCDataChannel, data_channel_message::DataChannelMessage},
    media::Sample,
    track::track_local::track_local_static_sample::TrackLocalStaticSample,
};

pub type Output = mpsc::Sender<(Uuid, Value)>;
#[derive(Clone)]
pub struct Writer {
    id: Uuid,
    output: Output,
    stop: Arc<AtomicBool>,
}
impl Writer {
    async fn send(&self, channel: &str, data: &[u8], text: bool) -> Result<usize> {
        ensure!(data.len() <= 2 * 1024 * 1024, "Relay frame too large");
        for (part, chunk) in data.chunks(32768).enumerate() {
            ensure!(!self.stop.load(Ordering::Relaxed), "Relay closed");
            let payload = json!({"channel":channel,"data":STANDARD.encode(chunk),"text":text,"part":part,"last":(part+1)*32768>=data.len()});
            tokio::time::timeout(Duration::from_secs(2), self.output.send((self.id, payload)))
                .await??;
        }
        Ok(data.len())
    }
}
pub struct Channel {
    rtc: Option<Arc<RTCDataChannel>>,
    relay: Option<(Writer, String)>,
    message: Mutex<Option<OnMessageHdlrFn>>,
}
impl Channel {
    pub fn rtc(rtc: Arc<RTCDataChannel>) -> Arc<Self> {
        Arc::new(Self {
            rtc: Some(rtc),
            relay: None,
            message: Mutex::new(None),
        })
    }
    fn relay(writer: Writer, name: &str) -> Arc<Self> {
        Arc::new(Self {
            rtc: None,
            relay: Some((writer, name.into())),
            message: Mutex::new(None),
        })
    }
    pub fn on_message(&self, handler: OnMessageHdlrFn) {
        if let Some(rtc) = &self.rtc {
            rtc.on_message(handler);
        } else {
            *self.message.lock().unwrap() = Some(handler);
        }
    }
    pub async fn receive(&self, message: DataChannelMessage) {
        let future = self
            .message
            .lock()
            .unwrap()
            .as_mut()
            .map(|handler| handler(message));
        if let Some(future) = future {
            future.await;
        }
    }
    pub async fn send(&self, data: &Bytes) -> Result<usize> {
        if let Some(rtc) = &self.rtc {
            Ok(rtc.send(data).await?)
        } else {
            let (w, n) = self.relay.as_ref().unwrap();
            w.send(n, data, false).await
        }
    }
    pub async fn send_text(&self, data: impl Into<String>) -> Result<usize> {
        let data = data.into();
        if let Some(rtc) = &self.rtc {
            Ok(rtc.send_text(data).await?)
        } else {
            let (w, n) = self.relay.as_ref().unwrap();
            w.send(n, data.as_bytes(), true).await
        }
    }
    pub async fn buffered_amount(&self) -> usize {
        if let Some(rtc) = &self.rtc {
            rtc.buffered_amount().await
        } else {
            0
        }
    }
    pub async fn close(&self) -> Result<()> {
        if let Some(rtc) = &self.rtc {
            rtc.close().await?;
        } else if let Some((w, _)) = &self.relay {
            w.stop.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}
pub enum MediaTrack {
    Rtc(Arc<TrackLocalStaticSample>),
    Relay(Writer, &'static str),
}
impl MediaTrack {
    pub async fn write_sample(&self, sample: &Sample) -> Result<()> {
        match self {
            Self::Rtc(track) => track.write_sample(sample).await?,
            Self::Relay(w, n) => {
                w.send(n, &sample.data, false).await?;
            }
        }
        Ok(())
    }
}
pub struct RelayPeer {
    stop: Arc<AtomicBool>,
    killer: crate::terminal::Killer,
    input: Arc<Channel>,
    files: Option<Arc<Channel>>,
}
impl RelayPeer {
    pub async fn start(
        id: Uuid,
        output: Output,
        shell: PathBuf,
        desktop: bool,
        options: crate::transport::DesktopOptions,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let killer = Arc::new(Mutex::new(None));
        let writer = Writer {
            id,
            output,
            stop: stop.clone(),
        };
        let input = Channel::relay(writer.clone(), "input");
        let files = if desktop {
            options
                .file_root
                .as_ref()
                .map(|_| Channel::relay(writer.clone(), "files"))
        } else {
            None
        };
        let peer = Self {
            stop: stop.clone(),
            killer: killer.clone(),
            input: input.clone(),
            files: files.clone(),
        };
        if desktop {
            let selection = Arc::new(AtomicU32::new(options.monitor_id.unwrap_or(u32::MAX)));
            let quality = Arc::new(Mutex::new(crate::desktop::VideoSettings::default()));
            crate::desktop::input(
                input.clone(),
                stop.clone(),
                options.clipboard,
                selection.clone(),
                quality.clone(),
            );
            if let (Some(channel), Some(root)) = (files, options.file_root) {
                crate::files::attach(channel, root, stop.clone());
            }
            input.send_text(json!({"type":"capabilities","audio":options.audio,"clipboard":options.clipboard,"files":peer.files.is_some(),"videoSettings":true}).to_string()).await?;
            crate::desktop::capture(
                Arc::new(MediaTrack::Relay(writer.clone(), "video")),
                stop.clone(),
                selection,
                Arc::new(AtomicBool::new(true)),
                quality,
            );
            if options.audio {
                crate::audio::start(
                    Arc::new(MediaTrack::Relay(writer, "audio")),
                    stop,
                    options.state,
                );
            }
        } else {
            crate::terminal::start(input, shell, killer)?;
        }
        Ok(peer)
    }
    pub fn finished(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
    pub async fn receive(&self, payload: &Value) -> Result<()> {
        ensure!(
            payload["part"] == 0 && payload["last"] == true,
            "Fragmented control input"
        );
        let encoded = payload["data"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing relay data"))?;
        ensure!(encoded.len() <= 28000, "Control input too large");
        let data = STANDARD.decode(encoded)?;
        let channel = match payload["channel"].as_str() {
            Some("input") => &self.input,
            Some("files") => self
                .files
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Files denied"))?,
            _ => anyhow::bail!("Unknown relay channel"),
        };
        channel
            .receive(DataChannelMessage {
                is_string: payload["text"] == true,
                data: Bytes::from(data),
            })
            .await;
        Ok(())
    }
}
impl Drop for RelayPeer {
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
    async fn relay_reaches_real_terminal_and_closes() {
        let (tx, mut rx) = mpsc::channel(32);
        let id = Uuid::new_v4();
        let shell = PathBuf::from(if cfg!(windows) {
            r"C:\Windows\System32\cmd.exe"
        } else {
            "/bin/sh"
        });
        let peer = RelayPeer::start(
            id,
            tx,
            shell,
            false,
            crate::transport::DesktopOptions {
                clipboard: false,
                monitor_id: None,
                file_root: None,
                audio: false,
                state: PathBuf::new(),
            },
        )
        .await
        .unwrap();
        let command = if cfg!(windows) {
            "echo RELAY_^OK\r"
        } else {
            "printf 'RELAY_%s\\n' OK\n"
        };
        if !cfg!(windows) {
            peer.receive(&json!({"channel":"input","data":STANDARD.encode(command),"text":false,"part":0,"last":true})).await.unwrap();
        }
        let mut output = String::new();
        let output_result = tokio::time::timeout(Duration::from_secs(20), async {
            let mut cursor_requests = 0;
            let mut command_sent = !cfg!(windows);
            while let Some((session, packet)) = rx.recv().await {
                assert_eq!(session, id);
                output.push_str(&String::from_utf8_lossy(
                    &STANDARD.decode(packet["data"].as_str().unwrap()).unwrap(),
                ));
                // Emulate the cursor-position reply sent by browser terminal emulators.
                let requests = output.matches("\x1b[6n").count();
                while cursor_requests < requests {
                    peer.receive(&json!({"channel":"input","data":STANDARD.encode("\x1b[1;1R"),"text":false,"part":0,"last":true})).await.unwrap();
                    cursor_requests += 1;
                }
                if !command_sent && output.contains('>') {
                    peer.receive(&json!({"channel":"input","data":STANDARD.encode(crate::terminal::test_input(&output, command)),"text":false,"part":0,"last":true})).await.unwrap();
                    command_sent = true;
                }
                if output.contains("RELAY_OK") {
                    break;
                }
            }
        })
        .await;
        assert!(
            output_result.is_ok(),
            "Terminal timed out; output: {output:?}"
        );
        assert!(output.contains("RELAY_OK"));
        assert!(
            peer.receive(
                &json!({"channel":"files","data":"e30=","part":0,"last":true,"text":true})
            )
            .await
            .is_err()
        );
        let stop = peer.stop.clone();
        drop(peer);
        assert!(stop.load(Ordering::Relaxed));
    }
    #[tokio::test]
    async fn media_chunks_are_bounded_and_reassemble() {
        let (tx, mut rx) = mpsc::channel(32);
        let id = Uuid::new_v4();
        let writer = Writer {
            id,
            output: tx,
            stop: Arc::new(AtomicBool::new(false)),
        };
        let source = vec![42u8; 100000];
        writer.send("video", &source, false).await.unwrap();
        drop(writer);
        let mut output = Vec::new();
        let mut part = 0;
        while let Some((_, v)) = rx.recv().await {
            assert_eq!(v["part"], part);
            assert!(v["data"].as_str().unwrap().len() <= 44000);
            output.extend(STANDARD.decode(v["data"].as_str().unwrap()).unwrap());
            part += 1;
        }
        assert_eq!(source, output);
    }
}
