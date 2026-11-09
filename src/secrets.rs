use anyhow::{Context, Result};
use serde::Deserialize;
use std::{fs::File, path::Path};

// Deliberately do not derive Debug for credentials.
#[derive(Default, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub rtsp_user: String,
    #[serde(default)]
    pub rtsp_pw: String,
    pub ambient_api_key: Option<String>,
    pub ambient_application_key: Option<String>,
}
impl Secrets {
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path).context("cannot open secrets file read-only")?;
        let secrets: Self = serde_json::from_reader(file)
            .map_err(|_| anyhow::anyhow!("invalid secrets JSON or credential field types"))?;
        Ok(secrets)
    }
    pub fn redact(&self, text: &str) -> String {
        text.replace(&self.rtsp_pw, "[redacted]")
            .replace(&self.rtsp_user, "[redacted]")
    }
}
