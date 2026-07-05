use crate::analyzer::{aggregate_window, stable_path_id};
use crate::model::{
    DiagnosticEvent, DiagnosticEventKind, HopClassification, HopEvidence, HopMetrics, HopNode,
    PathObservation, Severity, Target, TimeWindow, TraceSession,
};
use chrono::Utc;
use std::net::IpAddr;
use std::process::Command;

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
            .map(|ip| HopNode::Known { ip, hostname: None })
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
    use std::net::Ipv4Addr;

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
