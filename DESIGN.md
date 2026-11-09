# Stakecam design

## Implementation status and development environment

This document specifies the planned Rust implementation. Phase 1 capture is implemented and verified with automated tests and live-camera acceptance on 2026-11-08 (see README.md). Later commands, configuration sections, and deployment instructions below remain design targets. See README.md for the available capture command and its configuration defaults.

See [README.md](README.md#development) for the committed Dev Container, setup script, current toolchain observations, and verified development commands. Cross-compilation and Pi deployment are Phase 5 work.

## Architecture and conventions

Use one Rust 2024 Cargo package with a thin executable and focused library modules for configuration, secrets, camera capture, temperature collection, history, drawing, and Spaces uploads.

Use typed configuration, `Result` errors, contextual diagnostics, and replaceable interfaces for external services. Keep production code free of unchecked `unwrap()` calls.

Development is incremental: each phase adds one working command, and the phases are built in order. Matching the Python implementation is not a goal. The only compatibility requirement is the public metadata and remote object names the website reads (Phase 4).

### Commands

```sh
stakecam capture     --config config.toml --secrets-file secrets.json
stakecam temperature --config config.toml --secrets-file secrets.json
stakecam render      --config config.toml --input image.jpg --output annotated.jpg [--at <RFC3339-time>]
stakecam upload      --config config.toml --secrets-file secrets.json --input annotated.jpg [--at <RFC3339-time>]
stakecam run         --config config.toml --secrets-file secrets.json
```

`run` performs one complete cycle: capture, temperature, render, and upload. It passes the files and the exact capture time between steps in memory, so the user never names a capture. The individual commands exist for development, testing, and recovery; they take the capture time from the image filename, or from `--at` when given.

Each command runs once and exits. Scheduling and retention of local files are managed outside the application.

### Configuration

Define arguments with `clap` derive, using kebab-case flags and generated help. The command line carries only file paths and per-invocation inputs (`--config`, `--secrets-file`, `--input`, `--output`, `--at`, `--verbose`). Every other setting lives in TOML, with documented defaults where reasonable. Paths inside the TOML file resolve relative to that file; command-line paths resolve relative to the working directory.

Illustrative configuration (final keys are fixed as each phase is implemented):

```toml
data_dir = "data"
timezone = "America/Los_Angeles"  # used for local image filenames
image_prefix = "mazama_stake_"
metadata_file = "mazama_stake.json"  # local copy of the public metadata

[camera]
host = "192.168.1.25"
channel = 1                   # TrackMix: 0 = wide lens, 1 = telephoto
timeout_seconds = 15
accept_invalid_certs = true   # the camera's HTTPS certificate is self-signed

[temperature]
source = "fixture"            # until the sensor API is known
sampling_interval_minutes = 10

[overlay]
height_fraction = 0.12
inset_fraction = 0.03
trace_color = "#00ffff"
freezing_color = "#ffffff"
line_width = 2

[spaces]
region = "sfo3"
bucket = "stake-images"
image_object = "mazama/latest/stake_image.jpg"
metadata_object = "mazama/latest/stake_image.json"
image_alt = "Mazama Snow Stake"
image_expiration_seconds = 60
```

### Local files

All local output lives under `data_dir`:

```text
data/
  files/
    mazama_stake_202611081521.jpg            captured image
    mazama_stake_202611081521_annotated.jpg  rendered image
  mazama_stake.json                          public metadata, overwritten each upload
  temperature/<UTC-date>.jsonl               temperature history, one reading per line
```

Image names follow the Python convention: `<image_prefix><YYYYMMDDHHMM>.jpg`, in the configured `timezone`, to the minute. The timezone comes from configuration rather than the host clock, so filenames do not depend on how the Pi is set up. Rendered images append `_annotated` to the stem.

- A capture in the same minute as an existing image, including the repeated hour when daylight saving time ends, replaces that image and logs a warning.
- There is no per-image sidecar. Dimensions are read from the JPEG. The capture time is parsed from the 12-digit timestamp in the filename; `--at` overrides it for images whose names do not contain one. A local time that occurs twice when clocks fall back resolves to the earlier instant.

Write images and JSON documents to a temporary file and then rename it into place, so a partly written file never appears under its final name.

### Logging and exit codes

- Log to stderr so output reaches the systemd journal. Info level by default; `--verbose` enables debug.
- Never log credentials or authenticated camera URLs.
- Exit 0 on success and nonzero on any failure. `clap` usage errors keep its default exit code 2.
- Each command prints the paths of the files it wrote to stdout, so steps can be inspected or scripted.

## Phase 1: Read the camera and save an image locally

### Implementation

- Capture with one HTTPS request to the camera's snapshot API, which returns a JPEG:
  `GET https://<host>/cgi-bin/api.cgi?cmd=Snap&channel=<channel>&rs=<random>&user=<user>&password=<pw>`
  The camera is a Reolink TrackMix PoE; channel 1 is the telephoto lens (1920×1080) used for the stake view. A single request needs no login session, so a crashed run cannot leave a session open.
- Use `reqwest` with `rustls`, building the query with its URL encoder so passwords containing symbols are encoded correctly. Accept the self-signed certificate when `accept_invalid_certs` is set.
- Verify the response is a JPEG. The camera reports errors as JSON; surface its error detail.
- Decode with `image` and center-crop horizontally: width becomes `min(width, height)`; height remains unchanged.
- Save the JPEG as described under [Local files](#local-files), warning when an existing image is replaced.
- Load camera credentials from a read-only JSON secrets file using the existing `rtsp_user` and `rtsp_pw` fields; the names are kept so existing secrets files still work.
- Apply a finite request timeout. Never log the request URL, which contains the password; strip it from `reqwest` errors (`without_url()`) before reporting them.

### Independent acceptance

- Run against the real camera and inspect the saved JPEG.
- Confirm dimensions, crop, filenames in the configured timezone, and replacement with a warning on repeated captures within a minute.
- Test unreachable camera, wrong password, and unwritable output errors, and confirm no error message contains the password.
- Automated tests use fixture images and a fake capture source; they require no camera.

**Review stop:** No temperature collection, graph drawing, or uploads are required.

## Phase 2: Read temperatures and save them locally

### Implementation

- Introduce a replaceable temperature source returning observation time, collection time, source identifier, and temperature normalized to Celsius.
- Develop using an explicitly selected fixture source until the sensor API is known.
- Implement the real adapter once endpoint, authentication, response format, units, and timestamp behavior are available.
- Append each reading as one JSON line to `data/temperature/<UTC-date>.jsonl`, where the date is the observation date: `{"source": "...", "observed_at": "<RFC3339>", "collected_at": "<RFC3339>", "celsius": -1.5}`.
- Write each line with a single append. Skip a reading whose source and observation time already appear in that day's file.
- Readers skip malformed lines with a warning, so a crash during an append cannot block rendering.
- Reject nonfinite values and never replace a failed reading with zero.
- Only one process writes at a time because the systemd service does not overlap, so no file locking is needed.

### Independent acceptance

- Collect readings, restart the command, and verify history persists.
- Confirm Fahrenheit conversion where applicable, duplicate handling, readings spanning midnight UTC, malformed lines, and failed-reading behavior.
- Use temporary directories for automated tests.
- Test the real API separately once its details are available.

**Review stop:** This command works without a camera, image rendering, or Spaces access.

## Phase 3: Draw the temperature graph on an image

### Implementation

- Accept an existing image and read the preceding 24 hours of temperature history from the daily files covering that window.
- Take the capture time from the image filename, or from `--at <RFC3339-time>`.
- Draw using `image`/`imageproc`, then save a separate annotated JPEG. Preserve the original.
- Draw only a thin cyan temperature trace and a thin white horizontal freezing line at 0°C. Include no labels, axes, legend, or background panel.
- Initially use the bottom 12% of image height, inset 3% from the sides and bottom.
- Map the full rolling 24-hour window left to right. Include freezing in the vertical range, add padding, and use a minimum 10°C span.
- Leave missing periods empty and break the trace across gaps exceeding three configured sampling intervals.
- With fewer than two readings, omit the temperature trace; with no readings, omit the overlay.
- Keep placement, colors, and line thickness configurable in TOML for visual tuning.

### Independent acceptance

- Render fixture histories covering above-freezing, below-freezing, crossing-freezing, constant, sparse, and missing data.
- Verify graph geometry and pixels before JPEG compression.
- Inspect the annotated camera image and adjust presentation before accepting the phase.
- Rendering requires no camera, sensor connection, or Spaces credentials.

**Review stop:** No upload is performed.

## Phase 4: Upload the annotated image to Spaces

### Prerequisite

- The Spaces key exposed in git history was rotated and removed from the tracked Python scripts on 2026-11-08 (see SECURITY.md F1). Put the new key in the secrets file; commit placeholder examples only.

### Implementation

- Upload the selected local JPEG through the AWS Rust S3 SDK using the configured DigitalOcean endpoint.
- Generate the public metadata. Its structure must stay compatible with the website:
  - `image.url`: the public image URL in Spaces.
  - `image.alt`: descriptive text for the image.
  - `timestamp`: capture time as Unix seconds in UTC.
  - `valid_until`: capture time plus `image_expiration_seconds`, as Unix seconds. This sets metadata validity; it does not schedule captures.
- Use the original capture time (from `run`, the filename, or `--at`), not render or upload time.
- Save the metadata locally as `<data_dir>/<metadata_file>`, overwritten on each upload.
- Use the configured remote object names, public-read access, appropriate content types, and `Cache-Control: max-age=60`.
- Upload JPEG first, then metadata. If the image upload fails, do not upload metadata.
- Retain local files after failure. Use bounded timeouts and retries, and return nonzero on failure.
- Load the `spaces_key` and `spaces_access_key` secrets fields.
- Fixed remote names maintain website compatibility; the two uploads are not an atomic publication.

### Independent acceptance

- Use a fake uploader to verify bytes, object names, headers, ordering, and failure handling.
- Upload manually to a test Spaces prefix, inspect the JPEG and JSON, then verify the existing website reader.
- Confirm retries do not alter local images or capture timestamps.
- Confirm the capture time is parsed from the filename and overridden by `--at`.
- This command requires an existing image, not a live camera or sensor.

**Review stop:** The combined command and deployment are Phase 5.

## Phase 5: Combined run and Raspberry Pi deployment

### Implementation

- Add `run`, which performs capture, temperature, render, and upload in order, passing the exact capture time in memory rather than re-reading it from the filename.
- A capture, render, or upload failure stops the cycle. A temperature failure is logged and the cycle continues using existing history, because the graph should not block a fresh image. `run` still exits nonzero if any step failed.
- Deploy without a container. Every dependency is a Rust crate (`reqwest` with `rustls`, `image`, `imageproc`, the AWS SDK with `rustls`), so no Python runtime is needed. A GNU target build still depends on compatible system libraries on the Pi; verify those requirements, or validate a static musl build, before documenting installation.
- Cross-compile for the Pi in the dev container, e.g. `cargo zigbuild --release --target aarch64-unknown-linux-gnu` (or a static `aarch64-unknown-linux-musl` build), and copy the executable to `~/Work/stakecam/stakecam` on the Pi. Add the build and install steps to `bin/` scripts.
- Run it from a systemd oneshot service as the non-root Pi user who owns `~/Work/stakecam`. Keep the executable, shared tracked `config.toml`, private untracked `stake_cam_secrets.json` (mode `0600`), and generated `data/` in that directory. Keep `data_dir = "data"` so the same configuration works in development and deployment; copy secrets separately. Set `User=`, `WorkingDirectory=`, and absolute paths in `ExecStart=` using the actual Pi username and home directory; do not use literal `~` in unit paths. Create `data/` owned by that user before enabling the service. Depend on network readiness (`network-online.target`) and set `TimeoutStartSec=` so a stalled run cannot block later runs.
- Trigger the service from a systemd timer. A timer does not start a service that is still running, so runs never overlap.
- Measure total cycle duration on the Pi before finalizing the interval and timeout.

### Independent acceptance

- Run `run` manually on the Pi and verify camera connectivity, the annotated JPEG, metadata, Spaces object names, public URLs, and the website.
- Confirm a temperature-source failure still publishes an image and produces a nonzero exit.
- Confirm the timer fires on schedule and logs appear in the journal.

**Review stop:** Document the verified build, installation, and service commands in the README.

## Secrets and tests

- Keep credentials out of command-line values, TOML, logs, metadata, and fixtures. The secrets file is readable only by the service user.
- Keep unit tests alongside modules and integration tests under `tests/`. Use temporary directories, fixed clocks, fixture images, and fake external services.
- Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked` for every phase. Ordinary tests require no network or real credentials.
- Commit `Cargo.lock`, and document each phase's verified commands in the README.

## Open questions

- **Temperature sensor.** Endpoint, authentication, response format, units, and timestamp behavior.
- **Deployment details.** Pi model, capture interval, and timeouts.
- **Python reference.** `python/from-pi/rtsp2spaces.py` and `python/from-pi-also/rtsp2spaces.py` differ; which one reflects what runs on the Pi today.
