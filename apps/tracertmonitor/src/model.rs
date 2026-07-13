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
pub struct TargetEndpoint {
    pub input: String,
    pub resolved: Vec<IpAddr>,
    pub protocol: ProbeProtocol,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeProtocol {
    Tcp,
    Icmp,
}
const DEFAULT_DISCOVERY_MAX_TTL: u8 = 30;
const DEFAULT_DISCOVERY_ROUNDS: u8 = 3;
const DEFAULT_DISCOVERY_PROBES_PER_TTL: u8 = 3;
const DEFAULT_PACKET_INTERVAL_MS: u64 = 1_000;
const DEFAULT_WINDOW_SECONDS: u64 = 60;
const DEFAULT_GEOIP_TIMEOUT_MS: u64 = 1_500;
const DEFAULT_GEOIP_CACHE_TTL_SECONDS: u64 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticConfig {
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    #[serde(default)]
    pub timing: TimingConfig,
    #[serde(default)]
    pub geoip: GeoIpConfig,
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self {
            discovery: DiscoveryConfig::default(),
            timing: TimingConfig::default(),
            geoip: GeoIpConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    #[serde(default = "default_discovery_max_ttl")]
    pub max_ttl: u8,
    #[serde(default = "default_discovery_rounds")]
    pub rounds: u8,
    #[serde(default = "default_discovery_probes_per_ttl")]
    pub probes_per_ttl: u8,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            max_ttl: DEFAULT_DISCOVERY_MAX_TTL,
            rounds: DEFAULT_DISCOVERY_ROUNDS,
            probes_per_ttl: DEFAULT_DISCOVERY_PROBES_PER_TTL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimingConfig {
    #[serde(default = "default_packet_interval_ms")]
    pub packet_interval_ms: u64,
    #[serde(default = "default_window_seconds")]
    pub window_seconds: u64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            packet_interval_ms: DEFAULT_PACKET_INTERVAL_MS,
            window_seconds: DEFAULT_WINDOW_SECONDS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoIpConfig {
    #[serde(default)]
    pub online_enabled: bool,
    #[serde(default = "default_geoip_preset")]
    pub preset: String,
    #[serde(default)]
    pub url_template: Option<String>,
    #[serde(default)]
    pub local_db_path: Option<String>,
    #[serde(default = "default_geoip_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_geoip_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,
    #[serde(default = "default_skip_private_or_reserved")]
    pub skip_private_or_reserved: bool,
}

impl Default for GeoIpConfig {
    fn default() -> Self {
        Self {
            online_enabled: false,
            preset: default_geoip_preset(),
            url_template: None,
            local_db_path: None,
            timeout_ms: DEFAULT_GEOIP_TIMEOUT_MS,
            cache_ttl_seconds: DEFAULT_GEOIP_CACHE_TTL_SECONDS,
            skip_private_or_reserved: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoIpInfo {
    pub country_or_region: Option<String>,
    pub province: Option<String>,
    pub city: Option<String>,
    pub carrier_or_asn: Option<String>,
    pub source: Option<String>,
    pub status: GeoIpLookupStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoIpLookupStatus {
    OnlineHit,
    OnlineFailedLocalHit,
    LocalHit,
    Unknown,
    SkippedPrivateOrReserved,
}

fn default_discovery_max_ttl() -> u8 {
    DEFAULT_DISCOVERY_MAX_TTL
}

fn default_discovery_rounds() -> u8 {
    DEFAULT_DISCOVERY_ROUNDS
}

fn default_discovery_probes_per_ttl() -> u8 {
    DEFAULT_DISCOVERY_PROBES_PER_TTL
}

fn default_packet_interval_ms() -> u64 {
    DEFAULT_PACKET_INTERVAL_MS
}

fn default_window_seconds() -> u64 {
    DEFAULT_WINDOW_SECONDS
}

fn default_geoip_preset() -> String {
    "none".to_string()
}

fn default_geoip_timeout_ms() -> u64 {
    DEFAULT_GEOIP_TIMEOUT_MS
}

fn default_geoip_cache_ttl_seconds() -> u64 {
    DEFAULT_GEOIP_CACHE_TTL_SECONDS
}

fn default_skip_private_or_reserved() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReachabilityCheck {
    pub checked_at: DateTime<Utc>,
    pub kind: ReachabilityKind,
    pub status: ReachabilityStatus,
    pub remote_addr: Option<IpAddr>,
    pub rtt_ms: Option<f64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReachabilityKind {
    Dns,
    TcpPort,
    IcmpEcho,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReachabilityStatus {
    Reachable,
    Unreachable,
    BlockedOrFiltered,
    Unchecked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRound {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub flow: FlowKey,
    pub samples: Vec<TtlProbeSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowKey {
    pub id: String,
    pub protocol: ProbeProtocol,
    pub source_port: Option<u16>,
    pub destination_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlProbeSample {
    pub sampled_at: DateTime<Utc>,
    pub ttl: u8,
    pub response: ProbeResponse,
    pub rtt_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProbeResponse {
    TtlExceeded {
        responder: IpAddr,
        hostname: Option<String>,
    },
    TcpSynAck {
        responder: IpAddr,
    },
    TcpRst {
        responder: IpAddr,
    },
    IcmpEchoReply {
        responder: IpAddr,
    },
    Timeout,
    OtherIcmp {
        responder: IpAddr,
        icmp_type: u8,
        icmp_code: u8,
    },
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
    Known {
        ip: IpAddr,
        hostname: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        geoip: Option<GeoIpInfo>,
    },
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
    PrecheckCompleted,
    DiscoveryUpdated,
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
    fn serializes_diagnostic_config_with_geoip_defaults() {
        let config = DiagnosticConfig::default();
        let json = serde_json::to_value(&config).unwrap();

        assert_eq!(json["discovery"]["max_ttl"], 30);
        assert_eq!(json["discovery"]["rounds"], 3);
        assert_eq!(json["discovery"]["probes_per_ttl"], 3);
        assert_eq!(json["timing"]["packet_interval_ms"], 1000);
        assert_eq!(json["timing"]["window_seconds"], 60);
        assert_eq!(json["geoip"]["online_enabled"], false);
        assert_eq!(json["geoip"]["timeout_ms"], 1500);
        assert_eq!(json["geoip"]["cache_ttl_seconds"], 86400);
        assert_eq!(json["geoip"]["skip_private_or_reserved"], true);

        let partial: DiagnosticConfig = serde_json::from_value(serde_json::json!({
            "geoip": {
                "online_enabled": true,
                "preset": "custom",
                "url_template": "https://geo.example.test/{ip}",
                "skip_private_or_reserved": false
            }
        }))
        .unwrap();

        assert_eq!(30, partial.discovery.max_ttl);
        assert_eq!(1000, partial.timing.packet_interval_ms);
        assert!(partial.geoip.online_enabled);
        assert_eq!("custom", partial.geoip.preset);
        assert_eq!(
            Some("https://geo.example.test/{ip}"),
            partial.geoip.url_template.as_deref()
        );
        assert!(!partial.geoip.skip_private_or_reserved);
    }
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

    #[test]
    fn serializes_v1_target_precheck_and_probe_round() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 9, 30, 0).unwrap();
        let target = TargetEndpoint {
            input: "example.com".to_string(),
            resolved: vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))],
            protocol: ProbeProtocol::Tcp,
            port: 443,
        };
        let prechecks = vec![
            ReachabilityCheck {
                checked_at,
                kind: ReachabilityKind::TcpPort,
                status: ReachabilityStatus::Reachable,
                remote_addr: Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))),
                rtt_ms: Some(18.5),
                detail: Some("tcp syn-ack received".to_string()),
            },
            ReachabilityCheck {
                checked_at,
                kind: ReachabilityKind::IcmpEcho,
                status: ReachabilityStatus::BlockedOrFiltered,
                remote_addr: Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))),
                rtt_ms: None,
                detail: Some("icmp echo timed out; tcp evidence remains primary".to_string()),
            },
        ];
        let round = ProbeRound {
            id: "discovery-1".to_string(),
            started_at: checked_at,
            flow: FlowKey {
                id: "tcp-443-flow-a".to_string(),
                protocol: ProbeProtocol::Tcp,
                source_port: Some(49152),
                destination_port: Some(443),
            },
            samples: vec![TtlProbeSample {
                sampled_at: checked_at,
                ttl: 2,
                response: ProbeResponse::TtlExceeded {
                    responder: IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1)),
                    hostname: Some("edge-router.example.net".to_string()),
                },
                rtt_ms: Some(3.2),
            }],
        };

        #[derive(Serialize)]
        struct Evidence<'a> {
            target: &'a TargetEndpoint,
            prechecks: &'a [ReachabilityCheck],
            round: &'a ProbeRound,
        }

        let json = serde_json::to_value(Evidence {
            target: &target,
            prechecks: &prechecks,
            round: &round,
        })
        .unwrap();

        assert_eq!(json["target"]["protocol"], "tcp");
        assert_eq!(json["target"]["port"], 443);
        assert_eq!(json["prechecks"][0]["kind"], "tcp_port");
        assert_eq!(json["prechecks"][1]["status"], "blocked_or_filtered");
        assert_eq!(
            json["round"]["samples"][0]["response"]["kind"],
            "ttl_exceeded"
        );
    }
}
