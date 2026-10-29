# Task C1 — Daemon gate: per-endpoint tiers and allow-lists

Paste this whole file into your coding agent. It is self-contained.

## Context

Repo: `pachakutech-situated-ai-presence-mcp`. Read these first:
- `docs/architecture.md` (design rules: "messages describe, not carry"; presence owns motion)
- `docs/plans/local_multimodal_reflex.md` (trust tiers in section 2; running-system constraints in section 10)
- `docs/plans/presence_command_layer.md` (design) and `docs/plans/command_layer_protocol.md` (wire sketch, allow-list, test list)
- `daemon/src/protocol.rs`, `daemon/src/socket.rs`, `daemon/src/overlay.rs` (poll loop around line 3237), `src/policyGate.ts`

Today the daemon has **one** socket (`presence.sock`, path from `socket_path()` in `main.rs`, override `PRESENCE_DAEMON_SOCKET`). `SocketServer` in `socket.rs` accepts clients non-blockingly from the overlay loop (`pump`), parses each line into `Proposal`, and `dispatch`es. Every connection has equal authority. The Node `PolicyGate` (`src/policyGate.ts`) rate-limits MCP calls but the daemon does not check anything about *who* sent a message.

## Goal

Make **tier a property of the connection**, enforced in the daemon before dispatch, so later producers (user command channel, LFM3 sidecar) can be added without being able to call what the MCP client calls, and vice versa.

## Design decisions (already settled; do not re-open)

1. Tiers: `T0Physical` (overlay input; never a socket), `T1User` (command channel), `T2Client` (MCP, today's socket), `T3Sidecar` (LFM3).
2. One socket per socket-reachable tier, in the same directory as `presence.sock`:
   - `presence.sock` → `T2Client` (unchanged path and override)
   - `command.sock` → `T1User` (override `PRESENCE_COMMAND_SOCKET`)
   - `sidecar.sock` → `T3Sidecar` (override `PRESENCE_SIDECAR_SOCKET`)
3. A message body can never claim a tier. Tier comes only from which listener accepted the connection.
4. Each socket file is created with mode `0600`.
5. The allow-list is a function `allowed(tier, &Proposal) -> bool` with an **exhaustive `match` and no wildcard arm**, so adding a new `Proposal` variant fails to compile until someone decides its tiers.
6. Rejections are normal `ProposalResult::err` replies with the error string starting `not_allowed_for_tier:` followed by the kind. Never panic, never silently drop.
7. Existing behavior on `presence.sock` is unchanged for every existing kind.
8. `T0Physical` has no wire format. `allowed(T0Physical, _)` returns false for every socket kind; T0 input will arrive through an internal channel (task C2/C3).

## Allow-list for existing kinds

| Kind | T2 (`presence.sock`) | T1 (`command.sock`) | T3 (`sidecar.sock`) |
|---|---|---|---|
| `highlightRegion`, `spawnPresence`, `animatePresence`, `retirePresence`, `addArtifact`, `retireArtifact`, `queryScreenText`, `querySpeech`, `concealHighlight` | yes | no | no |
| `stopPresence`, `shutdown`, `avatarRest`, `avatarFace`, `avatarSpeak`, `avatarStop`, `avatarWalk` | yes (as today) | no | no |

(T1 and T3 get their first kinds in C2 and later tasks. For now their sockets exist and reject everything; that is intended and tested.)

## Steps

1. **`daemon/src/gate.rs` (new):** `pub enum Tier { T0Physical, T1User, T2Client, T3Sidecar }` (derive `Clone, Copy, Debug, PartialEq, Eq`, and `Serialize` with wire names `"T0" "T1" "T2" "T3"`); `pub fn allowed(tier: Tier, p: &Proposal) -> bool` (exhaustive match); `pub fn check(tier: Tier, p: &Proposal) -> Result<(), ProposalResult>` returning the `not_allowed_for_tier:<kind>` error. Add `Proposal::kind_name(&self) -> &'static str` in `protocol.rs` (exhaustive match, wire names).
2. **`socket.rs`:**
   - `SocketServer` holds `listeners: Vec<Endpoint>` where `Endpoint { listener: UnixListener, path: PathBuf, tier: Tier }`.
   - `Client` gets `tier: Tier`, set from the accepting endpoint.
   - `SocketServer::bind` takes the three paths (or a struct of them) and binds all three, each non-blocking, each `set_permissions(0o600)` right after bind. If `command.sock` or `sidecar.sock` fails to bind, log and continue with the others (fail soft); `presence.sock` failing is still fatal as today.
   - In `read_client`, after a successful parse and **before** `dispatch`: `if let Err(r) = gate::check(client.tier, &proposal) { reply r; continue; }`.
   - `Drop` removes all socket files it created.
   - Replace `listener_fd()` with `listener_fds() -> impl Iterator<Item = RawFd>`.
3. **`overlay.rs`:** update the poll set near line 3237 to include every listener fd.
4. **`main.rs`:** add `command_socket_path()` and `sidecar_socket_path()` beside `socket_path()`, same directory and env-override pattern. Pass all three to the overlay/socket setup.
5. **Node side:** no behavior change. Add a one-line comment in `src/policyGate.ts` that the daemon gate is authoritative for tiers and this gate remains a convenience rate-limiter.
6. **Docs:** add a short "Endpoints and tiers" section to `docs/architecture.md` (or `docs/control-surface.md`, whichever already documents the socket), listing the three sockets and the rule that tier comes from the connection.

## Tests (unit tests, same style as the `tests` module in `socket.rs`)

`dispatch` needs a GPU pipeline, so test the gate as pure functions and the endpoint binding separately.

| ID | Test name | Asserts |
|---|---|---|
| A5 | `t2_allows_every_existing_kind` | For a parsed sample of every existing kind, `allowed(T2Client, p)` is true |
| A6 | `t1_and_t3_reject_every_existing_kind` | Same samples, `allowed(T1User|T3Sidecar, p)` false; `check` error starts with `not_allowed_for_tier:` and names the kind |
| A6b | `t0_has_no_socket_kinds` | `allowed(T0Physical, p)` false for all samples |
| A7 | `allow_list_table_matches_doc` | Table-driven over (tier, kind) matching the allow-list table above |
| A4 | `body_cannot_claim_tier` | A `highlightRegion` JSON with extra `"tier":"T0"` still parses as before (existing kinds keep their permissive parsing) and `check(T3Sidecar, p)` still rejects; nothing reads a tier from the body |
| A8 | `endpoints_are_0600_and_removed_on_drop` | Bind three endpoints into a temp dir (factor a `bind_endpoint(path, tier)` helper so this needs no GPU); assert mode `0o600`; drop; assert files gone |
| A8b | `missing_side_socket_is_fail_soft` | If `command.sock`'s directory is unwritable (or bind fails by injected error), the helper returns the error and `presence.sock` still binds |
| A1–A3 | (stubs) | Add `#[ignore]` placeholders named `observe_phrase_rejected_on_mcp`, `create_task_rejected_on_sidecar`, `draft_task_rejected_on_mcp`, to be enabled by C2/C3 |
| K1 | existing suite | `cargo test --manifest-path daemon/Cargo.toml` passes with all pre-existing tests unchanged |

Sample proposals for A5–A7: reuse the JSON strings already in the `socket.rs` tests and add one for each remaining kind.

## Out of scope

New proposal kinds, the ledger, tasks, overlay controls, scroll. Do not change MCP tool behavior or the Node gate's limits.

## Done when

- All tests above pass; the ignored stubs exist.
- Manually: run the daemon, `ls -l $XDG_RUNTIME_DIR/pachakutech/` shows three sockets with `srw-------`; `echo '{"kind":"querySpeech","proposalId":"x"}' | socat - UNIX-CONNECT:.../sidecar.sock` replies `not_allowed_for_tier:querySpeech`; the same line on `presence.sock` behaves as before; the MCP server still works end to end.
- One commit (or a short series) titled `daemon: per-endpoint tiers and allow-list gate (C1)`.
