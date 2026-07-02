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
    let data = writer.into_inner().map_err(|err| csv::Error::from(err.into_error()))?;
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
            suspicion: path
                .suspicion
                .as_ref()
                .map_or_else(String::new, |suspicion| suspicion.reason.clone()),
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
                HopNode::Known { ip, hostname } => (
                    "known".to_string(),
                    hostname.clone().unwrap_or_else(|| ip.to_string()),
                ),
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
                jitter_ms: hop
                    .metrics
                    .jitter_ms
                    .map_or_else(String::new, |value| value.to_string()),
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
