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
