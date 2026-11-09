use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{fs::File, path::Path};

// Deliberately do not derive Debug for credentials.
#[derive(Deserialize)]
pub struct Secrets {
    pub rtsp_user: String,
    pub rtsp_pw: String,
}
impl Secrets {
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path).context("cannot open secrets file read-only")?;
        let secrets: Self = serde_json::from_reader(file).map_err(|_| {
            anyhow::anyhow!("invalid secrets JSON; rtsp_user and rtsp_pw must be strings")
        })?;
        ensure!(
            !secrets.rtsp_user.is_empty() && !secrets.rtsp_pw.is_empty(),
            "camera credentials must not be empty"
        );
        Ok(secrets)
    }
    pub fn redact(&self, text: &str) -> String {
        text.replace(&self.rtsp_pw, "[redacted]")
            .replace(&self.rtsp_user, "[redacted]")
    }
}
