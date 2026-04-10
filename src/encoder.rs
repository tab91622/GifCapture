use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use gif::{Encoder, Frame, Repeat};

use crate::types::CapturedFrame;

#[derive(Debug, Clone)]
pub struct EncodeSuccess {
    pub output_path: PathBuf,
    pub frames: Vec<CapturedFrame>,
}

#[derive(Debug, Clone)]
pub struct EncodeFailure {
    pub message: String,
    pub frames: Vec<CapturedFrame>,
}

pub async fn encode_gif(
    output_path: PathBuf,
    frames: Vec<CapturedFrame>,
    fps: u32,
) -> Result<EncodeSuccess, EncodeFailure> {
    tokio::task::spawn_blocking(move || encode_gif_blocking(output_path, frames, fps))
        .await
        .map_err(|e| EncodeFailure {
            message: format!("Encode task failed: {e}"),
            frames: Vec::new(),
        })?
}

fn encode_gif_blocking(
    output_path: PathBuf,
    frames: Vec<CapturedFrame>,
    fps: u32,
) -> Result<EncodeSuccess, EncodeFailure> {
    if frames.is_empty() {
        return Err(EncodeFailure {
            message: "No frames to export".to_string(),
            frames,
        });
    }
    if fps == 0 {
        return Err(EncodeFailure {
            message: "FPS must be greater than 0".to_string(),
            frames,
        });
    }

    let first_width = frames[0].width;
    let first_height = frames[0].height;
    let file = match File::create(&output_path) {
        Ok(file) => file,
        Err(e) => {
            return Err(EncodeFailure {
                message: format!("Create output file failed: {e}"),
                frames,
            });
        }
    };
    let mut writer = BufWriter::new(file);

    let mut encoder = match Encoder::new(&mut writer, first_width, first_height, &[]) {
        Ok(encoder) => encoder,
        Err(e) => {
            return Err(EncodeFailure {
                message: format!("Init GIF encoder failed: {e}"),
                frames,
            });
        }
    };
    if let Err(e) = encoder.set_repeat(Repeat::Infinite) {
        return Err(EncodeFailure {
            message: format!("Set repeat mode failed: {e}"),
            frames,
        });
    }

    let delay = ((100.0 / fps as f32).round() as u16).max(1);

    for frame in &frames {
        if frame.width != first_width || frame.height != first_height {
            return Err(EncodeFailure {
                message: "Frame sizes are inconsistent".to_string(),
                frames,
            });
        }
        let mut rgba = frame.rgba.clone();
        let mut gif_frame = Frame::from_rgba_speed(first_width, first_height, &mut rgba, 10);
        gif_frame.delay = delay;
        if let Err(e) = encoder.write_frame(&gif_frame) {
            return Err(EncodeFailure {
                message: format!("Write GIF frame failed: {e}"),
                frames,
            });
        }
    }

    Ok(EncodeSuccess {
        output_path,
        frames,
    })
}
