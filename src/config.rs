use anyhow::{Context, Result, ensure};
use chrono_tz::Tz;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,
    pub timezone: String,
    pub image_prefix: String,
    pub camera: CameraConfig,
}
#[derive(Deserialize)]
pub struct CameraConfig {
    pub host: String,
    #[serde(default = "channel")]
    pub channel: u32,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub accept_invalid_certs: bool,
}
fn channel() -> u32 {
    1
}
fn timeout() -> u64 {
    15
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).context("cannot read configuration file")?;
        // Do not echo parser diagnostics: configuration may accidentally contain secrets.
        let mut config: Self = toml::from_str(&text).map_err(|_| {
            anyhow::anyhow!("invalid TOML configuration or missing required setting")
        })?;
        config.timezone()?;
        ensure!(
            !config.data_dir.as_os_str().is_empty(),
            "data_dir must not be empty"
        );
        ensure!(
            !config.image_prefix.contains(['/', '\\']) && !config.image_prefix.is_empty(),
            "image_prefix must be a nonempty filename prefix"
        );
        ensure!(
            config.camera.timeout_seconds > 0,
            "camera timeout must be positive"
        );
        if config.data_dir.is_relative() {
            config.data_dir = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(&config.data_dir);
        }
        Ok(config)
    }
    pub fn timezone(&self) -> Result<Tz> {
        self.timezone
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid configured timezone"))
    }
}
