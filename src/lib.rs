pub mod camera;
pub mod config;
pub mod runner;
pub mod secrets;
pub mod temperature;

use anyhow::{Context, Result};
use camera::FetchedImage;
use chrono::{DateTime, Utc};
use config::{CameraId, Crop};
use image::{ImageFormat, ImageReader, Limits};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

pub struct ProcessedImage {
    pub camera: CameraId,
    pub captured_at: DateTime<Utc>,
    pub jpeg: Vec<u8>,
}
pub struct Capture {
    pub camera: CameraId,
    pub path: PathBuf,
    pub captured_at: DateTime<Utc>,
    pub replaced: bool,
}

pub fn process(image: FetchedImage, crop: Crop) -> Result<ProcessedImage> {
    camera::validate_jpeg(&image.jpeg)?;
    let mut reader = ImageReader::with_format(Cursor::new(image.jpeg), ImageFormat::Jpeg);
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image_pixels = reader.decode().context("cannot decode camera JPEG")?;
    let pixels = match crop {
        Crop::None => image_pixels,
        Crop::CenterSquare => {
            // Preserve the existing Stake width-crop behavior, including portrait images.
            let width = image_pixels.width().min(image_pixels.height());
            image_pixels.crop_imm(
                (image_pixels.width() - width) / 2,
                0,
                width,
                image_pixels.height(),
            )
        }
    };
    log::info!(
        "{}: processed {}x{} capture",
        image.camera,
        pixels.width(),
        pixels.height()
    );
    let mut encoded = Cursor::new(Vec::new());
    pixels
        .write_to(&mut encoded, ImageFormat::Jpeg)
        .context("cannot encode capture JPEG")?;
    Ok(ProcessedImage {
        camera: image.camera,
        captured_at: image.captured_at,
        jpeg: encoded.into_inner(),
    })
}
pub fn save(image: ProcessedImage, data_dir: &Path) -> Result<Capture> {
    let directory = data_dir.join("files");
    fs::create_dir_all(&directory).context("cannot create capture directory")?;
    let path = directory.join(format!(
        "{}_{}z.jpg",
        image.camera.stem(),
        image.captured_at.format("%Y%m%d%H%M%S")
    ));
    let mut temporary =
        tempfile::NamedTempFile::new_in(&directory).context("cannot create temporary image")?;
    temporary
        .write_all(&image.jpeg)
        .context("cannot write capture JPEG")?;
    temporary.flush().context("cannot flush capture JPEG")?;
    let replaced = path.exists();
    temporary
        .persist(&path)
        .map_err(|e| e.error)
        .context("cannot publish capture JPEG")?;
    if replaced {
        log::warn!("replacing existing capture {}", path.display());
    }
    Ok(Capture {
        camera: image.camera,
        path,
        captured_at: image.captured_at,
        replaced,
    })
}
