use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::{CaptureRegion, CapturedFrame};

pub async fn capture_region(region: CaptureRegion) -> Result<CapturedFrame, String> {
    tokio::task::spawn_blocking(move || capture_region_blocking(region))
        .await
        .map_err(|e| format!("Capture task failed: {e}"))?
}

#[cfg(target_os = "macos")]
fn capture_region_blocking(region: CaptureRegion) -> Result<CapturedFrame, String> {
    let path = temp_capture_file();
    let rect = format!(
        "{},{},{},{}",
        region.x, region.y, region.width, region.height
    );

    let output = Command::new("screencapture")
        .arg("-x")
        .arg("-R")
        .arg(rect)
        .arg(&path)
        .output()
        .map_err(|e| format!("调用 screencapture 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not authorized") || stderr.contains("User canceled") {
            return Err(
                "No screen recording permission. Go to System Settings -> Privacy & Security -> Screen Recording and allow this app.".to_string(),
            );
        }
        return Err(format!("screencapture error: {stderr}"));
    }

    let bytes = fs::read(&path).map_err(|e| format!("Read capture file failed: {e}"))?;
    let _ = fs::remove_file(&path);

    let image = image::load_from_memory(&bytes)
        .map_err(|e| format!("Decode image failed: {e}"))?
        .to_rgba8();
    let width =
        u16::try_from(image.width()).map_err(|_| "Image width exceeds GIF limit".to_string())?;
    let height =
        u16::try_from(image.height()).map_err(|_| "Image height exceeds GIF limit".to_string())?;

    Ok(CapturedFrame {
        width,
        height,
        rgba: image.into_raw(),
    })
}

#[cfg(not(target_os = "macos"))]
fn capture_region_blocking(_region: CaptureRegion) -> Result<CapturedFrame, String> {
    Err("Current implementation supports macOS only".to_string())
}

fn temp_capture_file() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("gifcapture-{nanos}.png"))
}
