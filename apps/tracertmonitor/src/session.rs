use crate::analyzer::aggregate_window;
use crate::geoip::{DefaultGeoIpResolver, GeoIpResolver};
use crate::model::{
    DiagnosticConfig, DiagnosticEvent, DiagnosticEventKind, PathEvidence, PathObservation,
    ReachabilityCheck, Severity, TargetEndpoint, TimeWindow,
};
use crate::probe::{DiscoveryConfig, ProbeEngine, ProbeError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticPhase {
    Idle,
    Precheck,
    Discovering,
    Monitoring,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSnapshot {
    pub phase: DiagnosticPhase,
    pub target: TargetEndpoint,
    pub config: DiagnosticConfig,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub prechecks: Vec<ReachabilityCheck>,
    pub paths: Vec<PathEvidence>,
    pub observations: Vec<PathObservation>,
    pub events: Vec<DiagnosticEvent>,
}

#[derive(Debug, Clone)]
pub struct DiagnosticSession {
    phase: DiagnosticPhase,
    target: TargetEndpoint,
    config: DiagnosticConfig,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    prechecks: Vec<ReachabilityCheck>,
    paths: Vec<PathEvidence>,
    observations: Vec<PathObservation>,
    events: Vec<DiagnosticEvent>,
}

const MONITOR_LOSS_SPIKE_THRESHOLD_PCT: f64 = 5.0;
const MONITOR_LATENCY_SPIKE_THRESHOLD_MS: f64 = 150.0;

impl DiagnosticSession {
    pub fn new(target: TargetEndpoint) -> Self {
        Self::with_config(target, DiagnosticConfig::default())
    }

    pub fn with_config(target: TargetEndpoint, config: DiagnosticConfig) -> Self {
        Self {
            phase: DiagnosticPhase::Idle,
            target,
            config,
            started_at: Utc::now(),
            ended_at: None,
            prechecks: Vec::new(),
            paths: Vec::new(),
            observations: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn start_with_probe(
        &mut self,
        engine: &mut impl ProbeEngine,
        config: DiscoveryConfig,
        window: TimeWindow,
    ) -> Result<(), ProbeError> {
        let prechecks = match engine.precheck(&self.target) {
            Ok(prechecks) => prechecks,
            Err(error) => {
                self.record_probe_error(&error);
                return Err(error);
            }
        };
        self.start_precheck(prechecks);

        let observations = match engine.discover_paths(&self.target, config) {
            Ok(observations) => observations,
            Err(error) => {
                self.record_probe_error(&error);
                return Err(error);
            }
        };
        for observation in observations {
            self.record_observation(observation);
        }

        let mut geoip = DefaultGeoIpResolver::default();
        self.attach_geoip(&mut geoip);
        self.refresh_analysis(window);
        Ok(())
    }

    pub fn monitor_once(
        &mut self,
        engine: &mut impl ProbeEngine,
        window: TimeWindow,
    ) -> Result<(), ProbeError> {
        let observations = match engine.monitor_once(&self.target, &self.paths) {
            Ok(observations) => observations,
            Err(error) => {
                self.record_probe_error(&error);
                return Err(error);
            }
        };
        self.observations.extend(observations);
        let mut geoip = DefaultGeoIpResolver::default();
        self.attach_geoip(&mut geoip);
        self.refresh_analysis(window);
        self.record_monitoring_threshold_events(window.end);
        Ok(())
    }

    pub fn attach_geoip(&mut self, resolver: &mut impl GeoIpResolver) {
        let config = self.config.geoip.clone();
        for observation in &mut self.observations {
            attach_geoip_to_hops(&mut observation.hops, resolver, &config);
        }
        for path in &mut self.paths {
            attach_geoip_to_hops(&mut path.hops, resolver, &config);
        }
    }

    fn record_probe_error(&mut self, error: &ProbeError) {
        self.phase = DiagnosticPhase::Failed;
        self.events.push(DiagnosticEvent {
            at: Utc::now(),
            severity: Severity::Warning,
            kind: DiagnosticEventKind::EvidenceInsufficient,
            message: format!("Probe engine failed: {error}"),
        });
    }
    pub fn start_precheck(&mut self, checks: Vec<ReachabilityCheck>) {
        self.phase = DiagnosticPhase::Precheck;
        let event_at = checks
            .last()
            .map(|check| check.checked_at)
            .unwrap_or(self.started_at);
        self.prechecks.extend(checks);
        self.events.push(DiagnosticEvent {
            at: event_at,
            severity: Severity::Info,
            kind: DiagnosticEventKind::PrecheckCompleted,
            message: "Precheck evidence recorded".to_string(),
        });
    }

    pub fn record_observation(&mut self, observation: PathObservation) {
        self.phase = DiagnosticPhase::Discovering;
        self.observations.push(observation);
    }

    pub fn refresh_analysis(&mut self, window: TimeWindow) {
        let snapshot = aggregate_window(&self.observations, window);
        self.paths = snapshot.paths;
        if self.paths.is_empty() {
            self.phase = DiagnosticPhase::Discovering;
            return;
        }
        self.phase = DiagnosticPhase::Monitoring;
        self.events.push(DiagnosticEvent {
            at: snapshot.window_end,
            severity: Severity::Info,
            kind: DiagnosticEventKind::DiscoveryUpdated,
            message: "Path discovery analysis refreshed".to_string(),
        });
    }

    fn record_monitoring_threshold_events(&mut self, at: DateTime<Utc>) {
        let mut events = Vec::new();
        for path in &self.paths {
            if path.metrics.loss_pct >= MONITOR_LOSS_SPIKE_THRESHOLD_PCT {
                events.push(DiagnosticEvent {
                    at,
                    severity: path
                        .suspicion
                        .as_ref()
                        .map(|suspicion| suspicion.severity)
                        .unwrap_or(Severity::Warning),
                    kind: DiagnosticEventKind::LossSpike,
                    message: format!(
                        "{} loss {:.1}% across {} samples",
                        path.label, path.metrics.loss_pct, path.metrics.sample_count
                    ),
                });
            }
            if path.metrics.avg_rtt_ms >= MONITOR_LATENCY_SPIKE_THRESHOLD_MS {
                events.push(DiagnosticEvent {
                    at,
                    severity: Severity::Warning,
                    kind: DiagnosticEventKind::LatencySpike,
                    message: format!(
                        "{} average RTT {:.1} ms exceeds {:.1} ms",
                        path.label, path.metrics.avg_rtt_ms, MONITOR_LATENCY_SPIKE_THRESHOLD_MS
                    ),
                });
            }
        }
        self.events.extend(events);
    }

    pub fn stop(&mut self, ended_at: DateTime<Utc>) {
        self.phase = DiagnosticPhase::Stopped;
        self.ended_at = Some(ended_at);
    }

    pub fn snapshot(&self) -> DiagnosticSnapshot {
        DiagnosticSnapshot {
            phase: self.phase,
            target: self.target.clone(),
            config: self.config.clone(),
            started_at: self.started_at,
            ended_at: self.ended_at,
            prechecks: self.prechecks.clone(),
            paths: self.paths.clone(),
            observations: self.observations.clone(),
            events: self.events.clone(),
        }
    }
}

fn attach_geoip_to_hops(
    hops: &mut [crate::model::HopEvidence],
    resolver: &mut impl GeoIpResolver,
    config: &crate::model::GeoIpConfig,
) {
    for hop in hops {
        if let crate::model::HopNode::Known { ip, geoip, .. } = &mut hop.node {
            if geoip.is_none() {
                *geoip = Some(resolver.resolve(*ip, config));
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stable_path_id;
    use crate::geoip::GeoIpResolver;
    use crate::model::{
        DiagnosticEventKind, GeoIpConfig, GeoIpInfo, GeoIpLookupStatus, HopClassification,
        HopEvidence, HopMetrics, HopNode, PathObservation, ProbeProtocol, ReachabilityCheck,
        ReachabilityKind, ReachabilityStatus, TargetEndpoint, TimeWindow,
    };
    use crate::probe::{DiscoveryConfig, ProbeEngine, ProbeError};
    use chrono::{TimeZone, Utc};
    use std::net::{IpAddr, Ipv4Addr};

    struct FakeProbeEngine {
        prechecks: Vec<ReachabilityCheck>,
        observations: Vec<PathObservation>,
    }

    impl FakeProbeEngine {
        fn precheck(&self) -> Vec<ReachabilityCheck> {
            self.prechecks.clone()
        }

        fn discover_paths(&self) -> Vec<PathObservation> {
            self.observations.clone()
        }
    }

    fn known_hop(ttl: u8, last_octet: u8) -> HopEvidence {
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

    fn target_endpoint() -> TargetEndpoint {
        TargetEndpoint {
            input: "example.com".to_string(),
            resolved: vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))],
            protocol: ProbeProtocol::Tcp,
            port: 443,
        }
    }

    fn tcp_precheck(checked_at: chrono::DateTime<Utc>) -> ReachabilityCheck {
        ReachabilityCheck {
            checked_at,
            kind: ReachabilityKind::TcpPort,
            status: ReachabilityStatus::Reachable,
            remote_addr: Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))),
            rtt_ms: Some(16.0),
            detail: Some("tcp syn-ack received".to_string()),
        }
    }

    fn path_observation(observed_at: chrono::DateTime<Utc>, second_hop: u8) -> PathObservation {
        let hops = vec![known_hop(1, 1), known_hop(2, second_hop)];
        PathObservation {
            observed_at,
            path_id: stable_path_id(&hops),
            hops,
            rtt_ms: Some(20.0 + f64::from(second_hop)),
            lost: false,
            jitter_ms: Some(1.0),
        }
    }

    struct SessionProbeEngine {
        prechecks: Vec<ReachabilityCheck>,
        observations: Vec<PathObservation>,
        monitor_observations: Vec<Vec<PathObservation>>,
        precheck_calls: usize,
        discovery_calls: usize,
        monitor_calls: usize,
    }

    impl ProbeEngine for SessionProbeEngine {
        fn precheck(
            &mut self,
            _target: &TargetEndpoint,
        ) -> Result<Vec<ReachabilityCheck>, ProbeError> {
            self.precheck_calls += 1;
            Ok(self.prechecks.clone())
        }

        fn discover_paths(
            &mut self,
            _target: &TargetEndpoint,
            _config: DiscoveryConfig,
        ) -> Result<Vec<PathObservation>, ProbeError> {
            self.discovery_calls += 1;
            Ok(self.observations.clone())
        }

        fn monitor_once(
            &mut self,
            _target: &TargetEndpoint,
            _known_paths: &[crate::model::PathEvidence],
        ) -> Result<Vec<PathObservation>, ProbeError> {
            let observations = self
                .monitor_observations
                .get(self.monitor_calls)
                .cloned()
                .unwrap_or_default();
            self.monitor_calls += 1;
            Ok(observations)
        }
    }

    #[test]
    fn session_runs_probe_engine_and_records_events() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 13, 30, 0).unwrap();
        let mut engine = SessionProbeEngine {
            prechecks: vec![tcp_precheck(checked_at)],
            observations: vec![
                path_observation(checked_at + chrono::Duration::seconds(1), 10),
                path_observation(checked_at + chrono::Duration::seconds(2), 20),
            ],
            monitor_observations: Vec::new(),
            precheck_calls: 0,
            discovery_calls: 0,
            monitor_calls: 0,
        };
        let mut session = DiagnosticSession::new(target_endpoint());

        session
            .start_with_probe(
                &mut engine,
                DiscoveryConfig::default(),
                TimeWindow {
                    start: checked_at,
                    end: checked_at + chrono::Duration::seconds(10),
                },
            )
            .unwrap();

        let snapshot = session.snapshot();

        assert_eq!(1, engine.precheck_calls);
        assert_eq!(1, engine.discovery_calls);
        assert_eq!(DiagnosticPhase::Monitoring, snapshot.phase);
        assert_eq!(1, snapshot.prechecks.len());
        assert_eq!(2, snapshot.observations.len());
        assert_eq!(2, snapshot.paths.len());
        assert!(
            snapshot
                .events
                .iter()
                .any(|event| matches!(&event.kind, DiagnosticEventKind::PrecheckCompleted))
        );
        assert!(
            snapshot
                .events
                .iter()
                .any(|event| matches!(&event.kind, DiagnosticEventKind::DiscoveryUpdated))
        );
    }
    #[test]
    fn monitoring_records_path_share_over_time() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 14, 0, 0).unwrap();
        let discovery_a = path_observation(checked_at + chrono::Duration::seconds(1), 10);
        let monitor_a = path_observation(checked_at + chrono::Duration::seconds(2), 10);
        let monitor_b = path_observation(checked_at + chrono::Duration::seconds(3), 20);
        let path_a_id = discovery_a.path_id.clone();
        let mut engine = SessionProbeEngine {
            prechecks: vec![tcp_precheck(checked_at)],
            observations: vec![discovery_a],
            monitor_observations: vec![vec![monitor_a], vec![monitor_b]],
            precheck_calls: 0,
            discovery_calls: 0,
            monitor_calls: 0,
        };
        let mut session = DiagnosticSession::new(target_endpoint());
        let window = TimeWindow {
            start: checked_at,
            end: checked_at + chrono::Duration::seconds(10),
        };

        session
            .start_with_probe(&mut engine, DiscoveryConfig::default(), window)
            .unwrap();
        session.monitor_once(&mut engine, window).unwrap();
        session.monitor_once(&mut engine, window).unwrap();

        let snapshot = session.snapshot();
        let path_a = snapshot
            .paths
            .iter()
            .find(|path| path.id == path_a_id)
            .unwrap();

        assert_eq!(2, engine.monitor_calls);
        assert_eq!(DiagnosticPhase::Monitoring, snapshot.phase);
        assert_eq!(3, snapshot.observations.len());
        assert!((path_a.metrics.window_share_pct - 66.666).abs() < 0.01);
    }

    struct StaticGeoIpResolver;

    impl GeoIpResolver for StaticGeoIpResolver {
        fn resolve(&mut self, _ip: IpAddr, _config: &GeoIpConfig) -> GeoIpInfo {
            GeoIpInfo {
                country_or_region: Some("中国".to_string()),
                province: Some("广东省".to_string()),
                city: Some("广州市".to_string()),
                carrier_or_asn: Some("中国电信 AS4134".to_string()),
                source: Some("local-fixture".to_string()),
                status: GeoIpLookupStatus::LocalHit,
            }
        }
    }

    #[test]
    fn session_attaches_geoip_to_known_hops() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 14, 30, 0).unwrap();
        let mut session = DiagnosticSession::new(target_endpoint());
        let mut resolver = StaticGeoIpResolver;
        let hops = vec![known_hop(1, 10)];
        let path_id = stable_path_id(&hops);
        session.start_precheck(vec![tcp_precheck(checked_at)]);
        session.record_observation(PathObservation {
            observed_at: checked_at + chrono::Duration::seconds(1),
            path_id: path_id.clone(),
            hops,
            rtt_ms: Some(18.0),
            lost: false,
            jitter_ms: Some(0.5),
        });

        session.attach_geoip(&mut resolver);
        session.refresh_analysis(TimeWindow {
            start: checked_at,
            end: checked_at + chrono::Duration::seconds(10),
        });

        let snapshot = session.snapshot();
        let hop = &snapshot.paths[0].hops[0];
        assert_eq!(path_id, snapshot.paths[0].id);
        assert!(matches!(
            &hop.node,
            HopNode::Known {
                geoip: Some(info),
                ..
            } if info.province.as_deref() == Some("广东省")
                && info.city.as_deref() == Some("广州市")
                && info.carrier_or_asn.as_deref() == Some("中国电信 AS4134")
        ));
    }

    struct FailingDiscoveryEngine {
        prechecks: Vec<ReachabilityCheck>,
    }

    impl ProbeEngine for FailingDiscoveryEngine {
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
            Err(ProbeError::NoHops)
        }

        fn monitor_once(
            &mut self,
            _target: &TargetEndpoint,
            _known_paths: &[crate::model::PathEvidence],
        ) -> Result<Vec<PathObservation>, ProbeError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn session_records_probe_failure_as_event() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 13, 45, 0).unwrap();
        let mut engine = FailingDiscoveryEngine {
            prechecks: vec![tcp_precheck(checked_at)],
        };
        let mut session = DiagnosticSession::new(target_endpoint());

        let result = session.start_with_probe(
            &mut engine,
            DiscoveryConfig::default(),
            TimeWindow {
                start: checked_at,
                end: checked_at + chrono::Duration::seconds(10),
            },
        );

        assert!(matches!(result, Err(ProbeError::NoHops)));
        let snapshot = session.snapshot();
        assert_eq!(DiagnosticPhase::Failed, snapshot.phase);
        assert_eq!(1, snapshot.prechecks.len());
        assert!(snapshot.observations.is_empty());
        assert!(
            snapshot
                .events
                .iter()
                .any(|event| matches!(&event.kind, DiagnosticEventKind::EvidenceInsufficient))
        );
    }
    #[test]
    fn session_runs_precheck_discovery_and_snapshot() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 10, 0, 0).unwrap();
        let target = TargetEndpoint {
            input: "example.com".to_string(),
            resolved: vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))],
            protocol: ProbeProtocol::Tcp,
            port: 443,
        };
        let hops = vec![known_hop(1, 1), known_hop(2, 10)];
        let fake = FakeProbeEngine {
            prechecks: vec![ReachabilityCheck {
                checked_at,
                kind: ReachabilityKind::TcpPort,
                status: ReachabilityStatus::Reachable,
                remote_addr: Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))),
                rtt_ms: Some(16.0),
                detail: Some("tcp syn-ack received".to_string()),
            }],
            observations: vec![PathObservation {
                observed_at: checked_at + chrono::Duration::seconds(1),
                path_id: stable_path_id(&hops),
                hops,
                rtt_ms: Some(20.0),
                lost: false,
                jitter_ms: Some(1.0),
            }],
        };

        let mut session = DiagnosticSession::new(target.clone());
        session.start_precheck(fake.precheck());
        for observation in fake.discover_paths() {
            session.record_observation(observation);
        }
        session.refresh_analysis(TimeWindow {
            start: checked_at,
            end: checked_at + chrono::Duration::seconds(10),
        });

        let snapshot = session.snapshot();

        assert_eq!(DiagnosticPhase::Monitoring, snapshot.phase);
        assert_eq!(target.input, snapshot.target.input);
        assert_eq!(443, snapshot.target.port);
        assert_eq!(1, snapshot.prechecks.len());
        assert_eq!(1, snapshot.observations.len());
        assert_eq!(1, snapshot.paths.len());
        assert!(!snapshot.events.is_empty());
    }

    struct ProbeStub {
        prechecks: Vec<ReachabilityCheck>,
        observations: Vec<PathObservation>,
        precheck_calls: usize,
        discovery_calls: usize,
    }

    impl ProbeStub {
        fn precheck(&mut self) -> Vec<ReachabilityCheck> {
            self.precheck_calls += 1;
            self.prechecks.clone()
        }

        fn discover_paths(&mut self) -> Vec<PathObservation> {
            self.discovery_calls += 1;
            self.observations.clone()
        }
    }

    fn run_session_with_probe_stub(
        mut probe: ProbeStub,
        window: TimeWindow,
    ) -> (DiagnosticSnapshot, ProbeStub) {
        let mut session = DiagnosticSession::new(target_endpoint());
        session.start_precheck(probe.precheck());
        for observation in probe.discover_paths() {
            session.record_observation(observation);
        }
        session.refresh_analysis(window);
        (session.snapshot(), probe)
    }

    #[test]
    fn top_down_session_uses_probe_stub_for_precheck_and_discovery() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 11, 0, 0).unwrap();
        let probe = ProbeStub {
            prechecks: vec![tcp_precheck(checked_at)],
            observations: vec![
                path_observation(checked_at + chrono::Duration::seconds(1), 10),
                path_observation(checked_at + chrono::Duration::seconds(2), 20),
            ],
            precheck_calls: 0,
            discovery_calls: 0,
        };

        let (snapshot, probe) = run_session_with_probe_stub(
            probe,
            TimeWindow {
                start: checked_at,
                end: checked_at + chrono::Duration::seconds(10),
            },
        );

        assert_eq!(1, probe.precheck_calls);
        assert_eq!(1, probe.discovery_calls);
        assert_eq!(DiagnosticPhase::Monitoring, snapshot.phase);
        assert_eq!(1, snapshot.prechecks.len());
        assert_eq!(2, snapshot.observations.len());
        assert_eq!(2, snapshot.paths.len());
        assert!(
            snapshot
                .events
                .iter()
                .any(|event| matches!(&event.kind, DiagnosticEventKind::PrecheckCompleted))
        );
        assert!(
            snapshot
                .events
                .iter()
                .any(|event| matches!(&event.kind, DiagnosticEventKind::DiscoveryUpdated))
        );
    }

    struct SessionCallingDriver;

    impl SessionCallingDriver {
        fn run_until_stopped(
            &self,
            mut session: DiagnosticSession,
            prechecks: Vec<ReachabilityCheck>,
            observations: Vec<PathObservation>,
            window: TimeWindow,
            stopped_at: chrono::DateTime<Utc>,
        ) -> DiagnosticSnapshot {
            session.start_precheck(prechecks);
            for observation in observations {
                session.record_observation(observation);
            }
            session.refresh_analysis(window);
            session.stop(stopped_at);
            session.snapshot()
        }
    }

    #[test]
    fn bottom_up_calling_driver_can_drive_session_and_read_snapshot() {
        let checked_at = Utc.with_ymd_and_hms(2026, 7, 5, 12, 0, 0).unwrap();
        let driver = SessionCallingDriver;

        let snapshot = driver.run_until_stopped(
            DiagnosticSession::new(target_endpoint()),
            vec![tcp_precheck(checked_at)],
            vec![path_observation(
                checked_at + chrono::Duration::seconds(1),
                10,
            )],
            TimeWindow {
                start: checked_at,
                end: checked_at + chrono::Duration::seconds(10),
            },
            checked_at + chrono::Duration::seconds(30),
        );

        let json = serde_json::to_value(&snapshot).unwrap();

        assert_eq!(DiagnosticPhase::Stopped, snapshot.phase);
        assert_eq!(
            Some(checked_at + chrono::Duration::seconds(30)),
            snapshot.ended_at
        );
        assert_eq!(1, snapshot.paths.len());
        assert_eq!("stopped", json["phase"]);
        assert_eq!("example.com", json["target"]["input"]);
        assert_eq!(443, json["target"]["port"]);
    }
}
