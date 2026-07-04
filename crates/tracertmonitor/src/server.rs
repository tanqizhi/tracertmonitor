use crate::demo::demo_session_for_target_with_resolved;
use crate::export::{export_csv_bundle, export_json};
use crate::model::TraceSession;
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

type SharedSession = Arc<Mutex<TraceSession>>;

pub struct CockpitServer {
    listener: TcpListener,
    session: SharedSession,
}

impl CockpitServer {
    pub fn bind(addr: impl ToSocketAddrs, session: TraceSession) -> Result<Self, ServerError> {
        Ok(Self {
            listener: TcpListener::bind(addr)?,
            session: Arc::new(Mutex::new(session)),
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
    packet_interval_ms: Option<u64>,
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
    response_for_path(path, &current)
}

fn start_session(body: &str, session: &SharedSession) -> Result<RouteResponse, ServerError> {
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
    let _packet_interval_ms = request
        .packet_interval_ms
        .unwrap_or(2500)
        .clamp(200, 60_000);
    let resolved = resolve_target_addresses(target);
    let updated = demo_session_for_target_with_resolved(target, resolved);
    *session.lock().expect("session lock poisoned") = updated.clone();
    Ok(text_response(
        200,
        "application/json; charset=utf-8",
        &export_json(&updated)?,
    ))
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
fn current_session(session: &SharedSession) -> TraceSession {
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
    use crate::demo::demo_session;

    #[test]
    fn route_index_returns_html() {
        let response = response_for_path("/", &demo_session()).unwrap();

        assert_eq!(200, response.status);
        assert_eq!("text/html; charset=utf-8", response.content_type);
        assert!(String::from_utf8_lossy(&response.body).contains("TracertMonitor"));
    }

    #[test]
    fn start_route_updates_current_session() {
        let shared = Arc::new(Mutex::new(demo_session()));
        let response = response_for_request(
            "POST",
            "/api/session/start",
            r#"{"target":"223.5.5.5","packet_interval_ms":1000}"#,
            &shared,
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
    fn start_route_rejects_empty_target() {
        let shared = Arc::new(Mutex::new(demo_session()));
        let response = response_for_request(
            "POST",
            "/api/session/start",
            r#"{"target":"  ","packet_interval_ms":1000}"#,
            &shared,
        )
        .unwrap();

        assert_eq!(400, response.status);
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
        assert!(index.contains("packet-frequency"));
        assert!(index.contains("topology-mode"));
        assert!(app.contains("/api/session/start"));
        assert!(app.contains("startBackendSession"));
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
        assert!(styles.contains(".frequency-control"));
        assert!(styles.contains(".topology-mode-toggle"));
        assert!(styles.contains(".merged-node"));
        assert!(styles.contains(".merged-edge"));
        assert!(styles.contains(".axis-label"));
        assert!(styles.contains(".hover-target"));
        assert!(styles.contains(".edge-latency"));
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
