//! One screen box per presence. The numbers stay here, on the CPU.
//! A mark layer reads them only while it is supposed to be visible.
//! After that the box remains, so a later gesture can aim at it.

use std::time::{Duration, Instant};

pub const MAX_SECS: u32 = 360;
pub const MAX_MARKS: usize = 3;

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
        } else {
            self.items.push(Interest { id: id.to_string(), box_, shown_until });
        }
        Ok(box_)
    }

    pub fn retire(&mut self, id: &str) {
        self.items.retain(|it| it.id != id);
    }

    /// The declared box, including after the mark has faded.
    pub fn get(&self, id: &str) -> Option<UvBox> {
        self.items.iter().find(|it| it.id == id).map(|it| it.box_)
    }

    /// Boxes whose mark should be on screen right now. At most three,
    /// the same cap as live presences.
    pub fn visible(&self, now: Instant) -> Vec<(String, UvBox)> {
        self.items
            .iter()
            .filter(|it| it.shown_until > now)
            .take(MAX_MARKS)
            .map(|it| (it.id.clone(), it.box_))
            .collect()
    }
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

/// Premultiplied BGRA into a Wayland SHM buffer. Alpha is 0 outside a
/// rounded rectangle, so the square's corners stay pass-through.
/// `phase` is seconds. The edge breathes. The fill stays low.
pub fn paint_mark(buf: &mut [u8], width: i32, height: i32, stride: i32, phase: f32) {
    let w = width.max(1);
    let h = height.max(1);
    let pulse = (phase * 1.7).sin() * 0.5 + 0.5;
    let radius = 16.0_f32.min(w as f32 * 0.22).min(h as f32 * 0.22).max(3.0);
    for y in 0..h {
        for x in 0..w {
            let a = mark_alpha(x, y, w, h, radius, pulse, phase);
            let i = (y as i32 * stride + x * 4) as usize;
            if i + 3 >= buf.len() {
                continue;
            }
            // Warm amber, premultiplied.
            buf[i] = (0.22 * a * 255.0).round() as u8;
            buf[i + 1] = (0.68 * a * 255.0).round() as u8;
            buf[i + 2] = (0.95 * a * 255.0).round() as u8;
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
        paint_mark(&mut buf, 40, 40, stride, 0.0);
        assert_eq!(buf[3], 0, "corner of the square stays pass-through");
        let mid = (20 * stride + 20 * 4) as usize;
        assert!(buf[mid + 3] > 0, "the middle of the box is tinted");
    }
}
