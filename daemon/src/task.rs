//! Task book for one daemon run. In memory, empty at start, not kept across
//! a restart. Controls sit beside the word box. The word mark stays click-through.
//!
//! Clicks and hotkeys enter the ledger only through [`crate::ui_input::apply_ui_event`].

use crate::gate::Tier;
use crate::ledger::{Ledger, LedgerError, ObservedAppend, PHRASE_CAP};
use crate::protocol::{
    ClientControls, CreateTaskBody, ListPhraseFlagsBody, MatchInfo, MatchStatus, ObservedViewed, Proposal, ProposalResult,
    ResolveItemBody, TaskItemBody, TaskMode, TaskPace, TourSignalBody, TourSignalKind,
};
use crate::ui_input::{apply_ui_event, ClickTarget, Hotkey, UiEvent};
use serde_json::{json, Value};

pub const QUEUE_CAP: usize = 5;
pub const ITEM_CAP: usize = 12;
pub const DEFAULT_TTL_S: u32 = 120;
pub const DEFAULT_ITEM_TIMEOUT_S: u32 = 20;
pub const DWELL_MS: u64 = 1200;
pub const GRACE_MS: u64 = 1500;
pub const DOUBLE_CLICK_MS: u64 = 250;
pub const MOVE_WAIT_MS: u64 = 1500;
pub const MOVE_ATTEMPTS: u8 = 3;
pub const NUDGE_PX: i32 = 80;
pub const CONTROL_PX: i32 = 28;
pub const CONTROL_GAP: i32 = 8;
pub const PAIR_GAP: i32 = 4;
const PROMPT_CAP: usize = 120;
const NARRATION_CAP: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Draft,
    Pending,
    AwaitingStart,
    Active,
    Paused,
    Done,
    Dismissed,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    User,
    Agent,
    SidecarDraft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawnControls {
    None,
    Check,
    YesNo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemMatch {
    Pending,
    Located,
    Ambiguous,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug)]
pub enum TaskError {
    Schema,
    QueueFull,
    Quota,
    NoSuchTask,
    NoSuchPresence,
    SelectRequiresControls,
    ApproveTakesNoControls,
    TaskNotActive,
}

struct Item {
    phrase_id: String,
    query: Option<String>,
    narration: String,
    scroll: bool,
    page: Option<u32>,
    matched: ItemMatch,
    attempts: u8,
}

struct Task {
    id: String,
    presence_id: String,
    origin: Origin,
    mode: TaskMode,
    pace: TaskPace,
    drawn: DrawnControls,
    state: TaskState,
    prompt: String,
    ttl_seconds: u32,
    item_timeout_seconds: u32,
    deadline_ms: u64,
    doc_key: Option<String>,
    items: Vec<Item>,
    index: usize,
    entered_ms: Option<u64>,
    narration_done: bool,
    last_move_ms: Option<u64>,
}

/// The only moves a tour may ask the locator to perform. This enum is not
/// deserialized from a socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerMove {
    WebFind { doc_key: String, opening_phrase: String },
    PdfPage { doc_key: String, page: u32 },
    Nudge { dy_px: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    NoActiveTour,
    DocMismatch,
    OffGlass,
    TooSoon,
    AttemptsExhausted,
    NeedsStartConfirm,
}

struct Surface {
    phrase_id: String,
    task_id: String,
    target: ClickTarget,
    rect: PxRect,
    z: u64,
    /// `None` while this is the current control. `Some` is the grace deadline.
    until_ms: Option<u64>,
}

struct ShownMark {
    phrase_id: String,
    at_ms: u64,
    dwelled: bool,
}

pub struct TaskBook {
    tasks: Vec<Task>,
    next_task: u64,
    shown: Option<ShownMark>,
    last_click: Option<(u64, String, ClickTarget)>,
    surfaces: Vec<Surface>,
    next_z: u64,
    utterance_stops: u64,
    stop_pending: bool,
    moves_issued: u64,
}

impl TaskBook {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_task: 1,
            shown: None,
            last_click: None,
            surfaces: Vec::new(),
            next_z: 1,
            utterance_stops: 0,
            stop_pending: false,
            moves_issued: 0,
        }
    }

    pub fn utterance_stops(&self) -> u64 {
        self.utterance_stops
    }

    pub fn moves_issued(&self) -> u64 {
        self.moves_issued
    }

    /// The move the active item may take. Does not issue it.
    pub fn planned_move(&self, task_id: &str) -> Result<ViewerMove, MoveError> {
        let task = self.task(task_id).ok_or(MoveError::NoActiveTour)?;
        let item = current_item(task)?;
        Ok(planned_for(task, item))
    }

    /// Permit `mv` for the active item. A rejected move is not issued.
    pub fn authorize(
        &mut self,
        task_id: &str,
        mv: &ViewerMove,
        on_glass: bool,
        ledger: &mut Ledger,
        now_ms: u64,
    ) -> Result<(), MoveError> {
        let pos = self.tasks.iter().position(|task| task.id == task_id).ok_or(MoveError::NoActiveTour)?;
        let item_index = {
            let task = &self.tasks[pos];
            let item = current_item(task)?;
            if !move_fits(task, item, mv, on_glass)? {
                return Err(MoveError::DocMismatch);
            }
            task.index
        };
        if let Some(last) = self.tasks[pos].last_move_ms {
            if now_ms < last.saturating_add(MOVE_WAIT_MS) {
                return Err(MoveError::TooSoon);
            }
        }
        if self.tasks[pos].items[item_index].attempts >= MOVE_ATTEMPTS {
            let phrase = self.tasks[pos].items[item_index].phrase_id.clone();
            self.record_miss(task_id, &phrase, ledger, now_ms);
            return Err(MoveError::AttemptsExhausted);
        }
        self.tasks[pos].items[item_index].attempts += 1;
        self.tasks[pos].last_move_ms = Some(now_ms);
        self.moves_issued += 1;
        Ok(())
    }

    fn confirm_start(&mut self) -> bool {
        if let Some(pos) = self.tasks.iter().position(|task| task.state == TaskState::AwaitingStart) {
            self.tasks[pos].state = TaskState::Pending;
        } else if let Some(pos) = self.tasks.iter().position(|task| task.state == TaskState::Draft) {
            if self.runnable_count() < QUEUE_CAP {
                self.tasks[pos].state = TaskState::Pending;
            }
        }
        self.admit().is_some()
    }

    fn record_miss(&mut self, task_id: &str, phrase_id: &str, ledger: &mut Ledger, now_ms: u64) {
        let already = self.task(task_id).and_then(|task| {
            task.items.iter().find(|item| item.phrase_id == phrase_id).map(|item| item.matched)
        });
        if already == Some(ItemMatch::NotFound) {
            return;
        }
        let (query, doc_key) = self
            .task(task_id)
            .map(|task| {
                let query = task.items.iter().find(|item| item.phrase_id == phrase_id).and_then(|item| item.query.clone());
                (query, task.doc_key.clone())
            })
            .unwrap_or((None, None));
        let _ = ledger.append_observed(
            Tier::T2Client,
            ObservedAppend::Located {
                phrase_id: phrase_id.to_string(),
                task_id: Some(task_id.to_string()),
                query,
                doc_key,
                matched: MatchInfo { status: MatchStatus::NotFound, conf: 0.0, ocr_snippet: None },
                uv: None,
            },
            now_ms,
        );
        let _ = self.note_match(task_id, phrase_id, ItemMatch::NotFound);
        self.consider(ledger, now_ms);
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn runnable_count(&self) -> usize {
        self.tasks.iter().filter(|task| runnable(task.state)).count()
    }

    /// Promote the oldest pending task. Refuses while one is active or paused.
    pub fn admit(&mut self) -> Option<String> {
        if self.tasks.iter().any(|task| matches!(task.state, TaskState::Active | TaskState::Paused)) {
            return None;
        }
        let pos = self.tasks.iter().position(|task| task.state == TaskState::Pending)?;
        self.tasks[pos].state = TaskState::Active;
        Some(self.tasks[pos].id.clone())
    }

    /// Test and runner hook. The last item of an active task has been left.
    pub fn complete(&mut self, task_id: &str) -> Result<(), TaskError> {
        let task = self.task_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if !matches!(task.state, TaskState::Active | TaskState::Paused) {
            return Err(TaskError::TaskNotActive);
        }
        task.state = TaskState::Done;
        self.clear_shown_for(task_id);
        Ok(())
    }

    /// Presentation only. Writes no ledger event.
    pub fn set_presentation(
        &mut self,
        task_id: &str,
        mode: TaskMode,
        pace: TaskPace,
        controls: Option<ClientControls>,
    ) -> Result<(), TaskError> {
        let drawn = resolve_presentation(mode, controls)?;
        let task = self.task_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        task.mode = mode;
        task.pace = pace;
        task.drawn = drawn;
        Ok(())
    }

    /// Locate status for one item. `notFound` and `ambiguous` draw no control.
    pub fn note_match(&mut self, task_id: &str, phrase_id: &str, matched: ItemMatch) -> Result<(), TaskError> {
        let task = self.task_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        let item = task.items.iter_mut().find(|item| item.phrase_id == phrase_id).ok_or(TaskError::Schema)?;
        item.matched = matched;
        Ok(())
    }

    pub fn dismiss_presence(&mut self, presence_id: &str) {
        for task in &mut self.tasks {
            if task.presence_id == presence_id && open_for_end(task.state) {
                task.state = TaskState::Dismissed;
            }
        }
        self.shown = None;
    }

    pub fn handle_ui(&mut self, ledger: &mut Ledger, ev: UiEvent, now_ms: u64) -> bool {
        match ev {
            UiEvent::Click { phrase_id, target, ts_ms } => self.on_click(ledger, &phrase_id, target, ts_ms, now_ms),
            UiEvent::Hotkey(Hotkey::Ack) => self.hotkey_ack(ledger, now_ms),
            UiEvent::Hotkey(Hotkey::Stop) => self.stop_active(),
            UiEvent::Hotkey(Hotkey::Skip) => self.skip(ledger, now_ms),
            UiEvent::Hotkey(Hotkey::Back) => self.back(ledger, now_ms),
            UiEvent::Hotkey(Hotkey::Pause) => self.pause_active(),
            UiEvent::Hotkey(Hotkey::Resume) => self.resume_active(ledger, now_ms),
            UiEvent::Hotkey(Hotkey::Start) => self.confirm_start(),
            UiEvent::Acknowledge { .. } | UiEvent::Uncheck { .. } | UiEvent::Approve { .. } | UiEvent::Discard { .. } => false,
        }
    }

    pub fn note_highlight(&mut self, presence_id: &str, ledger: &mut Ledger, now_ms: u64) {
        let phrase = self
            .tasks
            .iter()
            .find(|task| task.presence_id == presence_id && live(task.state))
            .and_then(|task| task.items.get(task.index).map(|item| item.phrase_id.clone()));
        if let Some(phrase) = phrase {
            self.note_shown(ledger, &phrase, now_ms);
        }
    }

    pub fn note_hidden_presence(&mut self, presence_id: &str) {
        let phrase = self
            .tasks
            .iter()
            .find(|task| task.presence_id == presence_id && live(task.state))
            .and_then(|task| task.items.get(task.index).map(|item| item.phrase_id.clone()));
        if let Some(phrase) = phrase {
            self.note_hidden(&phrase);
        }
    }

    pub fn note_shown(&mut self, ledger: &mut Ledger, phrase_id: &str, now_ms: u64) {
        if !self.phrase_is_live(phrase_id) {
            return;
        }
        if viewed_name(ledger, phrase_id) == "acknowledged" {
            return;
        }
        if self.shown.as_ref().is_some_and(|mark| mark.phrase_id == phrase_id) {
            return;
        }
        self.shown = None;
        let Some(task_id) = self.task_id_for_live_phrase(phrase_id) else { return };
        if ledger
            .append_observed(
                Tier::T2Client,
                ObservedAppend::Viewed {
                    phrase_id: phrase_id.to_string(),
                    task_id: Some(task_id),
                    viewed: ObservedViewed::Shown,
                    uv: None,
                },
                now_ms,
            )
            .is_err()
        {
            return;
        }
        self.shown = Some(ShownMark { phrase_id: phrase_id.to_string(), at_ms: now_ms, dwelled: false });
    }

    pub fn note_hidden(&mut self, phrase_id: &str) {
        if self.shown.as_ref().is_some_and(|mark| mark.phrase_id == phrase_id) {
            self.shown = None;
        }
    }

    pub fn tick(&mut self, ledger: &mut Ledger, now_ms: u64) {
        let due = self.shown.as_ref().and_then(|mark| {
            if !mark.dwelled && now_ms.saturating_sub(mark.at_ms) >= DWELL_MS {
                Some((mark.phrase_id.clone(), mark.at_ms))
            } else {
                None
            }
        });
        if let Some((phrase_id, at_ms)) = due {
            if viewed_name(ledger, &phrase_id) != "acknowledged" {
                if let Some(task_id) = self.task_id_for_live_phrase(&phrase_id) {
                    let _ = ledger.append_observed(
                        Tier::T2Client,
                        ObservedAppend::Viewed {
                            phrase_id: phrase_id.clone(),
                            task_id: Some(task_id),
                            viewed: ObservedViewed::Dwelled,
                            uv: None,
                        },
                        now_ms,
                    );
                    if let Some(shown) = &mut self.shown {
                        if shown.phrase_id == phrase_id && shown.at_ms == at_ms {
                            shown.dwelled = true;
                        }
                    }
                }
            }
        }
        let mut ended = Vec::new();
        for task in &mut self.tasks {
            if open_for_end(task.state) && now_ms >= task.deadline_ms {
                task.state = TaskState::Expired;
                ended.push(task.id.clone());
            }
        }
        for id in ended {
            self.clear_shown_for(&id);
        }
        self.consider(ledger, now_ms);
    }

    pub fn control_rects(&self, task_id: &str, word: PxRect, out_w: i32, out_h: i32) -> Vec<(ClickTarget, PxRect)> {
        let Some(task) = self.task(task_id) else { return Vec::new() };
        if !live(task.state) {
            return Vec::new();
        }
        let Some(item) = task.items.get(task.index) else { return Vec::new() };
        if hides_control(item.matched) {
            return Vec::new();
        }
        place_controls(word, out_w, out_h, task.drawn)
    }

    pub fn checkbox_checked(&self, ledger: &Ledger, phrase_id: &str) -> bool {
        viewed_name(ledger, phrase_id) == "acknowledged"
    }

    /// Enter the current item of an active task. A second call writes nothing.
    pub fn begin_task(&mut self, task_id: &str, ledger: &mut Ledger, now_ms: u64) -> Result<(), TaskError> {
        let pos = self.tasks.iter().position(|task| task.id == task_id).ok_or(TaskError::NoSuchTask)?;
        if self.tasks[pos].state != TaskState::Active {
            return Err(TaskError::TaskNotActive);
        }
        if self.tasks[pos].entered_ms.is_some() {
            return Ok(());
        }
        self.enter_at(pos, ledger, now_ms);
        Ok(())
    }

    pub fn show_controls(&mut self, task_id: &str, word: PxRect, out_w: i32, out_h: i32, now_ms: u64) {
        self.drop_expired(now_ms);
        self.surfaces.retain(|surface| !(surface.task_id == task_id && surface.until_ms.is_none()));
        let Some(task) = self.task(task_id) else { return };
        if !live(task.state) {
            return;
        }
        let Some(item) = task.items.get(task.index) else { return };
        if hides_control(item.matched) || task.drawn == DrawnControls::None {
            return;
        }
        let phrase_id = item.phrase_id.clone();
        let drawn = task.drawn;
        for (target, rect) in place_controls(word, out_w, out_h, drawn) {
            let z = self.next_z;
            self.next_z += 1;
            self.surfaces.push(Surface {
                phrase_id: phrase_id.clone(),
                task_id: task_id.to_string(),
                target,
                rect,
                z,
                until_ms: None,
            });
        }
        // The previous control stays above the one just placed for the grace window.
        let raised: Vec<usize> = self
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, surface)| surface.task_id == task_id && surface.until_ms.is_some())
            .map(|(index, _)| index)
            .collect();
        for index in raised {
            self.surfaces[index].z = self.next_z;
            self.next_z += 1;
        }
    }

    /// Phrase id of the topmost live control under the pointer.
    pub fn hit(&mut self, x: i32, y: i32, now_ms: u64) -> Option<(String, ClickTarget)> {
        self.drop_expired(now_ms);
        let mut best: Option<usize> = None;
        for index in 0..self.surfaces.len() {
            let (alive, rect, z) = {
                let surface = &self.surfaces[index];
                (surface_alive(surface, now_ms), surface.rect, surface.z)
            };
            if !alive || x < rect.x || y < rect.y || x >= rect.x + rect.w || y >= rect.y + rect.h {
                continue;
            }
            if best.is_none_or(|prev| z >= self.surfaces[prev].z) {
                best = Some(index);
            }
        }
        best.map(|index| (self.surfaces[index].phrase_id.clone(), self.surfaces[index].target))
    }

    fn on_click(&mut self, ledger: &mut Ledger, phrase_id: &str, target: ClickTarget, ts_ms: u64, now_ms: u64) -> bool {
        if self.last_click.as_ref().is_some_and(|prev| {
            prev.1 == phrase_id && prev.2 == target && ts_ms.saturating_sub(prev.0) < DOUBLE_CLICK_MS
        }) {
            return false;
        }
        let Some(_task_id) = self.accepts_click(phrase_id, target, now_ms) else { return false };
        let event = match target {
            ClickTarget::Check if viewed_name(ledger, phrase_id) == "acknowledged" => {
                UiEvent::Uncheck { phrase_id: phrase_id.to_string() }
            }
            ClickTarget::Check => UiEvent::Acknowledge { phrase_id: phrase_id.to_string() },
            ClickTarget::Yes => UiEvent::Approve { phrase_id: phrase_id.to_string() },
            ClickTarget::No => UiEvent::Discard { phrase_id: phrase_id.to_string() },
        };
        if apply_ui_event(ledger, event, now_ms).is_err() {
            return false;
        }
        self.last_click = Some((ts_ms, phrase_id.to_string(), target));
        self.consider(ledger, now_ms);
        true
    }

    fn accepts_click(&mut self, phrase_id: &str, target: ClickTarget, now_ms: u64) -> Option<String> {
        self.drop_expired(now_ms);
        if let Some(surface) = self.surfaces.iter().rev().find(|surface| {
            surface_alive(surface, now_ms) && surface.phrase_id == phrase_id && surface.target == target
        }) {
            return Some(surface.task_id.clone());
        }
        if self.surfaces.iter().any(|surface| surface.phrase_id == phrase_id) {
            return None;
        }
        let task = self.tasks.iter().find(|task| live(task.state))?;
        let item = task.items.get(task.index)?;
        if item.phrase_id != phrase_id || hides_control(item.matched) {
            return None;
        }
        let drawn_ok = match (task.drawn, target) {
            (DrawnControls::Check, ClickTarget::Check) => true,
            (DrawnControls::YesNo, ClickTarget::Yes | ClickTarget::No) => true,
            _ => false,
        };
        if drawn_ok { Some(task.id.clone()) } else { None }
    }

    fn skip(&mut self, ledger: &mut Ledger, now_ms: u64) -> bool {
        if !self.tasks.iter().any(|task| task.state == TaskState::Active) {
            return false;
        }
        self.utterance_stops += 1;
        self.stop_pending = true;
        self.advance_active(ledger, now_ms);
        true
    }

    fn back(&mut self, ledger: &mut Ledger, now_ms: u64) -> bool {
        let Some(pos) = self.tasks.iter().position(|task| task.state == TaskState::Active) else {
            return false;
        };
        if self.tasks[pos].index == 0 {
            return true;
        }
        let task_id = self.tasks[pos].id.clone();
        let dwell = now_ms.saturating_sub(self.tasks[pos].entered_ms.unwrap_or(now_ms));
        let current = self.tasks[pos].items.get(self.tasks[pos].index).map(|item| (item.phrase_id.clone(), item.query.clone()));
        if let Some((phrase, query)) = current {
            self.append_tour(ledger, &phrase, &task_id, query, dwell, now_ms);
        }
        self.freeze_current(&task_id, now_ms);
        self.tasks[pos].index -= 1;
        self.enter_at(pos, ledger, now_ms);
        true
    }

    fn pause_active(&mut self) -> bool {
        let Some(task) = self.tasks.iter_mut().find(|task| task.state == TaskState::Active) else {
            return false;
        };
        task.state = TaskState::Paused;
        true
    }

    fn resume_active(&mut self, ledger: &mut Ledger, now_ms: u64) -> bool {
        let Some(pos) = self.tasks.iter().position(|task| task.state == TaskState::Paused) else {
            return false;
        };
        self.tasks[pos].state = TaskState::Active;
        self.consider(ledger, now_ms);
        true
    }

    fn consider(&mut self, ledger: &mut Ledger, now_ms: u64) {
        let snapshot = {
            let Some(task) = self.tasks.iter().find(|task| task.state == TaskState::Active) else { return };
            let Some(entered) = task.entered_ms else { return };
            let Some(item) = task.items.get(task.index) else { return };
            (task.pace, task.mode, entered, task.narration_done, task.item_timeout_seconds, item.phrase_id.clone(), item.matched)
        };
        let (pace, mode, entered, narration_done, timeout_s, phrase, matched) = snapshot;
        let timed_out = now_ms >= entered.saturating_add(u64::from(timeout_s) * 1000);
        let miss = hides_control(matched);
        let advance = match pace {
            TaskPace::Wait => timed_out || (!miss && item_answered(ledger, mode, &phrase)),
            TaskPace::Flow => miss || (narration_done && self.dwell_floor_met(&phrase, entered, now_ms)),
        };
        if advance {
            self.advance_active(ledger, now_ms);
        }
    }

    fn dwell_floor_met(&self, phrase: &str, entered: u64, now_ms: u64) -> bool {
        if let Some(shown) = &self.shown {
            if shown.phrase_id == phrase {
                return shown.dwelled || now_ms.saturating_sub(shown.at_ms) >= DWELL_MS;
            }
        }
        now_ms.saturating_sub(entered) >= DWELL_MS
    }

    fn advance_active(&mut self, ledger: &mut Ledger, now_ms: u64) {
        let Some(pos) = self.tasks.iter().position(|task| task.state == TaskState::Active) else { return };
        let task_id = self.tasks[pos].id.clone();
        let dwell = now_ms.saturating_sub(self.tasks[pos].entered_ms.unwrap_or(now_ms));
        let current = self.tasks[pos].items.get(self.tasks[pos].index).map(|item| (item.phrase_id.clone(), item.query.clone()));
        if let Some((phrase, query)) = current {
            self.append_tour(ledger, &phrase, &task_id, query, dwell, now_ms);
        }
        self.freeze_current(&task_id, now_ms);
        if self.tasks[pos].index + 1 >= self.tasks[pos].items.len() {
            self.tasks[pos].state = TaskState::Done;
            self.tasks[pos].entered_ms = None;
            self.tasks[pos].narration_done = false;
            self.clear_shown_for(&task_id);
            return;
        }
        self.tasks[pos].index += 1;
        self.enter_at(pos, ledger, now_ms);
    }

    fn enter_at(&mut self, pos: usize, ledger: &mut Ledger, now_ms: u64) {
        self.tasks[pos].entered_ms = Some(now_ms);
        self.tasks[pos].narration_done = false;
        let task_id = self.tasks[pos].id.clone();
        let current = self.tasks[pos].items.get(self.tasks[pos].index).map(|item| (item.phrase_id.clone(), item.query.clone()));
        if let Some((phrase, query)) = current {
            self.append_tour(ledger, &phrase, &task_id, query, 0, now_ms);
        }
    }

    fn append_tour(
        &mut self,
        ledger: &mut Ledger,
        phrase_id: &str,
        task_id: &str,
        query: Option<String>,
        dwell_ms: u64,
        now_ms: u64,
    ) {
        let _ = ledger.append_observed(
            Tier::T2Client,
            ObservedAppend::Tour {
                phrase_id: phrase_id.to_string(),
                task_id: Some(task_id.to_string()),
                query,
                dwell_ms,
            },
            now_ms,
        );
    }

    fn freeze_current(&mut self, task_id: &str, now_ms: u64) {
        let until = now_ms.saturating_add(GRACE_MS);
        for surface in &mut self.surfaces {
            if surface.task_id == task_id && surface.until_ms.is_none() {
                surface.until_ms = Some(until);
            }
        }
    }

    fn drop_expired(&mut self, now_ms: u64) {
        self.surfaces.retain(|surface| surface_alive(surface, now_ms));
    }

    fn mark_narration(&mut self, task_id: &str, ledger: &mut Ledger, now_ms: u64) -> Result<(), TaskError> {
        let pos = self.tasks.iter().position(|task| task.id == task_id).ok_or(TaskError::NoSuchTask)?;
        if !live(self.tasks[pos].state) {
            return Err(TaskError::TaskNotActive);
        }
        self.tasks[pos].narration_done = true;
        self.stop_pending = false;
        if self.tasks[pos].state == TaskState::Active {
            self.consider(ledger, now_ms);
        }
        Ok(())
    }

    fn preempt_task(&mut self, task_id: &str) -> Result<(), TaskError> {
        let task = self.task_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.state != TaskState::Active {
            return Err(TaskError::TaskNotActive);
        }
        task.state = TaskState::Paused;
        Ok(())
    }

    fn hotkey_ack(&mut self, ledger: &mut Ledger, now_ms: u64) -> bool {
        let Some(phrase) = self.tasks.iter().find(|task| live(task.state)).and_then(|task| {
            task.items.get(task.index).map(|item| item.phrase_id.clone())
        }) else {
            return false;
        };
        if hides_control(self.item_match(&phrase).unwrap_or(ItemMatch::Pending)) {
            return false;
        }
        if apply_ui_event(ledger, UiEvent::Acknowledge { phrase_id: phrase }, now_ms).is_err() {
            return false;
        }
        self.consider(ledger, now_ms);
        true
    }

    fn stop_active(&mut self) -> bool {
        let mut hit = false;
        let mut ids = Vec::new();
        for task in &mut self.tasks {
            if live(task.state) || matches!(task.state, TaskState::Pending | TaskState::AwaitingStart) {
                // Esc ends the task the same way as cancel: the one that is running.
                if live(task.state) {
                    task.state = TaskState::Dismissed;
                    ids.push(task.id.clone());
                    hit = true;
                }
            }
        }
        for id in ids {
            self.clear_shown_for(&id);
        }
        hit
    }

    fn phrase_is_live(&self, phrase_id: &str) -> bool {
        self.task_id_for_live_phrase(phrase_id).is_some()
    }

    fn task_id_for_live_phrase(&self, phrase_id: &str) -> Option<String> {
        self.tasks.iter().find_map(|task| {
            if live(task.state) && task.items.iter().any(|item| item.phrase_id == phrase_id) {
                Some(task.id.clone())
            } else {
                None
            }
        })
    }

    fn item_match(&self, phrase_id: &str) -> Option<ItemMatch> {
        self.tasks.iter().find_map(|task| {
            task.items.iter().find(|item| item.phrase_id == phrase_id).map(|item| item.matched)
        })
    }

    fn clear_shown_for(&mut self, task_id: &str) {
        let phrases: Vec<String> = self
            .task(task_id)
            .map(|task| task.items.iter().map(|item| item.phrase_id.clone()).collect())
            .unwrap_or_default();
        if self.shown.as_ref().is_some_and(|mark| phrases.iter().any(|id| id == &mark.phrase_id)) {
            self.shown = None;
        }
    }

    fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    fn task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    fn mint_task_id(&mut self) -> String {
        loop {
            let id = format!("tk-{}", self.next_task);
            self.next_task += 1;
            if !self.tasks.iter().any(|task| task.id == id) {
                return id;
            }
        }
    }
}

impl Default for TaskBook {
    fn default() -> Self {
        Self::new()
    }
}

fn runnable(state: TaskState) -> bool {
    matches!(state, TaskState::Pending | TaskState::AwaitingStart | TaskState::Active | TaskState::Paused)
}

fn live(state: TaskState) -> bool {
    matches!(state, TaskState::Active | TaskState::Paused)
}

fn open_for_end(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Pending | TaskState::AwaitingStart | TaskState::Active | TaskState::Paused
    )
}

fn current_item(task: &Task) -> Result<&Item, MoveError> {
    match task.state {
        TaskState::AwaitingStart => Err(MoveError::NeedsStartConfirm),
        TaskState::Active => task.items.get(task.index).ok_or(MoveError::NoActiveTour),
        _ => Err(MoveError::NoActiveTour),
    }
}

fn planned_for(task: &Task, item: &Item) -> ViewerMove {
    let doc_key = task.doc_key.clone().unwrap_or_default();
    if let Some(page) = item.page {
        ViewerMove::PdfPage { doc_key, page }
    } else {
        ViewerMove::WebFind { doc_key, opening_phrase: item.query.clone().unwrap_or_default() }
    }
}

fn move_fits(task: &Task, item: &Item, mv: &ViewerMove, on_glass: bool) -> Result<bool, MoveError> {
    match mv {
        ViewerMove::Nudge { dy_px } => {
            if !on_glass || dy_px.abs() != NUDGE_PX {
                return Err(MoveError::OffGlass);
            }
            Ok(true)
        }
        ViewerMove::WebFind { doc_key, opening_phrase } => {
            Ok(task.doc_key.as_deref() == Some(doc_key.as_str()) && item.query.as_deref() == Some(opening_phrase.as_str()))
        }
        ViewerMove::PdfPage { doc_key, page } => {
            Ok(task.doc_key.as_deref() == Some(doc_key.as_str()) && item.page == Some(*page))
        }
    }
}

/// Type-level hold: a new variant fails to compile here.
pub fn viewer_move_slot(mv: &ViewerMove) -> u8 {
    match mv {
        ViewerMove::WebFind { .. } => 0,
        ViewerMove::PdfPage { .. } => 1,
        ViewerMove::Nudge { .. } => 2,
    }
}

pub fn keyboard_grab_installed() -> bool {
    false
}

fn hides_control(matched: ItemMatch) -> bool {
    matches!(matched, ItemMatch::NotFound | ItemMatch::Ambiguous)
}

fn surface_alive(surface: &Surface, now_ms: u64) -> bool {
    surface.until_ms.is_none_or(|until| now_ms < until)
}

fn item_answered(ledger: &Ledger, mode: TaskMode, phrase_id: &str) -> bool {
    match mode {
        TaskMode::Approve => decision_set(ledger, phrase_id),
        TaskMode::Explain | TaskMode::Select => viewed_name(ledger, phrase_id) == "acknowledged",
    }
}

fn resolve_presentation(mode: TaskMode, controls: Option<ClientControls>) -> Result<DrawnControls, TaskError> {
    match mode {
        TaskMode::Approve => {
            if controls.is_some() {
                Err(TaskError::ApproveTakesNoControls)
            } else {
                Ok(DrawnControls::YesNo)
            }
        }
        TaskMode::Select => match controls {
            Some(ClientControls::None) => Err(TaskError::SelectRequiresControls),
            Some(ClientControls::Check) | None => Ok(DrawnControls::Check),
        },
        TaskMode::Explain => match controls {
            Some(ClientControls::Check) => Ok(DrawnControls::Check),
            Some(ClientControls::None) | None => Ok(DrawnControls::None),
        },
    }
}

fn default_pace(mode: TaskMode) -> TaskPace {
    match mode {
        TaskMode::Explain => TaskPace::Flow,
        TaskMode::Select | TaskMode::Approve => TaskPace::Wait,
    }
}

/// Squares beside `word`. Left when the gap fits, otherwise right, then clamped
/// to the output. The word rectangle is not one of the returned squares.
pub fn place_controls(word: PxRect, out_w: i32, out_h: i32, drawn: DrawnControls) -> Vec<(ClickTarget, PxRect)> {
    let y = (word.y + word.h / 2 - CONTROL_PX / 2).clamp(0, (out_h - CONTROL_PX).max(0));
    match drawn {
        DrawnControls::None => Vec::new(),
        DrawnControls::Check => {
            let x = place_block(word, out_w, CONTROL_PX);
            vec![(ClickTarget::Check, PxRect { x, y, w: CONTROL_PX, h: CONTROL_PX })]
        }
        DrawnControls::YesNo => {
            let width = CONTROL_PX + PAIR_GAP + CONTROL_PX;
            let x = place_block(word, out_w, width);
            vec![
                (ClickTarget::Yes, PxRect { x, y, w: CONTROL_PX, h: CONTROL_PX }),
                (ClickTarget::No, PxRect { x: x + CONTROL_PX + PAIR_GAP, y, w: CONTROL_PX, h: CONTROL_PX }),
            ]
        }
    }
}

fn place_block(word: PxRect, out_w: i32, width: i32) -> i32 {
    let left = word.x - CONTROL_GAP - width;
    let x = if left >= 0 { left } else { word.x + word.w + CONTROL_GAP };
    x.clamp(0, (out_w - width).max(0))
}

pub fn hit_control(rects: &[(ClickTarget, PxRect)], x: i32, y: i32) -> Option<ClickTarget> {
    rects.iter().rev().find(|(_, rect)| x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h).map(|(t, _)| *t)
}

/// The word mark and the avatar disc keep an empty input region.
pub fn word_mark_takes_input() -> bool {
    false
}

fn viewed_name(ledger: &Ledger, phrase_id: &str) -> &'static str {
    let Ok(value) = ledger.list_value(&ListPhraseFlagsBody {
        proposal_id: "view".into(),
        since: None,
        task_id: None,
        doc_key: None,
        decided_by: None,
        limit: None,
        events: false,
        wire_kind: None,
    }) else {
        return "none";
    };
    let Some(rows) = value.get("rows").and_then(|rows| rows.as_array()) else { return "none" };
    let Some(row) = rows.iter().find(|row| row.get("phraseId").and_then(|id| id.as_str()) == Some(phrase_id)) else {
        return "none";
    };
    match row.get("viewed").and_then(|v| v.as_str()) {
        Some("shown") => "shown",
        Some("dwelled") => "dwelled",
        Some("acknowledged") => "acknowledged",
        _ => "none",
    }
}

fn decision_set(ledger: &Ledger, phrase_id: &str) -> bool {
    let Ok(value) = ledger.list_value(&ListPhraseFlagsBody {
        proposal_id: "view".into(),
        since: None,
        task_id: None,
        doc_key: None,
        decided_by: None,
        limit: None,
        events: false,
        wire_kind: None,
    }) else {
        return false;
    };
    let Some(rows) = value.get("rows").and_then(|rows| rows.as_array()) else { return false };
    rows.iter().any(|row| {
        row.get("phraseId").and_then(|id| id.as_str()) == Some(phrase_id) && row.get("decision").and_then(|d| d.as_str()).is_some()
    })
}

fn err_wire(err: TaskError) -> &'static str {
    match err {
        TaskError::Schema => "schema",
        TaskError::QueueFull => "queue_full",
        TaskError::Quota => "quota: phrase cap",
        TaskError::NoSuchTask => "no_such_task",
        TaskError::NoSuchPresence => "no_such_presence",
        TaskError::SelectRequiresControls => "select_requires_controls",
        TaskError::ApproveTakesNoControls => "approve_takes_no_controls",
        TaskError::TaskNotActive => "task_not_active",
    }
}

fn cut(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn blank(value: &Option<String>) -> bool {
    value.as_ref().map(|text| text.trim().is_empty()).unwrap_or(true)
}

pub fn route_task(
    proposal: &Proposal,
    tier: Tier,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    presence_alive: bool,
    now_ms: u64,
) -> Option<ProposalResult> {
    match proposal {
        Proposal::CreateTask(body) => Some(route_create(tier, book, ledger, body, presence_alive, now_ms, false)),
        Proposal::DraftTask(body) => Some(route_create(tier, book, ledger, body, presence_alive, now_ms, true)),
        Proposal::CancelTask(body) => Some(route_cancel(tier, book, body)),
        Proposal::QueryTask(body) => Some(route_query(tier, book, body)),
        Proposal::ResolveItem(body) => Some(route_resolve(tier, book, ledger, body, now_ms)),
        Proposal::TourSignal(body) => Some(route_signal(tier, book, ledger, body, now_ms)),
        _ => None,
    }
}

fn route_create(
    tier: Tier,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    body: &CreateTaskBody,
    presence_alive: bool,
    now_ms: u64,
    draft: bool,
) -> ProposalResult {
    let kind = if draft { "draftTask" } else { "createTask" };
    let tier_ok = if draft { tier == Tier::T3Sidecar } else { matches!(tier, Tier::T1User | Tier::T2Client) };
    if !tier_ok {
        return ProposalResult::err(&body.proposal_id, format!("not_allowed_for_tier:{kind}"));
    }
    if !presence_alive {
        return ProposalResult::err(&body.proposal_id, "no_such_presence");
    }
    match insert_task(book, ledger, body, tier, now_ms, draft) {
        Ok(id) => ProposalResult::ok(&body.proposal_id, Some(book.query_value(&id).unwrap_or_else(|| json!({ "taskId": id })))),
        Err(err) => ProposalResult::err(&body.proposal_id, err_wire(err)),
    }
}

fn insert_task(
    book: &mut TaskBook,
    ledger: &mut Ledger,
    body: &CreateTaskBody,
    tier: Tier,
    now_ms: u64,
    draft: bool,
) -> Result<String, TaskError> {
    if body.items.is_empty() || body.items.len() > ITEM_CAP {
        return Err(TaskError::Schema);
    }
    let ttl = body.ttl_seconds.unwrap_or(DEFAULT_TTL_S);
    let item_timeout = body.item_timeout_seconds.unwrap_or(DEFAULT_ITEM_TIMEOUT_S);
    if ttl == 0 || item_timeout == 0 {
        return Err(TaskError::Schema);
    }
    let drawn = resolve_presentation(body.mode, body.controls)?;
    let pace = body.pace.unwrap_or_else(|| default_pace(body.mode));
    if !draft && book.runnable_count() >= QUEUE_CAP {
        return Err(TaskError::QueueFull);
    }
    let planned = plan_items(ledger, &body.items)?;
    let mut items = Vec::with_capacity(planned.len());
    for (spec, raw) in planned.into_iter().zip(body.items.iter()) {
        let phrase_id = if spec.minted {
            ledger.mint_and_hold().map_err(ledger_err)?
        } else {
            if spec.fresh {
                ledger.hold_phrase(&spec.phrase_id).map_err(ledger_err)?;
            }
            spec.phrase_id
        };
        items.push(Item {
            phrase_id,
            query: if blank(&raw.query) { None } else { raw.query.clone().map(|text| cut(&text, crate::ledger::TEXT_CAP)) },
            narration: raw.narration.as_deref().map(|text| cut(text, NARRATION_CAP)).unwrap_or_default(),
            scroll: raw.scroll,
            page: raw.page,
            matched: ItemMatch::Pending,
            attempts: 0,
        });
    }
    let id = book.mint_task_id();
    let scroll = body.items.iter().any(|item| item.scroll);
    let (origin, state) = if draft {
        (Origin::SidecarDraft, TaskState::Draft)
    } else if tier == Tier::T2Client && scroll {
        (Origin::Agent, TaskState::AwaitingStart)
    } else if tier == Tier::T1User {
        (Origin::User, TaskState::Pending)
    } else {
        (Origin::Agent, TaskState::Pending)
    };
    book.tasks.push(Task {
        id: id.clone(),
        presence_id: body.presence_id.clone(),
        origin,
        mode: body.mode,
        pace,
        drawn,
        state,
        prompt: body.prompt.as_deref().map(|text| cut(text, PROMPT_CAP)).unwrap_or_default(),
        ttl_seconds: ttl,
        item_timeout_seconds: item_timeout,
        deadline_ms: now_ms.saturating_add(u64::from(ttl) * 1000),
        doc_key: body.doc_key.as_ref().map(|text| cut(text, crate::ledger::TEXT_CAP)),
        items,
        index: 0,
        entered_ms: None,
        narration_done: false,
        last_move_ms: None,
    });
    Ok(id)
}

fn ledger_err(err: LedgerError) -> TaskError {
    match err {
        LedgerError::Quota => TaskError::Quota,
        LedgerError::Schema | LedgerError::Rejected(_) => TaskError::Schema,
    }
}

struct Planned {
    phrase_id: String,
    fresh: bool,
    minted: bool,
}

fn plan_items(ledger: &Ledger, items: &[TaskItemBody]) -> Result<Vec<Planned>, TaskError> {
    let mut planned = Vec::new();
    let mut fresh = 0usize;
    for item in items {
        let has_id = !blank(&item.phrase_id);
        let has_query = !blank(&item.query);
        if has_id == has_query {
            return Err(TaskError::Schema);
        }
        if has_id {
            let id = cut(item.phrase_id.as_deref().unwrap_or("").trim(), crate::ledger::TEXT_CAP);
            if id.is_empty() {
                return Err(TaskError::Schema);
            }
            let is_fresh = !ledger.holds(&id) && !planned.iter().any(|p: &Planned| p.phrase_id == id);
            if is_fresh {
                fresh += 1;
            }
            planned.push(Planned { phrase_id: id, fresh: is_fresh, minted: false });
        } else {
            fresh += 1;
            planned.push(Planned { phrase_id: String::new(), fresh: true, minted: true });
        }
    }
    if ledger.occupancy() + fresh > PHRASE_CAP {
        return Err(TaskError::Quota);
    }
    Ok(planned)
}

fn route_cancel(tier: Tier, book: &mut TaskBook, body: &crate::protocol::CancelTaskBody) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T2Client) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:cancelTask");
    }
    let id = body.task_id.clone();
    let found = {
        let Some(task) = book.task_mut(&id) else {
            return ProposalResult::err(&body.proposal_id, "no_such_task");
        };
        if open_for_end(task.state) || task.state == TaskState::Draft {
            task.state = TaskState::Dismissed;
        }
        true
    };
    let _ = found;
    book.clear_shown_for(&id);
    ProposalResult::ok(&body.proposal_id, book.query_value(&id))
}

fn route_resolve(
    tier: Tier,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    body: &ResolveItemBody,
    now_ms: u64,
) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T2Client) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:resolveItem");
    }
    let uv = match crate::ledger::checked_uv(body.u, body.v, body.w, body.h) {
        Ok(uv) => uv,
        Err(()) => return ProposalResult::err(&body.proposal_id, "schema"),
    };
    let Some(task) = book.task(&body.task_id) else {
        return ProposalResult::err(&body.proposal_id, "no_such_task");
    };
    if !live(task.state) {
        return ProposalResult::err(&body.proposal_id, "task_not_active");
    }
    let Some(item) = task.items.iter().find(|item| item.phrase_id == body.phrase_id) else {
        return ProposalResult::err(&body.proposal_id, "schema");
    };
    let query = item.query.clone();
    let doc_key = task.doc_key.clone();
    let active = task.state == TaskState::Active;
    let matched = match body.match_info.status {
        MatchStatus::Located => ItemMatch::Located,
        MatchStatus::Ambiguous => ItemMatch::Ambiguous,
        MatchStatus::NotFound => ItemMatch::NotFound,
    };
    let located = ObservedAppend::Located {
        phrase_id: body.phrase_id.clone(),
        task_id: Some(body.task_id.clone()),
        query,
        doc_key,
        matched: MatchInfo {
            status: body.match_info.status,
            conf: body.match_info.conf,
            ocr_snippet: body.match_info.ocr_snippet.clone(),
        },
        uv,
    };
    if ledger.append_observed(tier, located, now_ms).is_err() {
        return ProposalResult::err(&body.proposal_id, "schema");
    }
    if book.note_match(&body.task_id, &body.phrase_id, matched).is_err() {
        return ProposalResult::err(&body.proposal_id, "schema");
    }
    if active {
        book.consider(ledger, now_ms);
    }
    ProposalResult::ok(&body.proposal_id, book.query_value(&body.task_id))
}

fn route_signal(
    tier: Tier,
    book: &mut TaskBook,
    ledger: &mut Ledger,
    body: &TourSignalBody,
    now_ms: u64,
) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T2Client) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:tourSignal");
    }
    if book.task(&body.task_id).is_none() {
        return ProposalResult::err(&body.proposal_id, "no_such_task");
    }
    let result = match body.signal {
        TourSignalKind::Begin => book.begin_task(&body.task_id, ledger, now_ms),
        TourSignalKind::NarrationDone => book.mark_narration(&body.task_id, ledger, now_ms),
        TourSignalKind::Preempt => {
            match body.reason {
                Some(crate::protocol::PreemptReason::Page | crate::protocol::PreemptReason::Pointer) | None => {}
            }
            book.preempt_task(&body.task_id)
        }
    };
    match result {
        Ok(()) => ProposalResult::ok(&body.proposal_id, book.query_value(&body.task_id)),
        Err(err) => ProposalResult::err(&body.proposal_id, err_wire(err)),
    }
}

fn route_query(tier: Tier, book: &TaskBook, body: &crate::protocol::QueryTaskBody) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T2Client) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:queryTask");
    }
    match book.query_value(&body.task_id) {
        Some(detail) => ProposalResult::ok(&body.proposal_id, Some(detail)),
        None => ProposalResult::err(&body.proposal_id, "no_such_task"),
    }
}

impl TaskBook {
    pub fn task_len(&self) -> usize {
        self.tasks.len()
    }

    pub fn query_value(&self, task_id: &str) -> Option<Value> {
        let task = self.task(task_id)?;
        let current = task.index;
        let items: Vec<Value> = task
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                json!({
                    "phraseId": item.phrase_id,
                    "query": item.query,
                    "narration": item.narration,
                    "scroll": item.scroll,
                    "page": item.page,
                    "match": match_wire(item.matched),
                    "control": item_draws(task, item, index == current),
                })
            })
            .collect();
        Some(json!({
            "taskId": task.id,
            "presenceId": task.presence_id,
            "origin": match task.origin {
                Origin::User => "user",
                Origin::Agent => "agent",
                Origin::SidecarDraft => "sidecar-draft",
            },
            "mode": match task.mode {
                TaskMode::Explain => "explain",
                TaskMode::Select => "select",
                TaskMode::Approve => "approve",
            },
            "pace": match task.pace {
                TaskPace::Flow => "flow",
                TaskPace::Wait => "wait",
            },
            "controls": match task.drawn {
                DrawnControls::None => "none",
                DrawnControls::Check => "check",
                DrawnControls::YesNo => "yesno",
            },
            "state": state_wire(task.state),
            "ttlSeconds": task.ttl_seconds,
            "ttlDeadlineMs": task.deadline_ms,
            "itemTimeoutSeconds": task.item_timeout_seconds,
            "prompt": task.prompt,
            "docKey": task.doc_key,
            "index": task.index,
            "items": items,
            "stopUtterance": self.stop_pending,
        }))
    }

}

fn item_draws(task: &Task, item: &Item, current: bool) -> bool {
    current && live(task.state) && !hides_control(item.matched) && task.drawn != DrawnControls::None
}

fn state_wire(state: TaskState) -> &'static str {
    match state {
        TaskState::Draft => "draft",
        TaskState::Pending => "pending",
        TaskState::AwaitingStart => "awaitingStart",
        TaskState::Active => "active",
        TaskState::Paused => "paused",
        TaskState::Done => "done",
        TaskState::Dismissed => "dismissed",
        TaskState::Expired => "expired",
    }
}

fn match_wire(matched: ItemMatch) -> &'static str {
    match matched {
        ItemMatch::Pending => "pending",
        ItemMatch::Located => "located",
        ItemMatch::Ambiguous => "ambiguous",
        ItemMatch::NotFound => "notFound",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Tier;
    use crate::ledger::{LedgerKind, ObservedAppend, ViewedTier};
    use crate::protocol::{MatchInfo, MatchStatus};
    use crate::registry::Registry;
    use crate::ui_input::{ClickTarget, Hotkey, UiEvent};

    fn parse(json: &str) -> Proposal {
        serde_json::from_str(json).unwrap_or_else(|err| panic!("parse {json}: {err}"))
    }

    fn create(book: &mut TaskBook, ledger: &mut Ledger, json: &str, tier: Tier, now: u64) -> ProposalResult {
        route_task(&parse(json), tier, book, ledger, true, now).unwrap()
    }

    fn task_line(mode: &str, extra: &str, item: &str) -> String {
        format!(r#"{{"kind":"createTask","proposalId":"c","presenceId":"p","mode":"{mode}"{extra},"items":[{item}]}}"#)
    }

    fn explain_line() -> String {
        task_line("explain", "", r#"{"query":"the line"}"#)
    }

    struct Opened {
        book: TaskBook,
        ledger: Ledger,
        id: String,
        phrase: String,
    }

    fn open(json: &str) -> Opened {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, json, Tier::T2Client, 0);
        assert_eq!(result.status, "ok", "{}", result.error.unwrap_or_default());
        let detail = result.detail.unwrap();
        assert!(book.admit().is_some());
        Opened {
            id: detail["taskId"].as_str().unwrap().to_string(),
            phrase: detail["items"][0]["phraseId"].as_str().unwrap().to_string(),
            book,
            ledger,
        }
    }

    fn book_query(book: &TaskBook, id: &str) -> Value {
        book.query_value(id).unwrap_or(Value::Null)
    }

    fn row<'a>(ledger: &'a Ledger, phrase: &str) -> Value {
        let value = ledger
            .list_value(&ListPhraseFlagsBody {
                proposal_id: "p".into(),
                since: None,
                task_id: None,
                doc_key: None,
                decided_by: None,
                limit: None,
                events: false,
                wire_kind: None,
            })
            .unwrap();
        value["rows"].as_array().unwrap().iter().find(|row| row["phraseId"] == phrase).cloned().unwrap_or(Value::Null)
    }

    fn word_at(x: i32) -> PxRect {
        PxRect { x, y: 40, w: 80, h: 24 }
    }

    fn overlaps(a: PxRect, b: PxRect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn a3_draft_draws_nothing_and_does_not_count() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let rejected = create(
            &mut book,
            &mut ledger,
            r#"{"kind":"draftTask","proposalId":"d","presenceId":"p","mode":"explain","items":[{"query":"the line"}]}"#,
            Tier::T2Client,
            0,
        );
        assert_eq!(rejected.error.as_deref(), Some("not_allowed_for_tier:draftTask"));
        assert_eq!(book.len(), 0);
        let drafted = create(
            &mut book,
            &mut ledger,
            r#"{"kind":"draftTask","proposalId":"d","presenceId":"p","mode":"approve","items":[{"query":"the line"}]}"#,
            Tier::T3Sidecar,
            0,
        );
        assert_eq!(drafted.status, "ok");
        let id = drafted.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        let view = book_query(&book, &id);
        assert_eq!(view["state"], "draft");
        assert_eq!(view["origin"], "sidecar-draft");
        assert!(book.control_rects(&id, word_at(100), 1920, 1080).is_empty());
        assert_eq!(book.runnable_count(), 0);
        for n in 0..5 {
            let line = format!(r#"{{"kind":"createTask","proposalId":"c{n}","presenceId":"p","mode":"explain","items":[{{"query":"q{n}"}}]}}"#);
            assert_eq!(create(&mut book, &mut ledger, &line, Tier::T2Client, 0).status, "ok");
        }
        assert_eq!(book.runnable_count(), 5);
        let sixth = create(
            &mut book,
            &mut ledger,
            r#"{"kind":"createTask","proposalId":"c6","presenceId":"p","mode":"explain","items":[{"query":"too many"}]}"#,
            Tier::T2Client,
            0,
        );
        assert_eq!(sixth.error.as_deref(), Some("queue_full"));
        assert_eq!(book.runnable_count(), 5);
        assert!(ledger.events().is_empty());
    }

    #[test]
    fn b2_b3_e12_e13_decision_fields_and_item_shape_fail() {
        let base = r#"{"kind":"createTask","proposalId":"c","presenceId":"p","mode":"explain","items":[{"query":"the line"}]"#;
        for field in [r#","checked":true"#, r#","approved":true"#, r#","discarded":true"#, r#","decision":"approved""#, r#","viewed":"acknowledged""#] {
            let line = format!("{base}{field}}}");
            assert!(serde_json::from_str::<Proposal>(&line).is_err(), "{field}");
        }
        let draft = r#"{"kind":"draftTask","proposalId":"d","presenceId":"p","mode":"explain","items":[{"query":"the line"}],"checked":true}"#;
        assert!(serde_json::from_str::<Proposal>(draft).is_err());
        assert!(serde_json::from_str::<Proposal>(r#"{"kind":"controlTask","proposalId":"c","taskId":"tk-1"}"#).is_err());
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let both = create(&mut book, &mut ledger, &task_line("explain", "", r#"{"phraseId":"ph-9","query":"the line"}"#), Tier::T2Client, 0);
        assert_eq!(both.error.as_deref(), Some("schema"));
        let neither = create(&mut book, &mut ledger, &task_line("explain", "", "{}"), Tier::T2Client, 0);
        assert_eq!(neither.error.as_deref(), Some("schema"));
        let empty = create(&mut book, &mut ledger, r#"{"kind":"createTask","proposalId":"c","presenceId":"p","mode":"explain","items":[]}"#, Tier::T2Client, 0);
        assert_eq!(empty.error.as_deref(), Some("schema"));
        assert_eq!(book.len(), 0);
        assert!(ledger.events().is_empty());
    }

    #[test]
    fn b7_click_outside_the_active_task_is_dropped() {
        let mut opened = open(&explain_line().replace(r#""mode":"explain""#, r#""mode":"explain","controls":"check""#));
        let before = opened.ledger.events().len();
        let wrote = opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: "ph-other".into(), target: ClickTarget::Check, ts_ms: 1 },
            2,
        );
        assert!(!wrote);
        assert_eq!(opened.ledger.events().len(), before);
    }

    #[test]
    fn b8_click_after_expiry_cancel_or_completion_is_dropped() {
        let line = task_line("select", r#","ttlSeconds":1"#, r#"{"query":"the line"}"#);
        let mut opened = open(&line);
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 0);
        let before = opened.ledger.events().len();
        opened.book.tick(&mut opened.ledger, 1000);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "expired");
        assert!(!opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 3 },
            1001,
        ));
        assert_eq!(opened.ledger.events().len(), before);

        let mut opened = open(&task_line("select", "", r#"{"query":"the line"}"#));
        let cancel = create(
            &mut opened.book,
            &mut opened.ledger,
            &format!(r#"{{"kind":"cancelTask","proposalId":"x","taskId":"{}"}}"#, opened.id),
            Tier::T1User,
            5,
        );
        assert_eq!(cancel.status, "ok");
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "dismissed");
        assert_eq!(book_query(&opened.book, &opened.id)["items"][0]["phraseId"], opened.phrase);
        let before = opened.ledger.events().len();
        assert!(!opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 6 },
            6,
        ));
        assert_eq!(opened.ledger.events().len(), before);

        let mut opened = open(&task_line("select", "", r#"{"query":"the line"}"#));
        opened.book.complete(&opened.id).unwrap();
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "done");
        let before = opened.ledger.events().len();
        assert!(!opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase, target: ClickTarget::Check, ts_ms: 7 },
            7,
        ));
        assert_eq!(opened.ledger.events().len(), before);
    }

    #[test]
    fn b9_same_pointer_down_writes_once() {
        let mut opened = open(&task_line("select", "", r#"{"query":"the line"}"#));
        let click = UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 9 };
        assert!(opened.book.handle_ui(&mut opened.ledger, click, 9));
        let click = UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 9 };
        assert!(!opened.book.handle_ui(&mut opened.ledger, click, 9));
        let acks = opened.ledger.events().iter().filter(|ev| ev.viewed == Some(ViewedTier::Acknowledged)).count();
        assert_eq!(acks, 1);
    }

    #[test]
    fn c1_explain_none_is_click_through_and_dwells() {
        let mut opened = open(&explain_line());
        assert!(opened.book.control_rects(&opened.id, word_at(100), 1920, 1080).is_empty());
        assert!(!word_mark_takes_input());
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 0);
        assert_eq!(opened.ledger.events().len(), 1);
        assert_eq!(opened.ledger.events()[0].viewed, Some(ViewedTier::Shown));
        assert_eq!(opened.ledger.events()[0].by, Tier::T2Client);
        opened.book.tick(&mut opened.ledger, 1199);
        assert_eq!(opened.ledger.events().len(), 1);
        opened.book.tick(&mut opened.ledger, 1200);
        assert_eq!(opened.ledger.events().len(), 2);
        assert_eq!(opened.ledger.events()[1].viewed, Some(ViewedTier::Dwelled));
        assert_eq!(opened.ledger.events()[1].by, Tier::T2Client);
    }

    #[test]
    fn c2_c5_checkbox_writes_only_on_the_click() {
        let mut opened = open(&task_line("explain", r#","controls":"check""#, r#"{"query":"the line"}"#));
        let rects = opened.book.control_rects(&opened.id, word_at(100), 1920, 1080);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].0, ClickTarget::Check);
        assert!(opened.ledger.events().is_empty());
        assert!(opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 4 },
            4,
        ));
        assert_eq!(opened.ledger.events().last().unwrap().by, Tier::T0Physical);
        assert_eq!(row(&opened.ledger, &opened.phrase)["viewed"], "acknowledged");
    }

    #[test]
    fn c3_select_none_is_rejected() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &task_line("select", r#","controls":"none""#, r#"{"query":"the line"}"#), Tier::T2Client, 0);
        assert_eq!(result.error.as_deref(), Some("select_requires_controls"));
        assert_eq!(book.len(), 0);
    }

    #[test]
    fn c4_approve_rejects_a_controls_value() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &task_line("approve", r#","controls":"check""#, r#"{"query":"the line"}"#), Tier::T1User, 0);
        assert_eq!(result.error.as_deref(), Some("approve_takes_no_controls"));
        assert_eq!(book.len(), 0);
        assert!(ledger.events().is_empty());
    }

    #[test]
    fn c6_hotkey_ack_with_no_control() {
        let mut opened = open(&explain_line());
        assert!(opened.book.control_rects(&opened.id, word_at(100), 1920, 1080).is_empty());
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Ack), 3));
        assert_eq!(opened.ledger.events().last().unwrap().by, Tier::T0Physical);
        assert_eq!(row(&opened.ledger, &opened.phrase)["viewed"], "acknowledged");
    }

    #[test]
    fn c7_c8_checked_follows_the_ledger() {
        let mut opened = open(&task_line("explain", r#","controls":"check""#, r#"{"query":"the line"}"#));
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Ack), 1));
        let before = opened.ledger.events().len();
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 2);
        assert_eq!(opened.ledger.events().len(), before);
        assert!(opened.book.checkbox_checked(&opened.ledger, &opened.phrase));
        assert!(opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 3 },
            3,
        ));
        assert!(!opened.book.checkbox_checked(&opened.ledger, &opened.phrase));
        opened.book.note_hidden(&opened.phrase);
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 4);
        assert!(!opened.book.checkbox_checked(&opened.ledger, &opened.phrase));
    }

    #[test]
    fn c9_uncheck_returns_to_dwelled() {
        let mut opened = open(&task_line("select", "", r#"{"query":"the line"}"#));
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 0);
        opened.book.tick(&mut opened.ledger, 1200);
        assert!(opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 1300 },
            1300,
        ));
        assert_eq!(row(&opened.ledger, &opened.phrase)["viewed"], "acknowledged");
        assert!(opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Check, ts_ms: 2300 },
            2300,
        ));
        assert_eq!(opened.ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Cleared).count(), 1);
        assert_eq!(row(&opened.ledger, &opened.phrase)["viewed"], "dwelled");
    }

    #[test]
    fn c10_yes_and_no_are_both_kept() {
        let mut opened = open(&task_line("approve", "", r#"{"query":"the line"}"#));
        let rects = opened.book.control_rects(&opened.id, word_at(100), 1920, 1080);
        assert_eq!(rects.iter().map(|(t, _)| *t).collect::<Vec<_>>(), vec![ClickTarget::Yes, ClickTarget::No]);
        assert!(opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::Yes, ts_ms: 1 },
            1,
        ));
        assert!(opened.book.handle_ui(
            &mut opened.ledger,
            UiEvent::Click { phrase_id: opened.phrase.clone(), target: ClickTarget::No, ts_ms: 2 },
            2,
        ));
        assert!(opened.ledger.events().iter().any(|ev| ev.kind == LedgerKind::Approved));
        assert!(opened.ledger.events().iter().any(|ev| ev.kind == LedgerKind::Discarded));
        let view = row(&opened.ledger, &opened.phrase);
        assert_eq!(view["decision"], "discarded");
        assert_eq!(view["decisionBy"], "T0");
    }

    #[test]
    fn c11_presentation_writes_no_ledger_event() {
        let mut opened = open(&explain_line());
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 1);
        let before = opened.ledger.events().len();
        opened.book.set_presentation(&opened.id, TaskMode::Explain, TaskPace::Wait, Some(ClientControls::Check)).unwrap();
        assert_eq!(opened.ledger.events().len(), before);
        let view = book_query(&opened.book, &opened.id);
        assert_eq!(view["pace"], "wait");
        assert_eq!(view["controls"], "check");
        let err = opened.book.set_presentation(&opened.id, TaskMode::Approve, TaskPace::Flow, Some(ClientControls::None)).unwrap_err();
        assert!(matches!(err, TaskError::ApproveTakesNoControls));
        assert_eq!(opened.ledger.events().len(), before);
        assert_eq!(book_query(&opened.book, &opened.id)["controls"], "check");
    }

    #[test]
    fn c12_c13_clock_runs_without_a_checkbox_and_stops_when_hidden() {
        let mut opened = open(&explain_line());
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 0);
        opened.book.note_hidden(&opened.phrase);
        opened.book.tick(&mut opened.ledger, 5000);
        assert_eq!(opened.ledger.events().len(), 1);
        assert_eq!(opened.ledger.events()[0].viewed, Some(ViewedTier::Shown));
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 5000);
        opened.book.tick(&mut opened.ledger, 6200);
        assert_eq!(opened.ledger.events().last().unwrap().viewed, Some(ViewedTier::Dwelled));
    }

    #[test]
    fn e1_pending_active_done() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &explain_line(), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(book_query(&book, &id)["state"], "pending");
        assert_eq!(book_query(&book, &id)["origin"], "user");
        assert_eq!(book.admit().as_deref(), Some(id.as_str()));
        assert_eq!(book_query(&book, &id)["state"], "active");
        book.complete(&id).unwrap();
        assert_eq!(book_query(&book, &id)["state"], "done");
    }

    #[test]
    fn e2_second_task_queues_and_the_sixth_is_full() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let first = create(&mut book, &mut ledger, &explain_line(), Tier::T2Client, 0);
        let first_id = first.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        let second = create(
            &mut book,
            &mut ledger,
            r#"{"kind":"createTask","proposalId":"c2","presenceId":"p","mode":"explain","items":[{"query":"next"}]}"#,
            Tier::T2Client,
            1,
        );
        let second_id = second.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(book_query(&book, &first_id)["state"], "active");
        assert_eq!(book_query(&book, &second_id)["state"], "pending");
        for n in 3..=5 {
            let line = format!(r#"{{"kind":"createTask","proposalId":"c{n}","presenceId":"p","mode":"explain","items":[{{"query":"q{n}"}}]}}"#);
            assert_eq!(create(&mut book, &mut ledger, &line, Tier::T2Client, 1).status, "ok");
        }
        let sixth = create(
            &mut book,
            &mut ledger,
            r#"{"kind":"createTask","proposalId":"c6","presenceId":"p","mode":"explain","items":[{"query":"nope"}]}"#,
            Tier::T2Client,
            1,
        );
        assert_eq!(sixth.error.as_deref(), Some("queue_full"));
        assert_eq!(book.runnable_count(), 5);
        assert!(ledger.events().is_empty());
    }

    #[test]
    fn e5_esc_dismisses_and_leaves_the_ledger() {
        let mut opened = open(&explain_line());
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 1);
        let before = opened.ledger.events().len();
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Stop), 2));
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "dismissed");
        assert_eq!(opened.ledger.events().len(), before);
        assert_eq!(row(&opened.ledger, &opened.phrase)["viewed"], "shown");
    }

    #[test]
    fn e14_each_mode_draws_its_controls_off_the_word() {
        let samples = [
            (task_line("explain", "", r#"{"query":"the line"}"#), 0),
            (task_line("explain", r#","controls":"check""#, r#"{"query":"the line"}"#), 1),
            (task_line("select", "", r#"{"query":"the line"}"#), 1),
            (task_line("approve", "", r#"{"query":"the line"}"#), 2),
        ];
        let word = word_at(100);
        for (line, n) in samples {
            let opened = open(&line);
            let rects = opened.book.control_rects(&opened.id, word, 1920, 1080);
            assert_eq!(rects.len(), n, "{line}");
            assert!(rects.iter().all(|(_, rect)| !overlaps(*rect, word)));
            assert!(!word_mark_takes_input());
        }
        let opened = open(&task_line("explain", r#","controls":"check""#, r#"{"query":"the line"}"#));
        let left = opened.book.control_rects(&opened.id, word_at(100), 1920, 1080);
        assert_eq!(left[0].1.x, 100 - CONTROL_GAP - CONTROL_PX);
        let right = opened.book.control_rects(&opened.id, word_at(10), 1920, 1080);
        assert_eq!(right[0].1.x, 10 + 80 + CONTROL_GAP);
        let pair = open(&task_line("approve", "", r#"{"query":"the line"}"#));
        let yes = pair.book.control_rects(&pair.id, word_at(100), 1920, 1080);
        assert_eq!(yes[0].1.x, 100 - CONTROL_GAP - (CONTROL_PX + PAIR_GAP + CONTROL_PX));
        assert_eq!(yes[1].1.x, yes[0].1.x + CONTROL_PX + PAIR_GAP);
        let tight = pair.book.control_rects(&pair.id, PxRect { x: 0, y: 0, w: 70, h: 10 }, 80, 40);
        for (_, rect) in tight {
            assert!(rect.x >= 0 && rect.x + rect.w <= 80);
            assert!(rect.y >= 0 && rect.y + rect.h <= 40);
        }
    }

    #[test]
    fn e15_e16_not_found_and_ambiguous_draw_no_control() {
        let mut approve = open(&task_line("approve", "", r#"{"query":"the line"}"#));
        approve.book.note_match(&approve.id, &approve.phrase, ItemMatch::NotFound).unwrap();
        assert!(approve.book.control_rects(&approve.id, word_at(100), 1920, 1080).is_empty());
        assert_eq!(book_query(&approve.book, &approve.id)["items"][0]["control"], false);
        let mut explain = open(&task_line("explain", r#","controls":"check""#, r#"{"query":"the line"}"#));
        explain.book.note_match(&explain.id, &explain.phrase, ItemMatch::Ambiguous).unwrap();
        assert!(explain.book.control_rects(&explain.id, word_at(100), 1920, 1080).is_empty());
        let mut approve = open(&task_line("approve", "", r#"{"query":"other"}"#));
        approve.book.note_match(&approve.id, &approve.phrase, ItemMatch::Ambiguous).unwrap();
        assert!(approve.book.control_rects(&approve.id, word_at(100), 1920, 1080).is_empty());
    }

    #[test]
    fn phrase_cap_counts_reserved_ids_and_mints_do_not_collide() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        for i in 0..4 {
            let items = (0..12).map(|n| format!(r#"{{"query":"t{i}-{n}"}}"#)).collect::<Vec<_>>().join(",");
            let line = format!(r#"{{"kind":"createTask","proposalId":"c{i}","presenceId":"p","mode":"explain","items":[{items}]}}"#);
            assert_eq!(create(&mut book, &mut ledger, &line, Tier::T2Client, 0).status, "ok");
        }
        assert_eq!(ledger.occupancy(), 48);
        let items = (0..3).map(|n| format!(r#"{{"query":"extra-{n}"}}"#)).collect::<Vec<_>>().join(",");
        let over = create(
            &mut book,
            &mut ledger,
            &format!(r#"{{"kind":"createTask","proposalId":"over","presenceId":"p","mode":"explain","items":[{items}]}}"#),
            Tier::T2Client,
            0,
        );
        assert_eq!(over.error.as_deref(), Some("quota: phrase cap"));
        assert_eq!(book.len(), 4);
        assert_eq!(ledger.occupancy(), 48);
        assert!(ledger.events().is_empty());

        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &explain_line(), Tier::T2Client, 0);
        let phrase = result.detail.unwrap()["items"][0]["phraseId"].as_str().unwrap().to_string();
        assert_eq!(phrase, "ph-1");
        let observe = parse(r#"{"kind":"observePhrase","proposalId":"o","match":{"status":"located","conf":1}}"#);
        let observed = crate::ledger::route_ledger(&observe, Tier::T3Sidecar, &mut ledger, 1).unwrap();
        assert_eq!(observed.detail.unwrap()["phraseId"], "ph-2");
    }

    #[test]
    fn filled_ledger_rejects_a_new_task_phrase() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        for i in 0..crate::ledger::PHRASE_CAP {
            ledger
                .append_observed(
                    Tier::T3Sidecar,
                    ObservedAppend::Located {
                        phrase_id: format!("id-{i}"),
                        task_id: None,
                        query: None,
                        doc_key: None,
                        matched: MatchInfo { status: MatchStatus::Located, conf: 1.0, ocr_snippet: None },
                        uv: None,
                    },
                    i as u64,
                )
                .unwrap();
        }
        let result = create(&mut book, &mut ledger, &explain_line(), Tier::T2Client, 1);
        assert_eq!(result.error.as_deref(), Some("quota: phrase cap"));
        assert_eq!(book.len(), 0);
        assert_eq!(ledger.events().len(), crate::ledger::PHRASE_CAP);
    }

    #[test]
    fn retire_dismisses_the_task_and_keeps_the_rows() {
        let mut reg = Registry::new();
        reg.spawn_presence("p", "ctx", None, None);
        let id = {
            let (book, ledger) = reg.task_and_ledger();
            let result = route_task(&parse(&explain_line()), Tier::T2Client, book, ledger, true, 0).unwrap();
            result.detail.unwrap()["taskId"].as_str().unwrap().to_string()
        };
        reg.task_book_mut().admit().unwrap();
        reg.note_task_highlight("p", 5);
        let before = reg.ledger().events().len();
        assert!(before >= 1);
        reg.retire_presence("p").unwrap();
        assert_eq!(reg.ledger().events().len(), before);
        let view = book_query(reg.task_book(), &id);
        assert_eq!(view["state"], "dismissed");
        assert!(!reg.ledger().list_value(&ListPhraseFlagsBody {
            proposal_id: "p".into(),
            since: None,
            task_id: None,
            doc_key: None,
            decided_by: None,
            limit: None,
            events: false,
            wire_kind: None,
        }).unwrap()["rows"].as_array().unwrap().is_empty());
    }

    #[test]
    fn absent_presence_and_prompt_cut_and_mark_region() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let missing = route_task(&parse(&explain_line()), Tier::T2Client, &mut book, &mut ledger, false, 0).unwrap();
        assert_eq!(missing.error.as_deref(), Some("no_such_presence"));
        assert_eq!(book.len(), 0);
        let prompt = "p".repeat(150);
        let line = format!(r#"{{"kind":"createTask","proposalId":"c","presenceId":"p","mode":"explain","prompt":"{prompt}","items":[{{"query":"the line"}}]}}"#);
        let result = create(&mut book, &mut ledger, &line, Tier::T2Client, 0);
        assert_eq!(result.detail.unwrap()["prompt"].as_str().unwrap().chars().count(), 120);
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/overlay.rs")).unwrap();
        let start = src.find("fn spawn_mark(").unwrap();
        let body = &src[start..];
        let end = body.find("\n    fn ").unwrap();
        let body = &body[..end];
        assert!(!body.contains("region.add"));
        assert!(body.contains("KeyboardInteractivity::None"));
        assert!(body.contains("set_exclusive_zone(-1)"));
    }

    #[test]
    fn hit_control_uses_the_upper_square() {
        let lower = (ClickTarget::Check, PxRect { x: 0, y: 0, w: 28, h: 28 });
        let upper = (ClickTarget::Yes, PxRect { x: 10, y: 10, w: 28, h: 28 });
        assert_eq!(hit_control(&[lower, upper], 12, 12), Some(ClickTarget::Yes));
        assert_eq!(hit_control(&[lower, upper], 2, 2), Some(ClickTarget::Check));
        assert_eq!(hit_control(&[lower], 100, 100), None);
    }

    #[test]
    fn zero_ttl_is_schema_and_defaults_apply() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let zero = create(&mut book, &mut ledger, &task_line("explain", r#","ttlSeconds":0"#, r#"{"query":"the line"}"#), Tier::T2Client, 0);
        assert_eq!(zero.error.as_deref(), Some("schema"));
        let result = create(&mut book, &mut ledger, &explain_line(), Tier::T2Client, 50);
        let view = result.detail.unwrap();
        assert_eq!(view["ttlSeconds"], 120);
        assert_eq!(view["ttlDeadlineMs"], 50 + 120_000);
        assert_eq!(view["pace"], "flow");
        assert_eq!(view["controls"], "none");
        assert_eq!(view["origin"], "agent");
        assert_eq!(view["items"][0]["match"], "pending");
    }

    fn item_ids(book: &TaskBook, id: &str) -> Vec<String> {
        book_query(book, id)["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["phraseId"].as_str().unwrap().to_string())
            .collect()
    }

    fn signal(book: &mut TaskBook, ledger: &mut Ledger, id: &str, name: &str, now: u64) -> ProposalResult {
        let line = format!(r#"{{"kind":"tourSignal","proposalId":"t","taskId":"{id}","signal":"{name}"}}"#);
        route_task(&parse(&line), Tier::T2Client, book, ledger, true, now).unwrap()
    }

    fn resolve(book: &mut TaskBook, ledger: &mut Ledger, id: &str, phrase: &str, status: &str, extra: &str, tier: Tier, now: u64) -> ProposalResult {
        let line = format!(
            r#"{{"kind":"resolveItem","proposalId":"r","taskId":"{id}","phraseId":"{phrase}","match":{{"status":"{status}","conf":0.4}}{extra}}}"#
        );
        route_task(&parse(&line), tier, book, ledger, true, now).unwrap()
    }

    fn click(book: &mut TaskBook, ledger: &mut Ledger, phrase: &str, target: ClickTarget, ts: u64) -> bool {
        book.handle_ui(ledger, UiEvent::Click { phrase_id: phrase.to_string(), target, ts_ms: ts }, ts)
    }

    #[test]
    fn e6_wait_timeout_advances_undecided() {
        let mut opened = open(&task_line("select", r#","itemTimeoutSeconds":1"#, r#"{"query":"one"},{"query":"two"}"#));
        assert_eq!(signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0).status, "ok");
        assert_eq!(signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 1).status, "ok");
        assert_eq!(opened.ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Tour).count(), 1);
        assert_eq!(opened.ledger.events()[0].dwell_ms, Some(0));
        opened.book.tick(&mut opened.ledger, 999);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        opened.book.tick(&mut opened.ledger, 1000);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
        let phrase = item_ids(&opened.book, &opened.id)[0].clone();
        assert_eq!(row(&opened.ledger, &phrase)["viewed"], "none");
        assert!(row(&opened.ledger, &phrase)["decision"].is_null());
        assert!(opened.ledger.events().iter().all(|ev| ev.viewed != Some(ViewedTier::Acknowledged)));
        assert!(opened.ledger.events().iter().all(|ev| !matches!(ev.kind, LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Cleared)));
    }

    #[test]
    fn e7_wait_click_advances() {
        let mut opened = open(&task_line("select", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        assert!(click(&mut opened.book, &mut opened.ledger, &ids[0], ClickTarget::Check, 50));
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
        assert_eq!(row(&opened.ledger, &ids[0])["viewed"], "acknowledged");
    }

    #[test]
    fn e8_flow_waits_for_narration_and_dwell() {
        let mut opened = open(&task_line("explain", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 0);
        opened.book.tick(&mut opened.ledger, 1200);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "narrationDone", 1200);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);

        let mut opened = open(&task_line("explain", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        opened.book.note_shown(&mut opened.ledger, &opened.phrase, 0);
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "narrationDone", 0);
        opened.book.tick(&mut opened.ledger, 1199);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        opened.book.tick(&mut opened.ledger, 1200);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
    }

    #[test]
    fn e9_flow_check_click_does_not_pause() {
        let mut opened = open(&task_line("explain", r#","controls":"check""#, r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        assert!(click(&mut opened.book, &mut opened.ledger, &ids[0], ClickTarget::Check, 10));
        assert_eq!(row(&opened.ledger, &ids[0])["viewed"], "acknowledged");
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "narrationDone", 10);
        opened.book.tick(&mut opened.ledger, 1199);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        opened.book.tick(&mut opened.ledger, 1200);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
    }

    #[test]
    fn e10_flow_approve_leaves_the_rest_null() {
        let mut opened = open(&task_line("approve", r#","pace":"flow""#, r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        assert!(click(&mut opened.book, &mut opened.ledger, &ids[0], ClickTarget::Yes, 10));
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        assert_eq!(row(&opened.ledger, &ids[0])["decision"], "approved");
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "narrationDone", 10);
        opened.book.tick(&mut opened.ledger, 1200);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "narrationDone", 1200);
        opened.book.tick(&mut opened.ledger, 2399);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        opened.book.tick(&mut opened.ledger, 2400);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "done");
        assert_eq!(row(&opened.ledger, &ids[0])["decision"], "approved");
        assert!(row(&opened.ledger, &ids[1])["decision"].is_null());
    }

    #[test]
    fn e11_pause_resume_skip_back() {
        let mut opened = open(&task_line("explain", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let before_pause = opened.ledger.events().len();
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Pause), 10));
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "paused");
        assert_eq!(opened.ledger.events().len(), before_pause);
        assert_eq!(opened.book.utterance_stops(), 0);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Resume), 20));
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        let kept = opened.ledger.events()[0].seq;
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 30));
        assert_eq!(opened.book.utterance_stops(), 1);
        assert_eq!(book_query(&opened.book, &opened.id)["stopUtterance"], true);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        let after_skip = opened.ledger.events().len();
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Back), 40));
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        assert!(opened.ledger.events().len() > after_skip);
        assert!(opened.ledger.events().iter().any(|ev| ev.seq == kept));
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Back), 50));
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
    }

    #[test]
    fn preempt_pauses_without_a_decision() {
        let mut opened = open(&explain_line());
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let before = opened.ledger.events().len();
        assert_eq!(signal(&mut opened.book, &mut opened.ledger, &opened.id, "preempt", 5).status, "ok");
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "paused");
        assert_eq!(opened.ledger.events().len(), before);
        assert!(row(&opened.ledger, &opened.phrase)["decision"].is_null());
    }

    fn place(book: &mut TaskBook, id: &str, x: i32, now: u64) {
        book.show_controls(id, word_at(x), 1920, 1080, now);
    }

    #[test]
    fn f1_click_binds_to_the_earlier_item() {
        let mut opened = open(&task_line("select", "", r#"{"query":"a"},{"query":"b"},{"query":"c"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        place(&mut opened.book, &opened.id, 200, 0);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 100));
        place(&mut opened.book, &opened.id, 200, 100);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 200));
        place(&mut opened.book, &opened.id, 200, 200);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 2);
        assert!(click(&mut opened.book, &mut opened.ledger, &ids[1], ClickTarget::Check, 250));
        assert_eq!(row(&opened.ledger, &ids[1])["viewed"], "acknowledged");
        assert_ne!(row(&opened.ledger, &ids[2])["viewed"], "acknowledged");
    }

    #[test]
    fn f2_grace_window_records_the_old_item() {
        let mut opened = open(&task_line("select", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        place(&mut opened.book, &opened.id, 200, 0);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 10));
        place(&mut opened.book, &opened.id, 200, 10);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert!(click(&mut opened.book, &mut opened.ledger, &ids[0], ClickTarget::Check, 10 + GRACE_MS - 1));
        assert_eq!(row(&opened.ledger, &ids[0])["viewed"], "acknowledged");
    }

    #[test]
    fn f3_click_after_grace_is_dropped() {
        let mut opened = open(&task_line("select", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        place(&mut opened.book, &opened.id, 200, 0);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 10));
        place(&mut opened.book, &opened.id, 200, 10);
        let before = opened.ledger.events().len();
        assert!(!click(&mut opened.book, &mut opened.ledger, &ids[0], ClickTarget::Check, 10 + GRACE_MS));
        assert_eq!(opened.ledger.events().len(), before);
        assert_ne!(row(&opened.ledger, &ids[0])["viewed"], "acknowledged");
    }

    #[test]
    fn f4_grace_control_is_the_upper_surface() {
        let mut opened = open(&task_line("select", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        place(&mut opened.book, &opened.id, 200, 0);
        let rect = opened.book.control_rects(&opened.id, word_at(200), 1920, 1080)[0].1;
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 10));
        place(&mut opened.book, &opened.id, 200, 10);
        let hit = opened.book.hit(rect.x, rect.y, 10).unwrap();
        assert_eq!(hit.0, ids[0]);
        assert_eq!(hit.1, ClickTarget::Check);
    }

    #[test]
    fn f5_rapid_double_click_is_one_event() {
        let mut opened = open(&task_line("select", "", r#"{"query":"the line"}"#));
        assert!(click(&mut opened.book, &mut opened.ledger, &opened.phrase, ClickTarget::Check, 1000));
        assert!(!click(&mut opened.book, &mut opened.ledger, &opened.phrase, ClickTarget::Check, 1010));
        let acks = opened.ledger.events().iter().filter(|ev| ev.viewed == Some(ViewedTier::Acknowledged)).count();
        assert_eq!(acks, 1);
        assert!(click(&mut opened.book, &mut opened.ledger, &opened.phrase, ClickTarget::Check, 1000 + DOUBLE_CLICK_MS));
        assert_eq!(opened.ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Cleared).count(), 1);
    }

    #[test]
    fn f6_pointer_down_keeps_the_control_phrase() {
        let mut opened = open(&task_line("select", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        place(&mut opened.book, &opened.id, 200, 0);
        let old = opened.book.control_rects(&opened.id, word_at(200), 1920, 1080)[0].1;
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 10));
        place(&mut opened.book, &opened.id, 800, 10);
        let new = opened.book.control_rects(&opened.id, word_at(800), 1920, 1080)[0].1;
        assert_eq!(opened.book.hit(old.x, old.y, 10).unwrap().0, ids[0]);
        assert_eq!(opened.book.hit(new.x, new.y, 10).unwrap().0, ids[1]);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
    }

    #[test]
    fn f7_click_on_the_advance_timestamp_binds_once() {
        let mut opened = open(&task_line("select", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        place(&mut opened.book, &opened.id, 200, 0);
        let rect = opened.book.control_rects(&opened.id, word_at(200), 1920, 1080)[0].1;
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 4000));
        place(&mut opened.book, &opened.id, 200, 4000);
        let hit = opened.book.hit(rect.x, rect.y, 4000).unwrap();
        assert_eq!(hit.0, ids[0]);
        assert!(click(&mut opened.book, &mut opened.ledger, &hit.0, ClickTarget::Check, 4000));
        assert_eq!(row(&opened.ledger, &ids[0])["viewed"], "acknowledged");
        assert_ne!(row(&opened.ledger, &ids[1])["viewed"], "acknowledged");
        assert_eq!(opened.ledger.events().iter().filter(|ev| ev.viewed == Some(ViewedTier::Acknowledged)).count(), 1);
    }

    #[test]
    fn resolve_item_rejects_the_sidecar_and_bad_bodies() {
        for field in [r#","approved":true"#, r#","viewed":"acknowledged""#] {
            let line = format!(
                r#"{{"kind":"resolveItem","proposalId":"r","taskId":"tk-1","phraseId":"ph-1","match":{{"status":"located","conf":0.5}}{field}}}"#
            );
            assert!(serde_json::from_str::<Proposal>(&line).is_err(), "{field}");
        }
        let mut opened = open(&task_line("explain", "", r#"{"query":"one"},{"query":"two"}"#));
        let before = opened.ledger.events().len();
        let denied = resolve(&mut opened.book, &mut opened.ledger, &opened.id, &opened.phrase, "located", "", Tier::T3Sidecar, 1);
        assert_eq!(denied.error.as_deref(), Some("not_allowed_for_tier:resolveItem"));
        assert_eq!(opened.ledger.events().len(), before);
        let bad = resolve(&mut opened.book, &mut opened.ledger, &opened.id, &opened.phrase, "located", r#","u":2,"v":0,"w":1,"h":1"#, Tier::T2Client, 2);
        assert_eq!(bad.error.as_deref(), Some("schema"));
        assert_eq!(opened.ledger.events().len(), before);
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 3);
        let miss = resolve(&mut opened.book, &mut opened.ledger, &opened.id, &opened.phrase, "notFound", "", Tier::T1User, 4);
        assert_eq!(miss.status, "ok");
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["items"][0]["match"], "notFound");
        assert_eq!(opened.ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Located).count(), 1);
        assert_eq!(opened.ledger.events().iter().find(|ev| ev.kind == LedgerKind::Located).unwrap().by, Tier::T1User);
        assert!(opened.ledger.events().iter().all(|ev| ev.kind != LedgerKind::Viewed));

        let mut waiting = open(&task_line("select", r#","itemTimeoutSeconds":5"#, r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut waiting.book, &mut waiting.ledger, &waiting.id, "begin", 0);
        let held = resolve(&mut waiting.book, &mut waiting.ledger, &waiting.id, &waiting.phrase, "notFound", "", Tier::T2Client, 1);
        assert_eq!(held.status, "ok");
        assert_eq!(book_query(&waiting.book, &waiting.id)["index"], 0);
        waiting.book.tick(&mut waiting.ledger, 4999);
        assert_eq!(book_query(&waiting.book, &waiting.id)["index"], 0);
        assert!(waiting.book.control_rects(&waiting.id, word_at(100), 1920, 1080).is_empty());

        let mut paused = open(&explain_line());
        signal(&mut paused.book, &mut paused.ledger, &paused.id, "begin", 0);
        assert!(paused.book.handle_ui(&mut paused.ledger, UiEvent::Hotkey(Hotkey::Pause), 1));
        let wrote = resolve(&mut paused.book, &mut paused.ledger, &paused.id, &paused.phrase, "located", "", Tier::T2Client, 2);
        assert_eq!(wrote.status, "ok");
        assert_eq!(book_query(&paused.book, &paused.id)["state"], "paused");
        assert_eq!(book_query(&paused.book, &paused.id)["index"], 0);
        assert!(paused.ledger.events().iter().all(|ev| ev.kind != LedgerKind::Viewed));
        paused.book.note_shown(&mut paused.ledger, &paused.phrase, 3);
        assert!(paused.ledger.events().iter().any(|ev| ev.viewed == Some(ViewedTier::Shown)));
    }

    #[test]
    fn teaching_highlight_without_a_task_writes_no_phrase() {
        let mut reg = Registry::new();
        reg.spawn_presence("p", "ctx", None, None);
        reg.note_task_highlight("p", 5);
        assert!(reg.ledger().events().is_empty());
        assert_eq!(reg.ledger().occupancy(), 0);
    }

    fn scroll_line(tier_items: &str) -> String {
        format!(r#"{{"kind":"createTask","proposalId":"c","presenceId":"p","mode":"explain","docKey":"doc-a","items":[{tier_items}]}}"#)
    }

    fn web_find(doc: &str, phrase: &str) -> ViewerMove {
        ViewerMove::WebFind { doc_key: doc.into(), opening_phrase: phrase.into() }
    }

    #[test]
    fn g1_move_without_an_active_tour_is_rejected() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let missing = book.authorize("tk-none", &web_find("doc-a", "the line"), true, &mut ledger, 0);
        assert_eq!(missing, Err(MoveError::NoActiveTour));
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(book_query(&book, &id)["state"], "pending");
        assert_eq!(book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 0), Err(MoveError::NoActiveTour));
        let drafted = create(
            &mut book,
            &mut ledger,
            r#"{"kind":"draftTask","proposalId":"d","presenceId":"p","mode":"explain","docKey":"doc-a","items":[{"query":"the line","scroll":true}]}"#,
            Tier::T3Sidecar,
            0,
        );
        let draft = drafted.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(book_query(&book, &draft)["state"], "draft");
        assert_eq!(book.authorize(&draft, &web_find("doc-a", "the line"), true, &mut ledger, 0), Err(MoveError::NoActiveTour));
        assert_eq!(book.moves_issued(), 0);
        assert!(ledger.events().is_empty());
    }

    #[test]
    fn g2_move_for_a_different_doc_is_rejected() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        let wrong = book.authorize(&id, &web_find("other-doc", "the line"), true, &mut ledger, 0);
        assert_eq!(wrong, Err(MoveError::DocMismatch));
        assert_eq!(book.moves_issued(), 0);
    }

    #[test]
    fn g3_t2_scroll_waits_for_start() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T2Client, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(book_query(&book, &id)["state"], "awaitingStart");
        assert!(book.admit().is_none());
        assert_eq!(book.planned_move(&id), Err(MoveError::NeedsStartConfirm));
        assert_eq!(book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 0), Err(MoveError::NeedsStartConfirm));
        assert_eq!(book.moves_issued(), 0);
        assert!(book.control_rects(&id, word_at(100), 1920, 1080).is_empty());
    }

    #[test]
    fn g4_start_hands_the_runner_web_find_or_pdf_page() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T2Client, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert!(book.handle_ui(&mut ledger, UiEvent::Hotkey(Hotkey::Start), 1));
        assert_eq!(book_query(&book, &id)["state"], "active");
        assert_eq!(book.planned_move(&id).unwrap(), web_find("doc-a", "the line"));
        assert_eq!(viewer_move_slot(&book.planned_move(&id).unwrap()), 0);

        let pdf = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the page","scroll":true,"page":4}"#), Tier::T2Client, 2);
        let pdf_id = pdf.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.complete(&id).unwrap();
        assert!(book.handle_ui(&mut ledger, UiEvent::Hotkey(Hotkey::Start), 3));
        assert_eq!(book.planned_move(&pdf_id).unwrap(), ViewerMove::PdfPage { doc_key: "doc-a".into(), page: 4 });
        assert_eq!(viewer_move_slot(&book.planned_move(&pdf_id).unwrap()), 1);
    }

    #[test]
    fn g5_t1_does_not_wait_for_start() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let plain = create(&mut book, &mut ledger, &explain_line(), Tier::T2Client, 0);
        assert_eq!(plain.detail.unwrap()["state"], "pending");
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(book_query(&book, &id)["state"], "pending");
        book.admit().unwrap();
        assert_eq!(book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 0), Ok(()));
        assert_eq!(book.moves_issued(), 1);
    }

    fn preempt(book: &mut TaskBook, ledger: &mut Ledger, id: &str, reason: &str) {
        let line = format!(r#"{{"kind":"tourSignal","proposalId":"t","taskId":"{id}","signal":"preempt","reason":"{reason}"}}"#);
        let result = route_task(&parse(&line), Tier::T2Client, book, ledger, true, 10).unwrap();
        assert_eq!(result.status, "ok");
    }

    #[test]
    fn g6_g7_page_and_pointer_pause_and_issue_no_further_move() {
        for reason in ["page", "pointer"] {
            let mut book = TaskBook::new();
            let mut ledger = Ledger::new();
            let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
            let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
            book.admit().unwrap();
            book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 0).unwrap();
            let before = ledger.events().len();
            preempt(&mut book, &mut ledger, &id, reason);
            assert_eq!(book_query(&book, &id)["state"], "paused");
            assert_eq!(ledger.events().len(), before);
            assert_eq!(book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 2_000), Err(MoveError::NoActiveTour));
            assert_eq!(book.moves_issued(), 1);
            assert!(ledger.events().iter().all(|ev| !matches!(ev.kind, LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Cleared)));
        }
    }

    #[test]
    fn g8_ordinary_keys_do_not_pause_and_no_grab_is_installed() {
        assert!(!keyboard_grab_installed());
        let overlay = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/overlay.rs")).unwrap();
        assert!(!overlay.contains("get_keyboard"));
        let keys = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui_input.rs")).unwrap();
        let start = keys.find("pub enum Hotkey").unwrap();
        let body = &keys[start..keys[start..].find('}').unwrap() + start];
        assert!(!body.contains("Ordinary"));
        assert!(body.contains("Skip"));
        assert!(body.contains("Pause"));
        let mut opened = open(&task_line("explain", "", r#"{"query":"one"},{"query":"two"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 1));
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Pause), 2));
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "paused");
    }

    #[test]
    fn g9_a_failed_find_does_not_nudge() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(
            &mut book,
            &mut ledger,
            &task_line("explain", r#","pace":"flow","docKey":"doc-a""#, r#"{"query":"one","scroll":true},{"query":"two","scroll":true}"#),
            Tier::T1User,
            0,
        );
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        signal(&mut book, &mut ledger, &id, "begin", 0);
        let nudge = ViewerMove::Nudge { dy_px: NUDGE_PX };
        assert_eq!(book.authorize(&id, &nudge, false, &mut ledger, 1), Err(MoveError::OffGlass));
        assert_eq!(book.moves_issued(), 0);
        let ids = item_ids(&book, &id);
        let miss = resolve(&mut book, &mut ledger, &id, &ids[0], "notFound", "", Tier::T2Client, 2);
        assert_eq!(miss.status, "ok");
        assert_eq!(book_query(&book, &id)["index"], 1);
        assert_eq!(ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Located).count(), 1);

        let mut waiting_book = TaskBook::new();
        let mut waiting_ledger = Ledger::new();
        let waiting = create(
            &mut waiting_book,
            &mut waiting_ledger,
            &task_line("select", r#","docKey":"doc-a","itemTimeoutSeconds":5"#, r#"{"query":"one","scroll":true},{"query":"two","scroll":true}"#),
            Tier::T1User,
            0,
        );
        let wait_id = waiting.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        waiting_book.admit().unwrap();
        signal(&mut waiting_book, &mut waiting_ledger, &wait_id, "begin", 0);
        assert_eq!(waiting_book.authorize(&wait_id, &nudge, false, &mut waiting_ledger, 1), Err(MoveError::OffGlass));
        let phrase = item_ids(&waiting_book, &wait_id)[0].clone();
        resolve(&mut waiting_book, &mut waiting_ledger, &wait_id, &phrase, "notFound", "", Tier::T2Client, 2);
        assert_eq!(book_query(&waiting_book, &wait_id)["index"], 0);
        waiting_book.tick(&mut waiting_ledger, 4999);
        assert_eq!(book_query(&waiting_book, &wait_id)["index"], 0);
    }

    #[test]
    fn g10_locate_failure_after_a_move_is_one_miss() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(
            &mut book,
            &mut ledger,
            &task_line("explain", r#","docKey":"doc-a""#, r#"{"query":"one","scroll":true},{"query":"two","scroll":true}"#),
            Tier::T1User,
            0,
        );
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        signal(&mut book, &mut ledger, &id, "begin", 0);
        book.authorize(&id, &web_find("doc-a", "one"), true, &mut ledger, 1).unwrap();
        let phrase = item_ids(&book, &id)[0].clone();
        resolve(&mut book, &mut ledger, &id, &phrase, "notFound", "", Tier::T2Client, 2);
        assert_eq!(ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Located).count(), 1);
        assert_eq!(book_query(&book, &id)["index"], 1);
        book.tick(&mut ledger, 5_000);
        assert_eq!(book.moves_issued(), 1);
        assert_eq!(ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Located).count(), 1);
    }

    #[test]
    fn g11_viewer_move_has_three_variants_and_is_not_a_socket_kind() {
        let samples = [
            web_find("doc-a", "the line"),
            ViewerMove::PdfPage { doc_key: "doc-a".into(), page: 2 },
            ViewerMove::Nudge { dy_px: NUDGE_PX },
        ];
        assert_eq!(samples.iter().map(viewer_move_slot).collect::<Vec<_>>(), vec![0, 1, 2]);
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/task.rs")).unwrap();
        let start = src.find("pub enum ViewerMove").unwrap();
        let end = src[start..].find('}').unwrap();
        assert!(!src[start..start + end].contains("Deserialize"));
        assert!(serde_json::from_str::<Proposal>(r#"{"kind":"viewerMove","proposalId":"v"}"#).is_err());
    }

    #[test]
    fn g12_twelve_items_is_the_tour_cap() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let dozen = (0..12).map(|n| format!(r#"{{"query":"q{n}","scroll":true}}"#)).collect::<Vec<_>>().join(",");
        let ok = create(
            &mut book,
            &mut ledger,
            &format!(r#"{{"kind":"createTask","proposalId":"c","presenceId":"p","mode":"explain","docKey":"doc-a","items":[{dozen}]}}"#),
            Tier::T2Client,
            0,
        );
        assert_eq!(ok.status, "ok");
        assert_eq!(ok.detail.unwrap()["items"].as_array().unwrap().len(), 12);
        let extra = (0..13).map(|n| format!(r#"{{"query":"e{n}"}}"#)).collect::<Vec<_>>().join(",");
        let rejected = create(
            &mut book,
            &mut ledger,
            &format!(r#"{{"kind":"createTask","proposalId":"c2","presenceId":"p","mode":"explain","items":[{extra}]}}"#),
            Tier::T2Client,
            0,
        );
        assert_eq!(rejected.error.as_deref(), Some("schema"));
        assert_eq!(book.len(), 1);
    }

    #[test]
    fn g13_moves_do_not_outrun_the_locate_wait() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        let mv = web_find("doc-a", "the line");
        assert_eq!(book.authorize(&id, &mv, true, &mut ledger, 0), Ok(()));
        assert_eq!(book.authorize(&id, &mv, true, &mut ledger, MOVE_WAIT_MS - 1), Err(MoveError::TooSoon));
        assert_eq!(book.moves_issued(), 1);
        assert_eq!(book.authorize(&id, &mv, true, &mut ledger, MOVE_WAIT_MS), Ok(()));
        assert_eq!(book.moves_issued(), 2);
    }

    #[test]
    fn g14_nudge_off_the_glass_is_rejected() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        let nudge = ViewerMove::Nudge { dy_px: NUDGE_PX };
        assert_eq!(book.authorize(&id, &nudge, false, &mut ledger, 0), Err(MoveError::OffGlass));
        assert_eq!(book.moves_issued(), 0);
        assert_eq!(book.authorize(&id, &nudge, true, &mut ledger, 0), Ok(()));
        assert_eq!(book.moves_issued(), 1);
    }

    #[test]
    fn three_attempts_then_one_miss_and_no_loop() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(
            &mut book,
            &mut ledger,
            &task_line("explain", r#","docKey":"doc-a""#, r#"{"query":"one","scroll":true},{"query":"two","scroll":true}"#),
            Tier::T1User,
            0,
        );
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        signal(&mut book, &mut ledger, &id, "begin", 0);
        let mv = web_find("doc-a", "one");
        for n in 0u64..3 {
            book.authorize(&id, &mv, true, &mut ledger, n * MOVE_WAIT_MS).unwrap();
        }
        let err = book.authorize(&id, &mv, true, &mut ledger, 3 * MOVE_WAIT_MS);
        assert_eq!(err, Err(MoveError::AttemptsExhausted));
        assert_eq!(ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Located).count(), 1);
        assert_eq!(book_query(&book, &id)["index"], 1);
        assert_eq!(book.moves_issued(), 3);
        book.tick(&mut ledger, 10_000);
        assert_eq!(ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Located).count(), 1);
    }
}
