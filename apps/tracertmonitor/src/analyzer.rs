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
        .filter(|observation| {
            observation.observed_at >= window.start && observation.observed_at <= window.end
        })
        .collect::<Vec<_>>();
    let total = in_window.len().max(1);
    let mut grouped: BTreeMap<String, Vec<&PathObservation>> = BTreeMap::new();

    for observation in in_window {
        grouped
            .entry(observation.path_id.0.clone())
            .or_default()
            .push(observation);
    }

    let mut paths = grouped
        .into_iter()
        .map(|(id, observations)| {
            let hit_count = observations.len();
            let lost_count = observations
                .iter()
                .filter(|observation| observation.lost)
                .count();
            let avg_rtt_ms = average(
                observations
                    .iter()
                    .filter_map(|observation| observation.rtt_ms),
            );
            let avg_jitter_ms = average(
                observations
                    .iter()
                    .filter_map(|observation| observation.jitter_ms),
            );
            let loss_pct = lost_count as f64 / hit_count.max(1) as f64 * 100.0;
            let first_seen = observations
                .iter()
                .map(|observation| observation.observed_at)
                .min()
                .unwrap();
            let latest = observations
                .iter()
                .max_by_key(|observation| observation.observed_at)
                .unwrap();
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
                label: String::new(),
                hops: latest.hops.clone(),
                first_seen,
                last_seen: latest.observed_at,
                metrics,
                suspicion: suspicion_for(metrics),
            }
        })
        .collect::<Vec<_>>();

    paths.sort_by(|left, right| {
        left.first_seen
            .cmp(&right.first_seen)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    for (idx, path) in paths.iter_mut().enumerate() {
        path.label = path_label(idx);
    }
    paths.sort_by(|left, right| {
        right
            .metrics
            .hit_count
            .cmp(&left.metrics.hit_count)
            .then_with(|| left.label.cmp(&right.label))
    });

    WindowSnapshot {
        window_start: window.start,
        window_end: window.end,
        paths,
    }
}

fn path_label(idx: usize) -> String {
    let suffix = if idx < 26 {
        ((b'A' + idx as u8) as char).to_string()
    } else {
        format!("#{}", idx + 1)
    };
    format!("Path {suffix}")
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
            evidence: vec![format!(
                "loss {:.1}% across {} samples",
                metrics.loss_pct, metrics.sample_count
            )],
        });
    }
    if metrics.loss_pct >= 5.0 {
        return Some(Suspicion {
            severity: Severity::Warning,
            reason: "path packet loss is above 5%".to_string(),
            evidence: vec![format!(
                "loss {:.1}% across {} samples",
                metrics.loss_pct, metrics.sample_count
            )],
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        HopClassification, HopEvidence, HopMetrics, HopNode, PathObservation, TimeWindow,
    };
    use chrono::{TimeZone, Utc};
    use std::net::{IpAddr, Ipv4Addr};

    fn known(ttl: u8, last_octet: u8) -> HopEvidence {
        HopEvidence {
            ttl,
            node: HopNode::Known {
                ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, last_octet)),
                hostname: None,
                geoip: None,
            },
            metrics: HopMetrics::default(),
            classification: HopClassification::Normal,
        }
    }

    fn known_ip(ttl: u8, a: u8, b: u8, c: u8, d: u8) -> HopEvidence {
        HopEvidence {
            ttl,
            node: HopNode::Known {
                ip: IpAddr::V4(Ipv4Addr::new(a, b, c, d)),
                hostname: None,
                geoip: None,
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
    fn ecmp_discovery_creates_stable_paths() {
        let start = Utc.with_ymd_and_hms(2026, 7, 5, 15, 0, 0).unwrap();
        let target = known_ip(3, 203, 0, 113, 10);
        let path_a_hops = vec![
            known_ip(1, 192, 0, 2, 1),
            known_ip(2, 198, 51, 100, 1),
            target.clone(),
        ];
        let path_b_hops = vec![
            known_ip(1, 192, 0, 2, 1),
            known_ip(2, 198, 51, 100, 2),
            target.clone(),
        ];
        let path_c_hops = vec![known_ip(1, 192, 0, 2, 1), unknown(2), target];
        let path_a = stable_path_id(&path_a_hops);
        let path_b = stable_path_id(&path_b_hops);
        let path_c = stable_path_id(&path_c_hops);
        let observations = vec![
            PathObservation {
                observed_at: start,
                path_id: path_a.clone(),
                hops: path_a_hops,
                rtt_ms: Some(20.0),
                lost: false,
                jitter_ms: Some(1.0),
            },
            PathObservation {
                observed_at: start + chrono::Duration::seconds(1),
                path_id: path_b.clone(),
                hops: path_b_hops,
                rtt_ms: Some(21.0),
                lost: false,
                jitter_ms: Some(1.2),
            },
            PathObservation {
                observed_at: start + chrono::Duration::seconds(2),
                path_id: path_c.clone(),
                hops: path_c_hops,
                rtt_ms: Some(22.0),
                lost: false,
                jitter_ms: Some(1.4),
            },
        ];

        let initial = aggregate_window(
            &observations[..2],
            TimeWindow {
                start,
                end: start + chrono::Duration::seconds(1),
            },
        );
        assert_eq!("Path A", label_for(&initial, &path_a));
        assert_eq!("Path B", label_for(&initial, &path_b));

        let refreshed = aggregate_window(
            &observations,
            TimeWindow {
                start,
                end: start + chrono::Duration::seconds(10),
            },
        );

        assert_eq!(3, refreshed.paths.len());
        assert_ne!(path_a, path_b);
        assert_ne!(path_a, path_c);
        assert_eq!("Path A", label_for(&refreshed, &path_a));
        assert_eq!("Path B", label_for(&refreshed, &path_b));
        assert_eq!("Path C", label_for(&refreshed, &path_c));
    }

    #[test]
    fn unknown_middle_hop_is_preserved_without_failure() {
        let start = Utc.with_ymd_and_hms(2026, 7, 5, 15, 30, 0).unwrap();
        let hops = vec![
            known_ip(1, 192, 0, 2, 1),
            unknown(2),
            known_ip(3, 203, 0, 113, 10),
        ];
        let path_id = stable_path_id(&hops);
        let observations = vec![
            PathObservation {
                observed_at: start,
                path_id: path_id.clone(),
                hops: hops.clone(),
                rtt_ms: Some(30.0),
                lost: false,
                jitter_ms: Some(1.0),
            },
            PathObservation {
                observed_at: start + chrono::Duration::seconds(1),
                path_id: path_id.clone(),
                hops,
                rtt_ms: Some(31.0),
                lost: false,
                jitter_ms: Some(1.1),
            },
        ];

        let snapshot = aggregate_window(
            &observations,
            TimeWindow {
                start,
                end: start + chrono::Duration::seconds(10),
            },
        );
        let path = snapshot
            .paths
            .iter()
            .find(|path| path.id == path_id)
            .unwrap();

        assert!(path.id.0.contains("ttl2:*"));
        assert!(matches!(path.hops[1].node, HopNode::Unknown));
        assert!(matches!(
            path.hops[1].classification,
            HopClassification::Informational
        ));
        assert!(path.suspicion.is_none());
    }
    #[test]
    fn aggregate_window_calculates_share_and_flags_loss() {
        let start = Utc.with_ymd_and_hms(2026, 7, 2, 14, 0, 0).unwrap();
        let path_a = stable_path_id(&[known(1, 1), known(2, 2)]);
        let path_b = stable_path_id(&[known(1, 1), known(2, 9)]);
        let observations = vec![
            PathObservation {
                observed_at: start,
                path_id: path_a.clone(),
                hops: vec![known(1, 1), known(2, 2)],
                rtt_ms: Some(40.0),
                lost: false,
                jitter_ms: Some(3.0),
            },
            PathObservation {
                observed_at: start + chrono::Duration::seconds(1),
                path_id: path_b.clone(),
                hops: vec![known(1, 1), known(2, 9)],
                rtt_ms: Some(180.0),
                lost: true,
                jitter_ms: Some(35.0),
            },
            PathObservation {
                observed_at: start + chrono::Duration::seconds(2),
                path_id: path_b.clone(),
                hops: vec![known(1, 1), known(2, 9)],
                rtt_ms: Some(190.0),
                lost: true,
                jitter_ms: Some(41.0),
            },
        ];

        let snapshot = aggregate_window(
            &observations,
            TimeWindow {
                start,
                end: start + chrono::Duration::seconds(10),
            },
        );

        let bad_path = snapshot
            .paths
            .iter()
            .find(|path| path.id == path_b)
            .unwrap();
        assert_eq!(2, bad_path.metrics.hit_count);
        assert!((bad_path.metrics.window_share_pct - 66.666).abs() < 0.01);
        assert!(bad_path.suspicion.is_some());
    }
    fn label_for<'a>(snapshot: &'a WindowSnapshot, path_id: &PathId) -> &'a str {
        snapshot
            .paths
            .iter()
            .find(|path| &path.id == path_id)
            .map(|path| path.label.as_str())
            .unwrap()
    }
}
