use crate::{
    config::{CameraConfig, CameraId, Source},
    secrets::{CameraSecrets, Secrets},
};
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use reqwest::{Client, Url, redirect::Policy};
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

pub const MAX_JPEG_BYTES: usize = 32 * 1024 * 1024;
pub struct FetchedImage {
    pub camera: CameraId,
    pub captured_at: DateTime<Utc>,
    pub jpeg: Vec<u8>,
}

pub fn snapshot_url(config: &CameraConfig) -> Result<Url> {
    let host = config
        .host
        .as_deref()
        .context("missing HTTPS camera host")?;
    let url = Url::parse(&format!("https://{host}/cgi-bin/api.cgi"))
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
    Ok(url)
}
pub async fn bounded_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        anyhow::anyhow!("cannot read response (connection interrupted or timed out)")
    })? {
        ensure!(
            chunk.len() <= limit.saturating_sub(bytes.len()),
            "response exceeds payload limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub fn validate_jpeg(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= MAX_JPEG_BYTES,
        "camera response exceeds 32 MiB"
    );
    ensure!(
        image::guess_format(bytes).ok() == Some(image::ImageFormat::Jpeg),
        "camera response is not a JPEG"
    );
    Ok(())
}
pub async fn fetch(id: CameraId, config: &CameraConfig, secrets: &Secrets) -> Result<FetchedImage> {
    let credentials = secrets.camera(id)?;
    match config.source {
        Source::Https => {
            let client = Client::builder()
                .timeout(Duration::from_secs(config.timeout_seconds))
                .danger_accept_invalid_certs(config.accept_invalid_certs)
                .redirect(Policy::none())
                .build()
                .map_err(|e| e.without_url())
                .context("cannot build camera client")?;
            let mut url = snapshot_url(config)?;
            url.query_pairs_mut().extend_pairs([
                ("cmd", "Snap".to_owned()),
                ("channel", config.channel.to_string()),
                ("rs", rand::random::<u64>().to_string()),
                ("user", credentials.username.clone()),
                ("password", credentials.password.clone()),
            ]);
            let captured_at = Utc::now();
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|e| e.without_url())
                .context("camera request failed")?;
            let status = response.status();
            let jpeg = bounded_body(response, MAX_JPEG_BYTES).await?;
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&jpeg) {
                let code = json
                    .pointer("/0/error/rspCode")
                    .or_else(|| json.pointer("/error/rspCode"))
                    .and_then(|v| v.as_i64());
                bail!(
                    "camera API error (HTTP {}, code {:?})",
                    status.as_u16(),
                    code
                );
            }
            ensure!(
                status.is_success(),
                "camera returned HTTP {}",
                status.as_u16()
            );
            validate_jpeg(&jpeg)?;
            Ok(FetchedImage {
                camera: id,
                captured_at,
                jpeg,
            })
        }
        Source::ReolinkUid => uid_fetch(id, config, credentials).await,
    }
}
async fn uid_fetch(id: CameraId, config: &CameraConfig, s: &CameraSecrets) -> Result<FetchedImage> {
    use neolink_core::bc_protocol::{
        BcCamera, BcCameraOpt, ConnectionProtocol, Credentials, DiscoveryMethods,
    };
    let deadline = Instant::now() + Duration::from_secs(config.timeout_seconds);
    // Reserve a small part of the total deadline for logout and shutdown.
    let cleanup_budget =
        Duration::from_secs(2).min(Duration::from_secs(config.timeout_seconds) / 4);
    let work_deadline = deadline - cleanup_budget;
    let options = BcCameraOpt {
        name: id.to_string(),
        channel_id: config.channel,
        addrs: vec![],
        uid: s.uid.clone(),
        port: None,
        protocol: ConnectionProtocol::Udp,
        discovery: DiscoveryMethods::Relay,
        max_discovery_retries: 0,
        credentials: Credentials {
            username: s.username.clone(),
            password: Some(s.password.clone()),
        },
        debug: false,
    };
    let camera = timeout_at(work_deadline, BcCamera::new(&options))
        .await
        .context("UID connection timed out")?
        .map_err(|_| anyhow::anyhow!("UID connection failed"))?;
    let result = capture_session(&camera, id, work_deadline, deadline, cleanup_budget).await;
    // The fork's drop implementations abort remaining owned network tasks.
    drop(camera);
    result
}

// Keep session sequencing replaceable so timeout and cleanup behavior needs no real camera.
trait UidSession {
    async fn login(&self) -> Result<()>;
    async fn snapshot(&self) -> Result<Vec<u8>>;
    async fn logout(&self) -> Result<()>;
    async fn shutdown(&self) -> Result<()>;
}
impl UidSession for neolink_core::bc_protocol::BcCamera {
    async fn login(&self) -> Result<()> {
        self.login()
            .await
            .map(|_| ())
            .map_err(|_| anyhow::anyhow!("UID login failed"))
    }
    async fn snapshot(&self) -> Result<Vec<u8>> {
        self.get_snapshot()
            .await
            .map_err(|_| anyhow::anyhow!("UID snapshot failed"))
    }
    async fn logout(&self) -> Result<()> {
        self.logout()
            .await
            .map_err(|_| anyhow::anyhow!("UID logout failed"))
    }
    async fn shutdown(&self) -> Result<()> {
        self.shutdown()
            .await
            .map_err(|_| anyhow::anyhow!("UID shutdown failed"))
    }
}
async fn capture_session(
    camera: &impl UidSession,
    id: CameraId,
    work_deadline: Instant,
    deadline: Instant,
    cleanup_budget: Duration,
) -> Result<FetchedImage> {
    let result = async {
        timeout_at(work_deadline, UidSession::login(camera))
            .await
            .context("UID login timed out")??;
        let captured_at = Utc::now();
        let jpeg = timeout_at(work_deadline, camera.snapshot())
            .await
            .context("UID snapshot timed out")??;
        validate_jpeg(&jpeg)?;
        Ok(FetchedImage {
            camera: id,
            captured_at,
            jpeg,
        })
    }
    .await;
    // Logout/shutdown failures must not replace an otherwise successful image.
    let logout_deadline = (Instant::now() + cleanup_budget / 2).min(deadline);
    let _ = timeout_at(logout_deadline, UidSession::logout(camera)).await;
    let _ = timeout_at(deadline, UidSession::shutdown(camera)).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Session {
        fail: bool,
        hang_snapshot: bool,
        hang_cleanup: bool,
        logged_out: AtomicBool,
        shutdown: AtomicBool,
        cancelled: AtomicBool,
    }
    struct CancelGuard<'a>(&'a AtomicBool);
    impl Drop for CancelGuard<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    impl UidSession for Session {
        async fn login(&self) -> Result<()> {
            Ok(())
        }
        async fn snapshot(&self) -> Result<Vec<u8>> {
            let _guard = CancelGuard(&self.cancelled);
            if self.hang_snapshot {
                std::future::pending::<()>().await;
            }
            ensure!(!self.fail, "simulated snapshot failure");
            Ok(vec![0xff, 0xd8, 0xff, 0xe0])
        }
        async fn logout(&self) -> Result<()> {
            self.logged_out.store(true, Ordering::SeqCst);
            if self.hang_cleanup {
                std::future::pending::<()>().await;
            }
            Ok(())
        }
        async fn shutdown(&self) -> Result<()> {
            self.shutdown.store(true, Ordering::SeqCst);
            if self.hang_cleanup {
                std::future::pending::<()>().await;
            }
            Ok(())
        }
    }
    #[tokio::test]
    async fn uid_cleanup_after_success_failure_and_timeout_is_bounded() {
        for (fail, hang_snapshot, hang_cleanup) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let session = Session {
                fail,
                hang_snapshot,
                hang_cleanup,
                logged_out: AtomicBool::new(false),
                shutdown: AtomicBool::new(false),
                cancelled: AtomicBool::new(false),
            };
            let now = Instant::now();
            let result = tokio::time::timeout(
                Duration::from_secs(1),
                capture_session(
                    &session,
                    CameraId::MazamaRanchCam,
                    now + Duration::from_millis(20),
                    now + Duration::from_millis(60),
                    Duration::from_millis(40),
                ),
            )
            .await
            .expect("operation must not hang");
            assert_eq!(result.is_ok(), !fail && !hang_snapshot);
            assert!(session.logged_out.load(Ordering::SeqCst));
            assert!(session.shutdown.load(Ordering::SeqCst));
            assert!(session.cancelled.load(Ordering::SeqCst));
        }
    }
}
