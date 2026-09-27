
---

# Doc 2 — Feature: session-inactive ingress power policy (pause / release / resume)

**File suggestion:** `docs/feature-session-ingress-power-policy.md`  
**Primary code:** `daemon/src/overlay.rs` (`run` loop, capture threads), future `actors/ingress/*` (screen + webcam)  
**Related:** park-gate ingress plan (bubble-free capture); this doc is **when** capture runs, not **how** exclusion works

---

## 1. Intent

When the session is idle, locked, under screensaver, or preparing for sleep:

1. **Presence stays hidden** (already: park off-screen).
2. **Live ingress producers stop** — do not keep pulling full-output frames (or webcam) into the substrate.
3. **Last good ingress may remain cached** so return-to-session can present immediately after the bugfix unparks.
4. **One policy** drives screen capture and, later, webcam — same enter/leave inactive edges.

This is independent of multi-MCP instance cleanup. It is **session lifecycle × ingress resources**.

---

## 2. Current gap

Today, when `hide` is true:

- Main loop parks and **stops consuming** `frame_rx` / **stops drawing**.
- Capture threads **ignore** hide and continue `next_frame` + sleep forever (dmabuf or SHM).
- No webcam tick yet, but V4L2 modules exist and will need the same policy.

So inactive session still pays ongoing capture cost; return-to-session is only a visibility problem (bugfix), not a full resource story.

---

## 3. Cost model (why release matters on GPU too)

| Producer | While hidden but still capturing | Severity of stop |
|----------|----------------------------------|------------------|
| SHM full-output | CPU convert + memfd + upload every interval | **High** — avoid as product path; stopping is dramatic |
| Dmabuf full-output | Compositor copy + import/retire images, GPU bandwidth, power | **Medium** — less than SHM, still real under idle/SS for long periods |
| Cached last `VkImage` | Idle GPU memory for one/two frames | **Low** — keep for instant redraw |
| Webcam (future) | Device node, LED, exclusivity, privacy | **High / mandatory** — release on inactive regardless of GPU math |

**Conclusion:** Do not treat “pause capture” as a CPU-only optimization. Dmabuf pause is worthwhile; webcam release is required; one **SessionIngressPolicy** should own both.

---

## 4. Design principles

1. **Single inactive definition** — same four signals as presence hide (after bugfix reliability).
2. **Separate “hide presence” from “run producers”** only if a future need appears; **v1: same predicate** (`should_run_capture == !should_hide_presence`).
3. **Pause producers, keep last committed ingress** (L1 + L2 + L3 below).
4. **Resume is edge-triggered** — on active, restart producers and request one refresh; present can use cache immediately.
5. **Coordinate with park-gate** — when capture resumes, new frames still go through bubble-free gated capture; do not free-run while visible.
6. **SHM is not the design target** — policy applies equally; quality expectations stay dmabuf-first.

---

## 5. Three levels of “release”

| Level | Name | On enter inactive | On enter active |
|-------|------|-------------------|-----------------|
| **L1** | Pause | Stop scheduling new captures; wait on “session active” | Resume cadence / honor `request_ingress_refresh` |
| **L2** | Release producers | Tear down or deeply idle screencopy client; stop dmabuf imports; **close webcam** | Reconnect screencopy; optional webcam reopen |
| **L3** | Cache | Keep last bubble-free texture + `ingress_seq` | Draw cache immediately; then refresh |

**v1 default:** L1 + L2 for live producers, L3 keep cache.

Optional later knobs:

- `PRESENCE_RELEASE_CAPTURE_ON_IDLE=0` → L1 only (pause loop, keep client).
- Webcam reopen on active only if a presence/artifact policy requires camera ingress.

---

## 6. Architecture

```text
Session signals (idle, locked, screensaver, sleeping)
        │
        ▼
SessionIngressPolicy
        │
        ├─ should_hide_presence() ──► overlay park / unpark (bugfix owns correctness)
        │
        └─ should_run_ingress() ──► false on inactive
                │
                ▼
        IngressController
                ├─ ScreenCaptureService  (dmabuf preferred, gated park-capture)
                └─ WebcamService         (future: V4L2 open/close)
                │
                ▼
        IngressSlot { seq, gpu_view, timestamp }
                │
                ├─ looking-glass present
                ├─ quiescence / perception (later)
                └─ any agent-facing read of substrate
```

Capture service must **not** only check the timer; it must wait until `should_run_ingress()`.

---

## 7. State machine

```text
                    session active
              ┌──────────────────────────┐
              │  ACTIVE                  │
              │  presence: unparked      │
              │  capture: gated loop on  │
              │  webcam: policy optional │
              └────────────┬─────────────┘
                     hide becomes true
                           │
                           ▼
              ┌──────────────────────────┐
              │  ENTER_INACTIVE          │
              │  park presence (once)    │
              │  stop capture requests   │
              │  release screen producer │
              │  release webcam          │
              │  keep IngressSlot cache  │
              └────────────┬─────────────┘
                           │
                           ▼
              ┌──────────────────────────┐
              │  INACTIVE                │
              │  no new frames           │
              │  no draw required        │
              └────────────┬─────────────┘
                     hide becomes false
                           │
                           ▼
              ┌──────────────────────────┐
              │  ENTER_ACTIVE            │
              │  unpark + force present  │
              │  restart screen producer │
              │  request one gated frame │
              │  webcam reopen if needed │
              └──────────────────────────┘
```

Edges must be **idempotent** (duplicate “inactive” while already inactive is a no-op).

---

## 8. Screen capture service behavior

### 8.1 While ACTIVE

- Cadence ≤ `CAPTURE_INTERVAL` (or every N presents), **single in-flight**.
- Each frame: park-gate protocol (companion plan) → commit `IngressSlot`.
- Present samples cache only.

### 8.2 On ENTER_INACTIVE

1. Set `capture_enabled = false` (atomic or command channel).
2. If a capture is in progress: either wait for done (short timeout) then unpark-from-capture-gate, or cancel and unpark; never leave disc parked only for a capture that will be discarded.
3. Stop screencopy client / join or park the capture thread on a condvar (no full-rate sleep loop).
4. Retire transient import resources if any (beyond the single cached frame).
5. Do **not** zero `ingress_seq` or destroy the last good view.

### 8.3 On ENTER_ACTIVE

1. `capture_enabled = true`.
2. Restart client if L2 released it.
3. `request_ingress_refresh()` once.
4. Present may draw cached frame in the same tick as unpark (bugfix).

### 8.4 SHM

If dmabuf unavailable: either no producer while you refuse SHM quality, or L1/L2 apply the same way to SHM. Do not special-case SHM as “must keep running.”

---

## 9. Webcam (future, same policy)

| Event | Action |
|-------|--------|
| ENTER_INACTIVE | `VIDIOC` stream off, close fd, ensure LED policy respected |
| ENTER_ACTIVE | Reopen only if substrate still wants camera ingress (config or live actor need) |
| Agent requests camera while inactive | Fail soft or queue until ACTIVE — prefer fail with clear error |

Do not leave `/dev/video*` held across lock/screensaver for privacy and device sharing.

---

## 10. API sketch

```text
struct SessionSignals {
  idle: bool,
  locked: bool,
  screensaver: bool,
  sleeping: bool,
}

impl SessionSignals {
  fn hide_presence(&self) -> bool { /* OR of all */ }
  fn run_ingress(&self) -> bool { !self.hide_presence() } // v1
}

struct IngressController {
  fn apply_session(&mut self, s: SessionSignals);
  fn latest(&self) -> Option<IngressFrameRef>;
  fn request_refresh(&self);
}

// Capture thread
loop {
  wait until capture_enabled && (timer_or_refresh_requested);
  run_one_gated_capture();
  publish_or_keep_previous_on_failure();
}
```

Overlay `run` loop:

```text
signals = read atomics + idle bit
if signals != last_signals:
  ingress.apply_session(signals)
  // park/unpark + force present handled here or inside apply_session
last_signals = signals
```

---

## 11. Interaction with other plans

| Plan | Interaction |
|------|-------------|
| **Bugfix: return after SS/lock** | Must land first or together; power policy assumes `hide` tracks session correctly |
| **Park-gate bubble-free ingress** | ACTIVE captures use the gate; INACTIVE does not capture at all |
| **Daemon-authoritative caps/ids** | Orthogonal |
| **Quiescence placement** | Reads ingress only when ACTIVE; no need to run on SS |

---

## 12. Implementation order

1. Land bugfix (reliable `hide` edges + force present on show).
2. Add `capture_enabled` gated by same hide predicate; capture thread waits (L1).
3. Confirm CPU/GPU load drops while screensaver/idle (quick `intel_gpu_top` / power observation optional).
4. L2: tear down screencopy client on inactive; recreate on active.
5. Keep L3 cache through the transition; verify instant disc on return.
6. When webcam is ticked: hook release/acquire on the same `apply_session`.
7. Document in `daemon/README.md`: inactive session pauses ingress producers.

---

## 13. Testing checklist

1. Idle hide: capture stops (no new `ingress_seq`); disc gone.
2. Idle resume: disc returns (cache), then seq advances after one gated capture.
3. Screensaver: same; no continuous dmabuf import while SS up.
4. Lock/unlock: same.
5. Long inactive (30+ min): no capture thread spinning at 10 Hz; webcam not held (when implemented).
6. Active multi-agent tool spam while visible: still one capture cadence, still gated; inactive still wins and pauses everything.

---

## 14. Acceptance

- [ ] While idle, locked, screensaver, or sleep-prepare, no ongoing full-output capture loop.
- [ ] Last ingress remains available for immediate present on session return.
- [ ] On return, presence unparks (bugfix) and ingress producers restart once.
- [ ] Same session policy is the extension point for webcam release/acquire.
- [ ] Dmabuf is the production path; policy does not depend on SHM behavior.

---

## 15. Summary

**Feature doc:** when `hide` is true, **stop and release ingress producers** (screen now, webcam next) while **keeping a cached frame** so return is instant and long inactive sessions do not keep taxing compositor/GPU—or holding the camera.