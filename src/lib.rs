pub mod camera;
pub mod config;
pub mod secrets;

use anyhow::{Context, Result, ensure};
use camera::CaptureSource;
use chrono::{DateTime, Utc};
use config::Config;
use image::{ImageFormat, ImageReader};
use std::{
    fs,
    io::{Cursor, Write},
    path::PathBuf,
};

pub struct Capture {
    pub path: PathBuf,
    pub captured_at: DateTime<Utc>,
    pub replaced: bool,
}
pub fn capture(source: &impl CaptureSource, config: &Config, at: DateTime<Utc>) -> Result<Capture> {
    let bytes = source.capture()?;
    ensure!(
        image::guess_format(&bytes).ok() == Some(ImageFormat::Jpeg),
        "capture source did not return a JPEG"
    );
    let image = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg)
        .decode()
        .context("cannot decode camera JPEG")?;
    let width = image.width().min(image.height());
    let cropped = image.crop_imm((image.width() - width) / 2, 0, width, image.height());
    let directory = config.data_dir.join("files");
    fs::create_dir_all(&directory).context("cannot create capture directory")?;
    let path = directory.join(format!(
        "{}{}.jpg",
        config.image_prefix,
        at.with_timezone(&config.timezone()?).format("%Y%m%d%H%M")
    ));
    let mut temporary =
        tempfile::NamedTempFile::new_in(&directory).context("cannot create temporary image")?;
    cropped
        .write_to(temporary.as_file_mut(), ImageFormat::Jpeg)
        .context("cannot encode capture JPEG")?;
    temporary.flush().context("cannot flush capture JPEG")?;
    let replaced = path.exists();
    temporary
        .persist(&path)
        .map_err(|e| e.error)
        .context("cannot publish capture JPEG")?;
    if replaced {
        log::warn!("replacing existing capture {}", path.display());
    }
    log::info!("saved {}x{} capture", cropped.width(), cropped.height());
    Ok(Capture {
        path,
        captured_at: at,
        replaced,
    })
}
