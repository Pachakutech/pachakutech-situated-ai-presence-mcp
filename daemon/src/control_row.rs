//! Hover row on one highlight. Pad, check, and X. The frame loop paints it.
//! The gesture path does not sleep. `tick` hides the row when the second
//! elapses, and a scroll hides it at once.

use crate::highlight::{MarkColor, UvBox};

/// How long the row stays after the pointer leaves it. The daemon clock measures it.
pub const CONTROL_ROW_MS: u64 = 1000;
pub const ROW_PX: i32 = 28;
/// Even gap, smaller than the square.
pub const ROW_GAP: i32 = 8;

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
            let editor = place_editor(row.px, screen_w, screen_h);
            let origin_x = editor.back.x;
            let origin_y = editor.back.y;
            let right = editor.field.x + editor.field.w;
            return Some(Paint {
                phrase_id: row.phrase_id.clone(),
                above: editor.above,
                editor: true,
                color,
                origin_x,
                origin_y,
                width: (right - origin_x).max(1),
                height: ROW_PX,
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

/// The pad square grows into the field. Check and X are gone. The field's
/// right edge is the highlight's right edge.
pub fn place_editor(mark: Px, screen_w: i32, screen_h: i32) -> EditorLayout {
    let row = place_row(mark, screen_w, screen_h);
    let right = mark.x.saturating_add(mark.w);
    let span = ROW_PX * 3 + ROW_GAP * 2;
    let width = span.max(240).min(screen_w.max(ROW_PX));
    let left = (right - width).max(0);
    let y = row.note.y;
    let back = Px { x: left, y, w: ROW_PX, h: ROW_PX };
    let field_x = left + ROW_PX + ROW_GAP;
    let field_w = (right - field_x).max(1);
    EditorLayout { above: row.above, back, field: Px { x: field_x, y, w: field_w, h: ROW_PX } }
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
    let mut cursor = 6;
    for ch in text.chars().take(48) {
        draw_glyph(buf, stride, x0 + cursor, y0 + 8, ch);
        cursor += 6;
        if cursor + 6 >= w {
            break;
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
        ' ' => 0,
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
        let editor = place_editor(mark(400, 300, 200, 40), 1920, 1080);
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
}
