use anyhow::Result;
use chrono::{TimeZone, Utc};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, RgbImage};
use stakecam::{camera::CaptureSource, capture, config::Config, secrets::Secrets};
use std::{fs, io::Cursor, path::Path};

struct Fixture(Vec<u8>);
impl CaptureSource for Fixture {
    fn capture(&self) -> Result<Vec<u8>> {
        Ok(self.0.clone())
    }
}
fn fixture(width: u32, height: u32) -> Fixture {
    let pixels = RgbImage::from_fn(width, height, |x, _| {
        if x < width / 4 {
            Rgb([255, 0, 0])
        } else if x >= width * 3 / 4 {
            Rgb([0, 0, 255])
        } else {
            Rgb([0, 255, 0])
        }
    });
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(pixels)
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .unwrap();
    Fixture(bytes.into_inner())
}
fn config(directory: &Path) -> Config {
    let path = directory.join("config.toml");
    fs::write(&path, "data_dir = 'data'\ntimezone = 'America/Los_Angeles'\nimage_prefix = 'mazama_stake_'\n[camera]\nhost = '127.0.0.1'\n").unwrap();
    Config::load(&path).unwrap()
}
#[test]
fn crops_center_and_atomically_replaces_same_minute() {
    let directory = tempfile::tempdir().unwrap();
    let config = config(directory.path());
    let at = Utc.with_ymd_and_hms(2026, 11, 8, 23, 21, 30).unwrap();
    let result = capture(&fixture(200, 100), &config, at).unwrap();
    assert_eq!(
        result.path,
        directory
            .path()
            .join("data/files/mazama_stake_202611081521.jpg")
    );
    assert_eq!(result.captured_at, at);
    assert!(!result.replaced);
    let image = image::open(&result.path).unwrap();
    assert_eq!(image.dimensions(), (100, 100));
    let center = image.to_rgb8().get_pixel(50, 50).0;
    assert!(center[1] > 200 && center[0] < 40 && center[2] < 40);
    assert!(capture(&fixture(50, 100), &config, at).unwrap().replaced);
    assert_eq!(image::open(&result.path).unwrap().dimensions(), (50, 100));
    assert_eq!(
        fs::read_dir(result.path.parent().unwrap()).unwrap().count(),
        1
    );
}
#[test]
fn repeated_dst_hour_replaces_same_filename() {
    let directory = tempfile::tempdir().unwrap();
    let config = config(directory.path());
    let first = Utc.with_ymd_and_hms(2026, 11, 1, 8, 30, 0).unwrap();
    let second = Utc.with_ymd_and_hms(2026, 11, 1, 9, 30, 0).unwrap();
    let a = capture(&fixture(10, 10), &config, first).unwrap();
    let b = capture(&fixture(10, 10), &config, second).unwrap();
    assert_eq!(a.path, b.path);
    assert!(b.replaced);
}
#[test]
fn bad_responses_and_failed_source_leave_no_output() {
    let directory = tempfile::tempdir().unwrap();
    let config = config(directory.path());
    for bytes in [
        b"not an image".to_vec(),
        b"\xff\xd8\xffbroken jpeg".to_vec(),
        b"[{\"error\":{\"detail\":\"bad login\"}}]".to_vec(),
    ] {
        assert!(capture(&Fixture(bytes), &config, Utc::now()).is_err());
    }
    struct Failed;
    impl CaptureSource for Failed {
        fn capture(&self) -> Result<Vec<u8>> {
            anyhow::bail!("fake unreachable camera")
        }
    }
    assert!(capture(&Failed, &config, Utc::now()).is_err());
    assert!(!config.data_dir.exists());
}
#[test]
fn output_failure_is_reported() {
    let directory = tempfile::tempdir().unwrap();
    let config = config(directory.path());
    fs::write(&config.data_dir, "blocks directory creation").unwrap();
    let error = capture(&fixture(10, 10), &config, Utc::now())
        .err()
        .unwrap();
    assert!(error.to_string().contains("capture directory"));
}
#[test]
fn secrets_and_configuration_errors_never_echo_contents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private.json");
    fs::write(
        &path,
        r#"{"rtsp_user":"fixture", "rtsp_pw":"fixture-password", "spaces_key":"unused"}"#,
    )
    .unwrap();
    assert!(Secrets::load(&path).is_ok());
    fs::write(
        &path,
        r#"{"rtsp_user":"fixture", "rtsp_pw":fixture-password}"#,
    )
    .unwrap();
    assert!(
        !Secrets::load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("fixture-password")
    );
    fs::write(&path, "rtsp_pw = fixture-password").unwrap();
    assert!(
        !Config::load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("fixture-password")
    );
}
#[test]
fn validates_settings_and_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let config = config(directory.path());
    assert_eq!(config.camera.channel, 1);
    assert_eq!(config.camera.timeout_seconds, 15);
    assert!(!config.camera.accept_invalid_certs);
    let path = directory.path().join("config.toml");
    let original = fs::read_to_string(&path).unwrap();
    for invalid in [
        original.replace("America/Los_Angeles", "invalid-zone"),
        original.replace("mazama_stake_", "../escape"),
        format!("{original}timeout_seconds = 0\n"),
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(Config::load(&path).is_err());
    }
}
