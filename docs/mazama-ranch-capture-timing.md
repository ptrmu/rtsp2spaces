# MazamaRanchCam capture timing — 2026-11-12

Three sequential live captures averaged **17.52 seconds** (17.448–17.558 s).
All succeeded using the Reolink relay path and the fixed core revision
`621a929eaac215a7d3746a6f2b41fd4f3d85371c`.

## Where the time goes

| Stage | Average | Share of total |
| --- | ---: | ---: |
| Discover camera and establish connection | 2.76 s | 16% |
| Login | 0.70 s | 4% |
| Request snapshot to first image packet | 0.31 s | 2% |
| Receive remaining image data | 11.46 s | 65% |
| Decode JPEG | 1.04 s | 6% |
| Re-encode JPEG | 1.24 s | 7% |
| Logout, shutdown, and save | Below 1 ms each | Below 1% |

Snapshot acquisition as a whole averaged 11.77 seconds. The images averaged
512,852 bytes (about 513 kB), giving effective transfer throughput around
45 kB/s after the first packet. Receiving image data dominates the elapsed time.
The measurements do not establish whether the throughput is constrained by the
relay, camera uplink, packet retransmissions, or other protocol behavior.

## Method and limits

An isolated copy under `/tmp/stakecam-timing` added monotonic timers to the
application and core. Production code was unchanged by profiling. The captures
used the development build, channel 0, and the existing camera configuration and
read-only secrets. Only fixed stage names, timings, sizes, and the selected
connection method were collected; credentials and raw protocol logs were excluded.

Total time was measured around each capture subprocess, excluding compilation.
Connection timing covers `BcCamera::new`; login and snapshot are measured
separately. Snapshot acknowledgement, first-packet, and completion counters are
cumulative from the start of the snapshot operation, so they must not be added
together. The remaining-transfer duration is completion minus first-packet time.
Decode timing includes JPEG validation and reader setup; encode timing includes
crop selection and logging (RanchCam uses no crop). Integer millisecond counters
truncate sub-millisecond durations to zero. Rounded stages need not sum exactly
to subprocess duration.

The [sanitized measurements](mazama-ranch-capture-timing-results.json) are retained
alongside this report. The temporary profiling harness is not a durable project
utility. Three samples describe this run, not long-term performance. Optimized
builds may reduce CPU processing time; that was not measured.

## Concurrency

Connection, login, and transfer use async Rust with Tokio. The runner fetches both
cameras and temperature concurrently, then waits for all fetches to finish before
processing their results. JPEG processing and saving use `spawn_blocking`, with
results processed sequentially. These focused measurements select only RanchCam
and therefore exclude waiting for other tasks at the fetch barrier.

## Optimization candidates

- Investigate direct connectivity and packet retransmissions to understand the
  dominant transfer time. Relay selection was observed, not proven to be the cause.
- Reuse a connection between captures to potentially avoid much of the 3.46-second
  connection/login cost. Session reuse has not been implemented or benchmarked.
- Preserve the original RanchCam JPEG after full validation when no crop is needed
  to potentially save the 1.24-second encoding cost and avoid recompression.
  Full decode validation would still be needed; this optimization is not applied.
