# RTSP2Spaces

RTSP2Spaces captures a still image from a Reolink camera (originally over RTSP, hence the name; now through its HTTPS snapshot API), draws a 24-hour temperature graph on it, and uploads the JPEG and JSON metadata to DigitalOcean Spaces for display on a website. A systemd timer on a Raspberry Pi runs it.

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

Clone the repository, open the folder in VS Code, and run **Dev Containers: Reopen in Container**. That is the whole setup: the container configuration, Cargo package, and lockfile are committed, so nothing needs generating. The container provides the Rust toolchain and rust-analyzer. After changing [`.devcontainer/devcontainer.json`](.devcontainer/devcontainer.json), run **Dev Containers: Rebuild Container**.

When the container is created it runs [`bin/setup.sh`](bin/setup.sh), which installs the Rust components, fetches dependencies, and runs the formatting, clippy, and test checks. Run it again by hand at any time, including before committing:

```sh
bin/setup.sh
```

Ordinary tests need no camera, sensor, network, or credentials. Testing against real devices requires the container to reach the camera on the local network and, for uploads, Spaces credentials.

## Usage

Commands are documented here as each phase is implemented and verified. See [DESIGN.md](DESIGN.md) for the planned interface.

## Secrets

Credentials live in a JSON secrets file passed with `--secrets-file`, never in the repository, configuration, or command-line values. The fields are `rtsp_user`, `rtsp_pw`, `spaces_key`, and `spaces_access_key`. See [SECURITY.md](SECURITY.md) for findings, the network plan, and hardening proposals.
