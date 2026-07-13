# 当前进度

## 一句话状态

TracertMonitor 现在已经有一个能运行的本地 Web 驾驶舱、demo 多路径数据、系统 `tracert/traceroute` fallback、V1 目标/协议/端口建模、TCP-first precheck、`ProbeEngine` 边界、`session` 中控、server start API、ECMP/unknown hop 分析、monitor tick、配置栏与 `DiagnosticConfig`、包含 prechecks 和 GeoIP 来源/状态的 JSON/CSV 导出，以及前端 V1 状态、端口输入和 GeoIP 节点展示。

2026-07-07 已完成 Task 10 的端到端验证和文档收口。下一步重点是 Trippy-backed 真实多轮探测、后台定时 monitor tick 和真实 GeoIP provider。

## 进度图

```mermaid
flowchart LR
    A["已完成\n独立 tracertmonitor workspace"] --> B["已完成\nWeb 驾驶舱 demo"]
    B --> C["已完成\n系统 tracert fallback"]
    C --> D["已完成\nV1 model/probe/session/server"]
    D --> E["已完成\nECMP 分析和 monitor tick"]
    E --> F["已完成\nV1 JSON/CSV 导出"]
    F --> G["已完成\n前端接入 V1 状态和端口"]
    G --> H["已完成\n配置栏和 DiagnosticConfig"]
    H --> I["已完成\nGeoIP resolver/fallback"]
    I --> J["已完成\n端到端验证和文档收口"]
    J --> K["下一步\nTrippy-backed probe/后台监测"]
```

## 已经有的内容

| 区域 | 当前状态 |
| --- | --- |
| 独立项目目录 | 已有 `tracertmonitor/` workspace，不再把产品代码混在上游 `trippy/` 目录里。 |
| vendored Trippy | 已把需要的 Trippy crates 放到 `vendor/trippy/`，方便后续复用。 |
| Web 驾驶舱 | 已有内嵌 HTML/CSS/JS 页面，能展示 demo 拓扑、路径监控、导出入口、目标/协议/端口输入、phase、precheck、配置栏和 GeoIP 节点标签。 |
| 目标输入 | 后端 start API 和前端输入区都已支持 target/protocol/port，端口默认 443。 |
| 配置栏 | 前端能提交 TTL、轮数、每 TTL 探测次数、窗口、探测间隔和 GeoIP 配置；后端用 `DiagnosticConfig` 兜底与限幅。 |
| 系统 traceroute fallback | `probe.rs` 可以调用系统 `tracert`/`traceroute` 做一次性探测。 |
| V1 数据模型 | `model.rs` 已增加 `TargetEndpoint`、`ProbeProtocol`、`ReachabilityCheck`、`ProbeRound`、`FlowKey`、`TtlProbeSample`、`ProbeResponse`、`DiagnosticConfig`、`GeoIpInfo`、`GeoIpLookupStatus` 等基础类型。 |
| Session 中控 | `session.rs` 已能通过 `ProbeEngine` 执行 precheck/discovery，多次 `monitor_once` 追加观测并刷新分析，并把 GeoIP 结果挂到已知 hop。 |
| 分析器 | `analyzer.rs` 已能生成稳定 path id、保留 unknown hop、识别 ECMP 多路径、按时间窗口聚合并做基础异常判断；GeoIP 不参与 path id。 |
| 导出 | `export.rs` 已能导出旧 `TraceSession` 和 V1 `DiagnosticSnapshot`，包括 target/protocol/port、prechecks、paths、observations、events、GeoIP 来源/状态和 `prechecks.csv`。 |
| Demo 数据 | `demo.rs` 已有确定性的多路径数据，包含 unknown hop 和疑似异常路径。 |
| 计划文档 | 已有 V1 诊断流程计划，中英文版本都在 `docs/superpowers/plans/`；`doc/next-task-plan.zh-CN.md` 已推进并完成 Task 10。 |

## 最近验证

2026-07-07 完成一次 V1 收口验证：

| 验证项 | 结果 |
| --- | --- |
| `node --check apps/tracertmonitor/src/assets/app.js` | 通过，退出码 0。 |
| `cargo test -p tracertmonitor --lib` | 通过，41 个测试全部成功。 |
| `cargo run -p tracertmonitor` | 可启动，控制台打印本地驾驶舱 URL。 |
| 本地 HTTP 冒烟 | `GET /`、`POST /api/session/start`、`GET /export/session.json` 均返回 200。 |
| 冒烟目标 | `www.ctyun.cn`、`tcp`、`443`。 |
| 冒烟响应 | phase 为 `monitoring`，包含 3 条 precheck、1 条 path、1 条 observation、2 条 event；导出 JSON 包含 target、prechecks、paths、observations、events。 |

## 还没有完成的内容

| 缺口 | 说明 |
| --- | --- |
| Trippy-backed probe | vendored crates 已准备，但主探测引擎仍以系统 fallback 和接口边界为主。 |
| 真实多轮 TTL sweep | 已有接口和分析语义，但还没有真正按多 flow/多轮主动发现 ECMP。 |
| 后台定时器/实时推送 | `session.monitor_once` 已有，但定时调用仍需由 server 或更上层接入；SSE/WebSocket 未做。 |
| 在线/本地 GeoIP provider 适配器 | 当前已有 provider-style resolver 接口和 fallback 语义；默认不访问第三方，真实 HTTP provider 和本地库读取仍需后续接入。 |
| 可视化手工长测 | 已完成本地 HTTP 冒烟和资源校验；后续仍建议在真实现场网络下做更长时间的可视化点击和导出演示。 |

## 当前代码模块状态

| 模块 | 状态 | 下一步 |
| --- | --- | --- |
| `main.rs` | 简单启动 server，加载 demo session，并在控制台打印本地 URL。 | 后续可自动打开浏览器或接入托盘/桌面壳。 |
| `server.rs` | 本地 HTTP、静态资源、API、导出路由已有；start API 已接收 target/protocol/port/config 并交给 `session`。 | 后续接定时 monitor tick 或实时推送。 |
| `probe.rs` | 系统 traceroute fallback、`ProbeEngine` 边界、TCP/DNS/ICMP precheck 证据结构已可用。 | 后续接入真正 Trippy-backed 多轮 discovery/monitoring。 |
| `model.rs` | 基础证据结构、V1 endpoint/precheck/probe round、`DiagnosticConfig` 和 GeoIP 类型已有。 | 后续随真实 provider 补充字段。 |
| `geoip.rs` | 已有 resolver/provider 接口、内网/保留地址跳过、在线失败本地 fallback、unknown 降级语义。 | 后续接真实在线 HTTP provider 和本地数据库读取。 |
| `analyzer.rs` | ECMP 路径识别、unknown hop 保留、窗口占比、基础丢包/延迟异常判断已有。 | 后续随真实 probe 证据扩展更多异常规则。 |
| `export.rs` | V1 snapshot JSON/CSV 导出已有，包含 prechecks、GeoIP 来源/状态和 `prechecks.csv`；旧 CSV 表继续可用。 | 后续把 probe round 证据纳入导出。 |
| `demo.rs` | 多路径 demo 数据可用。 | 保留，用于 UI、测试和离线演示。 |
| `session.rs` | 已接入 `ProbeEngine` 和 GeoIP resolver，支持 start/discovery/monitor tick、snapshot 和事件记录。 | 后续由 server 定时触发 monitor tick。 |

## 推荐下一步顺序

1. 已完成：扩展 `model.rs`，把目标端口、协议、precheck、flow、TTL sample 这些名词建好。
2. 已完成：新增 `session.rs`，让诊断流程有一个中控模块骨架。
3. 已完成：整理 `probe.rs`，保留系统 traceroute fallback，同时定义 TCP-first probe interface。
4. 已完成：让 `session` 接入 `ProbeEngine`，形成真实诊断中控调用链。
5. 已完成：修改 `server.rs`，把 start request 改成 target/protocol/port 并交给 session。
6. 已完成：强化 `analyzer.rs`，围绕 ECMP、多路径、unknown hop、时间窗口完善分析。
7. 已完成：给 `session` 增加 monitor tick，支持持续追加观测并刷新路径占比。
8. 已完成：扩展 `export.rs` 和导出路由，让 JSON/CSV 包含 V1 证据。
9. 已完成：前端接入 V1 状态和端口输入，显示 phase 与 precheck。
10. 已完成：配置栏和 `DiagnosticConfig`。
11. 已完成：GeoIP resolver/fallback、API/导出/前端节点展示。
12. 已完成：端到端验证和文档收口。
13. 下一步：Trippy-backed probe、后台定时 monitor tick、真实 GeoIP provider 和更完整的现场长测。

## 重要提醒

- 当前能跑的东西不要轻易拆坏，系统 traceroute fallback 先保留。
- V1 不追求一步到位接入所有 Trippy 能力，先把流程和数据模型跑通。
- TCP 是主线，ICMP 是辅助证据。
- 中间 hop 不响应不等于链路故障，必须结合后续 hop 和目标响应判断。
- GeoIP 是展示证据，不参与 path id，也不能单独决定链路故障。
- 在线 GeoIP 默认关闭；真实在线 provider 接入后，必须只在用户显式开启时访问第三方。
- 文档现在分两类：
  - `doc/`：人读的长期文档。
  - `docs/superpowers/`：agent 设计稿和实施计划归档。