#[path = "../src/video_resize.rs"]
mod video_resize;
use openh264::{
    OpenH264API, Timestamp,
    encoder::{BitRate, Encoder, EncoderConfig, FrameRate, RateControlMode},
    formats::{RgbaSliceU8, YUVBuffer},
};
use std::time::{Duration, Instant};
fn main() -> anyhow::Result<()> {
    let monitor = xcap::Monitor::all()?
        .into_iter()
        .find(|m| m.id().ok() == Some(33))
        .ok_or_else(|| anyhow::anyhow!("display 33 unavailable"))?;
    let mut encoder = Encoder::with_api_config(
        OpenH264API::from_source(),
        EncoderConfig::new()
            .bitrate(BitRate::from_bps(600000))
            .max_frame_rate(FrameRate::from_hz(10.0))
            .rate_control_mode(RateControlMode::Bitrate)
            .skip_frames(true),
    )?;
    #[cfg(target_os = "linux")]
    let fast = if std::env::var_os("REMVORA_PROBE_FAST").is_some() {
        Some(libwayshot_xcap::WayshotConnection::new()?)
    } else {
        None
    };
    let clock = Instant::now();
    for i in 0..20 {
        let t = Instant::now();
        #[cfg(target_os = "linux")]
        let frame = if let Some(connection) = &fast {
            use libwayshot_xcap::region::{LogicalRegion, Position, Region, Size};
            let image = connection
                .screenshot(
                    LogicalRegion {
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
                    },
                    false,
                )?
                .to_rgba8();
            xcap::image::RgbaImage::from_raw(image.width(), image.height(), image.into_raw())
                .unwrap()
        } else {
            monitor.capture_image()?
        };
        #[cfg(not(target_os = "linux"))]
        let frame = monitor.capture_image()?;
        let capture = t.elapsed();
        let resized = if std::env::var_os("REMVORA_PROBE_RESIZE").is_some() {
            video_resize::resize(&frame, 960, 540)
        } else {
            xcap::image::imageops::resize(
                &frame,
                960,
                540,
                xcap::image::imageops::FilterType::Triangle,
            )
        };
        let yuv = YUVBuffer::from_rgba8_source(RgbaSliceU8::new(resized.as_raw(), (960, 540)));
        let bytes = encoder
            .encode_at(
                &yuv,
                Timestamp::from_millis(clock.elapsed().as_millis() as u64),
            )?
            .to_vec();
        println!(
            "frame={i} capture_ms={} total_ms={} bytes={}",
            capture.as_millis(),
            t.elapsed().as_millis(),
            bytes.len()
        );
        std::thread::sleep(Duration::from_millis(100).saturating_sub(t.elapsed()));
    }
    Ok(())
}
