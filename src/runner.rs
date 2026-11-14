use crate::{
    camera::{self, FetchedImage},
    config::{Config, TaskId},
    secrets::Secrets,
    temperature::{self, Reading, TemperatureSource},
};
use anyhow::{Context, Result};
use std::{collections::HashMap, future::Future, path::PathBuf, pin::Pin, sync::Arc};
use tokio::task::JoinSet;

pub enum Fetched {
    Image(FetchedImage),
    Temperature(Vec<Reading>),
}
pub trait Fetcher: Send + Sync {
    fn fetch(&self, task: TaskId) -> Pin<Box<dyn Future<Output = Result<Fetched>> + Send + '_>>;
}
pub struct Sources {
    pub config: Arc<Config>,
    pub secrets: Arc<Secrets>,
    pub history_hours: u32,
}
impl Fetcher for Sources {
    fn fetch(&self, task: TaskId) -> Pin<Box<dyn Future<Output = Result<Fetched>> + Send + '_>> {
        Box::pin(async move {
            if let Some(id) = task.camera() {
                let settings = self
                    .config
                    .cameras
                    .get(&id)
                    .context("missing camera settings")?;
                return Ok(Fetched::Image(
                    camera::fetch(id, settings, &self.secrets).await?,
                ));
            }
            let settings = self
                .config
                .temperature
                .as_ref()
                .context("missing temperature settings")?;
            let now = chrono::Utc::now();
            let readings = if settings.source == "fixture" {
                let fixture = temperature::FixtureSource(
                    settings
                        .fixture_file
                        .clone()
                        .context("missing fixture_file")?,
                );
                let hours = self.history_hours;
                tokio::task::spawn_blocking(move || fixture.readings(now, hours))
                    .await
                    .context("temperature fixture worker failed")??
            } else {
                temperature::Ambient {
                    config: settings,
                    secrets: &self.secrets,
                }
                .readings(now, self.history_hours)
                .await?
            };
            Ok(Fetched::Temperature(readings))
        })
    }
}
#[derive(Default)]
pub struct Report {
    pub paths: Vec<PathBuf>,
    pub failures: Vec<String>,
}

pub async fn run(config: Arc<Config>, tasks: Vec<TaskId>, fetcher: Arc<dyn Fetcher>) -> Report {
    let mut workers = JoinSet::new();
    let mut worker_tasks = HashMap::new();
    for (index, task) in tasks.iter().copied().enumerate() {
        let fetcher = Arc::clone(&fetcher);
        let worker = workers.spawn(async move { (index, task, fetcher.fetch(task).await) });
        worker_tasks.insert(worker.id(), task);
    }
    let mut report = Report::default();
    let mut fetched = Vec::new();
    while let Some(outcome) = workers.join_next().await {
        match outcome {
            Ok((index, task, Ok(value))) => fetched.push((index, task, value)),
            Ok((_, task, Err(error))) => report.failures.push(format!("{task}: fetch: {error:#}")),
            Err(error) => {
                if let Some(task) = worker_tasks.get(&error.id()) {
                    report.failures.push(format!("{task}: fetch worker failed"));
                } else {
                    report.failures.push("fetch worker failed".into());
                }
            }
        }
    }
    // No image processing starts until every fetch worker has completed.
    fetched.sort_by_key(|(index, _, _)| *index);
    let fresh: Vec<Reading> = fetched
        .iter()
        .filter_map(|(_, _, value)| match value {
            Fetched::Temperature(readings) => Some(readings.as_slice()),
            _ => None,
        })
        .flatten()
        .cloned()
        .collect();
    let fresh = Arc::new(fresh);
    for (_, task, value) in fetched {
        let fresh = Arc::clone(&fresh);
        let config = Arc::clone(&config);
        let outcome = tokio::task::spawn_blocking(move || -> Result<Vec<PathBuf>> {
            match value {
                Fetched::Image(image) => {
                    let crop = config
                        .cameras
                        .get(&image.camera)
                        .context("missing camera settings")?
                        .crop;
                    let is_stake = image.camera == crate::config::CameraId::MazamaStakeCan;
                    let at = image.captured_at;
                    // Preserve the cropped original as well as the annotated output.
                    let jpeg = image.jpeg.clone();
                    let original = crate::process(image, crop)?;
                    let mut paths = vec![crate::save(original, &config.data_dir)?.path];
                    if is_stake && let Some(settings) = &config.temperature {
                        let mut readings = crate::graph::history(&config.data_dir, at)?;
                        readings.extend(fresh.iter().cloned());
                        let annotated = crate::process_with_graph(
                            FetchedImage {
                                camera: crate::config::CameraId::MazamaStakeCan,
                                captured_at: at,
                                jpeg,
                            },
                            crop,
                            Some((&readings, settings.sampling_interval_minutes, &config.graph)),
                        )?;
                        paths.push(crate::save_annotated(annotated, &config.data_dir)?.path);
                    }
                    Ok(paths)
                }
                Fetched::Temperature(readings) => {
                    Ok(temperature::persist(readings, &config.data_dir)?.1)
                }
            }
        })
        .await;
        match outcome {
            Ok(Ok(paths)) => report.paths.extend(paths),
            Ok(Err(error)) => report
                .failures
                .push(format!("{task}: process/save: {error:#}")),
            Err(_) => report
                .failures
                .push(format!("{task}: processing worker failed")),
        }
    }
    report
}
