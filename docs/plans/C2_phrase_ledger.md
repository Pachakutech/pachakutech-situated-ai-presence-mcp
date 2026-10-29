# Task C2 — Phrase ledger: event log, view, read tools, first T3 writer

Paste this whole file into your coding agent. It is self-contained. **Requires C1 merged** (tiers, `gate.rs`, three sockets).

## Context

Read first:
- `docs/plans/local_multimodal_reflex.md` section 3 (ledger) and `docs/plans/command_layer.md` sections 4.2–4.3 (data model, imperfect finding)
- `docs/plans/command_layer_protocol.md` sections 2, 5 and test groups B and D
- `daemon/src/registry.rs`, `daemon/src/socket.rs` (`route_presence` is the pattern for a unit-testable router), `daemon/src/gate.rs` (from C1), `src/index.ts` (MCP tools registered with `server.registerTool` and zod), `src/daemonClient.ts`

## Goal

A daemon-owned, append-only ledger of what happened to each phrase on glass, with a derived per-phrase view, readable by every client tier. The **central safety property**: only physical user input (T0) can ever produce `acknowledged`, `approved`, `discarded`, or the clearing of an acknowledgement, and this is enforced by the type system, not by convention.

## Design decisions (settled)

**Events** (`daemon/src/ledger.rs`):
```rust
pub struct LedgerEvent {
    pub seq: u64,                 // strictly increasing from 1, per daemon run
    pub ts_ms: u64,
    pub phrase_id: String,
    pub task_id: Option<String>,  // None until tasks exist (C3)
    pub kind: LedgerKind,         // Located | Viewed | Approved | Discarded | Cleared | Mismatch | Tour
    pub viewed: Option<ViewedTier>, // Shown | Dwelled | Acknowledged (kind = Viewed or Cleared)
    pub matched: Option<MatchInfo>, // { status: Located|Ambiguous|NotFound, conf: f32, ocr_snippet: Option<String> }
    pub uv: Option<UvBox>,
    pub query: Option<String>,
    pub doc_key: Option<String>,
    pub by: Tier,
}
```
Wire names camelCase (`phraseId`, `taskId`, `ocrSnippet`, `docKey`), enum values lowercase, `notFound` for `NotFound`, tiers `"T0".."T3"`.

**View** (one row per phrase, derived from events, never stored independently of them):
`{ phraseId, query, match, viewed: none|shown|dwelled|acknowledged, decision: null|approved|discarded, decisionBy, u, v, w, h, source: { docKey }, taskId, seq }`
- `viewed` = highest tier seen, except a later `Cleared(Acknowledged)` drops it back to the highest non-acknowledged tier seen.
- `decision` = latest `Approved`/`Discarded`; `decisionBy` its tier. Both kinds stay in the event log.
- `match`, `uv`, `query`, `docKey`, `taskId` = latest non-null values.
- `Mismatch` sets `match.status` (from its `MatchInfo`) and never touches `decision`.
- `null` means unset.

**The T0 witness (the type-level guarantee):**
```rust
// daemon/src/ui_input.rs
pub struct T0Witness { _private: () }        // field private to this module: nothing else can construct it
pub enum UiEvent { Acknowledge { phrase_id: String }, Uncheck { phrase_id: String },
                   Approve { phrase_id: String }, Discard { phrase_id: String } }
pub fn apply_ui_event(ledger: &mut Ledger, ev: UiEvent, now_ms: u64) -> Result<u64, LedgerError>
```
- `Ledger` exposes two append paths:
  - `append_observed(&mut self, ev: ObservedAppend) -> Result<u64, LedgerError>` — accepts only `Located`, `Viewed{Shown|Dwelled}`, `Mismatch`, `Tour`. Its input type **cannot represent** `Acknowledged`, `Approved`, `Discarded`, or `Cleared` (use a separate enum, e.g. `ObservedViewed { Shown, Dwelled }`).
  - `append_user(&mut self, _w: T0Witness, ev: UserAppend) -> Result<u64, LedgerError>` — the only way to write `Viewed(Acknowledged)`, `Approved`, `Discarded`, `Cleared(Acknowledged)`; always records `by: T0`.
- Only `ui_input::apply_ui_event` constructs a `T0Witness`. Nothing on any socket path calls it. In this task nothing in production calls it at all (the overlay controls come in C3); tests call it through `#[cfg(test)]`.

**Caps and lifecycle:**
- Phrases per session: 50 distinct `phraseId`s; the 51st new phrase → `LedgerError::Quota`, reply error `quota: phrase cap`.
- Event log cap: 5000. When exceeded, compact: keep, per phrase, the events needed to reproduce the current view exactly (latest of each field, plus the latest decision and acknowledgement events). Record `compacted_below: u64`. A `since` older than `compacted_below` returns the current view rows for affected phrases and `detail.compacted: true, detail.floor: <seq>`.
- Snippets and queries truncated to 200 chars on ingest. No document text beyond that is stored.
- In-memory only. Empty on daemon start (document this).
- Retiring a presence does not clear the ledger in this task (policy decided in C3).
- Phrase ids minted by the daemon as `ph-<n>` when an `observePhrase` arrives without one.

**New proposal kinds** (add to `protocol.rs`; new bodies use newtype variants wrapping structs with `#[serde(deny_unknown_fields)]`, so `"approved": true` or `"viewed":"acknowledged"` are parse errors. Verify with tests that the internally tagged `kind` field is not itself rejected):

| Kind | Body | Tiers (update `gate::allowed`) |
|---|---|---|
| `listPhraseFlags` | `proposalId`, `since?` (u64, exclusive), `taskId?`, `docKey?`, `decidedBy?` ("T0".."T3"), `limit?` (default 50, max 200), `events?` (bool, default false) | T1, T2, T3 |
| `subscribeLedger` | `proposalId`, `since?` | T1, T2, T3 |
| `observePhrase` | `proposalId`, `phraseId?`, `query?`, `docKey?`, `match` (`{status, conf, ocrSnippet?}`), `u? v? w? h?` (all four or none, each in [0,1]), `viewed?` (`shown`/`dwelled` only) | T3 only |

Replies (`ProposalResult::ok` detail):
- `listPhraseFlags`: `{ "seq": <latest>, "rows": [...] }` or with `events: true` `{ "seq": <latest>, "events": [...] }`; plus `compacted`/`floor` when applicable.
- `subscribeLedger`: an immediate ok `{ "seq": <latest> }`, then every subsequent ledger event is written to that connection as its own JSONL line `{ "ledgerEvent": {...} }`, starting after `since` if given (replaying from `since` first). Track `subscribed: Option<u64>` on `Client`; flush pending events in `pump` after reads. A client that falls more than 1000 events behind is sent `{ "ledgerOverflow": { "floor": <seq> } }` and unsubscribed.
- `observePhrase`: `{ "phraseId": "ph-…", "seq": <n> }`.

Routing: add `route_ledger(proposal: &Proposal, tier: Tier, ledger: &mut Ledger, now_ms: u64) -> Option<ProposalResult>` in `socket.rs` (or `ledger.rs`), unit-testable like `route_presence`. The `Ledger` lives in `Registry`.

**MCP (Node):**
- Add tool `listPhraseFlags` in `src/index.ts` (zod schema mirrors the body; returns the detail JSON as text). Through the existing `PolicyGate.assertAllowed` rate limit.
- Do **not** add any MCP tool that writes to the ledger.
- `skills/presence/SKILL.md`: add a short paragraph: the ledger exists; read it with `listPhraseFlags`; `acknowledged` and decisions come only from the user's hands; filter decisions with `decidedBy: "T0"`; `mismatch`/`notFound` mean finding failed, not that the user rejected.

## Tests

Unit tests in `ledger.rs`, `ui_input.rs`, and the `socket.rs` tests module.

| ID | Test name | Asserts |
|---|---|---|
| A1 | enable `observe_phrase_rejected_on_mcp` (stub from C1) | `check(T2Client, observePhrase)` → `not_allowed_for_tier:observePhrase` |
| A7 | extend `allow_list_table_matches_doc` | New kinds per the table above |
| B1 | `observe_cannot_express_acknowledged` | `observePhrase` JSON with `"viewed":"acknowledged"` fails to parse |
| B2 | `observe_rejects_decision_fields` | Bodies with `approved`, `discarded`, `decision`, `checked` fail to parse; a clean body with `kind` + `proposalId` parses |
| B4 | `fuzz_non_t0_paths_never_write_user_kinds` | Generate a few thousand random/mutated `observePhrase`, `listPhraseFlags`, `subscribeLedger` lines; route each through `check` + `route_ledger` for T1, T2, T3; assert the log never contains `Approved`, `Discarded`, `Viewed(Acknowledged)`, or `Cleared`. (A small hand-rolled mutator is fine; no new deps needed.) |
| B5 | `crop_text_saying_approve_is_just_a_snippet` | `observePhrase` with `ocrSnippet: "APPROVE ALL ITEMS"` → only `Located`/`Viewed(shown)` events |
| B6 | `only_ui_input_constructs_witness` | A `compile_fail` doctest on `T0Witness { _private: () }` outside `ui_input`, plus a grep-style test (or a comment-backed review checklist item) that `append_user` is called only from `ui_input.rs` |
| — | `ui_ack_uncheck_roundtrip` | `apply_ui_event(Acknowledge)` → view `acknowledged`, `by T0`; `Uncheck` → `Cleared` event, view back to `dwelled` (or `shown` if that was highest) |
| — | `approve_then_discard_latest_wins_history_kept` | Both events present; view `decision: discarded`, `decisionBy: T0` |
| D1 | `seq_strictly_increasing_gap_free` | |
| D2 | `since_is_exclusive` | |
| D3 | `decided_by_filter` | `decidedBy: T0` returns only phrases whose decision came from T0; `decidedBy: T3` returns none (T3 cannot decide) |
| D4 | `two_clients_same_ledger` | Two routed "clients" (different T2/T3 contexts) see identical rows and `seq` |
| D5 | `limit_default_and_cap` | 50 default, 200 max, 201 → clamped (not an error) |
| D6 | `phrase_cap_quota` | 51st distinct phrase → `quota: phrase cap`; events for existing phrases still accepted |
| D7 | `compaction_preserves_view` | Push past 5000 events; view rows byte-identical before and after; old `since` returns `compacted: true` with `floor` |
| D8 | `subscribe_streams_in_order_and_resumes` | Simulate a subscribed client buffer: events arrive in seq order; reconnect with `since` yields no gaps or duplicates; overflow at >1000 behind emits `ledgerOverflow` |
| D9 | `snippets_truncated` | 500-char snippet stored as 200 |
| D10 | `mismatch_never_sets_decision` | |
| — | `uv_all_or_none_and_bounded` | Partial UV or out-of-range values → `schema` error |
| K1 | existing suite | All pre-existing and C1 tests still pass |

## Out of scope

Tasks, modes, controls rendering, the overlay input region, hotkeys, tours, scroll, `createTask`/`draftTask` (C3+). Return-shape changes to locate/highlight fused MCP tools (ticket A2). Persistence across restarts.

## Done when

- All tests pass with `cargo test --manifest-path daemon/Cargo.toml`; `npm run build` passes.
- Manually: connect to `sidecar.sock`, send an `observePhrase`; connect to `presence.sock`, call `listPhraseFlags` and see the row with `viewed: "shown"` and `by`/`decisionBy` as expected; send the same `observePhrase` on `presence.sock` and get `not_allowed_for_tier`; a `subscribeLedger` connection receives the event live.
- The MCP `listPhraseFlags` tool returns the same rows.
- Commit(s) titled `daemon: phrase ledger, T0 witness, listPhraseFlags (C2)`.
