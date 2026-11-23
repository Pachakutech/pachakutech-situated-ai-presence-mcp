//! Hover row on one highlight. Pad, check, and X. The frame loop paints it.
//! The gesture path does not sleep. `tick` hides the row when the second
//! elapses, and a scroll hides it at once.

use crate::highlight::{MarkColor, UvBox};

/// How long the row stays after the pointer leaves it. The daemon clock measures it.
pub const CONTROL_ROW_MS: u64 = 1000;
pub const ROW_PX: i32 = 28;
/// Even gap, smaller than the square.
pub const ROW_GAP: i32 = 8;
/// Horizontal step of one glyph. A new line starts one square lower.
const GLYPH_STEP: i32 = 6;
const FIELD_INSET: i32 = 6;
const GLYPH_BASELINE: i32 = 8;
/// Lines the field shows. The caret is the end of the draft, so a longer note shows its last lines.
const FIELD_LINES: i32 = 4;
/// Glyphs kept when half the highlight cannot hold the back square and a short run.
const SHORT_RUN: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Px {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowButton {
    Note,
    Check,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowHit {
    Tint,
    Note,
    Check,
    Deny,
    Field,
    Miss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowLayout {
    pub above: bool,
    pub note: Px,
    pub check: Px,
    pub deny: Px,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorLayout {
    pub above: bool,
    pub back: Px,
    pub field: Px,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteFlush {
    pub phrase_id: String,
    pub text: String,
    pub replaces: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    None,
    /// The press started on the tint. The tint classifier owns it.
    Tint,
    Hand(RowButton),
    OpenNote,
    Back,
}

/// One pointer or key event, drained by the frame loop before `tick`.
#[derive(Clone, Debug, PartialEq)]
pub enum RowInput {
    Enter { phrase_id: String, x: i32, y: i32, w: i32, h: i32 },
    Pointer { inside: bool },
    Motion,
    Press { x: i32, y: i32 },
    Release { x: i32, y: i32 },
    Scroll,
    Text(String),
    Backspace,
}

/// What the frame loop draws. Screen pixels. Absent when the row is hidden.
#[derive(Clone, Debug)]
pub struct Paint {
    pub phrase_id: String,
    pub above: bool,
    pub editor: bool,
    pub color: MarkColor,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: i32,
    pub height: i32,
    /// Squares in screen pixels. The editor has the back square only.
    pub squares: Vec<(RowHit, Px)>,
    pub text: String,
}

#[derive(Clone, Debug)]
struct Open {
    phrase_id: String,
    px: Px,
    uv: Option<UvBox>,
    until_ms: u64,
    /// Pointer is on the tint or the row.
    inside: bool,
    press: Option<RowHit>,
    editor: bool,
    draft: String,
    stored: String,
    stored_id: Option<String>,
    pending: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ControlRow {
    open: Option<Open>,
    /// A flush the cadence refused. `tick` tries it again after the row is gone.
    pending: Option<NoteFlush>,
}

impl ControlRow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phrase(&self) -> Option<&str> {
        self.open.as_ref().map(|row| row.phrase_id.as_str())
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn editor_open(&self) -> bool {
        self.open.as_ref().is_some_and(|row| row.editor)
    }

    pub fn held_uv(&self, phrase_id: &str) -> Option<UvBox> {
        self.open.as_ref().filter(|row| row.phrase_id == phrase_id).and_then(|row| row.uv)
    }

    pub fn held_px(&self) -> Option<Px> {
        self.open.as_ref().map(|row| row.px)
    }

    pub fn draft(&self) -> &str {
        self.open.as_ref().map(|row| row.draft.as_str()).unwrap_or("")
    }

    /// Pointer entered this tint. Remembers the box. Writes no hand.
    pub fn enter(&mut self, phrase_id: &str, px: Px, uv: Option<UvBox>, now_ms: u64) {
        if phrase_id.is_empty() {
            return;
        }
        if self.open.as_ref().is_some_and(|row| row.phrase_id == phrase_id) {
            if let Some(row) = self.open.as_mut() {
                row.inside = true;
                row.until_ms = now_ms.saturating_add(CONTROL_ROW_MS);
            }
            return;
        }
        self.open = Some(Open {
            phrase_id: phrase_id.to_string(),
            px,
            uv,
            until_ms: now_ms.saturating_add(CONTROL_ROW_MS),
            inside: true,
            press: None,
            editor: false,
            draft: String::new(),
            stored: String::new(),
            stored_id: None,
            pending: false,
        });
    }

    /// A phone tap with the row down. Same second, no hand. Returns false when
    /// the row was already up, so the caller treats that tap as the tint switch.
    pub fn touch_reveal(&mut self, phrase_id: &str, px: Px, uv: Option<UvBox>, now_ms: u64) -> bool {
        if self.is_open() {
            return false;
        }
        self.enter(phrase_id, px, uv, now_ms);
        true
    }

    pub fn pointer(&mut self, inside: bool, now_ms: u64) {
        let Some(row) = self.open.as_mut() else { return };
        row.inside = inside;
        // Leave starts the second. A later enter or motion restarts it.
        row.until_ms = now_ms.saturating_add(CONTROL_ROW_MS);
    }

    pub fn motion(&mut self, now_ms: u64) {
        let Some(row) = self.open.as_mut() else { return };
        if row.inside {
            row.until_ms = now_ms.saturating_add(CONTROL_ROW_MS);
        }
    }

    pub fn press(&mut self, hit: RowHit) {
        let Some(row) = self.open.as_mut() else { return };
        if matches!(hit, RowHit::Miss) {
            return;
        }
        row.press = Some(hit);
    }

    pub fn release(&mut self, hit: RowHit) -> Release {
        let Some(row) = self.open.as_mut() else { return Release::None };
        let started = row.press.take();
        let Some(started) = started else { return Release::None };
        if hit != started {
            return Release::None;
        }
        if row.editor {
            return if matches!(hit, RowHit::Note) { Release::Back } else { Release::None };
        }
        match hit {
            RowHit::Tint => Release::Tint,
            RowHit::Note => Release::OpenNote,
            RowHit::Check => Release::Hand(RowButton::Check),
            RowHit::Deny => Release::Hand(RowButton::Deny),
            RowHit::Field | RowHit::Miss => Release::None,
        }
    }

    pub fn open_editor(&mut self, existing: &str, existing_id: Option<String>) {
        let Some(row) = self.open.as_mut() else { return };
        row.editor = true;
        row.draft = existing.to_string();
        row.stored = existing.to_string();
        row.stored_id = existing_id;
        row.press = None;
        row.pending = false;
    }

    pub fn push_text(&mut self, text: &str) -> Option<NoteFlush> {
        let Some(row) = self.open.as_mut() else { return None };
        if !row.editor {
            return None;
        }
        row.draft.push_str(text);
        flush_of(row)
    }

    pub fn backspace(&mut self) -> Option<NoteFlush> {
        let Some(row) = self.open.as_mut() else { return None };
        if !row.editor {
            return None;
        }
        row.draft.pop();
        flush_of(row)
    }

    /// Back closes the field. The flush is the latest text when it differs.
    pub fn back(&mut self) -> Option<NoteFlush> {
        let flush = self.open.as_ref().filter(|row| row.editor).and_then(|row| flush_of_ref(row));
        if let Some(row) = self.open.as_mut() {
            row.editor = false;
            row.draft.clear();
            row.press = None;
        }
        flush
    }

    /// Scroll hides the row, editor included. The flush is written first.
    pub fn scroll(&mut self) -> Option<NoteFlush> {
        let flush = self.open.as_ref().filter(|row| row.editor).and_then(|row| flush_of_ref(row));
        self.open = None;
        flush
    }

    pub fn note_stored(&mut self, id: &str, text: &str) {
        self.pending = None;
        if let Some(row) = self.open.as_mut() {
            row.stored = text.to_string();
            row.stored_id = Some(id.to_string());
            row.pending = false;
        }
    }

    pub fn note_pending(&mut self, flush: NoteFlush) {
        if let Some(row) = self.open.as_mut() {
            row.pending = true;
        }
        self.pending = Some(flush);
    }

    pub fn note_rejected(&mut self) {
        self.pending = None;
        if let Some(row) = self.open.as_mut() {
            row.pending = false;
        }
    }

    /// Restarts the second while the pointer is inside. Hides when it elapses
    /// with the pointer off the tint and the row, unless a press or the editor holds it.
    pub fn tick(&mut self, now_ms: u64) -> Option<NoteFlush> {
        if let Some(row) = self.open.as_mut() {
            if row.inside {
                row.until_ms = now_ms.saturating_add(CONTROL_ROW_MS);
            }
            let hold = row.editor || row.press.is_some() || row.inside;
            if !hold && now_ms >= row.until_ms {
                self.open = None;
            }
        }
        self.pending.clone()
    }

    pub fn paint(&self, color: MarkColor, screen_w: i32, screen_h: i32) -> Option<Paint> {
        let row = self.open.as_ref()?;
        if row.editor {
            let editor = place_editor(row.px, screen_w, screen_h, &row.draft);
            let origin_x = editor.back.x.min(editor.field.x);
            let origin_y = editor.back.y.min(editor.field.y);
            let right = (editor.back.x + editor.back.w).max(editor.field.x + editor.field.w);
            let bottom = (editor.back.y + editor.back.h).max(editor.field.y + editor.field.h);
            return Some(Paint {
                phrase_id: row.phrase_id.clone(),
                above: editor.above,
                editor: true,
                color,
                origin_x,
                origin_y,
                width: (right - origin_x).max(1),
                height: (bottom - origin_y).max(1),
                squares: vec![(RowHit::Note, editor.back), (RowHit::Field, editor.field)],
                text: row.draft.clone(),
            });
        }
        let layout = place_row(row.px, screen_w, screen_h);
        let origin_x = layout.note.x;
        let origin_y = layout.note.y;
        let right = layout.deny.x + layout.deny.w;
        Some(Paint {
            phrase_id: row.phrase_id.clone(),
            above: layout.above,
            editor: false,
            color,
            origin_x,
            origin_y,
            width: (right - origin_x).max(1),
            height: ROW_PX,
            squares: vec![
                (RowHit::Note, layout.note),
                (RowHit::Check, layout.check),
                (RowHit::Deny, layout.deny),
            ],
            text: String::new(),
        })
    }

    pub fn hit(&self, x: i32, y: i32, screen_w: i32, screen_h: i32) -> RowHit {
        let Some(paint) = self.paint(MarkColor::Yellow, screen_w, screen_h) else {
            return RowHit::Miss;
        };
        for (hit, rect) in paint.squares.iter().rev() {
            if x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h {
                return *hit;
            }
        }
        let Some(px) = self.held_px() else { return RowHit::Miss };
        if x >= px.x && y >= px.y && x < px.x + px.w && y < px.y + px.h {
            RowHit::Tint
        } else {
            RowHit::Miss
        }
    }
}

fn flush_of(row: &Open) -> Option<NoteFlush> {
    flush_of_ref(row)
}

fn flush_of_ref(row: &Open) -> Option<NoteFlush> {
    let text = row.draft.trim();
    if text.is_empty() || text == row.stored.trim() {
        return None;
    }
    Some(NoteFlush {
        phrase_id: row.phrase_id.clone(),
        text: text.to_string(),
        replaces: row.stored_id.clone(),
    })
}

/// Right edge of the X meets the highlight. Squares run left from that edge.
/// The row sits on the side with more room. A tie sits above. The gap keeps
/// the squares off the tint.
pub fn place_row(mark: Px, screen_w: i32, screen_h: i32) -> RowLayout {
    let _ = screen_w;
    let right = mark.x.saturating_add(mark.w);
    let deny_x = right - ROW_PX;
    let check_x = deny_x - ROW_GAP - ROW_PX;
    let note_x = check_x - ROW_GAP - ROW_PX;
    let above_gap = mark.y;
    let below_gap = screen_h.saturating_sub(mark.y.saturating_add(mark.h));
    let above = above_gap >= below_gap;
    let y = if above { mark.y - ROW_GAP - ROW_PX } else { mark.y.saturating_add(mark.h).saturating_add(ROW_GAP) };
    let square = |x| Px { x, y, w: ROW_PX, h: ROW_PX };
    RowLayout { above, note: square(note_x), check: square(check_x), deny: square(deny_x) }
}

/// The pad square becomes the back square. Check and X are gone. The field's
/// right edge stays on the highlight. Width and height follow the lines the
/// painter draws, up to four lines and half the highlight. A highlight so
/// narrow that half of it cannot hold the back square and a short run of
/// glyphs keeps that wider minimum.
pub fn place_editor(mark: Px, screen_w: i32, screen_h: i32, text: &str) -> EditorLayout {
    let row = place_row(mark, screen_w, screen_h);
    let (text_w, text_h) = field_box(mark.w, text);
    let right = mark.x.saturating_add(mark.w);
    let editor_w = ROW_PX + ROW_GAP + text_w;
    let left = (right - editor_w).max(0);
    let field_x = left + ROW_PX + ROW_GAP;
    let field_w = text_w.min(screen_w.saturating_sub(field_x).max(1));
    let (field_y, back_y) = if row.above {
        let bottom = row.note.y + ROW_PX;
        let y = (bottom - text_h).max(0);
        (y, (y + text_h - ROW_PX).max(0))
    } else {
        let y = row.note.y.max(0).min(screen_h.saturating_sub(text_h).max(0));
        (y, y)
    };
    let back = Px { x: left, y: back_y, w: ROW_PX, h: ROW_PX };
    EditorLayout { above: row.above, back, field: Px { x: field_x, y: field_y, w: field_w, h: text_h } }
}

/// Screen point of a click on the row layer. The layer origin is the square
/// that is drawn. A pointer origin latched on the pad is stale once the field
/// moves that layer, and it is not added.
pub fn row_screen_point(pointer_origin: (i32, i32), layer_origin: (i32, i32), surface_x: i32, surface_y: i32) -> (i32, i32) {
    let _stale = pointer_origin;
    (layer_origin.0.saturating_add(surface_x), layer_origin.1.saturating_add(surface_y))
}

fn field_box(mark_w: i32, text: &str) -> (i32, i32) {
    let lines = visible_lines(text);
    let widest = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0) as i32;
    let natural = line_width(widest);
    let half = mark_w.max(0) / 2;
    let minimum = ROW_PX + ROW_GAP + line_width(SHORT_RUN);
    let cap = half.max(minimum);
    let max_text = (cap - ROW_PX - ROW_GAP).max(line_width(0));
    let min_text = if half < minimum { line_width(SHORT_RUN) } else { line_width(0) };
    let text_w = natural.clamp(min_text, max_text);
    let text_h = (lines.len() as i32).max(1) * ROW_PX;
    (text_w, text_h)
}

fn visible_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.split('\n').collect();
    let start = lines.len().saturating_sub(FIELD_LINES as usize);
    lines[start..].to_vec()
}

fn line_width(chars: i32) -> i32 {
    FIELD_INSET * 2 + chars.max(0) * GLYPH_STEP
}

fn glyphs_that_fit(field_w: i32) -> usize {
    let room = field_w.saturating_sub(FIELD_INSET + 1);
    (room / GLYPH_STEP) as usize
}

pub fn overlaps(a: Px, b: Px) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// Check toggles accept and skip. X toggles deny and skip.
pub fn button_hand(from: MarkColor, button: RowButton) -> (MarkColor, &'static str) {
    match button {
        RowButton::Check => match from {
            MarkColor::Green => (MarkColor::Yellow, "skip"),
            MarkColor::Yellow | MarkColor::Red => (MarkColor::Green, "accept"),
        },
        RowButton::Deny => match from {
            MarkColor::Red => (MarkColor::Yellow, "skip"),
            MarkColor::Yellow | MarkColor::Green => (MarkColor::Red, "deny"),
        },
        RowButton::Note => (from, "note"),
    }
}

/// Premultiplied BGRA. The editor draws the back icon and the field, not a check or an X.
pub fn paint_buffer(buf: &mut [u8], paint: &Paint) {
    buf.fill(0);
    let stride = paint.width.saturating_mul(4);
    for (hit, rect) in &paint.squares {
        let local_x = rect.x - paint.origin_x;
        let local_y = rect.y - paint.origin_y;
        let selected = selected_icon(*hit, paint.color);
        fill_squircle(buf, paint.width, paint.height, stride, local_x, local_y, rect.w, rect.h, selected);
        match hit {
            RowHit::Note if paint.editor => draw_back(buf, stride, local_x, local_y),
            RowHit::Note => draw_pad(buf, stride, local_x, local_y),
            RowHit::Check => draw_check(buf, stride, local_x, local_y, selected),
            RowHit::Deny => draw_x(buf, stride, local_x, local_y, selected),
            RowHit::Field => draw_field(buf, stride, local_x, local_y, rect.w, rect.h, &paint.text),
            RowHit::Tint | RowHit::Miss => {}
        }
    }
}

fn selected_icon(hit: RowHit, color: MarkColor) -> bool {
    matches!((hit, color), (RowHit::Check, MarkColor::Green) | (RowHit::Deny, MarkColor::Red))
}

fn fill_squircle(buf: &mut [u8], bw: i32, bh: i32, stride: i32, x0: i32, y0: i32, w: i32, h: i32, selected: bool) {
    let radius = 8.0_f32.min(w as f32 * 0.35).min(h as f32 * 0.35);
    let alpha = if selected { 0.92 } else { 0.55 };
    for y in 0..h {
        for x in 0..w {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let hw = w as f32 * 0.5;
            let hh = h as f32 * 0.5;
            let qx = (px - hw).abs() - (hw - radius);
            let qy = (py - hh).abs() - (hh - radius);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            let inside = qx.max(qy).min(0.0);
            if outside + inside - radius > 0.5 {
                continue;
            }
            put(buf, bw, bh, stride, x0 + x, y0 + y, 0.95, 0.95, 0.95, alpha);
        }
    }
}

fn draw_pad(buf: &mut [u8], stride: i32, x0: i32, y0: i32) {
    for y in 8..20 {
        for x in 7..16 {
            put(buf, 10_000, 10_000, stride, x0 + x, y0 + y, 0.15, 0.2, 0.25, 0.95);
        }
    }
    for i in 0..10 {
        put(buf, 10_000, 10_000, stride, x0 + 15 + i / 2, y0 + 6 + i, 0.2, 0.35, 0.7, 1.0);
    }
}

fn draw_back(buf: &mut [u8], stride: i32, x0: i32, y0: i32) {
    for i in 0..12 {
        put(buf, 10_000, 10_000, stride, x0 + 8 + i, y0 + 13, 0.1, 0.15, 0.2, 1.0);
    }
    for i in 0..6 {
        put(buf, 10_000, 10_000, stride, x0 + 8 + i, y0 + 13 - i, 0.1, 0.15, 0.2, 1.0);
        put(buf, 10_000, 10_000, stride, x0 + 8 + i, y0 + 13 + i, 0.1, 0.15, 0.2, 1.0);
    }
}

fn draw_check(buf: &mut [u8], stride: i32, x0: i32, y0: i32, selected: bool) {
    let g = if selected { 0.55 } else { 0.35 };
    for i in 0..6 {
        put(buf, 10_000, 10_000, stride, x0 + 7 + i, y0 + 14 + i, 0.15, g, 0.2, 1.0);
    }
    for i in 0..10 {
        put(buf, 10_000, 10_000, stride, x0 + 12 + i, y0 + 19 - i, 0.15, g, 0.2, 1.0);
    }
}

fn draw_x(buf: &mut [u8], stride: i32, x0: i32, y0: i32, selected: bool) {
    let r = if selected { 0.75 } else { 0.4 };
    for i in 0..12 {
        put(buf, 10_000, 10_000, stride, x0 + 8 + i, y0 + 8 + i, r, 0.15, 0.15, 1.0);
        put(buf, 10_000, 10_000, stride, x0 + 19 - i, y0 + 8 + i, r, 0.15, 0.15, 1.0);
    }
}

fn draw_field(buf: &mut [u8], stride: i32, x0: i32, y0: i32, w: i32, h: i32, text: &str) {
    fill_squircle(buf, 10_000, 10_000, stride, x0, y0, w, h, false);
    let fit = glyphs_that_fit(w);
    for (i, line) in visible_lines(text).iter().enumerate() {
        let chars: Vec<char> = line.chars().filter(|ch| *ch != '\n').collect();
        let start = chars.len().saturating_sub(fit);
        let y = y0 + GLYPH_BASELINE + i as i32 * ROW_PX;
        let mut cursor = FIELD_INSET;
        for ch in chars.iter().skip(start).take(fit) {
            draw_glyph(buf, stride, x0 + cursor, y, *ch);
            cursor += GLYPH_STEP;
        }
    }
}

fn draw_glyph(buf: &mut [u8], stride: i32, x0: i32, y0: i32, ch: char) {
    let bits = glyph_bits(ch);
    for row in 0..5 {
        for col in 0..3 {
            if bits & (1 << (row * 3 + col)) == 0 {
                continue;
            }
            put(buf, 10_000, 10_000, stride, x0 + col, y0 + row, 0.1, 0.12, 0.16, 1.0);
        }
    }
}

fn glyph_bits(ch: char) -> u16 {
    match ch.to_ascii_lowercase() {
        'a' => 0b010_101_111_101_101,
        'b' => 0b110_101_110_101_110,
        'c' => 0b011_100_100_100_011,
        'd' => 0b011_101_101_101_011,
        'e' => 0b111_100_110_100_111,
        'f' => 0b111_100_110_100_100,
        'g' => 0b011_100_101_101_011,
        'h' => 0b101_101_111_101_101,
        'i' => 0b111_010_010_010_111,
        'k' => 0b101_101_110_101_101,
        'l' => 0b100_100_100_100_111,
        'm' => 0b101_111_111_101_101,
        'n' => 0b110_101_101_101_101,
        'o' => 0b010_101_101_101_010,
        'p' => 0b110_101_110_100_100,
        'r' => 0b110_101_110_101_101,
        's' => 0b011_100_010_001_110,
        't' => 0b111_010_010_010_010,
        'u' => 0b101_101_101_101_011,
        'v' => 0b101_101_101_101_010,
        'w' => 0b101_101_111_111_101,
        'x' => 0b101_101_010_101_101,
        'y' => 0b101_101_010_010_010,
        ' ' | '\n' | '\r' => 0,
        _ => 0b111_101_101_101_111,
    }
}

fn put(buf: &mut [u8], bw: i32, bh: i32, stride: i32, x: i32, y: i32, red: f32, green: f32, blue: f32, alpha: f32) {
    if x < 0 || y < 0 || (bw < 10_000 && (x >= bw || y >= bh)) {
        return;
    }
    let i = (y as i32 * stride + x * 4) as usize;
    if i + 3 >= buf.len() {
        return;
    }
    buf[i] = (blue * alpha * 255.0).round() as u8;
    buf[i + 1] = (green * alpha * 255.0).round() as u8;
    buf[i + 2] = (red * alpha * 255.0).round() as u8;
    buf[i + 3] = (alpha * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(x: i32, y: i32, w: i32, h: i32) -> Px {
        Px { x, y, w, h }
    }

    #[test]
    fn the_x_meets_the_highlight_and_a_tie_sits_above() {
        let highlight = mark(400, 300, 200, 40);
        let row = place_row(highlight, 1920, 1080);
        assert!(!row.above, "more room below");
        assert_eq!(row.deny.x + row.deny.w, highlight.x + highlight.w);
        assert_eq!(row.note.w, row.check.w);
        assert_eq!(row.check.w, row.deny.w);
        assert_eq!(row.note.h, ROW_PX);
        assert!(row.note.x < row.check.x && row.check.x < row.deny.x);
        assert_eq!(row.check.x - (row.note.x + row.note.w), ROW_GAP);
        assert_eq!(row.deny.x - (row.check.x + row.check.w), ROW_GAP);
        assert!(ROW_GAP < ROW_PX);
        assert!(!overlaps(row.note, highlight));
        assert!(!overlaps(row.check, highlight));
        assert!(!overlaps(row.deny, highlight));
        assert!(row.note.y >= highlight.y + highlight.h);

        let tied = mark(400, 520, 200, 40);
        let above = place_row(tied, 1920, 1080);
        assert!(above.above);
        assert_eq!(above.deny.x + above.deny.w, tied.x + tied.w);
        assert!(above.note.y + above.note.h <= tied.y);
        assert!(!overlaps(above.deny, tied));
    }

    #[test]
    fn hover_appends_no_hand_and_the_second_restarts_inside() {
        let mut row = ControlRow::new();
        let px = mark(400, 300, 200, 40);
        row.enter("lc-a", px, Some(UvBox { u: 0.2, v: 0.3, w: 0.1, h: 0.04 }), 1_000);
        assert!(row.is_open());
        assert_eq!(row.release(RowHit::Miss), Release::None);
        row.pointer(false, 1_200);
        assert!(row.tick(1_200 + CONTROL_ROW_MS - 1).is_none());
        assert!(row.is_open());
        assert!(row.tick(1_200 + CONTROL_ROW_MS).is_none());
        assert!(!row.is_open());

        row.enter("lc-a", px, None, 5_000);
        row.pointer(false, 5_100);
        row.pointer(true, 5_400);
        row.motion(5_400);
        assert!(row.is_open());
        assert!(row.tick(5_400 + CONTROL_ROW_MS - 1).is_none());
        assert!(row.is_open());
    }

    #[test]
    fn a_press_holds_the_row_and_a_slide_appends_nothing() {
        let mut row = ControlRow::new();
        row.enter("lc-a", mark(10, 10, 80, 20), None, 0);
        row.pointer(false, 10);
        row.press(RowHit::Check);
        assert!(row.tick(CONTROL_ROW_MS + 10).is_none());
        assert!(row.is_open());
        assert_eq!(row.release(RowHit::Miss), Release::None);
        row.press(RowHit::Check);
        assert_eq!(row.release(RowHit::Check), Release::Hand(RowButton::Check));
        row.press(RowHit::Tint);
        assert_eq!(row.release(RowHit::Tint), Release::Tint);
    }

    #[test]
    fn a_touch_reveal_writes_no_hand_and_uses_the_same_second() {
        let mut row = ControlRow::new();
        assert!(row.touch_reveal("lc-a", mark(10, 10, 40, 16), None, 0));
        assert!(row.is_open());
        assert_eq!(row.release(RowHit::Tint), Release::None);
        assert!(!row.touch_reveal("lc-a", mark(10, 10, 40, 16), None, 20));
        row.pointer(false, 20);
        assert!(row.tick(20 + CONTROL_ROW_MS).is_none());
        assert!(!row.is_open());
    }

    #[test]
    fn the_editor_drops_the_check_and_the_x() {
        let mut row = ControlRow::new();
        row.enter("lc-a", mark(400, 300, 200, 40), None, 0);
        row.open_editor("", None);
        let editor = place_editor(mark(400, 300, 200, 40), 1920, 1080, "");
        assert_eq!(editor.field.x + editor.field.w, 600);
        assert!(!overlaps(editor.back, mark(400, 300, 200, 40)));
        assert!(!overlaps(editor.field, mark(400, 300, 200, 40)));
        let paint = row.paint(MarkColor::Yellow, 1920, 1080).unwrap();
        assert!(paint.editor);
        assert!(paint.squares.iter().any(|(hit, _)| *hit == RowHit::Note));
        assert!(paint.squares.iter().all(|(hit, _)| !matches!(hit, RowHit::Check | RowHit::Deny)));
        let mut buf = vec![0u8; (paint.width * paint.height * 4) as usize];
        paint_buffer(&mut buf, &paint);
        assert!(buf.iter().any(|b| *b != 0), "the back icon and the field are drawn");
        row.press(RowHit::Note);
        assert_eq!(row.release(RowHit::Note), Release::Back);
    }

    #[test]
    fn the_check_and_the_x_follow_the_hand_table() {
        assert_eq!(button_hand(MarkColor::Yellow, RowButton::Check), (MarkColor::Green, "accept"));
        assert_eq!(button_hand(MarkColor::Red, RowButton::Check), (MarkColor::Green, "accept"));
        assert_eq!(button_hand(MarkColor::Green, RowButton::Check), (MarkColor::Yellow, "skip"));
        assert_eq!(button_hand(MarkColor::Yellow, RowButton::Deny), (MarkColor::Red, "deny"));
        assert_eq!(button_hand(MarkColor::Green, RowButton::Deny), (MarkColor::Red, "deny"));
        assert_eq!(button_hand(MarkColor::Red, RowButton::Deny), (MarkColor::Yellow, "skip"));
    }

    #[test]
    fn a_scroll_flushes_the_draft_and_hides_the_row() {
        let mut row = ControlRow::new();
        row.enter("lc-a", mark(10, 10, 80, 20), None, 0);
        row.open_editor("kept", Some("nt-1".into()));
        row.push_text(" more");
        let flush = row.scroll().unwrap();
        assert_eq!(flush.text, "kept more");
        assert_eq!(flush.replaces.as_deref(), Some("nt-1"));
        assert!(!row.is_open());
        row.enter("lc-a", mark(10, 10, 80, 20), None, 0);
        row.open_editor("", None);
        assert!(row.back().is_none());
        assert!(!row.editor_open());
    }

    fn glyph_ink(buf: &[u8], stride: i32, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 {
            return false;
        }
        let i = (y * stride + x * 4) as usize;
        i + 3 < buf.len() && buf[i] == 41 && buf[i + 1] == 31 && buf[i + 2] == 26 && buf[i + 3] == 255
    }

    fn glyph_drawn(buf: &[u8], stride: i32, x0: i32, y0: i32, ch: char) -> bool {
        let bits = glyph_bits(ch);
        if bits == 0 {
            return false;
        }
        for row in 0..5 {
            for col in 0..3 {
                let on = bits & (1 << (row * 3 + col)) != 0;
                if on != glyph_ink(buf, stride, x0 + col, y0 + row) {
                    return false;
                }
            }
        }
        true
    }

    fn ink_count(buf: &[u8]) -> usize {
        buf.chunks(4).filter(|px| px.len() == 4 && px[0] == 41 && px[1] == 31 && px[2] == 26 && px[3] == 255).count()
    }

    fn bits_on(ch: char) -> usize {
        let bits = glyph_bits(ch);
        (0..15).filter(|bit| bits & (1 << bit) != 0).count()
    }

    fn paint_of(text: &str, highlight: Px) -> (Paint, Vec<u8>) {
        let mut row = ControlRow::new();
        row.enter("lc-a", highlight, None, 0);
        row.open_editor(text, None);
        let paint = row.paint(MarkColor::Yellow, 1920, 1080).unwrap();
        let mut buf = vec![0u8; (paint.width * paint.height * 4) as usize];
        paint_buffer(&mut buf, &paint);
        (paint, buf)
    }

    #[test]
    fn a_newline_paints_the_next_line_and_not_the_block() {
        assert_eq!(glyph_bits('\n'), 0);
        let highlight = mark(400, 300, 200, 40);
        let (paint, buf) = paint_of("ab\ncd", highlight);
        let field = paint.squares.iter().find(|(hit, _)| *hit == RowHit::Field).unwrap().1;
        assert_eq!(field.h, ROW_PX * 2);
        let stride = paint.width * 4;
        let x0 = field.x - paint.origin_x + FIELD_INSET;
        let y0 = field.y - paint.origin_y + GLYPH_BASELINE;
        assert!(glyph_drawn(&buf, stride, x0, y0, 'a'));
        assert!(glyph_drawn(&buf, stride, x0 + GLYPH_STEP, y0, 'b'));
        assert!(glyph_drawn(&buf, stride, x0, y0 + ROW_PX, 'c'));
        assert!(glyph_drawn(&buf, stride, x0 + GLYPH_STEP, y0 + ROW_PX, 'd'));
        assert_eq!(ink_count(&buf), bits_on('a') + bits_on('b') + bits_on('c') + bits_on('d'));
    }

    #[test]
    fn the_field_grows_to_the_caret_and_stops_at_four_lines() {
        let highlight = mark(400, 300, 200, 40);
        let short = place_editor(highlight, 1920, 1080, "ab");
        assert_eq!(short.field.w, line_width(2));
        assert!(short.field.w < highlight.w / 2);
        assert_eq!(short.field.h, ROW_PX);
        assert_eq!(short.field.x + short.field.w, highlight.x + highlight.w);

        let long_text = "a".repeat(40) + "b";
        let long = place_editor(highlight, 1920, 1080, &long_text);
        let cap = (highlight.w / 2).max(ROW_PX + ROW_GAP + line_width(SHORT_RUN));
        assert!(long.field.w + ROW_PX + ROW_GAP <= cap);
        assert!(long.field.w > short.field.w);
        let (paint, buf) = paint_of(&long_text, highlight);
        let field = paint.squares.iter().find(|(hit, _)| *hit == RowHit::Field).unwrap().1;
        let stride = paint.width * 4;
        let fit = glyphs_that_fit(field.w);
        assert!(fit < long_text.chars().count());
        let x0 = field.x - paint.origin_x + FIELD_INSET;
        let y0 = field.y - paint.origin_y + GLYPH_BASELINE;
        let last_x = x0 + (fit as i32 - 1) * GLYPH_STEP;
        assert!(last_x + 3 < field.x - paint.origin_x + field.w);
        assert!(glyph_drawn(&buf, stride, x0, y0, 'a'));
        assert!(glyph_drawn(&buf, stride, last_x, y0, 'b'));
        assert_eq!(ink_count(&buf), bits_on('a') * (fit - 1) + bits_on('b'));

        let five = "a\nb\nc\nd\ne";
        let mut row = ControlRow::new();
        row.enter("lc-a", highlight, None, 0);
        row.open_editor(five, None);
        assert_eq!(row.draft(), five);
        let paint = row.paint(MarkColor::Yellow, 1920, 1080).unwrap();
        let field = paint.squares.iter().find(|(hit, _)| *hit == RowHit::Field).unwrap().1;
        assert_eq!(field.h, FIELD_LINES * ROW_PX);
        let mut buf = vec![0u8; (paint.width * paint.height * 4) as usize];
        paint_buffer(&mut buf, &paint);
        let stride = paint.width * 4;
        let x0 = field.x - paint.origin_x + FIELD_INSET;
        let y0 = field.y - paint.origin_y + GLYPH_BASELINE;
        assert!(glyph_drawn(&buf, stride, x0, y0, 'b'));
        assert!(!glyph_drawn(&buf, stride, x0, y0, 'a'));
        assert!(glyph_drawn(&buf, stride, x0, y0 + 3 * ROW_PX, 'e'));

        let more = "a\nb\nc\nd\ne\n";
        row.open_editor(more, None);
        assert_eq!(row.draft(), more);
        let grown = place_editor(highlight, 1920, 1080, more);
        assert_eq!(grown.field.h, FIELD_LINES * ROW_PX);
        let (paint, buf) = paint_of(more, highlight);
        let field = paint.squares.iter().find(|(hit, _)| *hit == RowHit::Field).unwrap().1;
        let stride = paint.width * 4;
        let x0 = field.x - paint.origin_x + FIELD_INSET;
        let y0 = field.y - paint.origin_y + GLYPH_BASELINE;
        assert!(glyph_drawn(&buf, stride, x0, y0, 'c'));
        assert!(glyph_drawn(&buf, stride, x0, y0 + ROW_PX, 'd'));
        assert!(glyph_drawn(&buf, stride, x0, y0 + 2 * ROW_PX, 'e'));
        assert_eq!(ink_count(&buf), bits_on('c') + bits_on('d') + bits_on('e'));

        let narrow = mark(10, 10, 20, 10);
        let kept = place_editor(narrow, 1920, 1080, "a");
        let minimum = ROW_PX + ROW_GAP + line_width(SHORT_RUN);
        assert!(narrow.w / 2 < minimum);
        assert_eq!(kept.field.w, line_width(SHORT_RUN));
        assert_eq!(kept.back.w + ROW_GAP + kept.field.w, minimum);
    }

    #[test]
    fn a_click_on_the_drawn_back_square_closes_after_the_layer_moves() {
        let highlight = mark(45, 317, 803, 10);
        let pad = place_row(highlight, 1920, 1080).note;
        assert_eq!(pad.x, 748);
        let mut row = ControlRow::new();
        row.enter("lc-fb6ee93cbdf55d1a", highlight, None, 0);
        row.open_editor("ab\ncd", None);
        let paint = row.paint(MarkColor::Yellow, 1920, 1080).unwrap();
        let back = paint.squares.iter().find(|(hit, _)| *hit == RowHit::Note).unwrap().1;
        let field = paint.squares.iter().find(|(hit, _)| *hit == RowHit::Field).unwrap().1;
        assert!(back.x + back.w <= field.x);
        assert_ne!(back.x, pad.x);
        assert_eq!(back.x, paint.origin_x);
        assert_eq!(back.y, paint.origin_y);
        assert!(paint.squares.iter().all(|(hit, _)| !matches!(hit, RowHit::Check | RowHit::Deny)));
        let layer = (paint.origin_x, paint.origin_y);
        let stale = row_screen_point((pad.x, pad.y), layer, 4, 4);
        let updated = row_screen_point(layer, layer, 4, 4);
        assert_eq!(stale, updated);
        assert_ne!(pad.x + 4, stale.0);
        assert!(stale.0 >= back.x && stale.0 < back.x + back.w);
        assert!(stale.1 >= back.y && stale.1 < back.y + back.h);
        assert_eq!(row.hit(stale.0, stale.1, 1920, 1080), RowHit::Note);
        assert_eq!(row.hit(updated.0, updated.1, 1920, 1080), RowHit::Note);
        row.push_text("!");
        row.press(RowHit::Note);
        assert_eq!(row.release(RowHit::Note), Release::Back);
        let flush = row.back().expect("the latest text");
        assert_eq!(flush.text, "ab\ncd!");
        assert!(!row.editor_open());

        let overlay = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/overlay.rs")).unwrap();
        assert!(overlay.contains("row_screen_point"));
        let surface = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/overlay/row_surface.rs")).unwrap();
        assert!(surface.contains("pointer_origin"));
        assert!(surface.contains("on_row"));
    }
}
