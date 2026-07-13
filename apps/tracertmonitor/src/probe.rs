use crate::analyzer::{aggregate_window, stable_path_id};
pub use crate::model::DiscoveryConfig;
use crate::model::{
    DiagnosticEvent, DiagnosticEventKind, HopClassification, HopEvidence, HopMetrics, HopNode,
    PathEvidence, PathObservation, ReachabilityCheck, ReachabilityKind, ReachabilityStatus,
    Severity, Target, TargetEndpoint, TimeWindow, TraceSession,
};
use chrono::Utc;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::process::Command;
use std::time::{Duration, Instant};

const DEFAULT_MAX_HOPS: u8 = 8;
const DEFAULT_TIMEOUT_MS: u64 = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("target did not resolve")]
    TargetNotResolved,
    #[error("probe command failed: {0}")]
    Command(std::io::Error),
    #[error("probe output was not valid utf-8")]
    Utf8,
    #[error("probe command exited with status {0}")]
    Status(String),
    #[error("probe output did not contain any hops")]
    NoHops,
}

pub trait ProbeEngine {
    fn precheck(&mut self, target: &TargetEndpoint) -> Result<Vec<ReachabilityCheck>, ProbeError>;

    fn discover_paths(
        &mut self,
        target: &TargetEndpoint,
        config: DiscoveryConfig,
    ) -> Result<Vec<PathObservation>, ProbeError>;

    fn monitor_once(
        &mut self,
        target: &TargetEndpoint,
        known_paths: &[PathEvidence],
    ) -> Result<Vec<PathObservation>, ProbeError>;
}

pub struct SystemProbeEngine<R = fn(IpAddr) -> Result<String, ProbeError>> {
    runner: R,
}

impl SystemProbeEngine<fn(IpAddr) -> Result<String, ProbeError>> {
    pub fn new() -> Self {
        Self {
            runner: run_system_traceroute,
        }
    }
}

impl Default for SystemProbeEngine<fn(IpAddr) -> Result<String, ProbeError>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> SystemProbeEngine<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R> ProbeEngine for SystemProbeEngine<R>
where
    R: FnMut(IpAddr) -> Result<String, ProbeError>,
{
    fn precheck(&mut self, target: &TargetEndpoint) -> Result<Vec<ReachabilityCheck>, ProbeError> {
        Ok(precheck_target_endpoint(target))
    }

    fn discover_paths(
        &mut self,
        target: &TargetEndpoint,
        config: DiscoveryConfig,
    ) -> Result<Vec<PathObservation>, ProbeError> {
        let _ = config;
        let session = probe_session_with_runner(&target.input, &target.resolved, &mut self.runner)?;
        Ok(session.observations)
    }

    fn monitor_once(
        &mut self,
        target: &TargetEndpoint,
        known_paths: &[PathEvidence],
    ) -> Result<Vec<PathObservation>, ProbeError> {
        let _ = known_paths;
        self.discover_paths(target, DiscoveryConfig::default())
    }
}

fn precheck_target_endpoint(target: &TargetEndpoint) -> Vec<ReachabilityCheck> {
    let checked_at = Utc::now();
    let mut checks = vec![ReachabilityCheck {
        checked_at,
        kind: ReachabilityKind::Dns,
        status: if target.resolved.is_empty() {
            ReachabilityStatus::Unreachable
        } else {
            ReachabilityStatus::Reachable
        },
        remote_addr: target.resolved.first().copied(),
        rtt_ms: None,
        detail: Some(if target.resolved.is_empty() {
            "target did not resolve".to_string()
        } else {
            format!("{} resolved address(es)", target.resolved.len())
        }),
    }];

    let Some(remote_addr) = target.resolved.first().copied() else {
        return checks;
    };

    checks.push(tcp_port_precheck(checked_at, remote_addr, target.port));
    checks.push(ReachabilityCheck {
        checked_at,
        kind: ReachabilityKind::IcmpEcho,
        status: ReachabilityStatus::Unchecked,
        remote_addr: Some(remote_addr),
        rtt_ms: None,
        detail: Some(
            "icmp echo is auxiliary and is not checked by the system fallback engine".to_string(),
        ),
    });
    checks
}

fn tcp_port_precheck(
    checked_at: chrono::DateTime<Utc>,
    remote_addr: IpAddr,
    port: u16,
) -> ReachabilityCheck {
    let socket = SocketAddr::new(remote_addr, port);
    let started = Instant::now();
    match TcpStream::connect_timeout(&socket, Duration::from_millis(DEFAULT_TIMEOUT_MS)) {
        Ok(_) => ReachabilityCheck {
            checked_at,
            kind: ReachabilityKind::TcpPort,
            status: ReachabilityStatus::Reachable,
            remote_addr: Some(remote_addr),
            rtt_ms: Some(started.elapsed().as_secs_f64() * 1000.0),
            detail: Some("tcp connection established".to_string()),
        },
        Err(error) => {
            let (status, detail) = tcp_error_status_and_detail(&error);
            ReachabilityCheck {
                checked_at,
                kind: ReachabilityKind::TcpPort,
                status,
                remote_addr: Some(remote_addr),
                rtt_ms: Some(started.elapsed().as_secs_f64() * 1000.0),
                detail: Some(detail),
            }
        }
    }
}

fn tcp_error_status_and_detail(error: &std::io::Error) -> (ReachabilityStatus, String) {
    match error.kind() {
        std::io::ErrorKind::ConnectionRefused => (
            ReachabilityStatus::Unreachable,
            "tcp connection refused; target responded but the port is closed".to_string(),
        ),
        std::io::ErrorKind::TimedOut => (
            ReachabilityStatus::BlockedOrFiltered,
            "tcp connection timed out".to_string(),
        ),
        _ => (
            ReachabilityStatus::BlockedOrFiltered,
            format!("tcp connection failed: {error}"),
        ),
    }
}
#[derive(Debug, Clone, PartialEq)]
struct ParsedHop {
    ttl: u8,
    samples_ms: Vec<Option<f64>>,
    addr: Option<IpAddr>,
}

pub fn probe_session_for_target(
    target_input: &str,
    resolved: &[IpAddr],
) -> Result<TraceSession, ProbeError> {
    probe_session_with_runner(target_input, resolved, run_system_traceroute)
}

fn probe_session_with_runner<F>(
    target_input: &str,
    resolved: &[IpAddr],
    mut runner: F,
) -> Result<TraceSession, ProbeError>
where
    F: FnMut(IpAddr) -> Result<String, ProbeError>,
{
    if resolved.is_empty() {
        return Err(ProbeError::TargetNotResolved);
    }
    let mut last_error = ProbeError::TargetNotResolved;
    for target_addr in resolved.iter().copied() {
        match runner(target_addr).and_then(|output| parse_windows_tracert(&output)) {
            Ok(hops) => return Ok(session_from_hops(target_input, resolved.to_vec(), hops)),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn run_system_traceroute(target_addr: IpAddr) -> Result<String, ProbeError> {
    let output = system_traceroute_command(target_addr)
        .output()
        .map_err(ProbeError::Command)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() && stdout.trim().is_empty() {
        return Err(ProbeError::Status(output.status.to_string()));
    }
    Ok(stdout)
}

#[cfg(windows)]
fn system_traceroute_command(target_addr: IpAddr) -> Command {
    let mut command = Command::new("tracert.exe");
    command
        .arg("-d")
        .arg("-h")
        .arg(DEFAULT_MAX_HOPS.to_string())
        .arg("-w")
        .arg(DEFAULT_TIMEOUT_MS.to_string())
        .arg(target_addr.to_string());
    command
}

#[cfg(not(windows))]
fn system_traceroute_command(target_addr: IpAddr) -> Command {
    let mut command = Command::new("traceroute");
    command
        .arg("-n")
        .arg("-m")
        .arg(DEFAULT_MAX_HOPS.to_string())
        .arg("-w")
        .arg((DEFAULT_TIMEOUT_MS / 1000).to_string())
        .arg(target_addr.to_string());
    command
}

fn session_from_hops(
    target_input: &str,
    resolved: Vec<IpAddr>,
    parsed: Vec<ParsedHop>,
) -> TraceSession {
    let observed_at = Utc::now();
    let hops = parsed.into_iter().map(hop_from_parsed).collect::<Vec<_>>();
    let path_id = stable_path_id(&hops);
    let rtt_ms = hops.iter().rev().find_map(|hop| {
        hop.metrics
            .last_ms
            .or(Some(hop.metrics.avg_ms))
            .filter(|value| *value > 0.0)
    });
    let lost = hops.iter().all(|hop| matches!(hop.node, HopNode::Unknown));
    let jitter_ms = average(
        hops.iter()
            .filter_map(|hop| hop.metrics.jitter_ms)
            .collect::<Vec<_>>()
            .as_slice(),
    );
    let observations = vec![PathObservation {
        observed_at,
        path_id,
        hops,
        rtt_ms,
        lost,
        jitter_ms,
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
            severity: Severity::Info,
            kind: DiagnosticEventKind::EvidenceInsufficient,
            message: "System traceroute probe completed one sampling round".to_string(),
        }],
    }
}

fn hop_from_parsed(parsed: ParsedHop) -> HopEvidence {
    let received = parsed
        .samples_ms
        .iter()
        .filter(|sample| sample.is_some())
        .filter_map(|sample| *sample)
        .collect::<Vec<_>>();
    let sent = parsed.samples_ms.len();
    let recv = received.len();
    HopEvidence {
        ttl: parsed.ttl,
        node: parsed
            .addr
            .map(|ip| HopNode::Known {
                ip,
                hostname: None,
                geoip: None,
            })
            .unwrap_or(HopNode::Unknown),
        metrics: HopMetrics {
            sent,
            recv,
            loss_pct: if sent == 0 {
                0.0
            } else {
                (sent - recv) as f64 / sent as f64 * 100.0
            },
            last_ms: received.last().copied(),
            avg_ms: average(&received).unwrap_or_default(),
            best_ms: received.iter().copied().reduce(f64::min),
            worst_ms: received.iter().copied().reduce(f64::max),
            stddev_ms: stddev(&received),
            jitter_ms: jitter(&received),
        },
        classification: if recv == 0 {
            HopClassification::Informational
        } else {
            HopClassification::Normal
        },
    }
}

fn parse_windows_tracert(output: &str) -> Result<Vec<ParsedHop>, ProbeError> {
    let hops = output
        .lines()
        .filter_map(parse_windows_tracert_line)
        .collect::<Vec<_>>();
    if hops.is_empty() {
        Err(ProbeError::NoHops)
    } else {
        Ok(hops)
    }
}

fn parse_windows_tracert_line(line: &str) -> Option<ParsedHop> {
    let mut parts = line.split_whitespace();
    let ttl = parts.next()?.parse::<u8>().ok()?;
    let tokens = parts.collect::<Vec<_>>();
    let mut samples_ms = Vec::new();
    let mut idx = 0;
    while samples_ms.len() < 3 && idx < tokens.len() {
        match tokens[idx] {
            "*" => {
                samples_ms.push(None);
                idx += 1;
            }
            "<1" => {
                if tokens.get(idx + 1).copied() == Some("ms") {
                    samples_ms.push(Some(0.5));
                    idx += 2;
                } else {
                    idx += 1;
                }
            }
            value => {
                if tokens.get(idx + 1).copied() == Some("ms") {
                    if let Ok(ms) = value.parse::<f64>() {
                        samples_ms.push(Some(ms));
                    }
                    idx += 2;
                } else {
                    idx += 1;
                }
            }
        }
    }
    let addr = tokens
        .iter()
        .rev()
        .find_map(|token| token.parse::<IpAddr>().ok());
    Some(ParsedHop {
        ttl,
        samples_ms,
        addr,
    })
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn jitter(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let deltas = values
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .collect::<Vec<_>>();
    average(&deltas)
}

fn stddev(values: &[f64]) -> f64 {
    let Some(avg) = average(values) else {
        return 0.0;
    };
    if values.len() < 2 {
        return 0.0;
    }
    let variance = values
        .iter()
        .map(|value| {
            let diff = value - avg;
            diff * diff
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    variance.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ProbeProtocol, ReachabilityCheck, ReachabilityKind, ReachabilityStatus, TargetEndpoint,
    };
    use chrono::{TimeZone, Utc};
    use std::net::Ipv4Addr;

    fn target_endpoint() -> TargetEndpoint {
        TargetEndpoint {
            input: "example.com".to_string(),
            resolved: vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))],
            protocol: ProbeProtocol::Tcp,
            port: 443,
        }
    }

    fn observed_path(at: chrono::DateTime<Utc>) -> PathObservation {
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
            observed_at: at,
            path_id: stable_path_id(&hops),
            hops,
            rtt_ms: Some(20.0),
            lost: false,
            jitter_ms: Some(1.0),
        }
    }

    struct FakeProbeEngine {
        prechecks: Vec<ReachabilityCheck>,
        observations: Vec<PathObservation>,
    }

    impl ProbeEngine for FakeProbeEngine {
        fn precheck(
            &mut self,
            _target: &TargetEndpoint,
        ) -> Result<Vec<ReachabilityCheck>, ProbeError> {
            Ok(self.prechecks.clone())
        }

        fn discover_paths(
            &mut self,
            _target: &TargetEndpoint,
            _config: DiscoveryConfig,
        ) -> Result<Vec<PathObservation>, ProbeError> {
            Ok(self.observations.clone())
        }

        fn monitor_once(
            &mut self,
            _target: &TargetEndpoint,
            _known_paths: &[crate::model::PathEvidence],
        ) -> Result<Vec<PathObservation>, ProbeError> {
            Ok(self.observations.clone())
        }
    }

    #[test]
    fn tcp_precheck_is_primary_and_icmp_is_auxiliary() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 13, 0, 0).unwrap();
        let target = target_endpoint();
        let mut engine = FakeProbeEngine {
            prechecks: vec![
                ReachabilityCheck {
                    checked_at,
                    kind: ReachabilityKind::TcpPort,
                    status: ReachabilityStatus::Reachable,
                    remote_addr: Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))),
                    rtt_ms: Some(18.0),
                    detail: Some("tcp syn-ack received".to_string()),
                },
                ReachabilityCheck {
                    checked_at,
                    kind: ReachabilityKind::IcmpEcho,
                    status: ReachabilityStatus::BlockedOrFiltered,
                    remote_addr: Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))),
                    rtt_ms: None,
                    detail: Some("icmp timed out".to_string()),
                },
            ],
            observations: vec![observed_path(checked_at + chrono::Duration::seconds(1))],
        };

        let checks = engine.precheck(&target).unwrap();
        let observations = engine
            .discover_paths(&target, DiscoveryConfig::default())
            .unwrap();

        let tcp_check = checks
            .iter()
            .find(|check| check.kind == ReachabilityKind::TcpPort)
            .unwrap();
        let icmp_check = checks
            .iter()
            .find(|check| check.kind == ReachabilityKind::IcmpEcho)
            .unwrap();
        assert_eq!(ReachabilityStatus::Reachable, tcp_check.status);
        assert_eq!(ReachabilityStatus::BlockedOrFiltered, icmp_check.status);
        assert_eq!(1, observations.len());
    }

    #[test]
    fn probe_engine_keeps_legacy_traceroute_fallback() {
        let target = target_endpoint();
        let output = r#"
Tracing route to example.com [203.0.113.10]
over a maximum of 8 hops:

  1     1 ms     2 ms     1 ms  192.168.1.1
  2    10 ms    11 ms    10 ms  203.0.113.10
"#;
        let mut engine = SystemProbeEngine::with_runner(|_addr| Ok(output.to_string()));

        let observations = engine
            .discover_paths(&target, DiscoveryConfig::default())
            .unwrap();

        assert_eq!(DiscoveryConfig::default().max_ttl, 30);
        assert_eq!(1, observations.len());
        assert_eq!(2, observations[0].hops.len());
        assert!(observations[0].hops.iter().any(|hop| {
            matches!(hop.node, HopNode::Known { ip, .. } if ip == IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)))
        }));
    }

    #[test]
    fn parses_windows_tracert_output_with_unknown_hops() {
        let output = r#"
Tracing route to baidu.com [124.237.177.164]
over a maximum of 8 hops:

  1    <1 ms    <1 ms    <1 ms  192.168.1.1
  2     4 ms     5 ms     4 ms  100.72.192.1
  3     *        *        *     Request timed out.
"#;

        let hops = parse_windows_tracert(output).unwrap();

        assert_eq!(3, hops.len());
        assert_eq!(1, hops[0].ttl);
        assert_eq!(
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            hops[0].addr
        );
        assert_eq!(vec![Some(4.0), Some(5.0), Some(4.0)], hops[1].samples_ms);
        assert_eq!(None, hops[2].addr);
        assert_eq!(vec![None, None, None], hops[2].samples_ms);
    }

    #[cfg(windows)]
    #[test]
    fn windows_traceroute_command_uses_explicit_exe_name() {
        let command = system_traceroute_command(IpAddr::V4(Ipv4Addr::new(157, 255, 219, 143)));

        assert_eq!("tracert.exe", command.get_program().to_string_lossy());
    }

    #[test]
    fn tries_next_resolved_address_when_first_probe_has_no_hops() {
        let first = IpAddr::V4(Ipv4Addr::new(157, 255, 219, 143));
        let second = IpAddr::V4(Ipv4Addr::new(124, 237, 177, 164));
        let output = r#"
Tracing route to 124.237.177.164 [124.237.177.164]
over a maximum of 8 hops:

  1    <1 ms    <1 ms    <1 ms  192.168.1.1
  2     4 ms     5 ms     4 ms  100.72.192.1
"#;
        let mut calls = Vec::new();

        let session = probe_session_with_runner("qq.com", &[first, second], |addr| {
            calls.push(addr);
            if addr == first {
                Err(ProbeError::NoHops)
            } else {
                Ok(output.to_string())
            }
        })
        .unwrap();

        assert_eq!(vec![first, second], calls);
        assert_eq!(1, session.paths.len());
        assert!(session.paths[0].hops.iter().any(|hop| {
            matches!(hop.node, HopNode::Known { ip, .. } if ip == IpAddr::V4(Ipv4Addr::new(100, 72, 192, 1)))
        }));
    }

    #[test]
    fn maps_traceroute_hops_into_trace_session() {
        let session = session_from_hops(
            "baidu.com",
            vec![IpAddr::V4(Ipv4Addr::new(124, 237, 177, 164))],
            vec![
                ParsedHop {
                    ttl: 1,
                    samples_ms: vec![Some(1.0), Some(1.0), Some(2.0)],
                    addr: Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
                },
                ParsedHop {
                    ttl: 2,
                    samples_ms: vec![None, None, None],
                    addr: None,
                },
            ],
        );

        assert_eq!("baidu.com", session.target.input);
        assert_eq!(1, session.paths.len());
        assert_eq!(2, session.paths[0].hops.len());
        assert_eq!(0.0, session.paths[0].hops[0].metrics.loss_pct);
        assert_eq!(100.0, session.paths[0].hops[1].metrics.loss_pct);
        assert!(matches!(session.paths[0].hops[1].node, HopNode::Unknown));
    }
}
