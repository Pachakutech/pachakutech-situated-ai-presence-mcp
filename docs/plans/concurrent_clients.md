# Spec: Daemon-Authoritative Caps, IDs, and Concurrent Socket Clients

**Status:** Design for implementation  
**Repo:** `pachakutech-situated-ai-presence-mcp`  
**Goal:** Make the Presence Daemon the single process of record for presence/artifact identity and concurrent limits so multiple MCP Binding processes can share one substrate safely.

---

## 1. Problem statement

Today:

| Concern | Location | Problem under multi-MCP |
|--------|----------|-------------------------|
| Rate limit, max 3 presences, max 20 artifacts | `src/policyGate.ts` (per process) | Caps multiply with number of MCP processes |
| `presenceId` / `artifactId` generation | `src/index.ts` (`Math.random()`) | IDs not globally unique or authoritative |
| Live id maps for assert/retire | `PolicyGate` | Process A cannot see process B’s ids |
| Scene Memory, GPU ranges, live presences | Daemon `Registry` / actors | Correct place for truth, but not enforcing caps/ids |

Socket accept already supports multiple clients (`Vec<Client>` + shared `Mutex<Registry>`). Completing “concurrent clients” means **authority**, not only accept.

---

## 2. Design principles

1. **One substrate, many bindings.** The daemon owns Scene Memory, GPU buffer ranges, live presence state, caps, and id allocation. MCP Binding is a thin policy-aware client of that API.
2. **Messages describe, not carry.** Wire format stays small JSONL; no pixels/buffers cross the Unix socket.
3. **Daemon is fail-closed on caps.** Over-cap and unknown-id requests return structured errors; the binding surfaces them to the agent.
4. **Binding may soft-rate-limit only.** Hard concurrent limits must not live only in TypeScript.
5. **GPU stays numeric.** String ids stay in CPU maps; `AnimatedSplatGpu.owner_id` remains a `u32` GPU tag linked via `gpu_ranges`.

---

## 3. Scope

### In scope

- Move concurrent caps and id authority into the daemon.
- Adjust Unix socket protocol request/response shapes as needed.
- Thin `PolicyGate` / `index.ts` / `daemonClient.ts` accordingly.
- Document multi-client semantics (accept, shared registry, errors).
- Optional but recommended: client disconnect cleanup policy (see §8).

### Out of scope (this change)

- Text-to-pose model, overlay raster, ingress ticking.
- Changing the six tool *names* or MCP tool list.
- Multi-machine / networked daemon (local Unix socket only).
- Per-user ACL / authentication on the socket (desktop session trust model unchanged).

---

## 4. Constants (daemon)

Define once in the daemon (e.g. `daemon/src/limits.rs` or on `Registry`):

| Name | Value | Applies to |
|------|-------|------------|
| `MAX_CONCURRENT_PRESENCES` | `3` | Live entries in PresenceActor / registry |
| `MAX_CONCURRENT_ARTIFACTS` | `20` | Held entries in Scene Memory |
| `MIN_INTERVAL_MS` | `1500` | Optional global or per-client rate limit on mutating tools except retires |
| `ID_PREFIX_PRESENCE` | `"p-"` | Generated presence ids |
| `ID_PREFIX_ARTIFACT` | `"a-"` | Generated artifact ids |
| `MAX_SOCKET_CLIENTS` | implementation choice, e.g. `8` or `32` | Optional accept backstop |

Retires (`retirePresence`, `retireArtifact`) are **never** rate-limited.

Presences and artifacts remain **independent** risk classes and independent caps.

---

## 5. Authority model

### 5.1 Identity

- **Daemon allocates** `presenceId` and `artifactId` on successful create.
- Client **must not** be required to supply create ids.
- Client **may** send an optional `clientSuggestedId` (see §6); daemon accepts only if unused and well-formed; otherwise allocates its own (or rejects—pick one policy and document it; **recommended: ignore suggestion on collision and allocate**).

**Id format (normative):**

- Presence: `p-` + 8–12 lowercase alphanumeric characters (cryptographic or strong RNG preferred over `Math.random`).
- Artifact: `a-` + same.
- Region (highlight): daemon may continue `r-{proposalId}` or allocate `r-…`; not capped as an instance registry.

### 5.2 Caps

On `spawnPresence`:

- If live presence count ≥ `MAX_CONCURRENT_PRESENCES` → error, no insert.

On `addArtifact`:

- If held artifact count ≥ `MAX_CONCURRENT_ARTIFACTS` → error, no insert.

On `animatePresence` / `retirePresence`:

- Unknown `presenceId` → error.

On `retireArtifact`:

- Unknown `artifactId` → error.

On `spawnPresence` with `artifactId`:

- If set and artifact not held → error (or spawn without artifact—**recommended: hard error** so agents cannot silently lose reference material).

### 5.3 Rate limiting

**Recommended phase 1:** keep soft rate limit only in the binding (UX), daemon enforces caps only.

**Recommended phase 2 (optional in same PR or follow-up):** daemon tracks last success time per `(client_id, kind)` or global per `kind`; reject with `rate_limited` if under `MIN_INTERVAL_MS`, excluding retires.

---

## 6. Wire protocol (JSONL over Unix socket)

Transport unchanged: one JSON object per line, proposal in, result out, existing socket path.

### 6.1 Result envelope (unchanged shape)

```json
{
  "proposalId": "q-…",
  "status": "ok" | "error",
  "detail": { },
  "error": "human-readable message",
  "errorCode": "optional_machine_code"
}
```

Add optional `errorCode` (string enum) for stable client handling. If serialization compatibility is a concern, `errorCode` may live inside `detail` on errors; **prefer top-level** for clarity.

**Error codes (normative set):**

| `errorCode` | When |
|-------------|------|
| `rate_limited` | Interval violated (if daemon enforces) |
| `presence_cap_exceeded` | Would exceed max live presences |
| `artifact_cap_exceeded` | Would exceed max held artifacts |
| `unknown_presence` | Missing `presenceId` |
| `unknown_artifact` | Missing `artifactId` |
| `malformed_proposal` | JSON / schema failure |
| `internal` | Unexpected daemon failure |

### 6.2 Create proposals — client omits server ids

**`spawnPresence` (client → daemon)**

```json
{
  "kind": "spawnPresence",
  "proposalId": "q-…",
  "sourceContext": "string",
  "styleHint": "string or null",
  "artifactId": "a-… or null",
  "clientSuggestedId": "optional string or null"
}
```

**Success `detail`:**

```json
{
  "presenceId": "p-…",
  "livePresenceCount": 1,
  "maxPresences": 3
}
```

**`addArtifact` (client → daemon)**

```json
{
  "kind": "addArtifact",
  "proposalId": "q-…",
  "description": "string",
  "sourceUri": "string or null",
  "clientSuggestedId": "optional string or null"
}
```

**Success `detail`:**

```json
{
  "artifactId": "a-…",
  "heldArtifactCount": 1,
  "maxArtifacts": 20
}
```

### 6.3 Instance proposals — client supplies daemon-issued ids

**`animatePresence` / `retirePresence`:** require `presenceId` (daemon-issued).

**`retireArtifact`:** require `artifactId`.

**`highlightRegion`:** unchanged fields; no instance cap.

### 6.4 Backward compatibility

During migration, if client still sends `presenceId` / `artifactId` on create (current protocol):

- **Preferred:** treat as `clientSuggestedId`; daemon validates uniqueness; response still returns the canonical id.
- Remove client-side mandatory generation once binding is updated.

---

## 7. Daemon internal interface

### 7.1 `Registry` responsibilities (extend existing)

```text
Registry
  - SceneMemory (artifacts)
  - PresenceActor (live presences)
  - gpu_ranges: HashMap<artifactId, GpuRange>
  - next_slot / next_owner / frame (existing GPU bookkeeping)
  - NEW: enforce caps before actor insert
  - NEW: allocate_presence_id() / allocate_artifact_id()
  - NEW: counts for live presences / artifacts
```

Suggested method semantics:

```text
fn spawn_presence(...) -> Result<SpawnOutcome, RegistryError>
fn add_artifact(...) -> Result<AddArtifactOutcome, RegistryError>
fn animate_presence(id, text) -> Result<(), RegistryError>
fn retire_presence(id) -> Result<(), RegistryError>
fn retire_artifact(id, pipeline) -> Result<(), RegistryError>
```

`RegistryError` maps 1:1 to `errorCode` values above.

`SpawnOutcome` / `AddArtifactOutcome` include the canonical string id and current counts (for `detail`).

### 7.2 Mapping to existing structures (do not redesign GPU)

- `artifact_id: String` → `SceneMemory` + `gpu_ranges`
- On upload, continue assigning monotonic `owner_id: u32` into `AnimatedSplatGpu` (96-byte layout unchanged)
- `presence_id: String` → `PresenceActor.live`
- `LivePresence.artifact_id: Option<String>` remains the link from presence → held cloud

No requirement to store string ids in the 96-byte splat.

### 7.3 Socket layer

- Keep multi-client `Vec<Client>` + shared `Mutex<Registry>`.
- Optionally assign opaque `client_id: u64` per accept for rate-limit and disconnect policy.
- Optional: reject accept when `clients.len() >= MAX_SOCKET_CLIENTS`.
- Hold registry lock only for bookkeeping + dispatch; avoid holding across long optional future GPU work if it becomes expensive (not required for v1 if current upload/tick stays short).

---

## 8. Disconnect / lifecycle policy

**Minimum (required for “shared substrate” honesty):**

- Caps count only objects still in daemon maps.
- Document that crashing MCP processes can leave live presences/artifacts until explicit retire or daemon restart.

**Recommended (include if effort is small):**

- On client disconnect, either:
    - **A.** Leave objects (explicit multi-agent share), or
    - **B.** Retire all objects created by that `client_id` (session-scoped).

**Spec default for v1:** **Policy A** (leave objects), with counts still global. Session-scoped cleanup is a documented follow-up flag, e.g. env `PRESENCE_SESSION_SCOPED=1`.

---

## 9. MCP Binding changes

### 9.1 `src/index.ts`

- **spawnPresence / addArtifact:** do not generate ids locally; do not `policy.register*` before daemon success.
- After daemon ok, return daemon’s `presenceId` / `artifactId` in tool result JSON (same agent-facing shape as today).
- **animate / retire:** pass through ids; on daemon `unknown_*`, surface error text to MCP result (do not only trust local maps).

### 9.2 `src/policyGate.ts`

**After change:**

- **Remove** hard enforcement of `MAX_CONCURRENT_*` and live maps as source of truth, **or** keep maps as optional local cache only (must not block a valid daemon id from another session).
- **Keep** optional local `MIN_INTERVAL_MS` soft limit for same-process spam.
- Prefer: PolicyGate becomes rate-limit-only (plus any future content policy), not instance registry.

### 9.3 `src/daemonClient.ts`

- Update RPC payloads: stop requiring create ids; parse `detail.presenceId` / `detail.artifactId`.
- Map `errorCode` to thrown `Error` messages that include the code string for debugging.
- Existing per-connection RPC serialization (`chain`) remains correct under multi-process (each process has its own connection).

### 9.4 Stub path (`daemonStub.ts`)

- When daemon is absent, stub must still allocate ids and enforce the **same caps** in-process so single-agent stub behavior matches the contract.
- Stub does not provide cross-process sharing (no socket).

---

## 10. Agent-facing MCP contract (unchanged intent)

Tools remain:

- `manifestHighlight`
- `spawnPresence` → returns `{ presenceId }`
- `animatePresence` / `retirePresence`
- `addArtifact` → returns `{ artifactId }`
- `retireArtifact`

Descriptions should note:

- Concurrent live presences capped at 3 **globally on the substrate** (not per agent process).
- Held artifacts capped at 20 globally.
- Ids returned by spawn/add are the only valid ids for later calls.

---

## 11. Testing checklist

1. **Single client:** spawn 3 presences ok; 4th returns `presence_cap_exceeded`.
2. **Single client:** add 20 artifacts ok; 21st `artifact_cap_exceeded`.
3. **Two clients (two connections):** client A spawns 2, client B spawns 1, either’s 4th spawn fails globally.
4. **Cross-client id:** A spawns, B cannot animate A’s id only if session isolation is enabled; under default Policy A, B **can** animate A’s id if it knows the string (document this; optional later: ownership checks).
5. **Unknown id:** animate/retire missing id → `unknown_presence` / `unknown_artifact`.
6. **Retire frees cap:** retire then spawn succeeds.
7. **Stub mode:** same cap numbers without daemon.
8. **Protocol:** create without client id; response includes server id.
9. **GPU path:** addArtifact still uploads and sets `gpu_ranges` + `owner_id` as today.

**Default multi-client semantics for v1:** global shared namespace of ids and caps (true shared substrate). If product later needs isolation, add optional `client_id` ownership checks—not part of the minimum.

---

## 12. Implementation order (for the implementing agent)

1. Add `RegistryError`, limits constants, id allocator, cap checks inside `Registry` create/retire paths.
2. Change `socket.rs` `dispatch` to return codes/details from `Result` outcomes; stop assuming client-supplied create ids are authoritative.
3. Update `protocol.rs` serde shapes (optional fields for suggested ids; create path without required presence/artifact id).
4. Update `daemonClient.ts` + `index.ts` + thin `policyGate.ts`.
5. Align `daemonStub.ts` with same cap/id rules.
6. Update `daemon/README.md`, `docs/architecture.md`: remove “one connection at a time”; state daemon authority + global caps; note multi-MCP share model.
7. Run through testing checklist above (manual or automated as available).

---

## 13. Non-goals / explicit non-requirements

- Embedding string `artifactId`/`presenceId` into the 96-byte GPU splat.
- Per-MCP private GPU partitions.
- Coordinating caps via the TypeScript layer indexing into splat buffers.
- Changing Vulkan pipeline, splat size, or `owner_id` meaning beyond existing GPU eviction/grouping use.

---

## 14. Acceptance criteria

This work is **done** when:

1. Two concurrent MCP Binding processes against one daemon share one presence cap (3) and one artifact cap (20).
2. All successful `presenceId` / `artifactId` values are allocated (or validated) by the daemon and returned in create results.
3. Hard concurrent limits are not enforceable solely in `policyGate.ts`.
4. Docs state multi-client accept + daemon authority accurately.
5. Existing single-client tool behavior and GPU ingest path remain functionally equivalent aside from id allocation locus.

---

This document is sufficient to implement without further design questions; optional choices already resolved above are: **daemon allocates ids**, **global shared caps**, **hard error on unknown artifact at spawn**, **disconnect leaves objects (Policy A)**, **rate limit soft-in-binding first**.