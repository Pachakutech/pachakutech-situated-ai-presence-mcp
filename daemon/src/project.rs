//! Durable project store. Plain files, no new crate. The session ledger stays
//! in memory; a project event is on disk before that append returns success.
//! Nothing here moves a page.

use crate::gate::Tier;
use crate::ledger::{LedgerEvent, LedgerKind, ViewedTier};
use crate::protocol::{CreateProjectBody, ProjectAskKind, ProjectSourceIn, Proposal, ProposalResult, QueryProjectBody};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROJECT_CAP: usize = 100;
const ASK_CAP: usize = 20;
const SOURCE_CAP: usize = 500;
const LOCUS_CAP: usize = 5000;
const EVENT_BYTES: u64 = 50 * 1024 * 1024;
const TITLE_CAP: usize = 200;
const ASK_CHARS: usize = 500;
const REF_CAP: usize = 2000;
const WORDS_CAP: usize = 1000;
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    Schema,
    QuotaProjects,
    QuotaAsks,
    QuotaSources,
    QuotaLoci,
    QuotaEvents,
    NoSuchProject,
    NoSuchLocus,
    NoSuchAsk,
    DraftUnconfirmed,
    NotAllowed,
}

impl StoreError {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::QuotaProjects => "quota:projects",
            Self::QuotaAsks => "quota:asks",
            Self::QuotaSources => "quota:sources",
            Self::QuotaLoci => "quota:loci",
            Self::QuotaEvents => "quota:events",
            Self::NoSuchProject => "no_such_project",
            Self::NoSuchLocus => "no_such_locus",
            Self::NoSuchAsk => "no_such_ask",
            Self::DraftUnconfirmed => "draft_unconfirmed",
            Self::NotAllowed => "not_allowed_for_tier:createProject",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AskKind {
    Claim,
    Question,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Web,
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceStatus {
    Pending,
    Mapped,
    Opened,
    Failed,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stance {
    Supports,
    Contrary,
    Mentions,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchKind {
    Exact,
    Near,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeepVia {
    Select,
    Approve,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CitationMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filled_by: Option<String>,
}

impl CitationMeta {
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.authors.is_none()
            && self.date.is_none()
            && self.container.is_none()
            && self.doi.is_none()
            && self.url.is_none()
            && self.canonical_url.is_none()
            && self.accessed.is_none()
            && self.page_labels.is_none()
            && self.filled_by.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    pub ask_id: String,
    pub text: String,
    pub kind: AskKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub source_id: String,
    pub kind: SourceKind,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_key: Option<String>,
    #[serde(default, skip_serializing_if = "CitationMeta::is_empty")]
    pub meta: CitationMeta,
    pub status: SourceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub source_index: u32,
    pub locus_id: Option<String>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub project_id: String,
    pub title: String,
    pub created_by: Tier,
    pub created_at: u64,
    pub draft: bool,
    pub asks: Vec<Ask>,
    pub sources: Vec<Source>,
    pub cursor: Cursor,
    pub granted_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewBox {
    pub u: f32,
    pub v: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Locus {
    pub locus_id: String,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_id: Option<String>,
    pub doc_key: String,
    pub span_start: u64,
    pub span_end: u64,
    pub words: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_path: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stance: Option<Stance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stance_by: Option<Tier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stance_conf: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_kind: Option<MatchKind>,
    #[serde(default)]
    pub ocr_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredLocus {
    pseq: u64,
    #[serde(flatten)]
    locus: Locus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TaskRec {
    task_id: String,
    mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ask_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_id: Option<String>,
    started_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEvent {
    pub pseq: u64,
    pub run_id: String,
    pub session_seq: u64,
    pub ts_ms: u64,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewed: Option<String>,
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locus_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub phrase_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keep {
    pub locus_id: String,
    pub ask_id: Option<String>,
    pub kept_at_ms: u64,
    pub via: KeepVia,
}

#[derive(Debug, Clone)]
pub struct AskInput {
    pub text: String,
    pub kind: AskKind,
}

#[derive(Debug, Clone)]
pub struct SourceInput {
    pub kind: SourceKind,
    pub reference: String,
}

#[derive(Debug, Clone)]
pub struct LocusInput {
    pub source_id: String,
    pub ask_id: Option<String>,
    pub doc_key: String,
    pub span_start: u64,
    pub span_end: u64,
    pub words: String,
    pub page: Option<String>,
    pub line: Option<u32>,
    pub heading_path: Option<Vec<String>>,
    pub stance: Option<Stance>,
    pub stance_by: Option<Tier>,
    pub stance_conf: Option<f32>,
    pub match_kind: Option<MatchKind>,
    pub ocr_only: bool,
    pub view: Option<ViewBox>,
}

struct Loaded {
    project: Project,
    loci: Vec<StoredLocus>,
    tasks: Vec<TaskRec>,
    events: Vec<ProjectEvent>,
    next_event: u64,
    next_locus: u64,
    last_fsync: Option<Instant>,
}

pub struct ProjectStore {
    root: PathBuf,
    projects: HashMap<String, Loaded>,
    mint: u64,
    run_id: String,
    /// Create does not call this. A test installs a reader that panics.
    #[allow(dead_code)]
    source_reader: fn(&str) -> Result<Vec<u8>, String>,
}

fn explode(reference: &str) -> Result<Vec<u8>, String> {
    panic!("project store read a source: {reference}");
}

pub fn projects_root_from_env() -> PathBuf {
    if let Ok(over) = std::env::var("PRESENCE_PROJECTS_DIR") {
        if !over.is_empty() {
            return PathBuf::from(over);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("pachakutech").join("projects");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local").join("share").join("pachakutech").join("projects")
}

impl ProjectStore {
    pub fn open(root: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(root)?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        let mut store = Self {
            root: root.to_path_buf(),
            projects: HashMap::new(),
            mint: 0,
            run_id: mint_run_id(),
            source_reader: explode,
        };
        store.load_all();
        Ok(store)
    }

    pub fn set_source_reader(&mut self, reader: fn(&str) -> Result<Vec<u8>, String>) {
        self.source_reader = reader;
    }

    pub fn create(
        &mut self,
        by: Tier,
        title: &str,
        asks: &[AskInput],
        sources: &[SourceInput],
        granted_roots: &[String],
        now_ms: u64,
    ) -> Result<String, StoreError> {
        let draft = match by {
            Tier::T1User => false,
            Tier::T3Sidecar => true,
            _ => return Err(StoreError::NotAllowed),
        };
        if title.is_empty() || title.chars().count() > TITLE_CAP {
            return Err(StoreError::Schema);
        }
        if asks.len() > ASK_CAP {
            return Err(StoreError::QuotaAsks);
        }
        if sources.len() > SOURCE_CAP {
            return Err(StoreError::QuotaSources);
        }
        for ask in asks {
            if ask.text.is_empty() || ask.text.chars().count() > ASK_CHARS {
                return Err(StoreError::Schema);
            }
        }
        for source in sources {
            if source.reference.is_empty() || source.reference.chars().count() > REF_CAP {
                return Err(StoreError::Schema);
            }
        }
        if self.projects.len() >= PROJECT_CAP {
            return Err(StoreError::QuotaProjects);
        }
        let project_id = self.mint_project_id();
        let dir = self.dir_for(&project_id)?;
        let project = Project {
            project_id: project_id.clone(),
            title: title.to_string(),
            created_by: by,
            created_at: now_ms,
            draft,
            asks: asks
                .iter()
                .enumerate()
                .map(|(index, ask)| Ask {
                    ask_id: format!("ak-{}", index + 1),
                    text: ask.text.clone(),
                    kind: ask.kind,
                })
                .collect(),
            sources: sources
                .iter()
                .enumerate()
                .map(|(index, source)| Source {
                    source_id: format!("sr-{}", index + 1),
                    kind: source.kind,
                    reference: source.reference.clone(),
                    doc_key: None,
                    meta: CitationMeta::default(),
                    status: SourceStatus::Pending,
                    reason: None,
                })
                .collect(),
            cursor: Cursor { source_index: 0, locus_id: None, updated_at: now_ms },
            granted_roots: granted_roots.to_vec(),
        };
        if let Err(err) = write_project_dir(&dir, &project) {
            let _ = fs::remove_dir_all(&dir);
            return Err(err);
        }
        self.projects.insert(
            project_id.clone(),
            Loaded {
                project,
                loci: Vec::new(),
                tasks: Vec::new(),
                events: Vec::new(),
                next_event: 1,
                next_locus: 1,
                last_fsync: None,
            },
        );
        Ok(project_id)
    }

    pub fn add_sources(&mut self, id: &str, sources: &[SourceInput]) -> Result<Vec<String>, StoreError> {
        self.ensure(id)?;
        if sources.is_empty() {
            return Err(StoreError::Schema);
        }
        for source in sources {
            if source.reference.is_empty() || source.reference.chars().count() > REF_CAP {
                return Err(StoreError::Schema);
            }
        }
        let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
        if loaded.project.sources.len() + sources.len() > SOURCE_CAP {
            return Err(StoreError::QuotaSources);
        }
        let start = loaded.project.sources.len();
        let mut ids = Vec::new();
        for (offset, source) in sources.iter().enumerate() {
            let source_id = format!("sr-{}", start + offset + 1);
            ids.push(source_id.clone());
            let loaded = self.projects.get_mut(id).ok_or(StoreError::NoSuchProject)?;
            loaded.project.sources.push(Source {
                source_id,
                kind: source.kind,
                reference: source.reference.clone(),
                doc_key: None,
                meta: CitationMeta::default(),
                status: SourceStatus::Pending,
                reason: None,
            });
        }
        self.rewrite(id)?;
        Ok(ids)
    }

    pub fn set_source_status(&mut self, id: &str, source_id: &str, status: SourceStatus, reason: Option<String>) -> Result<(), StoreError> {
        self.ensure(id)?;
        {
            let loaded = self.projects.get_mut(id).ok_or(StoreError::NoSuchProject)?;
            let source = loaded.project.sources.iter_mut().find(|source| source.source_id == source_id).ok_or(StoreError::Schema)?;
            source.status = status;
            source.reason = if status == SourceStatus::Failed { reason } else { None };
        }
        self.rewrite(id)
    }

    pub fn set_source_meta(&mut self, id: &str, source_id: &str, doc_key: Option<String>, meta: CitationMeta) -> Result<(), StoreError> {
        self.ensure(id)?;
        {
            let loaded = self.projects.get_mut(id).ok_or(StoreError::NoSuchProject)?;
            let source = loaded.project.sources.iter_mut().find(|source| source.source_id == source_id).ok_or(StoreError::Schema)?;
            source.doc_key = doc_key;
            source.meta = meta;
        }
        self.rewrite(id)
    }

    pub fn upsert_locus(&mut self, id: &str, input: LocusInput) -> Result<String, StoreError> {
        let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
        if loaded.project.draft {
            return Err(StoreError::DraftUnconfirmed);
        }
        if input.words.is_empty() || input.words.chars().count() > WORDS_CAP {
            return Err(StoreError::Schema);
        }
        if !loaded.project.sources.iter().any(|source| source.source_id == input.source_id) {
            return Err(StoreError::Schema);
        }
        if let Some(ask_id) = &input.ask_id {
            if !loaded.project.asks.iter().any(|ask| &ask.ask_id == ask_id) {
                return Err(StoreError::NoSuchAsk);
            }
        }
        let locus_id = locus_id(&input.doc_key, input.span_start, input.span_end, &input.words);
        let existing = loaded.loci.iter().find(|row| row.locus.locus_id == locus_id);
        if existing.is_none() && loaded.loci.len() >= LOCUS_CAP {
            return Err(StoreError::QuotaLoci);
        }
        let pseq = existing.map(|row| row.pseq).unwrap_or(loaded.next_locus);
        let locus = Locus {
            locus_id: locus_id.clone(),
            source_id: input.source_id,
            ask_id: input.ask_id,
            doc_key: input.doc_key,
            span_start: input.span_start,
            span_end: input.span_end,
            words: input.words,
            page: input.page,
            line: input.line,
            heading_path: input.heading_path,
            stance: input.stance,
            stance_by: input.stance_by,
            stance_conf: input.stance_conf,
            match_kind: input.match_kind,
            ocr_only: input.ocr_only,
            view: input.view,
        };
        let stored = StoredLocus { pseq, locus };
        let line = serde_json::to_string(&stored).map_err(|_| StoreError::Schema)?;
        let path = self.dir_for(id)?.join("loci.jsonl");
        append_line(&path, &line, false).map_err(|err| {
            eprintln!("[project] locus append: {err}");
            StoreError::Schema
        })?;
        let loaded = self.projects.get_mut(id).ok_or(StoreError::NoSuchProject)?;
        if let Some(row) = loaded.loci.iter_mut().find(|row| row.locus.locus_id == locus_id) {
            *row = stored;
        } else {
            loaded.loci.push(stored);
            loaded.next_locus = loaded.next_locus.max(pseq.saturating_add(1));
        }
        Ok(locus_id)
    }

    pub fn register_task(
        &mut self,
        id: &str,
        task_id: &str,
        mode: &str,
        ask_id: Option<&str>,
        source_id: Option<&str>,
        started_at: u64,
    ) -> Result<(), StoreError> {
        let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
        if loaded.project.draft {
            return Err(StoreError::DraftUnconfirmed);
        }
        if task_id.is_empty() || !matches!(mode, "explain" | "select" | "approve") {
            return Err(StoreError::Schema);
        }
        if let Some(ask_id) = ask_id {
            if !loaded.project.asks.iter().any(|ask| ask.ask_id == ask_id) {
                return Err(StoreError::NoSuchAsk);
            }
        }
        if let Some(source_id) = source_id {
            if !loaded.project.sources.iter().any(|source| source.source_id == source_id) {
                return Err(StoreError::Schema);
            }
        }
        let rec = TaskRec {
            task_id: task_id.to_string(),
            mode: mode.to_string(),
            ask_id: ask_id.map(str::to_string),
            source_id: source_id.map(str::to_string),
            started_at,
        };
        let line = serde_json::to_string(&rec).map_err(|_| StoreError::Schema)?;
        let path = self.dir_for(id)?.join("tasks.jsonl");
        append_line(&path, &line, false).map_err(|err| {
            eprintln!("[project] task append: {err}");
            StoreError::Schema
        })?;
        self.projects.get_mut(id).ok_or(StoreError::NoSuchProject)?.tasks.push(rec);
        Ok(())
    }

    pub fn set_cursor(&mut self, id: &str, cursor: Cursor) -> Result<(), StoreError> {
        self.ensure(id)?;
        self.projects.get_mut(id).ok_or(StoreError::NoSuchProject)?.project.cursor = cursor;
        self.rewrite(id)
    }

    pub fn append_event(&mut self, id: &str, ev: &LedgerEvent) -> Result<u64, StoreError> {
        let (pseq, due) = {
            let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
            let due = match loaded.last_fsync {
                None => true,
                Some(then) => then.elapsed() >= Duration::from_secs(1),
            };
            (loaded.next_event, due)
        };
        let record = ProjectEvent {
            pseq,
            run_id: self.run_id.clone(),
            session_seq: ev.seq,
            ts_ms: ev.ts_ms,
            kind: kind_name(ev.kind).to_string(),
            viewed: ev.viewed.map(viewed_name),
            by: ev.by.wire_name().to_string(),
            locus_id: ev.locus_id.clone(),
            ask_id: ev.ask_id.clone(),
            task_id: ev.task_id.clone(),
            phrase_id: ev.phrase_id.clone(),
        };
        let line = serde_json::to_string(&record).map_err(|_| StoreError::Schema)?;
        let path = self.dir_for(id)?.join("events.jsonl");
        let len = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if len.saturating_add(line.len() as u64 + 1) > EVENT_BYTES {
            return Err(StoreError::QuotaEvents);
        }
        if let Err(err) = drop_partial_tail(&path) {
            eprintln!("[project] events tail: {err}");
            return Err(StoreError::Schema);
        }
        let sync = ev.by == Tier::T0Physical || due;
        if let Err(err) = append_line(&path, &line, sync) {
            eprintln!("[project] events append: {err}");
            return Err(StoreError::Schema);
        }
        let loaded = self.projects.get_mut(id).ok_or(StoreError::NoSuchProject)?;
        let pseq = record.pseq;
        loaded.next_event = pseq.saturating_add(1);
        if sync {
            loaded.last_fsync = Some(Instant::now());
        }
        loaded.events.push(record);
        Ok(pseq)
    }

    pub fn check_binding(&self, project_id: &str, locus_id: &str, ask_id: Option<&str>) -> Result<(), StoreError> {
        let loaded = self.projects.get(project_id).ok_or(StoreError::NoSuchProject)?;
        if !loaded.loci.iter().any(|row| row.locus.locus_id == locus_id) {
            return Err(StoreError::NoSuchLocus);
        }
        if let Some(ask_id) = ask_id {
            if !loaded.project.asks.iter().any(|ask| ask.ask_id == ask_id) {
                return Err(StoreError::NoSuchAsk);
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Project> {
        self.projects.get(id).map(|loaded| &loaded.project)
    }

    pub fn loci(&self, id: &str, since_pseq: Option<u64>, limit: Option<u32>) -> Result<Vec<Locus>, StoreError> {
        let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
        let since = since_pseq.unwrap_or(0);
        let limit = match limit {
            None => DEFAULT_LIMIT,
            Some(n) => (n as usize).min(MAX_LIMIT),
        };
        let mut rows: Vec<&StoredLocus> = loaded.loci.iter().filter(|row| row.pseq > since).collect();
        rows.sort_by_key(|row| row.pseq);
        rows.truncate(limit);
        Ok(rows.into_iter().map(|row| row.locus.clone()).collect())
    }

    pub fn keeps(&self, id: &str) -> Result<Vec<Keep>, StoreError> {
        let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
        let mut out = Vec::new();
        for row in &loaded.loci {
            let events: Vec<&ProjectEvent> = loaded.events.iter().filter(|ev| ev.locus_id.as_deref() == Some(row.locus.locus_id.as_str())).collect();
            if let Some(keep) = keep_for(&events, &loaded.tasks, &row.locus, KeepVia::Select) {
                out.push(keep);
            }
            if let Some(keep) = keep_for(&events, &loaded.tasks, &row.locus, KeepVia::Approve) {
                out.push(keep);
            }
        }
        Ok(out)
    }

    pub fn decisions(&self, id: &str) -> Result<Vec<ProjectEvent>, StoreError> {
        let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
        Ok(loaded.events.iter().filter(|ev| is_decision(ev)).cloned().collect())
    }

    pub fn pseq(&self, id: &str) -> Result<u64, StoreError> {
        let loaded = self.projects.get(id).ok_or(StoreError::NoSuchProject)?;
        Ok(loaded.events.last().map(|ev| ev.pseq).unwrap_or(0))
    }

    pub fn query_value(&self, id: &str, since_pseq: Option<u64>, limit: Option<u32>) -> Result<Value, StoreError> {
        if !valid_project_id(id) {
            return Err(StoreError::Schema);
        }
        let project = self.get(id).ok_or(StoreError::NoSuchProject)?;
        let loci = self.loci(id, since_pseq, limit)?;
        let keeps = self.keeps(id)?;
        let pseq = self.pseq(id)?;
        Ok(json!({
            "project": project,
            "loci": loci,
            "keeps": keeps.iter().map(keep_value).collect::<Vec<_>>(),
            "cursor": project.cursor,
            "pseq": pseq,
        }))
    }

    fn ensure(&self, id: &str) -> Result<(), StoreError> {
        if self.projects.contains_key(id) {
            Ok(())
        } else {
            Err(StoreError::NoSuchProject)
        }
    }

    fn rewrite(&mut self, id: &str) -> Result<(), StoreError> {
        let project = self.projects.get(id).ok_or(StoreError::NoSuchProject)?.project.clone();
        let dir = self.dir_for(id)?;
        atomic_write(&dir.join("project.json"), &serde_json::to_vec_pretty(&project).map_err(|_| StoreError::Schema)?)
    }

    fn dir_for(&self, id: &str) -> Result<PathBuf, StoreError> {
        if !valid_project_id(id) {
            return Err(StoreError::Schema);
        }
        Ok(self.root.join(id))
    }

    fn mint_project_id(&mut self) -> String {
        loop {
            self.mint = self.mint.saturating_add(1);
            let material = format!(
                "{}:{}:{}",
                SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0),
                std::process::id(),
                self.mint
            );
            let id = format!("pj-{}", &sha256_hex(material.as_bytes())[..16]);
            if !self.projects.contains_key(&id) {
                return id;
            }
        }
    }

    fn load_all(&mut self) {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(err) => {
                eprintln!("[project] read {}: {err}", self.root.display());
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
            if !valid_project_id(name) {
                continue;
            }
            match load_dir(&path) {
                Ok(loaded) => {
                    self.projects.insert(loaded.project.project_id.clone(), loaded);
                }
                Err(err) => eprintln!("[project] skip {}: {err}", path.display()),
            }
        }
    }
}

pub fn route_project(proposal: &Proposal, tier: Tier, store: Option<&mut ProjectStore>, now_ms: u64) -> Option<ProposalResult> {
    match proposal {
        Proposal::CreateProject(body) => Some(route_create(tier, store, body, now_ms)),
        Proposal::QueryProject(body) => Some(route_query(tier, store, body)),
        _ => None,
    }
}

fn route_create(tier: Tier, store: Option<&mut ProjectStore>, body: &CreateProjectBody, now_ms: u64) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T3Sidecar) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:createProject");
    }
    let Some(store) = store else {
        return ProposalResult::err(&body.proposal_id, "projects_unavailable");
    };
    let mut asks = Vec::new();
    for ask in &body.asks {
        match ask_input(ask) {
            Ok(ask) => asks.push(ask),
            Err(err) => return ProposalResult::err(&body.proposal_id, err.as_wire()),
        }
    }
    let mut sources = Vec::new();
    for source in &body.sources {
        match source_input(source) {
            Ok(source) => sources.push(source),
            Err(err) => return ProposalResult::err(&body.proposal_id, err.as_wire()),
        }
    }
    match store.create(tier, &body.title, &asks, &sources, &body.granted_roots, now_ms) {
        Ok(project_id) => {
            let draft = store.get(&project_id).is_some_and(|project| project.draft);
            ProposalResult::ok(&body.proposal_id, Some(json!({ "projectId": project_id, "draft": draft })))
        }
        Err(err) => ProposalResult::err(&body.proposal_id, err.as_wire()),
    }
}

fn route_query(tier: Tier, store: Option<&mut ProjectStore>, body: &QueryProjectBody) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T2Client | Tier::T3Sidecar) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:queryProject");
    }
    let Some(store) = store else {
        return ProposalResult::err(&body.proposal_id, "projects_unavailable");
    };
    match store.query_value(&body.project_id, body.since_pseq, body.limit) {
        Ok(detail) => ProposalResult::ok(&body.proposal_id, Some(detail)),
        Err(err) => ProposalResult::err(&body.proposal_id, err.as_wire()),
    }
}

fn ask_input(ask: &crate::protocol::ProjectAskIn) -> Result<AskInput, StoreError> {
    if ask.text.is_empty() || ask.text.chars().count() > ASK_CHARS {
        return Err(StoreError::Schema);
    }
    let kind = match ask.kind {
        ProjectAskKind::Claim => AskKind::Claim,
        ProjectAskKind::Question => AskKind::Question,
    };
    Ok(AskInput { text: ask.text.clone(), kind })
}

fn source_input(source: &ProjectSourceIn) -> Result<SourceInput, StoreError> {
    let mut found = Vec::new();
    if let Some(url) = &source.url {
        found.push((SourceKind::Web, url.clone()));
    }
    if let Some(path) = &source.path {
        found.push((SourceKind::File, path.clone()));
    }
    if let Some(dir) = &source.dir {
        found.push((SourceKind::Dir, dir.clone()));
    }
    if found.len() != 1 {
        return Err(StoreError::Schema);
    }
    let (kind, reference) = found.remove(0);
    if reference.is_empty() || reference.chars().count() > REF_CAP {
        return Err(StoreError::Schema);
    }
    Ok(SourceInput { kind, reference })
}

fn keep_for(events: &[&ProjectEvent], tasks: &[TaskRec], locus: &Locus, via: KeepVia) -> Option<Keep> {
    let mode = match via {
        KeepVia::Select => "select",
        KeepVia::Approve => "approve",
    };
    let mut positive: Option<&ProjectEvent> = None;
    let mut negative: Option<&ProjectEvent> = None;
    for ev in events {
        let Some(task_id) = ev.task_id.as_deref() else { continue };
        let Some(task_mode) = task_mode(tasks, task_id) else { continue };
        if task_mode != mode {
            continue;
        }
        let positive_kind = match via {
            KeepVia::Select => ev.kind == "viewed" && ev.viewed.as_deref() == Some("acknowledged"),
            KeepVia::Approve => ev.kind == "approved",
        };
        let negative_kind = match via {
            KeepVia::Select => ev.kind == "cleared",
            KeepVia::Approve => ev.kind == "discarded",
        };
        if positive_kind {
            positive = Some(*ev);
        } else if negative_kind {
            negative = Some(*ev);
        }
    }
    let positive = positive?;
    if negative.is_some_and(|neg| neg.pseq > positive.pseq) {
        return None;
    }
    Some(Keep {
        locus_id: locus.locus_id.clone(),
        ask_id: positive.ask_id.clone().or_else(|| locus.ask_id.clone()),
        kept_at_ms: positive.ts_ms,
        via,
    })
}

fn task_mode<'a>(tasks: &'a [TaskRec], task_id: &str) -> Option<&'a str> {
    tasks.iter().rev().find(|task| task.task_id == task_id).map(|task| task.mode.as_str())
}

fn is_decision(ev: &ProjectEvent) -> bool {
    if ev.by != "T0" {
        return false;
    }
    match ev.kind.as_str() {
        "approved" | "discarded" | "cleared" => true,
        "viewed" => ev.viewed.as_deref() == Some("acknowledged"),
        _ => false,
    }
}

fn keep_value(keep: &Keep) -> Value {
    let mut value = json!({
        "locusId": keep.locus_id,
        "keptAt": keep.kept_at_ms,
        "via": match keep.via {
            KeepVia::Select => "select",
            KeepVia::Approve => "approve",
        },
    });
    if let Some(ask_id) = &keep.ask_id {
        value["askId"] = json!(ask_id);
    }
    value
}

fn kind_name(kind: LedgerKind) -> &'static str {
    match kind {
        LedgerKind::Located => "located",
        LedgerKind::Viewed => "viewed",
        LedgerKind::Approved => "approved",
        LedgerKind::Discarded => "discarded",
        LedgerKind::Cleared => "cleared",
        LedgerKind::Mismatch => "mismatch",
        LedgerKind::Tour => "tour",
    }
}

fn viewed_name(viewed: ViewedTier) -> String {
    match viewed {
        ViewedTier::Shown => "shown",
        ViewedTier::Dwelled => "dwelled",
        ViewedTier::Acknowledged => "acknowledged",
    }
    .to_string()
}

fn valid_project_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("pj-") else { return false };
    rest.len() == 16 && rest.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn locus_id(doc_key: &str, span_start: u64, span_end: u64, words: &str) -> String {
    let material = format!("{doc_key}\0{span_start}\0{span_end}\0{words}");
    format!("lc-{}", &sha256_hex(material.as_bytes())[..16])
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        out.push(char::from(b"0123456789abcdef"[(byte & 0xf) as usize]));
    }
    out
}

fn mint_run_id() -> String {
    let material = format!(
        "run:{}:{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    );
    format!("run-{}", &sha256_hex(material.as_bytes())[..16])
}

fn write_project_dir(dir: &Path, project: &Project) -> Result<(), StoreError> {
    fs::create_dir_all(dir).map_err(log_io)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(log_io)?;
    let export = dir.join("export");
    fs::create_dir_all(&export).map_err(log_io)?;
    fs::set_permissions(&export, fs::Permissions::from_mode(0o700)).map_err(log_io)?;
    let body = serde_json::to_vec_pretty(project).map_err(|_| StoreError::Schema)?;
    atomic_write(&dir.join("project.json"), &body)?;
    for name in ["loci.jsonl", "tasks.jsonl", "events.jsonl"] {
        touch(&dir.join(name))?;
    }
    Ok(())
}

fn touch(path: &Path) -> Result<(), StoreError> {
    OpenOptions::new().create(true).write(true).mode(0o600).open(path).map_err(log_io)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(log_io)?;
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let tmp = path.with_file_name(".project.json.tmp");
    {
        let mut file = OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(&tmp).map_err(log_io)?;
        file.write_all(bytes).map_err(log_io)?;
        file.sync_all().map_err(log_io)?;
    }
    fs::rename(&tmp, path).map_err(log_io)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(log_io)?;
    Ok(())
}

fn append_line(path: &Path, line: &str, sync: bool) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).mode(0o600).open(path)?;
    writeln!(file, "{line}")?;
    file.flush()?;
    if sync {
        file.sync_all()?;
    }
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    Ok(())
}

fn drop_partial_tail(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::End(-1))?;
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte)?;
    if byte[0] == b'\n' {
        return Ok(());
    }
    let mut pos = len;
    loop {
        pos -= 1;
        file.seek(SeekFrom::Start(pos))?;
        file.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            file.set_len(pos + 1)?;
            return Ok(());
        }
        if pos == 0 {
            file.set_len(0)?;
            return Ok(());
        }
    }
}

fn log_io(err: std::io::Error) -> StoreError {
    eprintln!("[project] {err}");
    StoreError::Schema
}

fn load_dir(dir: &Path) -> Result<Loaded, String> {
    let name = dir.file_name().and_then(|name| name.to_str()).unwrap_or("");
    let raw = fs::read_to_string(dir.join("project.json")).map_err(|err| err.to_string())?;
    let project: Project = serde_json::from_str(&raw).map_err(|err| format!("project.json: {err}"))?;
    if project.project_id != name {
        return Err("project id does not match the directory".to_string());
    }
    let loci = read_jsonl(&dir.join("loci.jsonl"))?;
    let mut stored = Vec::new();
    for line in loci {
        let row: StoredLocus = serde_json::from_value(line).map_err(|err| format!("locus: {err}"))?;
        if let Some(existing) = stored.iter_mut().find(|kept: &&mut StoredLocus| kept.locus.locus_id == row.locus.locus_id) {
            *existing = row;
        } else {
            stored.push(row);
        }
    }
    let tasks = read_jsonl(&dir.join("tasks.jsonl"))?;
    let mut task_rows = Vec::new();
    for line in tasks {
        task_rows.push(serde_json::from_value::<TaskRec>(line).map_err(|err| format!("task: {err}"))?);
    }
    let events = read_jsonl(&dir.join("events.jsonl"))?;
    let mut event_rows = Vec::new();
    for line in events {
        event_rows.push(serde_json::from_value::<ProjectEvent>(line).map_err(|err| format!("event: {err}"))?);
    }
    let next_event = event_rows.last().map(|ev| ev.pseq.saturating_add(1)).unwrap_or(1);
    let next_locus = stored.iter().map(|row| row.pseq).max().unwrap_or(0).saturating_add(1);
    Ok(Loaded {
        project,
        loci: stored,
        tasks: task_rows,
        events: event_rows,
        next_event,
        next_locus,
        last_fsync: None,
    })
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(value) => out.push(value),
            Err(_) if index + 1 == lines.len() && !text.ends_with('\n') => {
                eprintln!("[project] truncated line in {}", path.display());
            }
            Err(err) => return Err(format!("malformed line in {}: {err}", path.display())),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{self, Tier};
    use crate::ledger::{route_ledger, Ledger, LedgerEvent, LedgerKind, ViewedTier};
    use crate::protocol::Proposal;
    use crate::registry::Registry;
    use crate::task::route_task;
    use crate::ui_input::{apply_ui_event, UiEvent};
    use std::fs::OpenOptions;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Temp {
        let dir = std::env::temp_dir().join(format!("presence-project-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }

    fn parse(json: &str) -> Proposal {
        serde_json::from_str(json).unwrap_or_else(|err| panic!("parse {json}: {err}"))
    }

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn locus_in(source_id: &str, words: &str, span_start: u64) -> LocusInput {
        LocusInput {
            source_id: source_id.into(),
            ask_id: None,
            doc_key: "doc".into(),
            span_start,
            span_end: span_start + 10,
            words: words.into(),
            page: None,
            line: None,
            heading_path: None,
            stance: None,
            stance_by: None,
            stance_conf: None,
            match_kind: None,
            ocr_only: false,
            view: None,
        }
    }

    fn observe(phrase: &str, extra: &str) -> String {
        format!(
            r#"{{"kind":"observePhrase","proposalId":"o","phraseId":"{phrase}","query":"the line","docKey":"doc","match":{{"status":"located","conf":0.9}}{extra}}}"#
        )
    }

    fn observe_at(ledger: &mut Ledger, phrase: &str, extra: &str, now: u64) -> crate::protocol::ProposalResult {
        let result = route_ledger(&parse(&observe(phrase, extra)), Tier::T3Sidecar, ledger, now).unwrap();
        assert_eq!(result.status, "ok", "{phrase}: {:?}", result.error);
        result
    }

    fn sample_event(phrase: &str) -> LedgerEvent {
        LedgerEvent {
            seq: 1,
            ts_ms: 1,
            phrase_id: phrase.into(),
            task_id: Some("tk-1".into()),
            kind: LedgerKind::Viewed,
            viewed: Some(ViewedTier::Acknowledged),
            matched: None,
            uv: None,
            query: None,
            doc_key: None,
            by: Tier::T0Physical,
            dwell_ms: None,
            project_id: None,
            ask_id: None,
            locus_id: None,
        }
    }

    fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            let mut entries: Vec<_> = fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().path()).collect();
            entries.sort();
            for path in entries {
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let rel = path.strip_prefix(root).unwrap().to_string_lossy().to_string();
                    out.push((rel, fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out
    }

    fn slice_body(src: &str, marker: &str) -> String {
        let start = src.find(marker).unwrap_or_else(|| panic!("missing {marker}"));
        let brace = src[start..].find('{').unwrap() + start;
        let mut depth = 0i32;
        for (offset, byte) in src.as_bytes().iter().enumerate().skip(brace) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return src[start..=offset].to_string();
                    }
                }
                _ => {}
            }
        }
        panic!("unclosed {marker}");
    }

    fn open_ledger(root: &Path) -> Ledger {
        let mut ledger = Ledger::new();
        ledger.open_projects(root).unwrap();
        ledger
    }

    fn project_with_locus(ledger: &mut Ledger, title: &str) -> (String, String) {
        let id = ledger
            .projects_mut()
            .unwrap()
            .create(
                Tier::T1User,
                title,
                &[AskInput { text: "Is it so?".into(), kind: AskKind::Question }],
                &[SourceInput { kind: SourceKind::File, reference: "/tmp/note.txt".into() }],
                &[],
                1_000,
            )
            .unwrap();
        let locus = ledger.projects_mut().unwrap().upsert_locus(&id, locus_in("sr-1", "the words", 0)).unwrap();
        (id, locus)
    }

    #[test]
    fn create_and_reload() {
        let tmp = scratch("s1");
        let papers = tmp.0.join("papers");
        fs::create_dir(&papers).unwrap();
        fs::write(papers.join("note.txt"), b"FETCHED-BODY-SHOULD-NOT-APPEAR").unwrap();
        let file = tmp.0.join("paper.txt");
        fs::write(&file, b"FILE-BODY-SHOULD-NOT-APPEAR").unwrap();
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        store.set_source_reader(|reference| panic!("project store read a source: {reference}"));
        let id = store
            .create(
                Tier::T1User,
                "Field notes",
                &[
                    AskInput { text: "The claim".into(), kind: AskKind::Claim },
                    AskInput { text: "The question".into(), kind: AskKind::Question },
                ],
                &[
                    SourceInput { kind: SourceKind::Web, reference: "https://example.test/a".into() },
                    SourceInput { kind: SourceKind::File, reference: file.display().to_string() },
                    SourceInput { kind: SourceKind::Dir, reference: papers.display().to_string() },
                ],
                &["/home/ahb/granted".into()],
                1_700,
            )
            .unwrap();
        let original = store.get(&id).unwrap().clone();
        assert!(!original.draft);
        assert!(original.sources.iter().all(|source| source.status == SourceStatus::Pending && source.doc_key.is_none()));
        let raw = fs::read_to_string(tmp.0.join(&id).join("project.json")).unwrap();
        assert!(!raw.contains("FETCHED-BODY-SHOULD-NOT-APPEAR"));
        assert!(!raw.contains("FILE-BODY-SHOULD-NOT-APPEAR"));
        assert!(!raw.contains("docKey"));
        let export = tmp.0.join(&id).join("export");
        assert!(export.is_dir());
        assert_eq!(fs::read_dir(&export).unwrap().count(), 0);
        drop(store);
        let store = ProjectStore::open(&tmp.0).unwrap();
        let loaded = store.get(&id).unwrap();
        assert_eq!(loaded, &original);
        assert_eq!(loaded.asks[0].ask_id, "ak-1");
        assert_eq!(loaded.asks[1].ask_id, "ak-2");
        assert_eq!(loaded.sources[0].kind, SourceKind::Web);
        assert_eq!(loaded.sources[1].kind, SourceKind::File);
        assert_eq!(loaded.sources[2].kind, SourceKind::Dir);
        assert_eq!(loaded.sources[0].source_id, "sr-1");
        assert_eq!(loaded.sources[2].reference, papers.display().to_string());
    }

    #[test]
    fn ids_validated_no_traversal() {
        let tmp = scratch("s2");
        let escaped = tmp.0.join("..").join("x");
        let escaped_before = fs::metadata(&escaped).ok().map(|meta| meta.len());
        let absolute = std::env::temp_dir().join(format!("presence-project-abs-s2-{}", std::process::id()));
        let _ = fs::remove_file(&absolute);
        assert!(!absolute.exists());
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        let ids = [
            "../x".to_string(),
            "pj-ZZ".to_string(),
            "pj-0123456789abcde".to_string(),
            absolute.display().to_string(),
        ];
        for id in &ids {
            let proposal = parse(&format!(r#"{{"kind":"queryProject","proposalId":"q","projectId":"{id}"}}"#));
            let result = route_project(&proposal, Tier::T1User, Some(&mut store), 1).unwrap();
            assert_eq!(result.error.as_deref(), Some("schema"), "{id}");
        }
        assert_eq!(fs::read_dir(&tmp.0).unwrap().count(), 0);
        assert!(!absolute.exists());
        assert_eq!(fs::metadata(&escaped).ok().map(|meta| meta.len()), escaped_before);
        let _ = fs::remove_file(&absolute);
    }

    #[test]
    fn permissions() {
        let tmp = scratch("s3");
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        let id = store.create(Tier::T1User, "Perms", &[], &[], &[], 1).unwrap();
        assert_eq!(mode_of(&tmp.0), 0o700);
        let dir = tmp.0.join(&id);
        assert_eq!(mode_of(&dir), 0o700);
        assert_eq!(mode_of(&dir.join("export")), 0o700);
        for name in ["project.json", "loci.jsonl", "tasks.jsonl", "events.jsonl"] {
            assert_eq!(mode_of(&dir.join(name)), 0o600, "{name}");
        }
    }

    #[test]
    fn locus_id_is_content_address() {
        let tmp = scratch("s4");
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        let id = store
            .create(
                Tier::T1User,
                "Loci",
                &[],
                &[SourceInput { kind: SourceKind::File, reference: "a.txt".into() }],
                &[],
                1,
            )
            .unwrap();
        let mut first = locus_in("sr-1", "alpha", 0);
        let id1 = store.upsert_locus(&id, first.clone()).unwrap();
        assert_eq!(id1, locus_id("doc", 0, 10, "alpha"));
        first.page = Some("9".into());
        first.line = Some(4);
        assert_eq!(store.upsert_locus(&id, first).unwrap(), id1);
        let rows = store.loci(&id, None, Some(10)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].page.as_deref(), Some("9"));
        assert_eq!(rows[0].line, Some(4));
        assert_eq!(rows[0].locus_id, id1);
        let other = store.upsert_locus(&id, locus_in("sr-1", "beta", 0)).unwrap();
        assert_ne!(other, id1);
        let shifted = store.upsert_locus(&id, locus_in("sr-1", "alpha", 3)).unwrap();
        assert_ne!(shifted, id1);
        assert_ne!(shifted, other);
        let rows = store.loci(&id, None, Some(10)).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].locus_id, id1);
        assert_eq!(rows[0].page.as_deref(), Some("9"));
    }

    #[test]
    fn project_events_persist_before_success() {
        let tmp = scratch("s5");
        let mut ledger = open_ledger(&tmp.0);
        let (id, locus) = project_with_locus(&mut ledger, "Events");
        ledger.projects_mut().unwrap().register_task(&id, "tk-1", "select", Some("ak-1"), Some("sr-1"), 1).unwrap();
        let extra = format!(r#","projectId":"{id}","locusId":"{locus}","askId":"ak-1","taskId":"tk-1""#);
        observe_at(&mut ledger, "ph-1", &extra, 10);
        let path = tmp.0.join(&id).join("events.jsonl");
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("\"kind\":\"located\""));
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-1".into() }, 11).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("acknowledged"));
        let keeps = ledger.projects().unwrap().keeps(&id).unwrap();
        assert_eq!(keeps.len(), 1);
        assert_eq!(keeps[0].via, KeepVia::Select);
        assert_eq!(keeps[0].locus_id, locus);
    }

    #[test]
    fn t0_events_fsynced() {
        let prod = include_str!("project.rs").split("mod tests").next().unwrap();
        assert!(slice_body(prod, "fn append_line").contains("sync_all"));
        assert!(slice_body(prod, "fn append_event").contains("T0Physical"));
        let tmp = scratch("s6");
        let mut ledger = open_ledger(&tmp.0);
        let (id, locus) = project_with_locus(&mut ledger, "Sync");
        ledger.projects_mut().unwrap().register_task(&id, "tk-1", "select", Some("ak-1"), Some("sr-1"), 1).unwrap();
        let extra = format!(r#","projectId":"{id}","locusId":"{locus}","taskId":"tk-1""#);
        observe_at(&mut ledger, "ph-sync", &extra, 10);
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-sync".into() }, 11).unwrap();
        let path = tmp.0.join(&id).join("events.jsonl");
        let output = Command::new("cat").arg(&path).output().unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("acknowledged"));
        assert!(text.contains(&locus));
    }

    #[test]
    fn persistence_failure_fails_the_append() {
        let tmp = scratch("s7");
        let mut ledger = open_ledger(&tmp.0);
        let (id, locus) = project_with_locus(&mut ledger, "Locked");
        ledger.projects_mut().unwrap().register_task(&id, "tk-1", "select", Some("ak-1"), Some("sr-1"), 1).unwrap();
        let extra = format!(r#","projectId":"{id}","locusId":"{locus}","taskId":"tk-1""#);
        observe_at(&mut ledger, "ph-lock", &extra, 10);
        let path = tmp.0.join(&id).join("events.jsonl");
        let before = fs::read(&path).unwrap();
        let events_before = ledger.events().len();
        let pseq_before = ledger.projects().unwrap().pseq(&id).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        let err = apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-lock".into() }, 11);
        assert!(err.is_err());
        assert_eq!(ledger.events().len(), events_before);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(ledger.projects().unwrap().pseq(&id).unwrap(), pseq_before);
        assert!(ledger.projects().unwrap().keeps(&id).unwrap().is_empty());
    }

    #[test]
    fn t0_path_takes_no_project_id() {
        let ui = include_str!("ui_input.rs");
        let body = slice_body(ui, "pub enum UiEvent");
        assert!(!body.contains("project"), "{body}");
        let tmp = scratch("s8");
        let mut ledger = open_ledger(&tmp.0);
        let (id, locus) = project_with_locus(&mut ledger, "Stamp");
        let extra = format!(r#","projectId":"{id}","locusId":"{locus}","askId":"ak-1","taskId":"tk-1""#);
        observe_at(&mut ledger, "ph-bound", &extra, 10);
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-bound".into() }, 11).unwrap();
        let path = tmp.0.join(&id).join("events.jsonl");
        let bound_lines = fs::read_to_string(&path).unwrap().lines().count();
        let stamped = ledger.events().iter().find(|ev| ev.phrase_id == "ph-bound" && ev.viewed == Some(ViewedTier::Acknowledged)).unwrap();
        assert_eq!(stamped.project_id.as_deref(), Some(id.as_str()));
        assert_eq!(stamped.locus_id.as_deref(), Some(locus.as_str()));
        assert_eq!(stamped.ask_id.as_deref(), Some("ak-1"));
        observe_at(&mut ledger, "ph-free", "", 12);
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-free".into() }, 13).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), bound_lines);
        let free = ledger.events().iter().find(|ev| ev.phrase_id == "ph-free" && ev.viewed == Some(ViewedTier::Acknowledged)).unwrap();
        assert!(free.project_id.is_none());
        assert!(free.ask_id.is_none());
        assert!(free.locus_id.is_none());
    }

    #[test]
    fn observe_with_unknown_project_or_locus() {
        let tmp = scratch("s9");
        let mut ledger = open_ledger(&tmp.0);
        let (id, locus) = project_with_locus(&mut ledger, "Missing");
        let path = tmp.0.join(&id).join("events.jsonl");
        let before = fs::read(&path).unwrap();
        let cases = [
            (format!(r#","projectId":"pj-deadbeefdeadbeef","locusId":"{locus}""#), "no_such_project"),
            (format!(r#","projectId":"{id}","locusId":"lc-deadbeefdeadbeef""#), "no_such_locus"),
            (format!(r#","projectId":"{id}","locusId":"{locus}","askId":"ak-99""#), "no_such_ask"),
            (format!(r#","projectId":"{id}""#), "schema"),
            (r#","askId":"ak-1""#.to_string(), "schema"),
            (format!(r#","locusId":"{locus}""#), "schema"),
        ];
        for (extra, expect) in &cases {
            let result = route_ledger(&parse(&observe("ph-miss", extra)), Tier::T3Sidecar, &mut ledger, 10).unwrap();
            assert_eq!(result.error.as_deref(), Some(*expect), "{extra}");
            assert!(ledger.events().is_empty());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn keeps_by_mode() {
        let tmp = scratch("s10");
        let mut ledger = open_ledger(&tmp.0);
        let id = ledger
            .projects_mut()
            .unwrap()
            .create(
                Tier::T1User,
                "Keeps",
                &[AskInput { text: "Ask".into(), kind: AskKind::Claim }],
                &[SourceInput { kind: SourceKind::File, reference: "a.txt".into() }],
                &[],
                1,
            )
            .unwrap();
        let sel = ledger.projects_mut().unwrap().upsert_locus(&id, locus_in("sr-1", "select words", 0)).unwrap();
        let app = ledger.projects_mut().unwrap().upsert_locus(&id, locus_in("sr-1", "approve words", 1)).unwrap();
        let exp = ledger.projects_mut().unwrap().upsert_locus(&id, locus_in("sr-1", "explain words", 2)).unwrap();
        let unk = ledger.projects_mut().unwrap().upsert_locus(&id, locus_in("sr-1", "unknown words", 3)).unwrap();
        ledger.projects_mut().unwrap().register_task(&id, "tk-sel", "select", Some("ak-1"), Some("sr-1"), 1).unwrap();
        ledger.projects_mut().unwrap().register_task(&id, "tk-app", "approve", Some("ak-1"), Some("sr-1"), 1).unwrap();
        ledger.projects_mut().unwrap().register_task(&id, "tk-exp", "explain", Some("ak-1"), Some("sr-1"), 1).unwrap();
        let keeps = |ledger: &Ledger, locus: &str| {
            ledger.projects().unwrap().keeps(&id).unwrap().into_iter().filter(|keep| keep.locus_id == locus).collect::<Vec<_>>()
        };
        let bind = |locus: &str, task: &str| format!(r#","projectId":"{id}","locusId":"{locus}","askId":"ak-1","taskId":"{task}""#);
        observe_at(&mut ledger, "ph-sel", &bind(&sel, "tk-sel"), 10);
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-sel".into() }, 11).unwrap();
        assert_eq!(keeps(&ledger, &sel).len(), 1);
        assert_eq!(keeps(&ledger, &sel)[0].via, KeepVia::Select);
        apply_ui_event(&mut ledger, UiEvent::Uncheck { phrase_id: "ph-sel".into() }, 12).unwrap();
        assert!(keeps(&ledger, &sel).is_empty());
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-sel".into() }, 13).unwrap();
        assert_eq!(keeps(&ledger, &sel).len(), 1);
        observe_at(&mut ledger, "ph-app", &bind(&app, "tk-app"), 14);
        apply_ui_event(&mut ledger, UiEvent::Approve { phrase_id: "ph-app".into() }, 15).unwrap();
        assert_eq!(keeps(&ledger, &app).len(), 1);
        assert_eq!(keeps(&ledger, &app)[0].via, KeepVia::Approve);
        apply_ui_event(&mut ledger, UiEvent::Discard { phrase_id: "ph-app".into() }, 16).unwrap();
        assert!(keeps(&ledger, &app).is_empty());
        observe_at(&mut ledger, "ph-exp", &bind(&exp, "tk-exp"), 17);
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-exp".into() }, 18).unwrap();
        assert!(keeps(&ledger, &exp).is_empty());
        observe_at(&mut ledger, "ph-unk", &bind(&unk, "tk-missing"), 19);
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-unk".into() }, 20).unwrap();
        assert!(keeps(&ledger, &unk).is_empty());
    }

    #[test]
    fn survives_restart_timeout_retire() {
        let tmp = scratch("s11");
        let root = tmp.0.clone();
        let (id, locus, high, kept, cursor, words) = {
            let mut ledger = open_ledger(&root);
            let (id, locus) = project_with_locus(&mut ledger, "Survives");
            ledger.projects_mut().unwrap().register_task(&id, "tk-1", "select", Some("ak-1"), Some("sr-1"), 1).unwrap();
            let extra = format!(r#","projectId":"{id}","locusId":"{locus}","askId":"ak-1","taskId":"tk-1""#);
            observe_at(&mut ledger, "ph-live", &extra, 10);
            apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "ph-live".into() }, 11).unwrap();
            let cursor = Cursor { source_index: 2, locus_id: Some(locus.clone()), updated_at: 42 };
            ledger.projects_mut().unwrap().set_cursor(&id, cursor.clone()).unwrap();
            let high = ledger.projects().unwrap().pseq(&id).unwrap();
            let kept = ledger.projects().unwrap().keeps(&id).unwrap();
            let words = ledger.projects().unwrap().loci(&id, None, Some(10)).unwrap()[0].words.clone();
            (id, locus, high, kept, cursor, words)
        };
        let mut reg = Registry::new();
        reg.open_projects(&root).unwrap();
        assert!(reg.ledger().events().is_empty());
        assert_eq!(reg.ledger().association_len(), 0);
        assert_eq!(reg.projects_mut().unwrap().pseq(&id).unwrap(), high);
        assert_eq!(reg.projects_mut().unwrap().keeps(&id).unwrap(), kept);
        assert_eq!(reg.projects_mut().unwrap().get(&id).unwrap().cursor, cursor);
        assert_eq!(reg.projects_mut().unwrap().loci(&id, None, Some(10)).unwrap()[0].words, words);
        let extra = format!(r#","projectId":"{id}","locusId":"{locus}","askId":"ak-1","taskId":"tk-1""#);
        observe_at(reg.ledger_mut(), "ph-next", &extra, 30);
        assert_eq!(reg.projects_mut().unwrap().pseq(&id).unwrap(), high + 1);
        let snap = snapshot(&root);
        assert!(reg.spawn_presence("p-1", "ctx", None, None) || reg.has_presence("p-1"));
        let proposal = parse(
            r#"{"kind":"createTask","proposalId":"c","presenceId":"p-1","mode":"explain","ttlSeconds":1,"items":[{"query":"the line"}]}"#,
        );
        {
            let (book, ledger) = reg.task_and_ledger();
            let result = route_task(&proposal, Tier::T2Client, book, ledger, true, 40).unwrap();
            assert_eq!(result.status, "ok", "{:?}", result.error);
        }
        reg.tick_tasks(40 + 60_000);
        reg.retire_presence("p-1").unwrap();
        assert_eq!(snapshot(&root), snap);
    }

    #[test]
    fn truncated_last_line_tolerated() {
        let tmp = scratch("s12");
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        let bad = store.create(Tier::T1User, "Bad", &[], &[], &[], 1).unwrap();
        let good = store.create(Tier::T1User, "Good", &[], &[], &[], 1).unwrap();
        drop(store);
        fs::write(tmp.0.join(&bad).join("loci.jsonl"), "{\"no\":true}\n{\"still\":1}\n").unwrap();
        let line = r#"{"pseq":1,"runId":"run-0123456789abcdef","sessionSeq":1,"tsMs":5,"kind":"located","by":"T3","phraseId":"ph-1"}"#;
        let mut body = Vec::new();
        body.extend_from_slice(line.as_bytes());
        body.push(b'\n');
        body.extend_from_slice(b"FRAGMENT");
        fs::write(tmp.0.join(&good).join("events.jsonl"), &body).unwrap();
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        assert!(store.get(&bad).is_none());
        assert!(store.get(&good).is_some());
        assert_eq!(store.pseq(&good).unwrap(), 1);
        store.append_event(&good, &sample_event("ph-2")).unwrap();
        let text = fs::read_to_string(tmp.0.join(&good).join("events.jsonl")).unwrap();
        assert!(!text.contains("FRAGMENT"));
        assert!(text.contains("ph-1"));
        assert!(text.contains("ph-2"));
        assert!(text.ends_with('\n'));
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn caps() {
        let tmp = scratch("s13");
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        let asks: Vec<_> = (0..21).map(|i| AskInput { text: format!("q{i}"), kind: AskKind::Question }).collect();
        assert_eq!(store.create(Tier::T1User, "Too many asks", &asks, &[], &[], 1).unwrap_err(), StoreError::QuotaAsks);
        assert_eq!(fs::read_dir(&tmp.0).unwrap().count(), 0);
        let sources: Vec<_> = (0..500).map(|i| SourceInput { kind: SourceKind::File, reference: format!("f{i}") }).collect();
        let id = store.create(Tier::T1User, "Heavy", &[], &sources, &[], 1).unwrap();
        let project_path = tmp.0.join(&id).join("project.json");
        let before_sources = fs::metadata(&project_path).unwrap().len();
        let err = store.add_sources(&id, &[SourceInput { kind: SourceKind::File, reference: "one-more".into() }]).unwrap_err();
        assert_eq!(err, StoreError::QuotaSources);
        assert_eq!(store.get(&id).unwrap().sources.len(), 500);
        assert_eq!(fs::metadata(&project_path).unwrap().len(), before_sources);
        for i in 0..5000 {
            store.upsert_locus(&id, locus_in("sr-1", &format!("w{i}"), i)).unwrap();
        }
        let loci_path = tmp.0.join(&id).join("loci.jsonl");
        let before_loci = fs::metadata(&loci_path).unwrap().len();
        assert_eq!(store.upsert_locus(&id, locus_in("sr-1", "overflow", 9_000)).unwrap_err(), StoreError::QuotaLoci);
        assert_eq!(fs::metadata(&loci_path).unwrap().len(), before_loci);
        assert_eq!(store.loci(&id, Some(4999), Some(10)).unwrap().len(), 1);
        let events = tmp.0.join(&id).join("events.jsonl");
        OpenOptions::new().write(true).open(&events).unwrap().set_len(50 * 1024 * 1024 + 1).unwrap();
        let before_events = fs::metadata(&events).unwrap().len();
        assert_eq!(store.append_event(&id, &sample_event("ph-cap")).unwrap_err(), StoreError::QuotaEvents);
        assert_eq!(fs::metadata(&events).unwrap().len(), before_events);
        assert_eq!(store.pseq(&id).unwrap(), 0);
        for i in 0..99 {
            store.create(Tier::T1User, &format!("p{i}"), &[], &[], &[], 1).unwrap();
        }
        assert_eq!(store.create(Tier::T1User, "one more project", &[], &[], &[], 1).unwrap_err(), StoreError::QuotaProjects);
        let dirs = fs::read_dir(&tmp.0).unwrap().filter(|entry| entry.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(dirs, 100);
    }

    #[test]
    fn nothing_in_locate_or_moves_reads_projects() {
        let locate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/locate");
        let mut saw = false;
        for entry in fs::read_dir(&locate).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("ts") {
                continue;
            }
            saw = true;
            let text = fs::read_to_string(&path).unwrap();
            assert!(!text.contains("PRESENCE_PROJECTS_DIR"), "{}", path.display());
            assert!(!text.contains("ProjectStore"), "{}", path.display());
        }
        assert!(saw);
        let task = include_str!("task.rs");
        for name in ["planned_for", "move_fits", "authorize"] {
            let body = slice_body(task, &format!("fn {name}"));
            assert!(!body.contains("PRESENCE_PROJECTS_DIR"), "{name}");
            assert!(!body.contains("ProjectStore"), "{name}");
        }
    }

    #[test]
    fn create_project_tiers() {
        let tmp = scratch("s15");
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        let proposal = parse(r#"{"kind":"createProject","proposalId":"c","title":"Tiers","asks":[],"sources":[{"path":"a.txt"}]}"#);
        let denied = gate::check(Tier::T2Client, &proposal).unwrap_err();
        assert_eq!(denied.error.as_deref(), Some("not_allowed_for_tier:createProject"));
        let routed = route_project(&proposal, Tier::T2Client, Some(&mut store), 1).unwrap();
        assert_eq!(routed.error.as_deref(), Some("not_allowed_for_tier:createProject"));
        assert_eq!(store.create(Tier::T2Client, "Tiers", &[], &[], &[], 1).unwrap_err(), StoreError::NotAllowed);
        assert_eq!(fs::read_dir(&tmp.0).unwrap().count(), 0);
        let t1 = route_project(&proposal, Tier::T1User, Some(&mut store), 2).unwrap();
        assert_eq!(t1.status, "ok", "{:?}", t1.error);
        assert_eq!(t1.detail.unwrap()["draft"], false);
        let t3_body = parse(r#"{"kind":"createProject","proposalId":"d","title":"Draft","asks":[],"sources":[{"path":"a.txt"}]}"#);
        let t3 = route_project(&t3_body, Tier::T3Sidecar, Some(&mut store), 3).unwrap();
        assert_eq!(t3.status, "ok", "{:?}", t3.error);
        let detail = t3.detail.unwrap();
        assert_eq!(detail["draft"], true);
        let id = detail["projectId"].as_str().unwrap().to_string();
        assert!(store.get(&id).unwrap().draft);
        let tasks = fs::read(tmp.0.join(&id).join("tasks.jsonl")).unwrap();
        let loci = fs::read(tmp.0.join(&id).join("loci.jsonl")).unwrap();
        assert_eq!(
            store.register_task(&id, "tk-1", "select", None, Some("sr-1"), 4).unwrap_err(),
            StoreError::DraftUnconfirmed
        );
        assert_eq!(store.upsert_locus(&id, locus_in("sr-1", "words", 0)).unwrap_err(), StoreError::DraftUnconfirmed);
        assert_eq!(fs::read(tmp.0.join(&id).join("tasks.jsonl")).unwrap(), tasks);
        assert_eq!(fs::read(tmp.0.join(&id).join("loci.jsonl")).unwrap(), loci);
    }

    #[test]
    fn create_project_reads_nothing() {
        let prod = include_str!("project.rs").split("mod tests").next().unwrap();
        let body = slice_body(prod, "fn create(");
        assert!(!body.contains("read_dir"), "{body}");
        assert!(!body.contains("pdftotext"));
        assert!(!body.contains("fs::read"));
        assert!(!body.contains("source_reader"));
        let tmp = scratch("s16");
        let docs = tmp.0.join("docs");
        fs::create_dir(&docs).unwrap();
        fs::write(docs.join("child.txt"), b"secret-bytes").unwrap();
        let file = tmp.0.join("paper.txt");
        fs::write(&file, b"file-bytes").unwrap();
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        store.set_source_reader(|reference| panic!("project store read a source: {reference}"));
        let id = store
            .create(
                Tier::T1User,
                "Unread",
                &[],
                &[
                    SourceInput { kind: SourceKind::Web, reference: "https://example.test/paper".into() },
                    SourceInput { kind: SourceKind::File, reference: file.display().to_string() },
                    SourceInput { kind: SourceKind::Dir, reference: docs.display().to_string() },
                ],
                &[],
                1,
            )
            .unwrap();
        let project = store.get(&id).unwrap();
        assert_eq!(project.sources[0].reference, "https://example.test/paper");
        assert_eq!(project.sources[1].reference, file.display().to_string());
        assert_eq!(project.sources[2].reference, docs.display().to_string());
        assert!(project.sources.iter().all(|source| source.status == SourceStatus::Pending && source.doc_key.is_none()));
        let raw = fs::read_to_string(tmp.0.join(&id).join("project.json")).unwrap();
        assert!(!raw.contains("secret-bytes"));
        assert!(!raw.contains("file-bytes"));
    }

    #[test]
    fn query_paging() {
        let tmp = scratch("s17");
        let mut store = ProjectStore::open(&tmp.0).unwrap();
        let id = store
            .create(
                Tier::T1User,
                "Pages",
                &[],
                &[SourceInput { kind: SourceKind::File, reference: "a.txt".into() }],
                &[],
                1,
            )
            .unwrap();
        let mut ids = Vec::new();
        for i in 0..501 {
            ids.push(store.upsert_locus(&id, locus_in("sr-1", &format!("p{i}"), i)).unwrap());
        }
        let query = |store: &mut ProjectStore, extra: &str| {
            let proposal = parse(&format!(r#"{{"kind":"queryProject","proposalId":"q","projectId":"{id}"{extra}}}"#));
            let result = route_project(&proposal, Tier::T2Client, Some(store), 1).unwrap();
            assert_eq!(result.status, "ok", "{:?}", result.error);
            result.detail.unwrap()
        };
        let first = query(&mut store, "");
        assert_eq!(first["pseq"], 0);
        assert_eq!(first["loci"].as_array().unwrap().len(), 100);
        assert_eq!(first["loci"][0]["locusId"], ids[0]);
        assert_eq!(first["loci"][99]["locusId"], ids[99]);
        assert!(first["loci"].as_array().unwrap().iter().all(|row| row.get("pseq").is_none()));
        let page = query(&mut store, r#","sincePseq":1"#);
        assert_eq!(page["loci"][0]["locusId"], ids[1]);
        assert_eq!(page["loci"].as_array().unwrap().len(), 100);
        let capped = query(&mut store, r#","limit":1000"#);
        assert_eq!(capped["loci"].as_array().unwrap().len(), 500);
        assert_eq!(capped["loci"][0]["locusId"], ids[0]);
        let tail = query(&mut store, r#","sincePseq":500"#);
        assert_eq!(tail["loci"].as_array().unwrap().len(), 1);
        assert_eq!(tail["loci"][0]["locusId"], ids[500]);
    }

    #[test]
    fn store_unavailable_is_fail_soft() {
        let tmp = scratch("s18");
        let file = tmp.0.join("not-a-directory");
        fs::write(&file, b"x").unwrap();
        let mut reg = Registry::new();
        assert!(reg.open_projects(&file).is_err());
        assert!(reg.projects_mut().is_none());
        let create = parse(r#"{"kind":"createProject","proposalId":"c","title":"t","asks":[],"sources":[]}"#);
        let result = route_project(&create, Tier::T1User, reg.projects_mut(), 1).unwrap();
        assert_eq!(result.error.as_deref(), Some("projects_unavailable"));
        let ok = route_ledger(&parse(&observe("ph-free", "")), Tier::T3Sidecar, reg.ledger_mut(), 2).unwrap();
        assert_eq!(ok.status, "ok", "{:?}", ok.error);
        assert_eq!(reg.ledger().events().len(), 1);
        let denied = route_ledger(
            &parse(&observe("ph-bound", r#","projectId":"pj-0123456789abcdef","locusId":"lc-0123456789abcdef""#)),
            Tier::T3Sidecar,
            reg.ledger_mut(),
            3,
        )
        .unwrap();
        assert_eq!(denied.error.as_deref(), Some("projects_unavailable"));
        assert_eq!(reg.ledger().events().len(), 1);
        let half = route_ledger(
            &parse(&observe("ph-half", r#","projectId":"pj-0123456789abcdef""#)),
            Tier::T3Sidecar,
            reg.ledger_mut(),
            4,
        )
        .unwrap();
        assert_eq!(half.error.as_deref(), Some("schema"));
        assert_eq!(reg.ledger().events().len(), 1);
    }

    #[test]
    fn wire_omits_empty_project_fields() {
        let bare = sample_event("ph-bare");
        let wire = bare.wire();
        assert!(wire.get("projectId").is_none());
        assert!(wire.get("askId").is_none());
        assert!(wire.get("locusId").is_none());
        let mut set = bare.clone();
        set.project_id = Some("pj-0123456789abcdef".into());
        set.ask_id = Some("ak-1".into());
        set.locus_id = Some("lc-0123456789abcdef".into());
        let wire = set.wire();
        assert_eq!(wire["projectId"], "pj-0123456789abcdef");
        assert_eq!(wire["askId"], "ak-1");
        assert_eq!(wire["locusId"], "lc-0123456789abcdef");
        let tmp = scratch("s19");
        let mut ledger = open_ledger(&tmp.0);
        observe_at(&mut ledger, "ph-free", "", 10);
        let (id, locus) = project_with_locus(&mut ledger, "Wire");
        let extra = format!(r#","projectId":"{id}","locusId":"{locus}","askId":"ak-1""#);
        observe_at(&mut ledger, "ph-bound", &extra, 11);
        let listed = route_ledger(
            &parse(r#"{"kind":"listPhraseFlags","proposalId":"p"}"#),
            Tier::T1User,
            &mut ledger,
            12,
        )
        .unwrap();
        let rows = listed.detail.unwrap();
        let free = rows["rows"].as_array().unwrap().iter().find(|row| row["phraseId"] == "ph-free").unwrap();
        assert!(free.get("projectId").is_none());
        assert!(free.get("askId").is_none());
        assert!(free.get("locusId").is_none());
        let bound = rows["rows"].as_array().unwrap().iter().find(|row| row["phraseId"] == "ph-bound").unwrap();
        assert_eq!(bound["projectId"], id);
        assert_eq!(bound["askId"], "ak-1");
        assert_eq!(bound["locusId"], locus);
        let listed = route_ledger(
            &parse(r#"{"kind":"listPhraseFlags","proposalId":"p","events":true}"#),
            Tier::T1User,
            &mut ledger,
            13,
        )
        .unwrap();
        let events = listed.detail.unwrap();
        let free_ev = events["events"].as_array().unwrap().iter().find(|ev| ev["phraseId"] == "ph-free").unwrap();
        assert!(free_ev.get("projectId").is_none());
        let bound_ev = events["events"].as_array().unwrap().iter().find(|ev| ev["phraseId"] == "ph-bound").unwrap();
        assert_eq!(bound_ev["projectId"], id);
        assert_eq!(bound_ev["locusId"], locus);
        assert_eq!(bound_ev["askId"], "ak-1");
    }

    #[test]
    fn create_task_body_has_no_project_field() {
        let line = r#"{"kind":"createTask","proposalId":"c","presenceId":"p","mode":"explain","items":[{"query":"the line"}],"projectId":"pj-0123456789abcdef"}"#;
        assert!(serde_json::from_str::<Proposal>(line).is_err());
    }
}
