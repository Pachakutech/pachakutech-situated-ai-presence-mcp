//! Project tour. `OpenSource` is an internal command. It is not a socket kind.
//! The locate modules open the window this module names. This module does not
//! read a screen buffer and does not spawn a viewer.

use crate::gate::Tier;
use crate::ledger::Ledger;
use crate::project::{Cursor, Locus, MatchKind, Project, ProjectStore, Source, SourceKind, SourceStatus, StoreError};
use crate::protocol::{ProposalResult, ReportSourceOpenBody, StartProjectTourBody, TaskMode, TaskPace};
use crate::task::{TaskBook, TourSeed, TourSeedItem, ViewerMove, MOVE_WAIT_MS};
use crate::ui_input::{Hotkey, UiEvent};
use serde_json::{json, Value};
use std::path::Path;

pub const LOCATE_PORT: u16 = 9222;
pub const OPEN_CAP: u32 = 50;

pub fn locate_profile() -> String {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    format!("{runtime}/pachakutech/locate-chromium")
}

/// The only opens a project tour can ask for. There is no follow, click, or key variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenSource {
    Open { project_id: String, source_id: String },
    Close { project_id: String, source_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenSpec {
    Web { url: String, profile: String, port: u16 },
    Pdf { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenFault {
    NoOpener,
    Failed(String),
}

pub trait SourceOpener {
    fn open(&mut self, spec: &OpenSpec) -> Result<(), OpenFault>;
    fn close(&mut self);
    /// `Ok` from `open` parks the spec. It does not mark the source opened.
    fn parks(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub enum OpenOutcome {
    Opened,
    Closed,
    Waiting,
    /// The spec is on the desk. The source status is unchanged.
    Parked,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Awaiting,
    Active,
    Stopped,
}

struct Active {
    project_id: String,
    ask_id: Option<String>,
    mode: TaskMode,
    pace: TaskPace,
    origin_user: bool,
    phase: Phase,
    /// Source ids captured when the tour was listed. A source added later is not on this list.
    source_ids: Vec<String>,
    names: Vec<String>,
    opens: u32,
    last_open_ms: Option<u64>,
    open_source: Option<String>,
    task_id: Option<String>,
    /// Locus ids in the task inserted for this tour. The next chunk starts after the last one.
    chunk: Vec<String>,
    /// One listed source waiting for the locate process. Absent once that process reports.
    park: Option<ParkedOpen>,
}

struct ParkedOpen {
    source_id: String,
    spec: OpenSpec,
}

#[derive(Default)]
pub struct TourDesk {
    active: Option<Active>,
}

pub struct AllowOpener;

impl SourceOpener for AllowOpener {
    fn open(&mut self, _spec: &OpenSpec) -> Result<(), OpenFault> {
        Ok(())
    }
    fn close(&mut self) {}
}

/// Production opener. `Ok` stores the spec. The source stays at its current status.
pub struct ParkOpener;

impl SourceOpener for ParkOpener {
    fn open(&mut self, _spec: &OpenSpec) -> Result<(), OpenFault> {
        Ok(())
    }
    fn close(&mut self) {}
    fn parks(&self) -> bool {
        true
    }
}

pub(crate) enum Planned {
    Finished(ProposalResult),
    Insert { proposal_id: String, seed: TourSeed, detail: Value },
}

pub(crate) fn route_start(body: &StartProjectTourBody, tier: Tier, store: &mut ProjectStore, desk: &mut TourDesk, now_ms: u64) -> Planned {
    let mut opener = ParkOpener;
    plan(body, tier, store, desk, &mut opener, now_ms)
}

pub(crate) fn commit(
    planned: Planned,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    now_ms: u64,
) -> ProposalResult {
    match planned {
        Planned::Finished(result) => result,
        Planned::Insert { proposal_id, seed, mut detail } => {
            let locus_ids = seed.locus_ids.clone();
            match book.insert_tour_chunk(ledger, seed, now_ms) {
                Ok(id) => match associate_chunk(ledger, desk, store, &id, &locus_ids, now_ms) {
                    Ok(()) => {
                        remember_chunk(desk, id.clone(), locus_ids);
                        detail["taskId"] = json!(id);
                        ProposalResult::ok(&proposal_id, Some(detail))
                    }
                    Err(wire) => {
                        book.end_without_decision(&id);
                        ProposalResult::err(&proposal_id, wire)
                    }
                },
                Err(err) => ProposalResult::err(&proposal_id, task_wire(err)),
            }
        }
    }
}

/// Holds are already done. Bind each locus, then register the task. A failure drops the new bindings.
pub(crate) fn associate_chunk(
    ledger: &mut Ledger,
    desk: &TourDesk,
    store: &mut ProjectStore,
    task_id: &str,
    locus_ids: &[String],
    now_ms: u64,
) -> Result<(), &'static str> {
    let Some(active) = desk.active.as_ref() else {
        return Err("schema");
    };
    let source = active.open_source.clone().filter(|id| !id.is_empty()).ok_or("schema")?;
    let project_id = active.project_id.clone();
    let ask_id = active.ask_id.clone();
    let mode = match active.mode {
        TaskMode::Explain => "explain",
        TaskMode::Select => "select",
        TaskMode::Approve => "approve",
    };
    let mut planted = Vec::new();
    for locus in locus_ids {
        match ledger.bind_shown_locus(locus, &project_id, locus, ask_id.clone()) {
            Ok(plant) => planted.push(plant),
            Err(_) => {
                ledger.undo_shown(&planted);
                return Err("schema");
            }
        }
    }
    if let Err(err) = store.register_task(&project_id, task_id, mode, ask_id.as_deref(), Some(source.as_str()), now_ms) {
        ledger.undo_shown(&planted);
        return Err(err.as_wire());
    }
    Ok(())
}

pub fn start(
    body: &StartProjectTourBody,
    tier: Tier,
    store: &mut ProjectStore,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    desk: &mut TourDesk,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) -> ProposalResult {
    let planned = plan(body, tier, store, desk, opener, now_ms);
    commit(planned, book, ledger, desk, store, now_ms)
}

pub fn phase_name(desk: &TourDesk) -> Option<&'static str> {
    desk.active.as_ref().map(|active| match active.phase {
        Phase::Awaiting => "awaitingStart",
        Phase::Active => "active",
        Phase::Stopped => "stopped",
    })
}

pub fn task_id(desk: &TourDesk) -> Option<&str> {
    desk.active.as_ref().and_then(|active| active.task_id.as_deref())
}

pub fn tour_project(desk: &TourDesk) -> Option<&str> {
    desk.active.as_ref().map(|active| active.project_id.as_str())
}

/// The parked spec for this project, in the `queryProject` shape. Absent when nothing is parked.
pub fn open_request_for(desk: &TourDesk, project_id: &str) -> Option<Value> {
    let active = desk.active.as_ref()?;
    if active.project_id != project_id {
        return None;
    }
    let park = active.park.as_ref()?;
    Some(match &park.spec {
        OpenSpec::Web { url, profile, port } => json!({
            "sourceId": park.source_id,
            "kind": "web",
            "reference": url,
            "profile": profile,
            "port": port,
        }),
        OpenSpec::Pdf { path } => json!({
            "sourceId": park.source_id,
            "kind": "pdf",
            "reference": path,
        }),
    })
}

pub(crate) fn remember_chunk(desk: &mut TourDesk, task_id: String, locus_ids: Vec<String>) {
    if let Some(active) = desk.active.as_mut() {
        active.task_id = Some(task_id);
        active.chunk = locus_ids;
    }
}

pub fn apply(
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    cmd: OpenSource,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) -> Result<OpenOutcome, String> {
    let (OpenSource::Open { project_id, source_id } | OpenSource::Close { project_id, source_id }) = &cmd;
    let Some(active) = desk.active.as_ref() else {
        return Err("no_active_project_tour".into());
    };
    if active.phase != Phase::Active || active.project_id != *project_id {
        return Err("no_active_project_tour".into());
    }
    let project = store.get(project_id).cloned().ok_or_else(|| StoreError::NoSuchProject.as_wire().to_string())?;
    if !project.sources.iter().any(|source| source.source_id == *source_id) {
        return Err("source_not_in_project".into());
    }
    match cmd {
        OpenSource::Close { source_id, .. } => {
            if desk.active.as_ref().and_then(|active| active.open_source.clone()).as_deref() == Some(source_id.as_str()) {
                opener.close();
                if let Some(active) = desk.active.as_mut() {
                    active.open_source = None;
                }
            }
            Ok(OpenOutcome::Closed)
        }
        OpenSource::Open { source_id, .. } => open_source(desk, store, &source_id, opener, now_ms),
    }
}

pub fn on_hotkey(
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    opener: &mut dyn SourceOpener,
    hotkey: Hotkey,
    now_ms: u64,
) -> bool {
    match hotkey {
        Hotkey::Start => {
            if book.has_awaiting_start() {
                return book.handle_ui(ledger, UiEvent::Hotkey(Hotkey::Start), now_ms);
            }
            let awaiting = desk.active.as_ref().is_some_and(|active| active.phase == Phase::Awaiting);
            if !awaiting {
                return book.handle_ui(ledger, UiEvent::Hotkey(Hotkey::Start), now_ms);
            }
            confirm_listed(desk, store, book, ledger, opener, now_ms);
            true
        }
        Hotkey::NextSource => move_source(desk, store, book, opener, 1, now_ms),
        Hotkey::PrevSource => move_source(desk, store, book, opener, -1, now_ms),
        _ => false,
    }
}

/// The cursor moves to the following highlight locus. A locus before that cursor is already left.
pub fn leave_current_locus(store: &mut ProjectStore, project_id: &str, now_ms: u64) -> Result<(), String> {
    let project = store.get(project_id).cloned().ok_or_else(|| StoreError::NoSuchProject.as_wire().to_string())?;
    let index = project.cursor.source_index as usize;
    let Some(source) = project.sources.get(index) else {
        return Ok(());
    };
    let rows = highlight_loci(store, project_id, &source.source_id, None)?;
    let next = if let Some(id) = project.cursor.locus_id.as_deref() {
        rows.iter().position(|locus| locus.locus_id == id).and_then(|pos| rows.get(pos + 1))
    } else {
        rows.first()
    };
    if let Some(locus) = next {
        return write_cursor(store, project_id, project.cursor.source_index, Some(locus.locus_id.clone()), now_ms);
    }
    if let Some(later) = project.sources.iter().enumerate().skip(index + 1).find(|(_, source)| source.status != SourceStatus::Failed) {
        let rows = highlight_loci(store, project_id, &later.1.source_id, None)?;
        let locus = rows.first().map(|locus| locus.locus_id.clone());
        return write_cursor(store, project_id, later.0 as u32, locus, now_ms);
    }
    Ok(())
}

/// WebFind or PdfPage from the locus the adapter stored. A `none` locus has no move.
pub fn move_for_locus(source: &Source, locus: &Locus) -> Option<ViewerMove> {
    if matches!(locus.match_kind, Some(MatchKind::None)) || locus.words.is_empty() {
        return None;
    }
    let doc_key = locus.doc_key.clone();
    if source.kind == SourceKind::File && source.reference.to_ascii_lowercase().ends_with(".pdf") {
        Some(ViewerMove::PdfPage { doc_key, page: page_number(locus).unwrap_or(1) })
    } else if source.kind == SourceKind::Web {
        Some(ViewerMove::WebFind { doc_key, opening_phrase: opening_words(&locus.words) })
    } else {
        None
    }
}

fn plan(
    body: &StartProjectTourBody,
    tier: Tier,
    store: &mut ProjectStore,
    desk: &mut TourDesk,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) -> Planned {
    if !matches!(tier, Tier::T1User | Tier::T2Client) {
        return Planned::Finished(ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:startProjectTour"));
    }
    let Some(project) = store.get(&body.project_id).cloned() else {
        return Planned::Finished(ProposalResult::err(&body.proposal_id, "no_such_project"));
    };
    if project.draft {
        return Planned::Finished(ProposalResult::err(&body.proposal_id, "draft_unconfirmed"));
    }
    if let Some(ask_id) = &body.ask_id {
        if !project.asks.iter().any(|ask| ask.ask_id == *ask_id) {
            return Planned::Finished(ProposalResult::err(&body.proposal_id, "no_such_ask"));
        }
    }
    let listed: Vec<&Source> = project.sources.iter().filter(|source| source.status != SourceStatus::Failed).collect();
    let source_ids: Vec<String> = listed.iter().map(|source| source.source_id.clone()).collect();
    let names: Vec<String> = listed.iter().map(|source| list_name(source)).collect();
    let pace = body.pace.unwrap_or(match body.mode {
        TaskMode::Explain => TaskPace::Flow,
        TaskMode::Select | TaskMode::Approve => TaskPace::Wait,
    });
    desk.active = Some(Active {
        project_id: body.project_id.clone(),
        ask_id: body.ask_id.clone(),
        mode: body.mode,
        pace,
        origin_user: tier == Tier::T1User,
        phase: Phase::Awaiting,
        source_ids,
        names: names.clone(),
        opens: 0,
        last_open_ms: None,
        open_source: None,
        task_id: None,
        chunk: Vec::new(),
        park: None,
    });
    if tier == Tier::T2Client {
        return Planned::Finished(ProposalResult::ok(
            &body.proposal_id,
            Some(json!({
                "projectId": body.project_id,
                "state": "awaitingStart",
                "sources": names,
            })),
        ));
    }
    activate(body.proposal_id.clone(), store, desk, opener, now_ms)
}

fn confirm_listed(
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) {
    let Some(proposal_id) = desk.active.as_ref().map(|_| "hotkey-start".to_string()) else {
        return;
    };
    let planned = activate(proposal_id, store, desk, opener, now_ms);
    let _ = commit(planned, book, ledger, desk, store, now_ms);
}

fn activate(
    proposal_id: String,
    store: &mut ProjectStore,
    desk: &mut TourDesk,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) -> Planned {
    let Some(active) = desk.active.as_mut() else {
        return Planned::Finished(ProposalResult::err(&proposal_id, "no_active_project_tour"));
    };
    active.phase = Phase::Active;
    let project_id = active.project_id.clone();
    let names = active.names.clone();
    let source_ids = active.source_ids.clone();
    let ask_id = active.ask_id.clone();
    let Some(project) = store.get(&project_id).cloned() else {
        return Planned::Finished(ProposalResult::err(&proposal_id, "no_such_project"));
    };
    let start_at = project.cursor.source_index as usize;
    let mut spoken: Option<String> = None;
    let mut opened: Option<String> = None;
    let mut waiting = false;
    for source_id in source_ids {
        let Some(source) = project.sources.iter().find(|source| source.source_id == source_id) else {
            continue;
        };
        let index = project.sources.iter().position(|item| item.source_id == source_id).unwrap_or(0);
        if index < start_at {
            continue;
        }
        if waiting {
            break;
        }
        match open_source(desk, store, &source_id, opener, now_ms) {
            Ok(OpenOutcome::Waiting) => {
                waiting = true;
                opened = Some(source_id);
                break;
            }
            Ok(OpenOutcome::Parked) => {
                waiting = true;
                break;
            }
            Ok(OpenOutcome::Closed) => {}
            Ok(OpenOutcome::Opened) => {
                opened = Some(source_id.clone());
                let cursor_locus = if index == start_at { project.cursor.locus_id.clone() } else { None };
                let rows = match highlight_loci(store, &project_id, &source_id, ask_id.as_deref()) {
                    Ok(rows) => rows,
                    Err(message) => return Planned::Finished(ProposalResult::err(&proposal_id, message)),
                };
                let rows = from_cursor(rows, cursor_locus.as_deref());
                if rows.is_empty() {
                    let _ = write_cursor(store, &project_id, next_index(&project, index), None, now_ms);
                    continue;
                }
                let first = &rows[0];
                let _ = write_cursor(store, &project_id, index as u32, Some(first.locus_id.clone()), now_ms);
                let seed = seed_for(desk, source, &rows);
                let mut detail = json!({
                    "projectId": project_id,
                    "state": "active",
                    "sources": names,
                    "opened": source_id,
                });
                if let Some(line) = spoken.clone() {
                    detail["spoken"] = json!(line);
                }
                return Planned::Insert { proposal_id, seed, detail };
            }
            Err(message) if message == "quota:opens" => {
                return Planned::Finished(ProposalResult::err(&proposal_id, message));
            }
            Err(message) => {
                spoken = Some(spoken_line(&message));
                let _ = store.set_source_status(&project_id, &source_id, SourceStatus::Failed, Some(message));
                continue;
            }
        }
    }
    let mut detail = json!({
        "projectId": project_id,
        "state": "active",
        "sources": names,
    });
    if let Some(id) = opened {
        detail["opened"] = json!(id);
    }
    if waiting {
        detail["waiting"] = json!(true);
    }
    if let Some(line) = spoken {
        detail["spoken"] = json!(line);
    }
    Planned::Finished(ProposalResult::ok(&proposal_id, Some(detail)))
}

fn seed_for(desk: &TourDesk, source: &Source, rows: &[Locus]) -> TourSeed {
    let active = desk.active.as_ref().expect("tour");
    let pdf = source.kind == SourceKind::File && source.reference.to_ascii_lowercase().ends_with(".pdf");
    let chosen: Vec<&Locus> = rows.iter().take(12).collect();
    let items = chosen
        .iter()
        .map(|locus| TourSeedItem {
            query: opening_words(&locus.words),
            narration: if locus.match_kind == Some(MatchKind::Near) {
                locus.narration.clone().unwrap_or_default()
            } else {
                String::new()
            },
            page: if pdf { page_number(locus) } else { None },
        })
        .collect();
    let locus_ids = chosen.iter().map(|locus| locus.locus_id.clone()).collect();
    TourSeed {
        origin_user: active.origin_user,
        mode: active.mode,
        pace: active.pace,
        doc_key: chosen.first().map(|locus| locus.doc_key.clone()),
        items,
        locus_ids,
    }
}

/// The next chunk after a finished one. The caller stores the returned ids on the desk.
/// A second call with the same chunk returns the same seed, so the chunk must be replaced after insert.
pub(crate) fn plan_follow(
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) -> Option<TourSeed> {
    let (project_id, ask_id, source_ids, last) = {
        let active = desk.active.as_ref()?;
        if active.phase != Phase::Active || active.chunk.is_empty() {
            return None;
        }
        (active.project_id.clone(), active.ask_id.clone(), active.source_ids.clone(), active.chunk.last().cloned()?)
    };
    let mut owner_at = None;
    let mut same_source = None;
    for (pos, source_id) in source_ids.iter().enumerate() {
        let Ok(rows) = highlight_loci(store, &project_id, source_id, ask_id.as_deref()) else {
            continue;
        };
        if let Some(at) = rows.iter().position(|locus| locus.locus_id == last) {
            owner_at = Some(pos);
            let rest: Vec<Locus> = rows.into_iter().skip(at + 1).collect();
            if !rest.is_empty() {
                same_source = Some((source_id.clone(), rest));
            }
            break;
        }
    }
    let Some(owner_at) = owner_at else {
        if let Some(active) = desk.active.as_mut() {
            active.chunk.clear();
        }
        return None;
    };
    let pending = if let Some(found) = same_source {
        Some(found)
    } else {
        let mut found = None;
        for source_id in source_ids.iter().skip(owner_at + 1) {
            let failed = match store.get(&project_id).and_then(|project| {
                project.sources.iter().find(|source| source.source_id == *source_id).map(|source| source.status)
            }) {
                Some(SourceStatus::Failed) | None => true,
                Some(_) => false,
            };
            if failed {
                continue;
            }
            let Ok(rows) = highlight_loci(store, &project_id, source_id, ask_id.as_deref()) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }
            match open_source(desk, store, source_id, opener, now_ms) {
                Ok(OpenOutcome::Opened) => {
                    found = Some((source_id.clone(), rows));
                    break;
                }
                Ok(OpenOutcome::Parked) | Ok(OpenOutcome::Waiting) => return None,
                Ok(OpenOutcome::Closed) => {}
                Err(message) if message == "quota:opens" => {
                    if let Some(active) = desk.active.as_mut() {
                        active.phase = Phase::Stopped;
                        active.chunk.clear();
                    }
                    return None;
                }
                Err(message) => {
                    let _ = store.set_source_status(&project_id, source_id, SourceStatus::Failed, Some(message));
                }
            }
        }
        found
    };
    let Some((source_id, rows)) = pending else {
        if let Some(active) = desk.active.as_mut() {
            active.chunk.clear();
        }
        return None;
    };
    let project = store.get(&project_id)?.clone();
    let source = project.sources.iter().find(|source| source.source_id == source_id)?.clone();
    let index = project.sources.iter().position(|source| source.source_id == source_id).unwrap_or(0) as u32;
    let first = rows[0].locus_id.clone();
    write_cursor(store, &project_id, index, Some(first), now_ms).ok()?;
    Some(seed_for(desk, &source, &rows))
}

/// When the current tour task is done, insert the next chunk. Pending and expired tasks stay put.
pub fn follow_done(
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) -> Option<String> {
    let current = task_id(desk)?.to_string();
    if !book.query_value(&current).is_some_and(|view| view["state"] == "done") {
        return None;
    }
    let seed = plan_follow(desk, store, opener, now_ms)?;
    let locus_ids = seed.locus_ids.clone();
    let id = book.insert_tour_chunk(ledger, seed, now_ms).ok()?;
    if let Err(err) = associate_chunk(ledger, desk, store, &id, &locus_ids, now_ms) {
        book.end_without_decision(&id);
        eprintln!("[tour] follow bind: {err}");
        return None;
    }
    remember_chunk(desk, id.clone(), locus_ids);
    Some(id)
}

fn open_source(
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    source_id: &str,
    opener: &mut dyn SourceOpener,
    now_ms: u64,
) -> Result<OpenOutcome, String> {
    let Some(active) = desk.active.as_ref() else {
        return Err("no_active_project_tour".into());
    };
    if active.phase == Phase::Stopped || active.opens >= OPEN_CAP {
        if let Some(active) = desk.active.as_mut() {
            active.phase = Phase::Stopped;
        }
        return Err("quota:opens".into());
    }
    if active.park.is_some() {
        return Ok(OpenOutcome::Waiting);
    }
    if let Some(last) = active.last_open_ms {
        if active.open_source.is_some() && now_ms < last.saturating_add(MOVE_WAIT_MS) {
            return Ok(OpenOutcome::Waiting);
        }
    }
    let project_id = active.project_id.clone();
    let project = store.get(&project_id).cloned().ok_or_else(|| StoreError::NoSuchProject.as_wire().to_string())?;
    let source = project.sources.iter().find(|source| source.source_id == source_id).ok_or_else(|| "source_not_in_project".to_string())?;
    let spec = match spec_for(source) {
        Ok(spec) => spec,
        Err(OpenFault::NoOpener) => return Err("no-opener".into()),
        Err(OpenFault::Failed(reason)) => return Err(reason),
    };
    if opener.parks() {
        return match opener.open(&spec) {
            Ok(()) => {
                if let Some(active) = desk.active.as_mut() {
                    active.park = Some(ParkedOpen { source_id: source_id.to_string(), spec });
                }
                Ok(OpenOutcome::Parked)
            }
            Err(OpenFault::NoOpener) => Err("no-opener".into()),
            Err(OpenFault::Failed(reason)) => {
                if let Some(active) = desk.active.as_mut() {
                    active.opens = active.opens.saturating_add(1);
                    active.open_source = None;
                    active.last_open_ms = None;
                    active.park = None;
                }
                Err(reason)
            }
        };
    }
    if desk.active.as_ref().and_then(|active| active.open_source.as_ref()).is_some() {
        opener.close();
    }
    match opener.open(&spec) {
        Ok(()) => {
            if let Some(active) = desk.active.as_mut() {
                active.opens = active.opens.saturating_add(1);
                active.last_open_ms = Some(now_ms);
                active.open_source = Some(source_id.to_string());
            }
            let _ = store.set_source_status(&project_id, source_id, SourceStatus::Opened, None);
            Ok(OpenOutcome::Opened)
        }
        Err(OpenFault::NoOpener) => Err("no-opener".into()),
        Err(OpenFault::Failed(reason)) => {
            if let Some(active) = desk.active.as_mut() {
                active.opens = active.opens.saturating_add(1);
                active.open_source = None;
                active.last_open_ms = None;
            }
            Err(reason)
        }
    }
}

/// `ok: true` marks the parked source opened and inserts the chunk.
/// A reason marks it failed and parks the next listed source.
pub(crate) fn plan_report(body: &ReportSourceOpenBody, store: &mut ProjectStore, desk: &mut TourDesk, now_ms: u64) -> Planned {
    let ok = match report_kind(body) {
        Ok(ok) => ok,
        Err(()) => return Planned::Finished(ProposalResult::err(&body.proposal_id, "schema")),
    };
    let (tour_project, parked_source) = {
        let Some(active) = desk.active.as_ref() else {
            return Planned::Finished(ProposalResult::err(&body.proposal_id, "source_not_in_project"));
        };
        let Some(park) = active.park.as_ref() else {
            return Planned::Finished(ProposalResult::err(&body.proposal_id, "source_not_in_project"));
        };
        (active.project_id.clone(), park.source_id.clone())
    };
    if tour_project != body.project_id || parked_source != body.source_id {
        return Planned::Finished(ProposalResult::err(&body.proposal_id, "source_not_in_project"));
    }
    let project_id = body.project_id.clone();
    let source_id = body.source_id.clone();
    let Some(project) = store.get(&project_id).cloned() else {
        return Planned::Finished(ProposalResult::err(&body.proposal_id, "source_not_in_project"));
    };
    if !project.sources.iter().any(|source| source.source_id == source_id) {
        return Planned::Finished(ProposalResult::err(&body.proposal_id, "source_not_in_project"));
    }
    if ok {
        if store.set_source_status(&project_id, &source_id, SourceStatus::Opened, None).is_err() {
            return Planned::Finished(ProposalResult::err(&body.proposal_id, "source_not_in_project"));
        }
        if let Some(active) = desk.active.as_mut() {
            active.opens = active.opens.saturating_add(1);
            active.last_open_ms = Some(now_ms);
            active.open_source = Some(source_id.clone());
            active.park = None;
        }
        let ask_id = desk.active.as_ref().and_then(|active| active.ask_id.clone());
        let project = match store.get(&project_id).cloned() {
            Some(project) => project,
            None => return Planned::Finished(ProposalResult::err(&body.proposal_id, "no_such_project")),
        };
        let index = project.sources.iter().position(|source| source.source_id == source_id).unwrap_or(0);
        let source = project.sources.iter().find(|source| source.source_id == source_id).unwrap().clone();
        let cursor_locus = if index == project.cursor.source_index as usize { project.cursor.locus_id.clone() } else { None };
        let rows = match highlight_loci(store, &project_id, &source_id, ask_id.as_deref()) {
            Ok(rows) => rows,
            Err(message) => return Planned::Finished(ProposalResult::err(&body.proposal_id, message)),
        };
        let rows = from_cursor(rows, cursor_locus.as_deref());
        if rows.is_empty() {
            let _ = write_cursor(store, &project_id, next_index(&project, index), None, now_ms);
            return match park_later(desk, store, &source_id, now_ms) {
                Ok(()) => Planned::Finished(ProposalResult::ok(&body.proposal_id, Some(report_detail(&project_id, &source_id, "opened")))),
                Err(message) => Planned::Finished(ProposalResult::err(&body.proposal_id, message)),
            };
        }
        let first = rows[0].locus_id.clone();
        if let Err(message) = write_cursor(store, &project_id, index as u32, Some(first), now_ms) {
            return Planned::Finished(ProposalResult::err(&body.proposal_id, message));
        }
        let seed = seed_for(desk, &source, &rows);
        return Planned::Insert {
            proposal_id: body.proposal_id.clone(),
            seed,
            detail: report_detail(&project_id, &source_id, "opened"),
        };
    }
    let reason = body.reason.clone().unwrap_or_default();
    if store.set_source_status(&project_id, &source_id, SourceStatus::Failed, Some(reason)).is_err() {
        return Planned::Finished(ProposalResult::err(&body.proposal_id, "source_not_in_project"));
    }
    if let Some(active) = desk.active.as_mut() {
        active.opens = active.opens.saturating_add(1);
        active.open_source = None;
        active.last_open_ms = None;
        active.park = None;
    }
    match park_later(desk, store, &source_id, now_ms) {
        Ok(()) => Planned::Finished(ProposalResult::ok(&body.proposal_id, Some(report_detail(&project_id, &source_id, "failed")))),
        Err(message) => Planned::Finished(ProposalResult::err(&body.proposal_id, message)),
    }
}

pub fn report_open(
    body: &ReportSourceOpenBody,
    store: &mut ProjectStore,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    desk: &mut TourDesk,
    now_ms: u64,
) -> ProposalResult {
    let planned = plan_report(body, store, desk, now_ms);
    commit(planned, book, ledger, desk, store, now_ms)
}

fn report_kind(body: &ReportSourceOpenBody) -> Result<bool, ()> {
    match (body.ok, body.reason.as_deref()) {
        (Some(true), None) => Ok(true),
        (None, Some(reason)) if !reason.is_empty() => Ok(false),
        _ => Err(()),
    }
}

fn report_detail(project_id: &str, source_id: &str, state: &str) -> Value {
    json!({
        "projectId": project_id,
        "sourceId": source_id,
        "state": state,
    })
}

/// Park the next listed source after the one just reported. A parked ask does not count as an open.
fn park_later(desk: &mut TourDesk, store: &mut ProjectStore, after: &str, now_ms: u64) -> Result<(), String> {
    if let Some(active) = desk.active.as_mut() {
        active.open_source = None;
        active.park = None;
    }
    let (project_id, source_ids) = {
        let Some(active) = desk.active.as_ref() else {
            return Ok(());
        };
        (active.project_id.clone(), active.source_ids.clone())
    };
    let Some(pos) = source_ids.iter().position(|id| id == after) else {
        return Ok(());
    };
    let mut opener = ParkOpener;
    for source_id in source_ids.iter().skip(pos + 1) {
        let status = store.get(&project_id).and_then(|project| {
            project.sources.iter().find(|source| source.source_id == *source_id).map(|source| source.status)
        });
        if matches!(status, Some(SourceStatus::Failed) | None) {
            continue;
        }
        match open_source(desk, store, source_id, &mut opener, now_ms) {
            Ok(OpenOutcome::Parked) | Ok(OpenOutcome::Waiting) | Ok(OpenOutcome::Opened) => return Ok(()),
            Ok(OpenOutcome::Closed) => {}
            Err(message) if message == "quota:opens" => return Err(message),
            Err(message) => {
                let _ = store.set_source_status(&project_id, source_id, SourceStatus::Failed, Some(message));
            }
        }
    }
    Ok(())
}

fn move_source(
    desk: &mut TourDesk,
    store: &mut ProjectStore,
    book: &mut TaskBook,
    opener: &mut dyn SourceOpener,
    step: i32,
    now_ms: u64,
) -> bool {
    let Some(active) = desk.active.as_ref() else {
        return false;
    };
    if active.phase != Phase::Active {
        return false;
    }
    let project_id = active.project_id.clone();
    let source_ids = active.source_ids.clone();
    let ask_id = active.ask_id.clone();
    let task_id = active.task_id.clone();
    let Some(project) = store.get(&project_id).cloned() else {
        return false;
    };
    let current = project.cursor.source_index as usize;
    let current_id = project.sources.get(current).map(|source| source.source_id.clone());
    let Some(pos) = current_id.and_then(|id| source_ids.iter().position(|item| item == &id)) else {
        return false;
    };
    let next = pos as i32 + step;
    if next < 0 || next as usize >= source_ids.len() {
        return false;
    }
    let source_id = source_ids[next as usize].clone();
    let index = project.sources.iter().position(|source| source.source_id == source_id).unwrap_or(0) as u32;
    if let Some(task_id) = task_id {
        book.end_without_decision(&task_id);
    }
    if let Some(active) = desk.active.as_mut() {
        active.chunk.clear();
    }
    let locus = highlight_loci(store, &project_id, &source_id, ask_id.as_deref()).ok().and_then(|rows| rows.first().map(|locus| locus.locus_id.clone()));
    if write_cursor(store, &project_id, index, locus, now_ms).is_err() {
        return false;
    }
    if desk.active.as_ref().is_some_and(|active| active.phase == Phase::Active) {
        let _ = open_source(desk, store, &source_id, opener, now_ms);
    }
    true
}

fn spec_for(source: &Source) -> Result<OpenSpec, OpenFault> {
    match source.kind {
        SourceKind::Web => Ok(OpenSpec::Web {
            url: source.reference.clone(),
            profile: locate_profile(),
            port: LOCATE_PORT,
        }),
        SourceKind::File if source.reference.to_ascii_lowercase().ends_with(".pdf") => {
            Ok(OpenSpec::Pdf { path: source.reference.clone() })
        }
        _ => Err(OpenFault::NoOpener),
    }
}

fn highlight_loci(store: &ProjectStore, project_id: &str, source_id: &str, ask_id: Option<&str>) -> Result<Vec<Locus>, String> {
    let mut rows = store.loci_for(project_id, source_id).map_err(|err| err.as_wire().to_string())?;
    rows.retain(|locus| {
        if matches!(locus.match_kind, Some(MatchKind::None)) || locus.words.is_empty() {
            return false;
        }
        match ask_id {
            Some(ask) => locus.ask_id.as_deref() == Some(ask),
            None => true,
        }
    });
    rows.sort_by(|left, right| left.span_start.cmp(&right.span_start).then_with(|| left.locus_id.cmp(&right.locus_id)));
    Ok(rows)
}

fn from_cursor(rows: Vec<Locus>, cursor: Option<&str>) -> Vec<Locus> {
    let Some(id) = cursor else {
        return rows;
    };
    match rows.iter().position(|locus| locus.locus_id == id) {
        Some(pos) => rows.into_iter().skip(pos).collect(),
        None => rows,
    }
}

fn next_index(project: &Project, index: usize) -> u32 {
    project
        .sources
        .iter()
        .enumerate()
        .skip(index + 1)
        .find(|(_, source)| source.status != SourceStatus::Failed)
        .map(|(pos, _)| pos as u32)
        .unwrap_or(index as u32)
}

fn write_cursor(store: &mut ProjectStore, project_id: &str, source_index: u32, locus_id: Option<String>, now_ms: u64) -> Result<(), String> {
    store
        .set_cursor(
            project_id,
            Cursor { source_index, locus_id, updated_at: now_ms },
        )
        .map_err(|err| err.as_wire().to_string())
}

fn list_name(source: &Source) -> String {
    match source.kind {
        SourceKind::Web => host_of(&source.reference).unwrap_or_else(|| "web".into()),
        SourceKind::File | SourceKind::Dir => Path::new(&source.reference)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_string(),
    }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = if let Some(inside) = host.strip_prefix('[') {
        inside.split(']').next().unwrap_or("").to_string()
    } else {
        host.split(':').next().unwrap_or("").to_string()
    };
    if host.is_empty() { None } else { Some(host) }
}

fn page_number(locus: &Locus) -> Option<u32> {
    locus.page.as_deref().and_then(|text| text.parse().ok())
}

fn opening_words(words: &str) -> String {
    words.chars().take(200).collect()
}

fn spoken_line(reason: &str) -> String {
    format!("I could not open that source ({reason}).")
}

fn task_wire(err: crate::task::TaskError) -> &'static str {
    match err {
        crate::task::TaskError::Schema => "schema",
        crate::task::TaskError::QueueFull => "queue_full",
        crate::task::TaskError::Quota => "quota: phrase cap",
        crate::task::TaskError::NoSuchTask => "no_such_task",
        crate::task::TaskError::NoSuchPresence => "no_such_presence",
        crate::task::TaskError::SelectRequiresControls => "select_requires_controls",
        crate::task::TaskError::ApproveTakesNoControls => "approve_takes_no_controls",
        crate::task::TaskError::TaskNotActive => "task_not_active",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{self, Tier};
    use crate::ledger::{Ledger, LedgerKind};
    use crate::project::{AskInput, AskKind, LocusInput, ProjectStore, SourceInput, SourceKind};
    use crate::protocol::{Proposal, TaskMode, TaskPace};
    use crate::task::{route_task, TaskBook, ViewerMove, DEFAULT_TTL_S};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Rec {
        calls: Vec<OpenSpec>,
        closes: u32,
        fail: Vec<String>,
    }

    impl Rec {
        fn new() -> Self {
            Self { calls: Vec::new(), closes: 0, fail: Vec::new() }
        }
    }

    impl SourceOpener for Rec {
        fn open(&mut self, spec: &OpenSpec) -> Result<(), OpenFault> {
            let key = match spec {
                OpenSpec::Web { url, .. } => url.clone(),
                OpenSpec::Pdf { path } => path.clone(),
            };
            self.calls.push(spec.clone());
            if self.fail.iter().any(|item| item == &key) {
                return Err(OpenFault::Failed("paywall".into()));
            }
            Ok(())
        }
        fn close(&mut self) {
            self.closes += 1;
        }
    }

    struct Scratch {
        root: PathBuf,
        store: ProjectStore,
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn bench(name: &str) -> Scratch {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!("presence-c11-{name}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let store = ProjectStore::open(&root.join("projects")).unwrap();
        Scratch { root, store }
    }

    fn project(store: &mut ProjectStore, sources: &[SourceInput]) -> String {
        store
            .create(
                Tier::T1User,
                "Tour",
                &[AskInput { text: "Is X supported?".into(), kind: AskKind::Question }],
                sources,
                &[],
                1,
            )
            .unwrap()
    }

    fn put(
        store: &mut ProjectStore,
        id: &str,
        source_id: &str,
        words: &str,
        kind: MatchKind,
        narration: Option<&str>,
        page: Option<&str>,
        span: u64,
    ) -> String {
        store
            .upsert_locus(
                id,
                LocusInput {
                    source_id: source_id.into(),
                    ask_id: Some("ak-1".into()),
                    doc_key: format!("doc-{source_id}"),
                    span_start: span,
                    span_end: span + 10,
                    words: words.into(),
                    page: page.map(str::to_string),
                    line: Some(1),
                    heading_path: None,
                    stance: None,
                    stance_by: None,
                    stance_conf: None,
                    match_kind: Some(kind),
                    narration: narration.map(str::to_string),
                    ocr_only: false,
                    view: None,
                },
            )
            .unwrap()
    }

    fn body_of(id: &str, mode: &str) -> StartProjectTourBody {
        let raw = format!(r#"{{"kind":"startProjectTour","proposalId":"p","projectId":"{id}","askId":"ak-1","mode":"{mode}","pace":"flow"}}"#);
        let proposal: Proposal = serde_json::from_str(&raw).unwrap();
        let Proposal::StartProjectTour(body) = proposal else { panic!("start") };
        body
    }

    fn go(
        scratch: &mut Scratch,
        book: &mut TaskBook,
        ledger: &mut Ledger,
        desk: &mut TourDesk,
        opener: &mut Rec,
        id: &str,
        tier: Tier,
        now: u64,
    ) -> ProposalResult {
        let body = body_of(id, "explain");
        start(&body, tier, &mut scratch.store, book, ledger, desk, opener, now)
    }

    #[test]
    fn m1_open_outside_the_project_is_refused() {
        let mut scratch = bench("m1");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        assert_eq!(go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0).status, "ok");
        let err = apply(
            &mut desk,
            &mut scratch.store,
            OpenSource::Open { project_id: id, source_id: "sr-nope".into() },
            &mut opener,
            2_000,
        )
        .unwrap_err();
        assert_eq!(err, "source_not_in_project");
    }

    #[test]
    fn m2_open_without_a_tour_is_refused() {
        let mut scratch = bench("m2");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let err = apply(
            &mut desk,
            &mut scratch.store,
            OpenSource::Open { project_id: id, source_id: "sr-1".into() },
            &mut opener,
            0,
        )
        .unwrap_err();
        assert_eq!(err, "no_active_project_tour");
        assert!(opener.calls.is_empty());
    }

    #[test]
    fn m3_t2_lists_names_and_opens_nothing() {
        let mut scratch = bench("m3");
        let id = project(
            &mut scratch.store,
            &[
                SourceInput { kind: SourceKind::Web, reference: "https://example.com/papers/a".into() },
                SourceInput { kind: SourceKind::File, reference: "/tmp/papers/paper.pdf".into() },
                SourceInput { kind: SourceKind::Web, reference: "https://gone.example/x".into() },
            ],
        );
        scratch.store.set_source_status(&id, "sr-3", SourceStatus::Failed, Some("unreachable".into())).unwrap();
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let result = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T2Client, 0);
        assert_eq!(result.status, "ok");
        let detail = result.detail.unwrap();
        assert_eq!(detail["state"], "awaitingStart");
        assert_eq!(detail["sources"], json!(["example.com", "paper.pdf"]));
        let rendered = detail["sources"].to_string();
        assert!(!rendered.contains("https://"));
        assert!(!rendered.contains("/tmp"));
        assert!(opener.calls.is_empty());
        assert_eq!(book.task_len(), 0);
        assert_eq!(phase_name(&desk), Some("awaitingStart"));
    }

    #[test]
    fn m4_start_promotes_a_waiting_task_and_does_not_open() {
        let mut scratch = bench("m4");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let raw = r#"{"kind":"createTask","proposalId":"c","presenceId":"p-1","mode":"explain","items":[{"query":"the line","scroll":true}]}"#;
        let proposal: Proposal = serde_json::from_str(raw).unwrap();
        let created = route_task(&proposal, Tier::T2Client, &mut book, &mut ledger, true, 1).unwrap();
        let task_id = created.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(book.query_value(&task_id).unwrap()["state"], "awaitingStart");
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        assert_eq!(go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T2Client, 2).status, "ok");
        assert!(on_hotkey(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, Hotkey::Start, 3));
        assert_eq!(book.query_value(&task_id).unwrap()["state"], "active");
        assert_eq!(phase_name(&desk), Some("awaitingStart"));
        assert!(opener.calls.is_empty());
    }

    #[test]
    fn m5_opener_receives_the_locate_profile_or_the_pdf_path() {
        let mut scratch = bench("m5");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        assert_eq!(go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0).status, "ok");
        match &opener.calls[0] {
            OpenSpec::Web { url, profile, port } => {
                assert_eq!(url, "https://example.com/a");
                assert!(profile.ends_with("/pachakutech/locate-chromium"), "{profile}");
                assert!(!profile.contains(".config"), "{profile}");
                assert_eq!(*port, 9222);
            }
            other => panic!("web spec {other:?}"),
        }

        let mut pdf = bench("m5b");
        let id = project(&mut pdf.store, &[SourceInput { kind: SourceKind::File, reference: "/tmp/papers/paper.pdf".into() }]);
        put(&mut pdf.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, Some("4"), 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        assert_eq!(go(&mut pdf, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0).status, "ok");
        assert_eq!(opener.calls, vec![OpenSpec::Pdf { path: "/tmp/papers/paper.pdf".into() }]);

        let mut plain = bench("m5c");
        let id = project(&mut plain.store, &[SourceInput { kind: SourceKind::File, reference: "/tmp/notes/plain.txt".into() }]);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let result = go(&mut plain, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0);
        assert!(opener.calls.is_empty());
        let source = plain.store.get(&id).unwrap().sources.iter().find(|source| source.source_id == "sr-1").unwrap();
        assert_eq!(source.status, SourceStatus::Failed);
        assert_eq!(source.reason.as_deref(), Some("no-opener"));
        let line = result.detail.unwrap()["spoken"].as_str().unwrap().to_string();
        assert!(!line.contains('\n'));
        assert!(line.contains("no-opener"));
    }

    #[test]
    fn m6_one_open_at_a_time_with_a_cap_and_a_wait() {
        let mut scratch = bench("m6");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        assert_eq!(go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0).status, "ok");
        assert_eq!(opener.calls.len(), 1);
        let again = apply(
            &mut desk,
            &mut scratch.store,
            OpenSource::Open { project_id: id.clone(), source_id: "sr-1".into() },
            &mut opener,
            100,
        )
        .unwrap();
        assert!(matches!(again, OpenOutcome::Waiting));
        assert_eq!(opener.calls.len(), 1);
        assert_eq!(opener.closes, 0);
        apply(
            &mut desk,
            &mut scratch.store,
            OpenSource::Open { project_id: id.clone(), source_id: "sr-1".into() },
            &mut opener,
            MOVE_WAIT_MS,
        )
        .unwrap();
        assert_eq!(opener.calls.len(), 2);
        assert_eq!(opener.closes, 1);
        for n in 2..OPEN_CAP {
            apply(
                &mut desk,
                &mut scratch.store,
                OpenSource::Open { project_id: id.clone(), source_id: "sr-1".into() },
                &mut opener,
                u64::from(n) * MOVE_WAIT_MS,
            )
            .unwrap();
        }
        assert_eq!(opener.calls.len(), OPEN_CAP as usize);
        let err = apply(
            &mut desk,
            &mut scratch.store,
            OpenSource::Open { project_id: id, source_id: "sr-1".into() },
            &mut opener,
            u64::from(OPEN_CAP) * MOVE_WAIT_MS,
        )
        .unwrap_err();
        assert_eq!(err, "quota:opens");
        assert_eq!(opener.calls.len(), OPEN_CAP as usize);
        assert_eq!(phase_name(&desk), Some("stopped"));
    }

    #[test]
    fn m7_open_source_has_no_follow_and_does_not_parse() {
        let cmd = OpenSource::Open { project_id: "pj".into(), source_id: "sr-1".into() };
        let slot = match cmd {
            OpenSource::Open { .. } => 0u8,
            OpenSource::Close { .. } => 1u8,
        };
        assert_eq!(slot, 0);
        let closed = OpenSource::Close { project_id: "pj".into(), source_id: "sr-1".into() };
        assert!(matches!(closed, OpenSource::Close { .. }));
        assert!(serde_json::from_str::<Proposal>(r#"{"kind":"openSource","proposalId":"o","projectId":"pj","sourceId":"sr-1"}"#).is_err());
        assert!(serde_json::from_str::<Proposal>(r#"{"kind":"startProjectTour","proposalId":"p","projectId":"pj","mode":"explain","path":"/tmp/a"}"#).is_err());
        let refused: Proposal = serde_json::from_str(r#"{"kind":"startProjectTour","proposalId":"p","projectId":"pj-0123456789abcdef","mode":"explain"}"#).unwrap();
        assert!(!gate::allowed(Tier::T3Sidecar, &refused));
        assert_eq!(refused.kind_name(), "startProjectTour");
    }

    #[test]
    fn m8_expiry_leaves_the_cursor_and_resume_starts_at_the_next_locus() {
        let mut scratch = bench("m8");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        let first = put(&mut scratch.store, &id, "sr-1", "alpha passage", MatchKind::Exact, None, None, 0);
        let second = put(&mut scratch.store, &id, "sr-1", "beta passage", MatchKind::Exact, None, None, 20);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let started = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0);
        let task_id = started.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(scratch.store.get(&id).unwrap().cursor.locus_id.as_deref(), Some(first.as_str()));
        leave_current_locus(&mut scratch.store, &id, 10).unwrap();
        assert_eq!(scratch.store.get(&id).unwrap().cursor.locus_id.as_deref(), Some(second.as_str()));
        book.tick(&mut ledger, u64::from(DEFAULT_TTL_S) * 1000);
        assert_eq!(book.query_value(&task_id).unwrap()["state"], "expired");
        assert_eq!(scratch.store.get(&id).unwrap().cursor.locus_id.as_deref(), Some(second.as_str()));
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let again = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 200_000);
        let next_id = again.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        let items = again_items(&book, &next_id);
        assert_eq!(items, vec!["beta passage".to_string()]);
    }

    fn again_items(book: &TaskBook, id: &str) -> Vec<String> {
        book.query_value(id).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["query"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn m9_next_source_moves_the_cursor_and_writes_no_decision() {
        let mut scratch = bench("m9");
        let id = project(
            &mut scratch.store,
            &[
                SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() },
                SourceInput { kind: SourceKind::Web, reference: "https://other.example/b".into() },
            ],
        );
        put(&mut scratch.store, &id, "sr-1", "alpha passage", MatchKind::Exact, None, None, 0);
        put(&mut scratch.store, &id, "sr-2", "beta passage", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let started = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0);
        let task_id = started.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        let before = ledger.events().len();
        assert!(on_hotkey(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, Hotkey::NextSource, MOVE_WAIT_MS));
        assert_eq!(book.query_value(&task_id).unwrap()["state"], "dismissed");
        assert_eq!(scratch.store.get(&id).unwrap().cursor.source_index, 1);
        assert!(ledger.events().len() == before || ledger.events()[before..].iter().all(|ev| {
            !matches!(ev.kind, LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Cleared)
        }));
        assert!(!on_hotkey(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, Hotkey::Skip, MOVE_WAIT_MS * 2));
        assert!(on_hotkey(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, Hotkey::PrevSource, MOVE_WAIT_MS * 2));
        assert_eq!(scratch.store.get(&id).unwrap().cursor.source_index, 0);
    }

    #[test]
    fn m10_a_failed_open_is_recorded_and_the_next_source_is_attempted() {
        let mut scratch = bench("m10");
        let id = project(
            &mut scratch.store,
            &[
                SourceInput { kind: SourceKind::Web, reference: "https://pay.example/a".into() },
                SourceInput { kind: SourceKind::File, reference: "/tmp/papers/paper.pdf".into() },
            ],
        );
        put(&mut scratch.store, &id, "sr-2", "the quoted line", MatchKind::Exact, None, Some("2"), 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        opener.fail.push("https://pay.example/a".into());
        let result = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0);
        assert_eq!(result.status, "ok");
        let line = result.detail.unwrap()["spoken"].as_str().unwrap().to_string();
        assert!(!line.contains('\n'));
        assert!(line.contains("paywall"));
        let failed = scratch.store.get(&id).unwrap().sources.iter().find(|source| source.source_id == "sr-1").unwrap();
        assert_eq!(failed.status, SourceStatus::Failed);
        assert_eq!(failed.reason.as_deref(), Some("paywall"));
        assert!(matches!(opener.calls.last(), Some(OpenSpec::Pdf { path }) if path == "/tmp/papers/paper.pdf"));
        assert!(opener.calls.iter().any(|spec| matches!(spec, OpenSpec::Web { url, .. } if url == "https://pay.example/a")));
    }

    #[test]
    fn m11_exact_near_and_none_become_items_or_not() {
        let mut scratch = bench("m11");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        let sentence = "I couldn't find the quoted words. The closest passage is this one.";
        put(&mut scratch.store, &id, "sr-1", "exact words", MatchKind::Exact, None, None, 0);
        put(&mut scratch.store, &id, "sr-1", "closest passage", MatchKind::Near, Some(sentence), None, 30);
        put(&mut scratch.store, &id, "sr-1", "NOT-ON-THE-TOUR", MatchKind::None, None, None, 60);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let result = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0);
        let task_id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        let items = book.query_value(&task_id).unwrap()["items"].as_array().unwrap().clone();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["query"], "exact words");
        assert_eq!(items[0]["narration"], "");
        assert_eq!(items[1]["query"], "closest passage");
        assert_eq!(items[1]["narration"], sentence);
        assert!(items.iter().all(|item| item["query"] != "NOT-ON-THE-TOUR"));
    }

    #[test]
    fn m12_confirmed_scroll_item_is_pending() {
        let mut scratch = bench("m12");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        assert_eq!(go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T2Client, 0).status, "ok");
        assert_eq!(book.task_len(), 0);
        assert!(on_hotkey(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, Hotkey::Start, 1));
        let task_id = task_id(&desk).unwrap().to_string();
        let view = book.query_value(&task_id).unwrap();
        assert_eq!(view["state"], "pending");
        assert_ne!(view["state"], "awaitingStart");
        assert_eq!(view["origin"], "agent");
        assert_eq!(view["items"][0]["scroll"], true);
    }

    #[test]
    fn m13_the_move_comes_from_the_locus() {
        let mut scratch = bench("m13");
        let id = project(
            &mut scratch.store,
            &[
                SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() },
                SourceInput { kind: SourceKind::File, reference: "/tmp/papers/paper.pdf".into() },
            ],
        );
        put(&mut scratch.store, &id, "sr-1", "opening words here", MatchKind::Exact, None, None, 0);
        put(&mut scratch.store, &id, "sr-2", "page four words", MatchKind::Exact, None, Some("4"), 0);
        let project = scratch.store.get(&id).unwrap().clone();
        let web = project.sources.iter().find(|source| source.source_id == "sr-1").unwrap();
        let pdf = project.sources.iter().find(|source| source.source_id == "sr-2").unwrap();
        let web_locus = scratch.store.loci_for(&id, "sr-1").unwrap().pop().unwrap();
        let pdf_locus = scratch.store.loci_for(&id, "sr-2").unwrap().pop().unwrap();
        assert_eq!(
            move_for_locus(web, &web_locus),
            Some(ViewerMove::WebFind { doc_key: "doc-sr-1".into(), opening_phrase: "opening words here".into() })
        );
        assert_eq!(
            move_for_locus(pdf, &pdf_locus),
            Some(ViewerMove::PdfPage { doc_key: "doc-sr-2".into(), page: 4 })
        );
        let none = put(&mut scratch.store, &id, "sr-1", "", MatchKind::None, None, None, 80);
        let none_locus = scratch.store.loci_for(&id, "sr-1").unwrap().into_iter().find(|locus| locus.locus_id == none).unwrap();
        assert_eq!(move_for_locus(web, &none_locus), None);
        let src = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/tour.rs")).unwrap();
        let head = src.split("#[cfg(test)]").next().unwrap();
        assert!(!head.contains("docmap"), "{head}");
        assert!(!head.contains("DocMap"));
        assert!(!head.contains("read_passages"));
        assert!(!head.contains("Command::new"));
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let result = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0);
        let task_id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        assert_eq!(
            book.planned_move(&task_id).unwrap(),
            ViewerMove::WebFind { doc_key: "doc-sr-1".into(), opening_phrase: "opening words here".into() }
        );
        let _ = (TaskMode::Explain, TaskPace::Flow);
    }

    #[test]
    fn further_loci_in_the_source_follow_when_the_task_is_done() {
        let mut scratch = bench("follow");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.com/a".into() }]);
        let mut queries = Vec::new();
        for n in 0..13 {
            let words = format!("locus-{n:02}");
            put(&mut scratch.store, &id, "sr-1", &words, MatchKind::Exact, None, None, n * 10);
            queries.push(words);
        }
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let mut opener = Rec::new();
        let started = go(&mut scratch, &mut book, &mut ledger, &mut desk, &mut opener, &id, Tier::T1User, 0);
        let task_id = started.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(again_items(&book, &task_id), queries[..12]);
        assert!(follow_done(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, 1_000).is_none());
        book.admit().unwrap();
        book.complete(&task_id).unwrap();
        let next = follow_done(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, 5_000).unwrap();
        assert_eq!(again_items(&book, &next), vec![queries[12].clone()]);
        assert_eq!(opener.calls.len(), 1);
        book.admit().unwrap();
        book.complete(&next).unwrap();
        assert!(follow_done(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut opener, 8_000).is_none());
    }

    fn report_proposal(project: &str, source: &str, extra: &str) -> Proposal {
        let raw = format!(r#"{{"kind":"reportSourceOpen","proposalId":"r","projectId":"{project}","sourceId":"{source}"{extra}}}"#);
        serde_json::from_str(&raw).unwrap()
    }

    fn report_body(proposal: Proposal) -> crate::protocol::ReportSourceOpenBody {
        let Proposal::ReportSourceOpen(body) = proposal else { panic!("report") };
        body
    }

    #[test]
    fn w1_production_start_parks_and_leaves_the_source_mapped() {
        let mut scratch = bench("w1");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.test/a".into() }]);
        scratch.store.set_source_status(&id, "sr-1", SourceStatus::Mapped, None).unwrap();
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let body = body_of(&id, "explain");
        let planned = route_start(&body, Tier::T1User, &mut scratch.store, &mut desk, 0);
        let result = commit(planned, &mut book, &mut ledger, &mut desk, &mut scratch.store, 0);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        assert!(result.detail.unwrap().get("taskId").is_none());
        assert_eq!(scratch.store.get(&id).unwrap().sources[0].status, SourceStatus::Mapped);
        assert_eq!(desk.active.as_ref().unwrap().opens, 0);
        assert!(desk.active.as_ref().unwrap().last_open_ms.is_none());
        let req = open_request_for(&desk, &id).unwrap();
        assert_eq!(req["kind"], "web");
        assert_eq!(req["sourceId"], "sr-1");
        assert_eq!(req["reference"], "https://example.test/a");
        let profile = req["profile"].as_str().unwrap();
        assert!(profile.ends_with("/pachakutech/locate-chromium"), "{profile}");
        assert!(!profile.contains(".config"), "{profile}");
        assert_eq!(req["port"], 9222);
        assert!(open_request_for(&desk, "pj-0123456789abcdef").is_none());

        let mut two = bench("w1b");
        let id = project(
            &mut two.store,
            &[
                SourceInput { kind: SourceKind::Web, reference: "https://example.test/a".into() },
                SourceInput { kind: SourceKind::Web, reference: "https://example.test/b".into() },
            ],
        );
        two.store.set_source_status(&id, "sr-1", SourceStatus::Mapped, None).unwrap();
        two.store.set_source_status(&id, "sr-2", SourceStatus::Mapped, None).unwrap();
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let body = body_of(&id, "explain");
        let planned = route_start(&body, Tier::T1User, &mut two.store, &mut desk, 0);
        let result = commit(planned, &mut book, &mut ledger, &mut desk, &mut two.store, 0);
        assert_eq!(result.status, "ok");
        let mut opener = ParkOpener;
        let waited = apply(
            &mut desk,
            &mut two.store,
            OpenSource::Open { project_id: id.clone(), source_id: "sr-2".into() },
            &mut opener,
            10,
        )
        .unwrap();
        assert!(matches!(waited, OpenOutcome::Waiting));
        assert_eq!(open_request_for(&desk, &id).unwrap()["sourceId"], "sr-1");
        assert_eq!(desk.active.as_ref().unwrap().opens, 0);
        assert!(two.store.get(&id).unwrap().sources.iter().all(|source| source.status == SourceStatus::Mapped));
    }

    #[test]
    fn w2_a_report_opens_or_fails_and_parks_the_next() {
        let mut scratch = bench("w2");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.test/a".into() }]);
        scratch.store.set_source_status(&id, "sr-1", SourceStatus::Mapped, None).unwrap();
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let body = body_of(&id, "explain");
        let planned = route_start(&body, Tier::T1User, &mut scratch.store, &mut desk, 0);
        assert_eq!(commit(planned, &mut book, &mut ledger, &mut desk, &mut scratch.store, 0).status, "ok");
        let reported = report_open(&report_body(report_proposal(&id, "sr-1", r#","ok":true"#)), &mut scratch.store, &mut book, &mut ledger, &mut desk, 5);
        assert_eq!(reported.status, "ok", "{:?}", reported.error);
        let task_id = reported.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert!(!task_id.is_empty());
        assert_eq!(scratch.store.get(&id).unwrap().sources[0].status, SourceStatus::Opened);
        assert!(open_request_for(&desk, &id).is_none());
        assert_eq!(desk.active.as_ref().unwrap().opens, 1);
        assert_eq!(desk.active.as_ref().unwrap().last_open_ms, Some(5));
        let mut opener = Rec::new();
        let waited = apply(
            &mut desk,
            &mut scratch.store,
            OpenSource::Open { project_id: id.clone(), source_id: "sr-1".into() },
            &mut opener,
            100,
        )
        .unwrap();
        assert!(matches!(waited, OpenOutcome::Waiting));
        assert!(opener.calls.is_empty());

        let mut next = bench("w2b");
        let id = project(
            &mut next.store,
            &[
                SourceInput { kind: SourceKind::Web, reference: "https://pay.example/a".into() },
                SourceInput { kind: SourceKind::File, reference: "/tmp/papers/paper.pdf".into() },
            ],
        );
        next.store.set_source_status(&id, "sr-1", SourceStatus::Mapped, None).unwrap();
        next.store.set_source_status(&id, "sr-2", SourceStatus::Mapped, None).unwrap();
        put(&mut next.store, &id, "sr-2", "the quoted line", MatchKind::Exact, None, Some("2"), 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let body = body_of(&id, "explain");
        let planned = route_start(&body, Tier::T1User, &mut next.store, &mut desk, 0);
        assert_eq!(commit(planned, &mut book, &mut ledger, &mut desk, &mut next.store, 0).status, "ok");
        let failed = report_open(
            &report_body(report_proposal(&id, "sr-1", r#","reason":"paywall""#)),
            &mut next.store,
            &mut book,
            &mut ledger,
            &mut desk,
            8,
        );
        assert_eq!(failed.status, "ok", "{:?}", failed.error);
        assert!(failed.detail.unwrap().get("taskId").is_none());
        let first = next.store.get(&id).unwrap().sources.iter().find(|source| source.source_id == "sr-1").unwrap();
        assert_eq!(first.status, SourceStatus::Failed);
        assert_eq!(first.reason.as_deref(), Some("paywall"));
        let req = open_request_for(&desk, &id).unwrap();
        assert_eq!(req["kind"], "pdf");
        assert_eq!(req["sourceId"], "sr-2");
        assert_eq!(req["reference"], "/tmp/papers/paper.pdf");
        assert!(req.get("profile").is_none());
        assert!(req.get("port").is_none());
        assert_eq!(next.store.get(&id).unwrap().sources.iter().find(|source| source.source_id == "sr-2").unwrap().status, SourceStatus::Mapped);
        assert_eq!(desk.active.as_ref().unwrap().opens, 1);
    }

    #[test]
    fn w3_a_report_is_the_parked_source_only() {
        let mut scratch = bench("w3");
        let id = project(
            &mut scratch.store,
            &[
                SourceInput { kind: SourceKind::Web, reference: "https://example.test/a".into() },
                SourceInput { kind: SourceKind::Web, reference: "https://example.test/b".into() },
            ],
        );
        scratch.store.set_source_status(&id, "sr-1", SourceStatus::Mapped, None).unwrap();
        scratch.store.set_source_status(&id, "sr-2", SourceStatus::Mapped, None).unwrap();
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let body = body_of(&id, "explain");
        let planned = route_start(&body, Tier::T1User, &mut scratch.store, &mut desk, 0);
        assert_eq!(commit(planned, &mut book, &mut ledger, &mut desk, &mut scratch.store, 0).status, "ok");
        let wrong = report_open(
            &report_body(report_proposal(&id, "sr-2", r#","ok":true"#)),
            &mut scratch.store,
            &mut book,
            &mut ledger,
            &mut desk,
            1,
        );
        assert_eq!(wrong.error.as_deref(), Some("source_not_in_project"));
        assert_eq!(open_request_for(&desk, &id).unwrap()["sourceId"], "sr-1");
        assert!(scratch.store.get(&id).unwrap().sources.iter().all(|source| source.status == SourceStatus::Mapped));

        let mut reg = crate::registry::Registry::new();
        let proposal: Proposal = serde_json::from_str(
            r#"{"kind":"reportSourceOpen","proposalId":"r","projectId":"pj-0123456789abcdef","sourceId":"sr-1","ok":true}"#,
        )
        .unwrap();
        let refused = reg.route_tour_proposal(&proposal, Tier::T3Sidecar, 0).unwrap();
        assert_eq!(refused.error.as_deref(), Some("not_allowed_for_tier:reportSourceOpen"));

        assert!(serde_json::from_str::<Proposal>(
            r#"{"kind":"reportSourceOpen","proposalId":"r","projectId":"pj-0123456789abcdef","sourceId":"sr-1","ok":true,"url":"https://evil.test"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<Proposal>(
            r#"{"kind":"reportSourceOpen","proposalId":"r","projectId":"pj-0123456789abcdef","sourceId":"sr-1","reason":"x","path":"/tmp/a"}"#
        )
        .is_err());
        for extra in ["", r#","ok":false"#, r#","ok":true,"reason":"x""#, r#","reason":"""#] {
            let bad = report_open(
                &report_body(report_proposal("pj-0123456789abcdef", "sr-1", extra)),
                &mut scratch.store,
                &mut book,
                &mut ledger,
                &mut desk,
                2,
            );
            assert_eq!(bad.error.as_deref(), Some("schema"), "{extra}");
        }
        assert_eq!(open_request_for(&desk, &id).unwrap()["sourceId"], "sr-1");
    }

    #[test]
    fn w6_production_names_a_spec_and_spawns_no_viewer() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let tour = fs::read_to_string(root.join("tour.rs")).unwrap();
        let tour_head = tour.split("mod tests").next().unwrap();
        assert!(!tour_head.contains("Command::new"));
        assert!(!tour_head.contains("evince"));
        assert!(!tour_head.replace("locate-chromium", "").contains("chromium"));
        let socket = fs::read_to_string(root.join("socket.rs")).unwrap();
        let socket_head = socket.split("mod tests").next().unwrap();
        assert!(!socket_head.contains("Command::new"));
        assert!(!socket_head.contains("chromium"));
        assert!(!socket_head.contains("evince"));
        let overlay = fs::read_to_string(root.join("overlay.rs")).unwrap();
        assert!(!overlay.contains("evince"));
        assert!(!overlay.contains("chromium"));
    }

    #[test]
    fn w7_start_parks_when_no_task_is_waiting() {
        let mut scratch = bench("w7");
        let id = project(&mut scratch.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.test/a".into() }]);
        put(&mut scratch.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mut desk = TourDesk::default();
        let body = body_of(&id, "explain");
        let listed = start(&body, Tier::T2Client, &mut scratch.store, &mut book, &mut ledger, &mut desk, &mut ParkOpener, 0);
        assert_eq!(listed.detail.unwrap()["state"], "awaitingStart");
        assert!(open_request_for(&desk, &id).is_none());
        assert!(on_hotkey(&mut desk, &mut scratch.store, &mut book, &mut ledger, &mut ParkOpener, Hotkey::Start, 1));
        assert_eq!(phase_name(&desk), Some("active"));
        assert_eq!(open_request_for(&desk, &id).unwrap()["reference"], "https://example.test/a");
        assert!(task_id(&desk).is_none());
        assert_ne!(scratch.store.get(&id).unwrap().sources[0].status, SourceStatus::Opened);
        assert_eq!(desk.active.as_ref().unwrap().opens, 0);

        let mut waiting = bench("w7b");
        let id = project(&mut waiting.store, &[SourceInput { kind: SourceKind::Web, reference: "https://example.test/a".into() }]);
        put(&mut waiting.store, &id, "sr-1", "the quoted line", MatchKind::Exact, None, None, 0);
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let raw = r#"{"kind":"createTask","proposalId":"c","presenceId":"p-1","mode":"explain","items":[{"query":"the line","scroll":true}]}"#;
        let proposal: Proposal = serde_json::from_str(raw).unwrap();
        let created = route_task(&proposal, Tier::T2Client, &mut book, &mut ledger, true, 1).unwrap();
        let task_id = created.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        let mut desk = TourDesk::default();
        let body = body_of(&id, "explain");
        assert_eq!(start(&body, Tier::T2Client, &mut waiting.store, &mut book, &mut ledger, &mut desk, &mut ParkOpener, 2).status, "ok");
        assert!(on_hotkey(&mut desk, &mut waiting.store, &mut book, &mut ledger, &mut ParkOpener, Hotkey::Start, 3));
        assert_eq!(book.query_value(&task_id).unwrap()["state"], "active");
        assert_eq!(phase_name(&desk), Some("awaitingStart"));
        assert!(open_request_for(&desk, &id).is_none());
    }
}
