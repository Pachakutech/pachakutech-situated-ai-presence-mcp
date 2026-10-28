Here are two implementation-oriented write-ups aligned with the existing Presence contract (typed tools, policy gate, Unix-socket JSONL, “messages describe, not carry,” Presence owns motion).

---

## Subtask A — Interface changes for tight phrase flags

**Goal:** Give the outer agent a small, non-nested vocabulary for *what the user did with phrases on glass*, without inventing a second agent stack or folding approval into locate/highlight success.

### Problem

Today the surface is action-shaped (`locateScreenRegion`, `manifestHighlight`, `highlightAndSpeak`, `animatePresence`, …). Outcomes are mostly “ok / error / box found.” There is no first-class, queryable record of:

| Flag | Meaning |
|------|---------|
| **viewed** | Phrase (or passage) was on screen / in the measured crop at decision time |
| **approved** | User marked it keep / yes / in-set |
| **discarded** | User marked it drop / no / out-of-set |

Those three are **independent**. Approval does not imply discard is false; discard does not require prior approval; viewed is observational, not a user intent. Nesting “did the user like what we highlighted?” inside tool return prose forces the outer model to re-parse free text and invites nested sub-agents.

### Scope of interface change

**1. Phrase identity (stable, opaque)**  
Introduce a lightweight `phraseId` (or `markId`) minted when a region is successfully measured or when the user explicitly tags a selection. Do not require the outer agent to re-supply full query strings for every follow-up.

**2. New write tools (or one tool with a discriminated action)**  
Minimal set:

- `recordPhraseObservation` — sets **viewed** (and optional UV / source / query snapshot for the log). Callable from locate success paths and from LFM3 later.
- `setPhraseFlags` — sets **approved** and/or **discarded** independently (`approved?: boolean`, `discarded?: boolean`). Missing fields leave previous values unchanged; explicit `false` clears.

Optional UX bridge (daemon or MCP): pointer/hotkey on the tint layer that posts the same proposals so the human does not have to route through the LLM.

**3. New read tool**
- `listPhraseFlags` — returns a tight table, e.g.  
  `{ phraseId, query?, viewed, approved, discarded, u,v,w,h?, source?, updatedAt }`  
  No narrative. Cap length (e.g. last N or session window) so context stays small.

**4. Return shapes on existing fused tools**  
Extend successful JSON from `locateScreenRegion` / `manifestHighlightFromQuery` / `highlightAndSpeak` with optional:

```json
{
  "phraseId": "ph-…",
  "flags": { "viewed": true, "approved": null, "discarded": null }
}
```

`null` = unset, not false. Do not overload `status: "ok"` to mean approved.

**5. Policy gate**
- Rate-limit write tools like other non-retire tools (1.5s same-tool).
- Cap stored phrases (e.g. 50/session) independent of presence/artifact caps.
- Retire/clear phrases on presence retire or explicit `clearPhraseFlags` if needed.

**6. Skill / contract copy**  
Update `skills/presence/SKILL.md` and the one-pager:

- Outer agent **must not** invent nested “approval agents.”
- After mark/speak, prefer `listPhraseFlags` over asking the user in prose when the UI can set flags.
- Treat the three flags as orthogonal bits for downstream tasks (subgroup select, task approve, delete-from-set, etc.).

**7. Explicit non-goals**
- No joint/bone/UV authorship from the agent.
- No requirement that `approved ∧ ¬discarded`.
- No embedding of full document text in flag records—query snippet + UV + ids only.

**Acceptance**

- Agent can mark a phrase, record viewed, set approved and discarded in either order, list them, and get stable ids without re-locating.
- Two MCP clients sharing the daemon see the same flag store (store lives in daemon, not only in the Node policy gate).
- Docs state independence of the three flags with one concrete example each (approve+discard, select subgroup, approve task).

**Effort sketch:** protocol variants in `daemon/src/protocol.rs`, registry table beside presence/artifact, MCP tools in `src/index.ts`, skill paragraph, a few socket tests.

---

## Subtask B — Sistering LFM3 (fuller)

**Goal:** Run Liquid **LFM3** (~2B class: VL / d1 decision / small text) as a **parallel local specialist** between perception and presence: vision/decision in → highlight / speak intents + phrase observations out. Preserve the splat/overlay frame path; do not put the model in the expand SSBO set.

### Architectural placement

```text
[screen / doc crops] ──ingest──► Perception (existing + optional OCR crops)
        │
        ▼
   LFM3 sidecar (or later in-daemon actor)   ◄── not in Vulkan frame graph
        │  structured JSON only
        ▼
   Control path → Presence actor
        animatePresence / manifestHighlight / recordPhraseObservation
        expand + discs unchanged
```

Same rule as architecture.md: **messages describe, not carry.** LFM3 emits intents and measurements (`speak`, `u,v,w,h`, `phraseId` + flags), never skin matrices or splat positions.

### Phase 0 — Sidecar (lowest risk)

1. **Runtime:** `llama-server` / Ollama / small local client over `localhost` (GGUF 4-bit ~1 GB). Prefer **CPU or iGPU** for inference so Iris Xe–class overlay submits are not stalled; if same GPU, strict time budget + lower priority than disc submits.
2. **Caller:** MCP binding or a thin daemon worker posts JSON jobs; results return as **proposals** on the existing socket kinds (or new kinds that map 1:1 onto them).
3. **Inputs (decision cadence, not 30 Hz):**
    - Region crops 384–1024 on the long side (existing mark rects, locate boxes, occasional low-rate full frame).
    - Optional short text context (query, window class, last spoken line).
    - 64×64-only patches are out of scope for VL quality.
4. **Outputs (schema-locked):**
   ```json
   {
     "highlight": { "u": 0.1, "v": 0.2, "w": 0.15, "h": 0.08, "description": "…" },
     "speak": "…",
     "phrase": {
       "query": "…",
       "viewed": true,
       "approved": null,
       "discarded": null
     }
   }
   ```  
   Mapper in daemon/MCP turns this into `highlightRegion` / `animatePresence` / `recordPhraseObservation`. **No** free-form tool chatter to the outer agent unless wrapped as one presence intent.
5. **d1 / typed head (preferred for “is this the error banner?”):** calibrated point/box/yes-no over free chat when latency matters.

### Phase 1 — In-daemon actor

- Same tick loop, **async** inference job; results post onto the **control** blackboard channel (latest coherent message), never into expand bindings.
- Perception still owns dma-buf; export a CPU/GPU image the inference runtime accepts at decision rate (no zero-copy miracle required day one).
- GPU contention: document VRAM budget; fail soft (skip tick, keep last highlight) rather than hitch the disc.

### Phase 2 — Fine-tune (optional)

- SFT/GRPO on **your** schema only: highlight + speak + the three phrase flags.
- Keep frontier model behind MCP for hard reasoning; LFM3 stays the **local perception↔presence reflex**.

### Wiring to Subtask A flags

| LFM3 observation | Flag / action |
|------------------|---------------|
| Crop contains readable target phrase | `viewed: true` + mint/update `phraseId` |
| User gesture / UI control “keep” | `approved: true` (via UI → same tool, not only model) |
| User gesture “drop” / model “not this region” | `discarded: true` without clearing approved unless UI says so |
| Model suggests box | `manifestHighlight` + observation; outer agent still owns policy |

Outer agent sees **flags and ids**, not nested “LFM3 said…” transcripts. That is the main lever against agent-nesting confusion.

### Interface additions specific to the sister model

Beyond Subtask A:

- `proposeLocalPresenceIntent` (internal or MCP-gated): accepts the structured LFM3 JSON, runs policy gate, dispatches existing proposals. Outer agent may call it rarely; preferred path is daemon-side auto-map from sidecar.
- Config: `PRESENCE_LFM3_ENDPOINT`, model id, max crop size, max concurrent jobs (1 is enough initially).
- Health: `presence doctor` reports sidecar reachability without failing the overlay.

### Constraints (carry into the task ticket)

- Same **Vulkan frame graph** as `avatar_expand`: **no**.
- Same **daemon process graph** perception → model → presence: **yes**.
- Shared GPU: **yes, carefully**.
- Capability ceiling: situated UI/doc chores, not a full MCP agent replacement.
- Client never authors bones; coarse tags (“point left”) only via a thin mapper if ever added.

### Acceptance

1. Sidecar can, from a crop + short context, emit a valid highlight UV and/or speak string that the daemon executes through existing paths.
2. Successful local propose sets **viewed** (and leaves approved/discarded unset unless UI/model explicitly sets them).
3. Overlay FPS and highlight latency remain acceptable under concurrent LFM3 jobs (document measured numbers on Iris Xe-class).
4. Outer MCP skill documents: use `listPhraseFlags`; do not spawn a sub-agent to interpret LFM3 prose.
5. Failure modes: sidecar down → locate/highlight tools still work; no partial write into expand buffers.

### Suggested ticket split

| Ticket | Deliverable |
|--------|-------------|
| A1 | Daemon phrase flag store + protocol + MCP read/write tools |
| A2 | Skill + return-shape updates on locate/highlight fused tools |
| B1 | Sidecar client + JSON schema + map to existing proposals |
| B2 | Crop ingest from mark/locate at decision cadence |
| B3 | Doctor/config + GPU/CPU placement notes |
| B4 (later) | In-daemon actor + optional d1 head + SFT schema |

---

**Bottom line:** Subtask A shrinks what the outer agent must track to three orthogonal bits plus ids. Subtask B sisters LFM3 as a local reflex on the control plane so perception can drive marks and speech without nesting another conversational agent or touching the GPU-resident expand path.