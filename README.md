# RTSP2Spaces

RTSP2Spaces captures a still image from an RTSP camera, processes it, and uploads a JPEG and JSON metadata to DigitalOcean Spaces for display on a website.

The project is migrating from Python to Rust. The Python implementation is preserved in [`python/`](python/). The Rust application, Dev Container configuration, runtime image, and systemd units have not been created yet. The development and deployment workflows below describe the agreed design; they are not yet executable end to end.

## Architecture

Each invocation performs one capture and upload, then exits:

```text
RTSP camera
    |
    v
Capture frame -> Crop image -> Save JPEG and JSON
                                    |
                                    v
                            DigitalOcean Spaces
                                    |
                                    v
                              Website reader
```

The existing Python application connects to the camera using RTSP credentials, grabs several frames, and retrieves an image. It crops the image horizontally to the smaller of its width and height, writes a local JPEG, and generates metadata with these fields:

- `image.url`: the public image URL in Spaces.
- `image.alt`: descriptive text for the image.
- `timestamp`: capture time as Unix seconds in UTC.
- `valid_until`: capture time plus the configured expiration interval, as Unix seconds.

It uploads the JPEG first and then the metadata, using public-read access, appropriate content types, and a 60-second cache lifetime. Preserve the metadata structure and configured object names during migration so existing website readers remain compatible.

Capture frequency is controlled externally. The image expiration setting determines metadata validity; it does not schedule the next capture.

### Development and runtime environments

```text
Windows PC
  Docker Desktop using WSL 2
    Linux development container <- VS Code connects here
      Rust compiler, language tools, and development dependencies

Raspberry Pi running 64-bit Raspberry Pi OS
  systemd timer -> systemd service -> ARM64 runtime container
                                      Compiled program and runtime dependencies
```

The development container provides the compiler, editor tools, and native dependencies. VS Code terminals, builds, tests, and Rust language tools run inside it. The source directory is mounted into the container so edits persist on the host.

The runtime image contains the compiled executable and the libraries or tools needed to run it. It does not need the Rust compiler or VS Code tooling. Use the PC's native architecture for routine development; build the Pi runtime image for `linux/arm64` and verify it on the Pi.

## Development setup

### Prerequisites

- Windows with [WSL 2 enabled](https://learn.microsoft.com/en-us/windows/wsl/install).
- [Docker Desktop download](https://www.docker.com/products/docker-desktop/). Follow the [Windows installation guide](https://docs.docker.com/desktop/setup/install/windows-install/) and use the WSL 2 backend and Linux containers.
- [Visual Studio Code download](https://code.visualstudio.com/).
- Microsoft's [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) (`ms-vscode-remote.remote-containers`), installed in Windows VS Code.
- [rust-analyzer extension](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) (`rust-lang.rust-analyzer`), installed inside the development container or declared in its configuration.
- Optional: Microsoft's [WSL extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-wsl) (`ms-vscode-remote.remote-wsl`) if opening the repository from a WSL filesystem.
- Git on the host where the repository is checked out.

A Windows installation of Rust is not required for container-based development. For camera testing, the development container must be able to reach the RTSP camera over the network. Upload testing also requires Spaces access.

### Set up the development container

The repository does not yet contain a Dev Container configuration or Cargo package. These steps create the initial Rust environment; camera libraries and the Pi runtime image will be added later.

1. Install and start Docker Desktop. Under **Settings > General**, enable **Use WSL 2 based engine** if the option is shown, and ensure Docker is using Linux containers. Open a new PowerShell terminal and verify that the client can connect to Docker and run a container:

   ```powershell
   docker version
   docker run --rm hello-world
   ```

   You can open this Windows checkout directly without opening a separate WSL distribution. If working from a WSL checkout instead, enable that distribution under Docker Desktop's **Settings > Resources > WSL Integration**. See the [WSL backend guide](https://docs.docker.com/desktop/features/wsl/).

2. In VS Code, create a **Rust** profile starting from an **Empty Profile**, then install the Dev Containers extension in it. VS Code associates the profile with the project folder. Use a separate profile for Django projects. See [VS Code Profiles](https://code.visualstudio.com/docs/configure/profiles).
3. Open the repository folder in VS Code. For the current Windows checkout, run this in PowerShell:

   ```powershell
   code C:\Users\gauss\Documents\Work\RTSP2Spaces
   ```

4. Press **Ctrl+Shift+P** and run **Dev Containers: Add Development Container Configuration Files...**. Choose to add the configuration to the workspace, select **Rust**, and start without optional features. This creates `.devcontainer/devcontainer.json`; include the generated configuration in version control. See the [Dev Containers setup guide](https://code.visualstudio.com/docs/devcontainers/containers).
5. Run **Dev Containers: Reopen in Container** from the Command Palette. The first run downloads and prepares the development environment. Once connected, open a VS Code terminal and check the container's toolchain:

   ```sh
   rustc --version
   cargo --version
   ```

6. If the template has not included rust-analyzer, install it from the Extensions view into the connected development container. Declare it in `.devcontainer/devcontainer.json` so future container setups install it automatically. Add or merge this section without replacing the rest of the generated configuration:

   ```json
   "customizations": {
     "vscode": {
       "extensions": ["rust-lang.rust-analyzer"]
     }
   }
   ```

7. In the container terminal at the repository root, initialize one executable package. Run this only once, before `Cargo.toml` exists:

   ```sh
   cargo init --bin --name rtsp2spaces --edition 2024 --vcs none .
   ```

8. Add `/target/` to `.gitignore` and commit the application's `Cargo.lock`. Preserve the existing exclusions for secrets and generated files.
9. Verify the foundation inside the container and repeat these checks as implementation progresses:

   ```sh
   cargo run
   cargo check
   cargo test
   cargo fmt --check
   cargo clippy -- -D warnings
   ```

Initially, `cargo run` will run the generated sample application. Once the capture and upload flow is implemented, running it will require configuration and can write to Spaces.

For subsequent sessions, start Docker Desktop, open the repository in VS Code, and use **Dev Containers: Reopen in Container** if prompted. After changing the container configuration or its image dependencies, run **Dev Containers: Rebuild Container** to apply the changes.

### Migration order

Port command-line arguments and secrets loading first, followed by filenames and metadata, capture and image processing, and finally Spaces uploads. Compare the resulting images, metadata, and upload behavior with the chosen Python baseline.

The Python secrets format contains `rtsp_user`, `rtsp_pw`, `spaces_key`, and `spaces_access_key`. Keep credentials out of version control, image layers, and diagnostic output. Final Rust arguments and container paths will be documented when implemented.

## Raspberry Pi deployment

### Prerequisites

- A Raspberry Pi running 64-bit Raspberry Pi OS with Docker Engine installed.
- Network access to the RTSP camera and DigitalOcean Spaces.
- A host secrets file and a writable host directory for persistent output.

Application dependencies will be packaged in the runtime image rather than installed individually on the Pi.

### Planned deployment workflow

1. Build the Rust executable and its runtime image for `linux/arm64`. Use a separate runtime stage so development tools are excluded from the deployed image.
2. Transfer the image to the Pi through a registry or image archive. The distribution method and versioning convention remain to be selected.
3. Run a manual capture on the Pi with the secrets file mounted read-only and the output directory mounted writable. Verify camera connectivity, JPEG processing, metadata, Spaces object names, and public URLs.
4. Add a systemd oneshot service that runs the image in the foreground using `docker run --rm`, with the required mounts and application configuration. Make it depend on Docker and network readiness, and expose application logs and exit status through the service.
5. Add a systemd timer for the selected capture interval. Do not launch overlapping jobs; configure a finite service timeout so a stalled capture cannot block future runs indefinitely.
6. Measure container startup time and total job duration on the Pi before finalizing the interval and timeout.

The runtime image stays on the Pi between invocations. Each timer invocation creates a short-lived container from that local image and removes the container after completion. Building or pulling an image belongs to deployment, not the scheduled capture job. Host-mounted files persist after the container is removed.

Exact build, installation, and service commands will be added after the Dockerfile, application interface, and systemd units exist and have been verified.

## Design choices and open questions

Rust is the target language. A Dev Container keeps the PC development tools isolated and reproducible, while a runtime container packages the Pi's application dependencies together. Separate development and runtime images keep compiler and editor tooling out of deployment. A systemd timer preserves the existing one-shot execution model without adding a scheduling loop to the application.

The following details remain open:

- Capture and image-processing dependencies, including whether to retain OpenCV or use FFmpeg.
- The authoritative Python entry point and which auxiliary scripts to migrate.
- The exact Pi model, capture interval, timeouts, and failure handling.
- Container base images, pinned dependency versions, and image distribution.
- Final command-line options, mount paths, and systemd configuration.

Update this README as those choices are made and replace planned instructions with verified commands as the implementation becomes available.
