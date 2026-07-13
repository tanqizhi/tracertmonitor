# V1 Diagnostic Flow Next Tasks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按依赖顺序把 TracertMonitor 从 demo/fallback 驾驶舱推进到 V1 可用的 TCP-first 现场诊断流程。

**Architecture:** 继续保持单个 Rust 应用 crate。`model` 定义共享语言，`probe` 采集证据，`session` 组织诊断生命周期，`analyzer` 解释观测，`server` 暴露本地 HTTP/API，`export` 导出证据，前端只负责展示和发起请求。

**Tech Stack:** Rust、当前内嵌 HTML/CSS/原生 JavaScript、HTTP + JSON、CSV/JSON 导出、vendored Trippy crates、系统 `tracert/traceroute` fallback。

---

## 当前前置状态

已经完成：

- `apps/tracertmonitor/src/model.rs` 已有 V1 基础类型：`TargetEndpoint`、`ProbeProtocol`、`ReachabilityCheck`、`ProbeRound`、`FlowKey`、`TtlProbeSample`、`ProbeResponse`。
- `apps/tracertmonitor/src/session.rs` 已有 `DiagnosticSession`、`DiagnosticPhase`、snapshot、precheck/observation/analysis 刷新方法。
- `session` 已补直接生命周期测试、自顶向下 probe 桩模块测试、自下向上调用模块 driver 测试。

还没有完成：

- `probe` 还没有正式的 TCP-first `ProbeEngine` 边界。
- `server` 还没有把 start request 里的端口、协议和配置交给 `session`。
- 前端还没有正式展示 precheck/discovery/monitoring 状态。
- 导出还没有包含 V1 precheck、端口、协议、probe round 等证据。
- GeoIP 模型、resolver 和 fallback 基础能力已实现；端到端验证和文档收口已在 2026-07-07 完成。

## 总体顺序

```text
1. ProbeEngine 和 TCP-first precheck
2. Session 接入 ProbeEngine
3. Server API 接入 target/protocol/port/session
4. Analyzer 强化 ECMP 和 unknown hop 语义
5. Session 增加 monitor tick
6. Export 导出 V1 证据
7. 前端接入 V1 状态和端口输入
8. 配置栏和 DiagnosticConfig
9. GeoIP 模型、resolver 和 fallback
10. 端到端验证和文档收口（已完成，2026-07-07）
```

这个顺序的理由：先把下层证据采集边界定住，再让 `session` 变成真正中控，然后 `server` 和前端只需要调用稳定接口。GeoIP 和配置栏依赖 `DiagnosticConfig` 和 session snapshot，所以放在核心诊断链路之后。

---

### Task 1: 定义 ProbeEngine 和 TCP-first Precheck

**目标：** 让 `probe` 从“一次性系统 traceroute 函数”演进为“可被 session 调用的证据采集接口”。TCP 是主证据，ICMP 是辅助证据。

**Files:**

- Modify: `apps/tracertmonitor/src/probe.rs`
- Read: `doc/business-flow.zh-CN.md`
- Read: `doc/communication.zh-CN.md`

**做什么：**

- 新增 `ProbeEngine` trait 或等价接口，包含 `precheck`、`discover_paths`、`monitor_once`。
- 新增 `DiscoveryConfig` 或先用轻量配置结构表达 `max_ttl`、`rounds`、`probes_per_ttl`。
- 实现一个 `SystemProbeEngine` 或 `FallbackProbeEngine`，保留现在的系统 `tracert/traceroute` fallback。
- 增加 TCP precheck 的结构化结果：open、closed/RST、timeout/filtered 都要进入 `ReachabilityCheck`。
- 域名目标要记录 DNS 解析结果；IP 目标要保留原 IP。
- ICMP 失败不能覆盖 TCP 成功，因为现场很多业务禁 ICMP。

**测试计划：**

- [x] 写失败测试：`probe::tests::tcp_precheck_is_primary_and_icmp_is_auxiliary`
- [x] 写失败测试：`probe::tests::probe_engine_keeps_legacy_traceroute_fallback`
- [x] 跑测试并确认失败原因是接口不存在或行为未实现。
- [x] 实现最小 `ProbeEngine` 边界。
- [x] 跑 `cargo test -p tracertmonitor --lib probe::tests`

**测试关键断言：**

```rust
assert_eq!(ReachabilityStatus::Reachable, tcp_check.status);
assert_eq!(ReachabilityStatus::BlockedOrFiltered, icmp_check.status);
assert!(session_or_result_does_not_fail_when_tcp_is_reachable);
```


**验收标准：**

- `probe` 测试通过。
- 当前 `probe_session_for_target` 还可用。
- 新接口能被 fake engine 和未来 Trippy-backed engine 同时实现。

---

### Task 2: Session 接入 ProbeEngine

**目标：** 让 `session` 不再只接受外部手动喂数据，而是能通过 `ProbeEngine` 组织一次诊断流程。

**Files:**

- Modify: `apps/tracertmonitor/src/session.rs`
- Modify: `apps/tracertmonitor/src/probe.rs`
- Read: `doc/modules.zh-CN.md`

**做什么：**

- 在 `DiagnosticSession` 上增加同步方法，例如 `run_discovery_once` 或 `start_with_probe`。
- 方法内部顺序固定为：precheck -> discovery observations -> refresh_analysis。
- `session` 只调度和保存证据，不直接写 TCP socket 细节。
- `session` 记录 phase：`Precheck`、`Discovering`、`Monitoring` 或 `Failed`。
- 下层 probe 错误要变成 `DiagnosticEvent`，不要只作为字符串丢掉。

**测试计划：**

- [x] 写失败测试：`session::tests::session_runs_probe_engine_and_records_events`
- [x] 用 fake `ProbeEngine` 返回 TCP precheck 和两条 path observation。
- [x] 断言 session snapshot 包含 prechecks、observations、paths、events。
- [x] 断言 probe 失败时 snapshot phase 为 `Failed` 或事件为 `EvidenceInsufficient`。
- [x] 跑 `cargo test -p tracertmonitor --lib session::tests`

**测试关键断言：**

```rust
assert_eq!(DiagnosticPhase::Monitoring, snapshot.phase);
assert_eq!(1, snapshot.prechecks.len());
assert_eq!(2, snapshot.observations.len());
assert!(snapshot.events.iter().any(|event| matches!(event.kind, DiagnosticEventKind::DiscoveryUpdated)));
```


**验收标准：**

- `session` 能作为真正的中控调用 probe。
- fake probe 测试覆盖成功路径和失败路径。
- `server` 还可以暂时不接入，但下一步能直接调用 session 方法。

---

### Task 3: Server API 接入 Target/Protocol/Port/Session

**目标：** `/api/session/start` 正式接受目标、协议、端口和基础配置，并交给 `session`，而不是由 `server` 自己决定探测流程。

**Files:**

- Modify: `apps/tracertmonitor/src/server.rs`
- Modify: `apps/tracertmonitor/src/session.rs`
- Modify: `apps/tracertmonitor/src/model.rs` if request/response needs shared types

**做什么：**

- `StartSessionRequest` 增加 `protocol` 和 `port`。
- 缺省端口为 `443`。
- `port = 0` 返回 HTTP 400。
- target 为空返回 HTTP 400。
- 请求中的 target/protocol/port 转为 `TargetEndpoint`。
- `server` 调用 `session`，不要直接调用 `probe_session_for_target` 决定诊断流程。
- `/api/session` 返回 V1 snapshot，包含 phase、target、prechecks、paths、observations、events。

**测试计划：**

- [x] 写失败测试：`server::tests::start_route_accepts_target_protocol_and_tcp_port`
- [x] 写失败测试：`server::tests::start_route_rejects_zero_port`
- [x] 写失败测试：`server::tests::start_route_defaults_port_to_443`
- [x] 跑 `cargo test -p tracertmonitor --lib server::tests`

**测试关键请求：**

```json
{
  "target": "www.ctyun.cn",
  "protocol": "tcp",
  "port": 443,
  "packet_interval_ms": 1000
}
```


**验收标准：**

- start route 响应里能看到 `target.port = 443` 和 `target.protocol = tcp`。
- `server` 测试覆盖空 target、默认端口、非法端口。
- 旧的 demo 页面和导出路由不被破坏。

---

### Task 4: Analyzer 强化 ECMP 和 Unknown Hop 语义

**目标：** 分析器能把多轮 discovery 里的 ECMP 分支稳定识别成 Path A/B/C/D，并保留 unknown hop。

**Files:**

- Modify: `apps/tracertmonitor/src/analyzer.rs`
- Modify: `apps/tracertmonitor/src/model.rs` if snapshot type needs extension

**做什么：**

- 明确 path identity 基于有序 hop evidence。
- `unknown` hop 必须参与 path id，不能被过滤。
- 对同一个目标、不同 flow 下的不同中间 hop，生成不同稳定路径。
- Path label 尽量稳定，避免同一条路径在刷新后从 Path A 跳成 Path B。
- 只因为中间 hop unknown 不应标记故障；如果后续 hop 正常，它是 informational evidence。

**测试计划：**

- [x] 写失败测试：`analyzer::tests::ecmp_discovery_creates_stable_paths`
- [x] 写失败测试：`analyzer::tests::unknown_middle_hop_is_preserved_without_failure`
- [x] 跑 `cargo test -p tracertmonitor --lib analyzer::tests`

**测试关键场景：**

```text
Path A: TTL1 192.0.2.1 -> TTL2 198.51.100.1 -> target
Path B: TTL1 192.0.2.1 -> TTL2 198.51.100.2 -> target
Path C: TTL1 192.0.2.1 -> TTL2 unknown -> target
```


**验收标准：**

- analyzer 至少输出 Path A、Path B。
- unknown hop 被保留在 path id 和 hops 中。
- informational unknown 不直接触发 Critical suspicion。

---

### Task 5: Session 增加 Monitor Tick

**目标：** discovery 后进入持续监测，session 能周期性追加观测、刷新窗口指标和事件。

**Files:**

- Modify: `apps/tracertmonitor/src/session.rs`
- Modify: `apps/tracertmonitor/src/probe.rs`
- Modify: `apps/tracertmonitor/src/analyzer.rs` if window aggregation needs richer output

**做什么：**

- 增加 `monitor_once` 方法。
- `monitor_once` 从 `ProbeEngine` 取得新 observations。
- 追加 observations，而不是覆盖历史。
- 刷新当前时间窗口的 paths、loss、RTT、jitter、window share。
- 丢包或延迟超过阈值时记录 `DiagnosticEvent`。
- 不在 `probe` 或 `analyzer` 里开后台线程；定时器应由 server 或更上层调用 session 方法。

**测试计划：**

- [x] 写失败测试：`session::tests::monitoring_records_path_share_over_time`
- [x] fake probe 返回 Path A 两次、Path B 一次。
- [x] 断言 Path A window share 约为 66.7%。
- [x] 跑 `cargo test -p tracertmonitor --lib session::tests`

**测试关键断言：**

```rust
assert_eq!(DiagnosticPhase::Monitoring, snapshot.phase);
assert_eq!(3, snapshot.observations.len());
assert!((path_a.metrics.window_share_pct - 66.666).abs() < 0.01);
```


**验收标准：**

- session 可以多次 monitor tick。
- 旧 observations 不丢失。
- analyzer 计算出的路径占比可以用于前端路径图表。

---

### Task 6: Export 导出 V1 完整证据

**目标：** 导出的 JSON/CSV 能解释一次 V1 诊断，不只导出旧的路径摘要。

**Files:**

- Modify: `apps/tracertmonitor/src/export.rs`
- Modify: `apps/tracertmonitor/src/server.rs` if new export routes are added
- Modify: `apps/tracertmonitor/src/model.rs` if export DTO is needed

**做什么：**

- JSON 导出包含 target input、resolved IP、protocol、port。
- JSON 导出包含 prechecks。
- JSON 导出包含 observations、paths、events。
- CSV 增加或扩展 `prechecks.csv`。
- 现有 `path-summary.csv`、`hop-summary.csv`、`observations.csv`、`events.csv` 保持可用。
- 导出不重新探测，只读取 session snapshot。

**测试计划：**

- [x] 写失败测试：`export::tests::json_export_contains_v1_diagnostic_flow_evidence`
- [x] 写失败测试：`export::tests::csv_bundle_contains_prechecks_table`
- [x] 跑 `cargo test -p tracertmonitor --lib export::tests`

**测试关键断言：**

```rust
assert!(json.contains("\"port\":443"));
assert!(json.contains("tcp_port"));
assert!(csv.prechecks.contains("Kind,Status,RemoteAddr,RttMs,Detail"));
```


**验收标准：**

- 导出包能说明目标、端口、协议、预检查、路径、观测和事件。
- CSV 表头稳定，方便现场交付和后续脚本处理。
- 导出失败不会影响当前 session 状态。

---

### Task 7: 前端接入 V1 状态和端口输入

**目标：** Web 驾驶舱首屏符合 V1 现场使用流程：目标、协议、端口、开始诊断、阶段状态、预检查结果。

**Files:**

- Modify: `apps/tracertmonitor/src/assets/index.html`
- Modify: `apps/tracertmonitor/src/assets/app.js`
- Modify: `apps/tracertmonitor/src/assets/styles.css`
- Modify: `apps/tracertmonitor/src/server.rs` tests that inspect assets

**做什么：**

- 主输入区保留目标、协议、端口、开始按钮。
- 端口默认 `443`。
- `app.js` 向 `/api/session/start` 发送 target/protocol/port/config。
- 展示 phase：precheck、discovering、monitoring、failed、stopped。
- 展示 precheck 结果：DNS、TCP port、ICMP echo。
- 图表继续使用现有 paths/observations 数据，不重写整个 UI。

**测试计划：**

- [x] 更新 `server::tests::cockpit_assets_expose_realtime_monitoring_layout`
- [x] 新增断言：HTML 包含端口输入和 phase 容器。
- [x] 新增断言：JS payload 包含 `port` 和 `protocol`。
- [x] 跑 `node --check apps/tracertmonitor/src/assets/app.js`
- [x] 跑 `cargo test -p tracertmonitor --lib server::tests`

**测试关键断言：**

```rust
assert!(index.contains("port-input"));
assert!(index.contains("session-phase"));
assert!(app.contains("protocol"));
assert!(app.contains("port"));
```


**验收标准：**

- 页面能提交目标和端口。
- phase 和 precheck 能从后端响应渲染。
- JS 语法检查通过。

---

### Task 8: 配置栏和 DiagnosticConfig

**目标：** 主输入区保持轻量，把 TTL、轮数、间隔、GeoIP 开关等高级项放入配置栏。

**Files:**

- Modify: `apps/tracertmonitor/src/model.rs`
- Modify: `apps/tracertmonitor/src/server.rs`
- Modify: `apps/tracertmonitor/src/session.rs`
- Modify: `apps/tracertmonitor/src/assets/index.html`
- Modify: `apps/tracertmonitor/src/assets/app.js`
- Modify: `apps/tracertmonitor/src/assets/styles.css`

**做什么：**

- 增加 `DiagnosticConfig`。
- 增加 `DiscoveryConfig`：`max_ttl`、`rounds`、`probes_per_ttl`。
- 增加 `TimingConfig`：`packet_interval_ms`、`window_seconds`。
- 增加 `GeoIpConfig`：online 开关、preset、URL template、本地库路径、timeout、cache、skip private/reserved。
- `server` 从 request 读取 config，缺省值在后端兜底。
- 前端配置栏只负责提交配置，不直接决定诊断结论。

**测试计划：**

- [x] 写失败测试：`model::tests::serializes_diagnostic_config_with_geoip_defaults`
- [x] 写失败测试：`server::tests::start_route_accepts_diagnostic_config`
- [x] 分别跑 `cargo test -p tracertmonitor --lib model::tests` 和 `cargo test -p tracertmonitor --lib server::tests`

**建议默认值：**

```text
max_ttl = 30
rounds = 3
probes_per_ttl = 3
packet_interval_ms = 1000
window_seconds = 60
geoip.online_enabled = false
geoip.timeout_ms = 1500
geoip.cache_ttl_seconds = 86400
geoip.skip_private_or_reserved = true
```


**验收标准：**

- 主输入区不堆高级参数。
- 后端收到完整 config。
- 在线 GeoIP 默认关闭。

---

### Task 9: GeoIP 模型、Resolver 和 Fallback

**目标：** 拓扑节点能显示省、市、运营商/ASN，且在线 GeoIP 失败不会影响诊断。

**Files:**

- Create: `apps/tracertmonitor/src/geoip.rs`
- Modify: `apps/tracertmonitor/src/lib.rs`
- Modify: `apps/tracertmonitor/src/model.rs`
- Modify: `apps/tracertmonitor/src/session.rs`
- Modify: `apps/tracertmonitor/src/export.rs`
- Modify: `apps/tracertmonitor/src/assets/app.js`

**做什么：**

- 增加 `GeoIpInfo`：country/region、province、city、carrier_or_asn、source、status。
- 增加 `GeoIpLookupStatus`：`online_hit`、`online_failed_local_hit`、`local_hit`、`unknown`、`skipped_private_or_reserved`。
- `geoip` 模块提供 resolver 接口。
- 在线 provider 默认关闭，只在用户显式开启时请求 URL template。
- 内网、保留地址默认跳过在线查询。
- 在线失败后 fallback 本地库；本地也失败则 unknown。
- GeoIP 是展示证据，不参与 path id。

**测试计划：**

- [x] 写失败测试：`geoip::tests::skips_private_or_reserved_addresses`
- [x] 写失败测试：`geoip::tests::online_failure_falls_back_to_local_hit`
- [x] 写失败测试：`session::tests::session_attaches_geoip_to_known_hops`
- [x] 跑 GeoIP/session 聚焦测试，并跑 `cargo test -p tracertmonitor --lib`

**测试关键断言：**

```rust
assert_eq!(GeoIpLookupStatus::SkippedPrivateOrReserved, private_result.status);
assert_eq!(GeoIpLookupStatus::OnlineFailedLocalHit, fallback_result.status);
assert_eq!(Some("广东省"), hop_geoip.province.as_deref());
```


**验收标准：**

- 不开启在线 GeoIP 时不请求第三方。
- 在线失败不会让诊断失败。
- API 和导出都能看到 GeoIP 来源和状态。

---

### Task 10: 端到端验证和文档收口

**状态：** Task 10 已完成（2026-07-07）。

**目标：** 确认 V1 诊断链路能从 Web 输入走到 session、probe、analyzer、export，并把文档更新到真实状态。

**Files:**

- Modify: `doc/current-progress.zh-CN.md`
- Modify: `doc/modules.zh-CN.md`
- Modify: `doc/communication.zh-CN.md`
- Modify: `doc/business-flow.zh-CN.md` if behavior changes
- Modify: `docs/superpowers/plans/2026-07-05-v1-diagnostic-flow.zh-CN.md`

**做什么：**

- 更新当前进度，标记已完成任务。
- 更新模块设计，补充 `probe/session/geoip/config` 的真实边界。
- 更新通信文档，补充 start request 和 session snapshot 的最终 JSON 形状。
- 更新实施计划 checkbox。
- 做一次端到端手工冒烟测试。

**验证命令：**

```powershell
node --check apps/tracertmonitor/src/assets/app.js
cargo test -p tracertmonitor --lib
cargo run -p tracertmonitor
```

**手工验证步骤：**

- 打开本地驾驶舱 URL。
- 输入 `www.ctyun.cn`。
- 端口输入 `443`。
- 点击开始诊断。
- 确认显示 precheck、discovering、monitoring 状态。
- 确认导出 JSON 包含 target、protocol、port、prechecks、paths、observations、events。
- 如果开启 GeoIP，确认节点详情显示省、市、运营商或 unknown/fallback 状态。



**2026-07-07 验证记录：**

- `node --check apps/tracertmonitor/src/assets/app.js` 通过。
- `cargo test -p tracertmonitor --lib` 通过，41 个测试全部成功。
- `cargo run -p tracertmonitor` 可启动并打印本地驾驶舱 URL。
- 本地 HTTP 冒烟：`GET /`、`POST /api/session/start`、`GET /export/session.json` 均返回 200。
- 冒烟目标：`www.ctyun.cn`、`tcp`、`443`；响应 phase 为 `monitoring`，导出 JSON 包含 target、prechecks、paths、observations、events。

**验收标准：**

- `cargo test -p tracertmonitor --lib` 全部通过。
- JS 语法检查通过。
- Web 驾驶舱能完成一次 V1 冒烟流程。
- `doc/` 里的长期文档与当前代码状态一致。

---

## 执行建议

推荐一次只推进一个 task。每个 task 都按这个节奏做：

```text
写失败测试 -> 确认失败原因正确 -> 最小实现 -> 聚焦测试通过 -> 全量库测试通过 -> 更新对应文档/checkbox
```

如果中途发现真实 Trippy-backed probe 接入复杂度过高，先保留系统 `tracert/traceroute` fallback，把 `ProbeEngine` 接口和 session/server/export 流程跑通。V1 的重点是现场诊断流程和证据闭环，不是一步吃完所有底层探测能力。


