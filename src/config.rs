use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[value(rename_all = "verbatim")]
pub enum CameraId {
    MazamaStakeCan,
    MazamaRanchCam,
}
impl CameraId {
    pub fn stem(self) -> &'static str {
        match self {
            Self::MazamaStakeCan => "mazama_stake_can",
            Self::MazamaRanchCam => "mazama_ranch_cam",
        }
    }
}
impl fmt::Display for CameraId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
pub enum TaskId {
    MazamaStakeCan,
    MazamaRanchCam,
    ChechaquoTemp,
}
impl TaskId {
    pub fn camera(self) -> Option<CameraId> {
        match self {
            Self::MazamaStakeCan => Some(CameraId::MazamaStakeCan),
            Self::MazamaRanchCam => Some(CameraId::MazamaRanchCam),
            Self::ChechaquoTemp => None,
        }
    }
}
impl From<CameraId> for TaskId {
    fn from(id: CameraId) -> Self {
        match id {
            CameraId::MazamaStakeCan => Self::MazamaStakeCan,
            CameraId::MazamaRanchCam => Self::MazamaRanchCam,
        }
    }
}
impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Deserialize)]
pub struct Config {
    #[serde(default)]
    pub graph: crate::graph::GraphConfig,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub tasks: Vec<TaskId>,
    #[serde(default)]
    pub cameras: BTreeMap<CameraId, CameraConfig>,
    pub temperature: Option<crate::temperature::TemperatureConfig>,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Https,
    ReolinkUid,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Crop {
    CenterSquare,
    None,
}
#[derive(Clone, Deserialize)]
pub struct CameraConfig {
    pub source: Source,
    pub host: Option<String>,
    pub channel: u8,
    pub crop: Crop,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub accept_invalid_certs: bool,
}
fn timeout() -> u64 {
    15
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).context("cannot read configuration file")?;
        let mut config: Self = toml::from_str(&text).map_err(|_| {
            anyhow::anyhow!("invalid TOML configuration or missing required setting")
        })?;
        ensure!(
            !config.data_dir.as_os_str().is_empty(),
            "data_dir must not be empty"
        );
        config.graph.validate()?;
        let base = path.parent().unwrap_or(Path::new("."));
        if config.data_dir.is_relative() {
            config.data_dir = base.join(&config.data_dir);
        }
        if let Some(t) = &mut config.temperature
            && let Some(fixture) = &mut t.fixture_file
            && fixture.is_relative()
        {
            *fixture = base.join(&*fixture);
        }
        Ok(config)
    }
    pub fn validate_tasks(&self, tasks: &[TaskId]) -> Result<()> {
        ensure!(!tasks.is_empty(), "no tasks selected");
        let mut seen = HashSet::new();
        for &task in tasks {
            ensure!(seen.insert(task), "duplicate task {task}");
            if let Some(id) = task.camera() {
                let camera = self
                    .cameras
                    .get(&id)
                    .with_context(|| format!("missing configuration for {id}"))?;
                ensure!(camera.timeout_seconds > 0, "{id}: timeout must be positive");
                match id {
                    CameraId::MazamaStakeCan => {
                        ensure!(
                            camera.source == Source::Https && camera.crop == Crop::CenterSquare,
                            "{id}: requires https source and center_square crop"
                        );
                        crate::camera::snapshot_url(camera)
                            .with_context(|| format!("{id}: invalid HTTPS settings"))?;
                    }
                    CameraId::MazamaRanchCam => ensure!(
                        camera.source == Source::ReolinkUid
                            && camera.crop == Crop::None
                            && camera.channel == 0,
                        "{id}: requires reolink_uid source, channel 0 and no crop"
                    ),
                }
            } else {
                self.temperature
                    .as_ref()
                    .context("missing ChechaquoTemp [temperature] configuration")?
                    .validate()?;
            }
        }
        Ok(())
    }
}
