# Portable Network Diagnostics Cockpit MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first runnable vertical slice of TracertMonitor: a new single-binary crate with tested path evidence modeling, active-window aggregation, full evidence export, and a local Web cockpit using deterministic demo data.

**Architecture:** Add a new workspace crate named `tracertmonitor`. Keep model, analysis, export, demo data, server, and embedded assets separate so each unit is small and testable. Use deterministic demo observations first; live `trippy-core` probing comes in the next plan after the data contract and cockpit are verified.

**Tech Stack:** Rust 2024, `serde`, `serde_json`, `chrono`, `csv`, `thiserror`, standard-library `TcpListener`, embedded HTML/CSS/JavaScript assets, existing Trippy workspace conventions.

---

## Scope

This plan is the first executable milestone.

Included:

- New `tracertmonitor` workspace crate and binary.
- Stable path identity for observed ECMP paths.
- Unknown/no-response hop preservation.
- Active-window aggregation for full session, 5 minutes, 1 minute, and custom time ranges.
- Suspicious path annotation with evidence.
- Complete JSON export containing all paths, hops, observations, events, and suspicion data.
- CSV exports for path summary, hop summary, raw observations, and events.
- Local HTTP server with embedded Web cockpit assets.
- Browser cockpit shell with all observed paths, path shares, selectable paths, topology zoom/pan, custom window controls, export links, and hover readout.

Deferred:

- Live `trippy-core` probing.
- Administrator/root privilege checks.
- Real WebSocket or SSE streaming.
- Vendored production graph library.
- Windows/macOS packaging.

## File Structure

- Modify: `Cargo.toml`
  - Add `crates/tracertmonitor` to workspace members.

- Create: `crates/tracertmonitor/Cargo.toml`
  - New crate manifest.

- Create: `crates/tracertmonitor/src/lib.rs`
  - Module exports.

- Create: `crates/tracertmonitor/src/main.rs`
  - Starts the local cockpit using demo session data.

- Create: `crates/tracertmonitor/src/model.rs`
  - Serializable evidence model.

- Create: `crates/tracertmonitor/src/analyzer.rs`
  - Stable path IDs, window aggregation, suspicious path annotation.

- Create: `crates/tracertmonitor/src/demo.rs`
  - Deterministic ECMP demo session.

- Create: `crates/tracertmonitor/src/export.rs`
  - JSON and CSV exports.

- Create: `crates/tracertmonitor/src/server.rs`
  - Minimal local HTTP server and route handling.

- Create: `crates/tracertmonitor/src/assets/index.html`
  - Cockpit page shell.

- Create: `crates/tracertmonitor/src/assets/app.js`
  - Browser renderer and custom-window recalculation.

- Create: `crates/tracertmonitor/src/assets/styles.css`
  - Troubleshooting cockpit styling.

## Task 1: Add The Workspace Crate

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/tracertmonitor/Cargo.toml`
- Create: `crates/tracertmonitor/src/lib.rs`
- Create: `crates/tracertmonitor/src/main.rs`

- [ ] **Step 1: Add workspace member**

In root `Cargo.toml`, add:

```toml
"crates/tracertmonitor",
```

inside the `members` list before `"examples/*"`.

- [ ] **Step 2: Create crate manifest**

Create `crates/tracertmonitor/Cargo.toml`:

```toml
[package]
name = "tracertmonitor"
description = "A portable network diagnostics cockpit"
version.workspace = true
authors.workspace = true
homepage.workspace = true
repository.workspace = true
readme.workspace = true
license.workspace = true
edition.workspace = true
rust-version.workspace = true
keywords.workspace = true
categories.workspace = true

[dependencies]
chrono = { workspace = true, default-features = false, features = ["clock", "serde"] }
csv.workspace = true
serde = { workspace = true, default-features = false, features = ["derive", "std"] }
serde_json.workspace = true
thiserror.workspace = true
trippy-core.workspace = true

[lints]
workspace = true
```

- [ ] **Step 3: Create module exports**

Create `crates/tracertmonitor/src/lib.rs`:

```rust
pub mod analyzer;
pub mod demo;
pub mod export;
pub mod model;
pub mod server;
```

- [ ] **Step 4: Create binary entrypoint**

Create `crates/tracertmonitor/src/main.rs`:

```rust
use std::net::Ipv4Addr;

fn main() -> Result<(), tracertmonitor::server::ServerError> {
    let session = tracertmonitor::demo::demo_session();
    let server = tracertmonitor::server::CockpitServer::bind((Ipv4Addr::LOCALHOST, 0), session)?;
    println!("TracertMonitor cockpit: http://{}", server.local_addr()?);
    server.run()
}
```

- [ ] **Step 5: Verify expected failure**

Run:

```shell
cargo check -p tracertmonitor
```

Expected: fail because the declared modules do not exist yet.

- [ ] **Step 6: Commit**

```shell
git add Cargo.toml crates/tracertmonitor
git commit -m "feat: add tracertmonitor crate"
```

## Task 2: Add Serializable Evidence Model

**Files:**

- Create: `crates/tracertmonitor/src/model.rs`

- [ ] **Step 1: Write failing model test**

Create `crates/tracertmonitor/src/model.rs` with this test at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn serializes_unknown_hops_and_suspicion() {
        let session = TraceSession {
            target: Target {
                input: "example.com".to_string(),
                resolved: vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))],
            },
            started_at: Utc.with_ymd_and_hms(2026, 7, 2, 14, 0, 0).unwrap(),
            ended_at: None,
            paths: vec![PathEvidence {
                id: PathId("path-a".to_string()),
                label: "Path A".to_string(),
                hops: vec![HopEvidence {
                    ttl: 3,
                    node: HopNode::Unknown,
                    metrics: HopMetrics::default(),
                    classification: HopClassification::Informational,
                }],
                first_seen: Utc.with_ymd_and_hms(2026, 7, 2, 14, 0, 1).unwrap(),
                last_seen: Utc.with_ymd_and_hms(2026, 7, 2, 14, 0, 9).unwrap(),
                metrics: PathMetrics::default(),
                suspicion: Some(Suspicion {
                    severity: Severity::Warning,
                    reason: "path packet loss is above peer paths".to_string(),
                    evidence: vec!["Path A loss 12.5%; peers below 1%".to_string()],
                }),
            }],
            observations: Vec::new(),
            events: Vec::new(),
        };

        let json = serde_json::to_string(&session).unwrap();

        assert!(json.contains("\"unknown\""));
        assert!(json.contains("path packet loss"));
    }
}
```

- [ ] **Step 2: Verify red**

Run:

```shell
cargo test -p tracertmonitor model::tests::serializes_unknown_hops_and_suspicion
```

Expected: fail because the model types are missing.

- [ ] **Step 3: Implement model**

Add this above the tests:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSession {
    pub target: Target,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub paths: Vec<PathEvidence>,
    pub observations: Vec<PathObservation>,
    pub events: Vec<DiagnosticEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub input: String,
    pub resolved: Vec<IpAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathEvidence {
    pub id: PathId,
    pub label: String,
    pub hops: Vec<HopEvidence>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub metrics: PathMetrics,
    pub suspicion: Option<Suspicion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathObservation {
    pub observed_at: DateTime<Utc>,
    pub path_id: PathId,
    pub hops: Vec<HopEvidence>,
    pub rtt_ms: Option<f64>,
    pub lost: bool,
    pub jitter_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HopEvidence {
    pub ttl: u8,
    pub node: HopNode,
    pub metrics: HopMetrics,
    pub classification: HopClassification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HopNode {
    Known { ip: IpAddr, hostname: Option<String> },
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HopClassification {
    #[default]
    Normal,
    Informational,
    Suspicious,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct HopMetrics {
    pub sent: usize,
    pub recv: usize,
    pub loss_pct: f64,
    pub last_ms: Option<f64>,
    pub avg_ms: f64,
    pub best_ms: Option<f64>,
    pub worst_ms: Option<f64>,
    pub stddev_ms: f64,
    pub jitter_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct PathMetrics {
    pub hit_count: usize,
    pub sample_count: usize,
    pub window_share_pct: f64,
    pub loss_pct: f64,
    pub avg_rtt_ms: f64,
    pub avg_jitter_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suspicion {
    pub severity: Severity,
    pub reason: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub at: DateTime<Utc>,
    pub severity: Severity,
    pub kind: DiagnosticEventKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticEventKind {
    PathAppeared,
    PathDisappeared,
    LossSpike,
    LatencySpike,
    JitterIncrease,
    TargetUnreachable,
    DnsChanged,
    EvidenceInsufficient,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
```

- [ ] **Step 4: Verify green**

Run:

```shell
cargo test -p tracertmonitor model::tests::serializes_unknown_hops_and_suspicion
```

Expected: pass.

- [ ] **Step 5: Commit**

```shell
git add crates/tracertmonitor/src/model.rs
git commit -m "feat: add diagnostics evidence model"
```

## Task 3: Aggregate Paths By Window

**Files:**

- Create: `crates/tracertmonitor/src/analyzer.rs`

- [ ] **Step 1: Write failing analyzer tests**

Create `crates/tracertmonitor/src/analyzer.rs` with tests for stable path IDs and window aggregation:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HopClassification, HopEvidence, HopMetrics, HopNode, PathObservation, TimeWindow};
    use chrono::{TimeZone, Utc};
    use std::net::{IpAddr, Ipv4Addr};

    fn known(ttl: u8, last_octet: u8) -> HopEvidence {
        HopEvidence {
            ttl,
            node: HopNode::Known {
                ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, last_octet)),
                hostname: None,
            },
            metrics: HopMetrics::default(),
            classification: HopClassification::Normal,
        }
    }

    fn unknown(ttl: u8) -> HopEvidence {
        HopEvidence {
            ttl,
            node: HopNode::Unknown,
            metrics: HopMetrics::default(),
            classification: HopClassification::Informational,
        }
    }

    #[test]
    fn stable_path_id_preserves_unknown_hops() {
        let left = stable_path_id(&[known(1, 1), unknown(2), known(3, 3)]);
        let right = stable_path_id(&[known(1, 1), unknown(2), known(3, 3)]);

        assert_eq!(left, right);
        assert!(left.0.contains("ttl2:*"));
    }

    #[test]
    fn aggregate_window_calculates_share_and_flags_loss() {
        let start = Utc.with_ymd_and_hms(2026, 7, 2, 14, 0, 0).unwrap();
        let path_a = stable_path_id(&[known(1, 1), known(2, 2)]);
        let path_b = stable_path_id(&[known(1, 1), known(2, 9)]);
        let observations = vec![
            PathObservation { observed_at: start, path_id: path_a.clone(), hops: vec![known(1, 1), known(2, 2)], rtt_ms: Some(40.0), lost: false, jitter_ms: Some(3.0) },
            PathObservation { observed_at: start + chrono::Duration::seconds(1), path_id: path_b.clone(), hops: vec![known(1, 1), known(2, 9)], rtt_ms: Some(180.0), lost: true, jitter_ms: Some(35.0) },
            PathObservation { observed_at: start + chrono::Duration::seconds(2), path_id: path_b.clone(), hops: vec![known(1, 1), known(2, 9)], rtt_ms: Some(190.0), lost: true, jitter_ms: Some(41.0) },
        ];

        let snapshot = aggregate_window(
            &observations,
            TimeWindow { start, end: start + chrono::Duration::seconds(10) },
        );

        let bad_path = snapshot.paths.iter().find(|path| path.id == path_b).unwrap();
        assert_eq!(2, bad_path.metrics.hit_count);
        assert!((bad_path.metrics.window_share_pct - 66.666).abs() < 0.01);
        assert!(bad_path.suspicion.is_some());
    }
}
```

- [ ] **Step 2: Verify red**

Run:

```shell
cargo test -p tracertmonitor analyzer::tests
```

Expected: fail because analyzer functions do not exist.

- [ ] **Step 3: Implement analyzer**

Implement `stable_path_id`, `aggregate_window`, and helper functions:

```rust
use crate::model::{
    HopEvidence, HopNode, PathEvidence, PathId, PathMetrics, PathObservation, Severity, Suspicion,
    TimeWindow,
};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone)]
pub struct WindowSnapshot {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub paths: Vec<PathEvidence>,
}

pub fn stable_path_id(hops: &[HopEvidence]) -> PathId {
    let mut id = String::from("path:");
    for hop in hops {
        let _ = write!(&mut id, "ttl{}:", hop.ttl);
        match &hop.node {
            HopNode::Known { ip, .. } => {
                let _ = write!(&mut id, "{ip}");
            }
            HopNode::Unknown => id.push('*'),
        }
        id.push('|');
    }
    PathId(id)
}

pub fn aggregate_window(observations: &[PathObservation], window: TimeWindow) -> WindowSnapshot {
    let in_window = observations
        .iter()
        .filter(|observation| observation.observed_at >= window.start && observation.observed_at <= window.end)
        .collect::<Vec<_>>();
    let total = in_window.len().max(1);
    let mut grouped: BTreeMap<String, Vec<&PathObservation>> = BTreeMap::new();

    for observation in in_window {
        grouped.entry(observation.path_id.0.clone()).or_default().push(observation);
    }

    let mut paths = grouped
        .into_iter()
        .enumerate()
        .map(|(idx, (id, observations))| {
            let hit_count = observations.len();
            let lost_count = observations.iter().filter(|observation| observation.lost).count();
            let avg_rtt_ms = average(observations.iter().filter_map(|observation| observation.rtt_ms));
            let avg_jitter_ms = average(observations.iter().filter_map(|observation| observation.jitter_ms));
            let loss_pct = lost_count as f64 / hit_count.max(1) as f64 * 100.0;
            let first_seen = observations.first().unwrap().observed_at;
            let last_seen = observations.last().unwrap().observed_at;
            let hops = observations.last().unwrap().hops.clone();
            let metrics = PathMetrics {
                hit_count,
                sample_count: hit_count,
                window_share_pct: hit_count as f64 / total as f64 * 100.0,
                loss_pct,
                avg_rtt_ms,
                avg_jitter_ms,
            };
            PathEvidence {
                id: PathId(id),
                label: format!("Path {}", (b'A' + idx as u8) as char),
                hops,
                first_seen,
                last_seen,
                metrics,
                suspicion: suspicion_for(metrics),
            }
        })
        .collect::<Vec<_>>();

    paths.sort_by(|left, right| right.metrics.hit_count.cmp(&left.metrics.hit_count));

    WindowSnapshot {
        window_start: window.start,
        window_end: window.end,
        paths,
    }
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn suspicion_for(metrics: PathMetrics) -> Option<Suspicion> {
    if metrics.sample_count < 2 {
        return None;
    }
    if metrics.loss_pct >= 10.0 {
        return Some(Suspicion {
            severity: Severity::Critical,
            reason: "path packet loss is above 10%".to_string(),
            evidence: vec![format!("loss {:.1}% across {} samples", metrics.loss_pct, metrics.sample_count)],
        });
    }
    if metrics.loss_pct >= 5.0 {
        return Some(Suspicion {
            severity: Severity::Warning,
            reason: "path packet loss is above 5%".to_string(),
            evidence: vec![format!("loss {:.1}% across {} samples", metrics.loss_pct, metrics.sample_count)],
        });
    }
    None
}
```

- [ ] **Step 4: Verify green**

Run:

```shell
cargo test -p tracertmonitor analyzer::tests
```

Expected: pass.

- [ ] **Step 5: Commit**

```shell
git add crates/tracertmonitor/src/analyzer.rs
git commit -m "feat: aggregate diagnostic paths by window"
```

## Task 4: Add Deterministic Demo Evidence

**Files:**

- Create: `crates/tracertmonitor/src/demo.rs`

- [ ] **Step 1: Write failing demo test**

Create a test that proves demo data has multiple paths, a suspicious path, and an unknown hop:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_session_contains_multiple_paths_unknown_hops_and_suspicion() {
        let session = demo_session();

        assert!(session.paths.len() >= 3);
        assert!(session.paths.iter().any(|path| path.suspicion.is_some()));
        assert!(session.paths.iter().flat_map(|path| &path.hops).any(|hop| matches!(hop.node, crate::model::HopNode::Unknown)));
    }
}
```

- [ ] **Step 2: Verify red**

Run:

```shell
cargo test -p tracertmonitor demo::tests
```

Expected: fail because `demo_session` does not exist.

- [ ] **Step 3: Implement deterministic demo**

Implement a session with three paths, one unknown TTL, and Path B loss/latency:

```rust
use crate::analyzer::{aggregate_window, stable_path_id};
use crate::model::{
    DiagnosticEvent, DiagnosticEventKind, HopClassification, HopEvidence, HopMetrics, HopNode,
    PathObservation, Severity, Target, TimeWindow, TraceSession,
};
use chrono::{TimeZone, Utc};
use std::net::{IpAddr, Ipv4Addr};

pub fn demo_session() -> TraceSession {
    let started_at = Utc.with_ymd_and_hms(2026, 7, 2, 14, 0, 0).unwrap();
    let path_a = vec![known(1, 192, 168, 1, 1), known(2, 10, 0, 0, 1), unknown(3), known(4, 203, 0, 113, 10)];
    let path_b = vec![known(1, 192, 168, 1, 1), known(2, 10, 0, 0, 9), unknown(3), known(4, 203, 0, 113, 10)];
    let path_c = vec![known(1, 192, 168, 1, 1), known(2, 10, 0, 0, 5), known(3, 198, 51, 100, 2), known(4, 203, 0, 113, 10)];
    let id_a = stable_path_id(&path_a);
    let id_b = stable_path_id(&path_b);
    let id_c = stable_path_id(&path_c);

    let mut observations = Vec::new();
    for i in 0..30 {
        let observed_at = started_at + chrono::Duration::seconds(i * 10);
        let (path_id, hops, rtt_ms, lost, jitter_ms) = if i % 10 < 5 {
            (id_a.clone(), path_a.clone(), Some(42.0 + f64::from(i % 4)), false, Some(3.0))
        } else if i % 10 < 8 {
            (id_b.clone(), path_b.clone(), Some(185.0 + f64::from(i % 6)), i % 2 == 0, Some(38.0))
        } else {
            (id_c.clone(), path_c.clone(), Some(55.0 + f64::from(i % 5)), false, Some(5.0))
        };
        observations.push(PathObservation { observed_at, path_id, hops, rtt_ms, lost, jitter_ms });
    }

    let window = TimeWindow {
        start: started_at,
        end: started_at + chrono::Duration::minutes(5),
    };
    let snapshot = aggregate_window(&observations, window);

    TraceSession {
        target: Target {
            input: "example.com".to_string(),
            resolved: vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))],
        },
        started_at,
        ended_at: Some(window.end),
        paths: snapshot.paths,
        observations,
        events: vec![DiagnosticEvent {
            at: started_at + chrono::Duration::seconds(50),
            severity: Severity::Warning,
            kind: DiagnosticEventKind::LossSpike,
            message: "Path B shows higher latency and packet loss than peer paths".to_string(),
        }],
    }
}

fn known(ttl: u8, a: u8, b: u8, c: u8, d: u8) -> HopEvidence {
    HopEvidence {
        ttl,
        node: HopNode::Known { ip: IpAddr::V4(Ipv4Addr::new(a, b, c, d)), hostname: None },
        metrics: HopMetrics::default(),
        classification: HopClassification::Normal,
    }
}

fn unknown(ttl: u8) -> HopEvidence {
    HopEvidence {
        ttl,
        node: HopNode::Unknown,
        metrics: HopMetrics::default(),
        classification: HopClassification::Informational,
    }
}
```

- [ ] **Step 4: Verify green**

Run:

```shell
cargo test -p tracertmonitor demo::tests
```

Expected: pass.

- [ ] **Step 5: Commit**

```shell
git add crates/tracertmonitor/src/demo.rs
git commit -m "feat: add deterministic diagnostic demo session"
```

## Task 5: Export Complete Evidence

**Files:**

- Create: `crates/tracertmonitor/src/export.rs`

- [ ] **Step 1: Write failing export tests**

Create tests for JSON and every CSV table:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::demo_session;

    #[test]
    fn json_export_contains_all_evidence() {
        let json = export_json(&demo_session()).unwrap();

        assert!(json.contains("\"paths\""));
        assert!(json.contains("\"observations\""));
        assert!(json.contains("\"events\""));
        assert!(json.contains("\"suspicion\""));
    }

    #[test]
    fn csv_export_contains_all_tables() {
        let csv = export_csv_bundle(&demo_session()).unwrap();

        assert!(csv.path_summary.contains("PathId,Label,HitCount,WindowSharePct,LossPct,AvgRttMs,AvgJitterMs,Suspicion"));
        assert!(csv.hop_summary.contains("PathId,Label,Ttl,NodeKind,Address,Classification,LossPct,AvgMs,JitterMs"));
        assert!(csv.observations.contains("ObservedAt,PathId,RttMs,Lost,JitterMs,HopCount"));
        assert!(csv.events.contains("At,Severity,Kind,Message"));
    }
}
```

- [ ] **Step 2: Verify red**

Run:

```shell
cargo test -p tracertmonitor export::tests
```

Expected: fail because export functions do not exist.

- [ ] **Step 3: Implement exports**

Implement `export_json`, `export_csv_bundle`, and table writers:

```rust
use crate::model::{HopNode, TraceSession};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct CsvBundle {
    pub path_summary: String,
    pub hop_summary: String,
    pub observations: String,
    pub events: String,
}

pub fn export_json(session: &TraceSession) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(session)
}

pub fn export_csv_bundle(session: &TraceSession) -> Result<CsvBundle, csv::Error> {
    Ok(CsvBundle {
        path_summary: export_path_summary(session)?,
        hop_summary: export_hop_summary(session)?,
        observations: export_observations(session)?,
        events: export_events(session)?,
    })
}

fn finish_csv(writer: csv::Writer<Vec<u8>>) -> Result<String, csv::Error> {
    let data = writer.into_inner().map_err(|err| err.into_error().into())?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

fn export_path_summary(session: &TraceSession) -> Result<String, csv::Error> {
    #[derive(Serialize)]
    struct Row {
        #[serde(rename = "PathId")]
        path_id: String,
        #[serde(rename = "Label")]
        label: String,
        #[serde(rename = "HitCount")]
        hit_count: usize,
        #[serde(rename = "WindowSharePct")]
        window_share_pct: f64,
        #[serde(rename = "LossPct")]
        loss_pct: f64,
        #[serde(rename = "AvgRttMs")]
        avg_rtt_ms: f64,
        #[serde(rename = "AvgJitterMs")]
        avg_jitter_ms: f64,
        #[serde(rename = "Suspicion")]
        suspicion: String,
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    for path in &session.paths {
        writer.serialize(Row {
            path_id: path.id.0.clone(),
            label: path.label.clone(),
            hit_count: path.metrics.hit_count,
            window_share_pct: path.metrics.window_share_pct,
            loss_pct: path.metrics.loss_pct,
            avg_rtt_ms: path.metrics.avg_rtt_ms,
            avg_jitter_ms: path.metrics.avg_jitter_ms,
            suspicion: path.suspicion.as_ref().map_or_else(String::new, |s| s.reason.clone()),
        })?;
    }
    finish_csv(writer)
}

fn export_hop_summary(session: &TraceSession) -> Result<String, csv::Error> {
    #[derive(Serialize)]
    struct Row {
        #[serde(rename = "PathId")]
        path_id: String,
        #[serde(rename = "Label")]
        label: String,
        #[serde(rename = "Ttl")]
        ttl: u8,
        #[serde(rename = "NodeKind")]
        node_kind: String,
        #[serde(rename = "Address")]
        address: String,
        #[serde(rename = "Classification")]
        classification: String,
        #[serde(rename = "LossPct")]
        loss_pct: f64,
        #[serde(rename = "AvgMs")]
        avg_ms: f64,
        #[serde(rename = "JitterMs")]
        jitter_ms: String,
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    for path in &session.paths {
        for hop in &path.hops {
            let (node_kind, address) = match &hop.node {
                HopNode::Known { ip, hostname } => ("known".to_string(), hostname.clone().unwrap_or_else(|| ip.to_string())),
                HopNode::Unknown => ("unknown".to_string(), "*".to_string()),
            };
            writer.serialize(Row {
                path_id: path.id.0.clone(),
                label: path.label.clone(),
                ttl: hop.ttl,
                node_kind,
                address,
                classification: format!("{:?}", hop.classification),
                loss_pct: hop.metrics.loss_pct,
                avg_ms: hop.metrics.avg_ms,
                jitter_ms: hop.metrics.jitter_ms.map_or_else(String::new, |value| value.to_string()),
            })?;
        }
    }
    finish_csv(writer)
}

fn export_observations(session: &TraceSession) -> Result<String, csv::Error> {
    #[derive(Serialize)]
    struct Row {
        #[serde(rename = "ObservedAt")]
        observed_at: String,
        #[serde(rename = "PathId")]
        path_id: String,
        #[serde(rename = "RttMs")]
        rtt_ms: String,
        #[serde(rename = "Lost")]
        lost: bool,
        #[serde(rename = "JitterMs")]
        jitter_ms: String,
        #[serde(rename = "HopCount")]
        hop_count: usize,
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    for observation in &session.observations {
        writer.serialize(Row {
            observed_at: observation.observed_at.to_rfc3339(),
            path_id: observation.path_id.0.clone(),
            rtt_ms: observation.rtt_ms.map_or_else(String::new, |value| value.to_string()),
            lost: observation.lost,
            jitter_ms: observation.jitter_ms.map_or_else(String::new, |value| value.to_string()),
            hop_count: observation.hops.len(),
        })?;
    }
    finish_csv(writer)
}

fn export_events(session: &TraceSession) -> Result<String, csv::Error> {
    #[derive(Serialize)]
    struct Row {
        #[serde(rename = "At")]
        at: String,
        #[serde(rename = "Severity")]
        severity: String,
        #[serde(rename = "Kind")]
        kind: String,
        #[serde(rename = "Message")]
        message: String,
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    for event in &session.events {
        writer.serialize(Row {
            at: event.at.to_rfc3339(),
            severity: format!("{:?}", event.severity),
            kind: format!("{:?}", event.kind),
            message: event.message.clone(),
        })?;
    }
    finish_csv(writer)
}
```

- [ ] **Step 4: Verify green**

Run:

```shell
cargo test -p tracertmonitor export::tests
```

Expected: pass.

- [ ] **Step 5: Commit**

```shell
git add crates/tracertmonitor/src/export.rs
git commit -m "feat: export complete diagnostic evidence"
```

## Task 6: Serve The Embedded Cockpit

**Files:**

- Create: `crates/tracertmonitor/src/server.rs`
- Create: `crates/tracertmonitor/src/assets/index.html`
- Create: `crates/tracertmonitor/src/assets/app.js`
- Create: `crates/tracertmonitor/src/assets/styles.css`

- [ ] **Step 1: Write failing server tests**

Create route tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::demo_session;

    #[test]
    fn route_index_returns_html() {
        let response = response_for_path("/", &demo_session()).unwrap();

        assert_eq!(200, response.status);
        assert_eq!("text/html; charset=utf-8", response.content_type);
        assert!(String::from_utf8_lossy(&response.body).contains("TracertMonitor"));
    }

    #[test]
    fn route_exports_complete_csv_tables() {
        let session = demo_session();

        assert!(String::from_utf8_lossy(&response_for_path("/export/hop-summary.csv", &session).unwrap().body).contains("NodeKind"));
        assert!(String::from_utf8_lossy(&response_for_path("/export/observations.csv", &session).unwrap().body).contains("ObservedAt"));
    }
}
```

- [ ] **Step 2: Verify red**

Run:

```shell
cargo test -p tracertmonitor server::tests
```

Expected: fail because server route handling is missing.

- [ ] **Step 3: Create minimal assets**

Create `index.html`, `app.js`, and `styles.css` with temporary compile-safe content:

```html
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <title>TracertMonitor</title>
  <link rel="stylesheet" href="/assets/styles.css">
</head>
<body>
  <main id="app">TracertMonitor</main>
  <script src="/assets/app.js"></script>
</body>
</html>
```

```javascript
console.log("TracertMonitor cockpit loaded");
```

```css
body {
  font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
}
```

- [ ] **Step 4: Implement route handling**

Implement local routes:

```rust
use crate::export::{export_csv_bundle, export_json};
use crate::model::TraceSession;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use thiserror::Error;

const INDEX_HTML: &str = include_str!("assets/index.html");
const APP_JS: &str = include_str!("assets/app.js");
const STYLES_CSS: &str = include_str!("assets/styles.css");

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json export error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("csv export error: {0}")]
    Csv(#[from] csv::Error),
}

pub struct CockpitServer {
    listener: TcpListener,
    session: TraceSession,
}

impl CockpitServer {
    pub fn bind(addr: impl ToSocketAddrs, session: TraceSession) -> Result<Self, ServerError> {
        Ok(Self { listener: TcpListener::bind(addr)?, session })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, ServerError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn run(self) -> Result<(), ServerError> {
        for stream in self.listener.incoming() {
            handle_stream(stream?, &self.session)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct RouteResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

pub fn response_for_path(path: &str, session: &TraceSession) -> Result<RouteResponse, ServerError> {
    match path {
        "/" | "/index.html" => Ok(text_response(200, "text/html; charset=utf-8", INDEX_HTML)),
        "/assets/app.js" => Ok(text_response(200, "text/javascript; charset=utf-8", APP_JS)),
        "/assets/styles.css" => Ok(text_response(200, "text/css; charset=utf-8", STYLES_CSS)),
        "/api/session" | "/export/session.json" => Ok(text_response(200, "application/json; charset=utf-8", &export_json(session)?)),
        "/export/path-summary.csv" => Ok(text_response(200, "text/csv; charset=utf-8", &export_csv_bundle(session)?.path_summary)),
        "/export/hop-summary.csv" => Ok(text_response(200, "text/csv; charset=utf-8", &export_csv_bundle(session)?.hop_summary)),
        "/export/observations.csv" => Ok(text_response(200, "text/csv; charset=utf-8", &export_csv_bundle(session)?.observations)),
        "/export/events.csv" => Ok(text_response(200, "text/csv; charset=utf-8", &export_csv_bundle(session)?.events)),
        _ => Ok(text_response(404, "text/plain; charset=utf-8", "not found")),
    }
}

fn handle_stream(mut stream: TcpStream, session: &TraceSession) -> Result<(), ServerError> {
    let mut buffer = [0_u8; 2048];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");
    let response = response_for_path(path, session)?;
    let status_text = if response.status == 200 { "OK" } else { "Not Found" };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        status_text,
        response.content_type,
        response.body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&response.body)?;
    Ok(())
}

fn text_response(status: u16, content_type: &'static str, body: &str) -> RouteResponse {
    RouteResponse { status, content_type, body: body.as_bytes().to_vec() }
}
```

- [ ] **Step 5: Verify green**

Run:

```shell
cargo test -p tracertmonitor server::tests
```

Expected: pass.

- [ ] **Step 6: Commit**

```shell
git add crates/tracertmonitor/src/server.rs crates/tracertmonitor/src/assets
git commit -m "feat: serve embedded diagnostics cockpit"
```

## Task 7: Build The Browser Cockpit

**Files:**

- Modify: `crates/tracertmonitor/src/assets/index.html`
- Modify: `crates/tracertmonitor/src/assets/app.js`
- Modify: `crates/tracertmonitor/src/assets/styles.css`

- [ ] **Step 1: Replace HTML with cockpit layout**

Create sections for session bar, suspicion summary, topology, detail panel, window controls, path lanes, and export links. Include export links for JSON, path CSV, hop CSV, observation CSV, and event CSV.

- [ ] **Step 2: Implement app renderer**

Implement JavaScript state and render functions:

```javascript
const state = {
  session: null,
  activePaths: [],
  selectedPathId: null,
  windowMode: "full",
  customStart: null,
  customEnd: null,
  zoom: 1,
  panX: 0,
  panY: 0,
};

async function boot() {
  const response = await fetch("/api/session");
  state.session = await response.json();
  state.activePaths = state.session.paths;
  state.selectedPathId = state.session.paths[0]?.id ?? null;
  render();
}

function pathsForActiveWindow() {
  const range = currentWindowRange();
  const observations = state.session.observations.filter((observation) => {
    const observedAt = new Date(observation.observed_at);
    return observedAt >= range.start && observedAt <= range.end;
  });
  const byPath = new Map();
  for (const observation of observations) {
    if (!byPath.has(observation.path_id)) byPath.set(observation.path_id, []);
    byPath.get(observation.path_id).push(observation);
  }
  const total = Math.max(1, observations.length);
  return state.session.paths
    .filter((path) => byPath.has(path.id))
    .map((path) => {
      const hits = byPath.get(path.id);
      const lost = hits.filter((hit) => hit.lost).length;
      return {
        ...path,
        metrics: {
          ...path.metrics,
          hit_count: hits.length,
          sample_count: hits.length,
          window_share_pct: hits.length / total * 100,
          loss_pct: lost / Math.max(1, hits.length) * 100,
          avg_rtt_ms: average(hits.map((hit) => hit.rtt_ms).filter((value) => value !== null)),
          avg_jitter_ms: average(hits.map((hit) => hit.jitter_ms).filter((value) => value !== null)),
        },
      };
    });
}

function currentWindowRange() {
  const times = state.session.observations.map((observation) => new Date(observation.observed_at));
  const first = new Date(Math.min(...times));
  const last = new Date(Math.max(...times));
  if (state.windowMode === "1m") return { start: new Date(last.getTime() - 60_000), end: last };
  if (state.windowMode === "5m") return { start: new Date(last.getTime() - 300_000), end: last };
  if (state.windowMode === "custom" && state.customStart && state.customEnd) {
    return { start: new Date(state.customStart), end: new Date(state.customEnd) };
  }
  return { start: first, end: last };
}

function average(values) {
  if (!values.length) return 0;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}
```

Complete the renderer with:

- `render()` recalculating `state.activePaths = pathsForActiveWindow()`.
- `renderTopology()` drawing all active paths as SVG rows with unknown hops.
- Mouse wheel buttons or controls adjusting `zoom`.
- Pointer drag adjusting `panX` and `panY`.
- `renderLanes()` showing each path and updating hover readout with exact RTT/loss/jitter/share.
- Custom time inputs appearing when the window selector is `custom`.

- [ ] **Step 3: Add dense cockpit styling**

Use a restrained operational palette with full-screen grid layout, compact panels, clear path status colors, visible unknown hop styling, and responsive single-column layout under 980px.

- [ ] **Step 4: Run check**

Run:

```shell
cargo check -p tracertmonitor
```

Expected: pass because static assets are embedded at compile time.

- [ ] **Step 5: Commit**

```shell
git add crates/tracertmonitor/src/assets
git commit -m "feat: add diagnostics cockpit UI"
```

## Task 8: Verify MVP

**Files:**

- No new files.

- [ ] **Step 1: Run crate tests**

Run:

```shell
cargo test -p tracertmonitor
```

Expected: all `tracertmonitor` tests pass.

- [ ] **Step 2: Run workspace check**

Run:

```shell
cargo check --workspace --all-features --tests
```

Expected: workspace check passes.

- [ ] **Step 3: Run local cockpit**

Run:

```shell
cargo run -p tracertmonitor
```

Expected output:

```text
TracertMonitor cockpit: http://127.0.0.1:<port>
```

Manual browser checks:

- Target appears in the header.
- Multiple paths appear in the topology.
- Unknown hop appears as a visible node.
- Zoom and pan controls work.
- Clicking a path updates the details panel.
- Built-in windows change path shares.
- Custom start/end window changes path shares.
- Hovering a path lane shows exact values.
- JSON and all CSV export links return data.

- [ ] **Step 4: Commit verification fixes**

If verification reveals small issues, fix them using TDD where applicable and commit:

```shell
git add Cargo.toml Cargo.lock crates/tracertmonitor
git commit -m "fix: stabilize tracertmonitor MVP"
```

## Self-Review

Spec coverage:

- Single executable shape: covered by new binary crate.
- Local Web cockpit: covered by Tasks 6 and 7.
- All observed paths: covered by Tasks 3, 4, and 7.
- Unknown/no-response hop preservation: covered by Tasks 2, 3, 4, and 7.
- Path appearance share: covered by Tasks 3 and 7.
- Built-in and custom windows: covered by Task 7.
- Hover exact values: covered by Task 7.
- Complete JSON export: covered by Task 5.
- Complete CSV export for paths, hops, observations, and events: covered by Task 5 and Task 6.
- Suspicious path annotation: covered by Tasks 3, 4, and 5.

Planned follow-up specs:

- Live `trippy-core` probing and privilege checks.
- Production graph library integration.
- WebSocket/SSE live updates.
- Windows/macOS packaging.

Placeholder scan:

- No `TBD`, `TODO`, or placeholder-only steps.

Type consistency:

- `PathId`, `TraceSession`, `PathEvidence`, `HopEvidence`, `PathObservation`, `TimeWindow`, and `Suspicion` are defined before later tasks use them.
- Export, server, and front-end tasks consume the same `TraceSession` shape.
