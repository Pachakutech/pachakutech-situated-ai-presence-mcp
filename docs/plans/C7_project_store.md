# C7 — project store and persisted project events

Implementation brief. C1, C2, C3, C3b, and C3c have landed (`8a851c0`, `752d77b`, `79bd44c`, `011d4dd`). C4, C5, and C6 are specified and not required here. C8 through C13 call the API this ticket defines. They are not built in this ticket.

This file replaces the earlier paste-brief that required only C1 and C2. The records, the caps, and the durability rule are unchanged. The rows below were numbered P in that draft. They are S here, because P is the ask brief in `C4_C5_C6_command_report_map.md`.

Read first:

- `docs/plans/research_tours.md` sections 2, 7, 10, and 12
- `docs/plans/reflex-review.md` ("Citation project")
- `docs/plans/complete/viewer_find.md` (the old page-notes store under `~/.local/share/pachakutech/pages/` was removed because it steered scrolling)
- `docs/plans/C4_C5_C6_command_report_map.md` (the map is a different directory; this store does not become that map)
- `daemon/src/ledger.rs`, `daemon/src/ui_input.rs`, `daemon/src/gate.rs`, `daemon/src/protocol.rs`, `daemon/src/socket.rs`, `daemon/src/task.rs`
- `daemon/Cargo.toml` (no new crate; `sha2` is already a dependency)

## Goal

A durable, daemon-owned project store: projects, asks, sources, loci, and the project's own append-only event log. A ledger event that belongs to a project is written into that log before the ledger append returns success. The project, its loci, and its keeps survive tour timeout, presence retire, and a daemon restart. Nothing in this store moves a page.

C7 is the store and the read. Intake checks are C8. Adapters and citation facts are C9. Choosing loci is C10. Opening sources is C11. Export files are C12. MCP tools and skill text are C13.

## Decisions that bind this ticket

1. **Plain files, no new dependency.** `sha2` mints ids. SQLite is not added. `research_tours.md` section 2 already names these files. Do not edit that sentence.

```text
$PRESENCE_PROJECTS_DIR
  default $XDG_DATA_HOME/pachakutech/projects
  else ~/.local/share/pachakutech/projects
  <projectId>/
    project.json     # project, asks, sources. Write tmp, fsync, rename.
    loci.jsonl       # append-only. The latest record for a locusId wins.
    tasks.jsonl      # append-only { taskId, mode, askId?, sourceId?, startedAt }
    events.jsonl     # append-only project events
    export/          # created empty. C12 writes here.
```

Directories `0700`, files `0600`. A truncated last line in a `.jsonl` is skipped and logged. Any other malformed line fails the load of that project only. Other projects load.

2. **Ids are the only path components a client can influence.** `projectId` is `pj-` plus 16 lowercase hex digits, minted from `sha2` over time, pid, and a counter. `askId` is `ak-<n>`. `sourceId` is `sr-<n>`. `locusId` is `lc-` plus the first 16 hex digits of `sha256(docKey || 0x00 || spanStart || 0x00 || spanEnd || 0x00 || spanText)`. The same words at the same place in the same content are the same locus. A client id is matched against its pattern before it is used. A `projectId` of `../../.ssh` is `schema`, and no path is joined.

3. **The session ledger stays in memory.** A restart leaves it empty and does not replay `events.jsonl` into it. The project derives keeps from its own files. The in-memory phrase association is empty after a restart. A later ticket binds a phrase again when it shows the locus. C7 does not rebuild that table from disk.

4. **T0 still takes only a phrase id.** `UiEvent` grows no project field. `append_user` stamps `projectId`, `locusId`, and `askId` from the association table. A phrase with no row is stamped with none of them, and no project file is touched.

5. **`createTask` grows no project fields.** Binding a task to a project is C11, through `register_task` on the store API. C7 does not change the task wire.

6. **T2 cannot create a project yet.** The protocol table is the end state. Until C8, `createProject` is T1, and T3 stored as a draft. T2 is `not_allowed_for_tier:createProject`. C8 is the ticket that opens T2 and enforces URL and path rules.

7. **A draft is stored and unused.** T3 `createProject` sets `draft: true`. `queryProject` may read it. `register_task` and `upsert_locus` return `draft_unconfirmed` and write nothing. The T0 confirm that clears the flag is C8. This draft is not a task `draft` and does not enter the task book.

8. **Creating a project reads nothing.** Sources are stored as `pending` records: a URL string, a path string, or a directory string. No fetch, no directory listing, no `pdftotext`, no doc-map read or write. `doc_key` stays empty. Citation meta stays empty. C9 fills both.

9. **The move path and the locator do not read this directory.** `planned_for`, `move_fits`, and `authorize` do not name `ProjectStore` or `PRESENCE_PROJECTS_DIR`. `src/locate/*.ts` does not either. The doc-map directory from the C6 brief is a different path, and this ticket does not open it.

10. **`samples()` stays the sixteen pre-C2 kinds.** Add `project_samples()` and chain it beside `task_samples()` in `t0_has_no_socket_kinds` and `allow_list_table_matches_doc`. Do not fold the new kinds into the T2 or-pattern. Do not add `deny_unknown_fields` on the `Proposal` enum. The new bodies have `deny_unknown_fields`, plus the ignored `kind` field the other bodies use.

11. **The MCP surface waits for C13.** `queryProject` and `createProject` are socket kinds. This ticket does not edit `src/index.ts`, `src/daemon.ts`, or either presence skill. `DaemonClient` stays one reply per proposal.

12. **The live daemon is not restarted.** Restart is an in-process reopen of a temp store. The viseme failure `GLB lacks viseme_aa` is left in place. Tracked `dist` is not part of the commit.

## Records

CamelCase on the wire and on disk.

```rust
pub struct Project {
    project_id, title: String, /* <= 200 */
    created_by: Tier, created_at_ms: u64, draft: bool,
    asks: Vec<Ask>, sources: Vec<Source>,
    cursor: Cursor, granted_roots: Vec<String>,
}
pub struct Ask    { ask_id, text: String, /* <= 500 */ kind: AskKind /* claim | question */ }
pub struct Source { source_id, kind: SourceKind, /* web | file | dir */
                    reference: String, doc_key: Option<String>,
                    meta: CitationMeta, status: SourceStatus }
pub struct Locus  { locus_id, source_id, ask_id: Option<String>, doc_key: String,
                    span_start: u64, span_end: u64, words: String, /* <= 1000 */
                    page: Option<String>, line: Option<u32>,
                    heading_path: Option<Vec<String>>,
                    stance: Option<Stance>, stance_by: Option<Tier>, stance_conf: Option<f32>,
                    match_kind: Option<MatchKind>, /* exact | near | none */
                    ocr_only: bool, view: Option<UvBox> }
pub struct Cursor { source_index: u32, locus_id: Option<String>, updated_at_ms: u64 }
pub struct ProjectEvent {
    pseq: u64,          /* per project, from 1, survives restart */
    run_id: String,     /* minted once at daemon start */
    session_seq: u64,   /* the ledger seq, which resets on restart */
    ts_ms: u64, kind: LedgerKind, viewed: Option<ViewedTier>, by: Tier,
    locus_id: Option<String>, ask_id: Option<String>,
    task_id: Option<String>, phrase_id: String,
}
```

`SourceStatus` is `pending | mapped | opened | failed | done`. C7 writes `pending` only. A failed status carries a reason string when a later ticket sets it.

`CitationMeta` fields, all optional, all empty in C7: `title`, `authors`, `date`, `container`, `doi`, `url`, `canonical_url`, `accessed`, `page_labels`, `filled_by`. C9 fills them. A missing field stays absent. Nothing is guessed.

`Stance` is `supports | contrary | mentions | unknown`. C7 stores the field. C10 is the ticket that sets it. Stance never creates a keep.

## Ledger

`LedgerEvent` gains `project_id`, `ask_id`, and `locus_id`, each `Option`. `push` takes them and passes `None` at every existing call. The wire object includes each field only when it is set, the same rule as `dwellMs`. The derived row does the same. Existing tests that read a body without those fields stay valid.

The association table is `phraseId -> (projectId, locusId, askId)`. `observePhrase` may carry `projectId`, `locusId`, and `askId`.

- All three absent: today's observe path. No association, no project write.
- `projectId` set and `locusId` missing, or `askId` set without `projectId`: `schema`. No write.
- `projectId` or `locusId` unknown, or `askId` not an ask of that project: `no_such_project`, `no_such_locus`, or `no_such_ask`. No write.
- Both known: associate the phrase, then append. A project-bound observe whose project write fails appends no ledger event.

`append_event` runs before the ledger append returns success. T0 events (acknowledge, clear, approve, discard) are `fsync`ed before that return. Other project events are flushed at once and `fsync`ed at most once a second. The `fsync` stays on the caller. A keep must not be reported in memory and missing on disk. If `events.jsonl` cannot be written, the ledger append returns the error and the in-memory log is unchanged.

A phrase cap, a task queue cap, and a doc-map cap are unchanged. Project caps are separate: 100 projects, 20 asks, 500 sources, 5000 loci, and 50 MB of `events.jsonl`. Exceeding one returns `quota:projects`, `quota:asks`, `quota:sources`, `quota:loci`, or `quota:events`, and writes nothing for that call.

## Keeps

Derived per project, per locus, from `events.jsonl` and the task mode in `tasks.jsonl`.

- Select: the later of `Viewed(Acknowledged)` and `Cleared(Acknowledged)`. Acknowledged is kept.
- Approve: the later of `Approved` and `Discarded`. Approved is kept.
- Explain: an acknowledgement is seen. It is not a keep.
- An event whose task has no mode in `tasks.jsonl` is not a keep.

```rust
ProjectStore::keeps(project_id) -> Vec<Keep { locus_id, ask_id, kept_at_ms, via: select|approve }>
ProjectStore::decisions(project_id) -> Vec<ProjectEvent>  /* T0 acknowledge, clear, approve, discard */
```

C12 reads these. C7 does not write an export.

## Store API

Internal. Tests and later tickets call it. C7's sockets are only `createProject` and `queryProject`.

```rust
impl ProjectStore {
    pub fn open(root: &Path) -> io::Result<Self>;
    pub fn create(&mut self, by: Tier, title, asks, sources, granted_roots) -> Result<String, StoreError>;
    pub fn add_sources(&mut self, id, sources) -> Result<Vec<String>, StoreError>;
    pub fn set_source_status(&mut self, id, source_id, status) -> Result<(), StoreError>;
    pub fn set_source_meta(&mut self, id, source_id, doc_key, meta) -> Result<(), StoreError>;
    pub fn upsert_locus(&mut self, id, locus: LocusInput) -> Result<String, StoreError>;
    pub fn register_task(&mut self, id, task_id, mode, ask_id, source_id) -> Result<(), StoreError>;
    pub fn set_cursor(&mut self, id, cursor) -> Result<(), StoreError>;
    pub fn append_event(&mut self, id, ev: &LedgerEvent) -> Result<u64, StoreError>;
    pub fn get(&self, id) -> Option<&Project>;
    pub fn loci(&self, id, since_pseq: Option<u64>, limit) -> Result<Vec<Locus>, StoreError>;
    pub fn keeps(&self, id) -> Result<Vec<Keep>, StoreError>;
    pub fn decisions(&self, id) -> Result<Vec<ProjectEvent>, StoreError>;
}
```

`append_event` is called from the ledger only. A direct test may call it through a ledger append.

A project is not deleted by a socket kind. The user deletes the directory. Document that in the architecture paragraph. There is no clear.

## Sockets

Update `Proposal::proposal_id`, `kind_name`, `gate::allowed`, and the final match in `socket.rs`. Route beside `route_task`.

| Kind | Tiers | Body |
|---|---|---|
| `createProject` | T1 creates an active project. T3 creates `draft: true`. T2 is refused. | `proposalId`, `title`, `asks: [{ text, kind }]`, `sources: [{ url } \| { path } \| { dir }]`, optional `grantedRoots: [string]`. No `presenceId`. |
| `queryProject` | T1, T2, T3 | `proposalId`, `projectId`, optional `sincePseq`, optional `limit` (default 100, max 500). Reply: `project`, `loci`, `keeps`, `cursor`, `pseq`. |

`sincePseq` is exclusive. A reference string over 2000 characters is `schema`. Title and ask caps are the record caps. Unknown fields are `schema`.

The registry owns the store, opened at startup. A root that cannot be opened is logged. The overlay still runs. Project kinds then return `projects_unavailable`. `observePhrase` without a project id still works. `observePhrase` with a project id returns `projects_unavailable` and writes nothing.

## Architecture paragraph

Add to `docs/architecture.md`, after the phrase-ledger section:

The project store is files under `$PRESENCE_PROJECTS_DIR` (default `$XDG_DATA_HOME/pachakutech/projects`, else `~/.local/share/pachakutech/projects`): `project.json`, `loci.jsonl`, `tasks.jsonl`, `events.jsonl`, and an empty `export/`. A ledger event that names a project is appended there before the call returns. The session ledger is still empty after a restart. The project remains. The store is not a scroll guide, and the locator does not read it. Deleting a project is deleting that directory.

## Tests

Use a temp directory under `std::env::temp_dir()` with a unique name. No new crate.

| ID | Test | Asserts |
|---|---|---|
| S1 | `create_and_reload` | Two asks and a mixed URL, path, and dir list. Reopen. The same records, in the same order. Nothing was fetched or listed. |
| S2 | `ids_validated_no_traversal` | `queryProject` with `../x`, `pj-ZZ`, `pj-` plus 15 hex digits, or an absolute path as the id → `schema`. No path outside the root is created. |
| S3 | `permissions` | Directories `0700`, files `0600`. |
| S4 | `locus_id_is_content_address` | The same docKey and span mint one id. A changed word or offset mints another. A second upsert keeps one locus and the later fields. |
| S5 | `project_events_persist_before_success` | A project-bound `observePhrase` and a T0 keep are both in `events.jsonl` when the call returns. |
| S6 | `t0_events_fsynced` | After a T0 keep, a new process reading the file sees the line. The test documents that `fsync` ran. |
| S7 | `persistence_failure_fails_the_append` | `events.jsonl` unwritable. A T0 keep returns an error. The in-memory ledger has no new event. |
| S8 | `t0_path_takes_no_project_id` | `UiEvent` has no project field. The stamped id comes from the association. |
| S9 | `observe_with_unknown_project_or_locus` | `no_such_project`, `no_such_locus`, `no_such_ask`. A half-set body is `schema`. |
| S10 | `keeps_by_mode` | Select: ack kept, uncheck dropped, ack kept again. Approve: approved kept, discarded dropped. Explain: ack is not a keep. An unknown task is not a keep. |
| S11 | `survives_restart_timeout_retire` | Reopen keeps loci, keeps, cursor, and `pseq`. Retire and tour expiry change no file. The next `pseq` continues. The in-memory ledger and the association are empty in the new store. |
| S12 | `truncated_last_line_tolerated` | A half line at the end is skipped. A malformed earlier line fails only that project. |
| S13 | `caps` | The 101st project, 21st ask, 501st source, 5001st locus → `quota:<which>`. |
| S14 | `nothing_in_locate_or_moves_reads_projects` | `src/locate/*.ts`, `planned_for`, `move_fits`, and `authorize` do not name `PRESENCE_PROJECTS_DIR` or `ProjectStore`. |
| S15 | `create_project_tiers` | T1 ok. T2 `not_allowed_for_tier:createProject`. T3 stored `draft: true`. Draft `register_task` and `upsert_locus` return `draft_unconfirmed`. |
| S16 | `create_project_reads_nothing` | A URL and a path are stored. A stand-in reader panics if called, and the test does not panic. |
| S17 | `query_paging` | `sincePseq` exclusive. `limit` defaults to 100 and caps at 500. |
| S18 | `store_unavailable_is_fail_soft` | An unwritable root leaves the overlay path running. Project kinds return `projects_unavailable`. An observe with no project id still writes a ledger event. |
| S19 | `wire_omits_empty_project_fields` | A ledger event with no project id has no `projectId`, `askId`, or `locusId` key. One with a project id has those keys. |
| S20 | `create_task_body_has_no_project_field` | A `projectId` on `createTask` is `schema`. |

Protocol rows L8, L9, and L10 are S11, S5, and S4. L1 through L7 and L11 through L12 belong to C8 and C9.

`cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon --offline`. The viseme failure stays. This ticket does not change TypeScript, so `npm run build` is not required. If a TypeScript file is touched anyway, restore tracked `dist` and leave it uncommitted.

## Out of scope

URL and path checks, granted-root enforcement beyond storing the strings, directory snapshots (C8). Fetching, mapping, citation facts (C9). Choosing loci and stance (C10). `openSource` and the multi-source runner (C11). Any file under `export/` except the empty directory (C12). MCP tools and skill text (C13). Notes (C7b). Deleting a project. A live daemon restart.

## Done when

S1 through S20 pass. The architecture paragraph is in `docs/architecture.md`. Commit title: `daemon: project store and persisted project events (C7)`. The live daemon is not restarted.
