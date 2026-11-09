use crate::{config::CameraConfig, secrets::Secrets};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Url, blocking::Client, redirect::Policy};
use std::{io::Read, time::Duration};

pub trait CaptureSource {
    fn capture(&self) -> Result<Vec<u8>>;
}
pub struct Camera<'a> {
    pub config: &'a CameraConfig,
    pub secrets: &'a Secrets,
}
impl Camera<'_> {
    fn url(&self) -> Result<Url> {
        let host = &self.config.host;
        let mut url = Url::parse(&format!("https://{host}/cgi-bin/api.cgi"))
            .map_err(|_| anyhow::anyhow!("invalid camera host"))?;
        ensure!(
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/cgi-bin/api.cgi"
                && url.query().is_none()
                && url.fragment().is_none(),
            "camera host must contain only a hostname or IP address and optional port"
        );
        url.query_pairs_mut().extend_pairs([
            ("cmd", "Snap".to_owned()),
            ("channel", self.config.channel.to_string()),
            ("rs", rand::random::<u64>().to_string()),
            ("user", self.secrets.rtsp_user.clone()),
            ("password", self.secrets.rtsp_pw.clone()),
        ]);
        Ok(url)
    }
}
impl CaptureSource for Camera<'_> {
    fn capture(&self) -> Result<Vec<u8>> {
        ensure!(
            !self.secrets.rtsp_user.is_empty() && !self.secrets.rtsp_pw.is_empty(),
            "camera credentials must not be empty"
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .danger_accept_invalid_certs(self.config.accept_invalid_certs)
            .redirect(Policy::none())
            .build()
            .map_err(|e| e.without_url())
            .context("cannot build camera client")?;
        let response = client
            .get(self.url()?)
            .send()
            .map_err(|e| e.without_url())
            .context("camera request failed")?;
        let status = response.status();
        let mut bytes = Vec::new();
        // Bound memory use even when a server returns an unexpected response.
        response
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                anyhow::anyhow!("cannot read camera response (connection interrupted or timed out)")
            })?;
        ensure!(
            bytes.len() <= 32 * 1024 * 1024,
            "camera response exceeds 32 MiB"
        );
        self.validate_response(status, &bytes)?;
        Ok(bytes)
    }
}
impl Camera<'_> {
    fn validate_response(&self, status: reqwest::StatusCode, bytes: &[u8]) -> Result<()> {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
            let error = json.pointer("/0/error").or_else(|| json.get("error"));
            let code = error
                .and_then(|e| e.get("rspCode"))
                .and_then(|v| v.as_i64());
            let detail = error
                .and_then(|e| e.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("unexpected JSON response");
            // Reflected URLs can contain encoded credentials; omit them entirely.
            let detail = if detail.contains("://") || detail.contains("password=") {
                "camera returned an error containing a URL (detail omitted)".to_owned()
            } else {
                self.secrets.redact(detail)
            };
            bail!(
                "camera API error (HTTP {}, code {:?}): {}",
                status.as_u16(),
                code,
                detail
            );
        }
        ensure!(
            status.is_success(),
            "camera returned HTTP {}",
            status.as_u16()
        );
        ensure!(
            image::guess_format(bytes).ok() == Some(image::ImageFormat::Jpeg),
            "camera response is not a JPEG"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_encodes_credentials_and_rejects_injected_host() {
        let mut config = CameraConfig {
            host: "127.0.0.1:443".into(),
            channel: 1,
            timeout_seconds: 1,
            accept_invalid_certs: false,
        };
        let secrets = Secrets {
            rtsp_user: "test-user".into(),
            rtsp_pw: "test &?/#% password".into(),
            ..Default::default()
        };
        let url = Camera {
            config: &config,
            secrets: &secrets,
        }
        .url()
        .unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "password").unwrap().1,
            secrets.rtsp_pw
        );
        config.host = "example.com/path?password=oops".into();
        assert!(
            Camera {
                config: &config,
                secrets: &secrets
            }
            .url()
            .is_err()
        );
    }
}

#[cfg(test)]
mod response_tests {
    use super::*;
    #[test]
    fn api_errors_are_useful_without_leaking_credentials() {
        let config = CameraConfig {
            host: "127.0.0.1".into(),
            channel: 1,
            timeout_seconds: 1,
            accept_invalid_certs: false,
        };
        let secrets = Secrets {
            rtsp_user: "fixture-user".into(),
            rtsp_pw: "fixture-\"password".into(),
            ..Default::default()
        };
        let camera = Camera {
            config: &config,
            secrets: &secrets,
        };
        for detail in [
            format!("invalid login {}", secrets.rtsp_pw),
            "https://host/?password=encoded%22password".into(),
        ] {
            let bytes =
                serde_json::to_vec(&serde_json::json!([{"error":{"rspCode":-6,"detail":detail}}]))
                    .unwrap();
            let error = camera
                .validate_response(reqwest::StatusCode::OK, &bytes)
                .unwrap_err()
                .to_string();
            assert!(error.contains("-6"));
            assert!(!error.contains(&secrets.rtsp_pw));
            assert!(!error.contains("https://"));
            assert!(!error.contains("encoded%22password"));
        }
        assert!(
            camera
                .validate_response(reqwest::StatusCode::UNAUTHORIZED, b"unauthorized")
                .unwrap_err()
                .to_string()
                .contains("401")
        );
        assert!(
            camera
                .validate_response(reqwest::StatusCode::OK, b"not a jpeg")
                .is_err()
        );
    }
    #[test]
    fn transport_errors_strip_authenticated_url() {
        // Unsupported schemes fail before any connection, but still attach a URL.
        let failure = Client::new()
            .get("ftp://fixture-user:fixture-password@127.0.0.1/")
            .send()
            .unwrap_err();
        assert!(failure.url().is_some());
        let clean = failure.without_url().to_string();
        assert!(!clean.contains("fixture-password"));
        assert!(!clean.contains("fixture-user"));
    }
}
