//! One screen box per locus. One presence points at one of them.
//! The numbers stay here, on the CPU.
//! A mark layer reads them only while it is supposed to be visible.
//! After that the box remains, so a later gesture can aim at it.

use std::time::{Duration, Instant};

pub const MAX_SECS: u32 = 360;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvBox {
    pub u: f32,
    pub v: f32,
    pub w: f32,
    pub h: f32,
}

struct Interest {
    id: String,
    box_: UvBox,
    shown_until: Instant,
    /// The mark stays up for the utterance, then the speech-finished path conceals it.
    until_speech: bool,
    /// Stays up until conceal. A timer and the end of speech do not take it down.
    held: bool,
}

pub struct InterestBook {
    items: Vec<Interest>,
}

impl InterestBook {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Replace this presence's box. `secs` is clamped to 1..=360.
    /// Origin is the top left of the whole screen. `v` grows downward.
    pub fn declare(&mut self, id: &str, raw: UvBox, secs: u32, now: Instant) -> Result<UvBox, String> {
        let box_ = sanitize(raw)?;
        let secs = secs.clamp(1, MAX_SECS);
        let shown_until = now + Duration::from_secs(secs as u64);
        if let Some(existing) = self.items.iter_mut().find(|it| it.id == id) {
            existing.box_ = box_;
            existing.shown_until = shown_until;
            existing.until_speech = false;
            existing.held = false;
        } else {
            self.items.push(Interest { id: id.to_string(), box_, shown_until, until_speech: false, held: false });
        }
        Ok(box_)
    }

    /// This presence's mark fades when its utterance ends, not on the countdown.
    /// The safety cap from `declare` still applies if speech never finishes.
    pub fn hold_for_speech(&mut self, id: &str) {
        if let Some(existing) = self.items.iter_mut().find(|it| it.id == id) {
            existing.until_speech = true;
            existing.held = false;
        }
    }

    /// The mark stays until it is retired. Speech ending and the lease do not hide it.
    pub fn hold_until_conceal(&mut self, id: &str) {
        if let Some(existing) = self.items.iter_mut().find(|it| it.id == id) {
            existing.held = true;
            existing.until_speech = false;
        }
    }

    /// Hide the mark and keep the box. A timed mark is left alone.
    pub fn conceal_if_speech_held(&mut self, id: &str, now: Instant) -> bool {
        if let Some(existing) = self.items.iter_mut().find(|it| it.id == id && it.until_speech) {
            existing.shown_until = now;
            existing.until_speech = false;
            return true;
        }
        false
    }

    pub fn retire(&mut self, id: &str) {
        self.items.retain(|it| it.id != id);
    }

    /// The declared box, including after the mark has faded.
    pub fn get(&self, id: &str) -> Option<UvBox> {
        self.items.iter().find(|it| it.id == id).map(|it| it.box_)
    }

    /// Boxes whose mark should be on screen right now.
    /// Every held or unexpired mark. The aim is one of these.
    pub fn visible(&self, now: Instant) -> Vec<(String, UvBox)> {
        self.items
            .iter()
            .filter(|it| it.held || it.shown_until > now)
            .map(|it| (it.id.clone(), it.box_))
            .collect()
    }
}

/// Pixel rects for the heat field. Only aim marks. Companions stay paint.
/// Width or height of 1 or less is stored as 2, matching the mask. Empty slots stay 0.
pub fn aim_heat_rects(measured: &[(String, i32, i32, i32, i32)], aim_ids: &[String]) -> [[f32; 4]; 3] {
    let mut rects = [[0.0f32; 4]; 3];
    let mut filled = 0usize;
    let mut seen: Vec<&str> = Vec::new();
    for aim in aim_ids {
        if filled >= rects.len() {
            break;
        }
        if seen.iter().any(|id| *id == aim.as_str()) {
            continue;
        }
        let Some((_, x, y, w, h)) = measured.iter().find(|(id, _, _, _, _)| id == aim) else {
            continue;
        };
        seen.push(aim.as_str());
        let rw = if *w <= 1 { 2.0 } else { *w as f32 };
        let rh = if *h <= 1 { 2.0 } else { *h as f32 };
        rects[filled] = [*x as f32, *y as f32, rw, rh];
        filled += 1;
    }
    rects
}

impl Default for InterestBook {
    fn default() -> Self {
        Self::new()
    }
}

fn sanitize(raw: UvBox) -> Result<UvBox, String> {
    let UvBox { u, v, w, h } = raw;
    if ![u, v, w, h].iter().all(|n| n.is_finite()) {
        return Err("highlight box must be finite".into());
    }
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return Err("highlight origin must lie on the screen (0..1)".into());
    }
    if w <= 0.0 || h <= 0.0 {
        return Err("highlight box must have a positive size".into());
    }
    let w = w.min(1.0 - u);
    let h = h.min(1.0 - v);
    if w < 1.0 / 4096.0 || h < 1.0 / 4096.0 {
        return Err("highlight box is empty".into());
    }
    Ok(UvBox { u, v, w, h })
}

/// Corner radius of the painted mark, in pixels. The paint pad uses the same curve.
pub fn mark_corner_radius(w: i32, h: i32) -> f32 {
    let w = w.max(1) as f32;
    let h = h.max(1) as f32;
    16.0_f32.min(w * 0.22).min(h * 0.22).max(3.0)
}

/// Grow a measured pixel box so the edge and bevel sit outside the content.
/// A side that would leave the screen does not grow. The book keeps the measured box.
pub fn expand_paint_rect(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    screen_w: u32,
    screen_h: u32,
) -> (i32, i32, i32, i32) {
    let pad = mark_corner_radius(w, h).ceil() as i32 + 2;
    let sw = screen_w.max(1) as i32;
    let sh = screen_h.max(1) as i32;
    let x2 = (x - pad).max(0);
    let y2 = (y - pad).max(0);
    let right = x.saturating_add(w).saturating_add(pad).min(sw);
    let bottom = y.saturating_add(h).saturating_add(pad).min(sh);
    let w2 = (right - x2).max(1);
    let h2 = (bottom - y2).max(1);
    (x2, y2, w2, h2)
}

/// Full-screen UV to a pixel rectangle. `v` is down, matching the screen.
pub fn to_pixels(box_: UvBox, screen_w: u32, screen_h: u32) -> (i32, i32, i32, i32) {
    let sw = screen_w.max(1) as f32;
    let sh = screen_h.max(1) as f32;
    let mut x = (box_.u * sw).round() as i32;
    let mut y = (box_.v * sh).round() as i32;
    let mut w = (box_.w * sw).round().max(1.0) as i32;
    let mut h = (box_.h * sh).round().max(1.0) as i32;
    let swi = screen_w.max(1) as i32;
    let shi = screen_h.max(1) as i32;
    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }
    if x + w > swi {
        w = swi - x;
    }
    if y + h > shi {
        h = shi - y;
    }
    (x, y, w.max(1), h.max(1))
}

/// True when the two pixel rectangles share a pixel.
/// An edge that only touches does not. An empty side does not.
pub fn rects_share_a_pixel(ax: i32, ay: i32, aw: i32, ah: i32, bx: i32, by: i32, bw: i32, bh: i32) -> bool {
    if aw <= 0 || ah <= 0 || bw <= 0 || bh <= 0 {
        return false;
    }
    let ax2 = ax.saturating_add(aw);
    let ay2 = ay.saturating_add(ah);
    let bx2 = bx.saturating_add(bw);
    let by2 = by.saturating_add(bh);
    ax < bx2 && bx < ax2 && ay < by2 && by < ay2
}

/// The three conditions of the highlight. Yellow is the tint that already ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkColor {
    Yellow,
    Green,
    Red,
}

impl MarkColor {
    /// Premultiplied factors, blue, green, red. Alpha and radius stay outside.
    pub fn factors(self) -> (f32, f32, f32) {
        match self {
            Self::Yellow => (0.22, 0.68, 0.95),
            Self::Green => (0.25, 0.72, 0.28),
            Self::Red => (0.22, 0.28, 0.78),
        }
    }
}

/// Premultiplied BGRA into a Wayland SHM buffer. Alpha is 0 outside a
/// rounded rectangle, so the square's corners stay pass-through.
/// `phase` is seconds. The edge breathes. The fill stays low.
pub fn paint_mark(buf: &mut [u8], width: i32, height: i32, stride: i32, phase: f32, color: MarkColor) {
    let (blue, green, red) = color.factors();
    let w = width.max(1);
    let h = height.max(1);
    let pulse = (phase * 1.7).sin() * 0.5 + 0.5;
    let radius = mark_corner_radius(w, h);
    for y in 0..h {
        for x in 0..w {
            let a = mark_alpha(x, y, w, h, radius, pulse, phase);
            let i = (y as i32 * stride + x * 4) as usize;
            if i + 3 >= buf.len() {
                continue;
            }
            buf[i] = (blue * a * 255.0).round() as u8;
            buf[i + 1] = (green * a * 255.0).round() as u8;
            buf[i + 2] = (red * a * 255.0).round() as u8;
            buf[i + 3] = (a * 255.0).round() as u8;
        }
    }
}

fn mark_alpha(x: i32, y: i32, w: i32, h: i32, radius: f32, pulse: f32, phase: f32) -> f32 {
    let px = x as f32 + 0.5;
    let py = y as f32 + 0.5;
    let hw = w as f32 * 0.5;
    let hh = h as f32 * 0.5;
    let qx = (px - hw).abs() - (hw - radius);
    let qy = (py - hh).abs() - (hh - radius);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let inside = qx.max(qy).min(0.0);
    let sd = outside + inside - radius;
    if sd > 1.25 {
        return 0.0;
    }
    let cover = (1.0 - sd.max(0.0) / 1.25).clamp(0.0, 1.0);
    let band = smoothstep(14.0, 1.5, -sd) * (1.0 - smoothstep(0.0, 2.2, sd));
    let fill = (0.05 + 0.03 * pulse) * cover;
    let edge = (0.34 + 0.16 * pulse) * band * cover;
    let travel = (px / w as f32 + phase * 0.12).fract();
    let streak = smoothstep(0.07, 0.0, (travel - 0.15).abs()) * band * cover * 0.22;
    (fill + edge + streak).clamp(0.0, 1.0)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_at(u: f32, v: f32, w: f32, h: f32) -> UvBox {
        UvBox { u, v, w, h }
    }

    #[test]
    fn a_box_outlives_its_mark_and_retire_drops_it() {
        let mut book = InterestBook::new();
        let t0 = Instant::now();
        let got = book.declare("p-a", box_at(0.1, 0.2, 0.3, 0.4), 5, t0).unwrap();
        assert_eq!(got, box_at(0.1, 0.2, 0.3, 0.4));
        assert_eq!(book.visible(t0 + Duration::from_secs(4)).len(), 1);
        assert!(book.visible(t0 + Duration::from_secs(5)).is_empty());
        assert_eq!(book.get("p-a"), Some(box_at(0.1, 0.2, 0.3, 0.4)));
        book.retire("p-a");
        assert_eq!(book.get("p-a"), None);
    }

    #[test]
    fn duration_caps_at_six_minutes_and_a_new_call_replaces_the_box() {
        let mut book = InterestBook::new();
        let t0 = Instant::now();
        book.declare("p-a", box_at(0.0, 0.0, 0.2, 0.2), 600, t0).unwrap();
        assert_eq!(book.visible(t0 + Duration::from_secs(360)).len(), 0);
        assert_eq!(book.visible(t0 + Duration::from_secs(359)).len(), 1);
        book.declare("p-a", box_at(0.5, 0.5, 0.9, 0.9), 5, t0).unwrap();
        assert_eq!(book.get("p-a").unwrap().u, 0.5);
        assert!(book.get("p-a").unwrap().w <= 0.5);
    }

    #[test]
    fn a_held_mark_stays_past_the_lease_until_it_is_retired() {
        let mut book = InterestBook::new();
        let t0 = Instant::now();
        book.declare("p-a", box_at(0.1, 0.2, 0.3, 0.4), 1, t0).unwrap();
        book.hold_until_conceal("p-a");
        assert!(!book.conceal_if_speech_held("p-a", t0));
        assert_eq!(book.visible(t0 + Duration::from_secs(10_000)).len(), 1);
        book.declare("p-a", box_at(0.2, 0.2, 0.2, 0.2), 1, t0).unwrap();
        book.hold_until_conceal("p-a");
        assert_eq!(book.get("p-a").unwrap().u, 0.2);
        assert_eq!(book.visible(t0 + Duration::from_secs(10_000)).len(), 1);
        book.retire("p-a");
        assert!(book.visible(t0).is_empty());
    }

    #[test]
    fn a_speech_hold_fades_when_the_utterance_ends_and_a_timed_mark_does_not() {
        let mut book = InterestBook::new();
        let t0 = Instant::now();
        book.declare("p-a", box_at(0.1, 0.2, 0.3, 0.4), 360, t0).unwrap();
        book.hold_for_speech("p-a");
        book.declare("p-b", box_at(0.2, 0.2, 0.2, 0.2), 30, t0).unwrap();
        assert!(!book.conceal_if_speech_held("p-b", t0));
        assert_eq!(book.visible(t0).len(), 2, "a timed mark is not a speech hold");
        assert!(book.conceal_if_speech_held("p-a", t0));
        let visible = book.visible(t0);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].0, "p-b");
        assert_eq!(book.get("p-a").unwrap().u, 0.1, "the box remains after the tint drops");
    }

    #[test]
    fn origin_off_the_screen_is_refused() {
        let mut book = InterestBook::new();
        let err = book.declare("p-a", box_at(-0.1, 0.0, 0.2, 0.2), 5, Instant::now());
        assert!(err.is_err());
    }

    #[test]
    fn pixels_cover_the_full_screen_and_corners_stay_clear() {
        let (x, y, w, h) = to_pixels(box_at(0.0, 0.0, 1.0, 1.0), 1920, 1080);
        assert_eq!((x, y, w, h), (0, 0, 1920, 1080));
        let (x, y, w, h) = to_pixels(box_at(0.25, 0.5, 0.5, 0.25), 1000, 800);
        assert_eq!((x, y), (250, 400));
        assert_eq!((w, h), (500, 200));

        let stride = 40 * 4;
        let mut buf = vec![7u8; (stride * 40) as usize];
        paint_mark(&mut buf, 40, 40, stride, 0.0, MarkColor::Yellow);
        assert_eq!(buf[3], 0, "corner of the square stays pass-through");
        let mid = (20 * stride + 20 * 4) as usize;
        assert!(buf[mid + 3] > 0, "the middle of the box is tinted");
    }

    #[test]
    fn an_overlapping_box_shares_a_pixel_and_a_touching_edge_does_not() {
        // The allyl readings. 833×14 sits inside 838×17. 768×30 shares the line.
        assert!(rects_share_a_pixel(983, 661, 838, 17, 988, 664, 833, 14));
        assert!(rects_share_a_pixel(983, 661, 838, 17, 1048, 655, 768, 30));
        assert!(rects_share_a_pixel(988, 664, 833, 14, 1048, 655, 768, 30));
        // The right edge of 983+838 meets the next pixel and does not share one.
        assert!(!rects_share_a_pixel(983, 661, 838, 17, 1821, 661, 10, 17));
        assert!(!rects_share_a_pixel(983, 661, 838, 17, 100, 100, 40, 12));
        assert!(!rects_share_a_pixel(983, 661, 0, 17, 983, 661, 10, 17));
        assert!(!rects_share_a_pixel(10, 10, 4, 4, 14, 10, 4, 4));
    }

    fn paint_pad(w: i32, h: i32) -> i32 {
        mark_corner_radius(w, h).ceil() as i32 + 2
    }

    #[test]
    fn expand_paint_grows_an_interior_box_on_every_side() {
        let (x, y, w, h) = (400, 300, 200, 80);
        let pad = paint_pad(w, h);
        let (x2, y2, w2, h2) = expand_paint_rect(x, y, w, h, 1920, 1080);
        assert_eq!((x2, y2, w2, h2), (x - pad, y - pad, w + 2 * pad, h + 2 * pad));
        assert!(x2 < x && y2 < y && x2 + w2 > x + w && y2 + h2 > y + h);
    }

    #[test]
    fn expand_paint_holds_the_left_edge_on_the_screen() {
        let (w, h) = (200, 80);
        let pad = paint_pad(w, h);
        let (x2, y2, w2, h2) = expand_paint_rect(0, 300, w, h, 1920, 1080);
        assert_eq!(x2, 0);
        assert_eq!(y2, 300 - pad);
        assert_eq!(w2, w + pad);
        assert_eq!(h2, h + 2 * pad);
        assert!(x2 + w2 <= 1920 && y2 + h2 <= 1080);
    }

    #[test]
    fn expand_paint_holds_the_top_left_corner_on_the_screen() {
        let (w, h) = (200, 80);
        let pad = paint_pad(w, h);
        let (x2, y2, w2, h2) = expand_paint_rect(0, 0, w, h, 1920, 1080);
        assert_eq!((x2, y2), (0, 0));
        assert_eq!((w2, h2), (w + pad, h + pad));
    }

    #[test]
    fn expand_paint_holds_a_flush_far_edge_on_the_screen() {
        let (w, h) = (200, 80);
        let pad = paint_pad(w, h);
        let (x2, y2, w2, h2) = expand_paint_rect(1920 - w, 300, w, h, 1920, 1080);
        assert_eq!(x2, 1920 - w - pad);
        assert_eq!(y2, 300 - pad);
        assert_eq!(x2 + w2, 1920);
        assert_eq!(h2, h + 2 * pad);
        let (x2, y2, w2, h2) = expand_paint_rect(400, 1080 - h, w, h, 1920, 1080);
        assert_eq!(y2 + h2, 1080);
        assert_eq!(x2, 400 - pad);
        assert_eq!(w2, w + 2 * pad);
        assert_eq!(y2, 1080 - h - pad);
    }

    #[test]
    fn expand_paint_leaves_a_full_output_box_unchanged() {
        let got = expand_paint_rect(0, 0, 1920, 1080, 1920, 1080);
        assert_eq!(got, (0, 0, 1920, 1080));
    }

    #[test]
    fn expand_paint_does_not_replace_the_box_the_book_stores() {
        let mut book = InterestBook::new();
        let t0 = Instant::now();
        let raw = box_at(400.0 / 1920.0, 300.0 / 1080.0, 200.0 / 1920.0, 80.0 / 1080.0);
        let stored = book.declare("p-a", raw, 5, t0).unwrap();
        assert_eq!(stored, raw);
        let (x, y, w, h) = to_pixels(book.get("p-a").unwrap(), 1920, 1080);
        let (x2, y2, w2, h2) = expand_paint_rect(x, y, w, h, 1920, 1080);
        assert!(w2 > w && h2 > h && x2 < x && y2 < y);
        assert_eq!(book.get("p-a"), Some(raw));
        assert_eq!(book.visible(t0)[0].1, raw);
    }

    #[test]
    fn four_held_marks_all_stay_visible() {
        let mut book = InterestBook::new();
        let t0 = Instant::now();
        for i in 0..4 {
            let id = format!("lc-{i}");
            book.declare(&id, box_at(0.1, 0.1, 0.2, 0.2), 1, t0).unwrap();
            book.hold_until_conceal(&id);
        }
        let later = t0 + Duration::from_secs(10_000);
        assert_eq!(book.visible(later).len(), 4);
    }

    #[test]
    fn heat_rects_are_the_aim_while_companions_show() {
        let measured = vec![
            ("lc-1".to_string(), 10, 20, 30, 40),
            ("lc-2".to_string(), 50, 60, 70, 80),
            ("lc-3".to_string(), 90, 100, 1, 12),
        ];
        let rects = aim_heat_rects(&measured, &["lc-2".to_string()]);
        assert_eq!(rects[0], [50.0, 60.0, 70.0, 80.0]);
        assert_eq!(rects[1], [0.0; 4]);
        assert_eq!(rects[2], [0.0; 4]);
        let thin = aim_heat_rects(&measured, &["lc-3".to_string()]);
        assert_eq!(thin[0], [90.0, 100.0, 2.0, 12.0]);
        assert_eq!(thin[1], [0.0; 4]);
    }
}
