use anyhow::Result;
use chrono::{TimeZone, Utc};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, RgbImage};
use stakecam::{
    camera::{FetchedImage, snapshot_url},
    config::{CameraId, Config, Crop, TaskId},
    process, save,
    secrets::Secrets,
};
use std::{fs, io::Cursor, path::Path};

fn fixture(id: CameraId, width: u32, height: u32) -> FetchedImage {
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
    FetchedImage {
        camera: id,
        captured_at: Utc.with_ymd_and_hms(2026, 11, 8, 23, 21, 30).unwrap(),
        jpeg: bytes.into_inner(),
    }
}
fn config(directory: &Path) -> Config {
    let path = directory.join("config.toml");
    fs::write(&path, "data_dir = 'data'\ntasks = ['MazamaStakeCan', 'MazamaRanchCam']\n[cameras.MazamaStakeCan]\nsource = 'https'\nhost = '127.0.0.1'\nchannel = 1\ncrop = 'center_square'\n[cameras.MazamaRanchCam]\nsource = 'reolink_uid'\nchannel = 0\ncrop = 'none'\ntimeout_seconds = 60\n").unwrap();
    Config::load(&path).unwrap()
}
#[test]
fn camera_policies_utc_seconds_and_atomic_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let a = save(
        process(
            fixture(CameraId::MazamaStakeCan, 200, 100),
            Crop::CenterSquare,
        )
        .unwrap(),
        temp.path(),
    )
    .unwrap();
    assert!(a.path.ends_with("mazama_stake_can_20261108232130z.jpg"));
    assert!(!a.replaced);
    let image = image::open(&a.path).unwrap();
    assert_eq!(image.dimensions(), (100, 100));
    assert!(image.to_rgb8().get_pixel(50, 50).0[1] > 200);
    assert!(
        save(
            process(
                fixture(CameraId::MazamaStakeCan, 50, 100),
                Crop::CenterSquare
            )
            .unwrap(),
            temp.path()
        )
        .unwrap()
        .replaced
    );
    assert_eq!(image::open(a.path).unwrap().dimensions(), (50, 100));
    let b = save(
        process(fixture(CameraId::MazamaRanchCam, 200, 100), Crop::None).unwrap(),
        temp.path(),
    )
    .unwrap();
    assert!(b.path.ends_with("mazama_ranch_cam_20261108232130z.jpg"));
    assert_eq!(image::open(&b.path).unwrap().dimensions(), (200, 100));
    assert_eq!(fs::read_dir(b.path.parent().unwrap()).unwrap().count(), 2);
}
#[test]
fn invalid_images_decode_limits_and_save_failure() {
    for bytes in [
        b"not jpeg".to_vec(),
        b"\xff\xd8\xffbroken".to_vec(),
        vec![0; 32 * 1024 * 1024 + 1],
    ] {
        let mut image = fixture(CameraId::MazamaStakeCan, 10, 10);
        image.jpeg = bytes;
        assert!(process(image, Crop::None).is_err());
    }
    assert!(process(fixture(CameraId::MazamaRanchCam, 8193, 1), Crop::None).is_err());
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked");
    fs::write(&blocked, "file").unwrap();
    assert!(
        save(
            process(fixture(CameraId::MazamaStakeCan, 10, 10), Crop::None).unwrap(),
            &blocked
        )
        .is_err()
    );
}
#[test]
fn selected_settings_validate_before_fetch() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = config(temp.path());
    c.validate_tasks(&c.tasks).unwrap();
    assert!(
        c.validate_tasks(&[TaskId::MazamaStakeCan, TaskId::MazamaStakeCan])
            .is_err()
    );
    c.cameras
        .get_mut(&CameraId::MazamaRanchCam)
        .unwrap()
        .timeout_seconds = 0;
    assert!(c.validate_tasks(&c.tasks).is_err());
    c.validate_tasks(&[TaskId::MazamaStakeCan]).unwrap();
    let stake = c.cameras.get_mut(&CameraId::MazamaStakeCan).unwrap();
    for host in [
        "example.com/path?password=oops",
        "user:password@example.com",
        "example.com#secret",
    ] {
        stake.host = Some(host.into());
        assert!(snapshot_url(stake).is_err());
    }
    fs::write(
        temp.path().join("bad.toml"),
        "tasks = ['unknown']\ndata_dir = 'data'",
    )
    .unwrap();
    assert!(Config::load(&temp.path().join("bad.toml")).is_err());
}
#[test]
fn hierarchical_credentials_are_isolated_and_redacted() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("private.json");
    fs::write(&path,r#"{"cameras":{"MazamaStakeCan":{"username":"fixture-user","password":"fixture-password"}},"temperature":{"ambient":{"api_key":"fixture-api","application_key":"fixture-app"}}}"#).unwrap();
    let s = Secrets::load(&path).unwrap();
    s.camera(CameraId::MazamaStakeCan).unwrap();
    assert!(s.camera(CameraId::MazamaRanchCam).is_err());
    let redacted = s.redact("fixture-user fixture-password fixture-api fixture-app");
    assert!(!redacted.contains("fixture-"));
    fs::write(&path, r#"{"password":fixture-password}"#).unwrap();
    assert!(
        !Secrets::load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("fixture-password")
    );
    fs::write(&path, r#"{"rtsp_user":"old","rtsp_pw":"old"}"#).unwrap();
    assert!(
        Secrets::load(&path)
            .unwrap()
            .camera(CameraId::MazamaStakeCan)
            .is_err()
    );
}

use stakecam::runner::{self, Fetched, Fetcher};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
struct Fake {
    barrier: tokio::sync::Barrier,
    completed: AtomicUsize,
    data_dir: std::path::PathBuf,
    fail: Option<TaskId>,
}
impl Fetcher for Fake {
    fn fetch(&self, task: TaskId) -> Pin<Box<dyn Future<Output = Result<Fetched>> + Send + '_>> {
        Box::pin(async move {
            // This would deadlock if fetches ran serially.
            tokio::time::timeout(std::time::Duration::from_secs(2), self.barrier.wait()).await?;
            assert!(
                !self.data_dir.exists(),
                "save happened before fetch barrier"
            );
            self.completed.fetch_add(1, Ordering::SeqCst);
            if self.fail == Some(task) {
                anyhow::bail!("simulated unavailable camera");
            }
            Ok(Fetched::Image(fixture(task.camera().unwrap(), 200, 100)))
        })
    }
}
#[tokio::test]
async fn concurrent_fetch_barrier_and_partial_success_both_directions() {
    for fail in [
        None,
        Some(TaskId::MazamaStakeCan),
        Some(TaskId::MazamaRanchCam),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let c = Arc::new(config(temp.path()));
        let fake = Arc::new(Fake {
            barrier: tokio::sync::Barrier::new(2),
            completed: AtomicUsize::new(0),
            data_dir: c.data_dir.clone(),
            fail,
        });
        let report = runner::run(Arc::clone(&c), c.tasks.clone(), fake.clone()).await;
        assert_eq!(fake.completed.load(Ordering::SeqCst), 2);
        assert_eq!(report.paths.len(), if fail.is_some() { 1 } else { 2 });
        assert_eq!(report.failures.len(), usize::from(fail.is_some()));
        if fail.is_none() {
            assert!(
                report.paths[0]
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("mazama_stake_can")
            );
        }
    }
}

struct Mixed {
    bad_image: bool,
}
impl Fetcher for Mixed {
    fn fetch(&self, task: TaskId) -> Pin<Box<dyn Future<Output = Result<Fetched>> + Send + '_>> {
        Box::pin(async move {
            if let Some(id) = task.camera() {
                let mut image = fixture(id, 20, 10);
                if self.bad_image && id == CameraId::MazamaStakeCan {
                    image.jpeg = b"invalid JPEG".to_vec();
                }
                Ok(Fetched::Image(image))
            } else {
                Ok(Fetched::Temperature(vec![stakecam::temperature::Reading {
                    source: "fixture".into(),
                    observed_at: Utc::now(),
                    collected_at: Utc::now(),
                    celsius: 2.0,
                }]))
            }
        })
    }
}
#[tokio::test]
async fn processing_and_temperature_persistence_failures_are_isolated() {
    let temp = tempfile::tempdir().unwrap();
    let c = Arc::new(config(temp.path()));
    let report = runner::run(
        Arc::clone(&c),
        c.tasks.clone(),
        Arc::new(Mixed { bad_image: true }),
    )
    .await;
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].starts_with("MazamaStakeCan: process/save:"));
    assert_eq!(report.paths.len(), 1);
    assert!(
        report.paths[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("mazama_ranch_cam")
    );
    fs::create_dir_all(&c.data_dir).unwrap();
    fs::write(c.data_dir.join("temperature"), "blocked").unwrap();
    let report = runner::run(
        c,
        vec![TaskId::ChechaquoTemp, TaskId::MazamaStakeCan],
        Arc::new(Mixed { bad_image: false }),
    )
    .await;
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].starts_with("ChechaquoTemp: process/save:"));
    assert_eq!(report.paths.len(), 1);
}

struct StakeAndTemperature {
    barrier: tokio::sync::Barrier,
    directory: std::path::PathBuf,
}
impl Fetcher for StakeAndTemperature {
    fn fetch(&self, task: TaskId) -> Pin<Box<dyn Future<Output = Result<Fetched>> + Send + '_>> {
        Box::pin(async move {
            tokio::time::timeout(std::time::Duration::from_secs(2), self.barrier.wait()).await?;
            assert!(
                !self.directory.exists(),
                "processing started before both fetches finished"
            );
            let image = fixture(CameraId::MazamaStakeCan, 400, 200);
            if task.camera().is_some() {
                return Ok(Fetched::Image(image));
            }
            Ok(Fetched::Temperature(
                [(-10, -2.0), (-5, 2.0)]
                    .into_iter()
                    .map(|(minutes, celsius)| stakecam::temperature::Reading {
                        source: "fixture".into(),
                        observed_at: image.captured_at + chrono::Duration::minutes(minutes),
                        collected_at: image.captured_at,
                        celsius,
                    })
                    .collect(),
            ))
        })
    }
}
#[tokio::test]
async fn stake_and_temperature_fetch_concurrently_then_crop_and_draw_in_either_order() {
    for tasks in [
        vec![TaskId::MazamaStakeCan, TaskId::ChechaquoTemp],
        vec![TaskId::ChechaquoTemp, TaskId::MazamaStakeCan],
    ] {
        let temp = tempfile::tempdir().unwrap();
        config(temp.path());
        let path = temp.path().join("config.toml");
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("\n[temperature]\nsource = 'fixture'\nfixture_file = 'unused.json'\n");
        fs::write(&path, text).unwrap();
        let c = Arc::new(Config::load(&path).unwrap());
        let fetcher = Arc::new(StakeAndTemperature {
            barrier: tokio::sync::Barrier::new(2),
            directory: c.data_dir.clone(),
        });
        let report = runner::run(c, tasks, fetcher).await;
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let annotated = report
            .paths
            .iter()
            .find(|p| p.to_string_lossy().ends_with("_annotated.jpg"))
            .unwrap();
        let original = report
            .paths
            .iter()
            .find(|p| p.extension().is_some_and(|e| e == "jpg") && *p != annotated)
            .unwrap();
        let before = image::open(original).unwrap().to_rgb8();
        let after = image::open(annotated).unwrap().to_rgb8();
        assert_eq!(after.dimensions(), (200, 200));
        assert_ne!(before, after);
        assert!(
            after
                .enumerate_pixels()
                .any(|(_, y, p)| y > 165 && p[0] > 180 && p[1] > 180 && p[2] > 180)
        );
    }
}
