use crate::relay::MediaTrack as TrackLocalStaticSample;
// System-output capture only; never falls back to a microphone. Opus / 48 kHz stereo.
use anyhow::Result;
#[cfg(windows)]
use anyhow::ensure;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use webrtc::media::Sample;

fn encoder() -> Result<opus::Encoder> {
    let mut encoder = opus::Encoder::new(48000, opus::Channels::Stereo, opus::Application::Audio)?;
    encoder.set_bitrate(opus::Bitrate::Bits(96000))?;
    Ok(encoder)
}
async fn send(
    track: &TrackLocalStaticSample,
    encoder: &mut opus::Encoder,
    pcm: &[f32],
) -> Result<()> {
    let mut packet = [0; 4000];
    let n = encoder.encode_float(pcm, &mut packet)?;
    tokio::time::timeout(
        Duration::from_secs(2),
        track.write_sample(&Sample {
            data: bytes::Bytes::copy_from_slice(&packet[..n]),
            duration: Duration::from_millis(20),
            ..Default::default()
        }),
    )
    .await??;
    Ok(())
}
pub fn start(track: Arc<TrackLocalStaticSample>, stop: Arc<AtomicBool>, state: std::path::PathBuf) {
    #[cfg(not(target_os = "macos"))]
    let _ = state;
    #[cfg(target_os = "macos")]
    tokio::spawn(async move {
        if let Err(error) = capture_macos(track, stop, state).await {
            tracing::warn!(%error,"System audio unavailable; desktop remains active");
        }
    });
    #[cfg(target_os = "linux")]
    tokio::spawn(async move {
        if capture_linux(track, stop).await.is_err() {
            tracing::warn!("System audio unavailable; desktop remains active");
        }
    });
    #[cfg(windows)]
    {
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            if let Err(error) = capture_native(track, stop, runtime) {
                tracing::warn!(%error,"System audio unavailable; desktop remains active");
            }
        });
    }
}
#[cfg(target_os = "macos")]
async fn capture_macos(
    track: Arc<TrackLocalStaticSample>,
    stop: Arc<AtomicBool>,
    state: std::path::PathBuf,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::Builder::new()
        .prefix("audio-")
        .tempdir_in(state)?;
    let path = directory.path().join("remvora-audio");
    std::fs::write(
        &path,
        include_bytes!(concat!(env!("OUT_DIR"), "/remvora-audio")),
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    let mut child = tokio::process::Command::new(path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("No audio stream"))?;
    pump(&mut output, &track, &stop).await?;
    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
}
#[cfg(target_os = "linux")]
async fn capture_linux(track: Arc<TrackLocalStaticSample>, stop: Arc<AtomicBool>) -> Result<()> {
    // PulseAudio or PipeWire's pulse compatibility daemon; explicit output monitor, no microphone.
    let mut child = tokio::process::Command::new("/usr/bin/parec")
        .args([
            "--device=@DEFAULT_MONITOR@",
            "--format=float32le",
            "--rate=48000",
            "--channels=2",
            "--latency-msec=40",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("No audio stream"))?;
    pump(&mut output, &track, &stop).await?;
    child.kill().await?;
    let _ = child.wait().await;
    Ok(())
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn pump(
    output: &mut tokio::process::ChildStdout,
    track: &TrackLocalStaticSample,
    stop: &AtomicBool,
) -> Result<()> {
    use tokio::io::AsyncReadExt;
    let mut encoder = encoder()?;
    let mut bytes = [0u8; 7680];
    let mut offset = 0;
    while !stop.load(Ordering::Relaxed) {
        match tokio::time::timeout(
            Duration::from_millis(250),
            output.read(&mut bytes[offset..]),
        )
        .await
        {
            Err(_) => continue,
            Ok(Err(error)) => return Err(error.into()),
            Ok(Ok(0)) => anyhow::bail!("System audio capture ended"),
            Ok(Ok(n)) => offset += n,
        }
        if offset == bytes.len() {
            let pcm: Vec<f32> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| {
                    let v = f32::from_le_bytes(*b);
                    if v.is_finite() {
                        v.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
                .collect();
            send(track, &mut encoder, &pcm).await?;
            offset = 0;
        }
    }
    Ok(())
}
#[cfg(windows)]
fn capture_native(
    track: Arc<TrackLocalStaticSample>,
    stop: Arc<AtomicBool>,
    runtime: tokio::runtime::Handle,
) -> Result<()> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No output device"))?;
    let supported = device.default_output_config()?;
    let config = supported.config();
    ensure!(
        config.channels > 0
            && config.channels <= 32
            && (8000..=192000).contains(&config.sample_rate),
        "Unsupported output format"
    );
    tracing::info!(
        rate = config.sample_rate,
        channels = config.channels,
        "Opening system audio loopback"
    );
    let (tx, rx) = std::sync::mpsc::sync_channel(8);
    let mut converter = Converter::new(config.sample_rate, config.channels as usize);
    let failed = Arc::new(AtomicBool::new(false));
    let error_flag = failed.clone();
    let error = move |_error| {
        error_flag.store(true, Ordering::Relaxed);
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &_| {
                converter.push(data, |frame| {
                    let _ = tx.try_send(frame);
                });
            },
            error,
            Some(Duration::from_secs(3)),
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &_| {
                let data: Vec<f32> = data.iter().map(|x| f32::from(*x) / 32768.0).collect();
                converter.push(&data, |frame| {
                    let _ = tx.try_send(frame);
                });
            },
            error,
            Some(Duration::from_secs(3)),
        )?,
        _ => anyhow::bail!("Unsupported output sample format"),
    };
    stream.play()?;
    tracing::info!("System audio loopback started");
    let mut encoder = encoder()?;
    while !stop.load(Ordering::Relaxed) && !failed.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(pcm) => runtime.block_on(send(&track, &mut encoder, &pcm))?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }
    drop(stream);
    Ok(())
}
// Streaming linear interpolation preserves fractional position across native callbacks.
#[cfg(any(windows, test))]
struct Converter {
    rate: u32,
    channels: usize,
    phase: f64,
    previous: [f32; 2],
    started: bool,
    output: Vec<f32>,
}
#[cfg(any(windows, test))]
impl Converter {
    fn new(rate: u32, channels: usize) -> Self {
        Self {
            rate,
            channels,
            phase: 0.0,
            previous: [0.0; 2],
            started: false,
            output: Vec::with_capacity(1920),
        }
    }
    fn push(&mut self, data: &[f32], mut emit: impl FnMut(Vec<f32>)) {
        for frame in data.chunks_exact(self.channels) {
            let current = [frame[0], frame[if self.channels > 1 { 1 } else { 0 }]];
            if !self.started {
                self.previous = current;
                self.started = true;
                continue;
            }
            while self.phase < 1.0 {
                for (old, new) in self.previous.iter().zip(current) {
                    let value = old + (new - old) * self.phase as f32;
                    self.output.push(if value.is_finite() {
                        value.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    });
                }
                if self.output.len() == 1920 {
                    emit(std::mem::replace(
                        &mut self.output,
                        Vec::with_capacity(1920),
                    ));
                }
                self.phase += f64::from(self.rate) / 48000.0;
            }
            self.phase -= 1.0;
            self.previous = current;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resamples_and_encodes_real_opus() {
        let mut c = Converter::new(44100, 1);
        let mut frames = Vec::new();
        let source: Vec<f32> = (0..4411).map(|i| (i as f32 * 0.04).sin() * 0.1).collect();
        for data in source.chunks(127) {
            c.push(data, |f| frames.push(f));
        }
        assert!(frames.len() >= 4);
        let mut e = encoder().unwrap();
        let mut d = opus::Decoder::new(48000, opus::Channels::Stereo).unwrap();
        let mut bytes = [0; 4000];
        let n = e.encode_float(&frames[0], &mut bytes).unwrap();
        let mut output = [0.0; 1920];
        assert_eq!(
            d.decode_float(&bytes[..n], &mut output, false).unwrap(),
            960
        );
        assert!(output.iter().any(|v| v.abs() > 0.001));
    }
}
