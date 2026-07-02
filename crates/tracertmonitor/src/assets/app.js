const state = {
  session: null,
  activePaths: [],
  selectedPathId: null,
  windowMode: "full",
  customStart: null,
  customEnd: null,
  zoom: 1,
  panX: 0,
  panY: 0,
  dragging: false,
  dragStart: null,
};

async function boot() {
  const response = await fetch("/api/session");
  state.session = await response.json();
  state.activePaths = state.session.paths;
  state.selectedPathId = state.session.paths[0]?.id ?? null;
  seedCustomWindowInputs();
  render();
}

function render() {
  state.activePaths = pathsForActiveWindow();
  if (!state.activePaths.some((path) => path.id === state.selectedPathId)) {
    state.selectedPathId = state.activePaths[0]?.id ?? null;
  }
  document.querySelector("#target").textContent =
    `${state.session.target.input} -> ${state.session.target.resolved.join(", ")}`;
  renderSuspicions();
  renderEvents();
  renderTopology();
  renderDetails();
  renderLanes();
}

function pathsForActiveWindow() {
  const range = currentWindowRange();
  const observations = state.session.observations.filter((observation) => {
    const observedAt = new Date(observation.observed_at);
    return observedAt >= range.start && observedAt <= range.end;
  });
  const byPath = new Map();
  for (const observation of observations) {
    if (!byPath.has(observation.path_id)) byPath.set(observation.path_id, []);
    byPath.get(observation.path_id).push(observation);
  }
  const total = Math.max(1, observations.length);
  return state.session.paths
    .filter((path) => byPath.has(path.id))
    .map((path) => {
      const hits = byPath.get(path.id);
      const lost = hits.filter((hit) => hit.lost).length;
      return {
        ...path,
        metrics: {
          ...path.metrics,
          hit_count: hits.length,
          sample_count: hits.length,
          window_share_pct: hits.length / total * 100,
          loss_pct: lost / Math.max(1, hits.length) * 100,
          avg_rtt_ms: average(hits.map((hit) => hit.rtt_ms).filter((value) => value !== null)),
          avg_jitter_ms: average(hits.map((hit) => hit.jitter_ms).filter((value) => value !== null)),
        },
      };
    });
}

function currentWindowRange() {
  const times = state.session.observations.map((observation) => new Date(observation.observed_at));
  const first = new Date(Math.min(...times));
  const last = new Date(Math.max(...times));
  if (state.windowMode === "1m") return { start: new Date(last.getTime() - 60_000), end: last };
  if (state.windowMode === "5m") return { start: new Date(last.getTime() - 300_000), end: last };
  if (state.windowMode === "custom" && state.customStart && state.customEnd) {
    return { start: new Date(state.customStart), end: new Date(state.customEnd) };
  }
  return { start: first, end: last };
}

function average(values) {
  if (!values.length) return 0;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
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

  state.activePaths.forEach((path, pathIndex) => drawPath(svg, path, pathIndex));
  root.appendChild(svg);
}

function drawPath(svg, path, pathIndex) {
  const selected = path.id === state.selectedPathId;
  const y = 72 + pathIndex * 112;
  const points = path.hops.map((hop, index) => ({ x: 118 + index * 190, y, hop }));
  for (let i = 0; i < points.length - 1; i += 1) {
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", points[i].x);
    line.setAttribute("y1", points[i].y);
    line.setAttribute("x2", points[i + 1].x);
    line.setAttribute("y2", points[i + 1].y);
    line.setAttribute("class", selected ? "edge selected" : "edge");
    svg.appendChild(line);
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
    const lane = document.createElement("button");
    lane.type = "button";
    lane.className = path.id === state.selectedPathId ? "lane selected" : "lane";
    lane.innerHTML = `
      <strong>${path.label}</strong>
      <span>Share ${path.metrics.window_share_pct.toFixed(1)}%</span>
      <span>RTT ${path.metrics.avg_rtt_ms.toFixed(1)}ms</span>
      <span>Loss ${path.metrics.loss_pct.toFixed(1)}%</span>
      <span>Jitter ${path.metrics.avg_jitter_ms.toFixed(1)}ms</span>
    `;
    lane.addEventListener("click", () => {
      state.selectedPathId = path.id;
      render();
    });
    lane.addEventListener("mousemove", () => {
      document.querySelector("#hover-readout").textContent =
        `${path.label} | RTT ${path.metrics.avg_rtt_ms.toFixed(1)}ms | Loss ${path.metrics.loss_pct.toFixed(1)}% | Jitter ${path.metrics.avg_jitter_ms.toFixed(1)}ms | Share ${path.metrics.window_share_pct.toFixed(1)}% | Hits ${path.metrics.hit_count}`;
    });
    root.appendChild(lane);
  }
}

function seedCustomWindowInputs() {
  const times = state.session.observations.map((observation) => new Date(observation.observed_at));
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
