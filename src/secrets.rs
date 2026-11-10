use crate::config::CameraId;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::BTreeMap, fs::File, path::Path};

// Secret-bearing types deliberately do not implement Debug.
#[derive(Default, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub cameras: BTreeMap<CameraId, CameraSecrets>,
    #[serde(default)]
    pub temperature: TemperatureSecrets,
}
#[derive(Default, Deserialize)]
pub struct CameraSecrets {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub uid: Option<String>,
}
#[derive(Default, Deserialize)]
pub struct TemperatureSecrets {
    #[serde(default)]
    pub ambient: AmbientSecrets,
}
#[derive(Default, Deserialize)]
pub struct AmbientSecrets {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub application_key: String,
}
impl Secrets {
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path).context("cannot open secrets file read-only")?;
        serde_json::from_reader(file)
            .map_err(|_| anyhow::anyhow!("invalid secrets JSON or credential field types"))
    }
    pub fn camera(&self, id: CameraId) -> Result<&CameraSecrets> {
        let s = self
            .cameras
            .get(&id)
            .context("missing camera credentials")?;
        ensure!(
            !s.username.is_empty() && !s.password.is_empty(),
            "camera credentials must not be empty"
        );
        if id == CameraId::MazamaRanchCam {
            ensure!(
                s.uid.as_ref().is_some_and(|s| !s.is_empty()),
                "missing camera UID"
            );
        }
        Ok(s)
    }
    pub fn redact(&self, text: &str) -> String {
        let mut values = vec![
            self.temperature.ambient.api_key.as_str(),
            self.temperature.ambient.application_key.as_str(),
        ];
        for s in self.cameras.values() {
            values.extend([
                s.username.as_str(),
                s.password.as_str(),
                s.uid.as_deref().unwrap_or(""),
            ]);
        }
        values.retain(|s| !s.is_empty());
        values.sort_by_key(|s| std::cmp::Reverse(s.len()));
        values.into_iter().fold(text.to_owned(), |text, value| {
            text.replace(value, "[redacted]")
        })
    }
}
