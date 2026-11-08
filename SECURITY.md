# Security

Security findings, decisions, and proposals for RTSP2Spaces. Update this file as findings are resolved or the design changes.

## System and assets

```text
Reolink TrackMix PoE camera  --HTTPS snapshot-->  Raspberry Pi  --HTTPS upload-->  DigitalOcean Spaces  -->  public website
(192.168.1.25)                                    (systemd timer)                  (public-read objects)
                                                       ^
Windows dev machine (Dev Container)  -- builds and deploys --'
```

| Asset | What an attacker gains | Sensitivity |
|---|---|---|
| Spaces access key (`spaces_key`, `spaces_access_key`) | Write access to the bucket: replace the website image, host arbitrary content under your domain's bucket, run up storage and bandwidth charges. Depending on key scope, access to other buckets in the account. | **High** |
| Camera admin password (`rtsp_user`, `rtsp_pw`) | Full control of the camera: view both lenses, pan/tilt/zoom, change settings, enable services, install firmware. The image is public anyway, but the device is a foothold on the home network. | Medium; lower once the camera is isolated |
| Camera firmware | A compromised camera can attack other devices on its network or exfiltrate video. | Medium |
| Raspberry Pi | Holds both credentials; sits on the home network with internet access. | Medium |
| GitHub repository (`ptrmu/rtsp2spaces`) | **Public.** Anything committed, including history, is readable by anyone. | — |

The published image and metadata are public by design and are not considered sensitive.

## Findings

Findings as of 2026-11-08.

### F1. Spaces access key published on GitHub — resolved (key rotated 2026-11-08)

`python/from-pi/grab_and_push.py` and `python/from-pi/grab_and_push.py.sav` contain a DigitalOcean Spaces access key ID and secret in plain text. They were first committed on 2025-10-12 (`0e97a6e`) and are pushed to the public GitHub repository, so the key was treated as compromised.

- **Done 2026-11-08:** the key was rotated in DigitalOcean.
- **Checked 2026-11-08:** no apparent misuse of the bucket or billing during the exposure. Nothing uses Spaces until this program's upload phase, which will use the new key.
- **Done 2026-11-08:** the embedded key and camera passwords were replaced with placeholders in the tracked Python files.
- The old key remains in git history. It is now worthless, so rewriting history is optional.

### F2. Camera admin passwords published on GitHub — medium

`python/from-pi/grab_and_push.py`, `grab_and_push.py.sav`, and `grab_aruco.py` contain RTSP URLs with the `admin` password for `192.168.1.25` (the stake camera, same password as the secrets files) and `192.168.1.183` (a second camera, unreachable on 2026-11-08).

- **Decision:** accepted risk for now. The camera view is public, and the camera is not reachable from the internet unless P2P cloud access or port forwarding is enabled (see F4).
- Note that the password is the device admin password, not just a viewing password. Change it when the camera moves to the aux network (see [Network plan](#network-plan)), and use a separate low-privilege account for the Pi if the TrackMix allows snapshots from one.
- The passwords were removed from the current tracked files on 2026-11-08 but remain in git history until the password is changed.

### F3. Camera firmware is two years old — medium

The camera reports `Reolink TrackMix PoE`, firmware `v3.0.0.4255_2411271220` (built November 2024), hardware `IPC_529SD78MP`.

- **Proposal:** check Reolink's download center for a newer TrackMix PoE release and install it. Re-check periodically; Reolink does not auto-update PoE cameras.

### F4. Reolink security record — informational

No credible report of a deliberate backdoor in Reolink firmware was found. Reolink has a steady history of ordinary vulnerabilities:

- Hard-coded key in the P2P cloud protocol (CVE-2020-25173, fixed January 2021) and a hard-coded TLS private key in the RLC-410W.
- OS command injection, including two on CISA's Known Exploited Vulnerabilities list: CVE-2021-40407 (RLC-410W) and CVE-2019-11001.
- Firmware-update flaws: CVE-2021-40419 (RLC-410W, arbitrary firmware update) and a 2025 report of missing signature validation on a doorbell model.
- Many 2024–2026 CVEs, mostly in doorbells, Wi-Fi models, and the Home Hub. None found names the TrackMix.

The well-known camera "backdoors" belong to other vendors (Xiongmai/HiSilicon, Dahua). The practical conclusion: treat the camera firmware as untrusted and limit what it can reach, rather than rely on updates alone.

Sources: [Talos](https://blog.talosintelligence.com/vuln-spotlight-reolink-cameras/), [CISA ICSA-21-019-02](https://www.cisa.gov/news-events/ics-advisories/icsa-21-019-02), [CISA KEV additions, Dec 2024](https://www.cisa.gov/news-events/alerts/2024/12/18/cisa-adds-four-known-exploited-vulnerabilities-catalog), [NVD CVE-2021-40419](https://nvd.nist.gov/vuln/detail/CVE-2021-40419), [Nozomi Networks](https://www.nozominetworks.com/blog/new-reolink-p2p-vulnerabilities-show-iot-security-camera-risks), [Pentest Partners](https://www.pentestpartners.com/security-blog/whats-my-name-again-reolink-camera-command-injection/), [OpenCVE Reolink list](https://app.opencve.io/cve/?vendor=reolink).

### F5. Camera services and network placement — medium

- The camera is on the same network as people's devices (`oncrr`).
- Observed open ports on 2026-11-08: 80 (redirects to HTTPS), 443 (web UI and API), 554 (RTSP). Other services (ONVIF, RTMP, P2P/UID, UPnP) were not checked.
- **Proposal:** read the camera's P2P, UPnP, and port settings through its API (read-only), then disable everything the system does not use. See [Network plan](#network-plan) for timing.

### F6. Camera certificate is self-signed — low

The camera's HTTPS certificate cannot be verified, so the program accepts it (`accept_invalid_certs = true`). Another device on the same network could impersonate the camera and capture the password.

- **Proposal:** later, pin the camera's certificate fingerprint in configuration instead of accepting any certificate. Isolating the camera on the aux network reduces the risk in the meantime.

### F7. Password in the snapshot URL — low, accepted

The single-request capture sends the camera password as a URL query parameter. HTTPS encrypts it on the network. The program must never log the URL; DESIGN requires stripping it from `reqwest` errors and testing that no error message contains the password.

### F8. Secrets files ignored by name only — low

`.gitignore` excludes `stake_cam_secrets.json` and `aruco_cam_secrets.json` by exact name. A secrets file with any other name would be committed.

- **Proposal:** add the pattern `*secrets*.json` to `.gitignore` and commit only `*.example.json` placeholders.

## Network plan

The home has two networks: **`oncrr`** for people's devices and **`aux`** for devices. The camera will move to `aux` **after the program is working**; until then it stays on `oncrr` at `192.168.1.25`.

Target rules once the camera is on `aux`:

| From | To | Allow |
|---|---|---|
| Raspberry Pi | camera | TCP 443 only |
| Dev machine | camera | TCP 443 only (for development and testing; web UI for administration) |
| Raspberry Pi | internet | HTTPS to DigitalOcean Spaces, plus OS updates and NTP |
| camera | internet | **Deny.** This blocks Reolink P2P cloud access and limits a compromised camera. Allow NTP only if the camera's clock matters (the program uses the Pi's clock, not the camera's). |
| camera | `oncrr` | **Deny.** |
| anything else on `aux` | camera | Deny where the router supports it. |

Placement of the Pi is a choice to make when the network work happens:

- **Pi on `oncrr`** with a router rule allowing it through to the camera. Simplest if the router supports per-host rules between networks.
- **Pi on `aux`** with internet access allowed for the Pi only. Keeps all devices together, but the dev machine then needs its own rule to reach the Pi for deployment.

Steps, in order, after the program is working:

1. Configure router rules so the dev machine and the Pi can reach `aux` on TCP 443.
2. Move the camera to `aux`, reserve its address, and update `camera.host` in the configuration.
3. Change the camera admin password (F2) and update the secrets file on the Pi.
4. Block the camera's internet access and its access to `oncrr`.
5. Disable unused camera services (F5): P2P/UID, UPnP, RTSP, RTMP, ONVIF, plain HTTP. RTSP must stay on until the Python script is retired.
6. Confirm a scheduled run still succeeds and the camera cannot reach the internet.

## Proposals by area

### Credentials

- Keep credentials out of the repository, command-line values, TOML, logs, metadata, and test fixtures (DESIGN.md).
- On the Pi, store the secrets file in `/etc/rtsp2spaces/`, owned by the service user, mode `0600`.
- When rotating the Spaces key (F1), create a key limited to the one bucket if DigitalOcean offers bucket-scoped keys for the account, rather than a full-access key.

### Raspberry Pi

- Run the service as a dedicated unprivileged user (DESIGN.md Phase 5).
- Add systemd sandboxing to the service: `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/rtsp2spaces`, `ProtectHome=yes`, `PrivateTmp=yes`, `RestrictAddressFamilies=AF_INET AF_INET6`. Verify with `systemd-analyze security rtsp2spaces.service`.
- Use SSH keys only (disable password login) and enable `unattended-upgrades` for security updates.

### Dependencies and build

- Commit `Cargo.lock` and build with `--locked` (already in DESIGN.md and `bin/setup.sh`).
- **Proposal:** add `cargo audit` (or `cargo deny check advisories`) to `bin/setup.sh` to flag crates with known vulnerabilities.
- Use `rustls` rather than OpenSSL so TLS is updated with the Rust build, not the Pi's system packages.

### Repository

- The repository is public. Review every commit for secrets before pushing.
- **Proposal:** enable GitHub secret scanning and push protection in the repository settings; both are free for public repositories.

## Action summary

| When | Action | Finding |
|---|---|---|
| ~~Now~~ | ~~Rotate the Spaces key~~ — done 2026-11-08 | F1 |
| ~~Now~~ | ~~Check bucket contents and billing for misuse~~ — none apparent, 2026-11-08 | F1 |
| ~~Before Phase 4~~ | ~~Remove embedded credentials from tracked Python files~~ — done 2026-11-08 | F1, F2 |
| Any time | Update camera firmware | F3 |
| Any time | Add `*secrets*.json` to `.gitignore`; enable GitHub secret scanning | F8 |
| Phase 1 | Never log the snapshot URL; test that errors contain no password | F7 |
| Phase 5 | Service user, `0600` secrets, systemd sandboxing, SSH keys, unattended upgrades | — |
| Phase 5 | Add `cargo audit` to `bin/setup.sh` | — |
| After the program works | Network plan steps 1–6, including camera password change and service cleanup | F2, F5 |
| Later | Pin the camera certificate fingerprint | F6 |
