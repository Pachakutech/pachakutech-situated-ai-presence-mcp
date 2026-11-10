//! Task book for one daemon run. In memory, empty at start, not kept across
//! a restart. The highlight is the control. The squares beside the words stay
//! unpainted.
//!
//! Clicks and hotkeys enter the ledger only through [`crate::ui_input::apply_ui_event`].

use crate::gate::Tier;
use crate::highlight::MarkColor;
use crate::ledger::{Ledger, LedgerError, LedgerKind, ObservedAppend, PHRASE_CAP};
use crate::protocol::{
    ClientControls, CreateTaskBody, ListPhraseFlagsBody, MatchInfo, MatchStatus, ObservedViewed, Proposal, ProposalResult,
    PreemptReason, ResolveItemBody, TaskItemBody, TaskMode, TaskPace, TourSignalBody, TourSignalKind, NARRATION_CAP,
};
use crate::ui_input::{apply_ui_event, ClickTarget, Hotkey, UiEvent};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const QUEUE_CAP: usize = 5;
pub const ITEM_CAP: usize = 12;
pub const DEFAULT_TTL_S: u32 = 120;
pub const DEFAULT_ITEM_TIMEOUT_S: u32 = 20;
pub const DWELL_MS: u64 = 1200;
pub const GRACE_MS: u64 = 1500;
pub const DOUBLE_CLICK_MS: u64 = 250;
/// Hold at least this long, measured on the daemon clock, and the release is a long tap.
pub const LONG_TAP_MS: u64 = 500;
/// evdev `BTN_LEFT`.
pub const BTN_LEFT: u32 = 0x110;
pub const MOVE_WAIT_MS: u64 = 1500;
pub const MOVE_ATTEMPTS: u8 = 3;
pub const NUDGE_PX: i32 = 80;

/// One chunk of a project tour. Built by the tour, inserted here.
pub struct TourSeed {
    pub origin_user: bool,
    pub mode: crate::protocol::TaskMode,
    pub pace: crate::protocol::TaskPace,
    pub doc_key: Option<String>,
    pub items: Vec<TourSeedItem>,
    /// Locus ids in this chunk, parallel to `items`. Each item's phrase id.
    pub locus_ids: Vec<String>,
    /// After the list, one more pass over skips that are still on the glass.
    pub again: bool,
}

/// One pointer sample, stamped with the daemon clock when the compositor delivered it.
#[derive(Clone, Debug)]
pub struct TintMark {
    pub id: String,
    pub generation: u64,
}

#[derive(Clone, Debug)]
pub struct TintSample {
    pub down: bool,
    pub button: u32,
    pub ts_ms: u64,
    /// Present when the pointer is on a mark. Absent on a release outside every mark.
    pub mark: Option<TintMark>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tap {
    Short,
    Long,
}

/// Release on the same generation. A different generation, or no generation, is no gesture.
pub fn classify_tap(
    press_button: u32,
    release_button: u32,
    press_ms: u64,
    release_ms: u64,
    press_generation: u64,
    release_generation: Option<u64>,
) -> Option<Tap> {
    if press_button != BTN_LEFT || release_button != BTN_LEFT {
        return None;
    }
    if release_generation != Some(press_generation) {
        return None;
    }
    if release_ms.saturating_sub(press_ms) >= LONG_TAP_MS {
        Some(Tap::Long)
    } else {
        Some(Tap::Short)
    }
}

pub fn switch_tint(from: MarkColor, tap: Tap) -> (MarkColor, LedgerKind) {
    match (from, tap) {
        (MarkColor::Yellow, Tap::Short) => (MarkColor::Green, LedgerKind::Accept),
        (MarkColor::Green, Tap::Short) => (MarkColor::Yellow, LedgerKind::Skip),
        (MarkColor::Yellow | MarkColor::Green, Tap::Long) => (MarkColor::Red, LedgerKind::Deny),
        (MarkColor::Red, Tap::Short | Tap::Long) => (MarkColor::Yellow, LedgerKind::Skip),
    }
}

/// No hand row, and a skip row, both read as skip.
pub fn hand_condition(ledger: &Ledger, phrase_id: &str) -> &'static str {
    match ledger.latest_hand(phrase_id) {
        Some(LedgerKind::Accept) => "accept",
        Some(LedgerKind::Deny) => "deny",
        _ => "skip",
    }
}

pub struct TourSeedItem {
    pub query: String,
    pub narration: String,
    pub page: Option<u32>,
}
pub const CONTROL_PX: i32 = 28;
pub const CONTROL_GAP: i32 = 8;
pub const PAIR_GAP: i32 = 4;
const PROMPT_CAP: usize = 120;

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
    /// Shown once during the after-list pass. A later pass does not return to it.
    continued: bool,
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
    /// The clip finished. A narrated citation does not leave because of this.
    narration_done: bool,
    last_move_ms: Option<u64>,
    /// Their scroll. Further moves are refused. The task stays active and the voice is not stopped.
    yield_scroll: bool,
    /// The next nudge is refused once.
    hold_nudge: bool,
    again: bool,
    /// The driver may place a skip from the glass only. It does not find or nudge.
    pass_only: bool,
    say_seq: u64,
    say_open: bool,
    /// The locus a re-entry tap left. A second tap inside the grace rewrites the hand.
    left_phrase: Option<String>,
    left_until_ms: u64,
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
    /// Their scroll, or the one nudge a pointer withheld.
    Yielded,
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
    /// Presence whose highlight is showing this phrase. Empty when a test notes the phrase directly.
    mark_id: String,
}

struct TintPress {
    mark_id: String,
    generation: u64,
    button: u32,
    ts_ms: u64,
    phrase_id: String,
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
    pointer: Option<(i32, i32)>,
    tint_press: Option<TintPress>,
    /// Color cached when the highlight was noted and when a gesture completed.
    tints: HashMap<String, MarkColor>,
    /// Locus marks to take down after a re-entry tap.
    dropped: Vec<String>,
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
            pointer: None,
            tint_press: None,
            tints: HashMap::new(),
            dropped: Vec::new(),
        }
    }

    pub fn utterance_stops(&self) -> u64 {
        self.utterance_stops
    }

    pub fn moves_issued(&self) -> u64 {
        self.moves_issued
    }

    pub fn has_awaiting_start(&self) -> bool {
        self.tasks.iter().any(|task| task.state == TaskState::AwaitingStart)
    }

    /// A project-tour chunk. Scroll items stay `pending`. This path does not use the T2 start-card rule.
    pub fn insert_tour_chunk(&mut self, ledger: &mut Ledger, seed: TourSeed, now_ms: u64) -> Result<String, TaskError> {
        if seed.items.is_empty() || seed.items.len() > ITEM_CAP || seed.locus_ids.len() != seed.items.len() {
            return Err(TaskError::Schema);
        }
        if seed.locus_ids.iter().any(|id| id.trim().is_empty()) {
            return Err(TaskError::Schema);
        }
        if self.runnable_count() >= QUEUE_CAP {
            return Err(TaskError::QueueFull);
        }
        let drawn = resolve_presentation(seed.mode, None)?;
        let fresh = seed.locus_ids.iter().filter(|id| !ledger.holds(id)).count();
        if ledger.occupancy() + fresh > PHRASE_CAP {
            return Err(TaskError::Quota);
        }
        for id in &seed.locus_ids {
            ledger.hold_phrase(id).map_err(ledger_err)?;
        }
        let mut items = Vec::with_capacity(seed.items.len());
        for (raw, phrase_id) in seed.items.iter().zip(seed.locus_ids.iter()) {
            items.push(Item {
                phrase_id: phrase_id.clone(),
                query: Some(cut(&raw.query, crate::ledger::TEXT_CAP)),
                narration: cut(&raw.narration, NARRATION_CAP),
                scroll: true,
                page: raw.page,
                matched: ItemMatch::Pending,
                attempts: 0,
                continued: false,
            });
        }
        let id = self.mint_task_id();
        self.tasks.push(Task {
            id: id.clone(),
            presence_id: "project-tour".into(),
            origin: if seed.origin_user { Origin::User } else { Origin::Agent },
            mode: seed.mode,
            pace: seed.pace,
            drawn,
            state: TaskState::Pending,
            prompt: String::new(),
            ttl_seconds: DEFAULT_TTL_S,
            item_timeout_seconds: DEFAULT_ITEM_TIMEOUT_S,
            deadline_ms: now_ms.saturating_add(u64::from(DEFAULT_TTL_S) * 1000),
            doc_key: seed.doc_key,
            items,
            index: 0,
            entered_ms: None,
            narration_done: false,
            last_move_ms: None,
            yield_scroll: false,
            hold_nudge: false,
            again: seed.again,
            pass_only: false,
            say_seq: 0,
            say_open: false,
            left_phrase: None,
            left_until_ms: 0,
        });
        Ok(id)
    }

    /// Ends the per-source task. Writes no phrase decision.
    pub fn end_without_decision(&mut self, task_id: &str) -> bool {
        let Some(pos) = self.tasks.iter().position(|task| task.id == task_id) else {
            return false;
        };
        if !open_for_end(self.tasks[pos].state) {
            return false;
        }
        self.tasks[pos].state = TaskState::Dismissed;
        self.clear_shown_for(task_id);
        true
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
        if self.tasks[pos].yield_scroll {
            return Err(MoveError::Yielded);
        }
        if self.tasks[pos].hold_nudge && matches!(mv, ViewerMove::Nudge { .. }) {
            self.tasks[pos].hold_nudge = false;
            return Err(MoveError::Yielded);
        }
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
    /// A shown phrase keeps `located` when a later resolve says `notFound`.
    pub fn note_match(&mut self, task_id: &str, phrase_id: &str, matched: ItemMatch) -> Result<(), TaskError> {
        let shown = self.shown.as_ref().is_some_and(|mark| mark.phrase_id == phrase_id);
        let task = self.task_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        let item = task.items.iter_mut().find(|item| item.phrase_id == phrase_id).ok_or(TaskError::Schema)?;
        if matched == ItemMatch::NotFound && item.matched == ItemMatch::Located && shown {
            return Ok(());
        }
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
            UiEvent::Hotkey(Hotkey::Note) => self.hotkey_note(ledger, now_ms),
            // Project confirm is applied on the project store. Start still promotes a task.
            // Source keys are applied by the project tour, which also ends the per-source task.
            UiEvent::Hotkey(Hotkey::ConfirmProject) | UiEvent::Hotkey(Hotkey::NextSource) | UiEvent::Hotkey(Hotkey::PrevSource) => false,
            UiEvent::ArmNote { .. }
            | UiEvent::Acknowledge { .. }
            | UiEvent::Uncheck { .. }
            | UiEvent::Approve { .. }
            | UiEvent::Discard { .. }
            | UiEvent::Skip { .. }
            | UiEvent::Accept { .. }
            | UiEvent::Deny { .. } => false,
        }
    }

    pub fn mark_colors(&self) -> Vec<(String, MarkColor)> {
        self.tints.iter().map(|(id, color)| (id.clone(), *color)).collect()
    }

    /// Pointer samples already stamped. A finished gesture appends one hand row.
    pub fn apply_tint_samples(&mut self, ledger: &mut Ledger, samples: Vec<TintSample>) -> Vec<String> {
        for sample in samples {
            self.apply_tint_sample(ledger, sample);
        }
        std::mem::take(&mut self.dropped)
    }

    fn apply_tint_sample(&mut self, ledger: &mut Ledger, sample: TintSample) {
        if sample.down {
            let Some(mark) = sample.mark else {
                self.tint_press = None;
                return;
            };
            let Some(phrase) = self.phrase_of_mark(&mark.id) else {
                self.tint_press = None;
                return;
            };
            self.tint_press = Some(TintPress {
                mark_id: mark.id,
                generation: mark.generation,
                button: sample.button,
                ts_ms: sample.ts_ms,
                phrase_id: phrase,
            });
            return;
        }
        let Some(press) = self.tint_press.take() else {
            return;
        };
        let release_generation = sample.mark.as_ref().filter(|mark| mark.id == press.mark_id).map(|mark| mark.generation);
        let Some(tap) = classify_tap(press.button, sample.button, press.ts_ms, sample.ts_ms, press.generation, release_generation) else {
            return;
        };
        let from = self.tints.get(&press.mark_id).copied().unwrap_or(MarkColor::Yellow);
        let (color, kind) = switch_tint(from, tap);
        if self.is_reentry(&press.phrase_id) {
            self.reenter(ledger, press.phrase_id, press.mark_id, color, kind, sample.ts_ms);
            return;
        }
        let still = self.shown.as_ref().is_some_and(|shown| shown.phrase_id == press.phrase_id);
        if !still {
            return;
        }
        let event = match kind {
            LedgerKind::Accept => UiEvent::Accept { phrase_id: press.phrase_id.clone() },
            LedgerKind::Deny => UiEvent::Deny { phrase_id: press.phrase_id.clone() },
            LedgerKind::Skip => UiEvent::Skip { phrase_id: press.phrase_id.clone() },
            _ => return,
        };
        if apply_ui_event(ledger, event, sample.ts_ms).is_err() {
            return;
        }
        self.tints.insert(press.mark_id, color);
        self.locate_shown(&press.phrase_id);
        self.consider(ledger, sample.ts_ms);
    }

    fn current_phrase(&self) -> Option<String> {
        self.tasks
            .iter()
            .find(|task| live(task.state))
            .and_then(|task| task.items.get(task.index).map(|item| item.phrase_id.clone()))
    }

    fn phrase_of_mark(&self, mark_id: &str) -> Option<String> {
        if let Some(phrase) = self.tasks.iter().find(|task| live(task.state)).and_then(|task| {
            task.items.iter().find(|item| item.phrase_id == mark_id).map(|item| item.phrase_id.clone())
        }) {
            return Some(phrase);
        }
        let shown = self.shown.as_ref()?;
        if shown.mark_id == mark_id {
            return Some(shown.phrase_id.clone());
        }
        None
    }

    fn is_reentry(&self, phrase_id: &str) -> bool {
        self.current_phrase().as_deref() != Some(phrase_id)
            && self.tasks.iter().any(|task| live(task.state) && task.items.iter().any(|item| item.phrase_id == phrase_id))
    }

    fn reenter(
        &mut self,
        ledger: &mut Ledger,
        phrase_id: String,
        mark_id: String,
        color: MarkColor,
        kind: LedgerKind,
        now_ms: u64,
    ) {
        let Some(pos) = self.tasks.iter().position(|task| {
            live(task.state) && task.items.iter().any(|item| item.phrase_id == phrase_id)
        }) else {
            return;
        };
        let grace = self.tasks[pos].left_phrase.as_deref() == Some(phrase_id.as_str()) && now_ms < self.tasks[pos].left_until_ms;
        if !grace {
            if let Some(index) = self.tasks[pos].items.iter().position(|item| item.phrase_id == phrase_id) {
                if self.tasks[pos].index != index {
                    self.tasks[pos].index = index;
                    self.enter_at(pos, ledger, now_ms);
                }
            }
        }
        let event = match kind {
            LedgerKind::Accept => UiEvent::Accept { phrase_id: phrase_id.clone() },
            LedgerKind::Deny => UiEvent::Deny { phrase_id: phrase_id.clone() },
            LedgerKind::Skip => UiEvent::Skip { phrase_id: phrase_id.clone() },
            _ => return,
        };
        if apply_ui_event(ledger, event, now_ms).is_err() {
            return;
        }
        self.tints.insert(mark_id, color);
        self.tasks[pos].say_seq = self.tasks[pos].say_seq.saturating_add(1);
        self.tasks[pos].say_open = true;
        if grace {
            return;
        }
        self.tasks[pos].left_phrase = Some(phrase_id.clone());
        self.tasks[pos].left_until_ms = now_ms.saturating_add(GRACE_MS);
        self.dropped.push(phrase_id);
        self.advance_active(ledger, now_ms);
    }

    /// A locus mark. The current item becomes the shown phrase. Another locus only takes its color.
    pub fn note_phrase_mark(&mut self, phrase_id: &str, ledger: &mut Ledger, now_ms: u64) {
        self.remember_tint(phrase_id, ledger, phrase_id);
        if self.current_phrase().as_deref() == Some(phrase_id) {
            self.locate_shown(phrase_id);
            self.note_shown_on(ledger, phrase_id, phrase_id, now_ms);
        }
    }

    pub fn note_highlight(&mut self, presence_id: &str, ledger: &mut Ledger, now_ms: u64) {
        let phrase = self.phrase_for_mark(presence_id);
        if let Some(phrase) = phrase {
            self.locate_shown(&phrase);
            self.note_shown_on(ledger, &phrase, presence_id, now_ms);
            self.remember_tint(presence_id, ledger, &phrase);
        }
    }

    /// The current phrase is on the glass. `notFound` and a match still pending become `located`.
    /// `ambiguous` stays a miss.
    fn locate_shown(&mut self, phrase_id: &str) {
        if self.current_phrase().as_deref() != Some(phrase_id) {
            return;
        }
        let Some(task) = self.tasks.iter_mut().find(|task| live(task.state)) else { return };
        let Some(item) = task.items.get_mut(task.index) else { return };
        if item.phrase_id != phrase_id {
            return;
        }
        if matches!(item.matched, ItemMatch::Pending | ItemMatch::NotFound) {
            item.matched = ItemMatch::Located;
        }
    }

    /// A live task on this presence owns the mark. Otherwise an active project tour does.
    fn phrase_for_mark(&self, presence_id: &str) -> Option<String> {
        if let Some(phrase) = self
            .tasks
            .iter()
            .find(|task| task.presence_id == presence_id && live(task.state))
            .and_then(|task| task.items.get(task.index).map(|item| item.phrase_id.clone()))
        {
            return Some(phrase);
        }
        self.tasks
            .iter()
            .find(|task| task.presence_id == "project-tour" && live(task.state))
            .and_then(|task| task.items.get(task.index).map(|item| item.phrase_id.clone()))
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
        self.note_shown_on(ledger, phrase_id, "", now_ms);
    }

    fn note_shown_on(&mut self, ledger: &mut Ledger, phrase_id: &str, mark_id: &str, now_ms: u64) {
        if !self.phrase_is_live(phrase_id) {
            return;
        }
        if viewed_name(ledger, phrase_id) == "acknowledged" {
            self.retarget_shown(phrase_id, mark_id);
            return;
        }
        if self.shown.as_ref().is_some_and(|mark| mark.phrase_id == phrase_id) {
            self.retarget_shown(phrase_id, mark_id);
            return;
        }
        self.tint_press = None;
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
        self.shown = Some(ShownMark {
            phrase_id: phrase_id.to_string(),
            at_ms: now_ms,
            dwelled: false,
            mark_id: mark_id.to_string(),
        });
    }

    fn retarget_shown(&mut self, phrase_id: &str, mark_id: &str) {
        if mark_id.is_empty() {
            return;
        }
        if let Some(shown) = &mut self.shown {
            if shown.phrase_id == phrase_id {
                shown.mark_id = mark_id.to_string();
            }
        }
    }

    fn remember_tint(&mut self, mark_id: &str, ledger: &Ledger, phrase_id: &str) {
        if mark_id.is_empty() {
            return;
        }
        let color = match ledger.latest_hand(phrase_id) {
            Some(LedgerKind::Accept) => MarkColor::Green,
            Some(LedgerKind::Deny) => MarkColor::Red,
            _ => MarkColor::Yellow,
        };
        self.tints.insert(mark_id.to_string(), color);
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
        let placed = place_controls(word, out_w, out_h, drawn);
        let note = place_note_button(out_w, out_h, &placed);
        for (target, rect) in placed.into_iter().chain(note) {
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
        self.pointer = Some((x, y));
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
            ClickTarget::Note => {
                if apply_ui_event(ledger, UiEvent::ArmNote { phrase_id: phrase_id.to_string() }, now_ms).is_err() {
                    return false;
                }
                self.last_click = Some((ts_ms, phrase_id.to_string(), target));
                return true;
            }
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
            (DrawnControls::Check | DrawnControls::YesNo, ClickTarget::Note) => true,
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
            (
                task.pace,
                task.mode,
                entered,
                task.narration_done,
                task.item_timeout_seconds,
                item.phrase_id.clone(),
                item.matched,
                item.narration.clone(),
            )
        };
        let (pace, mode, entered, narration_done, timeout_s, phrase, matched, narration) = snapshot;
        let window = u64::from(timeout_s).saturating_mul(1000);
        let narrated = !narration.trim().is_empty();
        // A narrated citation stays until a hand. The item clock is for a list with no cue.
        let timed_out = !narrated && now_ms >= entered.saturating_add(window);
        let miss = hides_control(matched);
        let advance = match pace {
            TaskPace::Wait => timed_out || (!miss && item_answered(ledger, mode, &phrase)),
            TaskPace::Flow if narrated => !miss && item_answered(ledger, mode, &phrase),
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
        if self.tasks[pos].pass_only {
            let idx = self.tasks[pos].index;
            self.tasks[pos].items[idx].continued = true;
            if self.continue_skips(pos, ledger, now_ms) {
                return;
            }
            self.finish_task(pos, &task_id);
            return;
        }
        if self.tasks[pos].index + 1 >= self.tasks[pos].items.len() {
            if self.tasks[pos].again && self.continue_skips(pos, ledger, now_ms) {
                return;
            }
            self.finish_task(pos, &task_id);
            return;
        }
        self.tasks[pos].index += 1;
        self.enter_at(pos, ledger, now_ms);
    }

    /// The next skip that this pass has not already shown. The scroll stays yielded.
    fn continue_skips(&mut self, pos: usize, ledger: &mut Ledger, now_ms: u64) -> bool {
        let Some(skip_at) = self.tasks[pos].items.iter().position(|item| {
            !item.continued && hand_condition(ledger, &item.phrase_id) == "skip"
        }) else {
            return false;
        };
        self.tasks[pos].index = skip_at;
        self.enter_at(pos, ledger, now_ms);
        self.tasks[pos].pass_only = true;
        self.tasks[pos].yield_scroll = true;
        true
    }

    fn finish_task(&mut self, pos: usize, task_id: &str) {
        self.tasks[pos].state = TaskState::Done;
        self.tasks[pos].entered_ms = None;
        self.tasks[pos].narration_done = false;
        self.tasks[pos].pass_only = false;
        self.clear_shown_for(task_id);
    }

    fn enter_at(&mut self, pos: usize, ledger: &mut Ledger, now_ms: u64) {
        self.tasks[pos].entered_ms = Some(now_ms);
        self.tasks[pos].narration_done = false;
        self.tasks[pos].yield_scroll = false;
        self.tasks[pos].hold_nudge = false;
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

    fn preempt_task(&mut self, task_id: &str, reason: Option<PreemptReason>) -> Result<(), TaskError> {
        let task = self.task_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.state != TaskState::Active {
            return Err(TaskError::TaskNotActive);
        }
        match reason {
            Some(PreemptReason::Pointer) => task.hold_nudge = true,
            Some(PreemptReason::Page) | None => task.yield_scroll = true,
        }
        Ok(())
    }

    /// The glass pass found nothing left to show. No hand is written.
    fn settle_task(&mut self, task_id: &str) -> Result<(), TaskError> {
        let task = self.task_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.state != TaskState::Active {
            return Err(TaskError::TaskNotActive);
        }
        task.state = TaskState::Done;
        task.pass_only = false;
        task.yield_scroll = false;
        task.entered_ms = None;
        Ok(())
    }

    fn hotkey_note(&mut self, ledger: &mut Ledger, now_ms: u64) -> bool {
        let point = self.pointer;
        if let Some((x, y)) = point {
            if let Some((phrase_id, _)) = self.hit(x, y, now_ms) {
                return apply_ui_event(ledger, UiEvent::ArmNote { phrase_id }, now_ms).is_ok();
            }
        }
        let Some(phrase) = self.tasks.iter().find(|task| live(task.state)).and_then(|task| {
            task.items.get(task.index).map(|item| item.phrase_id.clone())
        }) else {
            return false;
        };
        apply_ui_event(ledger, UiEvent::ArmNote { phrase_id: phrase }, now_ms).is_ok()
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
    match ledger.latest_hand(phrase_id) {
        Some(LedgerKind::Accept | LedgerKind::Deny) => return true,
        Some(LedgerKind::Skip) | None => {}
        Some(_) => {}
    }
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

/// The note square sits further left of the leftmost control already placed.
/// `control_rects` does not include it.
pub fn place_note_button(out_w: i32, out_h: i32, existing: &[(ClickTarget, PxRect)]) -> Option<(ClickTarget, PxRect)> {
    let (first_x, y, right) = existing.iter().fold(None, |acc: Option<(i32, i32, i32)>, (_, rect)| {
        Some(match acc {
            None => (rect.x, rect.y, rect.x + rect.w),
            Some((left, y, right)) => (left.min(rect.x), y, right.max(rect.x + rect.w)),
        })
    })?;
    let preferred = first_x - PAIR_GAP - CONTROL_PX;
    let x = if preferred >= 0 { preferred } else { right + CONTROL_GAP };
    let x = x.clamp(0, (out_w - CONTROL_PX).max(0));
    let y = y.clamp(0, (out_h - CONTROL_PX).max(0));
    Some((ClickTarget::Note, PxRect { x, y, w: CONTROL_PX, h: CONTROL_PX }))
}

fn place_block(word: PxRect, out_w: i32, width: i32) -> i32 {
    let left = word.x - CONTROL_GAP - width;
    let x = if left >= 0 { left } else { word.x + word.w + CONTROL_GAP };
    x.clamp(0, (out_w - width).max(0))
}

pub fn hit_control(rects: &[(ClickTarget, PxRect)], x: i32, y: i32) -> Option<ClickTarget> {
    rects.iter().rev().find(|(_, rect)| x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h).map(|(t, _)| *t)
}

/// The highlight takes the pointer. The disc does not.
pub fn word_mark_takes_input() -> bool {
    true
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
        Proposal::QueryTask(body) => Some(route_query(tier, book, ledger, body)),
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
            continued: false,
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
        yield_scroll: false,
        hold_nudge: false,
        again: false,
        pass_only: false,
        say_seq: 0,
        say_open: false,
        left_phrase: None,
        left_until_ms: 0,
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
        TourSignalKind::Preempt => book.preempt_task(&body.task_id, body.reason),
        TourSignalKind::Settle => book.settle_task(&body.task_id),
    };
    match result {
        Ok(()) => ProposalResult::ok(&body.proposal_id, book.query_value(&body.task_id)),
        Err(err) => ProposalResult::err(&body.proposal_id, err_wire(err)),
    }
}

fn route_query(tier: Tier, book: &TaskBook, ledger: &Ledger, body: &crate::protocol::QueryTaskBody) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T2Client) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:queryTask");
    }
    match book.query_value(&body.task_id) {
        Some(mut detail) => {
            stamp_hands(&mut detail, ledger);
            ProposalResult::ok(&body.proposal_id, Some(detail))
        }
        None => ProposalResult::err(&body.proposal_id, "no_such_task"),
    }
}

fn stamp_hands(detail: &mut Value, ledger: &Ledger) {
    let Some(items) = detail.get_mut("items").and_then(|value| value.as_array_mut()) else {
        return;
    };
    for item in items {
        let Some(id) = item.get("phraseId").and_then(|value| value.as_str()).map(str::to_string) else {
            continue;
        };
        if let Some(obj) = item.as_object_mut() {
            obj.insert("hand".to_string(), json!(hand_condition(ledger, &id)));
        }
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
            "again": task.again,
            "passOnly": task.pass_only,
            "saySeq": task.say_seq,
            "say": if task.say_open { Value::String("Understood.".into()) } else { Value::Null },
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
        assert!(word_mark_takes_input());
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
            assert!(word_mark_takes_input());
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
        reg.note_task_highlight("p", None, 5);
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
        assert_eq!(body.matches("region.add").count(), 1);
        assert_eq!(src.matches("region.add").count(), 1);
        assert!(body.contains("KeyboardInteractivity::None"));
        assert!(body.contains("set_exclusive_zone(-1)"));
        assert!(!src.contains("get_keyboard"));
        assert!(!keyboard_grab_installed());
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
    fn e6_narrated_citation_stays_until_a_hand() {
        let mut opened = open(&task_line(
            "select",
            r#","itemTimeoutSeconds":1"#,
            r#"{"query":"one","narration":"why this sentence"},{"query":"two","narration":"the next line"}"#,
        ));
        assert_eq!(signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0).status, "ok");
        opened.book.tick(&mut opened.ledger, 1000);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        assert_eq!(signal(&mut opened.book, &mut opened.ledger, &opened.id, "narrationDone", 2000).status, "ok");
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
        opened.book.tick(&mut opened.ledger, 60_000);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");
        let ids = item_ids(&opened.book, &opened.id);
        assert!(click(&mut opened.book, &mut opened.ledger, &ids[0], ClickTarget::Check, 60_100));
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");

        let mut plain = open(&task_line("select", r#","itemTimeoutSeconds":1"#, r#"{"query":"one"},{"query":"two"}"#));
        assert_eq!(signal(&mut plain.book, &mut plain.ledger, &plain.id, "begin", 0).status, "ok");
        assert_eq!(signal(&mut plain.book, &mut plain.ledger, &plain.id, "narrationDone", 100).status, "ok");
        plain.book.tick(&mut plain.ledger, 999);
        assert_eq!(book_query(&plain.book, &plain.id)["index"], 0);
        plain.book.tick(&mut plain.ledger, 1000);
        assert_eq!(book_query(&plain.book, &plain.id)["index"], 1);

        let mut flowing = open(&task_line(
            "explain",
            "",
            r#"{"query":"one","narration":"why this sentence"},{"query":"two","narration":"the next line"}"#,
        ));
        signal(&mut flowing.book, &mut flowing.ledger, &flowing.id, "begin", 0);
        flowing.book.note_shown(&mut flowing.ledger, &flowing.phrase, 0);
        signal(&mut flowing.book, &mut flowing.ledger, &flowing.id, "narrationDone", 1_200);
        flowing.book.tick(&mut flowing.ledger, 5_000);
        assert_eq!(book_query(&flowing.book, &flowing.id)["index"], 0);
        assert_eq!(book_query(&flowing.book, &flowing.id)["state"], "active");
        flowing.book.note_highlight("p", &mut flowing.ledger, 5_100);
        flowing.book.apply_tint_samples(
            &mut flowing.ledger,
            vec![
                TintSample {
                    down: true,
                    button: BTN_LEFT,
                    ts_ms: 5_200,
                    mark: Some(TintMark { id: "p".into(), generation: 1 }),
                },
                TintSample {
                    down: false,
                    button: BTN_LEFT,
                    ts_ms: 5_300,
                    mark: Some(TintMark { id: "p".into(), generation: 1 }),
                },
            ],
        );
        assert_eq!(book_query(&flowing.book, &flowing.id)["index"], 1);
        assert_eq!(book_query(&flowing.book, &flowing.id)["state"], "active");
    }

    #[test]
    fn a_shown_mark_clears_not_found_and_the_hand_leaves() {
        let mut opened = open(&task_line(
            "select",
            "",
            r#"{"query":"allyl","narration":"why this sentence"},{"query":"inductive","narration":"the next line"}"#,
        ));
        assert_eq!(signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0).status, "ok");
        let phrase = opened.phrase.clone();
        let missed = resolve(&mut opened.book, &mut opened.ledger, &opened.id, &phrase, "notFound", "", Tier::T2Client, 1_000);
        assert_eq!(missed.status, "ok");
        assert_eq!(book_query(&opened.book, &opened.id)["items"][0]["match"], "notFound");
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        opened.book.note_phrase_mark(&phrase, &mut opened.ledger, 2_000);
        assert_eq!(book_query(&opened.book, &opened.id)["items"][0]["match"], "located");
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 0);
        let again = resolve(&mut opened.book, &mut opened.ledger, &opened.id, &phrase, "notFound", "", Tier::T2Client, 2_500);
        assert_eq!(again.status, "ok");
        assert_eq!(book_query(&opened.book, &opened.id)["items"][0]["match"], "located");
        opened.book.apply_tint_samples(
            &mut opened.ledger,
            vec![
                TintSample {
                    down: true,
                    button: BTN_LEFT,
                    ts_ms: 3_000,
                    mark: Some(TintMark { id: phrase.clone(), generation: 1 }),
                },
                TintSample {
                    down: false,
                    button: BTN_LEFT,
                    ts_ms: 3_200,
                    mark: Some(TintMark { id: phrase, generation: 1 }),
                },
            ],
        );
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 1);
        assert_eq!(book_query(&opened.book, &opened.id)["state"], "active");

        let mut pending = open(&task_line("select", "", r#"{"query":"one","narration":"why"},{"query":"two","narration":"next"}"#));
        signal(&mut pending.book, &mut pending.ledger, &pending.id, "begin", 0);
        pending.book.note_highlight("p", &mut pending.ledger, 100);
        assert_eq!(book_query(&pending.book, &pending.id)["items"][0]["match"], "located");
        let stuck = resolve(
            &mut pending.book,
            &mut pending.ledger,
            &pending.id,
            &pending.phrase,
            "notFound",
            "",
            Tier::T2Client,
            200,
        );
        assert_eq!(stuck.status, "ok");
        assert_eq!(book_query(&pending.book, &pending.id)["items"][0]["match"], "located");
        assert_eq!(book_query(&pending.book, &pending.id)["index"], 0);

        let mut ambiguous = open(&task_line(
            "select",
            "",
            r#"{"query":"one","narration":"why"},{"query":"two","narration":"next"}"#,
        ));
        signal(&mut ambiguous.book, &mut ambiguous.ledger, &ambiguous.id, "begin", 0);
        resolve(&mut ambiguous.book, &mut ambiguous.ledger, &ambiguous.id, &ambiguous.phrase, "ambiguous", "", Tier::T2Client, 100);
        ambiguous.book.note_phrase_mark(&ambiguous.phrase, &mut ambiguous.ledger, 200);
        assert_eq!(book_query(&ambiguous.book, &ambiguous.id)["items"][0]["match"], "ambiguous");
        let amb_phrase = ambiguous.phrase.clone();
        ambiguous.book.apply_tint_samples(
            &mut ambiguous.ledger,
            vec![
                TintSample {
                    down: true,
                    button: BTN_LEFT,
                    ts_ms: 300,
                    mark: Some(TintMark { id: amb_phrase.clone(), generation: 1 }),
                },
                TintSample {
                    down: false,
                    button: BTN_LEFT,
                    ts_ms: 900,
                    mark: Some(TintMark { id: amb_phrase, generation: 1 }),
                },
            ],
        );
        assert_eq!(book_query(&ambiguous.book, &ambiguous.id)["items"][0]["match"], "ambiguous");
        assert_eq!(book_query(&ambiguous.book, &ambiguous.id)["index"], 0);
        assert_eq!(book_query(&ambiguous.book, &ambiguous.id)["state"], "active");
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
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 0).unwrap();
        let before = ledger.events().len();
        assert_eq!(signal(&mut book, &mut ledger, &id, "preempt", 5).status, "ok");
        assert_eq!(book_query(&book, &id)["state"], "active");
        assert_eq!(ledger.events().len(), before);
        let phrase = item_ids(&book, &id)[0].clone();
        assert!(row(&ledger, &phrase)["decision"].is_null());
        assert_eq!(book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 2_000), Err(MoveError::Yielded));
        assert_eq!(book.moves_issued(), 1);
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
        reg.note_task_highlight("p", None, 5);
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
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 0).unwrap();
        let before = ledger.events().len();
        preempt(&mut book, &mut ledger, &id, "page");
        assert_eq!(book_query(&book, &id)["state"], "active");
        assert_eq!(ledger.events().len(), before);
        assert_eq!(book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 2_000), Err(MoveError::Yielded));
        assert_eq!(book.moves_issued(), 1);
        assert!(ledger.events().iter().all(|ev| !matches!(ev.kind, LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Cleared)));

        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let result = create(&mut book, &mut ledger, &scroll_line(r#"{"query":"the line","scroll":true}"#), Tier::T1User, 0);
        let id = result.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        book.admit().unwrap();
        book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 0).unwrap();
        let before = ledger.events().len();
        preempt(&mut book, &mut ledger, &id, "pointer");
        assert_eq!(book_query(&book, &id)["state"], "active");
        assert_eq!(ledger.events().len(), before);
        let nudge = ViewerMove::Nudge { dy_px: NUDGE_PX };
        assert_eq!(book.authorize(&id, &nudge, true, &mut ledger, 2_000), Err(MoveError::Yielded));
        assert_eq!(book.moves_issued(), 1);
        book.authorize(&id, &web_find("doc-a", "the line"), true, &mut ledger, 2_000).unwrap();
        assert_eq!(book.moves_issued(), 2);
        book.authorize(&id, &nudge, true, &mut ledger, 3_500).unwrap();
        assert_eq!(book.moves_issued(), 3);
        assert!(ledger.events().iter().all(|ev| !matches!(ev.kind, LedgerKind::Approved | LedgerKind::Discarded | LedgerKind::Cleared)));
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

    #[test]
    fn o3_note_control_binds_the_clicked_item() {
        let check = place_controls(word_at(100), 1920, 1080, DrawnControls::Check);
        let note = place_note_button(1920, 1080, &check).unwrap();
        assert_eq!(check[0].1.x, 64);
        assert_eq!(note.1.x, 32);
        assert_eq!(note.0, ClickTarget::Note);

        let mut opened = open(&task_line("select", "", r#"{"query":"a"},{"query":"b"},{"query":"c"}"#));
        signal(&mut opened.book, &mut opened.ledger, &opened.id, "begin", 0);
        let ids = item_ids(&opened.book, &opened.id);
        place(&mut opened.book, &opened.id, 200, 0);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 100));
        place(&mut opened.book, &opened.id, 400, 100);
        assert!(opened.book.handle_ui(&mut opened.ledger, UiEvent::Hotkey(Hotkey::Skip), 200));
        place(&mut opened.book, &opened.id, 700, 200);
        assert_eq!(book_query(&opened.book, &opened.id)["index"], 2);
        let hit = opened.book.hit(340, 45, 250).unwrap();
        assert_eq!(hit.0, ids[1]);
        assert_eq!(hit.1, ClickTarget::Note);
        assert!(click(&mut opened.book, &mut opened.ledger, &ids[1], ClickTarget::Note, 260));
        let proposal: Proposal = serde_json::from_str(r#"{"kind":"addNote","proposalId":"n","text":"cut the second clause","via":"typed"}"#).unwrap();
        let result = crate::ledger::route_note(&proposal, Tier::T1User, &mut opened.ledger, 300).unwrap();
        assert_eq!(result.status, "ok", "{:?}", result.error);
        assert_eq!(row(&opened.ledger, &ids[1])["notes"][0]["text"], "cut the second clause");
        assert!(row(&opened.ledger, &ids[2]).get("notes").is_none());
        let rects = opened.book.control_rects(&opened.id, word_at(100), 1920, 1080);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].0, ClickTarget::Check);

        let mut hot = open(&task_line("select", "", r#"{"query":"a"},{"query":"b"},{"query":"c"}"#));
        signal(&mut hot.book, &mut hot.ledger, &hot.id, "begin", 0);
        let hot_ids = item_ids(&hot.book, &hot.id);
        place(&mut hot.book, &hot.id, 200, 0);
        assert!(hot.book.handle_ui(&mut hot.ledger, UiEvent::Hotkey(Hotkey::Skip), 100));
        place(&mut hot.book, &hot.id, 400, 100);
        assert!(hot.book.handle_ui(&mut hot.ledger, UiEvent::Hotkey(Hotkey::Skip), 200));
        place(&mut hot.book, &hot.id, 700, 200);
        hot.book.hit(340, 45, 250).unwrap();
        assert!(hot.book.handle_ui(&mut hot.ledger, UiEvent::Hotkey(Hotkey::Note), 260));
        let armed = crate::ledger::route_note(&proposal, Tier::T1User, &mut hot.ledger, 300).unwrap();
        assert_eq!(armed.status, "ok", "{:?}", armed.error);
        assert_eq!(row(&hot.ledger, &hot_ids[1])["notes"][0]["text"], "cut the second clause");

        let mut active = open(&task_line("select", "", r#"{"query":"a"},{"query":"b"}"#));
        signal(&mut active.book, &mut active.ledger, &active.id, "begin", 0);
        let active_ids = item_ids(&active.book, &active.id);
        assert!(active.book.handle_ui(&mut active.ledger, UiEvent::Hotkey(Hotkey::Note), 10));
        let on_active = crate::ledger::route_note(&proposal, Tier::T1User, &mut active.ledger, 20).unwrap();
        assert_eq!(on_active.status, "ok", "{:?}", on_active.error);
        assert_eq!(row(&active.ledger, &active_ids[0])["notes"][0]["text"], "cut the second clause");
        assert!(row(&active.ledger, &active_ids[1]).get("notes").is_none());
    }

    fn hand_kinds(ledger: &Ledger) -> Vec<LedgerKind> {
        ledger
            .events()
            .iter()
            .filter(|ev| matches!(ev.kind, LedgerKind::Skip | LedgerKind::Accept | LedgerKind::Deny))
            .map(|ev| ev.kind)
            .collect()
    }

    fn tint_of(book: &TaskBook) -> Option<crate::highlight::MarkColor> {
        book.mark_colors().into_iter().find(|(id, _)| id == "driver").map(|(_, color)| color)
    }

    fn show_tour(pace: crate::protocol::TaskPace, loci: &[&str]) -> (TaskBook, Ledger, String) {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let locus_ids: Vec<String> = loci.iter().map(|id| (*id).to_string()).collect();
        let items = locus_ids
            .iter()
            .map(|id| TourSeedItem { query: format!("words {id}"), narration: String::new(), page: None })
            .collect();
        let id = book
            .insert_tour_chunk(
                &mut ledger,
                TourSeed {
                    origin_user: true,
                    mode: crate::protocol::TaskMode::Explain,
                    pace,
                    doc_key: None,
                    items,
                    locus_ids,
                    again: false,
                },
                1_000,
            )
            .unwrap();
        assert!(book.admit().is_some());
        book.begin_task(&id, &mut ledger, 2_000).unwrap();
        book.note_highlight("driver", &mut ledger, 2_100);
        (book, ledger, id)
    }

    fn gesture(book: &mut TaskBook, ledger: &mut Ledger, down_ms: u64, up_ms: u64, button: u32, release_generation: Option<u64>) {
        book.apply_tint_samples(
            ledger,
            vec![
                TintSample {
                    down: true,
                    button,
                    ts_ms: down_ms,
                    mark: Some(TintMark { id: "driver".into(), generation: 1 }),
                },
                TintSample {
                    down: false,
                    button,
                    ts_ms: up_ms,
                    mark: release_generation.map(|generation| TintMark { id: "driver".into(), generation }),
                },
            ],
        );
    }

    #[test]
    fn classify_tap_is_short_or_long_on_the_same_generation() {
        assert_eq!(classify_tap(BTN_LEFT, BTN_LEFT, 0, 499, 1, Some(1)), Some(Tap::Short));
        assert_eq!(classify_tap(BTN_LEFT, BTN_LEFT, 0, 500, 1, Some(1)), Some(Tap::Long));
        assert_eq!(classify_tap(BTN_LEFT, BTN_LEFT, 0, 100, 1, None), None);
        assert_eq!(classify_tap(BTN_LEFT, BTN_LEFT, 0, 100, 1, Some(2)), None);
        assert_eq!(classify_tap(0x111, 0x111, 0, 100, 1, Some(1)), None);
        assert_eq!(classify_tap(BTN_LEFT, 0x111, 0, 100, 1, Some(1)), None);
        assert_eq!(classify_tap(BTN_LEFT, BTN_LEFT, 100, 200, 1, Some(1)), Some(Tap::Short));
        assert_eq!(classify_tap(BTN_LEFT, BTN_LEFT, 300, 400, 1, Some(1)), Some(Tap::Short));
    }

    #[test]
    fn switch_tint_follows_the_three_conditions() {
        use crate::highlight::MarkColor;
        assert_eq!(switch_tint(MarkColor::Yellow, Tap::Short), (MarkColor::Green, LedgerKind::Accept));
        assert_eq!(switch_tint(MarkColor::Green, Tap::Short), (MarkColor::Yellow, LedgerKind::Skip));
        assert_eq!(switch_tint(MarkColor::Yellow, Tap::Long), (MarkColor::Red, LedgerKind::Deny));
        assert_eq!(switch_tint(MarkColor::Green, Tap::Long), (MarkColor::Red, LedgerKind::Deny));
        assert_eq!(switch_tint(MarkColor::Red, Tap::Short), (MarkColor::Yellow, LedgerKind::Skip));
        assert_eq!(switch_tint(MarkColor::Red, Tap::Long), (MarkColor::Yellow, LedgerKind::Skip));
    }

    #[test]
    fn insert_tour_chunk_uses_the_locus_id() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let mismatch = book.insert_tour_chunk(
            &mut ledger,
            TourSeed {
                origin_user: true,
                mode: crate::protocol::TaskMode::Explain,
                pace: crate::protocol::TaskPace::Flow,
                doc_key: None,
                items: vec![TourSeedItem { query: "a".into(), narration: String::new(), page: None }],
                locus_ids: vec![],
                again: false,
            },
            0,
        );
        assert!(matches!(mismatch, Err(TaskError::Schema)));
        assert_eq!(ledger.occupancy(), 0);
        assert!(!ledger.holds("ph-1"));
        let empty = book.insert_tour_chunk(
            &mut ledger,
            TourSeed {
                origin_user: true,
                mode: crate::protocol::TaskMode::Explain,
                pace: crate::protocol::TaskPace::Flow,
                doc_key: None,
                items: vec![TourSeedItem { query: "a".into(), narration: String::new(), page: None }],
                locus_ids: vec!["  ".into()],
                again: false,
            },
            0,
        );
        assert!(matches!(empty, Err(TaskError::Schema)));
        assert_eq!(ledger.occupancy(), 0);
        let id = book
            .insert_tour_chunk(
                &mut ledger,
                TourSeed {
                    origin_user: true,
                    mode: crate::protocol::TaskMode::Explain,
                    pace: crate::protocol::TaskPace::Flow,
                    doc_key: None,
                    items: vec![TourSeedItem { query: "the line".into(), narration: String::new(), page: None }],
                    locus_ids: vec!["lc-9".into()],
                    again: false,
                },
                0,
            )
            .unwrap();
        assert_eq!(book.query_value(&id).unwrap()["items"][0]["phraseId"], "lc-9");
        assert!(ledger.holds("lc-9"));
        assert!(!ledger.holds("ph-1"));
    }

    #[test]
    fn a_slide_off_or_a_right_button_appends_no_row() {
        let (mut book, mut ledger, id) = show_tour(crate::protocol::TaskPace::Flow, &["lc-a", "lc-b"]);
        assert_eq!(hand_condition(&ledger, "lc-a"), "skip");
        assert!(hand_kinds(&ledger).is_empty());
        assert_eq!(tint_of(&book), Some(crate::highlight::MarkColor::Yellow));
        gesture(&mut book, &mut ledger, 3_000, 3_100, 0x111, Some(1));
        gesture(&mut book, &mut ledger, 3_200, 3_300, BTN_LEFT, None);
        gesture(&mut book, &mut ledger, 3_400, 3_500, BTN_LEFT, Some(9));
        book.apply_tint_samples(
            &mut ledger,
            vec![TintSample {
                down: true,
                button: BTN_LEFT,
                ts_ms: 3_600,
                mark: Some(TintMark { id: "driver".into(), generation: 1 }),
            }],
        );
        book.note_shown(&mut ledger, "lc-b", 3_700);
        book.apply_tint_samples(
            &mut ledger,
            vec![TintSample {
                down: false,
                button: BTN_LEFT,
                ts_ms: 3_800,
                mark: Some(TintMark { id: "driver".into(), generation: 1 }),
            }],
        );
        assert!(hand_kinds(&ledger).is_empty());
        assert_eq!(tint_of(&book), Some(crate::highlight::MarkColor::Yellow));
        assert_eq!(book_query(&book, &id)["index"], 0);
        assert_eq!(book_query(&book, &id)["state"], "active");
    }

    #[test]
    fn a_flow_tap_changes_color_and_leaves_the_item() {
        let (mut book, mut ledger, id) = show_tour(crate::protocol::TaskPace::Flow, &["lc-a", "lc-b"]);
        gesture(&mut book, &mut ledger, 3_000, 3_200, BTN_LEFT, Some(1));
        assert_eq!(hand_kinds(&ledger), vec![LedgerKind::Accept]);
        assert_eq!(tint_of(&book), Some(crate::highlight::MarkColor::Green));
        assert_eq!(row(&ledger, "lc-a")["decision"], "accept");
        assert_eq!(book_query(&book, &id)["index"], 0);
        assert_eq!(book_query(&book, &id)["state"], "active");
        assert!(ledger.events().iter().filter(|ev| ev.kind == LedgerKind::Accept).all(|ev| ev.project_id.is_none()));
        gesture(&mut book, &mut ledger, 3_300, 3_400, BTN_LEFT, Some(1));
        assert_eq!(hand_kinds(&ledger), vec![LedgerKind::Accept, LedgerKind::Skip]);
        assert_eq!(tint_of(&book), Some(crate::highlight::MarkColor::Yellow));
        assert!(row(&ledger, "lc-a")["decision"].is_null());
        assert_eq!(book_query(&book, &id)["index"], 0);
        assert_eq!(book_query(&book, &id)["state"], "active");
        book.set_presentation(&id, crate::protocol::TaskMode::Explain, crate::protocol::TaskPace::Wait, None).unwrap();
        book.tick(&mut ledger, 4_000);
        assert_eq!(book_query(&book, &id)["index"], 0);
        assert_eq!(book_query(&book, &id)["state"], "active");
        gesture(&mut book, &mut ledger, 4_100, 4_200, BTN_LEFT, Some(1));
        assert_eq!(hand_kinds(&ledger), vec![LedgerKind::Accept, LedgerKind::Skip, LedgerKind::Accept]);
        assert_eq!(book_query(&book, &id)["index"], 1);
        assert_eq!(book_query(&book, &id)["state"], "active");
    }

    #[test]
    fn a_wait_deny_answers_the_item() {
        let (mut book, mut ledger, id) = show_tour(crate::protocol::TaskPace::Wait, &["lc-a", "lc-b"]);
        assert_eq!(hand_condition(&ledger, "lc-a"), "skip");
        assert!(hand_kinds(&ledger).is_empty());
        gesture(&mut book, &mut ledger, 3_000, 3_500, BTN_LEFT, Some(1));
        assert_eq!(hand_kinds(&ledger), vec![LedgerKind::Deny]);
        assert_eq!(tint_of(&book), Some(crate::highlight::MarkColor::Red));
        assert_eq!(book_query(&book, &id)["index"], 1);
        assert_eq!(book_query(&book, &id)["state"], "active");
    }

    fn gesture_on(book: &mut TaskBook, ledger: &mut Ledger, mark: &str, down_ms: u64, up_ms: u64) -> Vec<String> {
        book.apply_tint_samples(
            ledger,
            vec![
                TintSample {
                    down: true,
                    button: BTN_LEFT,
                    ts_ms: down_ms,
                    mark: Some(TintMark { id: mark.into(), generation: 1 }),
                },
                TintSample {
                    down: false,
                    button: BTN_LEFT,
                    ts_ms: up_ms,
                    mark: Some(TintMark { id: mark.into(), generation: 1 }),
                },
            ],
        )
    }

    fn queried(book: &mut TaskBook, ledger: &mut Ledger, id: &str) -> Value {
        let line = format!(r#"{{"kind":"queryTask","proposalId":"q","taskId":"{id}"}}"#);
        route_task(&parse(&line), Tier::T2Client, book, ledger, true, 0).unwrap().detail.unwrap()
    }

    #[test]
    fn a_tap_on_another_locus_says_understood_and_takes_the_next() {
        let (mut book, mut ledger, id) = show_tour(crate::protocol::TaskPace::Flow, &["lc-a", "lc-b", "lc-c"]);
        assert_eq!(queried(&mut book, &mut ledger, &id)["items"][1]["hand"], "skip");
        let dropped = gesture_on(&mut book, &mut ledger, "lc-b", 3_000, 3_200);
        assert_eq!(dropped, vec!["lc-b".to_string()]);
        assert_eq!(hand_kinds(&ledger), vec![LedgerKind::Accept]);
        assert_eq!(row(&ledger, "lc-b")["decision"], "accept");
        let view = queried(&mut book, &mut ledger, &id);
        assert_eq!(view["say"], "Understood.");
        assert_eq!(view["index"], 2);
        assert_eq!(view["state"], "active");
        assert_eq!(view["items"][1]["hand"], "accept");
        let seq = view["saySeq"].as_u64().unwrap();
        let again = gesture_on(&mut book, &mut ledger, "lc-b", 3_300, 3_400);
        assert!(again.is_empty());
        let view = queried(&mut book, &mut ledger, &id);
        assert_eq!(view["index"], 2);
        assert_eq!(view["state"], "active");
        assert_eq!(view["saySeq"].as_u64().unwrap(), seq + 1);
        assert_eq!(row(&ledger, "lc-b")["decision"].as_str(), None);
        assert_eq!(hand_kinds(&ledger), vec![LedgerKind::Accept, LedgerKind::Skip]);
    }

    #[test]
    fn a_tap_on_the_last_locus_ends_the_list() {
        let (mut book, mut ledger, id) = show_tour(crate::protocol::TaskPace::Flow, &["lc-a", "lc-b"]);
        gesture_on(&mut book, &mut ledger, "lc-b", 3_000, 3_200);
        let view = book_query(&book, &id);
        assert_eq!(view["state"], "done");
        assert_eq!(view["index"], 1);
        assert_eq!(view["say"], "Understood.");
        assert_eq!(row(&ledger, "lc-b")["decision"], "accept");
    }

    #[test]
    fn again_walks_remaining_skips_without_a_scroll_and_settle_ends() {
        let mut book = TaskBook::new();
        let mut ledger = Ledger::new();
        let id = book
            .insert_tour_chunk(
                &mut ledger,
                TourSeed {
                    origin_user: true,
                    mode: crate::protocol::TaskMode::Explain,
                    pace: crate::protocol::TaskPace::Flow,
                    doc_key: Some("doc-a".into()),
                    items: vec![
                        TourSeedItem { query: "one".into(), narration: String::new(), page: None },
                        TourSeedItem { query: "two".into(), narration: String::new(), page: None },
                    ],
                    locus_ids: vec!["lc-a".into(), "lc-b".into()],
                    again: true,
                },
                1_000,
            )
            .unwrap();
        book.admit().unwrap();
        book.begin_task(&id, &mut ledger, 2_000).unwrap();
        let walk = |book: &mut TaskBook, ledger: &mut Ledger, at: u64| {
            book.tick(ledger, at);
            assert_eq!(signal(book, ledger, &id, "narrationDone", at).status, "ok");
        };
        walk(&mut book, &mut ledger, 2_000 + DWELL_MS);
        assert_eq!(book_query(&book, &id)["index"], 1);
        assert_eq!(book_query(&book, &id)["passOnly"], false);
        walk(&mut book, &mut ledger, 2_000 + DWELL_MS * 2);
        assert_eq!(book_query(&book, &id)["state"], "active");
        assert_eq!(book_query(&book, &id)["passOnly"], true);
        assert_eq!(book_query(&book, &id)["index"], 0);
        assert_eq!(book_query(&book, &id)["again"], true);
        assert_eq!(book.authorize(&id, &web_find("doc-a", "one"), true, &mut ledger, 2_000 + DWELL_MS * 2), Err(MoveError::Yielded));
        let hands = hand_kinds(&ledger);
        walk(&mut book, &mut ledger, 2_000 + DWELL_MS * 3);
        assert_eq!(book_query(&book, &id)["index"], 1);
        assert_eq!(book_query(&book, &id)["passOnly"], true);
        walk(&mut book, &mut ledger, 2_000 + DWELL_MS * 4);
        assert_eq!(book_query(&book, &id)["state"], "done");
        assert_eq!(hand_kinds(&ledger), hands);

        let (mut settled, mut settled_ledger, settled_id) = show_tour(crate::protocol::TaskPace::Flow, &["lc-a", "lc-b"]);
        let before = hand_kinds(&settled_ledger);
        assert_eq!(signal(&mut settled, &mut settled_ledger, &settled_id, "settle", 4_000).status, "ok");
        assert_eq!(book_query(&settled, &settled_id)["state"], "done");
        assert_eq!(book_query(&settled, &settled_id)["passOnly"], false);
        assert!(book_query(&settled, &settled_id)["say"].is_null());
        assert_eq!(hand_kinds(&settled_ledger), before);
    }
}
