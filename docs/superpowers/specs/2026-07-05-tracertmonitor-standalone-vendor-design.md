# TracertMonitor Standalone Vendor Design

## Goal

Separate TracertMonitor from the upstream `trippy` workspace while keeping the useful Trippy crates available locally for reuse.

## Chosen Approach

Use an independent product workspace at `C:\Users\34337\Documents\网络分析监控\tracertmonitor`.

The new workspace owns the TracertMonitor application and vendors selected Trippy crates under `vendor/trippy/`. This avoids Git submodule setup, keeps offline/customer-site builds simpler, and allows the product code to evolve without being mixed into the upstream Trippy repository.

## Initial Directory Structure

```text
tracertmonitor/
  Cargo.toml
  Cargo.lock
  README.md
  LICENSE
  apps/
    tracertmonitor/
  vendor/
    trippy/
      crates/
        trippy-core/
        trippy-packet/
        trippy-privilege/
        trippy-dns/
  docs/
    superpowers/
```

## Module Direction

The first migration preserves the existing runnable application under `apps/tracertmonitor`.

After the standalone workspace is verified, the product should be split into capability crates:

- `tracertmonitor-domain`: shared data types for targets, flows, hops, observations, paths, and snapshots.
- `tracertmonitor-probe`: `ProbeEngine` implementations backed by Trippy and system traceroute fallback.
- `tracertmonitor-runtime`: session lifecycle, scheduler, multi-target polling, and multi-flow orchestration.
- `tracertmonitor-analyzer`: path merging, ECMP share calculation, latency/loss/jitter aggregation, and suspicion evidence.
- `tracertmonitor-storage`: SQLite persistence for sessions, observations, paths, and events.
- `tracertmonitor-export`: JSON, CSV, HTML, and Markdown evidence export.
- `tracertmonitor-api`: local HTTP and future SSE/WebSocket API.
- `web/cockpit`: frontend cockpit assets, topology view, timeseries charts, and controls.

## Dependency Rules

- Web code must not depend on Trippy.
- API code must not directly send probes.
- Probe code must emit observations, not UI snapshots.
- Analyzer code must not schedule tasks or write SQLite.
- Storage code must not decide whether a path is suspicious.
- Export code must read data and evidence, not run probes.
- Trippy dependencies must stay behind the probe capability boundary.

## Migration Acceptance

- `cargo test -p tracertmonitor` passes inside the new workspace.
- `cargo run -p tracertmonitor` starts the local cockpit from the new workspace.
- The old `trippy` directory remains available as an upstream reference.
- The new workspace can be committed and pushed to `tanqizhi/tracertmonitor`.
