# Portable Network Diagnostics Cockpit Design

## Purpose

Build a portable network diagnostics tool based on `trippy` for first-line technical staff at customer sites.

The tool is not a long-running NOC monitoring platform. It is a local, session-based troubleshooting cockpit used when a customer reports that access to a website, domain, or IP is slow and the operator needs to find whether the problem is tied to a specific public-network path, ECMP branch, hop range, or time window.

## Product Shape

- Ship as a single executable, with Windows as the first priority.
- Keep the architecture portable enough to produce a macOS build later.
- Require administrator/root privileges for the first version so that `trippy-core` can keep strong probing capability.
- Start a local Web cockpit on `127.0.0.1` using a random available port.
- Open the default browser automatically when possible; otherwise print the local URL in the console.
- Embed front-end static assets into the executable for the first version.

## Primary Use Case

An engineer opens the tool at a customer site, enters one target domain or IP, and monitors it for several minutes to several tens of minutes.

The operator is trying to answer questions such as:

- Which paths were observed during the diagnosis?
- Did ECMP split probes across multiple paths?
- Did only one path show higher latency, jitter, or packet loss?
- Which hop or hop range is the earliest useful evidence of degradation?
- Was the slow period correlated with a path change or higher probability of a bad path?
- Can the observed data be exported for later analysis or customer communication?

The first version should focus on one active target. The design should not block supporting 1 to 3 targets later.

## Non-Goals For Version 1

- Customer LAN device discovery and internal network topology mapping.
- Centralized multi-probe deployment.
- Long-term storage service or multi-user platform.
- Heavy desktop shell such as Electron or Tauri.
- Automated root-cause claims without showing supporting evidence.

## Recommended Architecture

Use an embedded-core architecture.

The new executable should use `trippy-core` directly instead of launching `trip` as a child process and parsing its output. This gives the application direct access to hop metrics, flow/path data, samples, and errors.

Core layers:

- `Probe Engine`: runs tracing through `trippy-core` and captures hop-level observations.
- `Session Orchestrator`: manages the active diagnostic session, target configuration, start/pause/stop state, analysis window, and exports.
- `Path Analyzer`: converts flow observations into stable paths, aggregates path metrics, identifies suspicious paths, and preserves unknown hops.
- `Local API`: serves static front-end assets, session state, exports, and time-window queries.
- `WebSocket Stream`: pushes live target snapshots to the browser.
- `Web Cockpit`: renders the topology, path-level monitoring, hop detail, event timeline, and exports.

## Data Model

The application should normalize probe results into session data shaped around:

```text
Target -> Path -> Hop -> Metrics -> Time Window
```

Important entities:

- `TraceSession`: one diagnostic session for one target.
- `Target`: input host/IP plus resolved address information.
- `PathObservation`: one observed path at a specific time.
- `StablePath`: a stable identity for a path derived from path contents, not from a transient runtime ID.
- `HopObservation`: one TTL entry in a path, including known IPs and unknown/no-response hops.
- `PathMetrics`: path-level RTT, packet loss, jitter, hit count, and window share.
- `HopMetrics`: hop-level sent/received/loss/last/avg/best/worst/stddev/jitter and status.
- `TimelineBucket`: aggregated metrics for a fixed time bucket.
- `DiagnosticEvent`: path appeared, path disappeared, path share changed, loss spike, latency spike, jitter increase, target unreachable, DNS address change, or evidence-insufficient note.

`path_id` should be stable across the session. If a path disappears and later returns, it should keep the same identity so the UI can continue showing it as the same path.

## Time Windows

The cockpit must support windowed analysis.

Built-in windows:

- Last 1 minute.
- Last 5 minutes.
- Full session.

Custom windows:

- User can select an arbitrary start and end time.
- User can brush a range in the bottom timeline.
- User can manually enter time bounds when needed.

All derived values must follow the active window:

- Path topology.
- Path appearance count.
- Path appearance share.
- RTT, loss, jitter, and status.
- Suspicious path selection.
- Export contents.
- Chart hover values.

Example: if Path B is 12% of the full session but 68% during the customer's reported slow period, the custom window should show 68%.

## Web Cockpit Layout

The interface is a troubleshooting cockpit, not a decorative monitoring wall.

Main regions:

- Top session bar: target, resolved address, elapsed time, tracing strategy, current status, start/pause/stop/export actions.
- Left summary rail: target selector for future 1 to 3 target support, health summary, suspicious paths, and event summary.
- Center topology view: all observed paths in the active time window.
- Right hop/path detail panel: selected hop or selected path metrics.
- Bottom path monitoring area: one monitoring lane per path plus event and timeline controls.

## Full-Path Topology

The center topology should render all observed paths in the active time window, not only the current path.

Required behavior:

- Merge identical hop nodes across paths.
- Draw ECMP branches as explicit path splits.
- Label complete paths as `Path A`, `Path B`, `Path C`, and so on.
- Highlight the currently active path.
- Keep previously observed paths visible with lower emphasis.
- Display each path's active-window appearance share.
- Color suspicious path sections and degraded nodes.
- Preserve all unknown/no-response hops as visible nodes.

Unknown hop handling is required. A `*`, timeout, or unknown TTL is diagnostic information and must not be removed from the topology.

Unknown hop interpretation:

- If a TTL does not respond but later TTLs respond, show it as an unknown/no-response hop and do not mark the path as broken.
- If a TTL and all later TTLs fail during a window, mark it as possible forward loss, target unreachability, or filtering, depending on surrounding evidence.
- If only the target is silent while earlier hops respond, treat it as possible target-side filtering or target silent behavior.

## Topology Interactions

The topology must support:

- Mouse wheel zoom.
- Drag-to-pan canvas movement.
- Click path to highlight it and sync bottom monitoring lanes.
- Click hop to show hop detail and filter paths containing that hop.
- Double-click node or path to focus.
- Reset view.

The implementation should use a proven graph visualization library rather than hand-rolled SVG interaction. Candidate libraries include React Flow, Cytoscape.js, Sigma.js, or another library selected during implementation planning.

## Bottom Path Monitoring

The bottom area should monitor each observed path separately.

Required data per path:

- RTT over time.
- Packet loss over time.
- Jitter over time.
- Hit count over time.
- Appearance share over the active window.
- Path appeared/disappeared events.
- Suspicious status and reason.

Hover behavior is required. When the mouse is over a chart point or time bucket, the UI should show exact values:

```text
Path B
Time: 14:08:32
RTT avg: 182ms
Loss: 12.5%
Jitter: 38ms
Hits: 7 / 20
Window share: 35%
Event: packet loss spike
```

Hover values must reflect the active time window and selected time bucket.

## Suspicion And Diagnostic Hints

The tool should identify suspicious paths and show evidence. It should avoid making unsupported root-cause claims.

Path-level suspicion:

- A path has significantly higher loss than other paths in the same window.
- A path has significantly higher RTT or jitter than other paths in the same window.
- A path's appearance share rises during a user-selected slow period.
- A newly appeared path correlates with degraded target experience.
- A path frequently appears/disappears or flips between alternatives.

Hop-level suspicion:

- If one intermediate hop has high loss but later hops and the target are healthy, mark it as likely response-limited and informational.
- If loss or latency begins at a hop and continues through later hops and the target, mark the hop range as suspicious.
- If an unknown hop is followed by healthy later hops, keep it gray/informational.
- If an unknown hop is followed by persistent loss through later hops, mark the range as suspicious.

Target-level suspicion:

- If all paths degrade at the same time, suggest common upstream, local exit, or target-side investigation.
- If only one path degrades, highlight that path as the likely problematic branch.
- If DNS resolution changes and path behavior changes, surface the DNS/IP change as an event.

Suggested default thresholds for version 1:

- Loss above 5%: warning.
- Loss above 10%: critical.
- RTT 50% higher than peer paths in the same window: warning.
- Sustained jitter increase: warning.
- Too few samples: mark as evidence-insufficient rather than warning/critical.

## Export And Session Evidence

Exports must include all path data, not only a summary.

The export model should be:

- Full evidence first.
- Diagnostic conclusion second.
- Suspicious path called out explicitly with supporting data.

Required export contents:

- Session metadata: tool version, OS, start/end time, target, resolved IPs, tracing strategy, privileges, and configuration.
- Active analysis window: start/end time and whether it was preset or custom.
- All observed paths: path ID, label, hop sequence, first seen, last seen, hit count, appearance share, status, and suspicion score/reason.
- All path metrics: RTT, packet loss, jitter, hit count, and time-bucketed values for every path.
- All hop data for every path: TTL, IP/hostname when known, unknown-hop status, sent/received/loss, latency stats, jitter, NAT/extension info when available, and evidence classification.
- All timeline events: path appeared/disappeared, DNS change, loss spike, latency spike, jitter spike, target unreachable, and evidence-insufficient notes.
- Suspicious path section: explicitly names paths that may be problematic and explains why.
- Topology snapshot metadata: active window, selected path/hop, and current layout state when export was triggered.

Required export formats:

- `JSON`: complete session evidence, including all paths, hops, metrics, events, and suspicious-path annotations.
- `CSV`: separate tables for path summary, path time buckets, hop summary, hop time buckets, and events.
- `PNG` or `SVG`: current topology view with path labels, path shares, and highlighted suspicious paths.
- `Markdown` or `HTML`: human-readable troubleshooting summary for work orders or customer communication.

The human-readable summary should name suspected paths but must still include all path summaries.

Example summary shape:

```text
Target: example.com / 203.0.113.10
Session: 14:00:00 - 14:30:00
Analysis window: 14:05:00 - 14:10:00
Observed paths: 3

Suspected problematic path: Path B
Path B appearance share: 68%
Path B average RTT: 182ms
Path B loss: 12.5%
Suspected location: after TTL 7
Evidence: Path A and Path C had loss below 1% in the same window.

All path data: included in JSON/CSV export.
Note: TTL 4 was an unknown/no-response hop, but later hops responded, so it is informational rather than a link-break conclusion.
```

## Session Save And Restore

The first version should keep session data in memory while the tool is running.

When the user saves a session, write a `.netdiag-session.json` file that can be loaded later. It should restore:

- Target and resolved addresses.
- Session start/end time.
- All observed paths.
- All hops.
- All time buckets.
- All diagnostic events.
- Suspicious-path annotations.
- User-selected analysis windows.

## Error Handling

The UI should clearly distinguish:

- Insufficient privileges.
- Target DNS resolution failure.
- Local socket/probe setup failure.
- Firewall or packet capture restrictions.
- Target silent behavior.
- Intermediate hop response limiting.
- Actual sustained downstream packet loss evidence.

The tool should keep raw evidence available even when it cannot produce a confident diagnostic hint.

## Testing Strategy

Testing should focus on path analysis correctness and export fidelity.

Recommended coverage:

- Stable path ID generation for repeated paths.
- ECMP split and merge cases.
- Unknown hop preservation.
- Intermediate-hop response limiting versus downstream loss.
- Custom time window aggregation.
- Path appearance share calculation.
- Suspicious path selection with insufficient sample guards.
- JSON export includes all paths, hops, metrics, events, and annotations.
- CSV exports contain complete path/hop/time-bucket tables.
- Front-end chart hover shows exact values for the active bucket.
- Topology interactions preserve selected path/hop state.

`trippy-core` simulation resources can be reused for deterministic path and hop scenarios.

## Scope Check

This is a single implementation effort if scoped as:

- One target active in version 1.
- Local Web cockpit only.
- Embedded `trippy-core` engine.
- In-memory session with explicit save/export.
- Windows first, macOS-ready architecture.
- Full path evidence and suspicious-path annotations.

Multi-target concurrent monitoring, distributed probes, and customer LAN topology discovery should be separate later specifications.
