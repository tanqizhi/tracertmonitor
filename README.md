# TracertMonitor

TracertMonitor is a portable network diagnostics cockpit built as an independent product workspace.

The project vendors the Trippy probing libraries under `vendor/trippy/` so the product can reuse Trippy's packet, privilege, DNS, and tracing capabilities without staying inside the upstream Trippy repository.

## Current Layout

```text
apps/tracertmonitor/              Current runnable cockpit application
vendor/trippy/crates/trippy-core/ Vendored Trippy tracing core
vendor/trippy/crates/trippy-packet/
vendor/trippy/crates/trippy-privilege/
vendor/trippy/crates/trippy-dns/
docs/superpowers/                 Product specs and implementation plans
```

## Run

```powershell
cargo run -p tracertmonitor
```

The program prints a local URL such as `http://127.0.0.1:1551`.
