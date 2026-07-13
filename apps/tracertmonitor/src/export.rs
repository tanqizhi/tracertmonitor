use crate::model::{
    DiagnosticEvent, GeoIpLookupStatus, HopNode, PathEvidence, PathObservation, ReachabilityCheck,
    ReachabilityKind, ReachabilityStatus, TraceSession,
};
use crate::session::DiagnosticSnapshot;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct CsvBundle {
    pub prechecks: String,
    pub path_summary: String,
    pub hop_summary: String,
    pub observations: String,
    pub events: String,
}
pub fn export_json(session: &TraceSession) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(session)
}

pub fn export_diagnostic_json(session: &DiagnosticSnapshot) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(session)
}

pub fn export_csv_bundle(session: &TraceSession) -> Result<CsvBundle, csv::Error> {
    Ok(CsvBundle {
        prechecks: export_prechecks(&[])?,
        path_summary: export_path_summary(&session.paths)?,
        hop_summary: export_hop_summary(&session.paths)?,
        observations: export_observations(&session.observations)?,
        events: export_events(&session.events)?,
    })
}

pub fn export_diagnostic_csv_bundle(session: &DiagnosticSnapshot) -> Result<CsvBundle, csv::Error> {
    Ok(CsvBundle {
        prechecks: export_prechecks(&session.prechecks)?,
        path_summary: export_path_summary(&session.paths)?,
        hop_summary: export_hop_summary(&session.paths)?,
        observations: export_observations(&session.observations)?,
        events: export_events(&session.events)?,
    })
}
fn finish_csv(writer: csv::Writer<Vec<u8>>) -> Result<String, csv::Error> {
    let data = writer
        .into_inner()
        .map_err(|err| csv::Error::from(err.into_error()))?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

fn export_prechecks(prechecks: &[ReachabilityCheck]) -> Result<String, csv::Error> {
    #[derive(Serialize)]
    struct Row {
        #[serde(rename = "Kind")]
        kind: &'static str,
        #[serde(rename = "Status")]
        status: &'static str,
        #[serde(rename = "RemoteAddr")]
        remote_addr: String,
        #[serde(rename = "RttMs")]
        rtt_ms: String,
        #[serde(rename = "Detail")]
        detail: String,
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    for check in prechecks {
        writer.serialize(Row {
            kind: reachability_kind_name(check.kind),
            status: reachability_status_name(check.status),
            remote_addr: check
                .remote_addr
                .map_or_else(String::new, |addr| addr.to_string()),
            rtt_ms: check
                .rtt_ms
                .map_or_else(String::new, |value| value.to_string()),
            detail: check.detail.clone().unwrap_or_default(),
        })?;
    }
    finish_csv(writer)
}

fn reachability_kind_name(kind: ReachabilityKind) -> &'static str {
    match kind {
        ReachabilityKind::Dns => "dns",
        ReachabilityKind::TcpPort => "tcp_port",
        ReachabilityKind::IcmpEcho => "icmp_echo",
    }
}

fn reachability_status_name(status: ReachabilityStatus) -> &'static str {
    match status {
        ReachabilityStatus::Reachable => "reachable",
        ReachabilityStatus::Unreachable => "unreachable",
        ReachabilityStatus::BlockedOrFiltered => "blocked_or_filtered",
        ReachabilityStatus::Unchecked => "unchecked",
    }
}

fn export_path_summary(paths: &[PathEvidence]) -> Result<String, csv::Error> {
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
    for path in paths {
        writer.serialize(Row {
            path_id: path.id.0.clone(),
            label: path.label.clone(),
            hit_count: path.metrics.hit_count,
            window_share_pct: path.metrics.window_share_pct,
            loss_pct: path.metrics.loss_pct,
            avg_rtt_ms: path.metrics.avg_rtt_ms,
            avg_jitter_ms: path.metrics.avg_jitter_ms,
            suspicion: path
                .suspicion
                .as_ref()
                .map_or_else(String::new, |suspicion| suspicion.reason.clone()),
        })?;
    }
    finish_csv(writer)
}

fn export_hop_summary(paths: &[PathEvidence]) -> Result<String, csv::Error> {
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
        #[serde(rename = "GeoCountryOrRegion")]
        geo_country_or_region: String,
        #[serde(rename = "GeoProvince")]
        geo_province: String,
        #[serde(rename = "GeoCity")]
        geo_city: String,
        #[serde(rename = "GeoCarrierOrAsn")]
        geo_carrier_or_asn: String,
        #[serde(rename = "GeoSource")]
        geo_source: String,
        #[serde(rename = "GeoStatus")]
        geo_status: String,
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    for path in paths {
        for hop in &path.hops {
            let (node_kind, address, geoip) = match &hop.node {
                HopNode::Known {
                    ip,
                    hostname,
                    geoip,
                } => (
                    "known".to_string(),
                    hostname.clone().unwrap_or_else(|| ip.to_string()),
                    geoip.as_ref(),
                ),
                HopNode::Unknown => ("unknown".to_string(), "*".to_string(), None),
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
                jitter_ms: hop
                    .metrics
                    .jitter_ms
                    .map_or_else(String::new, |value| value.to_string()),
                geo_country_or_region: geoip
                    .and_then(|info| info.country_or_region.clone())
                    .unwrap_or_default(),
                geo_province: geoip
                    .and_then(|info| info.province.clone())
                    .unwrap_or_default(),
                geo_city: geoip.and_then(|info| info.city.clone()).unwrap_or_default(),
                geo_carrier_or_asn: geoip
                    .and_then(|info| info.carrier_or_asn.clone())
                    .unwrap_or_default(),
                geo_source: geoip
                    .and_then(|info| info.source.clone())
                    .unwrap_or_default(),
                geo_status: geoip
                    .map(|info| geoip_status_name(info.status).to_string())
                    .unwrap_or_default(),
            })?;
        }
    }
    finish_csv(writer)
}

fn geoip_status_name(status: GeoIpLookupStatus) -> &'static str {
    match status {
        GeoIpLookupStatus::OnlineHit => "online_hit",
        GeoIpLookupStatus::OnlineFailedLocalHit => "online_failed_local_hit",
        GeoIpLookupStatus::LocalHit => "local_hit",
        GeoIpLookupStatus::Unknown => "unknown",
        GeoIpLookupStatus::SkippedPrivateOrReserved => "skipped_private_or_reserved",
    }
}

fn export_observations(observations: &[PathObservation]) -> Result<String, csv::Error> {
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
    for observation in observations {
        writer.serialize(Row {
            observed_at: observation.observed_at.to_rfc3339(),
            path_id: observation.path_id.0.clone(),
            rtt_ms: observation
                .rtt_ms
                .map_or_else(String::new, |value| value.to_string()),
            lost: observation.lost,
            jitter_ms: observation
                .jitter_ms
                .map_or_else(String::new, |value| value.to_string()),
            hop_count: observation.hops.len(),
        })?;
    }
    finish_csv(writer)
}

fn export_events(events: &[DiagnosticEvent]) -> Result<String, csv::Error> {
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
    for event in events {
        writer.serialize(Row {
            at: event.at.to_rfc3339(),
            severity: format!("{:?}", event.severity),
            kind: format!("{:?}", event.kind),
            message: event.message.clone(),
        })?;
    }
    finish_csv(writer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::demo_session;
    use crate::model::{
        DiagnosticConfig, DiagnosticEvent, DiagnosticEventKind, GeoIpInfo, GeoIpLookupStatus,
        HopClassification, HopEvidence, HopMetrics, HopNode, PathEvidence, PathId, PathMetrics,
        PathObservation, ProbeProtocol, ReachabilityCheck, ReachabilityKind, ReachabilityStatus,
        Severity, TargetEndpoint,
    };
    use crate::session::{DiagnosticPhase, DiagnosticSnapshot};
    use chrono::{TimeZone, Utc};
    use std::net::{IpAddr, Ipv4Addr};

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

        assert!(csv.path_summary.contains(
            "PathId,Label,HitCount,WindowSharePct,LossPct,AvgRttMs,AvgJitterMs,Suspicion"
        ));
        assert!(
            csv.hop_summary.contains(
                "PathId,Label,Ttl,NodeKind,Address,Classification,LossPct,AvgMs,JitterMs"
            )
        );
        assert!(
            csv.observations
                .contains("ObservedAt,PathId,RttMs,Lost,JitterMs,HopCount")
        );
        assert!(csv.events.contains("At,Severity,Kind,Message"));
    }

    #[test]
    fn json_export_contains_v1_diagnostic_flow_evidence() {
        let json = export_diagnostic_json(&v1_snapshot()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(json.contains("\"port\": 443"));
        assert!(json.contains("tcp_port"));
        assert_eq!("www.ctyun.cn", value["target"]["input"]);
        assert_eq!("tcp", value["target"]["protocol"]);
        assert_eq!(443, value["target"]["port"]);
        assert_eq!("203.0.113.10", value["target"]["resolved"][0]);
        assert_eq!("dns", value["prechecks"][0]["kind"]);
        assert_eq!("tcp_port", value["prechecks"][1]["kind"]);
        assert_eq!("monitoring", value["phase"]);
        assert_eq!(1, value["observations"].as_array().unwrap().len());
        assert_eq!(1, value["paths"].as_array().unwrap().len());
        assert_eq!(1, value["events"].as_array().unwrap().len());
    }

    #[test]
    fn csv_bundle_contains_prechecks_table() {
        let csv = export_diagnostic_csv_bundle(&v1_snapshot()).unwrap();

        assert!(
            csv.prechecks
                .contains("Kind,Status,RemoteAddr,RttMs,Detail")
        );
        assert!(
            csv.prechecks
                .contains("tcp_port,reachable,203.0.113.10,18.5,tcp syn-ack received")
        );
        assert!(csv.path_summary.contains("PathId,Label"));
        assert!(csv.hop_summary.contains("PathId,Label,Ttl"));
        assert!(csv.observations.contains("ObservedAt,PathId"));
        assert!(csv.events.contains("At,Severity,Kind,Message"));
    }

    #[test]
    fn csv_hop_summary_includes_geoip_evidence() {
        let csv = export_diagnostic_csv_bundle(&v1_snapshot()).unwrap();

        assert!(csv.hop_summary.contains(
            "PathId,Label,Ttl,NodeKind,Address,Classification,LossPct,AvgMs,JitterMs,GeoCountryOrRegion,GeoProvince,GeoCity,GeoCarrierOrAsn,GeoSource,GeoStatus"
        ));
        assert!(csv.hop_summary.contains(
            "path-a,Path A,1,known,edge-router.example.net,Normal,0.0,0.0,,中国,广东省,广州市,中国电信 AS4134,local-fixture,local_hit"
        ));
    }

    fn v1_snapshot() -> DiagnosticSnapshot {
        let started_at = Utc.with_ymd_and_hms(2026, 7, 5, 16, 0, 0).unwrap();
        let target_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));
        let router_ip = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1));
        let hops = vec![
            HopEvidence {
                ttl: 1,
                node: HopNode::Known {
                    ip: router_ip,
                    hostname: Some("edge-router.example.net".to_string()),
                    geoip: Some(GeoIpInfo {
                        country_or_region: Some("中国".to_string()),
                        province: Some("广东省".to_string()),
                        city: Some("广州市".to_string()),
                        carrier_or_asn: Some("中国电信 AS4134".to_string()),
                        source: Some("local-fixture".to_string()),
                        status: GeoIpLookupStatus::LocalHit,
                    }),
                },
                metrics: HopMetrics::default(),
                classification: HopClassification::Normal,
            },
            HopEvidence {
                ttl: 2,
                node: HopNode::Known {
                    ip: target_ip,
                    hostname: None,
                    geoip: None,
                },
                metrics: HopMetrics::default(),
                classification: HopClassification::Normal,
            },
        ];

        DiagnosticSnapshot {
            phase: DiagnosticPhase::Monitoring,
            target: TargetEndpoint {
                input: "www.ctyun.cn".to_string(),
                resolved: vec![target_ip],
                protocol: ProbeProtocol::Tcp,
                port: 443,
            },
            config: DiagnosticConfig::default(),
            started_at,
            ended_at: None,
            prechecks: vec![
                ReachabilityCheck {
                    checked_at: started_at,
                    kind: ReachabilityKind::Dns,
                    status: ReachabilityStatus::Reachable,
                    remote_addr: Some(target_ip),
                    rtt_ms: Some(1.2),
                    detail: Some("resolved target".to_string()),
                },
                ReachabilityCheck {
                    checked_at: started_at,
                    kind: ReachabilityKind::TcpPort,
                    status: ReachabilityStatus::Reachable,
                    remote_addr: Some(target_ip),
                    rtt_ms: Some(18.5),
                    detail: Some("tcp syn-ack received".to_string()),
                },
            ],
            paths: vec![PathEvidence {
                id: PathId("path-a".to_string()),
                label: "Path A".to_string(),
                hops: hops.clone(),
                first_seen: started_at,
                last_seen: started_at,
                metrics: PathMetrics {
                    hit_count: 1,
                    sample_count: 1,
                    window_share_pct: 100.0,
                    loss_pct: 0.0,
                    avg_rtt_ms: 18.5,
                    avg_jitter_ms: 0.0,
                },
                suspicion: None,
            }],
            observations: vec![PathObservation {
                observed_at: started_at,
                path_id: PathId("path-a".to_string()),
                hops,
                rtt_ms: Some(18.5),
                lost: false,
                jitter_ms: Some(0.0),
            }],
            events: vec![DiagnosticEvent {
                at: started_at,
                severity: Severity::Info,
                kind: DiagnosticEventKind::PrecheckCompleted,
                message: "Precheck evidence recorded".to_string(),
            }],
        }
    }
}
