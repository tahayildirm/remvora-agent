use crate::relay::{Channel as RTCDataChannel, MediaTrack as TrackLocalStaticSample};
use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use openh264::{
    encoder::{BitRate, Encoder, EncoderConfig, FrameRate, RateControlMode},
    formats::{RgbaSliceU8, YUVBuffer},
};
use serde::Deserialize;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;
use webrtc::media::Sample;

/// Platform capture implementations are contained in xcap; transport only sees H.264 samples.
pub trait ScreenCaptureProvider {
    fn frame(&self) -> Result<xcap::image::RgbaImage>;
}
struct NativeCapture {
    monitor: xcap::Monitor,
    #[cfg(target_os = "linux")]
    wayland: Option<libwayshot_xcap::WayshotConnection>,
}
impl NativeCapture {
    fn new(monitor: xcap::Monitor) -> Self {
        #[cfg(target_os = "linux")]
        let wayland = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            // Probe once. Supported wlroots compositors avoid a failed portal request
            // on every frame; other desktops retain xcap's existing capture path.
            libwayshot_xcap::WayshotConnection::new()
                .ok()
                .filter(|connection| fast_wayland_frame(connection, &monitor).is_ok())
        } else {
            None
        };
        Self {
            monitor,
            #[cfg(target_os = "linux")]
            wayland,
        }
    }
}
#[cfg(target_os = "linux")]
fn fast_wayland_frame(
    connection: &libwayshot_xcap::WayshotConnection,
    monitor: &xcap::Monitor,
) -> Result<xcap::image::RgbaImage> {
    use libwayshot_xcap::region::{LogicalRegion, Position, Region, Size};
    let region = LogicalRegion {
        inner: Region {
            position: Position {
                x: monitor.x()?,
                y: monitor.y()?,
            },
            size: Size {
                width: monitor.width()?,
                height: monitor.height()?,
            },
        },
    };
    let frame = connection.screenshot(region, false)?.to_rgba8();
    xcap::image::RgbaImage::from_raw(frame.width(), frame.height(), frame.into_raw())
        .ok_or_else(|| anyhow::anyhow!("Invalid Wayland frame"))
}
impl ScreenCaptureProvider for NativeCapture {
    fn frame(&self) -> Result<xcap::image::RgbaImage> {
        #[cfg(target_os = "linux")]
        if let Some(connection) = &self.wayland {
            return fast_wayland_frame(connection, &self.monitor);
        }
        Ok(self.monitor.capture_image()?)
    }
}
/// Selection fails closed if a configured display is disconnected; it never silently changes target.
fn select_monitor(id: Option<u32>) -> Result<xcap::Monitor> {
    xcap::Monitor::all()?
        .into_iter()
        .find(|monitor| match id {
            Some(id) => monitor.id().ok() == Some(id),
            None => monitor.is_primary().unwrap_or(false),
        })
        .ok_or_else(|| anyhow::anyhow!("Selected display unavailable"))
}
pub fn displays() -> Result<Vec<serde_json::Value>> {
    xcap::Monitor::all()?.iter().map(|monitor| Ok(serde_json::json!({
        "id":monitor.id()?, "name":monitor.name()?, "x":monitor.x()?, "y":monitor.y()?,
        "width":monitor.width()?, "height":monitor.height()?, "primary":monitor.is_primary()?
    }))).collect()
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct VideoSettings {
    pub fps: u32,
    pub bitrate: u32,
    pub width: u32,
}
impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            fps: 15,
            bitrate: 1_200_000,
            width: 1280,
        }
    }
}
impl VideoSettings {
    fn validated(fps: u32, bitrate: u32, width: u32) -> Result<Self> {
        ensure!(
            (3..=30).contains(&fps)
                && (150_000..=8_000_000).contains(&bitrate)
                && (640..=1920).contains(&width),
            "Invalid video settings"
        );
        Ok(Self {
            fps,
            bitrate,
            width: width / 2 * 2,
        })
    }
}
pub type VideoConfig = Arc<Mutex<VideoSettings>>;
fn video_encoder(settings: VideoSettings) -> Result<Encoder> {
    Ok(Encoder::with_api_config(
        openh264::OpenH264API::from_source(),
        EncoderConfig::new()
            .bitrate(BitRate::from_bps(settings.bitrate))
            .max_frame_rate(FrameRate::from_hz(settings.fps as f32))
            .rate_control_mode(RateControlMode::Bitrate)
            .skip_frames(true),
    )?)
}
#[derive(Deserialize)]
pub struct Input {
    #[serde(rename = "type")]
    kind: String,
    x: Option<f64>,
    y: Option<f64>,
    button: Option<u8>,
    key: Option<String>,
    delta: Option<i32>,
    text: Option<String>,
    monitor_id: Option<u32>,
    fps: Option<u32>,
    bitrate: Option<u32>,
    width: Option<u32>,
    nonce: Option<u64>,
}
pub trait InputProvider {
    fn apply(&mut self, input: Input) -> Result<()>;
}
struct NativeInput {
    enigo: Enigo,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    pressed_buttons: [bool; 3],
    clipboard: Option<arboard::Clipboard>,
    pressed_keys: Vec<Key>,
}
impl NativeInput {
    fn clipboard_shortcut(&mut self, letter: char) -> Result<()> {
        for key in self.pressed_keys.drain(..) {
            self.enigo.key(key, Direction::Release)?;
        }
        let modifier = if cfg!(target_os = "macos") {
            Key::Meta
        } else {
            Key::Control
        };
        self.enigo.key(modifier, Direction::Press)?;
        std::thread::sleep(Duration::from_millis(20));
        let result = self.enigo.key(Key::Unicode(letter), Direction::Click);
        let release = self.enigo.key(modifier, Direction::Release);
        result?;
        release?;
        Ok(())
    }
}
impl InputProvider for NativeInput {
    fn apply(&mut self, input: Input) -> Result<()> {
        match input.kind.as_str() {
            "move" => {
                let x = input.x.unwrap_or(0.0);
                let y = input.y.unwrap_or(0.0);
                ensure!(x.is_finite() && y.is_finite(), "Invalid coordinates");
                self.enigo.move_mouse(
                    self.x + (x.clamp(0.0, 1.0) * f64::from(self.width.saturating_sub(1))) as i32,
                    self.y + (y.clamp(0.0, 1.0) * f64::from(self.height.saturating_sub(1))) as i32,
                    Coordinate::Abs,
                )?;
                // macOS posts pointer events asynchronously. Enigo's subsequent button
                // event reads the OS cursor position, so let the move reach that queue first.
                #[cfg(target_os = "macos")]
                std::thread::sleep(Duration::from_millis(20));
            }
            "down" | "up" => {
                let button = match input.button {
                    Some(0) => Button::Left,
                    Some(1) => Button::Middle,
                    Some(2) => Button::Right,
                    _ => return Ok(()),
                };
                self.pressed_buttons[usize::from(input.button.unwrap())] = input.kind == "down";
                self.enigo.button(
                    button,
                    if input.kind == "down" {
                        Direction::Press
                    } else {
                        Direction::Release
                    },
                )?;
            }
            "scroll" => self
                .enigo
                .scroll(input.delta.unwrap_or(0).clamp(-20, 20), Axis::Vertical)?,
            "releaseAll" => {
                for key in self.pressed_keys.drain(..) {
                    self.enigo.key(key, Direction::Release)?;
                }
                for (i, button) in [Button::Left, Button::Middle, Button::Right]
                    .iter()
                    .enumerate()
                {
                    if self.pressed_buttons[i] {
                        self.enigo.button(*button, Direction::Release)?;
                        self.pressed_buttons[i] = false;
                    }
                }
            }
            "clipboard" | "clipboard.paste" => {
                let text = input.text.as_deref().unwrap_or("");
                ensure!(text.len() <= 16384, "Clipboard text exceeds limit");
                if let Some(clipboard) = &mut self.clipboard {
                    clipboard.set_text(text)?;
                    if input.kind == "clipboard.paste" {
                        std::thread::sleep(Duration::from_millis(30));
                        self.clipboard_shortcut('v')?;
                    }
                }
            }
            "keyDown" | "keyUp" => {
                if let Some(key) = input.key.as_deref().and_then(key) {
                    if input.kind == "keyDown" {
                        if !self.pressed_keys.contains(&key) {
                            ensure!(self.pressed_keys.len() < 256, "Too many pressed keys");
                            self.pressed_keys.push(key);
                        }
                    } else {
                        self.pressed_keys.retain(|pressed| *pressed != key);
                    }
                    self.enigo.key(
                        key,
                        if input.kind == "keyDown" {
                            Direction::Press
                        } else {
                            Direction::Release
                        },
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
fn key(value: &str) -> Option<Key> {
    Some(match value {
        "Enter" => Key::Return,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "Shift" => Key::Shift,
        "CapsLock" => Key::CapsLock,
        "Control" => Key::Control,
        "Alt" => Key::Alt,
        "Meta" => Key::Meta,
        "ArrowUp" => Key::UpArrow,
        "ArrowDown" => Key::DownArrow,
        "ArrowLeft" => Key::LeftArrow,
        "ArrowRight" => Key::RightArrow,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,

        _ => {
            let mut chars = value.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Key::Unicode(ch)
        }
    })
}

impl Drop for NativeInput {
    fn drop(&mut self) {
        for key in self.pressed_keys.drain(..) {
            let _ = self.enigo.key(key, Direction::Release);
        }
        for (index, button) in [Button::Left, Button::Middle, Button::Right]
            .into_iter()
            .enumerate()
        {
            if self.pressed_buttons[index] {
                let _ = self.enigo.button(button, Direction::Release);
            }
        }
    }
}

pub fn input(
    channel: Arc<RTCDataChannel>,
    stop: Arc<AtomicBool>,
    clipboard: bool,
    selection: Arc<AtomicU32>,
    quality: VideoConfig,
) {
    let (tx, mut rx) = mpsc::channel::<Input>(64);
    let overflow_stop = stop.clone();
    channel.on_message(Box::new(move |message| {
        let tx = tx.clone();
        let overflow_stop = overflow_stop.clone();
        Box::pin(async move {
            if message.is_string
                && message.data.len() <= 20000
                && let Ok(input) = serde_json::from_slice(&message.data)
                && tx.try_send(input).is_err()
            {
                overflow_stop.store(true, Ordering::Relaxed);
            }
        })
    }));
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let result: Result<()> = (|| {
            let monitor = select_monitor(selected(&selection))?;
            let mut input = NativeInput {
                enigo: Enigo::new(&Settings::default())?,
                x: monitor.x()?,
                y: monitor.y()?,
                width: monitor.width()?,
                height: monitor.height()?,
                pressed_buttons: [false; 3],
                pressed_keys: Vec::new(),
                clipboard: if clipboard {
                    Some(arboard::Clipboard::new()?)
                } else {
                    None
                },
            };
            while !stop.load(Ordering::Relaxed) {
                match rx.try_recv() {
                    Ok(command) => {
                        if command.kind == "video.configure" {
                            if let (Some(fps), Some(bitrate), Some(width)) =
                                (command.fps, command.bitrate, command.width)
                                && let Ok(settings) = VideoSettings::validated(fps, bitrate, width)
                            {
                                *quality.lock().unwrap() = settings;
                                runtime.block_on(channel.send_text(serde_json::json!({"type":"video.applied","settings":settings}).to_string()))?;
                            }
                            continue;
                        }
                        if command.kind == "video.ping" {
                            runtime.block_on(
                                channel.send_text(
                                    serde_json::json!({"type":"video.pong","nonce":command.nonce})
                                        .to_string(),
                                ),
                            )?;
                            continue;
                        }
                        if command.kind == "display.list" {
                            let response = serde_json::json!({"type":"display.list","displays":displays()?,"selected":selected(&selection)});
                            runtime.block_on(channel.send_text(response.to_string()))?;
                            continue;
                        }
                        if command.kind == "display.select" {
                            let monitor = select_monitor(command.monitor_id)?;
                            for key in input.pressed_keys.drain(..) {
                                input.enigo.key(key, Direction::Release)?;
                            }
                            for (i, button) in [Button::Left, Button::Middle, Button::Right]
                                .iter()
                                .enumerate()
                            {
                                if input.pressed_buttons[i] {
                                    input.enigo.button(*button, Direction::Release)?;
                                    input.pressed_buttons[i] = false;
                                }
                            }
                            input.x = monitor.x()?;
                            input.y = monitor.y()?;
                            input.width = monitor.width()?;
                            input.height = monitor.height()?;
                            selection.store(monitor.id()?, Ordering::SeqCst);
                            runtime.block_on(channel.send_text(serde_json::json!({"type":"display.selected","id":monitor.id()?}).to_string()))?;
                            continue;
                        }

                        if matches!(
                            command.kind.as_str(),
                            "clipboard.read" | "clipboard.copy" | "clipboard.cut"
                        ) {
                            if input.clipboard.is_some() && command.kind != "clipboard.read" {
                                input.clipboard_shortcut(if command.kind == "clipboard.cut" {
                                    'x'
                                } else {
                                    'c'
                                })?;
                                std::thread::sleep(Duration::from_millis(60));
                            }
                            let response = match input.clipboard.as_mut() {
                                None => {
                                    serde_json::json!({"type":"clipboard.result","code":"clipboardDenied"})
                                }
                                Some(clipboard) => match clipboard.get_text() {
                                    Ok(text) if text.len() <= 16384 => {
                                        serde_json::json!({"type":"clipboard.text","text":text})
                                    }
                                    _ => {
                                        serde_json::json!({"type":"clipboard.result","code":"clipboardUnavailable"})
                                    }
                                },
                            };
                            runtime.block_on(async {
                                tokio::time::timeout(
                                    Duration::from_secs(2),
                                    channel.send_text({
                                        let encoded = response.to_string();
                                        if encoded.len() <= 20000 { encoded } else { serde_json::json!({"type":"clipboard.result","code":"clipboardUnavailable"}).to_string() }
                                    }),
                                )
                                .await
                            })??;
                            continue;
                        }
                        let clipboard_command =
                            matches!(command.kind.as_str(), "clipboard" | "clipboard.paste");
                        input.apply(command)?;
                        if clipboard_command {
                            let code = if clipboard {
                                "clipboardApplied"
                            } else {
                                "clipboardDenied"
                            };
                            runtime.block_on(async {
                                tokio::time::timeout(
                                    Duration::from_secs(2),
                                    channel.send_text(
                                        serde_json::json!({"type":"clipboard.result","code":code})
                                            .to_string(),
                                    ),
                                )
                                .await
                            })??;
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
            Ok(())
        })();
        if result.is_err() {
            stop.store(true, Ordering::Relaxed);
            tracing::warn!("Desktop input stopped: display access unavailable");
        }
    });
}
fn selected(selection: &AtomicU32) -> Option<u32> {
    let id = selection.load(Ordering::SeqCst);
    if id == u32::MAX { None } else { Some(id) }
}
pub fn capture(
    track: Arc<TrackLocalStaticSample>,
    stop: Arc<AtomicBool>,
    selection: Arc<AtomicU32>,
    keyframe: Arc<AtomicBool>,
    quality: VideoConfig,
) {
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let result: Result<()> = (|| {
            let mut current = selected(&selection);
            let mut capture = NativeCapture::new(select_monitor(current)?);
            let mut settings = *quality.lock().unwrap();
            let mut encoder = video_encoder(settings)?;
            let clock = std::time::Instant::now();
            let mut refreshed = std::time::Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let started = std::time::Instant::now();
                let requested = *quality.lock().unwrap();
                if requested != settings {
                    settings = requested;
                    encoder = video_encoder(settings)?;
                    keyframe.store(true, Ordering::Relaxed);
                }
                let interval = Duration::from_secs_f64(1.0 / f64::from(settings.fps));
                let next = selected(&selection);
                if next != current {
                    capture = NativeCapture::new(select_monitor(next)?);
                    current = next;
                }
                let frame = capture.frame().context("screen capture")?;
                let ratio = (f64::from(settings.width) / f64::from(frame.width())).min(1.0);
                let width = ((f64::from(frame.width()) * ratio) as u32 / 2 * 2).max(2);
                let height = ((f64::from(frame.height()) * ratio) as u32 / 2 * 2).max(2);
                let frame = crate::video_resize::resize(&frame, width, height);
                let source = YUVBuffer::from_rgba8_source(RgbaSliceU8::new(
                    frame.as_raw(),
                    (width as usize, height as usize),
                ));
                if keyframe.swap(false, Ordering::Relaxed)
                    || refreshed.elapsed() >= Duration::from_secs(2)
                {
                    encoder.force_intra_frame();
                    refreshed = std::time::Instant::now();
                }
                let data = encoder
                    .encode_at(
                        &source,
                        openh264::Timestamp::from_millis(clock.elapsed().as_millis() as u64),
                    )?
                    .to_vec();
                if data.is_empty() {
                    std::thread::sleep(interval.saturating_sub(started.elapsed()));
                    continue;
                }
                runtime
                    .block_on(async {
                        tokio::time::timeout(
                            Duration::from_secs(2),
                            track.write_sample(&Sample {
                                data: Bytes::from(data),
                                duration: interval,
                                ..Default::default()
                            }),
                        )
                        .await
                    })
                    .context("video send deadline")?
                    .context("video transport")?;
                std::thread::sleep(interval.saturating_sub(started.elapsed()));
            }
            Ok(())
        })();
        if let Err(error) = result
            && !stop.swap(true, Ordering::Relaxed)
        {
            tracing::warn!(error = %error, "Desktop stream stopped");
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_settings_are_bounded_and_encoder_reconfigures() {
        assert!(VideoSettings::validated(0, 1_000_000, 1280).is_err());
        assert!(VideoSettings::validated(60, 1_000_000, 1280).is_err());
        assert!(VideoSettings::validated(15, 50_000_000, 1280).is_err());
        assert!(VideoSettings::validated(15, 1_000_000, 9000).is_err());
        let rgba = vec![128; 64 * 64 * 4];
        let source = YUVBuffer::from_rgba8_source(RgbaSliceU8::new(&rgba, (64, 64)));
        for fps in [3, 15, 30] {
            let mut encoder =
                video_encoder(VideoSettings::validated(fps, 800_000, 1280).unwrap()).unwrap();
            encoder.force_intra_frame();
            let encoded = encoder.encode(&source).unwrap().to_vec();
            assert!(
                openh264::decoder::Decoder::new()
                    .unwrap()
                    .decode(&encoded)
                    .unwrap()
                    .is_some()
            );
        }
    }
    #[test]
    fn bitrate_control_preserves_cadence_with_advancing_timestamps() {
        let rgba = vec![128; 64 * 64 * 4];
        let source = YUVBuffer::from_rgba8_source(RgbaSliceU8::new(&rgba, (64, 64)));
        let mut encoder = video_encoder(VideoSettings::default()).unwrap();
        let frames = (0..60)
            .filter(|i| {
                !encoder
                    .encode_at(&source, openh264::Timestamp::from_millis(i * 67))
                    .unwrap()
                    .to_vec()
                    .is_empty()
            })
            .count();
        assert!(frames >= 50, "unexpected frame skipping: {frames}");
    }
    #[test]
    fn rejects_unknown_multi_character_keys() {
        assert!(key("RunShellCommand").is_none());
        assert!(key("Enter").is_some());
    }
    #[test]
    fn h264_encoder_produces_video() {
        let rgba = vec![128u8; 64 * 64 * 4];
        let source = YUVBuffer::from_rgba8_source(RgbaSliceU8::new(&rgba, (64, 64)));
        let mut encoder = Encoder::new().unwrap();
        assert!(!encoder.encode(&source).unwrap().to_vec().is_empty());
    }
    #[test]
    fn forced_keyframe_recovers_a_decoder_without_previous_frames() {
        let rgba = vec![128u8; 64 * 64 * 4];
        let source = YUVBuffer::from_rgba8_source(RgbaSliceU8::new(&rgba, (64, 64)));
        let mut encoder = Encoder::new().unwrap();
        for _ in 0..15 {
            let _ = encoder.encode(&source).unwrap();
        }
        encoder.force_intra_frame();
        let data = encoder.encode(&source).unwrap().to_vec();
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        assert!(decoder.decode(&data).unwrap().is_some());
    }
}
