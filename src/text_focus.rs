//! Optional, bounded accessibility metadata query. No field contents leave the device.
#[cfg(any(test, target_os = "linux", target_os = "windows"))]
use serde::Deserialize;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::process::Stdio;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::time::Duration;
#[cfg(any(test, target_os = "linux", target_os = "windows"))]
#[derive(Deserialize)]
struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
pub async fn query(x: i32, y: i32, width: u32, height: u32, regions: bool) -> Vec<[f64; 4]> {
    query_inner(x, y, width, height, regions)
        .await
        .unwrap_or_default()
}
async fn query_inner(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    regions: bool,
) -> Option<Vec<[f64; 4]>> {
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut c = tokio::process::Command::new("/usr/bin/python3");
        c.args(["-c", include_str!("focus/linux.py")]);
        if regions {
            c.arg("--regions");
        }
        c
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let root = std::env::var_os("SystemRoot")?;
        let mut c = tokio::process::Command::new(
            std::path::PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        c.creation_flags(0x08000000); // Do not flash a console window for metadata queries.
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("focus/windows.ps1"),
        ]);
        c
    };
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (x, y, width, height, regions);
        None
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let output = tokio::time::timeout(
            Duration::from_secs(2),
            command
                .kill_on_drop(true)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output(),
        )
        .await
        .ok()?
        .ok()?;
        if !output.status.success() || output.stdout.len() > 8192 {
            return None;
        }
        #[cfg(target_os = "linux")]
        if regions {
            let bounds: Vec<Bounds> = serde_json::from_slice(&output.stdout).ok()?;
            return Some(
                bounds
                    .into_iter()
                    .take(32)
                    .filter_map(|b| normalize(b, x, y, width, height))
                    .collect(),
            );
        }
        #[cfg(target_os = "windows")]
        let _ = regions;
        let bounds: Bounds = serde_json::from_slice(&output.stdout).ok()?;
        Some(normalize(bounds, x, y, width, height).into_iter().collect())
    }
}
#[cfg(any(test, target_os = "linux", target_os = "windows"))]
fn normalize(b: Bounds, x: i32, y: i32, width: u32, height: u32) -> Option<[f64; 4]> {
    if width == 0
        || height == 0
        || ![b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite())
        || b.width <= 0.0
        || b.height <= 0.0
    {
        return None;
    }
    let left = ((b.x - f64::from(x)) / f64::from(width)).max(0.0);
    let top = ((b.y - f64::from(y)) / f64::from(height)).max(0.0);
    let right = ((b.x + b.width - f64::from(x)) / f64::from(width)).min(1.0);
    let bottom = ((b.y + b.height - f64::from(y)) / f64::from(height)).min(1.0);
    (right > left && bottom > top).then_some([left, top, right, bottom])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_are_clipped_to_selected_screen() {
        assert_eq!(
            normalize(
                Bounds {
                    x: 90.,
                    y: 20.,
                    width: 50.,
                    height: 30.
                },
                100,
                0,
                100,
                100
            ),
            Some([0., 0.2, 0.4, 0.5])
        );
        assert!(
            normalize(
                Bounds {
                    x: 500.,
                    y: 20.,
                    width: 50.,
                    height: 30.
                },
                100,
                0,
                100,
                100
            )
            .is_none()
        );
        assert!(
            normalize(
                Bounds {
                    x: f64::NAN,
                    y: 0.,
                    width: 1.,
                    height: 1.
                },
                0,
                0,
                100,
                100
            )
            .is_none()
        );
    }
}
