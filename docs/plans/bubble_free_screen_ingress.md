# Feature plan: Bubble-free screen ingress (park gate + cached feed)

**Repo:** `pachakutech-situated-ai-presence-mcp`  
**Primary code:** `daemon/src/overlay.rs`, ingress under `daemon/src/actors/ingress/`  
**Status:** Design for implementation

---

## 1. Intent

The full-output screenshot is **runtime ingress**, not a private trick of the looking-glass.

- **Ingress:** a continuous (or regularly refreshed), **bubble-free** image of the desktop output, held in a form the substrate can sample (prefer GPU-resident dmabuf → `VkImage`).
- **Bubble / hyperbubble:** one **presence** that happens to render by sampling that ingress very literally (looking-glass). Other actors (quiescence, highlight resolution, future perception) should use the **same** ingress.

Therefore optimize for:

1. **No recursive self-sampling** (ingress must never contain the presence disc).
2. **Consistent ingress** under variable consumer rates (MCP agents, present loop, future actors call at different frequencies).
3. **Dmabuf / GPU path as the real product**; SHM is degrade-or-skip, not a design target.

---

## 2. Why Option 2 (gated background capture)

| Concern | Option 1 (capture only on overlay/present tick) | Option 2 (capture service + explicit park gate) |
|--------|--------------------------------------------------|--------------------------------------------------|
| Agent A tools at 0.2 Hz, agent B at 5 Hz, present at 10 Hz | Ingress refresh tied to whoever happens to drive the loop | Ingress refreshes on its **own** cadence |
| Quiescence / perception need a fresh frame without drawing the bubble | Must piggyback on present | Read latest committed ingress anytime |
| Park window | Easy to order, but capture rate = present interest | Park only for capture critical section; present uses **cache** |
| Multi-consumer consistency | Risk of “stale unless something drew” | Single producer, many readers of last good frame |

**Decision: Option 2.**

One **capture service** produces bubble-free frames on a fixed policy (e.g. ≤10 Hz). The present loop and all agents **only read** the latest committed ingress. Different agent frequencies never change how exclusion or capture ordering works.

Option 1 remains an acceptable fallback implementation detail if the capture service is in-process and synchronous *behind the same gate*; the architecture still treats “capture” as a producer, not as a side effect of `gpu.draw`.

---

## 3. Design principles

1. **Ingress is shared substrate state** — one latest frame (or small ring), versioned.
2. **Exclusion is compositional** — park the layer-shell disc off-screen for the capture critical section; destination (dmabuf vs SHM) does not change the need to exclude.
3. **Present is decoupled from capture** — between captures, the bubble (and any other consumer) uses the **last bubble-free** texture.
4. **Restore position is dynamic** — unpark uses current `bubble_x` / `bubble_y` from `set_bubble_pos` (quiescence may move the disc; never hard-code center on restore).
5. **Dmabuf-first** — quality path is GPU-resident; SHM may yield no new frame or a last-resort path, not a second architecture.
6. **Park does not change opacity** — geometry only; 50% alpha is out of scope unless added as a separate uniform later.

---

## 4. Roles

```text
┌─────────────────────────────────────────────────────────┐
│  Capture service (producer)                             │
│    wait for “need frame” or timer                       │
│    request park → wait parked_applied                   │
│    single-shot output copy (dmabuf preferred)           │
│    commit frame + sequence number → ingress slot        │
│    signal capture_done → allow unpark                   │
└─────────────────────────────────────────────────────────┘
         ▲ park / unpark commands              │ published ingress
         │                                     ▼
┌────────────────────┐              ┌──────────────────────────┐
│ Overlay / present  │              │ Consumers                │
│  park_offscreen    │              │  • looking-glass present │
│  unpark_at_bubble  │              │  • quiescence (later)    │
│  draw from cache   │              │  • highlight / perception│
└────────────────────┘              │  • MCP-driven actors     │
                                    └──────────────────────────┘
```

The bubble is **not** the owner of the screenshot path; it is a client of ingress.

---

## 5. Park gate protocol (normative)

Shared state (atomics / mutex + condvar, names illustrative):

| Flag / value | Meaning |
|--------------|---------|
| `ingress_seq` | Monotonic id of last **committed** bubble-free frame |
| `capture_requested` | Producer wants a new sample |
| `parked_for_capture` | Overlay has committed off-screen margins and believes compositor will exclude the disc |
| `capture_in_progress` | Copy started; do not unpark until done or failed |
| `last_error` | Optional; sticky failure for diagnostics |

### 5.1 Happy path

```text
Producer                          Overlay (main / Wayland owner)
────────                          ─────────────────────────────
set capture_requested
                                  if visible and not idle-hide:
                                    park_offscreen()
                                    flush + barrier (roundtrip / apply)
                                    set parked_for_capture = true
wait until parked_for_capture
  (or timeout → fail this attempt)
set capture_in_progress
single-shot dmabuf (or SHM) copy
import / publish to ingress slot
ingress_seq += 1
clear capture_in_progress,
  capture_requested
signal done
                                  clear parked_for_capture
                                  unpark_at_bubble()  // latest bubble_x/y
                                  // present continues from new or previous cache
```

### 5.2 Rules

- **Never** start full-output capture while the disc is believed on-output (`parked_for_capture == false` and surface not idle-hidden).
- **Never** unpark while `capture_in_progress`.
- If already hidden for idle/lock/sleep, capture **may** run without a park dance (surface already off-screen); still publish as normal ingress.
- On capture failure: clear in-progress, unpark if we parked, keep previous `ingress_seq` (do not publish a partial/self-including frame).
- `set_bubble_pos` during park updates stored coordinates only; **unpark always uses latest** `bubble_x`/`bubble_y`.

### 5.3 Barrier

After `park_offscreen()`, require at least one Wayland `flush` + `roundtrip` (or equivalent) on the **overlay** connection before setting `parked_for_capture`. Document that exclusion is margin-based (`-(BUBBLE_PX*4)` top with Top|Left anchors), not alpha.

---

## 6. Capture cadence and cache

**Producer policy (default):**

- Target interval: `CAPTURE_INTERVAL` (existing 100 ms / ≤10 Hz) or “every N present ticks,” whichever is simpler to implement first.
- **At most one** in-flight capture.
- Coalesce multiple “need frame” signals into one capture.

**Present policy:**

- Every draw: sample **current ingress** texture (last committed).
- Do **not** park on pure present ticks.
- If `ingress_seq == 0` (no frame yet): show nothing / clear / previous stub — **prefer empty over SHM garbage**.

**Consumer policy (agents / actors):**

- Read `ingress_seq` + texture/view handle (or CPU mirror only if explicitly required later).
- No consumer may trigger an ungated capture that skips the park protocol.
- Optional: `request_ingress_refresh()` sets `capture_requested` without blocking the agent on completion (async); blocking wait is allowed with timeout for tools that need a fresh frame.

This is what makes multi-agent frequency safe: agents never own the copy; they only request or read.

---

## 7. Backend priority

| Backend | Role |
|---------|------|
| **Dmabuf import** | Production ingress. Publish GPU image + seq. |
| **SHM full-res** | Not a product target. Prefer: no publish, keep last dmabuf frame, or disable. |
| **SHM downscaled (optional)** | Only if a machine has no dmabuf path and “some ingress” is required; not optimized for, may be omitted entirely. |

Do not structure the gate or cadence around 1080p CPU convert cost. Structure it around **reliable exclusion + stable seq**.

---

## 8. API sketch (daemon-internal)

```text
/// Published ingress: bubble-free, versioned.
struct IngressFrameSlot {
  seq: u64,
  // dmabuf path: VkImage / view / sampler ready for sampling
  // optional: small metadata width, height, timestamp
}

/// Producer side
fn capture_service_loop(...)  // timer or capture_requested

/// Overlay side (Wayland owner)
fn park_offscreen()
fn unpark_at_bubble()          // uses current bubble_x, bubble_y
fn set_bubble_pos(x, y)        // existing; quiescence later

/// Readers
fn latest_ingress() -> Option<&IngressFrameSlot>
fn request_ingress_refresh()
```

MCP protocol: **unchanged** for this feature. Ingress is internal substrate; tools continue to speak manifestations only.

---

## 9. Interaction with existing overlay code

Reuse:

- `BUBBLE_PX`, `set_bubble_pos`, `park_offscreen`, `unpark_at_bubble`
- `CAPTURE_INTERVAL`
- Dmabuf feed types (`DmabufScreenFeed`, import path)
- Idle/lock/sleep hide via park

Change:

- Stop **free-running** capture on a second client while the disc may be on-screen.
- Route all output copies through the **park gate**.
- Present loop: draw from **cached** ingress; capture service owns refresh.
- Treat SHM path as non-goals for quality (match §7).

---

## 10. Consistency guarantees (acceptance)

1. **No self-inclusion:** For every committed `ingress_seq`, the frame was captured only while the presence disc was off-output (or fully hidden). Manual or automated check: pixels under the last on-screen bubble AABB match the underlay, not the disc.
2. **Stable under multi-consumer load:** Two readers at different rates always see a monotone `ingress_seq` and never force an ungated capture.
3. **Present without capture:** With capture paused, present still draws the last good ingress (or nothing if seq=0).
4. **Dynamic restore:** After capture, disc returns to latest `set_bubble_pos`, not startup center.
5. **Dmabuf-first:** On a machine with working dmabuf import, ingress commits use that path; SHM is not required for “feature complete.”
6. **Opacity unchanged** by park/unpark.

---

## 11. Implementation order

1. Introduce gate state + “latest ingress” slot (seq + GPU view).
2. Implement overlay responses: park barrier → `parked_for_capture`; on done → unpark at latest pos.
3. Convert capture to **single-shot under gate** (dmabuf); remove free-run while visible.
4. Point `gpu.draw` / looking-glass at **cached** ingress only.
5. Wire `request_ingress_refresh` for future actors; present timer can set the same flag.
6. Idle/hide: skip redundant park; define whether capture continues while hidden (recommended: yes, still useful ingress, or pause—pick one and document; **default: pause while hidden** to save work).
7. SHM: explicit non-publish or last-resort; do not block the feature on it.
8. Document in `daemon/README.md` / architecture: ingress is substrate; bubble is a presence on that feed.

---

## 12. Non-goals

- Quiescence / “least busy region” heuristics (consumer of ingress + `set_bubble_pos` only).
- 50% (or any) opacity as exclusion substitute.
- MCP schema changes.
- Perfect zero-flicker park (cache already minimizes park frequency; further flicker work is optional).
- Optimizing full-resolution CPU screencopy.

---

## 13. One-paragraph summary

Build a **gated capture service** that parks the movable layer-shell disc, copies the output once (dmabuf preferred), commits a versioned **bubble-free ingress** frame, then unparks to the **current** bubble position; the looking-glass and all other agents only **read** that cached ingress so recursion cannot depend on agent frequency.