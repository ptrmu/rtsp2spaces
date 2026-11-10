# Neolink core evaluation — 2026-11-09

## Recommendation

Keep the current pinned core for now. Neither candidate demonstrated a clear snapshot-reliability advantage in this small live comparison, and neither is a safe unmodified replacement for our hardened core. MAD had one more successful capture; six attempts per version cannot establish that it is more reliable.

This is not an endorsement of v0.6.2 as the best maintained upstream. Vertoforce remains a useful candidate for a future rebase because it adds connection-poller regression tests and simulation tooling. MAD is a useful alternative for its dependency maintenance and incorporated community changes. Resolve the shared snapshot failure before selecting a replacement on reliability grounds; reapply and validate our bounds/cancellation requirements on whichever baseline is selected. Do not open upstream hardening PRs until that target and its compatibility requirements are clear.

No application source, production dependency pin, private credentials, or vendored core source was changed by the evaluation. Only this report, its sanitized results, and an UPSTREAM.md pointer were added.

## Versions and method

Compared only the embedded core, not RTSP/GStreamer executables:

| Variant | Revision | Core version |
| --- | --- | --- |
| Current control | `e47a0d5734b0a06b53bd8d78be9ca2f6b8396480` plus existing local patches | 0.6.2 |
| [vertoforce](https://github.com/vertoforce/neolink/tree/c77bd5cff8167bad153a58a7a04c5da460ab13c3) | `c77bd5cff8167bad153a58a7a04c5da460ab13c3` | 0.6.3-rc.3 |
| [MutuallyAssuredDeployment](https://github.com/MutuallyAssuredDeployment/neolink/tree/beacb39619382cec2624a207c137a467a408e2a7) | `beacb39619382cec2624a207c137a467a408e2a7` | 0.7.0-beta |

Used isolated application copies under `/tmp/stakecam-evaluation/{control,vertoforce,mad}`. Candidates used their unmodified core source; their dependency graphs were resolved separately and locked. All three compiled with the existing UID adapter without API changes. All passed the same 13 application tests and application Clippy with warnings denied. Two control-only size-guard tests were excluded from all isolated copies for a matched test set; the repository's original 15-test suite was not changed. Upstream cores' own full test suites were not run. Cargo emitted non-fatal dependency warnings.

The harness used the same private credentials read-only, channel 0, UID discovery with relay fallback enabled, and the configured 60-second operation deadline. No raw protocol logs or raw error payloads were printed. Connection method was not instrumented, so this does not prove every successful session used relay rather than another available discovery path.

Ran two three-capture batches per version in order control → vertoforce → MAD, then MAD → vertoforce → control. Camera sessions never overlapped. Each batch used one process, repeatedly connecting, logging in, capturing and cleaning up. Successful native JPEGs were fully decoded and checked for the JPEG end marker before saving. Measured durations include fetching, cleanup and validation; this is not an isolated transport benchmark.

After the capture batches, each version underwent ten forced cancellations after 10 ms and two invocations with a one-second configured operation deadline. Counted open file descriptors after a 300 ms settling period. These tests exercise early discovery cancellation/deadlines, not every lifecycle stage or process-shutdown race.

## Live results

| Variant | Complete JPEGs | Median successful capture | Maximum early cancellation | Maximum short-deadline operation |
| --- | --- | --- | --- | --- |
| Current patched v0.6.2 | 4/6 | 17.27 s | 12 ms | 752 ms |
| vertoforce | 4/6 | 17.22 s | 12 ms | 754 ms |
| MutuallyAssuredDeployment | 5/6 | 17.23 s | 12 ms | 752 ms |

All 13 successful images decoded at **3840×2160**, with JPEG end markers present. Their native payloads were about 530 kB. All 54 observations returned to the initial **10 open file descriptors** after settling. That short observation window is not proof of leak freedom or long-term stability; RSS grew as image-decoder allocations warmed up and was not used to rank candidates.

Three failures were positively classified as “camera response is not a JPEG”: one for each version. The first control and vertoforce failures were recorded with an insufficiently specific “other” category. The harness's fixed diagnostic whitelist was expanded before the second batches; it is not valid to retroactively classify those first two failures. No automatic retries were used and failed samples were retained in the results.

## Source review

- Both candidates migrate XML handling to quick-xml; MAD also updates the core dependency versions more broadly. Vertoforce adds connection-poller busy-spin and subscriber-depth regression tests and a fake-camera/simulation facility. Those are maintenance benefits, not evidence of improved native snapshots.
- Both candidates retain unbounded native snapshot accumulation and unchecked advertised snapshot sizes. Their BC body parser still subtracts the extension offset without our bounds checks. Their UDP reorder/ACK bookkeeping lacks our fixed window. Our input-bound patches remain necessary for our requirements.
- Both candidates retain cancellation signalling followed by detached cleanup on drop. No descriptor leak was observed in the tested early-cancellation path, but this does not establish a hard cleanup bound for all states. Retain or independently prove the cancellation guarantees before migrating.
- Both candidates use nonblocking delivery when a subscriber queue is full, without a snapshot-specific lossless policy. Vertoforce uses larger subscriber queues than MAD. Dropping chunks is a concrete risk for native JPEG assembly, even when that policy helps continuous video remain responsive. Queue saturation was not forced in this live comparison.
- All three snapshot implementations subscribe for the binary response only after receiving its XML acknowledgement, leaving a possible packet-delivery race. All three also return accumulated bytes when the final size differs from the advertised size; the candidates log this at debug level. These are hypotheses to investigate alongside the observed non-JPEG responses, not proven causes.

## Next investigation and artifacts

Before migrating, add a deterministic early-binary-arrival/queue-pressure snapshot test, instrument only safe counters (advertised length, received length, chunk count, JPEG-header/end-marker presence), and determine why the current snapshot path sometimes returns non-JPEG bytes. Then compare a candidate with the required hardening reapplied over a larger run and exercise cancellation during login, snapshot and cleanup. Do not attribute failures to relay connectivity without evidence.

[Sanitized observations, source-tree/dependency-lock hashes and summary](neolink-evaluation-results.json) are retained in the repository. The temporary harness is `/tmp/stakecam-evaluation/harness.rs`, with expanded stage diagnostics in each isolated copy's `examples/evaluate.rs`; the runner is `live.py` in the same directory. Native images are under `/tmp/stakecam-evaluation/images/`. Temporary source copies, binaries and images are not durable repository artifacts.

Build/check commands were `cargo build --offline --locked --example evaluate`, `cargo test --offline --locked`, and `cargo clippy --offline --locked --all-targets -- -D warnings`, in each isolated application copy, using a shared `CARGO_TARGET_DIR=/workspaces/stakecam/target/neolink-eval`. Candidate dependencies were fetched first. The runner executed `/tmp/stakecam-evaluation/evaluate-<variant> <variant> <output-dir> <capture|cancel|deadline> <count>` sequentially.
