const DEFAULT_PACKET_INTERVAL_MS = 2500;
const TARGET_PRESETS = [
  { label: "百度", value: "baidu.com" },
  { label: "阿里 DNS", value: "223.5.5.5" },
  { label: "天翼云", value: "www.ctyun.cn" },
  { label: "湖南电信", value: "hn.189.cn" },
];
const MAX_LIVE_AGE_MS = 45 * 60 * 1000;
const CHART_WIDTH = 520;
const CHART_HEIGHT = 168;
const CHART_PAD = { left: 56, right: 24, top: 22, bottom: 42 };

const state = {
  session: null,
  baseObservations: [],
  liveObservations: [],
  templatesByPath: new Map(),
  activePaths: [],
  selectedPathId: null,
  topologyMode: "paths",
  windowMode: "full",
  customStart: null,
  customEnd: null,
  zoom: 1,
  panX: 0,
  panY: 0,
  dragging: false,
  dragStart: null,
  liveCursor: 0,
  liveTimer: null,
  targetInput: "",
  packetIntervalMs: DEFAULT_PACKET_INTERVAL_MS,
};

async function boot() {
  const response = await fetch("/api/session");
  state.session = await response.json();
  state.baseObservations = [...state.session.observations].sort(compareObservedAt);
  state.liveObservations = state.baseObservations.map((observation) => ({ ...observation }));
  state.templatesByPath = groupObservationsByPath(state.baseObservations);
  state.selectedPathId = state.session.paths[0]?.id ?? null;
  state.targetInput = state.session.target.input;
  seedCustomWindowInputs();
  setupMonitorControls();
  setupTopologyControls();
  render();
  startLiveLoop();
}

function startLiveLoop() {
  restartLiveLoop();
}

function restartLiveLoop() {
  if (state.liveTimer) clearInterval(state.liveTimer);
  state.liveTimer = setInterval(() => {
    appendLiveSamples();
    render();
  }, state.packetIntervalMs);
}

function setupMonitorControls() {
  const targetInput = document.querySelector("#target-input");
  targetInput.value = state.targetInput;
  document.querySelector("#target-apply").addEventListener("click", applyTargetInput);
  targetInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") applyTargetInput();
  });

  const presetRoot = document.querySelector("#target-presets");
  presetRoot.innerHTML = "";
  for (const preset of TARGET_PRESETS) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = preset.label;
    button.title = preset.value;
    button.dataset.target = preset.value;
    button.addEventListener("click", () => applyTargetPreset(preset.value));
    presetRoot.appendChild(button);
  }

  const frequency = document.querySelector("#packet-frequency");
  const custom = document.querySelector("#packet-frequency-custom");
  frequency.value = String(DEFAULT_PACKET_INTERVAL_MS);
  frequency.addEventListener("change", () => applyPacketFrequency());
  custom.addEventListener("change", () => applyPacketFrequency());
  custom.addEventListener("input", () => applyPacketFrequency(false));
  renderMonitorControls();
}

function setupTopologyControls() {
  document.querySelectorAll("[data-topology-mode]").forEach((button) => {
    button.addEventListener("click", () => setTopologyMode(button.dataset.topologyMode));
  });
  renderTopologyModeControls();
}

function setTopologyMode(mode) {
  if (!["paths", "merged"].includes(mode) || state.topologyMode === mode) return;
  state.topologyMode = mode;
  renderTopologyModeControls();
  renderTopology();
}

function renderTopologyModeControls() {
  document.querySelectorAll("[data-topology-mode]").forEach((button) => {
    const active = button.dataset.topologyMode === state.topologyMode;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", active ? "true" : "false");
  });
}

function applyTargetInput() {
  const value = document.querySelector("#target-input").value.trim();
  if (!value) return;
  state.targetInput = value;
  renderMonitorControls();
  render();
}

function applyTargetPreset(value) {
  state.targetInput = value;
  document.querySelector("#target-input").value = value;
  renderMonitorControls();
  render();
}

function applyPacketFrequency(restart = true) {
  const frequency = document.querySelector("#packet-frequency");
  const custom = document.querySelector("#packet-frequency-custom");
  const customSelected = frequency.value === "custom";
  custom.hidden = !customSelected;
  const seconds = customSelected ? Number(custom.value) : Number(frequency.value) / 1000;
  const safeSeconds = clamp(Number.isFinite(seconds) ? seconds : 2.5, 0.2, 60);
  state.packetIntervalMs = Math.round(safeSeconds * 1000);
  if (customSelected && String(safeSeconds) !== custom.value) custom.value = safeSeconds.toString();
  renderMonitorControls();
  if (restart) restartLiveLoop();
}

function renderMonitorControls() {
  document.querySelectorAll("#target-presets button").forEach((button) => {
    button.classList.toggle("active", button.dataset.target === state.targetInput);
  });
  document.querySelector("#packet-frequency-label").textContent = `当前 ${packetFrequencyText(state.packetIntervalMs)}`;
  document.querySelector("#live-state").textContent = `实时监测中 · ${packetFrequencyText(state.packetIntervalMs)}`;
}

function packetFrequencyText(intervalMs) {
  const seconds = intervalMs / 1000;
  return `${Number.isInteger(seconds) ? seconds.toFixed(0) : seconds.toFixed(1)} 秒 / 包`;
}

function appendLiveSamples() {
  const now = new Date();
  state.session.paths.forEach((path, pathIndex) => {
    const templates = state.templatesByPath.get(path.id) ?? state.baseObservations;
    if (!templates.length) return;
    const template = templates[state.liveCursor % templates.length];
    const wave = Math.sin((state.liveCursor + pathIndex) / 4) * 6;
    const pressure = Math.cos((state.liveCursor + pathIndex * 3) / 7) * 3;
    const rtt = template.rtt_ms === null
      ? null
      : Math.max(1, Number((template.rtt_ms + wave + pressure).toFixed(1)));
    const jitter = template.jitter_ms === null
      ? null
      : Math.max(0, Number((template.jitter_ms + Math.abs(wave / 3)).toFixed(1)));

    state.liveObservations.push({
      ...template,
      observed_at: new Date(now.getTime() + pathIndex * 180).toISOString(),
      rtt_ms: rtt,
      jitter_ms: jitter,
    });
  });

  state.liveCursor += 1;
  const cutoff = Date.now() - MAX_LIVE_AGE_MS;
  state.liveObservations = state.liveObservations.filter(
    (observation) => new Date(observation.observed_at).getTime() >= cutoff,
  );
}

function render() {
  state.activePaths = pathsForActiveWindow();
  if (!state.activePaths.some((path) => path.id === state.selectedPathId)) {
    state.selectedPathId = state.activePaths[0]?.id ?? null;
  }
  document.querySelector("#target").textContent = currentTargetLabel();
  document.querySelector("#sample-count").textContent = state.liveObservations.length.toString();
  document.querySelector("#refresh-at").textContent = formatTime(latestObservationTime());
  renderSuspicions();
  renderEvents();
  renderTopology();
  renderDetails();
  renderLanes();
}

function currentTargetLabel() {
  const resolved = state.session.target.resolved.join(", ");
  if (state.targetInput === state.session.target.input) {
    return `${state.session.target.input} -> ${resolved}`;
  }
  return `${state.targetInput} -> demo baseline ${state.session.target.input} (${resolved})`;
}

function pathsForActiveWindow() {
  const range = currentWindowRange();
  const observations = state.liveObservations.filter((observation) => {
    const observedAt = new Date(observation.observed_at);
    return observedAt >= range.start && observedAt <= range.end;
  });
  const byPath = groupObservationsByPath(observations);
  const total = Math.max(1, observations.length);

  return state.session.paths.map((path) => {
    const hits = byPath.get(path.id) ?? [];
    const metrics = metricsForHits(path, hits, total);
    return {
      ...path,
      metrics,
      suspicion: deriveSuspicion(path, metrics),
      live_samples: hits,
    };
  });
}

function metricsForHits(path, hits, total) {
  const lost = hits.filter((hit) => hit.lost).length;
  const rtts = hits.map((hit) => hit.rtt_ms).filter((value) => value !== null);
  const jitters = hits.map((hit) => hit.jitter_ms).filter((value) => value !== null);
  return {
    ...path.metrics,
    hit_count: hits.length,
    sample_count: hits.length,
    window_share_pct: hits.length / total * 100,
    loss_pct: lost / Math.max(1, hits.length) * 100,
    avg_rtt_ms: average(rtts),
    avg_jitter_ms: average(jitters),
  };
}

function deriveSuspicion(path, metrics) {
  if (metrics.loss_pct >= 12) {
    return {
      severity: "critical",
      reason: `当前窗口丢包率 ${metrics.loss_pct.toFixed(1)}%，优先排查该路径`,
    };
  }
  if (metrics.avg_rtt_ms >= 95) {
    return {
      severity: "warning",
      reason: `当前窗口平均 RTT ${metrics.avg_rtt_ms.toFixed(1)}ms，存在高延迟迹象`,
    };
  }
  return path.suspicion && metrics.hit_count > 0 ? path.suspicion : null;
}

function currentWindowRange() {
  const observations = state.liveObservations.length ? state.liveObservations : state.baseObservations;
  const times = observations.map((observation) => new Date(observation.observed_at).getTime());
  const first = new Date(Math.min(...times));
  const last = new Date(Math.max(...times));
  if (state.windowMode === "1m") return { start: new Date(last.getTime() - 60_000), end: last };
  if (state.windowMode === "5m") return { start: new Date(last.getTime() - 300_000), end: last };
  if (state.windowMode === "custom" && state.customStart && state.customEnd) {
    return { start: new Date(state.customStart), end: new Date(state.customEnd) };
  }
  return { start: first, end: last };
}

function latestObservationTime() {
  const observations = state.liveObservations.length ? state.liveObservations : state.baseObservations;
  const latest = observations.reduce((max, observation) => {
    const time = new Date(observation.observed_at).getTime();
    return Math.max(max, time);
  }, 0);
  return latest ? new Date(latest).toISOString() : new Date().toISOString();
}

function average(values) {
  if (!values.length) return 0;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function groupObservationsByPath(observations) {
  const byPath = new Map();
  for (const observation of observations) {
    if (!byPath.has(observation.path_id)) byPath.set(observation.path_id, []);
    byPath.get(observation.path_id).push(observation);
  }
  for (const hits of byPath.values()) hits.sort(compareObservedAt);
  return byPath;
}

function compareObservedAt(left, right) {
  return new Date(left.observed_at) - new Date(right.observed_at);
}

function renderSuspicions() {
  const root = document.querySelector("#suspicions");
  root.innerHTML = "";
  const suspicious = state.activePaths.filter((path) => path.suspicion);
  if (!suspicious.length) {
    root.appendChild(emptyItem("当前窗口未标记疑似路径"));
    return;
  }
  for (const path of suspicious) {
    const item = document.createElement("button");
    item.className = "suspicion";
    item.type = "button";
    item.innerHTML = `<strong>${path.label}</strong><span>${path.suspicion.reason}</span>`;
    item.addEventListener("click", () => {
      state.selectedPathId = path.id;
      render();
    });
    root.appendChild(item);
  }
}

function renderEvents() {
  const root = document.querySelector("#events");
  root.innerHTML = "";
  for (const event of state.session.events.slice(0, 5)) {
    const item = document.createElement("div");
    item.className = "event-item";
    item.innerHTML = `<strong>${event.kind}</strong><span>${formatTime(event.at)} ${event.message}</span>`;
    root.appendChild(item);
  }
}

function renderTopology() {
  const root = document.querySelector("#topology");
  root.innerHTML = "";
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `${-state.panX} ${-state.panY} ${980 / state.zoom} ${430 / state.zoom}`);
  svg.classList.add("topology-svg");
  svg.addEventListener("wheel", (event) => {
    event.preventDefault();
    state.zoom = clamp(state.zoom + (event.deltaY < 0 ? 0.12 : -0.12), 0.5, 3);
    renderTopology();
  }, { passive: false });
  svg.addEventListener("pointerdown", (event) => {
    state.dragging = true;
    state.dragStart = { x: event.clientX, y: event.clientY, panX: state.panX, panY: state.panY };
    svg.setPointerCapture(event.pointerId);
  });
  svg.addEventListener("pointerup", () => {
    state.dragging = false;
  });
  svg.addEventListener("pointermove", (event) => {
    if (!state.dragging || !state.dragStart) return;
    state.panX = state.dragStart.panX - (event.clientX - state.dragStart.x) / state.zoom;
    state.panY = state.dragStart.panY - (event.clientY - state.dragStart.y) / state.zoom;
    renderTopology();
  });

  if (state.topologyMode === "merged") {
    drawMergedTopology(svg);
  } else {
    state.activePaths.forEach((path, pathIndex) => drawPath(svg, path, pathIndex));
  }
  root.appendChild(svg);
}

function drawMergedTopology(svg) {
  const topology = buildMergedTopology(state.activePaths);
  drawMergedLegend(svg, topology);
  for (const edge of topology.edges) drawMergedEdge(svg, edge);
  for (const node of topology.nodes) drawMergedNode(svg, node);
}

function buildMergedTopology(paths) {
  const nodesByKey = new Map();
  const edgesByKey = new Map();

  for (const path of paths) {
    const pathNodes = path.hops.map((hop) => {
      const key = topologyNodeKey(hop);
      let node = nodesByKey.get(key);
      if (!node) {
        node = {
          key,
          hop,
          pathIds: new Set(),
          pathRefs: [],
          ttlTotal: 0,
          ttlCount: 0,
          layer: hop.ttl,
          x: 0,
          y: 0,
        };
        nodesByKey.set(key, node);
      }
      node.pathIds.add(path.id);
      if (!node.pathRefs.some((item) => item.id === path.id)) node.pathRefs.push(path);
      node.ttlTotal += hop.ttl;
      node.ttlCount += 1;
      return { node, hop };
    });

    for (let index = 0; index < pathNodes.length - 1; index += 1) {
      const from = pathNodes[index];
      const to = pathNodes[index + 1];
      if (from.node.key === to.node.key) continue;
      const edgeKey = `${from.node.key}->${to.node.key}`;
      let edge = edgesByKey.get(edgeKey);
      if (!edge) {
        edge = {
          from: from.node,
          to: to.node,
          pathIds: new Set(),
          pathRefs: [],
          segments: [],
        };
        edgesByKey.set(edgeKey, edge);
      }
      edge.pathIds.add(path.id);
      if (!edge.pathRefs.some((item) => item.id === path.id)) edge.pathRefs.push(path);
      edge.segments.push({ path, fromHop: from.hop, toHop: to.hop });
    }
  }

  const nodes = [...nodesByKey.values()];
  for (const node of nodes) {
    node.layer = Math.max(1, Math.round(node.ttlTotal / Math.max(1, node.ttlCount)));
  }
  layoutMergedNodes(nodes);
  return { nodes, edges: [...edgesByKey.values()], paths };
}

function layoutMergedNodes(nodes) {
  const layers = new Map();
  for (const node of nodes) {
    if (!layers.has(node.layer)) layers.set(node.layer, []);
    layers.get(node.layer).push(node);
  }

  for (const layer of [...layers.keys()].sort((left, right) => left - right)) {
    const layerNodes = layers.get(layer).sort(compareMergedNodes);
    const spacing = layerNodes.length <= 1 ? 0 : clamp(300 / (layerNodes.length - 1), 58, 92);
    layerNodes.forEach((node, index) => {
      node.x = 96 + (layer - 1) * 145;
      node.y = 215 + (index - (layerNodes.length - 1) / 2) * spacing;
    });
  }
}

function compareMergedNodes(left, right) {
  const labelOrder = mergedNodeLabel(left).localeCompare(mergedNodeLabel(right));
  if (labelOrder !== 0) return labelOrder;
  return left.key.localeCompare(right.key);
}

function topologyNodeKey(hop) {
  if (hop.node.kind === "unknown") return `unknown:${hop.ttl}`;
  return `${hop.node.kind}:${nodeAddress(hop.node)}`;
}

function drawMergedEdge(svg, edge) {
  const selected = edge.pathIds.has(state.selectedPathId);
  const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
  line.setAttribute("x1", edge.from.x);
  line.setAttribute("y1", edge.from.y);
  line.setAttribute("x2", edge.to.x);
  line.setAttribute("y2", edge.to.y);
  line.setAttribute("class", `edge merged-edge ${selected ? "selected" : ""} ${edge.pathIds.size > 1 ? "shared" : "single"}`);

  const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
  title.textContent = edge.pathRefs
    .map((path) => `${path.label} ${path.metrics.window_share_pct.toFixed(1)}%`)
    .join(" / ");
  line.appendChild(title);
  svg.appendChild(line);

  const segment = edge.segments.find((item) => item.path.id === state.selectedPathId) ?? edge.segments[0];
  svg.appendChild(edgeLatencyLabel(
    segment.path,
    { x: edge.from.x, y: edge.from.y, hop: segment.fromHop },
    { x: edge.to.x, y: edge.to.y, hop: segment.toHop },
    selected,
  ));
}

function drawMergedNode(svg, node) {
  const selected = node.pathIds.has(state.selectedPathId);
  const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
  group.setAttribute("class", `node merged-node ${node.hop.node.kind === "unknown" ? "unknown" : ""} ${selected ? "selected" : ""}`);
  group.addEventListener("click", () => {
    state.selectedPathId = preferredMergedPathId(node);
    render();
  });
  group.addEventListener("dblclick", () => {
    state.panX = node.x - 360;
    state.panY = node.y - 160;
    state.zoom = 1.4;
    renderTopology();
  });

  const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
  title.textContent = node.pathRefs
    .map((path) => `${path.label} ${path.metrics.window_share_pct.toFixed(1)}%`)
    .join(" / ");
  group.appendChild(title);

  const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  circle.setAttribute("cx", node.x);
  circle.setAttribute("cy", node.y);
  circle.setAttribute("r", selected ? 26 : Math.min(25, 17 + node.pathIds.size * 2));
  group.appendChild(circle);

  const ttl = document.createElementNS("http://www.w3.org/2000/svg", "text");
  ttl.setAttribute("x", node.x);
  ttl.setAttribute("y", node.y + 5);
  ttl.setAttribute("class", "ttl-label");
  ttl.textContent = node.hop.ttl;
  group.appendChild(ttl);

  const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
  label.setAttribute("x", node.x);
  label.setAttribute("y", node.y + 45);
  label.textContent = mergedNodeLabel(node);
  group.appendChild(label);

  const metadata = document.createElementNS("http://www.w3.org/2000/svg", "text");
  metadata.setAttribute("x", node.x);
  metadata.setAttribute("y", node.y + 60);
  metadata.setAttribute("class", "node-metadata");
  metadata.textContent = `${node.pathIds.size} 条路径`;
  group.appendChild(metadata);

  svg.appendChild(group);
}

function drawMergedLegend(svg, topology) {
  const title = document.createElementNS("http://www.w3.org/2000/svg", "text");
  title.setAttribute("x", 34);
  title.setAttribute("y", 28);
  title.setAttribute("class", "merged-legend-title");
  title.textContent = `汇聚拓扑 · ${topology.paths.length} 条路径`;
  svg.appendChild(title);

  topology.paths.forEach((path, index) => {
    const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
    label.setAttribute("x", 34);
    label.setAttribute("y", 52 + index * 18);
    label.setAttribute("class", path.id === state.selectedPathId ? "merged-legend selected" : "merged-legend");
    label.textContent = `${path.label} ${path.metrics.window_share_pct.toFixed(1)}%`;
    label.addEventListener("click", () => {
      state.selectedPathId = path.id;
      render();
    });
    svg.appendChild(label);
  });
}

function preferredMergedPathId(node) {
  const selected = node.pathRefs.find((path) => path.id === state.selectedPathId);
  return selected?.id ?? node.pathRefs[0]?.id ?? state.selectedPathId;
}

function mergedNodeLabel(node) {
  return node.hop.node.kind === "unknown" ? `TTL ${node.hop.ttl} *` : nodeAddress(node.hop.node);
}

function drawPath(svg, path, pathIndex) {
  const selected = path.id === state.selectedPathId;
  const y = 64 + pathIndex * 100;
  const points = path.hops.map((hop, index) => ({ x: 118 + index * 190, y, hop }));
  for (let i = 0; i < points.length - 1; i += 1) {
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", points[i].x);
    line.setAttribute("y1", points[i].y);
    line.setAttribute("x2", points[i + 1].x);
    line.setAttribute("y2", points[i + 1].y);
    line.setAttribute("class", selected ? "edge selected" : "edge");
    svg.appendChild(line);
    svg.appendChild(edgeLatencyLabel(path, points[i], points[i + 1], selected));
  }

  for (const point of points) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.setAttribute("class", `node ${point.hop.node.kind === "unknown" ? "unknown" : ""}`);
    group.addEventListener("click", () => {
      state.selectedPathId = path.id;
      render();
    });
    group.addEventListener("dblclick", () => {
      state.panX = point.x - 360;
      state.panY = point.y - 160;
      state.zoom = 1.4;
      renderTopology();
    });

    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    circle.setAttribute("cx", point.x);
    circle.setAttribute("cy", point.y);
    circle.setAttribute("r", selected ? 24 : 18);
    group.appendChild(circle);

    const ttl = document.createElementNS("http://www.w3.org/2000/svg", "text");
    ttl.setAttribute("x", point.x);
    ttl.setAttribute("y", point.y + 5);
    ttl.setAttribute("class", "ttl-label");
    ttl.textContent = point.hop.ttl;
    group.appendChild(ttl);

    const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
    label.setAttribute("x", point.x);
    label.setAttribute("y", point.y + 45);
    label.textContent = point.hop.node.kind === "unknown" ? `TTL ${point.hop.ttl} *` : nodeAddress(point.hop.node);
    group.appendChild(label);
    svg.appendChild(group);
  }

  const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
  label.setAttribute("x", 42);
  label.setAttribute("y", y + 5);
  label.setAttribute("class", selected ? "path-label selected" : "path-label");
  label.textContent = `${path.label} ${path.metrics.window_share_pct.toFixed(1)}%`;
  svg.appendChild(label);
}

function edgeLatencyLabel(path, fromPoint, toPoint, selected) {
  const latency = edgeLatency(path, fromPoint.hop, toPoint.hop);
  const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
  const severity = edgeLatencySeverity(path, latency);
  group.setAttribute("class", `edge-latency ${selected ? "selected" : ""} ${latency.estimated ? "estimated" : "measured"} ${severity}`);
  group.setAttribute("transform", `translate(${((fromPoint.x + toPoint.x) / 2).toFixed(1)} ${((fromPoint.y + toPoint.y) / 2 - 14).toFixed(1)})`);

  const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
  title.textContent = latency.estimated
    ? "当前为窗口估算，接入真实逐跳探测后显示节点间 RTT"
    : "当前显示该 hop 的平均 RTT";
  group.appendChild(title);

  const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  rect.setAttribute("x", -31);
  rect.setAttribute("y", -13);
  rect.setAttribute("width", 62);
  rect.setAttribute("height", 24);
  rect.setAttribute("rx", 12);
  group.appendChild(rect);

  const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
  text.setAttribute("class", "edge-latency-text");
  text.setAttribute("x", 0);
  text.setAttribute("y", 4);
  text.textContent = latency.label;
  group.appendChild(text);
  return group;
}

function edgeLatency(path, fromHop, toHop) {
  const fromAvg = fromHop.metrics?.avg_ms ?? 0;
  const toAvg = toHop.metrics?.avg_ms ?? 0;
  if (toAvg > 0 && fromAvg > 0 && toAvg >= fromAvg) {
    return { label: `${(toAvg - fromAvg).toFixed(1)}ms`, value: toAvg - fromAvg, estimated: false };
  }
  if (toAvg > 0) {
    return { label: `${toAvg.toFixed(1)}ms`, value: toAvg, estimated: false };
  }
  const hopCount = Math.max(1, path.hops.length - 1);
  const estimate = path.metrics.avg_rtt_ms > 0 ? path.metrics.avg_rtt_ms / hopCount : null;
  if (estimate === null) return { label: "--ms", value: 0, estimated: true };
  return { label: `~${estimate.toFixed(1)}ms`, value: estimate, estimated: true };
}

function edgeLatencySeverity(path, latency) {
  if (path.metrics.loss_pct >= 12) return "bad";
  if (latency.value >= 80 || path.metrics.avg_rtt_ms >= 95) return "warn";
  return "normal";
}

function renderDetails() {
  const path = state.activePaths.find((item) => item.id === state.selectedPathId);
  const root = document.querySelector("#details");
  if (!path) {
    root.textContent = "未选择路径";
    return;
  }
  root.innerHTML = `
    <dl>
      <dt>路径</dt><dd>${path.label}</dd>
      <dt>窗口占比</dt><dd>${path.metrics.window_share_pct.toFixed(1)}%</dd>
      <dt>命中次数</dt><dd>${path.metrics.hit_count}</dd>
      <dt>平均 RTT</dt><dd>${path.metrics.avg_rtt_ms.toFixed(1)}ms</dd>
      <dt>丢包率</dt><dd>${path.metrics.loss_pct.toFixed(1)}%</dd>
      <dt>Jitter</dt><dd>${path.metrics.avg_jitter_ms.toFixed(1)}ms</dd>
      <dt>判断</dt><dd>${path.suspicion ? path.suspicion.reason : "未标记"}</dd>
    </dl>
  `;
}

function renderLanes() {
  const root = document.querySelector("#lanes");
  root.innerHTML = "";
  for (const path of state.activePaths) {
    const samples = observationsForPath(path);
    const card = document.createElement("article");
    card.className = path.id === state.selectedPathId ? "lane-card selected" : "lane-card";
    card.tabIndex = 0;
    card.addEventListener("click", () => {
      state.selectedPathId = path.id;
      render();
    });

    const top = document.createElement("div");
    top.className = "lane-top";
    top.innerHTML = `
      <div class="lane-title">
        <strong>${path.label}</strong>
        <span>${samples.length} samples in current window</span>
      </div>
      <span class="${healthClass(path)}">${healthText(path)}</span>
    `;
    card.appendChild(top);

    const metrics = document.createElement("div");
    metrics.className = "metrics";
    metrics.innerHTML = `
      <div class="metric"><span>Share</span><strong>${path.metrics.window_share_pct.toFixed(1)}%</strong></div>
      <div class="metric"><span>RTT</span><strong>${path.metrics.avg_rtt_ms.toFixed(1)}ms</strong></div>
      <div class="metric"><span>Loss</span><strong>${path.metrics.loss_pct.toFixed(1)}%</strong></div>
      <div class="metric"><span>Jitter</span><strong>${path.metrics.avg_jitter_ms.toFixed(1)}ms</strong></div>
    `;
    card.appendChild(metrics);

    const shell = document.createElement("div");
    shell.className = "chart-shell";
    shell.appendChild(renderPathChart(path, samples));
    card.appendChild(shell);

    const caption = document.createElement("div");
    caption.className = "chart-caption";
    caption.innerHTML = `<span>${formatWindowStart(samples)}</span><span>${formatWindowEnd(samples)}</span>`;
    card.appendChild(caption);

    root.appendChild(card);
  }
}

function renderPathChart(path, samples) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`);
  svg.classList.add("path-chart");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", `${path.label} RTT realtime chart`);

  if (!samples.length) {
    drawChartGrid(svg, 100, samples);
    const empty = document.createElementNS("http://www.w3.org/2000/svg", "text");
    empty.setAttribute("x", CHART_WIDTH / 2);
    empty.setAttribute("y", CHART_HEIGHT / 2);
    empty.setAttribute("text-anchor", "middle");
    empty.setAttribute("fill", "#617066");
    empty.textContent = "当前窗口暂无样本";
    svg.appendChild(empty);
    return svg;
  }

  const maxRtt = Math.max(20, ...samples.map((sample) => sample.rtt_ms ?? 0)) * 1.18;
  const points = samples.map((sample, index) => ({
    sample,
    x: scaleX(index, samples.length),
    y: sample.rtt_ms === null ? null : scaleY(sample.rtt_ms, maxRtt),
  }));

  createChartDefs(svg, path.id);
  drawChartGrid(svg, maxRtt, samples);

  const fill = document.createElementNS("http://www.w3.org/2000/svg", "path");
  fill.setAttribute("class", "chart-fill");
  fill.setAttribute("d", areaPath(points));
  fill.setAttribute("fill", `url(#${chartGradientId(path.id)})`);
  svg.appendChild(fill);

  const line = document.createElementNS("http://www.w3.org/2000/svg", "path");
  line.setAttribute("class", "chart-line");
  line.setAttribute("d", linePath(points));
  svg.appendChild(line);

  for (const point of points) {
    const dot = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    if (point.sample.lost) {
      dot.setAttribute("class", "loss-dot");
      dot.setAttribute("cx", point.x);
      dot.setAttribute("cy", CHART_PAD.top + 9);
      dot.setAttribute("r", 4.8);
    } else if (point.y !== null) {
      const elevated = point.sample.rtt_ms >= 95 ? " elevated" : "";
      dot.setAttribute("class", `sample-dot${elevated}`);
      dot.setAttribute("cx", point.x);
      dot.setAttribute("cy", point.y);
      dot.setAttribute("r", points.length <= 90 ? 3.2 : 2.4);
    }
    if (dot.getAttribute("class")) svg.appendChild(dot);
  }

  const highlight = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  highlight.setAttribute("class", "highlight-dot is-hidden");
  highlight.setAttribute("r", 6.5);
  svg.appendChild(highlight);

  const tooltip = createChartTooltip();
  svg.appendChild(tooltip);

  points.forEach((point, index) => {
    const target = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    target.setAttribute("class", "hover-target");
    target.setAttribute("cx", point.x);
    target.setAttribute("cy", point.sample.lost ? CHART_PAD.top + 9 : point.y ?? CHART_PAD.top);
    target.setAttribute("r", 10);
    target.addEventListener("mouseenter", () => showChartTooltip(tooltip, highlight, path, point, index, points.length));
    target.addEventListener("mousemove", () => showChartTooltip(tooltip, highlight, path, point, index, points.length));
    target.addEventListener("mouseleave", () => hideChartTooltip(tooltip, highlight));
    svg.appendChild(target);
  });

  svg.addEventListener("mouseleave", () => hideChartTooltip(tooltip, highlight));

  return svg;
}

function createChartDefs(svg, pathId) {
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  const gradient = document.createElementNS("http://www.w3.org/2000/svg", "linearGradient");
  gradient.setAttribute("id", chartGradientId(pathId));
  gradient.setAttribute("x1", "0");
  gradient.setAttribute("x2", "0");
  gradient.setAttribute("y1", "0");
  gradient.setAttribute("y2", "1");

  const top = document.createElementNS("http://www.w3.org/2000/svg", "stop");
  top.setAttribute("offset", "0%");
  top.setAttribute("stop-color", "#235c8f");
  top.setAttribute("stop-opacity", "0.22");
  gradient.appendChild(top);

  const bottom = document.createElementNS("http://www.w3.org/2000/svg", "stop");
  bottom.setAttribute("offset", "100%");
  bottom.setAttribute("stop-color", "#235c8f");
  bottom.setAttribute("stop-opacity", "0.02");
  gradient.appendChild(bottom);

  defs.appendChild(gradient);
  svg.appendChild(defs);
}

function chartGradientId(pathId) {
  return `chart-gradient-${pathId.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
}

function drawChartGrid(svg, maxRtt, samples) {
  const plotBottom = CHART_HEIGHT - CHART_PAD.bottom;
  const plotRight = CHART_WIDTH - CHART_PAD.right;
  const yTicks = [0, maxRtt / 2, maxRtt];

  for (const tick of yTicks) {
    const y = scaleY(tick, maxRtt);
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("class", "chart-grid");
    line.setAttribute("x1", CHART_PAD.left);
    line.setAttribute("x2", plotRight);
    line.setAttribute("y1", y);
    line.setAttribute("y2", y);
    svg.appendChild(line);

    drawAxisLabel(svg, `${Math.round(tick)}ms`, CHART_PAD.left - 9, y + 4, "end");
  }

  drawAxisLine(svg, CHART_PAD.left, CHART_PAD.top, CHART_PAD.left, plotBottom);
  drawAxisLine(svg, CHART_PAD.left, plotBottom, plotRight, plotBottom);
  drawAxisTitle(svg, "RTT(ms)", 12, CHART_PAD.top + 8, "start");

  const xTicks = axisSampleIndexes(samples.length);
  for (const index of xTicks) {
    const x = scaleX(index, Math.max(1, samples.length));
    const tick = document.createElementNS("http://www.w3.org/2000/svg", "line");
    tick.setAttribute("class", "chart-axis tick");
    tick.setAttribute("x1", x);
    tick.setAttribute("x2", x);
    tick.setAttribute("y1", plotBottom);
    tick.setAttribute("y2", plotBottom + 5);
    svg.appendChild(tick);

    const sample = samples[index];
    const anchor = index === 0 ? "start" : index === samples.length - 1 ? "end" : "middle";
    drawAxisLabel(svg, sample ? formatAxisTime(sample.observed_at) : "--", x, plotBottom + 20, anchor);
  }
}

function drawAxisLine(svg, x1, y1, x2, y2) {
  const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
  line.setAttribute("class", "chart-axis");
  line.setAttribute("x1", x1);
  line.setAttribute("x2", x2);
  line.setAttribute("y1", y1);
  line.setAttribute("y2", y2);
  svg.appendChild(line);
}

function drawAxisLabel(svg, text, x, y, anchor) {
  const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
  label.setAttribute("class", "axis-label");
  label.setAttribute("x", x);
  label.setAttribute("y", y);
  label.setAttribute("text-anchor", anchor);
  label.textContent = text;
  svg.appendChild(label);
}

function drawAxisTitle(svg, text, x, y, anchor) {
  const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
  label.setAttribute("class", "axis-title");
  label.setAttribute("x", x);
  label.setAttribute("y", y);
  label.setAttribute("text-anchor", anchor);
  label.textContent = text;
  svg.appendChild(label);
}

function axisSampleIndexes(count) {
  if (count <= 0) return [0];
  if (count === 1) return [0];
  return [...new Set([0, Math.floor((count - 1) / 2), count - 1])];
}

function createChartTooltip() {
  const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
  group.setAttribute("class", "chart-tooltip is-hidden");

  const bubble = document.createElementNS("http://www.w3.org/2000/svg", "path");
  bubble.setAttribute("class", "tooltip-bubble");
  bubble.setAttribute("d", "M8 0H168Q176 0 176 8V78Q176 86 168 86H96L86 98L76 86H8Q0 86 0 78V8Q0 0 8 0Z");
  group.appendChild(bubble);

  const title = document.createElementNS("http://www.w3.org/2000/svg", "text");
  title.setAttribute("class", "tooltip-title");
  title.setAttribute("x", 12);
  title.setAttribute("y", 18);
  group.appendChild(title);

  for (let i = 0; i < 4; i += 1) {
    const row = document.createElementNS("http://www.w3.org/2000/svg", "text");
    row.setAttribute("class", "tooltip-row");
    row.setAttribute("data-row", i.toString());
    row.setAttribute("x", 12);
    row.setAttribute("y", 37 + i * 14);
    group.appendChild(row);
  }

  return group;
}

function showChartTooltip(tooltip, highlight, path, point, index, total) {
  const y = point.sample.lost ? CHART_PAD.top + 9 : point.y ?? CHART_PAD.top;
  const x = point.x;
  const tooltipX = clamp(x + 14, CHART_PAD.left, CHART_WIDTH - 186);
  const tooltipY = clamp(y - 78, 8, CHART_HEIGHT - 110);
  const rows = tooltipRows(path, point.sample, index + 1, total);

  tooltip.querySelector(".tooltip-title").textContent = `${path.label} #${index + 1}`;
  rows.forEach((row, rowIndex) => {
    tooltip.querySelector(`[data-row="${rowIndex}"]`).textContent = row;
  });
  tooltip.setAttribute("transform", `translate(${tooltipX.toFixed(1)} ${tooltipY.toFixed(1)})`);
  tooltip.classList.remove("is-hidden");

  highlight.setAttribute("cx", x);
  highlight.setAttribute("cy", y);
  highlight.classList.remove("is-hidden");
  document.querySelector("#hover-readout").textContent = readoutForSample(path, point.sample, index + 1, total);
}

function hideChartTooltip(tooltip, highlight) {
  tooltip.classList.add("is-hidden");
  highlight.classList.add("is-hidden");
  document.querySelector("#hover-readout").textContent = "移动到路径折线图上查看当前时间点的精确数值";
}

function tooltipRows(path, sample, index, total) {
  const rtt = sample.rtt_ms === null ? "lost" : `${sample.rtt_ms.toFixed(1)}ms`;
  const jitter = sample.jitter_ms === null ? "--" : `${sample.jitter_ms.toFixed(1)}ms`;
  return [
    `${formatTime(sample.observed_at)} | ${index}/${total}`,
    `RTT ${rtt} | Jitter ${jitter}`,
    `Loss ${sample.lost ? "yes" : "no"} | Share ${path.metrics.window_share_pct.toFixed(1)}%`,
    `状态 ${sampleStatus(sample)}`,
  ];
}

function sampleStatus(sample) {
  if (sample.lost) return "丢包";
  if (sample.rtt_ms >= 95) return "高延迟";
  return "正常";
}

function formatAxisTime(value) {
  return new Date(value).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
}

function areaPath(points) {
  const bottom = CHART_HEIGHT - CHART_PAD.bottom;
  const segments = [];
  let current = [];
  for (const point of points) {
    if (point.y === null || point.sample.lost) {
      if (current.length) segments.push(current);
      current = [];
      continue;
    }
    current.push(point);
  }
  if (current.length) segments.push(current);

  return segments.map((segment) => {
    const first = segment[0];
    const last = segment[segment.length - 1];
    const top = segment.map((point, index) => `${index === 0 ? "M" : "L"}${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(" ");
    return `${top} L${last.x.toFixed(1)},${bottom} L${first.x.toFixed(1)},${bottom} Z`;
  }).join(" ");
}
function linePath(points) {
  let d = "";
  let drawing = false;
  for (const point of points) {
    if (point.y === null || point.sample.lost) {
      drawing = false;
      continue;
    }
    d += `${drawing ? "L" : "M"}${point.x.toFixed(1)},${point.y.toFixed(1)} `;
    drawing = true;
  }
  return d.trim();
}

function scaleX(index, count) {
  if (count <= 1) return CHART_PAD.left;
  const width = CHART_WIDTH - CHART_PAD.left - CHART_PAD.right;
  return CHART_PAD.left + index / (count - 1) * width;
}

function scaleY(value, maxRtt) {
  const height = CHART_HEIGHT - CHART_PAD.top - CHART_PAD.bottom;
  return CHART_PAD.top + (1 - value / maxRtt) * height;
}

function observationsForPath(path) {
  const range = currentWindowRange();
  return state.liveObservations
    .filter((observation) => observation.path_id === path.id)
    .filter((observation) => {
      const observedAt = new Date(observation.observed_at);
      return observedAt >= range.start && observedAt <= range.end;
    })
    .sort(compareObservedAt)
    .slice(-120);
}

function readoutForSample(path, sample, index, total) {
  const rtt = sample.rtt_ms === null ? "lost" : `${sample.rtt_ms.toFixed(1)}ms`;
  const jitter = sample.jitter_ms === null ? "--" : `${sample.jitter_ms.toFixed(1)}ms`;
  return `${path.label} | ${index}/${total} | ${formatTime(sample.observed_at)} | RTT ${rtt} | Loss ${sample.lost ? "yes" : "no"} | Jitter ${jitter} | Share ${path.metrics.window_share_pct.toFixed(1)}%`;
}

function healthText(path) {
  if (path.metrics.loss_pct >= 12) return "严重丢包";
  if (path.metrics.avg_rtt_ms >= 95) return "高延迟";
  if (path.suspicion) return "需关注";
  return "稳定";
}

function healthClass(path) {
  if (path.metrics.loss_pct >= 12) return "health-pill bad";
  if (path.metrics.avg_rtt_ms >= 95 || path.suspicion) return "health-pill warn";
  return "health-pill";
}

function formatWindowStart(samples) {
  if (!samples.length) return "--";
  return formatTime(samples[0].observed_at);
}

function formatWindowEnd(samples) {
  if (!samples.length) return "--";
  return formatTime(samples[samples.length - 1].observed_at);
}

function seedCustomWindowInputs() {
  const observations = state.liveObservations.length ? state.liveObservations : state.baseObservations;
  const times = observations.map((observation) => new Date(observation.observed_at).getTime());
  const first = new Date(Math.min(...times));
  const last = new Date(Math.max(...times));
  state.customStart = toLocalInput(first);
  state.customEnd = toLocalInput(last);
  document.querySelector("#window-start").value = state.customStart;
  document.querySelector("#window-end").value = state.customEnd;
}

function toLocalInput(date) {
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}

function nodeAddress(node) {
  if (node.kind === "unknown") return "*";
  return node.hostname || node.ip;
}

function formatTime(value) {
  return new Date(value).toLocaleTimeString([], { hour12: false });
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function emptyItem(text) {
  const item = document.createElement("div");
  item.className = "empty-item";
  item.textContent = text;
  return item;
}

document.querySelector("#zoom-in").addEventListener("click", () => {
  state.zoom = clamp(state.zoom + 0.2, 0.5, 3);
  renderTopology();
});
document.querySelector("#zoom-out").addEventListener("click", () => {
  state.zoom = clamp(state.zoom - 0.2, 0.5, 3);
  renderTopology();
});
document.querySelector("#zoom-reset").addEventListener("click", () => {
  state.zoom = 1;
  state.panX = 0;
  state.panY = 0;
  renderTopology();
});
document.querySelector("#window-select").addEventListener("change", (event) => {
  state.windowMode = event.target.value;
  const custom = state.windowMode === "custom";
  document.querySelector("#window-start").hidden = !custom;
  document.querySelector("#window-end").hidden = !custom;
  render();
});
document.querySelector("#window-start").addEventListener("change", (event) => {
  state.customStart = event.target.value;
  render();
});
document.querySelector("#window-end").addEventListener("change", (event) => {
  state.customEnd = event.target.value;
  render();
});

boot();
