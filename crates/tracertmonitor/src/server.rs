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
        Ok(Self {
            listener: TcpListener::bind(addr)?,
            session,
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

fn handle_stream(mut stream: TcpStream, session: &TraceSession) -> Result<(), ServerError> {
    let mut buffer = [0_u8; 2048];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
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
    fn cockpit_assets_expose_realtime_monitoring_layout() {
        let session = demo_session();
        let index_response = response_for_path("/", &session).unwrap();
        let app_response = response_for_path("/assets/app.js", &session).unwrap();
        let styles_response = response_for_path("/assets/styles.css", &session).unwrap();
        let index = String::from_utf8_lossy(&index_response.body);
        let app = String::from_utf8_lossy(&app_response.body);
        let styles = String::from_utf8_lossy(&styles_response.body);

        assert!(index.contains("路径实时监控"));
        assert!(app.contains("renderPathChart"));
        assert!(app.contains("setInterval"));
        assert!(app.contains("chart-axis"));
        assert!(app.contains("axis-label"));
        assert!(app.contains("chart-tooltip"));
        assert!(app.contains("hover-target"));
        assert!(app.contains("showChartTooltip"));
        assert!(styles.contains(".chart-tooltip"));
        assert!(styles.contains(".axis-label"));
        assert!(styles.contains(".hover-target"));
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

    #[test]
    fn route_exports_complete_csv_tables() {
        let session = demo_session();

        assert!(String::from_utf8_lossy(
            &response_for_path("/export/hop-summary.csv", &session)
                .unwrap()
                .body
        )
        .contains("NodeKind"));
        assert!(String::from_utf8_lossy(
            &response_for_path("/export/observations.csv", &session)
                .unwrap()
                .body
        )
        .contains("ObservedAt"));
    }
}
