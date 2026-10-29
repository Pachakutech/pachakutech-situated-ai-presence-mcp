# Task C7 — Project store and persisted project events

Paste this whole file into your coding agent. It is self-contained. **Requires C1 (gate) and C2 (ledger) merged.**

## Context

Read first:
- `docs/plans/research_tours.md` sections 2 (records), 7 (gate additions), 10 (differences from the review), 12 (tickets)
- `docs/plans/reflex-review.md` ("Citation project": the project outlives the tour and a daemon restart; a locus is a hash of content id and span)
- `docs/plans/complete/viewer_find.md` (the old page-notes store under `~/.local/share/pachakutech/pages/` was removed because it steered scrolling; this store must never do that)
- `daemon/src/ledger.rs`, `daemon/src/ui_input.rs`, `daemon/src/gate.rs` (from C1/C2), `daemon/src/protocol.rs`, `daemon/src/socket.rs`, `daemon/Cargo.toml` (note the project's preference for no build-time native dependencies)

## Goal

A durable, daemon-owned **project store**: projects, asks, sources, loci, and the project's own append-only event log. Ledger events that belong to a project are persisted into it at the moment they are written. The project and its keeps survive tour timeout, presence retire and daemon restart. Nothing else in the system reads this store to move a page.

This task builds the store, persistence and read access. Intake rules (C8), text adapters (C9), locus selection (C10), opening sources (C11) and exports (C12) come later and only call the APIs defined here.

## Design decisions (settled)

**Storage format: plain files, no new native dependency.**
```
$PRESENCE_PROJECTS_DIR  (default $XDG_DATA_HOME/pachakutech/projects, else ~/.local/share/pachakutech/projects)
  <projectId>/
    project.json     # project, asks, sources (rewritten atomically: write tmp, fsync, rename)
    loci.jsonl       # append-only locus records (latest record per locusId wins)
    tasks.jsonl      # append-only { taskId, mode, askId?, sourceId?, startedAt }
    events.jsonl     # append-only project events
    export/          # created empty; C12 writes here
```
- Directories `0700`, files `0600`.
- A truncated last line in any `.jsonl` (crash mid-write) is skipped with a log line on load. Any other malformed line fails the load of that project only (logged; other projects load).
- Full-text indexing is **not** in this store. The doc map (C6/C9) will own its own index.
- (Update `research_tours.md` section 2 where it says "a SQLite file plus the exports" to "JSONL files plus the exports; the doc map keeps its own index".)

**Ids.**
- `projectId` = `pj-` + 16 lowercase hex chars (from `sha2` over time, pid and a counter; no new crate needed). `askId` = `ak-<n>`, `sourceId` = `sr-<n>`, per project.
- `locusId` = `lc-` + first 16 hex of `sha256(docKey || 0x00 || spanStart || 0x00 || spanEnd || 0x00 || spanText)`. The same words at the same place in the same content are the same locus.
- **Every id received from a client is validated against its exact pattern before use.** Paths are built only from validated ids, never from any other client string. (A `projectId` of `../../.ssh` must be impossible to turn into a path.)

**Records** (camelCase on the wire and on disk):
```rust
pub struct Project { project_id, title: String /*<=200*/, created_by: Tier, created_at_ms: u64,
                     asks: Vec<Ask>, sources: Vec<Source>, cursor: Cursor, granted_roots: Vec<PathBuf> }
pub struct Ask     { ask_id, text: String /*<=500*/, kind: AskKind /*claim|question*/ }
pub struct Source  { source_id, kind: SourceKind /*web|file*/, reference: String /*url or absolute path*/,
                     doc_key: Option<String>, meta: CitationMeta /*all Option fields*/,
                     status: SourceStatus /*pending|mapped|opened|failed{reason}|done*/ }
pub struct Locus   { locus_id, source_id, ask_id: Option<String>, doc_key: String,
                     span_start: u64, span_end: u64, words: String /*<=1000*/,
                     page: Option<String> /*page label*/, line: Option<u32>, heading_path: Option<Vec<String>>,
                     stance: Option<Stance>, stance_by: Option<Tier>, stance_conf: Option<f32>,
                     match_kind: Option<MatchKind> /*exact|near|none*/, ocr_only: bool,
                     view: Option<UvBox> /*a view, not identity*/ }
pub struct Cursor  { source_index: u32, locus_id: Option<String>, updated_at_ms: u64 }
pub struct ProjectEvent { pseq: u64 /*per project, from 1, survives restart*/, run_id: String, session_seq: u64,
                          ts_ms: u64, kind: LedgerKind, viewed: Option<ViewedTier>, by: Tier,
                          locus_id: Option<String>, ask_id: Option<String>, task_id: Option<String>,
                          phrase_id: String }
```
`CitationMeta` fields: `title, authors: Vec<String>, date, container (journal/site), doi, url, canonical_url, accessed, page_labels: bool, filled_by: Map<field, "source"|"crossref"|"ocr">`. C7 only stores them; C9 fills them. A missing field stays `None`.

**Ledger integration.**
- `LedgerEvent` (C2) gains `project_id: Option<String>`, `ask_id: Option<String>`, `locus_id: Option<String>`.
- The ledger keeps an **association table** `phraseId -> (projectId, locusId, askId)`, set when a project-bound phrase is first observed or created. The T0 path (`ui_input::apply_ui_event`) takes only a `phraseId`, as in C2; the ledger stamps `projectId`/`locusId`/`askId` from the association. **The T0 path never accepts a project id from anyone.**
- `observePhrase` (T3) may carry `projectId` and `locusId`; both must exist (validated id, known project, known locus) or the request fails with `no_such_project` / `no_such_locus`.
- When a ledger event with a `project_id` is appended, the store appends a `ProjectEvent` to that project's `events.jsonl` **before** the append returns success. T0 events (keeps, unchecks, approvals, discards) are `fsync`ed immediately; other events are flushed immediately and `fsync`ed at most once per second. If the project write fails, the ledger append fails (no keep is ever recorded in memory but lost on disk).
- The session ledger itself stays in memory and empty on restart (C2). Project events are not replayed into it. The project derives its own view from `events.jsonl`.

**Keeps (derived per project, per locus).** Using `tasks.jsonl` to know each event's task mode:
- In a **select** task: latest of `Viewed(Acknowledged)` vs `Cleared(Acknowledged)` decides; acknowledged = kept.
- In an **approve** task: latest of `Approved` vs `Discarded` decides; approved = kept.
- In an **explain** task: acknowledgement means "seen" and is **not** a keep.
- Events without a known task mode never make a keep.
- Expose `ProjectStore::keeps(project_id) -> Vec<Keep { locus_id, ask_id, kept_at_ms, via: select|approve }>` and `decisions(project_id) -> Vec<ProjectEvent>` (all T0 decision-type events, for the audit CSV in C12).

**Store API (internal, used by later tickets and tests):**
```rust
impl ProjectStore {
    pub fn open(root: &Path) -> io::Result<Self>;                    // loads all projects; bad ones logged and skipped
    pub fn create(&mut self, by: Tier, title, asks, sources, granted_roots) -> Result<String, StoreError>;
    pub fn add_sources(&mut self, id, sources) -> Result<Vec<String>, StoreError>;
    pub fn set_source_status(&mut self, id, source_id, status) -> Result<(), StoreError>;
    pub fn set_source_meta(&mut self, id, source_id, doc_key, meta) -> Result<(), StoreError>;
    pub fn upsert_locus(&mut self, id, locus: LocusInput) -> Result<String /*locusId*/, StoreError>;
    pub fn register_task(&mut self, id, task_id, mode, ask_id, source_id) -> Result<(), StoreError>;
    pub fn set_cursor(&mut self, id, cursor) -> Result<(), StoreError>;
    pub fn append_event(&mut self, id, ev: &LedgerEvent) -> Result<u64 /*pseq*/, StoreError>; // called by the ledger only
    pub fn get(&self, id) -> Option<&Project>;
    pub fn loci(&self, id, since_pseq: Option<u64>, limit) -> ...;
    pub fn keeps(&self, id) -> Vec<Keep>;
    pub fn decisions(&self, id) -> Vec<ProjectEvent>;
}
```

**Caps:** 100 projects, 500 sources per project, 5000 loci per project, `events.jsonl` at most 50 MB per project. Exceeding any → `quota:<which>`. Asks per project: 20.

**Lifecycle:** a tour timeout, a presence retire and a daemon restart change nothing in the store. Deleting a project is out of scope (the user deletes the directory; document this). There is no "clear".

**Socket kinds in this task** (`gate::allowed` updated; bodies `deny_unknown_fields`):

| Kind | Tiers | Notes |
|---|---|---|
| `createProject` | **T1 only for now**; T3 → stored with `draft: true`, nothing else may use a draft project until a later T0 confirm (C8 builds the confirm) | Sources are stored as `pending` records. **Nothing in C7 fetches, maps, opens or reads a source.** URL and path rules for T2 arrive with C8, which then opens the kind to T2 |
| `queryProject` | T1, T2, T3 | `{ projectId, sincePseq?, limit? (default 100, max 500) }` → `{ project, loci, keeps, cursor, pseq }` |

**MCP (Node):** add a read-only `queryProject` tool. Do not expose `createProject` to MCP yet. Add one line to `skills/presence/SKILL.md`: projects exist, can be read with `queryProject`, and keeps come only from the user's hands.

**Hard rule (from `viewer_find.md`):** nothing under `src/locate/` and no `ViewerMove`/scroll code reads the projects directory. Add a test that enforces this (see P14).

## Steps

1. `daemon/src/project.rs` (new): records, id generation and validation, file layout, atomic `project.json` rewrite, JSONL append with flush/fsync policy, loader with truncated-line tolerance, caps, keeps and decisions derivation.
2. `ledger.rs`: new event fields, association table, call `ProjectStore::append_event` for project-bound events before success; fail the append if persistence fails.
3. `protocol.rs` / `gate.rs`: `createProject`, `queryProject` variants and allow-list rows; the exhaustive match forces the decision.
4. `socket.rs` / `registry.rs`: `ProjectStore` owned by the registry, opened at startup from `PRESENCE_PROJECTS_DIR` or the XDG default; a failure to open logs and runs with projects disabled (`projects_unavailable` errors) rather than stopping the overlay.
5. Node: `queryProject` MCP tool; skill line.
6. Docs: the `research_tours.md` storage sentence; a short "Project store" paragraph in `docs/architecture.md` listing the directory, the files and the "not a scroll guide" rule.

## Tests

All with a temp directory via `PRESENCE_PROJECTS_DIR` (hand-roll a temp dir under `std::env::temp_dir()` with a unique name; no new crate needed).

| ID | Test name | Asserts |
|---|---|---|
| P1 | `create_and_reload` | Create a project with 2 asks and a mixed URL/path list; reopen the store; identical records, order kept |
| P2 | `ids_validated_no_traversal` | `queryProject` with `../x`, `pj-ZZ`, `pj-` + 15 hex, absolute paths → `schema`; no filesystem access attempted |
| P3 | `permissions` | Dirs `0700`, files `0600` |
| P4 | `locus_id_is_content_address` | Same docKey and span → same id; a changed byte in words or offsets → different id; `upsert_locus` twice keeps one locus with the latest fields |
| P5 | `project_events_persist_before_success` | A project-bound `observePhrase` and a T0 keep both appear in `events.jsonl` when the call returns |
| P6 | `t0_events_fsynced` | (Behavioral) after a T0 keep, the bytes are on disk (reopen and read without the store); document that fsync is called |
| P7 | `persistence_failure_fails_the_append` | Make `events.jsonl` unwritable; a T0 keep returns an error and the in-memory ledger has no new event |
| P8 | `t0_path_takes_no_project_id` | `UiEvent` has no project field (type-level); the stamped `projectId` comes from the association table |
| P9 | `observe_with_unknown_project_or_locus` | `no_such_project` / `no_such_locus` |
| P10 | `keeps_by_mode` | Select: ack → kept, uncheck → not kept, re-ack → kept. Approve: approved → kept, then discarded → not kept. Explain: ack → not kept. Unknown task → not kept |
| P11 | `survives_restart_timeout_retire` | (L8) Keeps, loci, cursor and `pseq` intact after reopen; retire and tour expiry change nothing; `pseq` continues from the last value after restart |
| P12 | `truncated_last_line_tolerated` | Append half a line; reload skips it with a log; a malformed middle line fails only that project |
| P13 | `caps` | 101st project, 501st source, 5001st locus, 21st ask → `quota:<which>` |
| P14 | `nothing_in_locate_reads_projects` | A test (Node or a Rust test scanning the repo) that `src/locate/**` and the daemon move/scroll code contain no reference to the projects directory or `PRESENCE_PROJECTS_DIR` |
| P15 | `create_project_tiers` | T1 ok; T2 `not_allowed_for_tier` (until C8); T3 stored as `draft: true`; a draft project rejects `register_task` and `upsert_locus` from non-T0-confirmed paths |
| P16 | `create_project_reads_nothing` | Creating a project with a URL and a path performs no network or file read of those sources (inject a fake fetcher/reader that panics if called) |
| P17 | `query_paging` | `sincePseq` exclusive; `limit` default 100, max 500 |
| P18 | `store_unavailable_is_fail_soft` | Unwritable root → overlay keeps running; project kinds return `projects_unavailable`; C1/C2 behavior unchanged |
| L9 | (from the test list) | Ledger events with `projectId` are persisted when written (covered by P5) |
| L10 | (from the test list) | Same words found twice → same `locusId` (covered by P4) |
| K1 | existing suite | All C1/C2 and pre-existing tests pass |

## Out of scope

Validating URLs and paths, granted-root enforcement beyond storing them, directory snapshots (C8). Fetching, mapping, citation facts (C9). Choosing loci (C10). Opening sources, the multi-source tour runner (C11). Exports (C12). Deleting projects. Any MCP write tool.

## Done when

- All tests pass with `cargo test --manifest-path daemon/Cargo.toml`; `npm run build` passes.
- Manually: on `command.sock`, `createProject` with one URL and one PDF path returns a `projectId`; the directory appears with `0700`/`0600`; `queryProject` on `presence.sock` returns it; restart the daemon and query again with the same result; `createProject` on `presence.sock` returns `not_allowed_for_tier`.
- Commit(s) titled `daemon: project store and persisted project events (C7)`.
