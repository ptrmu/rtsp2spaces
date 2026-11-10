# Stakecam design

## Implementation status and development environment

This document specifies the planned Rust implementation. Phase 1 capture is implemented and verified with automated tests and live-camera acceptance on 2026-11-08 (see README.md). Phase 2 temperature collection and history seeding are also implemented and verified against Ambient Weather. A remote RLC-811A native JPEG capture through Neolink 0.6.2 and Reolink UID relay was also verified on 2026-11-09 using the standalone `bin/capture-remote` utility. Native UID capture, hierarchical camera/Ambient secrets, and concurrent configured tasks are now implemented. See README.md for validation results. Rendering, uploading, and deployment remain design targets. See README.md for available commands and configuration defaults.

See [README.md](README.md#development) for the committed Dev Container, setup script, current toolchain observations, and verified development commands. Cross-compilation and Pi deployment are Phase 5 work.

## Architecture and conventions

Use one Rust 2024 Cargo package with a thin executable and focused library modules for configuration, secrets, camera capture, temperature collection, history, drawing, and Spaces uploads.

Use typed configuration, `Result` errors, contextual diagnostics, and replaceable interfaces for external services. Keep production code free of unchecked `unwrap()` calls.

Development is incremental: each phase adds one working command, and the phases are built in order. Matching the Python implementation is not a goal. The only compatibility requirement is the public metadata and remote object names the website reads (Phase 4).

### Stage boundaries and future concurrency

Keep clear boundaries between fetching image bytes, processing images (decode, crop, render, and encode), saving local files, and uploading to Spaces. Each stage accepts explicit inputs and returns its result; sequencing belongs in the command orchestration rather than inside a stage. Preserve capture timestamps as data passed between stages.

Use one Tokio runtime for all network fetching: async reqwest for LAN snapshots and Ambient requests, and neolink_core for UID discovery/login/snapshot/logout. Spawn selected fetches as independent tasks and collect every outcome before image processing. Task failures do not cancel others.

Camera fetches return camera ID, full-precision timestamp, and bounded JPEG bytes in memory. After the fetch barrier, process and save successful images in selected task order. Await one `spawn_blocking` job at a time, calling separate synchronous processing and atomic-save functions. Temperature persistence also uses the blocking pool. Started blocking jobs run to completion; bound decoder dimensions and allocations.

Uploading remains future work. Preserve image-before-metadata ordering when implementing publication.

### Commands

```sh
stakecam
stakecam capture --camera MazamaStakeCan
stakecam capture --camera MazamaRanchCam
stakecam capture --camera MazamaStakeCan --camera MazamaRanchCam
stakecam temperature [--history-hours 24]
```

Global `--config` and `--secrets-file` default to `config.toml` and `stake_cam_secrets.json` in the working directory. With no subcommand, execute the configured `tasks` list. Stable task IDs are `MazamaStakeCan`, `MazamaRanchCam`, and `ChechaquoTemp`; reject unknown or duplicate tasks. Focused commands use the same configured elements. Default temperature collection requests the latest reading; history import remains explicit.

Validate selected task settings before network work. Missing credentials fail only the affected task; malformed shared files prevent execution. Return successful paths on stdout and task-specific diagnostics on stderr. Exit nonzero if any selected task fails, including processing/persistence failures.

Future render/upload commands will require camera selection and preserve capture timestamps. They are not implemented by this milestone.

Each command runs once and exits. Scheduling and retention of local files are managed outside the application.

### Configuration

Define arguments with `clap` derive, using kebab-case flags and generated help. The command line carries only file paths and per-invocation inputs (`--config`, `--secrets-file`, `--camera`, `--input`, `--output`, `--at`, `--verbose`). Every other setting lives in TOML, with documented defaults where reasonable. Paths inside the TOML file resolve relative to that file; command-line paths resolve relative to the working directory.

Illustrative configuration (final keys are fixed as each phase is implemented):

```toml
data_dir = "data"
tasks = ["MazamaStakeCan", "MazamaRanchCam", "ChechaquoTemp"]

[cameras.MazamaStakeCan]
source = "https"
host = "192.168.1.25"
channel = 1                   # TrackMix telephoto lens
crop = "center_square"
timeout_seconds = 15
accept_invalid_certs = true   # camera's self-signed HTTPS certificate

[cameras.MazamaRanchCam]
source = "reolink_uid"
channel = 0                   # RLC-811A
crop = "none"
timeout_seconds = 60
# UID and device credentials belong in the secrets file.

[temperature]
source = "ambient"
mac_address = "94:3C:C6:45:02:5F"
timeout_seconds = 15
sampling_interval_minutes = 5

[cameras.MazamaStakeCan.overlay]
enabled = true
height_fraction = 0.12
inset_fraction = 0.03
trace_color = "#00ffff"
freezing_color = "#ffffff"
line_width = 2

[spaces]
region = "sfo3"
bucket = "stake-images"

[cameras.MazamaStakeCan.publication]
metadata_file = "mazama_stake_can.json"
image_object = "mazama/latest/stake_image.jpg"
metadata_object = "mazama/latest/stake_image.json"
image_alt = "Mazama Snow Stake"
image_expiration_seconds = 60

[cameras.MazamaRanchCam.publication]
metadata_file = "mazama_ranch_cam.json"
image_object = "mazama/latest/ranch_image.jpg"
metadata_object = "mazama/latest/ranch_image.json"
image_alt = "Mazama Ranch"
image_expiration_seconds = 60
```

### Camera identity and local files

Use the exact stable IDs `MazamaStakeCan` (existing TrackMix stake camera) and `MazamaRanchCam` (remote RLC-811A). Configuration and secrets use the same case-sensitive keys. Validate selected camera entries, supported source and crop values, positive timeouts, and source-specific settings before any network work. Publication and overlay settings are only required by commands that use them. Do not accept arbitrary path components as camera IDs.

All local output lives under `data_dir`:

```text
data/
  files/
    mazama_stake_can_20261110012630z.jpg
    mazama_stake_can_20261110012630z_annotated.jpg
    mazama_ranch_cam_20261110012638z.jpg
  mazama_stake_can.json           latest local publication metadata
  mazama_ranch_cam.json           latest local publication metadata
  temperature/<UTC-date>.jsonl  shared station history
```

All generated filenames use lowercase. Map `MazamaStakeCan` to the filename stem `mazama_stake_can` and `MazamaRanchCam` to `mazama_ranch_cam`; configuration and secrets retain their case-sensitive camera IDs. Image names use `<camera_stem>_<YYYYMMDDHHMMSS>z.jpg`, in UTC to the second; the lowercase `z` explicitly denotes UTC. Record each camera's timestamp immediately before requesting its snapshot; carry the full-precision instant in memory. This is a client request timestamp, not a hardware exposure timestamp. Independent commands recover second precision from the filename; `--at` overrides it. The two images need not have the same timestamp. There is no timezone configuration. Rendered images append `_annotated` to the stem.

- A capture for the same camera in the same UTC second replaces the existing file with a warning. Different camera IDs prevent collisions between cameras.
- There is no per-image sidecar. Read dimensions from the JPEG. Preserve the Ranch camera's native aspect ratio; center-crop the Stake camera as before.
- Support existing `<prefix><YYYYMMDDHHMM>.jpg` filenames as UTC to the minute for manual render/upload, with explicit `--camera`. Older files remain in place; no automatic renaming. Historical local-time filenames require `--at` with their actual instant.
- Keep Stake camera public object names unchanged for the existing website. The Ranch camera gets its own image and metadata objects; it cannot overwrite Stake camera output. The former local `mazama_stake.json` is superseded by the two named metadata files.

Write images and JSON documents to a temporary file and then rename into place, so a partly written file never appears under its final name.

### Hierarchical secrets

The target private `stake_cam_secrets.json` groups credentials by use. Examples contain placeholders only:

```json
{
  "cameras": {
    "MazamaStakeCan": {
      "username": "REPLACE_WITH_STATE_USER",
      "password": "REPLACE_WITH_STATE_PASSWORD"
    },
    "MazamaRanchCam": {
      "uid": "REPLACE_WITH_RANCH_UID",
      "username": "REPLACE_WITH_RANCH_USER",
      "password": "REPLACE_WITH_RANCH_PASSWORD"
    }
  },
  "temperature": {
    "ambient": {
      "api_key": "REPLACE_WITH_AMBIENT_API_KEY",
      "application_key": "REPLACE_WITH_AMBIENT_APPLICATION_KEY"
    }
  },
  "spaces": {
    "access_key_id": "REPLACE_WITH_SPACES_ACCESS_KEY_ID",
    "secret_access_key": "REPLACE_WITH_SPACES_SECRET_ACCESS_KEY"
  }
}
```

Use typed nested structs without `Debug` for secret-bearing types. Load read-only and validate credentials only for the operation using them. Missing or invalid credentials for one camera are reported under its public camera ID and do not prevent the other capture attempt. Treat the UID as private; redact all credential values, including nested values, and disable Neolink protocol logging even with `--verbose`.

Migration is a coordinated implementation change: move top-level `rtsp_user`/`rtsp_pw` to `cameras.MazamaStakeCan.username`/`password`; move the current top-level `MazamaRanchCam` object under `cameras`; move Ambient fields under `temperature.ambient`. Map the old Spaces fields to the explicit AWS-style names after verifying which value is the access key ID and which is the secret; never infer from their ambiguous old names or print values. Remove migrated camera/Ambient fields without legacy fallback. Leave unrelated Spaces fields unchanged until upload implementation. Update the Rust loaders, capture utility, live checks, and README together. Implementation migrates local camera/Ambient credentials and tracked configuration together; application loading remains read-only.

### Logging and exit codes

- Log to stderr so output reaches the systemd journal. Info level by default; `--verbose` enables debug.
- Never log credentials or authenticated camera URLs.
- Exit 0 on success and nonzero on any failure. `clap` usage errors keep its default exit code 2.
- Each command prints the paths of the files it wrote to stdout, so steps can be inspected or scripted.

## Phase 1 extension: Capture both named cameras

The original single-camera HTTPS capture is implemented. The extension now uses concurrent fetches and serial image processing; see README.md for validation results.

### Library source and deployment

- Depend on `neolink_core` from `https://github.com/ptrmu/neolink.git`, pinned by full commit SHA in Cargo.toml and Cargo.lock. The `stakecam/native-snapshots` branch starts from the tested Neolink 0.6.2 revision; do not follow its moving branch tip automatically.
- Record source provenance, licensing, patch ownership and update instructions in `docs/neolink-dependency.md`. Protocol fixtures and core regression tests belong in the fork; Stakecam retains adapter policy and application integration tests.
- Use the library's native snapshot API without an external Neolink executable, Python wrapper or GStreamer runtime. Verify binary system dependencies on the Pi during deployment acceptance.
- Builds need network access to populate Cargo's Git and registry caches initially. Commit Cargo.lock; explicit dependency updates require core checks, Stakecam integration checks and live acceptance.

### Implementation

- Introduce typed camera settings keyed by the two stable IDs and source adapters behind the existing replaceable capture interface. Return image bytes and capture timestamp separately from crop, encoding, and local persistence. Keep the camera ID with each successful result.
- `MazamaStakeCan`: retain the existing HTTPS `Snap` request, query encoding, optional self-signed certificate acceptance, finite timeout, redirect rejection, and JPEG/error checks. Channel 1 is the TrackMix telephoto view. Apply `center_square` after fetching.
- `MazamaRanchCam`: connect using UID discovery with relay fallback, authenticate with its device credentials, request the native channel-0 JPEG, then log out and release the connection. Preserve the full frame (`crop = "none"`); the live test returned 3840×2160. Do not assume every response has that size.
- Bound the whole UID operation, including connection, login, snapshot, and cleanup, by the configured deadline. Attempt logout after login even when capture fails; cleanup must not wait indefinitely or discard an otherwise successful image. On timeout cancel adapter-owned network tasks while keeping the shared runtime available to other tasks. Do not change camera time, lights, zoom, or other settings during capture.
- Validate JPEG responses and enforce a 32 MiB payload ceiling. Review and bound the core snapshot assembly path before allocating from camera-reported sizes; a length check after downloading alone is insufficient.
- Fetch selected cameras and ChechaquoTemp concurrently; retain successes after the fetch barrier. Errors identify the camera and stage without exposing raw protocol payloads, credentials, UID, or authenticated URLs. Strip URLs from HTTPS transport errors and suppress or sanitize upstream errors.
- Load the hierarchical secrets above. Save each image using its mapped lowercase camera stem and own timestamp as specified under Local files. Keep the existing atomic-save behavior.

### Independent acceptance

- Offline tests verify both-camera orchestration, partial success in either direction, source selection, credential isolation, hierarchical validation, timeout/cancellation, bounded payloads, JPEG validation, crop policies, UTC filenames, same-second replacement, and secret-free diagnostics. Ordinary tests require no real camera or network.
- Verify source-specific configuration errors before network access; a malformed secrets document fails cleanly without echoing its contents.
- Run one `stakecam capture --camera MazamaStakeCan --camera MazamaRanchCam` invocation against both real cameras and inspect both saved JPEGs. Confirm Stake crop and Ranch full-frame dimensions, filenames, independent timestamps, and UID relay connectivity.
- Exercise an unavailable camera and confirm the other still saves successfully while the command exits nonzero. Validate cleanup on snapshot failure and timeout, with no upstream logging leaks.
- Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`. Cross-compilation and deployment acceptance remain Phase 5 work.

**Review stop:** Both named images are captured in one invocation. No rendering, uploads, or deployment are required for this extension.

## Phase 2: Read temperatures and save them locally

### Implementation

- Introduce a replaceable temperature source returning observation time, collection time, source identifier, and temperature normalized to Celsius.
- Support an explicitly selected fixture source for offline development.
- Read Ambient Weather through `https://rt.ambientweather.net/v1/devices` for the latest station reading and `/v1/devices/<MAC>` for history. Authenticate with `temperature.ambient.api_key` and `temperature.ambient.application_key` from the secrets file, keep authenticated URLs out of diagnostics, convert `tempf` to Celsius, and interpret `dateutc` as UTC milliseconds.
- `temperature --history-hours 24` seeds the rolling day in addition to the latest reading; values 0–24 are accepted. Page history backward using `endDate` with up to 288 readings per request and wait at least 1.1 seconds between requests.
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

- Accept an existing image and explicit camera ID. For `MazamaStakeCan`, read the preceding 24 hours of shared temperature history from the daily files covering that window. `MazamaRanchCam` has no overlay initially and is published from its captured original; a future overlay is opt-in per camera.
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

## Phase 4: Upload a camera image to Spaces

### Prerequisite

- The Spaces key exposed in git history was rotated and removed from the tracked Python scripts on 2026-11-08 (see SECURITY.md F1). Put the new key in the secrets file; commit placeholder examples only.

### Implementation

- Upload the selected local JPEG through the AWS Rust S3 SDK using the configured DigitalOcean endpoint.
- Generate the public metadata. Its structure must stay compatible with the website:
  - `image.url`: the public image URL in Spaces.
  - `image.alt`: descriptive text for the image.
  - `timestamp`: capture time as Unix seconds in UTC.
  - `valid_until`: capture time plus `image_expiration_seconds`, as Unix seconds. This sets metadata validity; it does not schedule captures.
- Use the original capture time (from the task runner, the filename, or `--at`), not render or upload time.
- Select publication settings by camera ID. Save metadata locally as `<data_dir>/<cameras.<CameraId>.publication.metadata_file>`, overwritten on each upload for that camera.
- Use the configured remote object names, public-read access, appropriate content types, and `Cache-Control: max-age=60`.
- Upload JPEG first, then metadata. If the image upload fails, do not upload metadata.
- Retain local files after failure. Use bounded timeouts and retries, and return nonzero on failure.
- Load shared `spaces.access_key_id` and `spaces.secret_access_key` credentials from the hierarchical secrets file.
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

- Extend the default task runner with rendering and uploads after the existing fetch barrier, processing successful camera results in order. Carry each camera ID and exact capture time in memory. Render Stake with its overlay; upload Ranch directly unless an overlay is explicitly configured.
- A camera capture, render, or upload failure stops only that camera's downstream work; continue the other camera. A temperature failure is logged and processing continues using existing history. Do not republish a stale image as a fresh capture. The default invocation exits nonzero if any required operation failed; successful publications remain valid.
- Deploy without a container. Compile the pinned `neolink_core` into the same executable alongside `reqwest` with `rustls`, `image`, `imageproc`, and the AWS SDK with `rustls`. No Python runtime, separate Neolink process, or GStreamer runtime is needed for native snapshot capture. A GNU target build still depends on compatible system libraries on the Pi; verify those requirements, or validate a static musl build, before documenting installation.
- Cross-compile for the Pi in the dev container, e.g. `cargo zigbuild --release --target aarch64-unknown-linux-gnu` (or a static `aarch64-unknown-linux-musl` build), and copy the executable to `~/Work/stakecam/stakecam` on the Pi. Add the build and install steps to `bin/` scripts.
- Run it from a systemd oneshot service as the non-root Pi user who owns `~/Work/stakecam`. Keep the executable, shared tracked `config.toml`, private untracked `stake_cam_secrets.json` (mode `0600`), and generated `data/` in that directory. Keep `data_dir = "data"` so the same configuration works in development and deployment; copy secrets separately. Set `User=`, `WorkingDirectory=`, and absolute paths in `ExecStart=` using the actual Pi username and home directory; do not use literal `~` in unit paths. Create `data/` owned by that user before enabling the service. Depend on network readiness (`network-online.target`) and set `TimeoutStartSec=` so a stalled run cannot block later runs.
- Trigger the service from a systemd timer. A timer does not start a service that is still running, so runs never overlap.
- Measure total cycle duration on the Pi before finalizing the interval and timeout.

### Independent acceptance

- Run the default invocation manually on the Pi and verify both camera connections, Stake annotated image, Ranch full-frame image, separate metadata and Spaces objects, public URLs, and the existing stake website reader. Confirm Stake LAN reachability and Ranch UID/relay access from the deployed network.
- Confirm a temperature-source failure still publishes both available images and produces a nonzero exit. Confirm failure of either camera leaves the other publication working and returns nonzero.
- Confirm the timer fires on schedule and logs appear in the journal.

**Review stop:** Document the verified build, installation, and service commands in the README.

## Secrets and tests

- Keep credentials out of command-line values, TOML, logs, metadata, and fixtures. The secrets file is readable only by the service user.
- Keep unit tests alongside modules and integration tests under `tests/`. Use temporary directories, fixed clocks, fixture images, and fake external services.
- Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked` for every phase. Ordinary tests require no network or real credentials.
- Commit `Cargo.lock`, and document each phase's verified commands in the README.

## Open questions

- **Temperature source resolved.** Ambient Weather station Oncrr; account API access and current/history readings verified. Local station access is not needed.
- **Deployment details.** Pi model, capture interval, and timeouts.
- **Python reference.** `python/from-pi/rtsp2spaces.py` and `python/from-pi-also/rtsp2spaces.py` differ; which one reflects what runs on the Pi today.
