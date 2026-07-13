use crate::analyzer::{aggregate_window, stable_path_id};
use crate::export::{
    export_csv_bundle, export_diagnostic_csv_bundle, export_diagnostic_json, export_json,
};
use crate::model::{
    DiagnosticConfig, DiagnosticEvent, DiagnosticEventKind, HopClassification, HopEvidence,
    HopMetrics, HopNode, PathObservation, ProbeProtocol, Severity, Target, TargetEndpoint,
    TimeWindow, TraceSession,
};
use crate::probe::{ProbeEngine, ProbeError, SystemProbeEngine};
use crate::session::{DiagnosticPhase, DiagnosticSession, DiagnosticSnapshot};
use chrono::Utc;
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
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

type SharedSession = Arc<Mutex<DiagnosticSnapshot>>;

pub struct CockpitServer {
    listener: TcpListener,
    session: SharedSession,
}

impl CockpitServer {
    pub fn bind(addr: impl ToSocketAddrs, session: TraceSession) -> Result<Self, ServerError> {
        Ok(Self {
            listener: TcpListener::bind(addr)?,
            session: Arc::new(Mutex::new(diagnostic_snapshot_from_trace_session(session))),
        })
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

#[derive(Debug, Deserialize)]
struct StartSessionRequest {
    target: String,
    protocol: Option<ProbeProtocol>,
    port: Option<u16>,
    packet_interval_ms: Option<u64>,
    config: Option<DiagnosticConfig>,
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
        "/api/session" | "/export/session.json" => Ok(text_response(
            200,
            "application/json; charset=utf-8",
            &export_json(session)?,
        )),
        "/export/prechecks.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_csv_bundle(session)?.prechecks,
        )),
        "/export/path-summary.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_csv_bundle(session)?.path_summary,
        )),
        "/export/hop-summary.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_csv_bundle(session)?.hop_summary,
        )),
        "/export/observations.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_csv_bundle(session)?.observations,
        )),
        "/export/events.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_csv_bundle(session)?.events,
        )),
        _ => Ok(text_response(404, "text/plain; charset=utf-8", "not found")),
    }
}

fn response_for_request(
    method: &str,
    path: &str,
    body: &str,
    session: &SharedSession,
) -> Result<RouteResponse, ServerError> {
    if method == "POST" && path == "/api/session/start" {
        return start_session(body, session);
    }
    let current = current_session(session);
    response_for_snapshot_path(path, &current)
}

fn response_for_snapshot_path(
    path: &str,
    session: &DiagnosticSnapshot,
) -> Result<RouteResponse, ServerError> {
    match path {
        "/api/session" | "/export/session.json" => Ok(text_response(
            200,
            "application/json; charset=utf-8",
            &export_diagnostic_json(session)?,
        )),
        "/export/prechecks.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_diagnostic_csv_bundle(session)?.prechecks,
        )),
        "/export/path-summary.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_diagnostic_csv_bundle(session)?.path_summary,
        )),
        "/export/hop-summary.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_diagnostic_csv_bundle(session)?.hop_summary,
        )),
        "/export/observations.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_diagnostic_csv_bundle(session)?.observations,
        )),
        "/export/events.csv" => Ok(text_response(
            200,
            "text/csv; charset=utf-8",
            &export_diagnostic_csv_bundle(session)?.events,
        )),
        _ => response_for_path(path, &trace_session_from_snapshot(session)),
    }
}

fn start_session(body: &str, session: &SharedSession) -> Result<RouteResponse, ServerError> {
    let mut engine = SystemProbeEngine::new();
    start_session_with_engine(body, session, &mut engine)
}

fn start_session_with_engine<E>(
    body: &str,
    session: &SharedSession,
    engine: &mut E,
) -> Result<RouteResponse, ServerError>
where
    E: ProbeEngine,
{
    let request = match serde_json::from_str::<StartSessionRequest>(body) {
        Ok(request) => request,
        Err(_) => {
            return Ok(text_response(
                400,
                "text/plain; charset=utf-8",
                "invalid session request",
            ));
        }
    };
    let target = request.target.trim();
    if target.is_empty() {
        return Ok(text_response(
            400,
            "text/plain; charset=utf-8",
            "target is required",
        ));
    }
    let protocol = request.protocol.unwrap_or(ProbeProtocol::Tcp);
    let port = request.port.unwrap_or(443);
    if port == 0 {
        return Ok(text_response(
            400,
            "text/plain; charset=utf-8",
            "port must be between 1 and 65535",
        ));
    }
    let config = diagnostic_config_from_request(&request);
    let target = TargetEndpoint {
        input: target.to_string(),
        resolved: resolve_target_addresses(target),
        protocol,
        port,
    };
    let now = Utc::now();
    let mut diagnostic = DiagnosticSession::with_config(target, config.clone());
    let _ = diagnostic.start_with_probe(
        engine,
        config.discovery,
        TimeWindow {
            start: now - chrono::Duration::days(3650),
            end: now
                + chrono::Duration::days(3650)
                + chrono::Duration::milliseconds(config.timing.packet_interval_ms as i64),
        },
    );
    let updated = diagnostic.snapshot();
    *session.lock().expect("session lock poisoned") = updated.clone();
    Ok(text_response(
        200,
        "application/json; charset=utf-8",
        &serde_json::to_string_pretty(&updated)?,
    ))
}

#[cfg(test)]
fn start_session_with_probe<F>(
    body: &str,
    session: &SharedSession,
    probe: F,
) -> Result<RouteResponse, ServerError>
where
    F: Fn(&str, &[IpAddr]) -> Result<TraceSession, ProbeError>,
{
    let request = match serde_json::from_str::<StartSessionRequest>(body) {
        Ok(request) => request,
        Err(_) => {
            return Ok(text_response(
                400,
                "text/plain; charset=utf-8",
                "invalid session request",
            ));
        }
    };
    let target = request.target.trim();
    if target.is_empty() {
        return Ok(text_response(
            400,
            "text/plain; charset=utf-8",
            "target is required",
        ));
    }
    let resolved = resolve_target_addresses(target);
    let updated = probe(target, &resolved)
        .unwrap_or_else(|error| probe_failure_session(target, resolved, error));
    let updated = diagnostic_snapshot_from_trace_session(updated);
    *session.lock().expect("session lock poisoned") = updated.clone();
    Ok(text_response(
        200,
        "application/json; charset=utf-8",
        &serde_json::to_string_pretty(&updated)?,
    ))
}

fn diagnostic_config_from_request(request: &StartSessionRequest) -> DiagnosticConfig {
    let mut config = request.config.clone().unwrap_or_default();
    if let Some(packet_interval_ms) = request.packet_interval_ms {
        config.timing.packet_interval_ms = packet_interval_ms;
    }
    config.discovery.max_ttl = config.discovery.max_ttl.clamp(1, 64);
    config.discovery.rounds = config.discovery.rounds.clamp(1, 20);
    config.discovery.probes_per_ttl = config.discovery.probes_per_ttl.clamp(1, 10);
    config.timing.packet_interval_ms = config.timing.packet_interval_ms.clamp(200, 60_000);
    config.timing.window_seconds = config.timing.window_seconds.clamp(1, 86_400);
    config.geoip.timeout_ms = config.geoip.timeout_ms.clamp(100, 30_000);
    config.geoip.cache_ttl_seconds = config.geoip.cache_ttl_seconds.max(1);
    config
}
fn probe_failure_session(
    target_input: &str,
    resolved: Vec<IpAddr>,
    error: ProbeError,
) -> TraceSession {
    let observed_at = Utc::now();
    let hops = vec![HopEvidence {
        ttl: 1,
        node: HopNode::Unknown,
        metrics: HopMetrics {
            sent: 1,
            recv: 0,
            loss_pct: 100.0,
            last_ms: None,
            avg_ms: 0.0,
            best_ms: None,
            worst_ms: None,
            stddev_ms: 0.0,
            jitter_ms: None,
        },
        classification: HopClassification::Informational,
    }];
    let path_id = stable_path_id(&hops);
    let observations = vec![PathObservation {
        observed_at,
        path_id,
        hops,
        rtt_ms: None,
        lost: true,
        jitter_ms: None,
    }];
    let window = TimeWindow {
        start: observed_at,
        end: observed_at,
    };
    let snapshot = aggregate_window(&observations, window);
    TraceSession {
        target: Target {
            input: target_input.trim().to_string(),
            resolved,
        },
        started_at: observed_at,
        ended_at: Some(observed_at),
        paths: snapshot.paths,
        observations,
        events: vec![DiagnosticEvent {
            at: observed_at,
            severity: Severity::Warning,
            kind: DiagnosticEventKind::EvidenceInsufficient,
            message: format!("probe failed: {error}"),
        }],
    }
}
fn resolve_target_addresses(target: &str) -> Vec<IpAddr> {
    let target = target.trim();
    if let Ok(addr) = target.parse::<IpAddr>() {
        return vec![addr];
    }
    match (target, 0).to_socket_addrs() {
        Ok(addrs) => {
            let mut resolved = Vec::new();
            for addr in addrs {
                if !resolved.contains(&addr.ip()) {
                    resolved.push(addr.ip());
                }
            }
            resolved
        }
        Err(_) => Vec::new(),
    }
}
fn diagnostic_snapshot_from_trace_session(session: TraceSession) -> DiagnosticSnapshot {
    DiagnosticSnapshot {
        phase: if session.paths.is_empty() {
            DiagnosticPhase::Idle
        } else {
            DiagnosticPhase::Monitoring
        },
        target: TargetEndpoint {
            input: session.target.input,
            resolved: session.target.resolved,
            protocol: ProbeProtocol::Tcp,
            port: 443,
        },
        config: DiagnosticConfig::default(),
        started_at: session.started_at,
        ended_at: session.ended_at,
        prechecks: Vec::new(),
        paths: session.paths,
        observations: session.observations,
        events: session.events,
    }
}

fn trace_session_from_snapshot(session: &DiagnosticSnapshot) -> TraceSession {
    TraceSession {
        target: Target {
            input: session.target.input.clone(),
            resolved: session.target.resolved.clone(),
        },
        started_at: session.started_at,
        ended_at: session.ended_at,
        paths: session.paths.clone(),
        observations: session.observations.clone(),
        events: session.events.clone(),
    }
}

fn current_session(session: &SharedSession) -> DiagnosticSnapshot {
    session.lock().expect("session lock poisoned").clone()
}
fn handle_stream(mut stream: TcpStream, session: &SharedSession) -> Result<(), ServerError> {
    let mut buffer = [0_u8; 16_384];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request.lines().next().unwrap_or("GET / HTTP/1.1");
    let method = request_line.split_whitespace().next().unwrap_or("GET");
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
    let response = response_for_request(method, path, body, session)?;
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
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
    RouteResponse {
        status,
        content_type,
        body: body.as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::{demo_session, demo_session_for_target_with_resolved};
    use crate::model::{
        ProbeProtocol, ReachabilityCheck, ReachabilityKind, ReachabilityStatus, TargetEndpoint,
    };
    use crate::probe::{DiscoveryConfig, ProbeEngine};
    use crate::session::{DiagnosticPhase, DiagnosticSnapshot};
    use chrono::{TimeZone, Utc};
    use std::net::Ipv4Addr;

    struct ServerProbeEngine {
        prechecks: Vec<ReachabilityCheck>,
        observations: Vec<PathObservation>,
        seen_targets: Vec<TargetEndpoint>,
        seen_configs: Vec<DiscoveryConfig>,
        discovery_calls: usize,
    }

    impl ServerProbeEngine {
        fn new() -> Self {
            let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 15, 0, 0).unwrap();
            Self {
                prechecks: vec![ReachabilityCheck {
                    checked_at,
                    kind: ReachabilityKind::TcpPort,
                    status: ReachabilityStatus::Reachable,
                    remote_addr: Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))),
                    rtt_ms: Some(16.0),
                    detail: Some("tcp syn-ack received".to_string()),
                }],
                observations: vec![server_observation(
                    checked_at + chrono::Duration::seconds(1),
                )],
                seen_targets: Vec::new(),
                seen_configs: Vec::new(),
                discovery_calls: 0,
            }
        }
    }

    impl ProbeEngine for ServerProbeEngine {
        fn precheck(
            &mut self,
            target: &TargetEndpoint,
        ) -> Result<Vec<ReachabilityCheck>, ProbeError> {
            self.seen_targets.push(target.clone());
            Ok(self.prechecks.clone())
        }

        fn discover_paths(
            &mut self,
            _target: &TargetEndpoint,
            config: DiscoveryConfig,
        ) -> Result<Vec<PathObservation>, ProbeError> {
            self.seen_configs.push(config);
            self.discovery_calls += 1;
            Ok(self.observations.clone())
        }

        fn monitor_once(
            &mut self,
            _target: &TargetEndpoint,
            _known_paths: &[crate::model::PathEvidence],
        ) -> Result<Vec<PathObservation>, ProbeError> {
            Ok(Vec::new())
        }
    }

    fn demo_snapshot() -> DiagnosticSnapshot {
        let session = demo_session();
        DiagnosticSnapshot {
            phase: DiagnosticPhase::Monitoring,
            target: TargetEndpoint {
                input: session.target.input,
                resolved: session.target.resolved,
                protocol: ProbeProtocol::Tcp,
                port: 443,
            },
            config: DiagnosticConfig::default(),
            started_at: session.started_at,
            ended_at: session.ended_at,
            prechecks: Vec::new(),
            paths: session.paths,
            observations: session.observations,
            events: session.events,
        }
    }

    fn server_observation(observed_at: chrono::DateTime<Utc>) -> PathObservation {
        let hops = vec![HopEvidence {
            ttl: 1,
            node: HopNode::Known {
                ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)),
                hostname: None,
                geoip: None,
            },
            metrics: HopMetrics::default(),
            classification: HopClassification::Normal,
        }];
        PathObservation {
            observed_at,
            path_id: stable_path_id(&hops),
            hops,
            rtt_ms: Some(20.0),
            lost: false,
            jitter_ms: Some(1.0),
        }
    }

    #[test]
    fn route_index_returns_html() {
        let response = response_for_path("/", &demo_session()).unwrap();

        assert_eq!(200, response.status);
        assert_eq!("text/html; charset=utf-8", response.content_type);
        assert!(String::from_utf8_lossy(&response.body).contains("TracertMonitor"));
    }

    #[test]
    fn start_route_updates_current_session() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let response = start_session_with_probe(
            r#"{"target":"223.5.5.5","packet_interval_ms":1000}"#,
            &shared,
            |target, resolved| {
                Ok(demo_session_for_target_with_resolved(
                    target,
                    resolved.to_vec(),
                ))
            },
        )
        .unwrap();

        assert_eq!(200, response.status);
        assert!(String::from_utf8_lossy(&response.body).contains("223.5.5.5"));

        let current = response_for_request("GET", "/api/session", "", &shared).unwrap();
        let current = String::from_utf8_lossy(&current.body);
        assert!(current.contains("223.5.5.5"));
        assert!(!current.contains("example.com"));
    }

    #[test]
    fn start_route_accepts_target_protocol_and_tcp_port() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let mut engine = ServerProbeEngine::new();

        let response = start_session_with_engine(
            r#"{"target":"www.ctyun.cn","protocol":"tcp","port":443,"packet_interval_ms":1000}"#,
            &shared,
            &mut engine,
        )
        .unwrap();

        assert_eq!(200, response.status);
        assert_eq!(1, engine.discovery_calls);
        let seen_target = engine.seen_targets.first().unwrap();
        assert_eq!("www.ctyun.cn", seen_target.input);
        assert_eq!(ProbeProtocol::Tcp, seen_target.protocol);
        assert_eq!(443, seen_target.port);

        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!("monitoring", body["phase"]);
        assert_eq!("www.ctyun.cn", body["target"]["input"]);
        assert_eq!("tcp", body["target"]["protocol"]);
        assert_eq!(443, body["target"]["port"]);
        assert_eq!("tcp_port", body["prechecks"][0]["kind"]);
    }

    #[test]
    fn start_route_rejects_zero_port() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let mut engine = ServerProbeEngine::new();

        let response = start_session_with_engine(
            r#"{"target":"www.ctyun.cn","protocol":"tcp","port":0}"#,
            &shared,
            &mut engine,
        )
        .unwrap();

        assert_eq!(400, response.status);
        assert!(engine.seen_targets.is_empty());
        assert_eq!(0, engine.discovery_calls);
    }

    #[test]
    fn start_route_defaults_port_to_443() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let mut engine = ServerProbeEngine::new();

        let response = start_session_with_engine(
            r#"{"target":"www.ctyun.cn","protocol":"tcp"}"#,
            &shared,
            &mut engine,
        )
        .unwrap();

        assert_eq!(200, response.status);
        let seen_target = engine.seen_targets.first().unwrap();
        assert_eq!(443, seen_target.port);

        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(443, body["target"]["port"]);
    }
    #[test]
    fn start_route_accepts_diagnostic_config() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let mut engine = ServerProbeEngine::new();

        let response = start_session_with_engine(
            r#"{
                "target":"www.ctyun.cn",
                "protocol":"tcp",
                "port":443,
                "config":{
                    "discovery":{"max_ttl":24,"rounds":4,"probes_per_ttl":2},
                    "timing":{"packet_interval_ms":750,"window_seconds":120},
                    "geoip":{
                        "online_enabled":true,
                        "preset":"custom",
                        "url_template":"https://geo.example.test/{ip}",
                        "local_db_path":"C:\\geo.mmdb",
                        "timeout_ms":900,
                        "cache_ttl_seconds":3600,
                        "skip_private_or_reserved":false
                    }
                }
            }"#,
            &shared,
            &mut engine,
        )
        .unwrap();

        assert_eq!(200, response.status);
        assert_eq!(1, engine.discovery_calls);
        assert_eq!(24, engine.seen_configs[0].max_ttl);
        assert_eq!(4, engine.seen_configs[0].rounds);
        assert_eq!(2, engine.seen_configs[0].probes_per_ttl);

        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(24, body["config"]["discovery"]["max_ttl"]);
        assert_eq!(750, body["config"]["timing"]["packet_interval_ms"]);
        assert_eq!(120, body["config"]["timing"]["window_seconds"]);
        assert_eq!(true, body["config"]["geoip"]["online_enabled"]);
        assert_eq!("custom", body["config"]["geoip"]["preset"]);
        assert_eq!(
            "https://geo.example.test/{ip}",
            body["config"]["geoip"]["url_template"]
        );
        assert_eq!("C:\\geo.mmdb", body["config"]["geoip"]["local_db_path"]);
        assert_eq!(900, body["config"]["geoip"]["timeout_ms"]);
        assert_eq!(3600, body["config"]["geoip"]["cache_ttl_seconds"]);
        assert_eq!(false, body["config"]["geoip"]["skip_private_or_reserved"]);
    }
    #[test]
    fn start_route_rejects_empty_target() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let response = start_session_with_probe(
            r#"{"target":"  ","packet_interval_ms":1000}"#,
            &shared,
            |target, resolved| {
                Ok(demo_session_for_target_with_resolved(
                    target,
                    resolved.to_vec(),
                ))
            },
        )
        .unwrap();

        assert_eq!(400, response.status);
    }

    #[test]
    fn start_route_probe_failure_does_not_return_demo_paths() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let response = start_session_with_probe(
            r#"{"target":"qq.com","packet_interval_ms":1000}"#,
            &shared,
            |_target, _resolved| Err(ProbeError::NoHops),
        )
        .unwrap();

        assert_eq!(200, response.status);
        let body = String::from_utf8_lossy(&response.body);
        assert!(body.contains("qq.com"));
        assert!(body.contains("probe failed"));
        assert!(!body.contains("10.0.0.1"));
        assert!(!body.contains("203.0.113.10"));
    }

    #[test]
    fn resolver_preserves_literal_ip_targets() {
        assert_eq!(
            vec!["223.5.5.5".parse::<IpAddr>().unwrap()],
            resolve_target_addresses("223.5.5.5")
        );
    }

    #[test]
    fn cockpit_assets_expose_realtime_monitoring_layout() {
        let session = demo_session();
        let index_response = response_for_path("/", &session).unwrap();
        let app_response = response_for_path("/assets/app.js", &session).unwrap();
        let styles_response = response_for_path("/assets/styles.css", &session).unwrap();
        let index = String::from_utf8_lossy(&index_response.body);
        let app = String::from_utf8_lossy(&app_response.body);
        let styles = String::from_utf8_lossy(&styles_response.body);

        assert!(index.contains("路径实时监控"));
        assert!(index.contains("target-input"));
        assert!(index.contains("protocol-select"));
        assert!(index.contains("port-input"));
        assert!(index.contains("session-phase"));
        assert!(index.contains("precheck-results"));
        assert!(index.contains("packet-frequency"));
        assert!(index.contains("diagnostic-config"));
        assert!(index.contains("config-max-ttl"));
        assert!(index.contains("config-rounds"));
        assert!(index.contains("config-probes-per-ttl"));
        assert!(index.contains("config-window-seconds"));
        assert!(index.contains("geoip-online-enabled"));
        assert!(index.contains("geoip-url-template"));
        assert!(index.contains("geoip-skip-private"));
        assert!(index.contains("topology-mode"));
        assert!(app.contains("/api/session/start"));
        assert!(app.contains("startBackendSession"));
        assert!(app.contains("readDiagnosticConfigControls"));
        assert!(app.contains("normalizeDiagnosticConfig"));
        assert!(app.contains("config: state.config"));
        assert!(app.contains("protocol"));
        assert!(app.contains("port"));
        assert!(app.contains("renderSessionPhase"));
        assert!(app.contains("renderPrechecks"));
        assert!(app.contains("geoIpSummary"));
        assert!(app.contains("pathGeoIpDetails"));
        assert!(index.contains("按路径"));
        assert!(index.contains("汇聚拓扑"));
        assert!(app.contains("renderPathChart"));
        assert!(app.contains("setInterval"));
        assert!(app.contains("topologyMode"));
        assert!(app.contains("drawMergedTopology"));
        assert!(app.contains("buildMergedTopology"));
        assert!(app.contains("setTopologyMode"));
        assert!(
            !js_function_body(&app, "compareMergedNodes", "topologyNodeKey")
                .contains("selectedPathId")
        );
        assert!(app.contains("TARGET_PRESETS"));
        assert!(app.contains("baidu.com"));
        assert!(app.contains("223.5.5.5"));
        assert!(app.contains("www.ctyun.cn"));
        assert!(app.contains("hn.189.cn"));
        assert!(app.contains("applyTargetPreset"));
        assert!(app.contains("restartLiveLoop"));
        assert!(app.contains("chart-axis"));
        assert!(app.contains("axis-label"));
        assert!(app.contains("chart-tooltip"));
        assert!(app.contains("hover-target"));
        assert!(app.contains("showChartTooltip"));
        assert!(app.contains("edgeLatencyLabel"));
        assert!(app.contains("edge-latency"));
        assert!(styles.contains(".chart-tooltip"));
        assert!(styles.contains(".monitor-controls"));
        assert!(styles.contains(".preset-sites"));
        assert!(styles.contains(".diagnostic-config"));
        assert!(styles.contains(".config-grid"));
        assert!(styles.contains(".frequency-control"));
        assert!(styles.contains(".topology-mode-toggle"));
        assert!(styles.contains(".merged-node"));
        assert!(styles.contains(".merged-edge"));
        assert!(styles.contains(".axis-label"));
        assert!(styles.contains(".hover-target"));
        assert!(styles.contains(".edge-latency"));
        assert!(styles.contains(".geoip-line"));
        assert!(css_rule(&styles, "body").contains("overflow-y: auto"));
        assert!(!css_rule(&styles, "body").contains("overflow: hidden"));
        assert!(css_rule(&styles, ".cockpit").contains("height: auto"));
        assert!(css_rule(&styles, ".cockpit").contains("min-height: 100vh"));
        assert!(!css_rule(&styles, ".cockpit").contains("min-height: 0"));
        assert!(css_rule(&styles, ".timeline-panel").contains("overflow: visible"));
        assert!(css_rule(&styles, ".lanes").contains("overflow: visible"));
    }

    fn css_rule<'a>(styles: &'a str, selector: &str) -> &'a str {
        let start = styles.find(&format!("{selector} {{")).unwrap();
        let body = &styles[start..];
        let end = body.find('}').unwrap();
        &body[..end]
    }

    fn js_function_body<'a>(source: &'a str, name: &str, next_name: &str) -> &'a str {
        let start = source.find(&format!("function {name}")).unwrap();
        let body = &source[start..];
        let end = body.find(&format!("function {next_name}")).unwrap();
        &body[..end]
    }

    #[test]
    fn export_routes_include_current_snapshot_prechecks() {
        let shared = Arc::new(Mutex::new(demo_snapshot()));
        let mut engine = ServerProbeEngine::new();
        start_session_with_engine(
            r#"{"target":"www.ctyun.cn","protocol":"tcp","port":443}"#,
            &shared,
            &mut engine,
        )
        .unwrap();

        let response = response_for_request("GET", "/export/prechecks.csv", "", &shared).unwrap();
        let body = String::from_utf8_lossy(&response.body);

        assert_eq!(200, response.status);
        assert!(body.contains("Kind,Status,RemoteAddr,RttMs,Detail"));
        assert!(body.contains("tcp_port,reachable,203.0.113.10,16,tcp syn-ack received"));

        let json = response_for_request("GET", "/export/session.json", "", &shared).unwrap();
        let json = String::from_utf8_lossy(&json.body);
        assert!(json.contains("\"prechecks\""));
        assert!(json.contains("\"port\": 443"));
    }
    #[test]
    fn route_exports_complete_csv_tables() {
        let session = demo_session();

        assert!(
            String::from_utf8_lossy(
                &response_for_path("/export/hop-summary.csv", &session)
                    .unwrap()
                    .body
            )
            .contains("NodeKind")
        );
        assert!(
            String::from_utf8_lossy(
                &response_for_path("/export/observations.csv", &session)
                    .unwrap()
                    .body
            )
            .contains("ObservedAt")
        );
    }
}
