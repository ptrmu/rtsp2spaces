# Neolink dependency

Source: https://github.com/ptrmu/neolink
Maintenance branch: `stakecam/native-snapshots`
Pinned revision: `4013b88a9a453b8104bc54f8055bac9c563d4578`
Package: `neolink_core` 0.6.2 under `crates/core`
Original source: QuantumEntangledAndy/neolink at `e47a0d5734b0a06b53bd8d78be9ca2f6b8396480` (v0.6.2)
License: AGPL-3.0-or-later; original notices and LICENSE remain in the fork.

## Ownership

The fork owns snapshot assembly/body/reorder bounds, networking cancellation,
protocol fixtures, and their unit tests. It preserves upstream history and samples.
Its core-only CI runs formatting, 50 unit tests, one documentation example and
Clippy without GStreamer. Inherited Clippy warnings remain visible.

Stakecam owns source selection, credential protection, adapter deadlines/error
policy, task concurrency/barriers, image processing and local persistence tests.
Live camera acceptance is separate from ordinary offline tests. There is no local
vendor copy or duplicated import of the core's private guard tests.

The fork's five commits separate input bounds, cancellation, modern-Rust
compatibility, inherited test repairs, and CI/documentation. Known non-JPEG
snapshot responses remain unresolved; see the historical [fork evaluation](neolink-evaluation.md).

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
