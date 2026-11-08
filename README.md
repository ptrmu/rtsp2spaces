# RTSP2Spaces

RTSP2Spaces is being rewritten to capture a still image from a Reolink camera (originally over RTSP, hence the name; now through its HTTPS snapshot API), draw a 24-hour temperature graph on it, and upload the JPEG and JSON metadata to DigitalOcean Spaces for display on a website. The planned deployment runs each cycle from a systemd timer on a Raspberry Pi.

```text
camera -> capture -> render temperature graph -> upload -> DigitalOcean Spaces -> website
                          ^
temperature sensor -> temperature history (local files)
```

## Status

The project is being rewritten in Rust. The Rust package is a placeholder; no phase is implemented yet. [DESIGN.md](DESIGN.md) defines the commands, configuration, and implementation phases. The earlier Python implementation is kept for reference in [`python/`](python/).

## Development

Development happens in a VS Code Dev Container, so no local Rust installation is needed.

Prerequisites:

- [Docker Desktop](https://docs.docker.com/desktop/setup/install/windows-install/) with the WSL 2 backend and Linux containers.
- [VS Code](https://code.visualstudio.com/) with the [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers).

Clone the repository, open the folder in VS Code, and run **Dev Containers: Reopen in Container**. The container configuration, Cargo package, and lockfile are committed, so nothing needs generating. Container creation and initial setup need network access to download the image, Rust components, and dependencies. The container provides the Rust toolchain and rust-analyzer. After changing [`.devcontainer/devcontainer.json`](.devcontainer/devcontainer.json), run **Dev Containers: Rebuild Container**.

The committed environment uses:

- Container image `mcr.microsoft.com/devcontainers/rust:2-1-trixie` (Debian 13 / Trixie).
- VS Code extension `rust-lang.rust-analyzer`.
- A single Rust 2024 binary package, `rtsp2spaces`, with no dependencies yet; build output goes into `target/`.
- The image's default Rust toolchain. No `rust-toolchain.toml` pins a compiler version, and the container image tag can receive updates. On 2026-11-08, this workspace runs Rust and Cargo 1.99.1 on `x86_64-unknown-linux-gnu`; this is an observed version, not a requirement.

When the container is created it runs [`bin/setup.sh`](bin/setup.sh), which installs the Rust components, fetches dependencies, and runs the formatting, clippy, and test checks. Run it again by hand at any time, including before committing:

```sh
bin/setup.sh
```

The setup script runs `cargo fetch --locked`, `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`. It reports failures and exits nonzero if any check fails. It installs `rustfmt` and `clippy`; the container supplies the Rust toolchain.

Pi cross-compilation tools, target installation, deployment scripts, and systemd units are not configured yet; they are planned for Phase 5 in DESIGN.md.

Ordinary tests need no camera, sensor, network, or credentials once dependencies are cached. There are currently no tests in the placeholder package. Testing against real devices requires the container to reach the camera on the local network and, for uploads, Spaces credentials.

## Usage

Run the current placeholder from the repository root:

```sh
cargo run --locked
```

It prints `Hello, world!` and exits. Camera capture, configuration parsing, temperature collection, rendering, and uploading are not implemented.

Commands are documented here as each phase is implemented and verified. See [DESIGN.md](DESIGN.md) for the planned interface.

## Secrets

The planned Rust commands will read credentials from a JSON secrets file passed with `--secrets-file`, never from configuration or command-line credential values. The placeholder does not read secrets. Keep credentials out of the repository. Currently, `.gitignore` excludes only `stake_cam_secrets.json` and `aruco_cam_secrets.json`; other secrets filenames need explicit exclusion (see SECURITY.md F8). The fields are `rtsp_user`, `rtsp_pw`, `spaces_key`, and `spaces_access_key`. See [SECURITY.md](SECURITY.md) for findings, the network plan, and hardening proposals.
