# Stakecam

Stakecam is being rewritten to capture a still image from a Reolink camera (through LAN HTTPS or native Reolink UID), draw a 24-hour temperature graph on it, and upload the JPEG and JSON metadata to DigitalOcean Spaces for display on a website. The planned deployment runs each cycle from a systemd timer on a Raspberry Pi.

```text
camera -> capture -> render temperature graph -> upload -> DigitalOcean Spaces -> website
                          ^
temperature sensor -> temperature history (local files)
```

## Status

The project is being rewritten in Rust. Phase 1 LAN capture was verified live on 2026-11-08. Native UID capture and concurrent configured tasks are now implemented; validation notes follow. Phase 2 current-temperature collection and 24-hour history import are implemented and verified against Ambient Weather. Rendering, uploading, and deployment remain planned. [DESIGN.md](DESIGN.md) defines the commands, configuration, and implementation phases. The earlier Python implementation is kept for reference in [`python/`](python/).

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

Use the shared `config.toml` and private `stake_cam_secrets.json`. Both default to the working directory; global `--config` and `--secrets-file` override them before or after a subcommand.

```sh
cargo run --locked --
cargo run --locked -- capture --camera MazamaRanchCam
cargo run --locked -- capture --camera MazamaStakeCan --camera MazamaRanchCam
cargo run --locked -- temperature
cargo run --locked -- temperature --history-hours 24
cargo run --locked -- --verbose
```

Without a subcommand the application runs the TOML task list:

```toml
tasks = ["MazamaStakeCan", "MazamaRanchCam", "ChechaquoTemp"]
```

Selected fetches run concurrently on one Tokio runtime. MazamaStakeCan uses LAN HTTPS; MazamaRanchCam uses native Reolink UID discovery with relay fallback. ChechaquoTemp fetches the configured station's latest reading. Camera fetches return JPEG bytes in memory. After every fetch finishes or fails, image processing and atomic saves run serially on Tokio's blocking pool, in selected order. Temperature filesystem work also uses that pool. Task failures remain isolated; any failure makes the command exit nonzero. Successful paths go to stdout and diagnostics to stderr. Usage errors exit 2.

Configure named cameras as shown in `config.example.toml`. Only selected elements require settings and credentials. Stake retains its width crop to `min(width, height)` while preserving height; Ranch keeps its full frame. JPEG responses are capped at 32 MiB. Decoding is limited to 8192 pixels per dimension and a 256 MiB allocation budget.

Images are saved atomically as `<data_dir>/files/mazama_stake_can_<YYYYMMDDHHMMSS>z.jpg` or `mazama_ranch_cam_<YYYYMMDDHHMMSS>z.jpg`. The timestamp is the UTC client snapshot-request time, not hardware exposure time. Same-camera captures in the same second replace the file with a warning. Existing images are not renamed. Relative TOML paths resolve against the configuration file; CLI paths resolve against the working directory.

Native UID capture uses `neolink_core` from [ptrmu/neolink](https://github.com/ptrmu/neolink/tree/stakecam/native-snapshots), pinned to an exact Git commit. [Dependency provenance and test ownership](docs/neolink-dependency.md) record the revision, patches, and update procedure. Cargo downloads the source on the first build; later offline builds require its Git and registry caches. Protocol fixtures and patch regression tests live in the fork; Stakecam keeps adapter and application integration tests. It requires no external Neolink executable, Python, or GStreamer. Pi deployment and binary system dependencies remain to be verified.

[RanchCam capture timing](docs/mazama-ranch-capture-timing.md) records measured stage durations, concurrency behavior, and optimization candidates.

```sh
bin/test-live
bin/test-live --config config.toml --secrets-file stake_cam_secrets.json
```

The live suite tests each camera, the configured cameras/temperature invocation, missing-credential isolation in both directions, a wrong Stake password, an unreachable LAN camera, and output failure. It suppresses raw output and checks nested credentials for leaks. Images remain under `data/live-tests/run-*/` for inspection. Same-second replacement and processing barriers are tested offline.

Validation on 2026-11-09: formatting, Clippy, and all offline tests passed. The native live suite passed focused captures, the combined task run, credential isolation in both directions, wrong-password handling, LAN timeout with Ranch success, and output failure, without credential leaks. Saved images were inspected: Stake 1080×1080 and Ranch 3840×2160. Results are retained in `data/live-tests/run-d4n2vme7/`. UID session tests cover snapshot failure, cancellation, bounded cleanup, and preservation of a successful image when cleanup times out.

A repeat live suite had two failed cases involving Ranch output; isolated reruns of the combined invocation and LAN-failure case passed. The earlier complete suite passed. Treat UID connectivity as fallible: failures remain visible and isolated, and this milestone adds no application-level retry policy.

`bin/capture-remote` remains an optional diagnostic utility requiring an external Neolink executable and its runtime dependencies. It now reads `cameras.MazamaRanchCam` from hierarchical secrets. The Rust application does not invoke it.

## Temperature collection

Read the station's latest outdoor temperature and save it:

```sh
cargo run --locked -- temperature --config config.toml --secrets-file stake_cam_secrets.json
```

Seed or refresh the past 24 hours (safe to repeat):

```sh
cargo run --locked -- temperature --config config.toml --secrets-file stake_cam_secrets.json --history-hours 24
```

`--history-hours` accepts 0–24; 0 (the default) collects only the latest reading. The shared `[temperature]` configuration selects `source = "ambient"`, the station's `mac_address`, a positive `timeout_seconds` (default 15), and `sampling_interval_minutes` (default 5, for future graph gap handling; this command does not schedule itself). Both Ambient keys are required in the private secrets file under `temperature.ambient.api_key` and `temperature.ambient.application_key`. Camera credentials are not required by the temperature command.

Readings are appended to `<data_dir>/temperature/<UTC-observation-date>.jsonl` with `source`, `observed_at`, `collected_at`, and `celsius`. The source is `ambient:<station MAC>`; observation time comes from the API's UTC millisecond timestamp, and Fahrenheit `tempf` is converted to Celsius. Duplicate station/time pairs are skipped, including across restarts. Malformed stored lines are skipped with a warning, and a truncated final line is separated before appending. Invalid or failed readings never become zero. Only one process may write temperature history at a time.

History requests page backward in batches of up to 288, wait at least 1.1 seconds between requests, and keep readings within the requested window. The latest reading is also collected. Paths are printed to stdout; the number of newly saved readings is logged to stderr. The initial live seed saved 289 readings spanning two UTC dates; a follow-up collection correctly added no duplicate.

For offline use, select `source = "fixture"` and `fixture_file = "temperature-fixture.json"` in `[temperature]`. The fixture is a JSON array of normalized readings with the fields above; its path resolves against the configuration file. No credentials or network are used for focused fixture collection; the secrets file is not opened.

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

Secrets are loaded read-only. Camera credentials are isolated by camera ID; UID is private. Ambient credentials are used only by ChechaquoTemp. HTTP and Neolink logging stay disabled even with `--verbose`.

```json
{
  "cameras": {
    "MazamaStakeCan": {
      "username": "REPLACE_WITH_STAKE_USER",
      "password": "REPLACE_WITH_STAKE_PASSWORD"
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
  }
}
```

Legacy flat camera/Ambient fields are no longer used. Move `rtsp_user`/`rtsp_pw` into `cameras.MazamaStakeCan`, the old top-level Ranch object into `cameras`, and Ambient fields into `temperature.ambient`. Leave unrelated Spaces fields unchanged until upload implementation. Restrict the file to mode 0600. `.gitignore` excludes `stake_cam_secrets.json`; other private filenames require explicit exclusions. Never put credentials in TOML, CLI values, logs, or tracked files.
