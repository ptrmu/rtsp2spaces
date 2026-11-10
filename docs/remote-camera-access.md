# Remote camera access options — 2026-11-10

Summary of a design discussion about getting images from a remote Reolink
RLC-811A (ChechaquoCam) that is reachable only by its UID and device
credentials. Nothing here changes DESIGN.md; any new capture path must be
specified there as its own phase before implementation.

## Decision

Keep Stakecam on the local Pi and capture from the camera by UID through the
vendored Neolink core. Measure reliability before considering alternatives.
UID access is preferred because the camera owner does nothing: no new
hardware or network changes, and their Reolink app keeps working.

## Constraints

- The camera owner runs the camera's network and wants to share pictures,
  but changes on their side should be kept to a minimum.
- With only UID access the camera is reachable only through Reolink's P2P/relay
  service, which speaks Reolink's proprietary Baichuan protocol. The official
  apps and clients use it but have no public API. The QuantumEntangledAndy
  Neolink lineage (the basis of our fork) is the only known open-source
  implementation of UID/relay connections. `reolink_aio` and the original
  `thirtythreeforty/neolink` speak Baichuan only over a direct local
  connection.
- The RLC-811A is a wired PoE camera, so it also supports the HTTP API
  (`cmd=Snap`), RTSP, and FTP upload. Battery and 4G Reolink models often lack
  these.

## Reliability notes for the UID path

- Relay connection setup can be slow and sometimes fails; use generous
  timeouts and a few bounded retries, and treat an occasional missed capture
  as normal.
- Prefer short sessions: connect, take one snapshot, disconnect.
- Record every outcome (success, timeout, login failure, other) so the success
  rate can be judged from data after a week or two.
- Keep the capture interval sensible; rapid reconnects may look abusive to the
  relay.

## Fallbacks, in order of owner effort

1. **FTP push to a DigitalOcean droplet.** The owner enters FTP settings once.
   Details below.
2. **Pi on a dedicated cable to the camera, with its own cellular uplink.** The
   owner provides only power. Details below.
3. **Pi on the owner's network.** Most reliable, but needs the owner's
   cooperation.

### FTP push

- Smallest droplet in the same region as the Space. pure-ftpd is preferred
  because `pure-uploadscript` runs a hook when each upload completes, so
  images can be moved on immediately without polling.
- Hardening: one camera-only user confined to its own folder with no shell
  access; FTPS with a Let's Encrypt certificate; firewall open only to port 21
  and a small passive port range; fail2ban.
- **To verify:** whether the RLC-811A firmware supports FTPS and timed or
  interval snapshot uploads (not only on motion). Reolink does not appear to
  support SFTP.
- Stakecam would need a new ingest mode (validate, name, record temperature,
  upload) instead of a pull. The camera controls timing and file names, so
  use the upload or embedded image time, not Reolink's file names.

### On-site device

If a device sits with the camera, it should capture over the local network
and upload directly to Spaces. It should not relay images back to be pulled.
It can use the Neolink core by local IP (skipping Reolink's relay) or the
HTTP `Snap` endpoint. The temperature data comes from Ambient's cloud API,
so it works from anywhere.

- Store only scoped credentials on the device: a Spaces key that can write
  only to the Stakecam bucket.
- Check the owner's data plan, since uploads go out over their connection.
- **Dedicated-cable variant:** connect the Pi's Ethernet to the camera through
  a PoE injector or switch, with static IPs and no routing between the
  networks. The Pi reaches the internet over Wi-Fi or, for full independence,
  cellular. What this costs: the owner's Reolink app and UID access stop
  working unless the Pi shares its connection with the camera. The camera
  needs a time server on the Pi (chrony). Firmware updates become manual.
  The camera may need recabling.

### Remote console access for an on-site device

All options use an outbound connection, so the owner's router needs no
open ports:

- Raspberry Pi Connect (`rpi-connect-lite`): browser shell, least setup.
- Reverse SSH tunnel (`autossh`) to a small droplet: no third party in the
  middle, but you maintain the droplet.
- Cloudflare Tunnel with Access, or ngrok (free tier is limited).
- Tailscale or ZeroTier on the device only (no subnet router), which gives
  access to that device and nothing else on the owner's network.
- Reduce how often a console is needed: have the device pull its own updates
  and push logs or a heartbeat to Spaces.

## Possible later move off the Pi

Because UID capture works from anywhere on the internet, Stakecam could run
on a droplet with its current capture path, with no FTP needed. The same
droplet could later host the FTP fallback. Before retiring the Pi, confirm
that every other configured camera is also reachable by UID or can push by
FTP, since a droplet cannot reach the local network.
