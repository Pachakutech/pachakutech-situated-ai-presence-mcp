//! In-memory phrase ledger for one daemon run. Empty at start. Not kept
//! across a restart. The quotation a citation stores lives in the project
//! store (C7); this log is the glass session, capped at 50 phrases and
//! 200-character snippets.

use crate::gate::Tier;
use crate::highlight::UvBox;
use crate::note_text::{self, NoteCarry, NoteFact, NoteFlags, NoteVia, NoteView};
use crate::protocol::{
    AddNoteBody, DeleteNoteBody, ListPhraseFlagsBody, MatchInfo, MatchStatus, NoteVia as WireVia, ObservePhraseBody,
    ObservedViewed, Proposal, ProposalResult,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub const PHRASE_CAP: usize = 50;
pub const EVENT_CAP: usize = 5000;
pub const TEXT_CAP: usize = 200;
pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 200;
pub const SUBSCRIBE_LAG: usize = 1000;
const NOTE_ARM_MS: u64 = 60_000;
const NOTE_RATE_MS: u64 = 2_000;
const NOTES_PER_PHRASE: usize = 20;
const NOTES_PER_PROJECT: usize = 200;
const NOTES_PER_SESSION: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerKind {
    Located,
    Viewed,
    Approved,
    Discarded,
    Cleared,
    Mismatch,
    Tour,
    Note,
    NoteDeleted,
    /// Hand on the highlight. Latest row for the locus is the condition.
    Skip,
    Accept,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewedTier {
    Shown,
    Dwelled,
    Acknowledged,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LedgerEvent {
    pub seq: u64,
    pub ts_ms: u64,
    pub phrase_id: String,
    pub task_id: Option<String>,
    pub kind: LedgerKind,
    pub viewed: Option<ViewedTier>,
    pub matched: Option<MatchInfo>,
    pub uv: Option<UvBox>,
    pub query: Option<String>,
    pub doc_key: Option<String>,
    pub by: Tier,
    /// Set on a tour enter or leave. Absent on every other kind, and omitted
    /// from the wire when absent so older event JSON stays stable.
    pub dwell_ms: Option<u64>,
    /// Stamped from the phrase association. Omitted from the wire when absent.
    pub project_id: Option<String>,
    pub ask_id: Option<String>,
    pub locus_id: Option<String>,
    /// Set on `Note` and `NoteDeleted`. Omitted from the wire when absent.
    pub note: Option<NoteCarry>,
}

#[derive(Clone)]
struct PhraseBind {
    project_id: String,
    locus_id: String,
    ask_id: Option<String>,
}

/// One `bind_shown_locus` call. `undo_shown` puts the previous association back.
pub(crate) struct BindingPlant {
    phrase_id: String,
    previous: Option<PhraseBind>,
}

/// Socket-visible observations. This enum cannot represent an acknowledgement,
/// an approval, a discard, or a clear.
#[derive(Debug, Clone)]
pub enum ObservedAppend {
    Located {
        phrase_id: String,
        task_id: Option<String>,
        query: Option<String>,
        doc_key: Option<String>,
        matched: MatchInfo,
        uv: Option<UvBox>,
    },
    Viewed {
        phrase_id: String,
        task_id: Option<String>,
        viewed: ObservedViewed,
        uv: Option<UvBox>,
    },
    Mismatch {
        phrase_id: String,
        task_id: Option<String>,
        matched: MatchInfo,
    },
    Tour {
        phrase_id: String,
        task_id: Option<String>,
        query: Option<String>,
        dwell_ms: u64,
    },
}

/// Physical-input records. `append_user` is the only writer, and it stamps `T0`.
#[derive(Debug, Clone)]
pub enum UserAppend {
    Acknowledged { phrase_id: String },
    Approved { phrase_id: String },
    Discarded { phrase_id: String },
    Cleared { phrase_id: String },
    /// Tint gestures. These do not stamp `viewed: acknowledged`.
    Skip { phrase_id: String },
    Accept { phrase_id: String },
    Deny { phrase_id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum LedgerError {
    Quota,
    Schema,
    Rejected(&'static str),
}

#[derive(Debug)]
pub enum SubPoll {
    Lines { lines: Vec<String>, through: u64 },
    Overflow { floor: u64 },
}

pub struct Ledger {
    events: Vec<LedgerEvent>,
    next_seq: u64,
    next_mint: u64,
    /// Highest seq removed by compaction. A `since` below this is not a
    /// complete event stream.
    compacted_below: u64,
    /// Phrase ids a task holds before any event is written. They count
    /// toward the session cap so two creates cannot pass 50.
    reserved: HashSet<String>,
    /// Copy of each appended event. Absent unless `attach_watch` opened one.
    /// Not opened from `new`, so tests that build a ledger do not touch a file.
    watch: Option<std::fs::File>,
    /// Phrase to project, for this process only. Empty when the ledger is new.
    assoc: HashMap<String, PhraseBind>,
    /// Registry-owned store. `new` leaves it closed so tests do not touch the
    /// user's project directory. The overlay opens it after the socket binds.
    projects: Option<crate::project::ProjectStore>,
    arming: Option<Arming>,
    note_mint: u64,
    last_note_ms: HashMap<String, u64>,
}

struct Arming {
    phrase_id: String,
    at_ms: u64,
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            next_seq: 1,
            next_mint: 1,
            compacted_below: 0,
            reserved: HashSet::new(),
            watch: None,
            assoc: HashMap::new(),
            projects: None,
            arming: None,
            note_mint: 0,
            last_note_ms: HashMap::new(),
        }
    }

    pub fn open_projects(&mut self, root: &std::path::Path) -> std::io::Result<()> {
        self.projects = Some(crate::project::ProjectStore::open(root)?);
        Ok(())
    }

    pub fn projects(&self) -> Option<&crate::project::ProjectStore> {
        self.projects.as_ref()
    }

    pub fn projects_mut(&mut self) -> Option<&mut crate::project::ProjectStore> {
        self.projects.as_mut()
    }

    pub(crate) fn take_projects(&mut self) -> Option<crate::project::ProjectStore> {
        self.projects.take()
    }

    pub(crate) fn give_projects(&mut self, store: crate::project::ProjectStore) {
        self.projects = Some(store);
    }

    pub fn association_len(&self) -> usize {
        self.assoc.len()
    }

    fn remember_binding(&mut self, phrase_id: &str, project_id: String, locus_id: String, ask_id: Option<String>) {
        self.assoc.insert(
            phrase_id.to_string(),
            PhraseBind { project_id, locus_id, ask_id },
        );
    }

    fn take_binding(&mut self, phrase_id: &str) -> Option<PhraseBind> {
        self.assoc.remove(phrase_id)
    }

    fn restore_binding(&mut self, phrase_id: &str, previous: Option<PhraseBind>) {
        match previous {
            Some(bind) => {
                self.assoc.insert(phrase_id.to_string(), bind);
            }
            None => {
                self.assoc.remove(phrase_id);
            }
        }
    }

    /// Phrase id and locus id are the same string. The tour commit is the caller.
    pub fn bind_shown_locus(
        &mut self,
        phrase_id: &str,
        project_id: &str,
        locus_id: &str,
        ask_id: Option<String>,
    ) -> Result<BindingPlant, LedgerError> {
        if phrase_id.is_empty() || project_id.is_empty() || phrase_id != locus_id {
            return Err(LedgerError::Schema);
        }
        let previous = self.assoc.get(phrase_id).cloned();
        self.remember_binding(phrase_id, project_id.to_string(), locus_id.to_string(), ask_id);
        Ok(BindingPlant { phrase_id: phrase_id.to_string(), previous })
    }

    pub(crate) fn undo_shown(&mut self, plants: &[BindingPlant]) {
        for plant in plants.iter().rev() {
            self.restore_binding(&plant.phrase_id, plant.previous.clone());
        }
    }

    /// Latest hand row for this phrase. The session log wins when it has one.
    /// After a restart that log is empty, so the project record supplies the color.
    pub fn latest_hand(&self, phrase_id: &str) -> Option<LedgerKind> {
        let phrase_id = cut(phrase_id.to_string());
        if let Some(kind) = self
            .events
            .iter()
            .rev()
            .find(|ev| ev.phrase_id == phrase_id && is_hand(ev.kind))
            .map(|ev| ev.kind)
        {
            return Some(kind);
        }
        let bind = self.assoc.get(&phrase_id)?;
        let store = self.projects.as_ref()?;
        match store.latest_hand_wire(&bind.project_id, &bind.locus_id)? {
            "accept" => Some(LedgerKind::Accept),
            "deny" => Some(LedgerKind::Deny),
            _ => Some(LedgerKind::Skip),
        }
    }

    fn latest_task_id(&self, phrase_id: &str) -> Option<String> {
        let phrase_id = cut(phrase_id.to_string());
        self.events.iter().rev().find(|ev| ev.phrase_id == phrase_id && ev.task_id.is_some()).and_then(|ev| ev.task_id.clone())
    }

    /// Open an absolute watch path. A relative path is logged and left closed.
    /// The parent is created mode `0700` when it is missing. The file is mode `0600`.
    /// A failed open is logged. The ledger itself is unchanged.
    pub fn attach_watch(&mut self, raw: &str) {
        self.watch = None;
        let path = Path::new(raw);
        if !path.is_absolute() {
            eprintln!("[ledger] watch path is not absolute: {raw}");
            return;
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                if let Err(err) = fs::create_dir_all(parent) {
                    eprintln!("[ledger] watch parent: {err}");
                    return;
                }
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }
        match OpenOptions::new().create(true).append(true).mode(0o600).open(path) {
            Ok(file) => {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
                self.watch = Some(file);
            }
            Err(err) => eprintln!("[ledger] watch open failed: {err}"),
        }
    }

    pub fn watch_attached(&self) -> bool {
        self.watch.is_some()
    }

    /// Ledger phrases plus task ids that have no event yet.
    pub fn occupancy(&self) -> usize {
        let mut n = self.distinct_phrases();
        for id in &self.reserved {
            if !self.known(id) {
                n += 1;
            }
        }
        n
    }

    /// Mint `ph-<n>`, skip ids already in the log or reserved, and reserve it.
    /// Does not write an event.
    pub fn mint_and_hold(&mut self) -> Result<String, LedgerError> {
        if self.occupancy() >= PHRASE_CAP {
            return Err(LedgerError::Quota);
        }
        let id = self.allocate_mint();
        self.reserved.insert(id.clone());
        Ok(id)
    }

    pub fn holds(&self, id: &str) -> bool {
        self.known(id) || self.reserved.contains(id)
    }

    /// Count a caller-supplied id toward the cap. An id already in the log
    /// or already reserved does not take a second slot.
    pub fn hold_phrase(&mut self, id: &str) -> Result<(), LedgerError> {
        if self.known(id) || self.reserved.contains(id) {
            return Ok(());
        }
        if self.occupancy() >= PHRASE_CAP {
            return Err(LedgerError::Quota);
        }
        self.reserved.insert(id.to_string());
        Ok(())
    }

    pub fn events(&self) -> &[LedgerEvent] {
        &self.events
    }

    pub fn latest_seq(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }

    pub fn compacted_below(&self) -> u64 {
        self.compacted_below
    }

    pub fn append_observed(&mut self, by: Tier, ev: ObservedAppend, now_ms: u64) -> Result<u64, LedgerError> {
        let (phrase_id, task_id, query, doc_key, kind, viewed, matched, uv, dwell_ms) = match ev {
            ObservedAppend::Located { phrase_id, task_id, query, doc_key, matched, uv } => {
                (phrase_id, task_id, query, doc_key, LedgerKind::Located, None, Some(matched), uv, None)
            }
            ObservedAppend::Viewed { phrase_id, task_id, viewed, uv } => {
                let tier = match viewed {
                    ObservedViewed::Shown => ViewedTier::Shown,
                    ObservedViewed::Dwelled => ViewedTier::Dwelled,
                };
                (phrase_id, task_id, None, None, LedgerKind::Viewed, Some(tier), None, uv, None)
            }
            ObservedAppend::Mismatch { phrase_id, task_id, matched } => {
                (phrase_id, task_id, None, None, LedgerKind::Mismatch, None, Some(matched), None, None)
            }
            ObservedAppend::Tour { phrase_id, task_id, query, dwell_ms } => {
                (phrase_id, task_id, query, None, LedgerKind::Tour, None, None, None, Some(dwell_ms))
            }
        };
        self.push(phrase_id, task_id, query, doc_key, kind, viewed, matched, uv, dwell_ms, by, now_ms, None)
    }

    /// Only `ui_input::apply_ui_event` can build the witness this requires.
    pub fn append_user(
        &mut self,
        _witness: crate::ui_input::T0Witness,
        ev: UserAppend,
        now_ms: u64,
    ) -> Result<u64, LedgerError> {
        let (phrase_id, kind, viewed) = match ev {
            UserAppend::Acknowledged { phrase_id } => (phrase_id, LedgerKind::Viewed, Some(ViewedTier::Acknowledged)),
            UserAppend::Approved { phrase_id } => (phrase_id, LedgerKind::Approved, None),
            UserAppend::Discarded { phrase_id } => (phrase_id, LedgerKind::Discarded, None),
            UserAppend::Cleared { phrase_id } => (phrase_id, LedgerKind::Cleared, Some(ViewedTier::Acknowledged)),
            UserAppend::Skip { phrase_id } => (phrase_id, LedgerKind::Skip, None),
            UserAppend::Accept { phrase_id } => (phrase_id, LedgerKind::Accept, None),
            UserAppend::Deny { phrase_id } => (phrase_id, LedgerKind::Deny, None),
        };
        let task_id = self.latest_task_id(&phrase_id);
        self.push(phrase_id, task_id, None, None, kind, viewed, None, None, None, Tier::T0Physical, now_ms, None)
    }

    pub fn arm_note(&mut self, phrase_id: &str, now_ms: u64) -> Result<(), LedgerError> {
        let phrase_id = cut(phrase_id.to_string());
        if phrase_id.is_empty() || !self.known(&phrase_id) {
            return Err(LedgerError::Rejected("no_such_phrase"));
        }
        self.arming = Some(Arming { phrase_id, at_ms: now_ms });
        Ok(())
    }

    pub fn add_note(&mut self, body: &AddNoteBody, now_ms: u64) -> Result<Value, LedgerError> {
        let _ = body.burst;
        let via = match body.via {
            WireVia::Typed => NoteVia::Typed,
            WireVia::Voice => NoteVia::Voice,
        };
        let cleaned = note_text::clean(&body.text).map_err(LedgerError::Rejected)?;
        let phrase_id = if let Some(raw) = body.phrase_id.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
            let phrase_id = cut(raw.to_string());
            if !self.known(&phrase_id) {
                return Err(LedgerError::Rejected("no_such_phrase"));
            }
            phrase_id
        } else {
            self.take_arming(now_ms)?
        };
        let replaces = match body.replaces.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
            Some(replaces) if self.live_note(&phrase_id, replaces) => Some(replaces.to_string()),
            Some(_) => return Err(LedgerError::Rejected("no_such_note")),
            None => None,
        };
        let flags = NoteFlags {
            transcribed: via == NoteVia::Voice,
            truncated: cleaned.truncated,
            normalized: cleaned.normalized,
        };
        let (_, note_id) = self.commit_note(phrase_id, cleaned.text, via, flags, replaces, now_ms)?;
        Ok(json!({ "noteId": note_id }))
    }

    pub fn delete_note(&mut self, note_id: &str, now_ms: u64) -> Result<Value, LedgerError> {
        let note_id = note_id.trim();
        if note_id.is_empty() {
            return Err(LedgerError::Schema);
        }
        let found = self
            .events
            .iter()
            .find(|ev| ev.kind == LedgerKind::Note && ev.note.as_ref().is_some_and(|note| note.note_id == note_id))
            .cloned();
        let Some(found) = found else {
            return Err(LedgerError::Rejected("no_such_note"));
        };
        if self.events.iter().any(|ev| {
            ev.kind == LedgerKind::NoteDeleted && ev.note.as_ref().is_some_and(|note| note.note_id == note_id)
        }) {
            return Err(LedgerError::Rejected("no_such_note"));
        }
        let phrase_id = found.phrase_id.clone();
        let planted = if self.assoc.get(&phrase_id).is_none() {
            if let (Some(project_id), Some(locus_id)) = (found.project_id.clone(), found.locus_id.clone()) {
                self.remember_binding(&phrase_id, project_id, locus_id, found.ask_id.clone());
                true
            } else {
                false
            }
        } else {
            false
        };
        let carry = NoteCarry { note_id: note_id.to_string(), text: None, via: None, replaces: None, flags: None };
        let task_id = found.task_id.clone().or_else(|| self.latest_task_id(&phrase_id));
        let result = self.push(
            phrase_id.clone(),
            task_id,
            None,
            None,
            LedgerKind::NoteDeleted,
            None,
            None,
            None,
            None,
            Tier::T1User,
            now_ms,
            Some(carry),
        );
        if planted {
            self.assoc.remove(&phrase_id);
        }
        result?;
        Ok(json!({ "noteId": note_id }))
    }

    pub fn list_value(&self, body: &ListPhraseFlagsBody) -> Result<Value, LedgerError> {
        let decided = match &body.decided_by {
            None => None,
            Some(raw) => Some(parse_tier(raw).ok_or(LedgerError::Schema)?),
        };
        let limit = match body.limit {
            None => DEFAULT_LIMIT,
            Some(n) => (n as usize).min(MAX_LIMIT),
        };
        let since = body.since.unwrap_or(0);
        let behind = self.compacted_below > 0 && since < self.compacted_below;
        let mut rows = self.derived();
        rows.retain(|row| row_matches(row, body, decided) && row.seq > since);
        rows.truncate(limit);
        let row_json = rows.iter().map(Derived::wire).collect::<Vec<_>>();
        // The derived view keeps each phrase's latest seq, so `since` still
        // selects rows after compaction. The event stream does not: a `since`
        // under the floor is answered with the rows and the floor.
        if body.events && behind {
            return Ok(json!({
                "seq": self.latest_seq(),
                "rows": row_json,
                "compacted": true,
                "floor": self.compacted_below,
            }));
        }
        if body.events {
            let mut events: Vec<&LedgerEvent> = self
                .events
                .iter()
                .filter(|ev| ev.seq > since && event_visible(ev, &self.derived_index(), body, decided))
                .collect();
            events.truncate(limit);
            return Ok(json!({
                "seq": self.latest_seq(),
                "events": events.iter().map(|ev| ev.wire()).collect::<Vec<_>>(),
            }));
        }
        Ok(json!({
            "seq": self.latest_seq(),
            "rows": row_json,
        }))
    }

    pub fn poll_subscriber(&self, through: u64, max_lines: usize) -> SubPoll {
        if self.compacted_below > 0 && through < self.compacted_below {
            return SubPoll::Overflow { floor: self.compacted_below };
        }
        let pending: Vec<&LedgerEvent> = self.events.iter().filter(|ev| ev.seq > through).collect();
        if pending.len() > SUBSCRIBE_LAG {
            return SubPoll::Overflow { floor: self.latest_seq() };
        }
        let mut lines = Vec::new();
        let mut cursor = through;
        for ev in pending.into_iter().take(max_lines) {
            lines.push(json!({ "ledgerEvent": ev.wire() }).to_string());
            cursor = ev.seq;
        }
        SubPoll::Lines { lines, through: cursor }
    }

    fn push(
        &mut self,
        phrase_id: String,
        task_id: Option<String>,
        query: Option<String>,
        doc_key: Option<String>,
        kind: LedgerKind,
        viewed: Option<ViewedTier>,
        matched: Option<MatchInfo>,
        uv: Option<UvBox>,
        dwell_ms: Option<u64>,
        by: Tier,
        now_ms: u64,
        note: Option<NoteCarry>,
    ) -> Result<u64, LedgerError> {
        let phrase_id = cut(phrase_id);
        if phrase_id.trim().is_empty() {
            return Err(LedgerError::Schema);
        }
        if !self.known(&phrase_id) && !self.reserved.contains(&phrase_id) && self.occupancy() >= PHRASE_CAP {
            return Err(LedgerError::Quota);
        }
        let bind = self.assoc.get(&phrase_id).cloned();
        let (project_id, locus_id, ask_id) = match &bind {
            Some(bind) => (Some(bind.project_id.clone()), Some(bind.locus_id.clone()), bind.ask_id.clone()),
            None => (None, None, None),
        };
        let seq = self.next_seq;
        let mut matched = matched;
        if let Some(info) = matched.as_mut() {
            info.ocr_snippet = info.ocr_snippet.take().map(cut);
        }
        let event = LedgerEvent {
            seq,
            ts_ms: now_ms,
            phrase_id,
            task_id: task_id.map(cut),
            kind,
            viewed,
            matched,
            uv,
            query: query.map(cut),
            doc_key: doc_key.map(cut),
            by,
            dwell_ms,
            project_id,
            ask_id,
            locus_id,
            note,
        };
        if let Some(project_id) = &event.project_id {
            let store = self.projects.as_mut().ok_or(LedgerError::Rejected("projects_unavailable"))?;
            store.append_event(project_id, &event).map_err(|err| LedgerError::Rejected(err.as_wire()))?;
        }
        self.next_seq += 1;
        self.events.push(event);
        if self.events.len() > EVENT_CAP {
            self.compact();
        }
        self.append_watch(seq);
        Ok(seq)
    }

    fn append_watch(&mut self, seq: u64) {
        let line = {
            let Some(ev) = self.events.iter().rev().find(|ev| ev.seq == seq) else { return };
            ev.wire().to_string()
        };
        let Some(file) = self.watch.as_mut() else { return };
        if let Err(err) = writeln!(file, "{line}").and_then(|()| file.flush()) {
            eprintln!("[ledger] watch append failed: {err}");
        }
    }

    fn compact(&mut self) {
        let mut keep: HashSet<u64> = HashSet::new();
        let ids = self.phrase_ids();
        for id in &ids {
            let mine: Vec<&LedgerEvent> = self.events.iter().filter(|ev| &ev.phrase_id == id).collect();
            keep.extend(seqs_to_keep(&mine));
        }
        let mut dropped_max = 0u64;
        for ev in &self.events {
            if !keep.contains(&ev.seq) {
                dropped_max = dropped_max.max(ev.seq);
            }
        }
        if dropped_max == 0 {
            return;
        }
        self.compacted_below = self.compacted_below.max(dropped_max);
        self.events.retain(|ev| keep.contains(&ev.seq));
    }

    fn phrase_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for ev in &self.events {
            if !ids.iter().any(|id| id == &ev.phrase_id) {
                ids.push(ev.phrase_id.clone());
            }
        }
        ids
    }

    fn distinct_phrases(&self) -> usize {
        self.phrase_ids().len()
    }

    fn known(&self, id: &str) -> bool {
        self.events.iter().any(|ev| ev.phrase_id == id)
    }

    fn allocate_mint(&mut self) -> String {
        loop {
            let id = format!("ph-{}", self.next_mint);
            self.next_mint += 1;
            if !self.known(&id) && !self.reserved.contains(&id) {
                return id;
            }
        }
    }

    fn accept_phrase_id(&mut self, requested: Option<String>) -> Result<String, LedgerError> {
        let id = match requested {
            Some(raw) if !raw.trim().is_empty() => cut(raw),
            _ => {
                if self.occupancy() >= PHRASE_CAP {
                    return Err(LedgerError::Quota);
                }
                self.allocate_mint()
            }
        };
        if !self.known(&id) && !self.reserved.contains(&id) && self.occupancy() >= PHRASE_CAP {
            return Err(LedgerError::Quota);
        }
        Ok(id)
    }

    fn take_arming(&mut self, now_ms: u64) -> Result<String, LedgerError> {
        let Some(arm) = self.arming.take() else {
            return Err(LedgerError::Rejected("no_armed_note"));
        };
        if now_ms.saturating_sub(arm.at_ms) > NOTE_ARM_MS {
            return Err(LedgerError::Rejected("no_armed_note"));
        }
        Ok(arm.phrase_id)
    }

    fn live_note(&self, phrase_id: &str, note_id: &str) -> bool {
        let present = self.events.iter().any(|ev| {
            ev.phrase_id == phrase_id
                && ev.kind == LedgerKind::Note
                && ev.note.as_ref().is_some_and(|note| note.note_id == note_id)
        });
        let deleted = self.events.iter().any(|ev| {
            ev.kind == LedgerKind::NoteDeleted && ev.note.as_ref().is_some_and(|note| note.note_id == note_id)
        });
        present && !deleted
    }

    fn mint_id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}{:016x}", self.note_mint);
        self.note_mint = self.note_mint.saturating_add(1);
        id
    }

    fn commit_note(
        &mut self,
        phrase_id: String,
        text: String,
        via: NoteVia,
        flags: NoteFlags,
        replaces: Option<String>,
        now_ms: u64,
    ) -> Result<(u64, String), LedgerError> {
        if let Some(last) = self.last_note_ms.get(&phrase_id) {
            if now_ms.saturating_sub(*last) < NOTE_RATE_MS {
                return Err(LedgerError::Rejected("rate_limited"));
            }
        }
        let on_phrase = self.events.iter().filter(|ev| ev.phrase_id == phrase_id && ev.kind == LedgerKind::Note).count();
        if on_phrase >= NOTES_PER_PHRASE {
            return Err(LedgerError::Rejected("quota:notes"));
        }
        if let Some(project_id) = self.assoc.get(&phrase_id).map(|bind| bind.project_id.clone()) {
            let store = self.projects.as_ref().ok_or(LedgerError::Rejected("projects_unavailable"))?;
            if store.count_notes(&project_id) >= NOTES_PER_PROJECT {
                return Err(LedgerError::Rejected("quota:notes"));
            }
        } else {
            let session = self.events.iter().filter(|ev| ev.kind == LedgerKind::Note && ev.project_id.is_none()).count();
            if session >= NOTES_PER_SESSION {
                return Err(LedgerError::Rejected("quota:notes"));
            }
        }
        let note_id = self.mint_id("nt-");
        let carry = NoteCarry {
            note_id: note_id.clone(),
            text: Some(text),
            via: Some(via),
            replaces,
            flags: Some(flags),
        };
        let task_id = self.latest_task_id(&phrase_id);
        let seq = self.push(
            phrase_id.clone(),
            task_id,
            None,
            None,
            LedgerKind::Note,
            None,
            None,
            None,
            None,
            Tier::T1User,
            now_ms,
            Some(carry),
        )?;
        self.last_note_ms.insert(phrase_id, now_ms);
        Ok((seq, note_id))
    }

    fn derived(&self) -> Vec<Derived> {
        let mut rows = Vec::new();
        for id in self.phrase_ids() {
            let mine: Vec<&LedgerEvent> = self.events.iter().filter(|ev| ev.phrase_id == id).collect();
            rows.push(derive_phrase(&mine));
        }
        rows.sort_by_key(|row| row.seq);
        rows
    }

    fn derived_index(&self) -> Vec<Derived> {
        self.derived()
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct Derived {
    phrase_id: String,
    query: Option<String>,
    matched: Option<MatchInfo>,
    viewed: Option<ViewedTier>,
    decision: Option<&'static str>,
    decision_by: Option<Tier>,
    uv: Option<UvBox>,
    doc_key: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
    ask_id: Option<String>,
    locus_id: Option<String>,
    seq: u64,
    notes: Vec<NoteView>,
}

impl Derived {
    fn wire(&self) -> Value {
        let mut value = json!({
            "phraseId": self.phrase_id,
            "query": self.query,
            "match": self.matched.as_ref().map(MatchInfo::wire),
            "viewed": viewed_wire(self.viewed),
            "decision": self.decision,
            "decisionBy": self.decision_by.map(Tier::wire_name),
            "u": self.uv.map(|b| b.u),
            "v": self.uv.map(|b| b.v),
            "w": self.uv.map(|b| b.w),
            "h": self.uv.map(|b| b.h),
            "source": { "docKey": self.doc_key },
            "taskId": self.task_id,
            "seq": self.seq,
        });
        insert_bound(&mut value, self.project_id.as_deref(), self.ask_id.as_deref(), self.locus_id.as_deref());
        if !self.notes.is_empty() {
            value["notes"] = json!(self.notes.iter().map(note_text::note_view_json).collect::<Vec<_>>());
        }
        value
    }
}

impl LedgerEvent {
    pub(crate) fn wire(&self) -> Value {
        let mut value = json!({
            "seq": self.seq,
            "ts": self.ts_ms,
            "phraseId": self.phrase_id,
            "taskId": self.task_id,
            "kind": kind_wire(self.kind),
            "viewed": viewed_wire(self.viewed),
            "match": self.matched.as_ref().map(MatchInfo::wire),
            "uv": self.uv.map(|b| json!({"u": b.u, "v": b.v, "w": b.w, "h": b.h})),
            "query": self.query,
            "docKey": self.doc_key,
            "by": self.by.wire_name(),
        });
        if let Some(dwell) = self.dwell_ms {
            value["dwellMs"] = json!(dwell);
        }
        insert_bound(&mut value, self.project_id.as_deref(), self.ask_id.as_deref(), self.locus_id.as_deref());
        if let Some(note) = &self.note {
            value["noteId"] = json!(note.note_id);
            if let Some(text) = &note.text {
                value["text"] = json!(text);
            }
            if let Some(via) = note.via {
                value["via"] = json!(via.wire());
            }
            if let Some(replaces) = &note.replaces {
                value["replaces"] = json!(replaces);
            }
            if let Some(flags) = &note.flags {
                value["flags"] = json!({
                    "transcribed": flags.transcribed,
                    "truncated": flags.truncated,
                    "normalized": flags.normalized,
                });
            }
        }
        value
    }
}

fn insert_bound(value: &mut Value, project_id: Option<&str>, ask_id: Option<&str>, locus_id: Option<&str>) {
    if let Some(project_id) = project_id {
        value["projectId"] = json!(project_id);
    }
    if let Some(ask_id) = ask_id {
        value["askId"] = json!(ask_id);
    }
    if let Some(locus_id) = locus_id {
        value["locusId"] = json!(locus_id);
    }
}

impl MatchInfo {
    fn wire(&self) -> Value {
        json!({
            "status": match self.status {
                MatchStatus::Located => "located",
                MatchStatus::Ambiguous => "ambiguous",
                MatchStatus::NotFound => "notFound",
            },
            "conf": self.conf,
            "ocrSnippet": self.ocr_snippet,
        })
    }
}

pub fn route_note(proposal: &Proposal, tier: Tier, ledger: &mut Ledger, now_ms: u64) -> Option<ProposalResult> {
    match proposal {
        Proposal::AddNote(body) => {
            if tier != Tier::T1User {
                return Some(ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:addNote"));
            }
            let id = body.proposal_id.clone();
            Some(map_note(&id, ledger.add_note(body, now_ms)))
        }
        Proposal::DeleteNote(body) => {
            if tier != Tier::T1User {
                return Some(ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:deleteNote"));
            }
            let id = body.proposal_id.clone();
            Some(map_note(&id, ledger.delete_note(&body.note_id, now_ms)))
        }
        _ => None,
    }
}

fn map_note(id: &str, result: Result<Value, LedgerError>) -> ProposalResult {
    match result {
        Ok(detail) => ProposalResult::ok(id, Some(detail)),
        Err(LedgerError::Schema) => ProposalResult::err(id, "schema"),
        Err(LedgerError::Quota) => ProposalResult::err(id, "quota: phrase cap"),
        Err(LedgerError::Rejected(message)) => ProposalResult::err(id, message),
    }
}

pub fn route_ledger(proposal: &Proposal, tier: Tier, ledger: &mut Ledger, now_ms: u64) -> Option<ProposalResult> {
    match proposal {
        Proposal::ListPhraseFlags(body) => Some(route_list(tier, ledger, body)),
        Proposal::SubscribeLedger(body) => Some(route_subscribe(tier, ledger, body)),
        Proposal::ObservePhrase(body) => Some(route_observe(tier, ledger, body, now_ms)),
        _ => None,
    }
}

fn route_list(tier: Tier, ledger: &Ledger, body: &ListPhraseFlagsBody) -> ProposalResult {
    if !tier_reads(tier) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:listPhraseFlags");
    }
    match ledger.list_value(body) {
        Ok(detail) => ProposalResult::ok(&body.proposal_id, Some(detail)),
        Err(LedgerError::Schema) => ProposalResult::err(&body.proposal_id, "schema"),
        Err(LedgerError::Quota) => ProposalResult::err(&body.proposal_id, "quota: phrase cap"),
        Err(LedgerError::Rejected(message)) => ProposalResult::err(&body.proposal_id, message),
    }
}

fn route_subscribe(tier: Tier, ledger: &Ledger, body: &crate::protocol::SubscribeLedgerBody) -> ProposalResult {
    if !tier_reads(tier) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:subscribeLedger");
    }
    ProposalResult::ok(&body.proposal_id, Some(json!({ "seq": ledger.latest_seq() })))
}

fn route_observe(tier: Tier, ledger: &mut Ledger, body: &ObservePhraseBody, now_ms: u64) -> ProposalResult {
    if tier != Tier::T3Sidecar {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:observePhrase");
    }
    let uv = match checked_uv(body.u, body.v, body.w, body.h) {
        Ok(uv) => uv,
        Err(()) => return ProposalResult::err(&body.proposal_id, "schema"),
    };
    let bind = match (&body.project_id, &body.locus_id, &body.ask_id) {
        (None, None, None) => None,
        (Some(project_id), Some(locus_id), ask_id) => Some((project_id.clone(), locus_id.clone(), ask_id.clone())),
        _ => return ProposalResult::err(&body.proposal_id, "schema"),
    };
    if let Some((project_id, locus_id, ask_id)) = &bind {
        let wire = match ledger.projects_mut() {
            None => return ProposalResult::err(&body.proposal_id, "projects_unavailable"),
            Some(store) => store.check_binding(project_id, locus_id, ask_id.as_deref()).err().map(|err| err.as_wire()),
        };
        if let Some(wire) = wire {
            return ProposalResult::err(&body.proposal_id, wire);
        }
    }
    let phrase_id = match ledger.accept_phrase_id(body.phrase_id.clone()) {
        Ok(id) => id,
        Err(LedgerError::Quota) => return ProposalResult::err(&body.proposal_id, "quota: phrase cap"),
        Err(LedgerError::Schema) => return ProposalResult::err(&body.proposal_id, "schema"),
        Err(LedgerError::Rejected(message)) => return ProposalResult::err(&body.proposal_id, message),
    };
    let previous = if bind.is_some() { ledger.take_binding(&phrase_id) } else { None };
    if let Some((project_id, locus_id, ask_id)) = bind.clone() {
        ledger.remember_binding(&phrase_id, project_id, locus_id, ask_id);
    }
    let located = ObservedAppend::Located {
        phrase_id: phrase_id.clone(),
        task_id: body.task_id.clone(),
        query: body.query.clone(),
        doc_key: body.doc_key.clone(),
        matched: body.match_info.clone(),
        uv,
    };
    if let Err(err) = ledger.append_observed(tier, located, now_ms) {
        if bind.is_some() {
            ledger.restore_binding(&phrase_id, previous);
        }
        return ProposalResult::err(&body.proposal_id, err_wire(err));
    }
    let seq = if let Some(viewed) = body.viewed {
        match ledger.append_observed(
            tier,
            ObservedAppend::Viewed { phrase_id: phrase_id.clone(), task_id: body.task_id.clone(), viewed, uv },
            now_ms,
        ) {
            Ok(seq) => seq,
            Err(err) => return ProposalResult::err(&body.proposal_id, err_wire(err)),
        }
    } else {
        ledger.latest_seq()
    };
    ProposalResult::ok(&body.proposal_id, Some(json!({ "phraseId": phrase_id, "seq": seq })))
}

fn tier_reads(tier: Tier) -> bool {
    matches!(tier, Tier::T1User | Tier::T2Client | Tier::T3Sidecar)
}

fn err_wire(err: LedgerError) -> &'static str {
    match err {
        LedgerError::Quota => "quota: phrase cap",
        LedgerError::Schema => "schema",
        LedgerError::Rejected(message) => message,
    }
}

pub(crate) fn checked_uv(u: Option<f32>, v: Option<f32>, w: Option<f32>, h: Option<f32>) -> Result<Option<UvBox>, ()> {
    let parts = [u, v, w, h];
    let n = parts.iter().filter(|part| part.is_some()).count();
    if n == 0 {
        return Ok(None);
    }
    if n != 4 {
        return Err(());
    }
    let box_ = UvBox { u: u.unwrap(), v: v.unwrap(), w: w.unwrap(), h: h.unwrap() };
    let finite = [box_.u, box_.v, box_.w, box_.h].iter().all(|n| n.is_finite() && (0.0..=1.0).contains(n));
    if finite { Ok(Some(box_)) } else { Err(()) }
}

fn parse_tier(raw: &str) -> Option<Tier> {
    Some(match raw {
        "T0" => Tier::T0Physical,
        "T1" => Tier::T1User,
        "T2" => Tier::T2Client,
        "T3" => Tier::T3Sidecar,
        _ => return None,
    })
}

fn cut(text: String) -> String {
    text.chars().take(TEXT_CAP).collect()
}

fn is_hand(kind: LedgerKind) -> bool {
    matches!(kind, LedgerKind::Skip | LedgerKind::Accept | LedgerKind::Deny)
}

fn kind_wire(kind: LedgerKind) -> &'static str {
    match kind {
        LedgerKind::Located => "located",
        LedgerKind::Viewed => "viewed",
        LedgerKind::Approved => "approved",
        LedgerKind::Discarded => "discarded",
        LedgerKind::Cleared => "cleared",
        LedgerKind::Mismatch => "mismatch",
        LedgerKind::Tour => "tour",
        LedgerKind::Note => "note",
        LedgerKind::NoteDeleted => "noteDeleted",
        LedgerKind::Skip => "skip",
        LedgerKind::Accept => "accept",
        LedgerKind::Deny => "deny",
    }
}

fn viewed_wire(viewed: Option<ViewedTier>) -> &'static str {
    match viewed {
        None => "none",
        Some(ViewedTier::Shown) => "shown",
        Some(ViewedTier::Dwelled) => "dwelled",
        Some(ViewedTier::Acknowledged) => "acknowledged",
    }
}

fn viewed_rank(viewed: ViewedTier) -> u8 {
    match viewed {
        ViewedTier::Shown => 1,
        ViewedTier::Dwelled => 2,
        ViewedTier::Acknowledged => 3,
    }
}

fn max_viewed(left: Option<ViewedTier>, right: ViewedTier) -> Option<ViewedTier> {
    match left {
        Some(current) if viewed_rank(current) >= viewed_rank(right) => Some(current),
        _ => Some(right),
    }
}

fn derive_phrase(events: &[&LedgerEvent]) -> Derived {
    let mut row = Derived {
        phrase_id: events.first().map(|ev| ev.phrase_id.clone()).unwrap_or_default(),
        query: None,
        matched: None,
        viewed: None,
        decision: None,
        decision_by: None,
        uv: None,
        doc_key: None,
        task_id: None,
        project_id: None,
        ask_id: None,
        locus_id: None,
        seq: 0,
        notes: Vec::new(),
    };
    let mut non_ack: Option<ViewedTier> = None;
    for ev in events {
        row.seq = ev.seq;
        if ev.query.is_some() {
            row.query = ev.query.clone();
        }
        if ev.matched.is_some() {
            row.matched = ev.matched.clone();
        }
        if ev.uv.is_some() {
            row.uv = ev.uv;
        }
        if ev.doc_key.is_some() {
            row.doc_key = ev.doc_key.clone();
        }
        if ev.task_id.is_some() {
            row.task_id = ev.task_id.clone();
        }
        if ev.project_id.is_some() {
            row.project_id = ev.project_id.clone();
            row.ask_id = ev.ask_id.clone();
            row.locus_id = ev.locus_id.clone();
        }
        match ev.kind {
            LedgerKind::Viewed => match ev.viewed {
                Some(ViewedTier::Acknowledged) => row.viewed = Some(ViewedTier::Acknowledged),
                Some(tier @ (ViewedTier::Shown | ViewedTier::Dwelled)) => {
                    non_ack = max_viewed(non_ack, tier);
                    if row.viewed != Some(ViewedTier::Acknowledged) {
                        row.viewed = max_viewed(row.viewed, tier);
                    }
                }
                None => {}
            },
            LedgerKind::Cleared => {
                if ev.viewed == Some(ViewedTier::Acknowledged) {
                    row.viewed = non_ack;
                }
            }
            LedgerKind::Approved => {
                row.decision = Some("approved");
                row.decision_by = Some(ev.by);
            }
            LedgerKind::Discarded => {
                row.decision = Some("discarded");
                row.decision_by = Some(ev.by);
            }
            LedgerKind::Accept => {
                row.decision = Some("accept");
                row.decision_by = Some(ev.by);
            }
            LedgerKind::Deny => {
                row.decision = Some("deny");
                row.decision_by = Some(ev.by);
            }
            LedgerKind::Skip => {
                row.decision = None;
                row.decision_by = None;
            }
            LedgerKind::Located | LedgerKind::Mismatch | LedgerKind::Tour => {}
            LedgerKind::Note | LedgerKind::NoteDeleted => {}
        }
    }
    row.notes = notes_of(events);
    row
}

fn notes_of(events: &[&LedgerEvent]) -> Vec<NoteView> {
    let mut facts = Vec::new();
    for ev in events {
        let Some(note) = &ev.note else { continue };
        match ev.kind {
            LedgerKind::Note => {
                let Some(text) = &note.text else { continue };
                facts.push(NoteFact {
                    note_id: note.note_id.clone(),
                    text: text.clone(),
                    via: note.via.map(NoteVia::wire).unwrap_or("typed").to_string(),
                    ts_ms: ev.ts_ms,
                    replaces: note.replaces.clone(),
                    flags: note.flags.clone().unwrap_or(NoteFlags {
                        transcribed: false,
                        truncated: false,
                        normalized: false,
                    }),
                    tombstone: false,
                });
            }
            LedgerKind::NoteDeleted => facts.push(NoteFact {
                note_id: note.note_id.clone(),
                text: String::new(),
                via: String::new(),
                ts_ms: ev.ts_ms,
                replaces: None,
                flags: NoteFlags { transcribed: false, truncated: false, normalized: false },
                tombstone: true,
            }),
            _ => {}
        }
    }
    note_text::visible_notes(&facts)
}

fn seqs_to_keep(events: &[&LedgerEvent]) -> HashSet<u64> {
    let mut keep = HashSet::new();
    if let Some(last) = events.last() {
        keep.insert(last.seq);
    }
    for ev in events {
        if matches!(ev.kind, LedgerKind::Note | LedgerKind::NoteDeleted) {
            keep.insert(ev.seq);
        }
    }
    for pick in [
        |ev: &&LedgerEvent| ev.query.is_some(),
        |ev: &&LedgerEvent| ev.matched.is_some(),
        |ev: &&LedgerEvent| ev.uv.is_some(),
        |ev: &&LedgerEvent| ev.doc_key.is_some(),
        |ev: &&LedgerEvent| ev.task_id.is_some(),
    ] {
        if let Some(ev) = events.iter().rev().find(|ev| pick(ev)) {
            keep.insert(ev.seq);
        }
    }
    if let Some(ev) = events.iter().rev().find(|ev| {
        matches!(
            ev.kind,
            LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Skip | LedgerKind::Accept | LedgerKind::Deny
        )
    }) {
        keep.insert(ev.seq);
    }
    let derived = derive_phrase(events);
    let non_ack = highest_non_ack(events);
    if let Some(tier) = non_ack {
        if let Some(ev) = events.iter().rev().find(|ev| ev.kind == LedgerKind::Viewed && ev.viewed == Some(tier)) {
            keep.insert(ev.seq);
        }
    }
    if derived.viewed == Some(ViewedTier::Acknowledged) {
        if let Some(ev) = events
            .iter()
            .rev()
            .find(|ev| ev.kind == LedgerKind::Viewed && ev.viewed == Some(ViewedTier::Acknowledged))
        {
            keep.insert(ev.seq);
        }
    }
    keep
}

fn highest_non_ack(events: &[&LedgerEvent]) -> Option<ViewedTier> {
    let mut best: Option<ViewedTier> = None;
    for ev in events {
        if ev.kind == LedgerKind::Viewed {
            if let Some(tier @ (ViewedTier::Shown | ViewedTier::Dwelled)) = ev.viewed {
                best = max_viewed(best, tier);
            }
        }
    }
    best
}

fn row_matches(row: &Derived, body: &ListPhraseFlagsBody, decided: Option<Tier>) -> bool {
    if let Some(task) = &body.task_id {
        if row.task_id.as_deref() != Some(task.as_str()) {
            return false;
        }
    }
    if let Some(doc) = &body.doc_key {
        if row.doc_key.as_deref() != Some(doc.as_str()) {
            return false;
        }
    }
    if let Some(tier) = decided {
        if row.decision_by != Some(tier) {
            return false;
        }
    }
    true
}

fn event_visible(ev: &LedgerEvent, rows: &[Derived], body: &ListPhraseFlagsBody, decided: Option<Tier>) -> bool {
    rows.iter().any(|row| row.phrase_id == ev.phrase_id && row_matches(row, body, decided))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::SubscribeLedgerBody;
    use crate::ui_input::{apply_ui_event, UiEvent};
    use std::fs;
    use std::path::Path;

    fn located(id: &str, conf: f32, snippet: Option<&str>) -> ObservedAppend {
        ObservedAppend::Located {
            phrase_id: id.to_string(),
            task_id: None,
            query: Some(format!("q-{id}")),
            doc_key: Some("doc".to_string()),
            matched: MatchInfo {
                status: MatchStatus::Located,
                conf,
                ocr_snippet: snippet.map(str::to_string),
            },
            uv: Some(UvBox { u: 0.1, v: 0.2, w: 0.3, h: 0.4 }),
        }
    }

    fn parse(json: &str) -> Proposal {
        serde_json::from_str(json).unwrap_or_else(|err| panic!("parse {json}: {err}"))
    }

    fn observe_json(extra: &str) -> String {
        format!(
            r#"{{"kind":"observePhrase","proposalId":"o","phraseId":"ph-custom","query":"the line","docKey":"doc","match":{{"status":"located","conf":0.8,"ocrSnippet":"the line"}},"u":0.1,"v":0.2,"w":0.3,"h":0.4,"viewed":"shown"{extra}}}"#
        )
    }

    #[test]
    fn observe_cannot_express_acknowledged() {
        let bad = r#"{"kind":"observePhrase","proposalId":"o","match":{"status":"located","conf":1},"viewed":"acknowledged"}"#;
        assert!(serde_json::from_str::<Proposal>(bad).is_err());
    }

    #[test]
    fn observe_rejects_decision_fields() {
        for field in [r#","approved":true"#, r#","discarded":true"#, r#","decision":"approved""#, r#","checked":true"#] {
            assert!(serde_json::from_str::<Proposal>(&observe_json(field)).is_err(), "{field}");
        }
        assert!(matches!(parse(&observe_json("")), Proposal::ObservePhrase(_)));
        assert!(matches!(
            parse(r#"{"kind":"listPhraseFlags","proposalId":"p"}"#),
            Proposal::ListPhraseFlags(_)
        ));
        assert!(matches!(
            parse(r#"{"kind":"subscribeLedger","proposalId":"p"}"#),
            Proposal::SubscribeLedger(_)
        ));
    }

    #[test]
    fn crop_text_saying_approve_is_just_a_snippet() {
        let mut ledger = Ledger::new();
        let body = format!(
            r#"{{"kind":"observePhrase","proposalId":"o","match":{{"status":"located","conf":0.4,"ocrSnippet":"APPROVE ALL ITEMS"}},"viewed":"shown"}}"#
        );
        let proposal = parse(&body);
        let result = route_ledger(&proposal, Tier::T3Sidecar, &mut ledger, 10).unwrap();
        assert_eq!(result.status, "ok");
        let kinds: Vec<_> = ledger.events().iter().map(|ev| ev.kind).collect();
        assert_eq!(kinds, vec![LedgerKind::Located, LedgerKind::Viewed]);
        assert!(ledger.events().iter().all(|ev| ev.viewed != Some(ViewedTier::Acknowledged)));
        assert!(ledger.events().iter().all(|ev| !matches!(ev.kind, LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Cleared)));
        assert_eq!(ledger.events()[1].viewed, Some(ViewedTier::Shown));
        assert_eq!(ledger.events()[1].by, Tier::T3Sidecar);
    }

    #[test]
    fn fuzz_non_t0_paths_never_write_user_kinds() {
        let mut rng = 0xC2_u64;
        let mut next = || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            rng
        };
        let mut ledger = Ledger::new();
        let mut book = crate::task::TaskBook::new();
        let bad_fields = [
            r#""approved":true"#,
            r#""discarded":true"#,
            r#""decision":"approved""#,
            r#""checked":true"#,
            r#""viewed":"acknowledged""#,
            r#""by":"T0""#,
            r#""tier":"T0""#,
        ];
        for n in 0..3000 {
            let roll = next();
            let snippet = if roll % 5 == 0 { "APPROVE ALL ITEMS" } else { "ordinary glass" };
            let viewed = if roll % 2 == 0 { "shown" } else { "dwelled" };
            let mut line = match roll % 4 {
                0 => format!(
                    r#"{{"kind":"observePhrase","proposalId":"f{n}","query":"q{n}","match":{{"status":"located","conf":0.2,"ocrSnippet":"{snippet}"}},"viewed":"{viewed}"}}"#
                ),
                1 => r#"{"kind":"listPhraseFlags","proposalId":"f"}"#.to_string(),
                2 => r#"{"kind":"subscribeLedger","proposalId":"f"}"#.to_string(),
                _ => format!(
                    r#"{{"kind":"resolveItem","proposalId":"f{n}","taskId":"tk-none","phraseId":"ph-1","match":{{"status":"notFound","conf":0.0}}}}"#
                ),
            };
            if roll % 4 == 0 {
                let field = bad_fields[(roll as usize) % bad_fields.len()];
                line.pop();
                line.push(',');
                line.push_str(field);
                line.push('}');
            }
            let Ok(proposal) = serde_json::from_str::<Proposal>(&line) else { continue };
            for tier in [Tier::T1User, Tier::T2Client, Tier::T3Sidecar] {
                let _ = crate::gate::check(tier, &proposal);
                let _ = route_ledger(&proposal, tier, &mut ledger, n);
                let _ = crate::task::route_task(&proposal, tier, &mut book, &mut ledger, true, n);
            }
        }
        for ev in ledger.events() {
            assert!(!matches!(ev.kind, LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Cleared), "{ev:?}");
            assert_ne!(ev.viewed, Some(ViewedTier::Acknowledged));
        }
    }

    #[test]
    fn tour_dwell_is_on_the_wire_only_when_set() {
        let mut ledger = Ledger::new();
        ledger.append_observed(Tier::T3Sidecar, located("one", 1.0, None), 1).unwrap();
        assert!(ledger.events()[0].wire().get("dwellMs").is_none());
        assert!(ledger.events()[0].dwell_ms.is_none());
        ledger
            .append_observed(
                Tier::T2Client,
                ObservedAppend::Tour {
                    phrase_id: "one".into(),
                    task_id: Some("tk-1".into()),
                    query: Some("the line".into()),
                    dwell_ms: 0,
                },
                2,
            )
            .unwrap();
        assert_eq!(ledger.events()[1].dwell_ms, Some(0));
        assert_eq!(ledger.events()[1].wire()["dwellMs"], 0);
        assert_eq!(ledger.events()[1].kind, LedgerKind::Tour);
        assert_ne!(ledger.events()[1].by, Tier::T0Physical);
    }

    #[test]
    fn seq_strictly_increasing_gap_free() {
        let mut ledger = Ledger::new();
        for i in 0..20 {
            ledger.append_observed(Tier::T3Sidecar, located("one", i as f32, None), i).unwrap();
        }
        let seqs: Vec<_> = ledger.events().iter().map(|ev| ev.seq).collect();
        assert_eq!(seqs, (1..=20).collect::<Vec<_>>());
    }

    #[test]
    fn since_is_exclusive() {
        let mut ledger = Ledger::new();
        ledger.append_observed(Tier::T3Sidecar, located("one", 1.0, None), 1).unwrap();
        let body = ListPhraseFlagsBody {
            proposal_id: "p".into(),
            since: Some(1),
            task_id: None,
            doc_key: None,
            decided_by: None,
            limit: None,
            events: true,
            wire_kind: None,
        };
        let detail = ledger.list_value(&body).unwrap();
        assert!(detail["events"].as_array().unwrap().is_empty());
        ledger.append_observed(Tier::T3Sidecar, located("one", 2.0, None), 2).unwrap();
        let detail = ledger.list_value(&body).unwrap();
        let events = detail["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["seq"], 2);
    }

    #[test]
    fn decided_by_filter() {
        let mut ledger = Ledger::new();
        ledger.append_observed(Tier::T3Sidecar, located("one", 1.0, None), 1).unwrap();
        apply_ui_event(&mut ledger, UiEvent::Approve { phrase_id: "one".into() }, 2).unwrap();
        ledger.append_observed(Tier::T3Sidecar, located("two", 1.0, None), 3).unwrap();
        let t0 = list_decided(&ledger, "T0");
        assert_eq!(t0["rows"].as_array().unwrap().len(), 1);
        assert_eq!(t0["rows"][0]["phraseId"], "one");
        assert_eq!(t0["rows"][0]["decisionBy"], "T0");
        let t3 = list_decided(&ledger, "T3");
        assert!(t3["rows"].as_array().unwrap().is_empty());
    }

    fn list_decided(ledger: &Ledger, who: &str) -> Value {
        ledger
            .list_value(&ListPhraseFlagsBody {
                proposal_id: "p".into(),
                since: None,
                task_id: None,
                doc_key: None,
                decided_by: Some(who.into()),
                limit: None,
                events: false,
                wire_kind: None,
            })
            .unwrap()
    }

    #[test]
    fn two_clients_same_ledger() {
        let mut ledger = Ledger::new();
        let observe = parse(&observe_json(""));
        route_ledger(&observe, Tier::T3Sidecar, &mut ledger, 5).unwrap();
        let list = parse(r#"{"kind":"listPhraseFlags","proposalId":"l"}"#);
        let t2 = route_ledger(&list, Tier::T2Client, &mut ledger, 6).unwrap();
        let t3 = route_ledger(&list, Tier::T3Sidecar, &mut ledger, 6).unwrap();
        assert_eq!(t2.detail, t3.detail);
        assert_eq!(t2.detail.as_ref().unwrap()["seq"], t3.detail.as_ref().unwrap()["seq"]);
    }

    #[test]
    fn limit_default_and_cap() {
        let mut ledger = Ledger::new();
        for i in 0..250 {
            ledger.append_observed(Tier::T3Sidecar, located("one", i as f32, None), i).unwrap();
        }
        let default_rows = ledger
            .list_value(&ListPhraseFlagsBody {
                proposal_id: "p".into(),
                since: None,
                task_id: None,
                doc_key: None,
                decided_by: None,
                limit: None,
                events: true,
                wire_kind: None,
            })
            .unwrap();
        assert_eq!(default_rows["events"].as_array().unwrap().len(), 50);
        let capped = ledger
            .list_value(&ListPhraseFlagsBody {
                proposal_id: "p".into(),
                since: None,
                task_id: None,
                doc_key: None,
                decided_by: None,
                limit: Some(201),
                events: true,
                wire_kind: None,
            })
            .unwrap();
        assert_eq!(capped["events"].as_array().unwrap().len(), 200);
        assert_eq!(capped["seq"], 250);
    }

    #[test]
    fn phrase_cap_quota() {
        let mut ledger = Ledger::new();
        for i in 0..PHRASE_CAP {
            ledger.append_observed(Tier::T3Sidecar, located(&format!("id-{i}"), 1.0, None), i as u64).unwrap();
        }
        let err = ledger.append_observed(Tier::T3Sidecar, located("id-new", 1.0, None), 99).unwrap_err();
        assert_eq!(err, LedgerError::Quota);
        let proposal = parse(
            r#"{"kind":"observePhrase","proposalId":"o","phraseId":"id-extra","match":{"status":"located","conf":1}}"#,
        );
        let result = route_ledger(&proposal, Tier::T3Sidecar, &mut ledger, 100).unwrap();
        assert_eq!(result.error.as_deref(), Some("quota: phrase cap"));
        ledger.append_observed(Tier::T3Sidecar, located("id-0", 2.0, None), 101).unwrap();
    }

    #[test]
    fn compaction_preserves_view() {
        let mut ledger = Ledger::new();
        for i in 0..30 {
            ledger
                .append_observed(Tier::T3Sidecar, located("one", i as f32, Some("glass")), i)
                .unwrap();
        }
        ledger
            .append_observed(
                Tier::T3Sidecar,
                ObservedAppend::Viewed {
                    phrase_id: "one".into(),
                    task_id: None,
                    viewed: ObservedViewed::Dwelled,
                    uv: None,
                },
                40,
            )
            .unwrap();
        apply_ui_event(&mut ledger, UiEvent::Approve { phrase_id: "one".into() }, 41).unwrap();
        apply_ui_event(&mut ledger, UiEvent::Discard { phrase_id: "one".into() }, 42).unwrap();
        let before = ledger.list_value(&bare_list(false)).unwrap();
        let before_len = ledger.events().len();
        ledger.compact();
        let after = ledger.list_value(&bare_list(false)).unwrap();
        assert_eq!(before, after);
        assert!(ledger.events().len() < before_len);
        assert!(ledger.compacted_below() > 0);
        let old = ledger.list_value(&bare_list(true)).unwrap();
        assert_eq!(old["compacted"], true);
        assert!(old["floor"].as_u64().unwrap() >= 1);
        assert!(old.get("rows").is_some());

        let mut flooded = Ledger::new();
        for i in 0..=EVENT_CAP {
            flooded.append_observed(Tier::T3Sidecar, located("one", 1.0, None), i as u64).unwrap();
        }
        assert!(flooded.events().len() <= EVENT_CAP);
        assert!(flooded.compacted_below() > 0);
    }

    fn bare_list(events: bool) -> ListPhraseFlagsBody {
        ListPhraseFlagsBody {
            proposal_id: "p".into(),
            since: Some(0),
            task_id: None,
            doc_key: None,
            decided_by: None,
            limit: Some(200),
            events,
            wire_kind: None,
        }
    }

    #[test]
    fn subscribe_streams_in_order_and_resumes() {
        let mut ledger = Ledger::new();
        for i in 0..3 {
            ledger.append_observed(Tier::T3Sidecar, located("one", i as f32, None), i).unwrap();
        }
        let SubPoll::Lines { lines, through } = ledger.poll_subscriber(0, 100) else {
            panic!("expected lines");
        };
        assert_eq!(lines.len(), 3);
        let seqs: Vec<u64> = lines
            .iter()
            .map(|line| serde_json::from_str::<Value>(line).unwrap()["ledgerEvent"]["seq"].as_u64().unwrap())
            .collect();
        assert_eq!(seqs, vec![1, 2, 3]);
        assert_eq!(through, 3);
        let SubPoll::Lines { lines, .. } = ledger.poll_subscriber(through, 100) else {
            panic!("expected lines");
        };
        assert!(lines.is_empty());
        ledger.append_observed(Tier::T3Sidecar, located("one", 4.0, None), 4).unwrap();
        let SubPoll::Lines { lines, through } = ledger.poll_subscriber(3, 100) else {
            panic!("expected lines");
        };
        assert_eq!(lines.len(), 1);
        assert_eq!(through, 4);
        for i in 0..1001 {
            ledger.append_observed(Tier::T3Sidecar, located("one", i as f32, None), 10 + i).unwrap();
        }
        let behind = ledger.poll_subscriber(4, usize::MAX);
        assert!(matches!(behind, SubPoll::Overflow { .. }));
    }

    #[test]
    fn snippets_truncated() {
        let mut snippet = "x".repeat(500);
        snippet.push_str("TAIL");
        let mut ledger = Ledger::new();
        ledger
            .append_observed(Tier::T3Sidecar, located("one", 1.0, Some(&snippet)), 1)
            .unwrap();
        let stored = ledger.events()[0].matched.as_ref().unwrap().ocr_snippet.as_ref().unwrap();
        assert_eq!(stored.chars().count(), TEXT_CAP);
        assert!(!stored.contains("TAIL"));
    }

    #[test]
    fn mismatch_never_sets_decision() {
        let mut ledger = Ledger::new();
        ledger.append_observed(Tier::T3Sidecar, located("one", 1.0, None), 1).unwrap();
        apply_ui_event(&mut ledger, UiEvent::Approve { phrase_id: "one".into() }, 2).unwrap();
        ledger
            .append_observed(
                Tier::T3Sidecar,
                ObservedAppend::Mismatch {
                    phrase_id: "one".into(),
                    task_id: None,
                    matched: MatchInfo { status: MatchStatus::NotFound, conf: 0.1, ocr_snippet: Some("nope".into()) },
                },
                3,
            )
            .unwrap();
        let detail = ledger.list_value(&bare_list(false)).unwrap();
        assert_eq!(detail["rows"][0]["decision"], "approved");
        assert_eq!(detail["rows"][0]["decisionBy"], "T0");
        assert_eq!(detail["rows"][0]["match"]["status"], "notFound");
    }

    #[test]
    fn uv_all_or_none_and_bounded() {
        let mut ledger = Ledger::new();
        let partial = parse(
            r#"{"kind":"observePhrase","proposalId":"o","match":{"status":"located","conf":1},"u":0.2}"#,
        );
        let result = route_ledger(&partial, Tier::T3Sidecar, &mut ledger, 1).unwrap();
        assert_eq!(result.error.as_deref(), Some("schema"));
        assert!(ledger.events().is_empty());
        let high = parse(
            r#"{"kind":"observePhrase","proposalId":"o","match":{"status":"located","conf":1},"u":0.1,"v":1.5,"w":0.1,"h":0.1}"#,
        );
        let result = route_ledger(&high, Tier::T3Sidecar, &mut ledger, 1).unwrap();
        assert_eq!(result.error.as_deref(), Some("schema"));
        let ok = parse(
            r#"{"kind":"observePhrase","proposalId":"o","match":{"status":"ambiguous","conf":0.3},"u":0,"v":0,"w":1,"h":1,"viewed":"dwelled"}"#,
        );
        let result = route_ledger(&ok, Tier::T3Sidecar, &mut ledger, 2).unwrap();
        assert_eq!(result.status, "ok");
        assert_eq!(result.detail.as_ref().unwrap()["phraseId"], "ph-1");
    }

    #[test]
    fn supplied_phrase_id_is_kept() {
        let mut ledger = Ledger::new();
        let proposal = parse(&observe_json(""));
        let result = route_ledger(&proposal, Tier::T3Sidecar, &mut ledger, 1).unwrap();
        assert_eq!(result.detail.as_ref().unwrap()["phraseId"], "ph-custom");
    }

    #[test]
    fn subscribe_reply_carries_latest_seq() {
        let mut ledger = Ledger::new();
        ledger.append_observed(Tier::T3Sidecar, located("one", 1.0, None), 1).unwrap();
        let proposal = parse(r#"{"kind":"subscribeLedger","proposalId":"s","since":0}"#);
        let result = route_ledger(&proposal, Tier::T1User, &mut ledger, 2).unwrap();
        assert_eq!(result.detail.as_ref().unwrap()["seq"], 1);
        let _: SubscribeLedgerBody = match proposal {
            Proposal::SubscribeLedger(body) => body,
            _ => unreachable!(),
        };
    }

    #[test]
    fn export_ledger_is_not_a_proposal() {
        assert!(serde_json::from_str::<Proposal>(r#"{"kind":"exportLedger"}"#).is_err());
        assert!(serde_json::from_str::<Proposal>(r#"{"kind":"clearLedger"}"#).is_err());
    }

    #[test]
    fn relative_watch_path_is_refused() {
        let name = "presence-ledger-relative-refused.jsonl";
        let _ = fs::remove_file(name);
        let mut ledger = Ledger::new();
        ledger.attach_watch(name);
        assert!(!ledger.watch_attached());
        assert!(!Path::new(name).exists());
    }

    #[test]
    fn unopenable_watch_leaves_the_event() {
        let dir = std::env::temp_dir().join(format!("presence-ledger-j6-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("not-a-directory");
        fs::write(&blocker, b"x").unwrap();
        let mut ledger = Ledger::new();
        ledger.attach_watch(blocker.join("ledger.jsonl").to_str().unwrap());
        assert!(!ledger.watch_attached());
        ledger.append_observed(Tier::T3Sidecar, located("ph-1", 0.5, None), 1).unwrap();
        assert_eq!(ledger.events().len(), 1);
        assert_eq!(ledger.events()[0].phrase_id, "ph-1");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dismiss_does_not_truncate_the_watch_file() {
        use crate::registry::Registry;
        let dir = std::env::temp_dir().join(format!("presence-ledger-j7-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let watch = dir.join("nested").join("ledger.jsonl");
        let mut reg = Registry::new();
        reg.ledger_mut().attach_watch(watch.to_str().unwrap());
        assert!(reg.ledger().watch_attached());
        let parent_mode = fs::metadata(watch.parent().unwrap()).unwrap().permissions().mode() & 0o777;
        let file_mode = fs::metadata(&watch).unwrap().permissions().mode() & 0o777;
        assert_eq!(parent_mode, 0o700);
        assert_eq!(file_mode, 0o600);
        reg.spawn_presence("p-1", "ctx", None, None);
        reg.ledger_mut().append_observed(Tier::T3Sidecar, located("ph-1", 0.9, None), 10).unwrap();
        let before = fs::read(&watch).unwrap();
        let line: serde_json::Value = serde_json::from_str(std::str::from_utf8(&before).unwrap().trim()).unwrap();
        assert_eq!(line["phraseId"], "ph-1");
        assert_eq!(line["seq"], 1);
        reg.retire_presence("p-1").unwrap();
        let after = fs::read(&watch).unwrap();
        assert_eq!(before, after);
        assert!(watch.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    fn seed(ledger: &mut Ledger, id: &str) {
        ledger.append_observed(Tier::T3Sidecar, located(id, 1.0, None), 1).unwrap();
    }

    fn add_at(ledger: &mut Ledger, text: &str, phrase: Option<&str>, replaces: Option<&str>, now: u64) -> ProposalResult {
        let text = serde_json::to_string(text).unwrap();
        let phrase = phrase.map(|id| format!(r#","phraseId":"{id}""#)).unwrap_or_default();
        let replaces = replaces.map(|id| format!(r#","replaces":"{id}""#)).unwrap_or_default();
        let line = format!(r#"{{"kind":"addNote","proposalId":"n","text":{text},"via":"typed"{phrase}{replaces}}}"#);
        route_note(&serde_json::from_str(&line).unwrap(), Tier::T1User, ledger, now).unwrap()
    }

    fn note_row(ledger: &Ledger, phrase: &str) -> Value {
        let detail = ledger.list_value(&bare_list(false)).unwrap();
        detail["rows"].as_array().unwrap().iter().find(|row| row["phraseId"] == phrase).cloned().unwrap_or(Value::Null)
    }

    fn prod_fn(name: &str) -> String {
        let src = include_str!("ledger.rs");
        let prod = src.split("mod tests").next().unwrap();
        let marker = format!("fn {name}");
        let start = prod.find(&marker).unwrap_or_else(|| panic!("missing {name}"));
        let brace = prod[start..].find('{').unwrap() + start;
        let mut depth = 0i32;
        for (offset, byte) in prod.as_bytes().iter().enumerate().skip(brace) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return prod[start..=offset].to_string();
                    }
                }
                _ => {}
            }
        }
        panic!("unclosed {name}");
    }

    #[test]
    fn o1_add_note_is_t1_only() {
        use crate::gate::check;
        let proposal: Proposal = serde_json::from_str(r#"{"kind":"addNote","proposalId":"n","text":"cut","via":"typed","phraseId":"ph-1"}"#).unwrap();
        for tier in [Tier::T0Physical, Tier::T2Client, Tier::T3Sidecar] {
            let err = check(tier, &proposal).unwrap_err();
            assert_eq!(err.error.as_deref(), Some("not_allowed_for_tier:addNote"));
        }
        assert!(check(Tier::T1User, &proposal).is_ok());
        let mut ledger = Ledger::new();
        seed(&mut ledger, "ph-1");
        let denied = route_note(&proposal, Tier::T2Client, &mut ledger, 5).unwrap();
        assert_eq!(denied.error.as_deref(), Some("not_allowed_for_tier:addNote"));
        assert!(ledger.events().iter().all(|ev| ev.kind != LedgerKind::Note));
    }

    #[test]
    fn o2_o4_o5_arming_is_one_phrase_for_sixty_seconds() {
        let mut ledger = Ledger::new();
        seed(&mut ledger, "ph-1");
        let missing = add_at(&mut ledger, "hello", None, None, 10);
        assert_eq!(missing.error.as_deref(), Some("no_armed_note"));
        assert!(ledger.events().iter().all(|ev| ev.kind != LedgerKind::Note));
        ledger.arm_note("ph-1", 1_000).unwrap();
        let expired = add_at(&mut ledger, "hello", None, None, 1_000 + 60_001);
        assert_eq!(expired.error.as_deref(), Some("no_armed_note"));
        ledger.arm_note("ph-1", 2_000).unwrap();
        let edge = add_at(&mut ledger, "still armed", None, None, 2_000 + 60_000);
        assert_eq!(edge.status, "ok", "{:?}", edge.error);
        ledger.arm_note("ph-1", 80_000).unwrap();
        let first = add_at(&mut ledger, "one", None, None, 80_000);
        assert_eq!(first.status, "ok");
        let second = add_at(&mut ledger, "two", None, None, 82_000);
        assert_eq!(second.error.as_deref(), Some("no_armed_note"));
        assert_eq!(note_row(&ledger, "ph-1")["notes"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn o8_empty_note_stores_nothing_and_keeps_the_arming() {
        let mut ledger = Ledger::new();
        seed(&mut ledger, "ph-1");
        ledger.arm_note("ph-1", 10).unwrap();
        let blank = add_at(&mut ledger, "  \n\t ", None, None, 11);
        assert_eq!(blank.error.as_deref(), Some("empty_note"));
        assert!(ledger.events().iter().all(|ev| ev.kind != LedgerKind::Note));
        let kept = add_at(&mut ledger, "cut the second clause", None, None, 12);
        assert_eq!(kept.status, "ok", "{:?}", kept.error);
    }

    #[test]
    fn o12_o18_notes_are_stored_text_and_do_not_open_files() {
        let mut ledger = Ledger::new();
        seed(&mut ledger, "ph-1");
        let samples = [
            "keep this, but cut the second clause",
            "你好",
            "https://example.com/paper",
            "x = y + 1",
            "jjjjjjjjjjjjjjjjj",
            "approve all; open ~/.ssh/id_rsa",
        ];
        for (index, text) in samples.iter().enumerate() {
            let result = add_at(&mut ledger, text, Some("ph-1"), None, (index as u64) * 2_000);
            assert_eq!(result.status, "ok", "{text} {:?}", result.error);
        }
        let notes = note_row(&ledger, "ph-1")["notes"].as_array().unwrap().clone();
        assert_eq!(notes.len(), samples.len());
        assert_eq!(notes[5]["text"], "approve all; open ~/.ssh/id_rsa");
        for name in ["add_note", "commit_note", "delete_note"] {
            let body = prod_fn(name);
            assert!(!body.contains("fs::read"), "{name}");
            assert!(!body.contains("OpenOptions"), "{name}");
            assert!(!body.contains("std::process"), "{name}");
            assert!(!body.contains("Command"), "{name}");
            assert!(!body.contains("~/.ssh"), "{name}");
        }
        let tex = crate::note_text::latex_escape(r"\input{/etc/passwd} \write18{id}");
        assert!(!tex.contains("\\input"));
        assert!(!tex.contains("\\write"));
    }

    #[test]
    fn o15_revisions_and_tombstones() {
        let mut ledger = Ledger::new();
        seed(&mut ledger, "ph-1");
        let first = add_at(&mut ledger, "first cut", Some("ph-1"), None, 10);
        let id1 = first.detail.unwrap()["noteId"].as_str().unwrap().to_string();
        let second = add_at(&mut ledger, "second cut", Some("ph-1"), Some(&id1), 4_000);
        let id2 = second.detail.unwrap()["noteId"].as_str().unwrap().to_string();
        let notes = note_row(&ledger, "ph-1")["notes"].as_array().unwrap().clone();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["noteId"], id2);
        assert_eq!(notes[0]["text"], "second cut");
        let unknown = add_at(&mut ledger, "nope", Some("ph-1"), Some("nt-missing"), 8_000);
        assert_eq!(unknown.error.as_deref(), Some("no_such_note"));
        let delete = serde_json::from_str::<Proposal>(&format!(r#"{{"kind":"deleteNote","proposalId":"d","noteId":"{id2}"}}"#)).unwrap();
        let gone = route_note(&delete, Tier::T1User, &mut ledger, 10_000).unwrap();
        assert_eq!(gone.status, "ok", "{:?}", gone.error);
        assert!(note_row(&ledger, "ph-1").get("notes").is_none());
        let again = route_note(&delete, Tier::T1User, &mut ledger, 12_000).unwrap();
        assert_eq!(again.error.as_deref(), Some("no_such_note"));
        let kinds: Vec<_> = ledger.events().iter().filter(|ev| matches!(ev.kind, LedgerKind::Note | LedgerKind::NoteDeleted)).map(|ev| ev.kind).collect();
        assert_eq!(kinds.len(), 3);
        assert!(kinds.contains(&LedgerKind::NoteDeleted));
    }

    #[test]
    fn o19_rate_and_quotas() {
        let mut ledger = Ledger::new();
        seed(&mut ledger, "ph-1");
        assert_eq!(add_at(&mut ledger, "a", Some("ph-1"), None, 0).status, "ok");
        assert_eq!(add_at(&mut ledger, "b", Some("ph-1"), None, 1_999).error.as_deref(), Some("rate_limited"));
        assert_eq!(add_at(&mut ledger, "c", Some("ph-1"), None, 2_000).status, "ok");
        for n in 2..20 {
            let result = add_at(&mut ledger, &format!("n{n}"), Some("ph-1"), None, (n as u64) * 2_000);
            assert_eq!(result.status, "ok", "{n} {:?}", result.error);
        }
        let over = add_at(&mut ledger, "too many", Some("ph-1"), None, 20 * 2_000);
        assert_eq!(over.error.as_deref(), Some("quota:notes"));
        assert_eq!(note_row(&ledger, "ph-1")["notes"].as_array().unwrap().len(), 20);

        let mut session = Ledger::new();
        for phrase in 0..25 {
            let id = format!("ph-{phrase}");
            seed(&mut session, &id);
            for note in 0..20 {
                let now = (phrase as u64) * 50_000 + (note as u64) * 2_000;
                let result = add_at(&mut session, "s", Some(&id), None, now);
                assert_eq!(result.status, "ok", "{phrase}:{note} {:?}", result.error);
            }
        }
        seed(&mut session, "ph-extra");
        let denied = add_at(&mut session, "s", Some("ph-extra"), None, 25 * 50_000);
        assert_eq!(denied.error.as_deref(), Some("quota:notes"));
        assert!(note_row(&session, "ph-extra").get("notes").is_none());
    }
}
