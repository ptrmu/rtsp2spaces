use crate::secrets::Secrets;
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Duration, Utc};
use reqwest::{blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration as Timeout,
};

#[derive(Deserialize)]
pub struct TemperatureConfig {
    pub source: String,
    pub mac_address: Option<String>,
    pub fixture_file: Option<PathBuf>,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "interval")]
    pub sampling_interval_minutes: u32,
}
fn timeout() -> u64 {
    15
}
fn interval() -> u32 {
    5
}
impl TemperatureConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.timeout_seconds > 0 && self.sampling_interval_minutes > 0,
            "temperature timeout and sampling interval must be positive"
        );
        match self.source.as_str() {
            "ambient" => {
                let mac = self.mac_address.as_deref().unwrap_or("");
                ensure!(
                    mac.len() == 17
                        && mac.split(':').count() == 6
                        && mac
                            .split(':')
                            .all(|part| part.len() == 2
                                && part.bytes().all(|c| c.is_ascii_hexdigit())),
                    "temperature mac_address must be a colon-separated MAC address"
                );
            }
            "fixture" => ensure!(
                self.fixture_file.is_some(),
                "fixture source requires fixture_file"
            ),
            _ => bail!("temperature source must be ambient or fixture"),
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Reading {
    pub source: String,
    pub observed_at: DateTime<Utc>,
    pub collected_at: DateTime<Utc>,
    pub celsius: f64,
}
pub trait TemperatureSource {
    fn readings(&self, now: DateTime<Utc>, history_hours: u32) -> Result<Vec<Reading>>;
}
pub struct Ambient<'a> {
    pub config: &'a TemperatureConfig,
    pub secrets: &'a Secrets,
}
#[derive(Deserialize)]
struct Sample {
    dateutc: i64,
    tempf: Option<f64>,
}
fn normalize(sample: Sample, source: &str, now: DateTime<Utc>) -> Result<Reading> {
    let fahrenheit = sample
        .tempf
        .context("Ambient reading is missing outdoor temperature tempf")?;
    let celsius = (fahrenheit - 32.0) * 5.0 / 9.0;
    ensure!(celsius.is_finite(), "temperature must be finite");
    Ok(Reading {
        source: source.to_owned(),
        observed_at: DateTime::from_timestamp_millis(sample.dateutc)
            .context("invalid observation timestamp")?,
        collected_at: now,
        celsius,
    })
}
impl TemperatureSource for Ambient<'_> {
    fn readings(&self, now: DateTime<Utc>, history_hours: u32) -> Result<Vec<Reading>> {
        self.config.validate()?;
        let api = self
            .secrets
            .ambient_api_key
            .as_deref()
            .filter(|v| !v.is_empty())
            .context("missing ambient_api_key")?;
        let app = self
            .secrets
            .ambient_application_key
            .as_deref()
            .filter(|v| !v.is_empty())
            .context("missing ambient_application_key")?;
        let mac = self
            .config
            .mac_address
            .as_deref()
            .context("missing temperature mac_address")?;
        let source = format!("ambient:{}", mac.to_uppercase());
        let client = Client::builder()
            .timeout(Timeout::from_secs(self.config.timeout_seconds))
            .redirect(Policy::none())
            .build()
            .map_err(|e| e.without_url())?;
        let fetch = |path: &str, extra: &[(&str, String)]| -> Result<serde_json::Value> {
            let response = client
                .get(format!("https://rt.ambientweather.net/v1/{path}"))
                .query(&[("apiKey", api), ("applicationKey", app)])
                .query(extra)
                .send()
                .map_err(|e| e.without_url())
                .context("Ambient request failed")?;
            ensure!(
                response.status().is_success(),
                "Ambient API returned HTTP {}",
                response.status().as_u16()
            );
            let mut bytes = Vec::new();
            response
                .take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| anyhow::anyhow!("cannot read Ambient response"))?;
            ensure!(
                bytes.len() <= 4 * 1024 * 1024,
                "Ambient response exceeds 4 MiB"
            );
            serde_json::from_slice(&bytes)
                .map_err(|_| anyhow::anyhow!("invalid Ambient JSON response"))
        };
        let devices = fetch("devices", &[])?;
        let device = devices
            .as_array()
            .context("invalid Ambient device list")?
            .iter()
            .find(|d| {
                d.get("macAddress")
                    .and_then(|v| v.as_str())
                    .is_some_and(|v| v.eq_ignore_ascii_case(mac))
            })
            .context("configured station not found in Ambient account")?;
        let latest: Sample = serde_json::from_value(
            device
                .get("lastData")
                .context("Ambient station has no latest reading")?
                .clone(),
        )
        .map_err(|_| anyhow::anyhow!("invalid Ambient latest reading"))?;
        let mut readings = vec![normalize(latest, &source, now)?];
        if history_hours > 0 {
            let start = now - Duration::hours(i64::from(history_hours));
            let mut end = now.timestamp_millis();
            loop {
                // Ambient limits each account to one request per second.
                thread::sleep(Timeout::from_millis(1100));
                let data = fetch(
                    &format!("devices/{mac}"),
                    &[("limit", "288".into()), ("endDate", end.to_string())],
                )?;
                let samples: Vec<Sample> = serde_json::from_value(data)
                    .map_err(|_| anyhow::anyhow!("invalid Ambient history response"))?;
                if samples.is_empty() {
                    break;
                }
                let oldest = samples
                    .iter()
                    .map(|s| s.dateutc)
                    .min()
                    .context("empty history page")?;
                ensure!(oldest <= end, "Ambient history pagination did not advance");
                for sample in samples {
                    let reading = normalize(sample, &source, now)?;
                    if reading.observed_at >= start && reading.observed_at <= now {
                        readings.push(reading);
                    }
                }
                if oldest <= start.timestamp_millis() {
                    break;
                }
                end = oldest.checked_sub(1).context("invalid history timestamp")?;
            }
        }
        Ok(readings)
    }
}
pub struct FixtureSource(pub PathBuf);
impl TemperatureSource for FixtureSource {
    fn readings(&self, now: DateTime<Utc>, hours: u32) -> Result<Vec<Reading>> {
        let bytes = fs::read(&self.0).context("cannot read temperature fixture")?;
        let mut readings: Vec<Reading> = serde_json::from_slice(&bytes).map_err(|_| {
            anyhow::anyhow!(
                "invalid temperature fixture (expected JSON array of normalized readings)"
            )
        })?;
        readings.retain(|r| {
            hours == 0
                || (r.observed_at >= now - Duration::hours(i64::from(hours))
                    && r.observed_at <= now)
        });
        if hours == 0 {
            readings.sort_by_key(|r| r.observed_at);
            readings = readings.into_iter().rev().take(1).collect();
        }
        for reading in &mut readings {
            reading.collected_at = now;
        }
        ensure!(
            !readings.is_empty(),
            "temperature fixture has no matching readings"
        );
        Ok(readings)
    }
}
pub fn read_history(path: &Path) -> Result<Vec<Reading>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).context("cannot read temperature history"),
    };
    let mut readings = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.context("cannot read history line")?;
        match serde_json::from_str::<Reading>(&line) {
            Ok(reading) if reading.celsius.is_finite() => readings.push(reading),
            _ => log::warn!("skipping malformed temperature history line"),
        }
    }
    Ok(readings)
}
pub fn collect(
    source: &impl TemperatureSource,
    directory: &Path,
    now: DateTime<Utc>,
    hours: u32,
) -> Result<(usize, Vec<PathBuf>)> {
    ensure!(hours <= 24, "history_hours must be at most 24");
    let mut readings = source.readings(now, hours)?;
    ensure!(
        !readings.is_empty(),
        "temperature source returned no readings"
    );
    ensure!(
        readings
            .iter()
            .all(|r| r.celsius.is_finite() && !r.source.is_empty()),
        "invalid temperature reading"
    );
    readings.sort_by_key(|r| r.observed_at);
    let directory = directory.join("temperature");
    fs::create_dir_all(&directory).context("cannot create temperature directory")?;
    let mut count = 0;
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for reading in readings {
        let path = directory.join(format!("{}.jsonl", reading.observed_at.format("%Y-%m-%d")));
        if !paths.contains(&path) {
            for previous in read_history(&path)? {
                seen.insert((previous.source, previous.observed_at));
            }
            paths.push(path.clone());
        }
        if !seen.insert((reading.source.clone(), reading.observed_at)) {
            continue;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .context("cannot open temperature history for append")?;
        // Write celsius as a JSON number with exactly two fractional digits.
        let mut fields = serde_json::to_value(&reading).context("cannot serialize temperature")?;
        fields
            .as_object_mut()
            .context("invalid temperature record")?
            .remove("celsius");
        let mut json = serde_json::to_string(&fields).context("cannot serialize temperature")?;
        json.pop(); // Closing brace of the serialized object.
        let mut line = format!("{json},\"celsius\":{:.2}}}", reading.celsius).into_bytes();
        // Separate a partial final line left by an interrupted append.
        use std::io::{Seek, SeekFrom};
        if file.metadata()?.len() > 0 {
            file.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                line.insert(0, b'\n');
            }
        }
        line.push(b'\n');
        let written = file
            .write(&line)
            .context("cannot append temperature reading")?;
        ensure!(
            written == line.len(),
            "partial temperature append; retry collection"
        );
        count += 1;
    }
    log::info!("saved {count} new temperature readings");
    Ok((count, paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    struct Fake(Vec<Reading>);
    impl TemperatureSource for Fake {
        fn readings(&self, _: DateTime<Utc>, _: u32) -> Result<Vec<Reading>> {
            Ok(self.0.clone())
        }
    }
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 9, 0, 5, 0).unwrap()
    }
    fn reading(at: DateTime<Utc>) -> Reading {
        Reading {
            source: "fixture".into(),
            observed_at: at,
            collected_at: now(),
            celsius: -1.5,
        }
    }
    #[test]
    fn converts_fahrenheit_and_rejects_missing_invalid_values() {
        for (f, c) in [(32.0, 0.0), (-40.0, -40.0), (44.24, 6.8)] {
            let r = normalize(
                Sample {
                    dateutc: now().timestamp_millis(),
                    tempf: Some(f),
                },
                "fixture",
                now(),
            )
            .unwrap();
            assert!((r.celsius - c).abs() < 1e-10);
            assert_eq!(r.observed_at, now());
        }
        for tempf in [None, Some(f64::NAN), Some(f64::INFINITY)] {
            assert!(normalize(Sample { dateutc: 0, tempf }, "fixture", now()).is_err());
        }
    }
    #[test]
    fn persists_across_restart_midnight_duplicates_and_truncated_tail() {
        let temp = tempfile::tempdir().unwrap();
        let a = reading(now() - Duration::minutes(10));
        let b = reading(now());
        let fake = Fake(vec![b.clone(), a.clone(), b.clone()]);
        let (count, paths) = collect(&fake, temp.path(), now(), 24).unwrap();
        assert_eq!(count, 2);
        let stored = fs::read_to_string(&paths[0]).unwrap();
        assert!(stored.contains("\"celsius\":-1.50"));
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("2026-11-08.jsonl"));
        assert_eq!(collect(&fake, temp.path(), now(), 24).unwrap().0, 0);
        let last = &paths[1];
        OpenOptions::new()
            .append(true)
            .open(last)
            .unwrap()
            .write_all(b"{broken")
            .unwrap();
        let c = reading(now() + Duration::minutes(1));
        assert_eq!(collect(&Fake(vec![c]), temp.path(), now(), 0).unwrap().0, 1);
        assert_eq!(read_history(last).unwrap().len(), 2);
        assert_eq!(collect(&fake, temp.path(), now(), 24).unwrap().0, 0);
    }
    #[test]
    fn invalid_or_failed_reading_does_not_create_history() {
        let temp = tempfile::tempdir().unwrap();
        let mut invalid = reading(now());
        invalid.celsius = f64::NAN;
        assert!(collect(&Fake(vec![invalid]), temp.path(), now(), 0).is_err());
        struct Failed;
        impl TemperatureSource for Failed {
            fn readings(&self, _: DateTime<Utc>, _: u32) -> Result<Vec<Reading>> {
                bail!("sensor unavailable")
            }
        }
        assert!(collect(&Failed, temp.path(), now(), 0).is_err());
        assert!(!temp.path().join("temperature").exists());
    }
    #[test]
    fn fixture_window_and_output_failure() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fixture.json");
        fs::write(
            &path,
            serde_json::to_vec(&vec![
                reading(now() - Duration::hours(25)),
                reading(now() - Duration::hours(1)),
                reading(now()),
            ])
            .unwrap(),
        )
        .unwrap();
        let fixture = FixtureSource(path);
        assert_eq!(fixture.readings(now(), 24).unwrap().len(), 2);
        assert_eq!(fixture.readings(now(), 0).unwrap().len(), 1);
        let blocked = temp.path().join("blocked");
        fs::write(&blocked, "file").unwrap();
        assert!(collect(&fixture, &blocked, now(), 0).is_err());
    }
}
