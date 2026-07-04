use crate::analyzer::{aggregate_window, stable_path_id};
use crate::model::{
    DiagnosticEvent, DiagnosticEventKind, HopClassification, HopEvidence, HopMetrics, HopNode,
    PathObservation, Severity, Target, TimeWindow, TraceSession,
};
use chrono::{TimeZone, Utc};
use std::net::{IpAddr, Ipv4Addr};

pub fn demo_session() -> TraceSession {
    demo_session_for_target("example.com")
}

pub fn demo_session_for_target(target_input: &str) -> TraceSession {
    let started_at = Utc.with_ymd_and_hms(2026, 7, 2, 14, 0, 0).unwrap();
    let path_a = vec![
        known(1, 192, 168, 1, 1),
        known(2, 10, 0, 0, 1),
        unknown(3),
        known(4, 203, 0, 113, 10),
    ];
    let path_b = vec![
        known(1, 192, 168, 1, 1),
        known(2, 10, 0, 0, 9),
        unknown(3),
        known(4, 203, 0, 113, 10),
    ];
    let path_c = vec![
        known(1, 192, 168, 1, 1),
        known(2, 10, 0, 0, 5),
        known(3, 198, 51, 100, 2),
        known(4, 203, 0, 113, 10),
    ];
    let id_a = stable_path_id(&path_a);
    let id_b = stable_path_id(&path_b);
    let id_c = stable_path_id(&path_c);

    let mut observations = Vec::new();
    for i in 0..30 {
        let observed_at = started_at + chrono::Duration::seconds(i * 10);
        let (path_id, hops, rtt_ms, lost, jitter_ms) = if i % 10 < 5 {
            (
                id_a.clone(),
                path_a.clone(),
                Some(42.0 + (i % 4) as f64),
                false,
                Some(3.0),
            )
        } else if i % 10 < 8 {
            (
                id_b.clone(),
                path_b.clone(),
                Some(185.0 + (i % 6) as f64),
                i % 2 == 0,
                Some(38.0),
            )
        } else {
            (
                id_c.clone(),
                path_c.clone(),
                Some(55.0 + (i % 5) as f64),
                false,
                Some(5.0),
            )
        };
        observations.push(PathObservation {
            observed_at,
            path_id,
            hops,
            rtt_ms,
            lost,
            jitter_ms,
        });
    }

    let window = TimeWindow {
        start: started_at,
        end: started_at + chrono::Duration::minutes(5),
    };
    let snapshot = aggregate_window(&observations, window);

    TraceSession {
        target: Target {
            input: normalized_target(target_input),
            resolved: demo_resolved_addresses(target_input),
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

fn normalized_target(target_input: &str) -> String {
    let trimmed = target_input.trim();
    if trimmed.is_empty() {
        "example.com".to_string()
    } else {
        trimmed.to_string()
    }
}

fn demo_resolved_addresses(target_input: &str) -> Vec<IpAddr> {
    target_input
        .trim()
        .parse::<IpAddr>()
        .map(|addr| vec![addr])
        .unwrap_or_else(|_| vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))])
}

fn known(ttl: u8, a: u8, b: u8, c: u8, d: u8) -> HopEvidence {
    HopEvidence {
        ttl,
        node: HopNode::Known {
            ip: IpAddr::V4(Ipv4Addr::new(a, b, c, d)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_session_contains_multiple_paths_unknown_hops_and_suspicion() {
        let session = demo_session();

        assert!(session.paths.len() >= 3);
        assert!(session.paths.iter().any(|path| path.suspicion.is_some()));
        assert!(
            session
                .paths
                .iter()
                .flat_map(|path| &path.hops)
                .any(|hop| matches!(hop.node, crate::model::HopNode::Unknown))
        );
    }

    #[test]
    fn demo_session_can_be_parameterized_by_target() {
        let session = demo_session_for_target("223.5.5.5");

        assert_eq!("223.5.5.5", session.target.input);
        assert_eq!(
            vec![IpAddr::V4(Ipv4Addr::new(223, 5, 5, 5))],
            session.target.resolved
        );
    }
}
