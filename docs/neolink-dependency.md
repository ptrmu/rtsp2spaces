# Neolink dependency

Source: https://github.com/ptrmu/neolink
Maintenance branch: `stakecam/native-snapshots`
Pinned revision: `621a929eaac215a7d3746a6f2b41fd4f3d85371c`
Package: `neolink_core` 0.6.2 under `crates/core`
Original source: QuantumEntangledAndy/neolink at `e47a0d5734b0a06b53bd8d78be9ca2f6b8396480` (v0.6.2)
License: AGPL-3.0-or-later; original notices and LICENSE remain in the fork.

## Ownership

The fork owns snapshot assembly/body/reorder bounds, networking cancellation,
protocol fixtures, and their unit tests. It preserves upstream history and samples.
Its core-only CI runs formatting, 52 unit tests, one documentation example and
Clippy without GStreamer. Inherited Clippy warnings remain visible.

Stakecam owns source selection, credential protection, adapter deadlines/error
policy, task concurrency/barriers, image processing and local persistence tests.
Live camera acceptance is separate from ordinary offline tests. There is no local
vendor copy or duplicated import of the core's private guard tests.

The fork includes input bounds, cancellation, modern-Rust compatibility, inherited
test repairs, CI/documentation, and the snapshot subscription race fix documented
below. See the historical [fork evaluation](neolink-evaluation.md) for observations
made before that fix.

## Building and updating

Cargo fetches the Git source and registry dependencies initially. A populated
Cargo cache supports `cargo build --offline --locked`. The Git revision in
Cargo.toml is immutable; the branch is a maintenance destination, not a floating
build dependency. Cargo.lock records the resolved source and transitive graph.

To update, run the fork's core checks, select the tested full commit SHA, update
`rev` in Cargo.toml, then run `cargo update -p neolink_core`. Review the lockfile,
run `bin/test`, and run live acceptance with `bin/test-live`. Update this record
with the same revision. Do not transfer protocol fixtures back into Stakecam.

Core checks in a checkout of the fork:

```sh
cargo fmt -p neolink_core -- --check
cargo test -p neolink_core --locked
cargo clippy -p neolink_core --all-targets --locked
```

Local core checks passed before publication. The published [core workflow](https://github.com/ptrmu/neolink/actions/runs/42770074211) also passed; inherited application/Docker workflows were skipped as intended.

## Stakecam migration validation

Formatting, Clippy, all 14 offline application/adapter tests and a cached
`cargo build --offline --locked` passed with this Git dependency. The real-task
runner integration test verifies that missing Ranch credentials or UID fail only
that task, preserve fixture temperature output, and do not expose credentials.
Core-private bounds tests and all protocol samples were removed from Stakecam.

The first combined live acceptance invocation after the migration failed with
`camera response is not a JPEG` for Ranch, matching the unresolved failure seen
before migration. Other successful task output was retained under
`data/live-tests/run-ktk76dvb/`. This setup is not evidence that snapshot reliability
has been fixed.

A subsequent focused Ranch live check passed using the same pinned dependency;
its JPEG is retained under `data/live-tests/run-5sniyr_6/`. The failed combined
attempt remains part of the acceptance record; no application retry policy was added.

## Confirmed snapshot subscription race — 2026-11-12

An isolated copy of the pinned core was instrumented with credential-free packet
counters under `/tmp/stakecam-snapshot-diagnosis`. Production dependency pins
were unchanged. Three sequential captures received their advertised sizes in
full. The fourth reproduced the non-JPEG failure:

```text
SNAP_ROUTE bytes=10000 matched=false header=true
SNAP_DIAG expected=523072 received=513072 header=false end=true
```

The connection router received the first binary snapshot packet, including the
JPEG header, but had no matching subscriber and discarded it. The missing 10,000
bytes exactly explain the difference between advertised and returned lengths.
`get_snapshot` registers its binary subscription only after consuming the XML
acknowledgement; binary packets can reach the router before that registration.
It then warns on a length mismatch but returns the incomplete image as success.
Stakecam correctly rejects those bytes during JPEG validation.

The protocol fix belongs in the fork: register the binary listener before sending
the snapshot request, preserve the separate acknowledgement subscription, and
reject incomplete transfers. Regression coverage should route binary packets
immediately after the acknowledgement, before the snapshot task resumes, and
verify full byte-for-byte delivery. Revision `621a929` applies this fix and adds
that deterministic regression test, plus short/long transfer rejection coverage.
Restoring the old subscription timing makes the burst regression test time out.
All 52 core tests and the documentation test pass; Clippy reports inherited
warnings and none in the changed snapshot code.

Validation after the fix: six sequential Ranch captures using the fixed local
core succeeded. The fix was published to the maintenance branch and Stakecam's
manifest/lockfile were pinned to its full revision. Against that published pin,
formatting, application Clippy, all 14 application tests, and all eight live
acceptance checks passed. Live images are retained under
`data/live-tests/run-7cv0gwrc/`. These checks establish the regression is covered;
six sequential captures are not a long-term reliability measurement.
