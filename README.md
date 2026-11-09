# Stakecam

Stakecam is being rewritten to capture a still image from a Reolink camera (through its HTTPS snapshot API), draw a 24-hour temperature graph on it, and upload the JPEG and JSON metadata to DigitalOcean Spaces for display on a website. The planned deployment runs each cycle from a systemd timer on a Raspberry Pi.

```text
camera -> capture -> render temperature graph -> upload -> DigitalOcean Spaces -> website
                          ^
temperature sensor -> temperature history (local files)
```

## Status

The project is being rewritten in Rust. Phase 1 camera capture is implemented and verified with automated fixture tests and live-camera checks on 2026-11-08. Phase 2 current-temperature collection and 24-hour history import are implemented and verified against Ambient Weather. Rendering, uploading, and deployment remain planned. [DESIGN.md](DESIGN.md) defines the commands, configuration, and implementation phases. The earlier Python implementation is kept for reference in [`python/`](python/).

## Development

Development happens in a VS Code Dev Container, so no local Rust installation is needed.

Prerequisites:

- [Docker Desktop](https://docs.docker.com/desktop/setup/install/windows-install/) with the WSL 2 backend and Linux containers.
- [VS Code](https://code.visualstudio.com/) with the [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers).

Clone the repository, open the folder in VS Code, and run **Dev Containers: Reopen in Container**. The container configuration, Cargo package, and lockfile are committed, so nothing needs generating. Container creation and initial setup need network access to download the image, Rust components, and dependencies. The container provides the Rust toolchain and rust-analyzer. After changing [`.devcontainer/devcontainer.json`](.devcontainer/devcontainer.json), run **Dev Containers: Rebuild Container**.

The committed environment uses:

- Container image `mcr.microsoft.com/devcontainers/rust:2-1-trixie` (Debian 13 / Trixie).
- VS Code extension `rust-lang.rust-analyzer`.
- A single Rust 2024 package, `stakecam`, with a thin binary and library modules; dependencies are declared in `Cargo.toml` and pinned in `Cargo.lock`; build output goes into `target/`.
- The image's default Rust toolchain. No `rust-toolchain.toml` pins a compiler version, and the container image tag can receive updates. On 2026-11-08, this workspace runs Rust and Cargo 1.99.1 on `x86_64-unknown-linux-gnu`; this is an observed version, not a requirement.

When the container is created it runs [`bin/setup.sh`](bin/setup.sh), which installs the Rust components, fetches dependencies, and runs the formatting, clippy, and test checks. Run it again by hand at any time, including before committing:

```sh
bin/setup.sh
```

The setup script runs `cargo fetch --locked`, `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`. It reports failures and exits nonzero if any check fails. It installs `rustfmt` and `clippy`; the container supplies the Rust toolchain.

Pi cross-compilation tools, target installation, deployment scripts, and systemd units are not configured yet; they are planned for Phase 5 in DESIGN.md.

Run all automated checks with:

```sh
bin/test
```

This runs formatting, Clippy for all targets and features, and all unit, integration, and documentation tests with all features enabled and the lockfile enforced. It runs every check even if an earlier check fails, and exits nonzero on failure. It can be invoked from any working directory by its path. Unlike `bin/setup.sh`, it does not install components or explicitly fetch dependencies; Cargo may still download missing dependencies. Live-camera acceptance checks are separate and are not run by this command.

Ordinary tests need no camera, sensor, network, or credentials once dependencies are cached. Tests generate fixture JPEGs and use fake capture sources. Testing against real devices requires the container to reach the camera on the local network and, for uploads, Spaces credentials.

## Usage

Use the shared `config.toml` and create a private `stake_cam_secrets.json` as described below. The configuration contains no credentials and is intended to be tracked; `config.example.toml` remains a reference template. Then run:

```sh
cargo run --locked -- capture --config config.toml --secrets-file stake_cam_secrets.json
```

The command requests one HTTPS snapshot, checks and decodes the JPEG, center-crops its width to `min(width, height)` while preserving height, and saves `<data_dir>/files/<image_prefix><YYYYMMDDHHMM>.jpg`. The filename timestamp is always UTC, independent of the host timezone. It prints the saved path to stdout and logs to stderr. Repeated captures within the same UTC minute replace the image with a warning. Saving uses a temporary file and rename. Older images named in local time are not renamed; their timestamps must be converted to UTC or supplied explicitly with `--at` when future render/upload commands are implemented.

`data_dir`, `image_prefix` (no path separators), and `[camera].host` are required. Relative `data_dir` resolves against the configuration file; command-line paths resolve against the working directory. Camera defaults are `channel = 1`, `timeout_seconds = 15` (must be positive), and `accept_invalid_certs = false`. The example explicitly enables the camera's self-signed certificate. `host` accepts a hostname or IP address with an optional port, without a scheme or path. Responses are limited to 32 MiB; redirects are rejected.

```sh
cargo run --locked -- capture --help
cargo run --locked -- --verbose capture --config config.toml --secrets-file stake_cam_secrets.json
```

Failures exit nonzero; command-line usage errors exit 2. Debug output excludes HTTP library tracing and authenticated URLs. Capture and temperature commands are available. See [DESIGN.md](DESIGN.md) for the planned interface.

Run the live acceptance suite (requires Python 3.11+, the Rust toolchain, camera connectivity, and real credentials):

```sh
bin/test-live
# Optional alternate files; relative arguments resolve against your working directory:
bin/test-live --config config.toml --secrets-file stake_cam_secrets.json
```

The script builds the executable and tests capture, same-minute replacement warnings, one deliberately wrong password, an unreachable camera address, and an output-directory failure. It suppresses raw command output and checks it for credential leaks. Temporary configurations and invalid credentials are removed automatically; images remain under `<data_dir>/live-tests/run-*/files/` for visual inspection. It leaves the normal capture files and configuration alone and exits nonzero if any check fails. A minute-boundary capture is retried to verify replacement.

Live-camera checks passed from the Dev Container on 2026-11-08: a saved 1080×1080 JPEG was visually inspected, two captures within one local minute replaced the same file with a warning, a wrong password produced the camera API error, an unreachable address timed out, and an output directory under `/proc` failed cleanly. The captured output contained none of the supplied secret values. Pi deployment filesystem permissions remain a Phase 5 check.

## Temperature collection

Read the station's latest outdoor temperature and save it:

```sh
cargo run --locked -- temperature --config config.toml --secrets-file stake_cam_secrets.json
```

Seed or refresh the past 24 hours (safe to repeat):

```sh
cargo run --locked -- temperature --config config.toml --secrets-file stake_cam_secrets.json --history-hours 24
```

`--history-hours` accepts 0–24; 0 (the default) collects only the latest reading. The shared `[temperature]` configuration selects `source = "ambient"`, the station's `mac_address`, a positive `timeout_seconds` (default 15), and `sampling_interval_minutes` (default 5, for future graph gap handling; this command does not schedule itself). Both Ambient keys are required in the private secrets file under `ambient_api_key` and `ambient_application_key`. Camera credentials are not required by the temperature command.

Readings are appended to `<data_dir>/temperature/<UTC-observation-date>.jsonl` with `source`, `observed_at`, `collected_at`, and `celsius`. The source is `ambient:<station MAC>`; observation time comes from the API's UTC millisecond timestamp, and Fahrenheit `tempf` is converted to Celsius. Duplicate station/time pairs are skipped, including across restarts. Malformed stored lines are skipped with a warning, and a truncated final line is separated before appending. Invalid or failed readings never become zero. Only one process may write temperature history at a time.

History requests page backward in batches of up to 288, wait at least 1.1 seconds between requests, and keep readings within the requested window. The latest reading is also collected. Paths are printed to stdout; the number of newly saved readings is logged to stderr. The initial live seed saved 289 readings spanning two UTC dates; a follow-up collection correctly added no duplicate.

For offline use, select `source = "fixture"` and `fixture_file = "temperature-fixture.json"` in `[temperature]`. The fixture is a JSON array of normalized readings with the fields above; its path resolves against the configuration file. No credentials or network are used for fixture collection (`--secrets-file` remains a required CLI argument but the file is not opened).

## Planned Pi layout

Phase 5 will deploy into `~/Work/stakecam` under the Pi user's home directory:

```text
~/Work/stakecam/
  stakecam             release executable
  config.toml             same configuration as development
  stake_cam_secrets.json  private, untracked, mode 0600
  data/                   generated output, untracked
```

Keep `data_dir = "data"` in both environments. It resolves against `config.toml`, so no configuration edits are needed during deployment. Both environments must be able to reach the configured camera address. Copy secrets separately and preserve their permissions.

The planned systemd service runs as the non-root Pi user with this directory as its working directory. Unit paths must use the actual absolute home path (for example `/home/<user>/Work/stakecam`), because systemd path settings do not expand `~`. Deployment scripts and units are still Phase 5 work.

## Secrets

Capture opens the JSON secrets file read-only and requires nonempty `rtsp_user` and `rtsp_pw` strings. Temperature collection uses `ambient_api_key` and `ambient_application_key`; capture ignores them. Existing files may also contain `spaces_key` and `spaces_access_key`, reserved for the future uploader. Never pass credentials as command-line values or put them in TOML, logs, or tracked files.

Create `stake_cam_secrets.json` locally with this structure, replacing the placeholders:

```json
{
  "rtsp_user": "REPLACE_WITH_CAMERA_USER",
  "rtsp_pw": "REPLACE_WITH_CAMERA_PASSWORD"
}
```

Restrict access with `chmod 600 stake_cam_secrets.json`. `.gitignore` excludes this filename and `aruco_cam_secrets.json`; other secrets filenames need explicit exclusion (see SECURITY.md F8). See [SECURITY.md](SECURITY.md) for findings, the network plan, and hardening proposals.
