# Cockpit Realtime Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework the demo cockpit into a full-viewport realtime monitoring UI with per-path line charts.

**Architecture:** Keep the existing embedded static assets and Rust server. Add a small asset regression test, then update HTML/CSS/JS so the browser simulates live observations from the deterministic demo session and renders per-path SVG charts.

**Tech Stack:** Rust unit tests, embedded HTML/CSS/vanilla JavaScript assets.

---

### Task 1: Asset Regression Test

**Files:**
- Modify: `crates/tracertmonitor/src/server.rs`

- [ ] **Step 1: Add a failing test**

Add a unit test that requests `/`, `/assets/app.js`, and `/assets/styles.css`, then asserts that the assets contain `路径实时监控`, `renderPathChart`, `setInterval`, `100vw`, and `100vh`.

- [ ] **Step 2: Run the focused test**

Run: `cargo test -p tracertmonitor --lib server::tests::cockpit_assets_expose_realtime_monitoring_layout`

Expected before implementation: fail because the current assets do not contain the realtime chart and full-screen layout markers.

### Task 2: Full-Viewport Layout

**Files:**
- Modify: `crates/tracertmonitor/src/assets/index.html`
- Modify: `crates/tracertmonitor/src/assets/styles.css`

- [ ] **Step 1: Update markup**

Rename the bottom header to `路径实时监控`, keep time-window controls, and give the timeline panel a structure that can host chart cards plus hover readout.

- [ ] **Step 2: Update CSS**

Make `.cockpit` use `width: 100vw`, `height: 100vh`, `grid-template-areas`, and a larger bottom row. Make `.timeline-panel` span all columns and make `.lanes` scroll internally when needed.

### Task 3: Realtime Path Charts

**Files:**
- Modify: `crates/tracertmonitor/src/assets/app.js`

- [ ] **Step 1: Add live observation state**

Create `liveObservations`, `baseObservations`, and `liveCursor` state. Seed from demo observations and append shifted observations on an interval.

- [ ] **Step 2: Recalculate active paths from live observations**

Use live observations in `pathsForActiveWindow()` and `currentWindowRange()`, preserving the selected path.

- [ ] **Step 3: Render per-path charts**

Replace text-only lanes with cards that call `renderPathChart(path)` and draw an SVG RTT polyline, loss markers, and hover points.

### Task 4: Verification

**Files:**
- Test only.

- [ ] **Step 1: Run JavaScript syntax check**

Run: `node --check crates/tracertmonitor/src/assets/app.js`

Expected: exit code 0.

- [ ] **Step 2: Run focused test**

Run: `cargo test -p tracertmonitor --lib server::tests::cockpit_assets_expose_realtime_monitoring_layout`

Expected: pass.

- [ ] **Step 3: Run crate tests**

Run: `cargo test -p tracertmonitor`

Expected: all tests pass.
