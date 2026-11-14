use crate::temperature::{self, Reading};
use anyhow::{Result, ensure};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use image::{Rgb, RgbImage};
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct GraphConfig {
    pub height_pixels: u32,
    pub inset_fraction: f64,
    pub above_freezing_color: [u8; 3],
    pub below_freezing_color: [u8; 3],
    pub freezing_color: [u8; 3],
    pub night_axis_color: [u8; 3],
    pub latitude: f64,
    pub longitude: f64,
    pub thickness: u32,
}
impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            height_pixels: 130,
            inset_fraction: 0.03,
            above_freezing_color: [255, 0, 0],
            below_freezing_color: [0, 160, 255],
            freezing_color: [255, 255, 255],
            night_axis_color: [192, 192, 192],
            latitude: 48.5933,
            longitude: -120.4058,
            thickness: 2,
        }
    }
}
impl GraphConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.height_pixels >= 2
                && self.height_pixels <= 8192
                && self.inset_fraction.is_finite()
                && self.inset_fraction >= 0.0
                && self.inset_fraction < 0.5,
            "graph height must be 2–8192 pixels and inset must be in [0, 0.5)"
        );
        ensure!(
            (1..=10).contains(&self.thickness),
            "graph thickness must be 1–10 pixels"
        );
        ensure!(
            self.latitude.is_finite()
                && (-90.0..=90.0).contains(&self.latitude)
                && self.longitude.is_finite()
                && (-180.0..=180.0).contains(&self.longitude),
            "invalid graph latitude or longitude"
        );
        Ok(())
    }
}

pub fn history(directory: &Path, at: DateTime<Utc>) -> Result<Vec<Reading>> {
    let start = at - Duration::hours(48);
    let mut day = start.date_naive();
    let mut readings = Vec::new();
    while day <= at.date_naive() {
        readings.extend(temperature::read_history(
            &directory.join("temperature").join(format!("{day}.jsonl")),
        )?);
        day = day.succ_opt().expect("48-hour window has a following date");
    }
    Ok(readings)
}

/// Draw directly on cropped pixels, before JPEG compression. Times are UTC.
pub fn draw(
    pixels: &mut RgbImage,
    readings: &[Reading],
    at: DateTime<Utc>,
    interval_minutes: u32,
    config: &GraphConfig,
) -> Result<()> {
    config.validate()?;
    ensure!(interval_minutes > 0, "sampling interval must be positive");
    let start = at - Duration::hours(48);
    let mut samples: Vec<_> = readings
        .iter()
        .filter(|r| r.celsius.is_finite() && r.observed_at >= start && r.observed_at <= at)
        .collect();
    samples.sort_by_key(|r| r.observed_at);
    samples.dedup_by_key(|r| r.observed_at);
    if samples.is_empty() || pixels.width() < 2 || pixels.height() < 2 {
        return Ok(());
    }
    let right = f64::from(pixels.width() - 1);
    let bottom = f64::from(pixels.height() - 1);
    let left = (right * config.inset_fraction).round() as i32;
    let right = (right * (1.0 - config.inset_fraction)).round() as i32;
    let bottom = (bottom * (1.0 - config.inset_fraction)).round() as i32;
    // Clamp only when the image cannot fit the requested fixed height.
    // Account for the stroke extending above/left of each rasterized point.
    let stroke_offset = config.thickness as i32 / 2;
    let bottom = bottom.max(stroke_offset);
    let top = (bottom - config.height_pixels as i32 + 1 + stroke_offset).max(stroke_offset);
    let min = samples.iter().map(|r| r.celsius).fold(0.0, f64::min);
    let max = samples.iter().map(|r| r.celsius).fold(0.0, f64::max);
    let span = ((max - min) * 1.2).max(10.0);
    let low = (min + max - span) / 2.0;
    let y = |c: f64| bottom - (((c - low) / span) * f64::from(bottom - top)).round() as i32;
    for x in left..=right {
        let fraction = f64::from(x - left) / f64::from((right - left).max(1));
        let time = start + Duration::milliseconds((fraction * 172_800_000.0).round() as i64);
        let color = if daylight(time, config.latitude, config.longitude) {
            config.freezing_color
        } else {
            config.night_axis_color
        };
        line(pixels, (x, y(0.0)), (x, y(0.0)), color, config.thickness);
    }
    line(
        pixels,
        (right, top),
        (right, bottom),
        config.freezing_color,
        config.thickness,
    );
    let point = |r: &Reading| {
        (
            left + ((r.observed_at - start).num_milliseconds() as f64 / 172_800_000.0
                * f64::from(right - left))
            .round() as i32,
            y(r.celsius),
        )
    };
    for pair in samples.windows(2) {
        if pair[0].source == pair[1].source
            && pair[1].observed_at - pair[0].observed_at
                <= Duration::minutes(i64::from(interval_minutes) * 3)
        {
            let a = point(pair[0]);
            let b = point(pair[1]);
            let color = |c: f64| {
                if c >= 0.0 {
                    config.above_freezing_color
                } else {
                    config.below_freezing_color
                }
            };
            if (pair[0].celsius < 0.0 && pair[1].celsius > 0.0)
                || (pair[0].celsius > 0.0 && pair[1].celsius < 0.0)
            {
                let fraction = -pair[0].celsius / (pair[1].celsius - pair[0].celsius);
                let crossing = (
                    a.0 + (fraction * f64::from(b.0 - a.0)).round() as i32,
                    y(0.0),
                );
                line(
                    pixels,
                    a,
                    crossing,
                    color(pair[0].celsius),
                    config.thickness,
                );
                line(
                    pixels,
                    crossing,
                    b,
                    color(pair[1].celsius),
                    config.thickness,
                );
            } else {
                let temperature = if pair[0].celsius == 0.0 {
                    pair[1].celsius
                } else {
                    pair[0].celsius
                };
                line(pixels, a, b, color(temperature), config.thickness);
            }
        }
    }
    Ok(())
}

/// NOAA solar-position approximation; sunrise/sunset use a 90.833° zenith
/// (solar disk and atmospheric refraction), rather than civil twilight.
/// https://gml.noaa.gov/grad/solcalc/solareqns.PDF
/// UTC calculation naturally handles local midnight and daylight saving time.
fn daylight(at: DateTime<Utc>, latitude: f64, longitude: f64) -> bool {
    let days = if at.date_naive().leap_year() {
        366.0
    } else {
        365.0
    };
    let hour = f64::from(at.num_seconds_from_midnight()) / 3600.0;
    let gamma = std::f64::consts::TAU / days * (f64::from(at.ordinal0()) + (hour - 12.0) / 24.0);
    let equation = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let declination = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    let solar_minutes = (hour * 60.0 + equation + 4.0 * longitude).rem_euclid(1440.0);
    let angle = (solar_minutes / 4.0 - 180.0).to_radians();
    let latitude = latitude.to_radians();
    let cos_zenith =
        latitude.sin() * declination.sin() + latitude.cos() * declination.cos() * angle.cos();
    cos_zenith >= 90.833_f64.to_radians().cos()
}

fn line(
    image: &mut RgbImage,
    (mut x, mut y): (i32, i32),
    (end_x, end_y): (i32, i32),
    color: [u8; 3],
    thickness: u32,
) {
    let dx = (end_x - x).abs();
    let dy = -(end_y - y).abs();
    let sx = if x < end_x { 1 } else { -1 };
    let sy = if y < end_y { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        for ox in 0..thickness as i32 {
            for oy in 0..thickness as i32 {
                let px = x + ox - (thickness as i32 / 2);
                let py = y + oy - (thickness as i32 / 2);
                if px >= 0 && py >= 0 && px < image.width() as i32 && py < image.height() as i32 {
                    image.put_pixel(px as u32, py as u32, Rgb(color));
                }
            }
        }
        if x == end_x && y == end_y {
            break;
        }
        let twice = error * 2;
        if twice >= dy {
            error += dy;
            x += sx;
        }
        if twice <= dx {
            error += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn mazama_sunrise_sunset_and_seasons() {
        let config = GraphConfig::default();
        // Winter sunrise falls between 07:00 and 08:00 PST; sunset
        // between 16:00 and 17:00 PST (00:00–01:00 UTC the next day).
        for (month, day, hour, expected) in [
            (12, 21, 15, false),
            (12, 21, 16, true),
            (12, 22, 0, true),
            (12, 22, 1, false),
            // Summer sunrise: 04:00–06:00 PDT; sunset: 21:00–22:00 PDT.
            (6, 21, 11, false),
            (6, 21, 13, true),
            (6, 22, 4, true),
            (6, 22, 5, false),
        ] {
            let at = Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0).unwrap();
            assert_eq!(
                daylight(at, config.latitude, config.longitude),
                expected,
                "{at}"
            );
        }
        // Leap day and both US daylight-saving change dates need no timezone offsets.
        for (year, month, day) in [(2024, 2, 29), (2026, 3, 8), (2026, 11, 1)] {
            assert!(daylight(
                Utc.with_ymd_and_hms(year, month, day, 20, 0, 0).unwrap(),
                config.latitude,
                config.longitude
            ));
            assert!(!daylight(
                Utc.with_ymd_and_hms(year, month, day, 8, 0, 0).unwrap(),
                config.latitude,
                config.longitude
            ));
        }
    }

    #[test]
    fn two_day_window_reads_three_daily_files_and_maps_oldest_samples_left() {
        let at = Utc.with_ymd_and_hms(2026, 11, 8, 12, 0, 0).unwrap();
        let readings: Vec<_> = [2881, 2880, 2875, 1440, 0]
            .into_iter()
            .map(|minutes| Reading {
                source: "fixture".into(),
                observed_at: at - Duration::minutes(minutes),
                collected_at: at,
                celsius: 2.0,
            })
            .collect();
        let directory = tempfile::tempdir().unwrap();
        let (_, paths) = temperature::persist(readings, directory.path()).unwrap();
        assert_eq!(paths.len(), 3);
        let stored = history(directory.path(), at).unwrap();
        let mut image = RgbImage::new(1000, 1000);
        let config = GraphConfig::default();
        draw(&mut image, &stored, at, 5, &config).unwrap();
        // The first five minutes of the 48-hour window belong at the left edge.
        assert!((0..1000).any(|y| image.get_pixel(30, y).0 == config.above_freezing_color));
        assert!((0..1000).all(|y| image.get_pixel(28, y).0 == [0, 0, 0]));
        assert_eq!(image.get_pixel(499, 918).0, config.night_axis_color);
        // No midpoint tick above or below the horizontal axis.
        assert_eq!(image.get_pixel(499, 915).0, [0, 0, 0]);
        assert_eq!(image.get_pixel(499, 921).0, [0, 0, 0]);
        assert!(image.pixels().any(|p| p.0 == config.night_axis_color));
    }

    #[test]
    fn fixed_height_axis_and_two_pixel_strokes() {
        let at = Utc.with_ymd_and_hms(2026, 11, 8, 12, 0, 0).unwrap();
        let config = GraphConfig::default();
        let readings = vec![Reading {
            source: "fixture".into(),
            observed_at: at,
            collected_at: at,
            celsius: 2.0,
        }];
        for height in [400, 1000, 1600] {
            let mut image = RgbImage::new(1000, height);
            draw(&mut image, &readings, at, 5, &config).unwrap();
            let axis: Vec<_> = (0..height)
                .filter(|y| image.get_pixel(969, *y).0 == config.freezing_color)
                .collect();
            assert_eq!(axis.len(), 130);
            for y in axis {
                assert_eq!(image.get_pixel(968, y).0, config.freezing_color);
                assert_eq!(image.get_pixel(970, y).0, [0, 0, 0]);
            }
        }
    }

    #[test]
    fn geometry_freezing_range_gaps_and_missing_data() {
        let at = Utc.with_ymd_and_hms(2026, 11, 8, 12, 0, 0).unwrap();
        let config = GraphConfig::default();
        for values in [[2.0, 8.0], [-8.0, -2.0], [-2.0, 2.0], [3.0, 3.0]] {
            let readings: Vec<_> = values
                .into_iter()
                .enumerate()
                .map(|(i, celsius)| Reading {
                    source: "fixture".into(),
                    observed_at: at - Duration::minutes(10 - i as i64 * 5),
                    collected_at: at,
                    celsius,
                })
                .collect();
            let mut image = RgbImage::new(1000, 1000);
            draw(&mut image, &readings, at, 5, &config).unwrap();
            assert!(
                image
                    .pixels()
                    .any(|p| p.0 == config.above_freezing_color
                        || p.0 == config.below_freezing_color)
            );
            if values.iter().any(|c| *c > 0.0) {
                assert!(image.pixels().any(|p| p.0 == config.above_freezing_color));
            }
            if values.iter().any(|c| *c < 0.0) {
                assert!(image.pixels().any(|p| p.0 == config.below_freezing_color));
            }
            assert!(image.pixels().any(|p| p.0 == config.freezing_color));
            assert!(
                image
                    .enumerate_pixels()
                    .filter(|(_, _, p)| p.0 != [0, 0, 0])
                    .all(|(x, y, _)| (29..=969).contains(&x) && (840..=969).contains(&y))
            );
            let mut sparse = readings.clone();
            sparse[0].observed_at = at - Duration::hours(48);
            let mut image = RgbImage::new(1000, 1000);
            draw(&mut image, &sparse, at, 5, &config).unwrap();
            assert!(
                !image
                    .pixels()
                    .any(|p| p.0 == config.above_freezing_color
                        || p.0 == config.below_freezing_color)
            );
        }
        let mut image = RgbImage::new(100, 100);
        draw(&mut image, &[], at, 5, &config).unwrap();
        assert!(image.pixels().all(|p| p.0 == [0, 0, 0]));
    }
}
