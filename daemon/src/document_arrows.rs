//! Back and forward bubbles at the ends of the scroll.
//! They are not a hand, and they are not the note field's back icon.
//! A click posts the source step. It does not write skip, accept, or deny.

use crate::highlight::MarkColor;
use crate::protocol::ReportDocEdgeBody;
use crate::ui_input::Hotkey;

/// Faint fill. The page reads through it. Below half.
pub const EDGE_ALPHA: f32 = 0.28;
/// The pointer is inside the bubble. Higher than [`EDGE_ALPHA`].
pub const EDGE_HOVER_ALPHA: f32 = 0.62;
/// Quiet blue. Not the yellow, green, or red of a hand.
pub const EDGE_RED: f32 = 0.45;
pub const EDGE_GREEN: f32 = 0.58;
pub const EDGE_BLUE: f32 = 0.72;
/// Diameter, and the inset from the corner.
pub const BUBBLE_PX: i32 = 44;
/// A reading within this many pixels of an end counts as that end.
pub const SCROLL_SLOP: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Back,
    Forward,
}

/// What the driver read. A failure hides the arrow that needed the number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeRead {
    Web { scroll_top: f64, scroll_height: f64, client_height: f64 },
    /// `pages` absent means the file's count was not read. The forward arrow hides.
    Pdf { page: u32, pages: Option<u32> },
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ends {
    /// `None` hides the back arrow. `Some(true)` is the top.
    pub top: Option<bool>,
    /// `None` hides the forward arrow. `Some(true)` is the bottom.
    pub bottom: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BubblePaint {
    pub which: Arrow,
    pub origin_x: i32,
    pub origin_y: i32,
    pub diameter: i32,
    pub hover: bool,
}

/// Pointer samples on a bubble. The frame loop drains them before it paints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArrowInput {
    Pointer { x: i32, y: i32 },
    Leave,
    Press { x: i32, y: i32 },
    Release { x: i32, y: i32 },
}

#[derive(Clone, Debug)]
pub struct DocumentArrows {
    read: EdgeRead,
    prev: bool,
    next: bool,
    hover: Option<Arrow>,
    press: Option<Arrow>,
}

impl Default for DocumentArrows {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentArrows {
    pub fn new() -> Self {
        Self {
            read: EdgeRead::Failed,
            prev: false,
            next: false,
            hover: None,
            press: None,
        }
    }

    pub fn note_read(&mut self, read: EdgeRead) {
        self.read = read;
        self.drop_hidden();
    }

    pub fn note_neighbors(&mut self, prev: bool, next: bool) {
        self.prev = prev;
        self.next = next;
        self.drop_hidden();
    }

    pub fn showing(&self) -> (bool, bool) {
        show(&ends_of(&self.read), self.prev, self.next)
    }

    pub fn hover_at(&mut self, x: i32, y: i32, screen_w: i32, screen_h: i32) {
        self.hover = self.hit(x, y, screen_w, screen_h);
    }

    pub fn leave(&mut self) {
        self.hover = None;
    }

    pub fn press_at(&mut self, x: i32, y: i32, screen_w: i32, screen_h: i32) {
        self.press = self.hit(x, y, screen_w, screen_h);
    }

    /// A release on the bubble that was pressed. A slide off writes nothing.
    pub fn release_at(&mut self, x: i32, y: i32, screen_w: i32, screen_h: i32) -> Option<Arrow> {
        let pressed = self.press.take();
        let hit = self.hit(x, y, screen_w, screen_h);
        match (pressed, hit) {
            (Some(pressed), Some(hit)) if pressed == hit => Some(pressed),
            _ => None,
        }
    }

    pub fn bubbles(&self, screen_w: i32, screen_h: i32) -> Vec<BubblePaint> {
        let (back, forward) = self.showing();
        let mut out = Vec::new();
        if back {
            if let Some(paint) = bubble(Arrow::Back, screen_w, screen_h, self.hover) {
                out.push(paint);
            }
        }
        if forward {
            if let Some(paint) = bubble(Arrow::Forward, screen_w, screen_h, self.hover) {
                out.push(paint);
            }
        }
        out
    }

    fn hit(&self, x: i32, y: i32, screen_w: i32, screen_h: i32) -> Option<Arrow> {
        let (back, forward) = self.showing();
        if back {
            if let Some(px) = place(Arrow::Back, screen_w, screen_h) {
                if inside(&px, x, y) {
                    return Some(Arrow::Back);
                }
            }
        }
        if forward {
            if let Some(px) = place(Arrow::Forward, screen_w, screen_h) {
                if inside(&px, x, y) {
                    return Some(Arrow::Forward);
                }
            }
        }
        None
    }

    fn drop_hidden(&mut self) {
        let (back, forward) = self.showing();
        if self.hover == Some(Arrow::Back) && !back {
            self.hover = None;
        }
        if self.hover == Some(Arrow::Forward) && !forward {
            self.hover = None;
        }
        if self.press == Some(Arrow::Back) && !back {
            self.press = None;
        }
        if self.press == Some(Arrow::Forward) && !forward {
            self.press = None;
        }
    }
}

/// The source step this bubble posts. It is not [`Hotkey::Back`].
pub fn arrow_hotkey(which: Arrow) -> Hotkey {
    match which {
        Arrow::Back => Hotkey::PrevSource,
        Arrow::Forward => Hotkey::NextSource,
    }
}

pub fn edge_from_report(body: &ReportDocEdgeBody) -> EdgeRead {
    match body.surface.as_str() {
        "web" => match (body.scroll_top, body.scroll_height, body.client_height) {
            (Some(scroll_top), Some(scroll_height), Some(client_height)) => {
                EdgeRead::Web { scroll_top, scroll_height, client_height }
            }
            _ => EdgeRead::Failed,
        },
        "pdf" => match body.page {
            Some(page) => EdgeRead::Pdf { page, pages: body.pages },
            None => EdgeRead::Failed,
        },
        _ => EdgeRead::Failed,
    }
}

pub fn ends_of(read: &EdgeRead) -> Ends {
    match *read {
        EdgeRead::Failed => Ends { top: None, bottom: None },
        EdgeRead::Web { scroll_top, scroll_height, client_height } => web_ends(scroll_top, scroll_height, client_height),
        EdgeRead::Pdf { page, pages } => pdf_ends(page, pages),
    }
}

pub fn show(ends: &Ends, prev: bool, next: bool) -> (bool, bool) {
    (ends.top == Some(true) && prev, ends.bottom == Some(true) && next)
}

fn web_ends(scroll_top: f64, scroll_height: f64, client_height: f64) -> Ends {
    if !scroll_top.is_finite() || !scroll_height.is_finite() || !client_height.is_finite() || client_height <= 0.0 || scroll_height < 0.0 {
        return Ends { top: None, bottom: None };
    }
    let at_top = scroll_top.abs() <= SCROLL_SLOP;
    let max_scroll = (scroll_height - client_height).max(0.0);
    let at_bottom = (scroll_top - max_scroll).abs() <= SCROLL_SLOP;
    Ends { top: Some(at_top), bottom: Some(at_bottom) }
}

fn pdf_ends(page: u32, pages: Option<u32>) -> Ends {
    if page == 0 {
        return Ends { top: None, bottom: None };
    }
    let bottom = match pages {
        Some(count) if count >= 1 && page <= count => Some(page == count),
        _ => None,
    };
    Ends { top: Some(page == 1), bottom }
}

struct Px {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

fn place(which: Arrow, screen_w: i32, screen_h: i32) -> Option<Px> {
    if screen_w < BUBBLE_PX * 3 || screen_h < BUBBLE_PX * 3 {
        return None;
    }
    let inset = BUBBLE_PX;
    let px = match which {
        Arrow::Back => Px { x: inset, y: inset, w: BUBBLE_PX, h: BUBBLE_PX },
        Arrow::Forward => Px {
            x: screen_w - inset - BUBBLE_PX,
            y: screen_h - inset - BUBBLE_PX,
            w: BUBBLE_PX,
            h: BUBBLE_PX,
        },
    };
    Some(px)
}

fn bubble(which: Arrow, screen_w: i32, screen_h: i32, hover: Option<Arrow>) -> Option<BubblePaint> {
    let px = place(which, screen_w, screen_h)?;
    Some(BubblePaint {
        which,
        origin_x: px.x,
        origin_y: px.y,
        diameter: px.w,
        hover: hover == Some(which),
    })
}

fn inside(px: &Px, x: i32, y: i32) -> bool {
    let cx = px.x as f32 + px.w as f32 * 0.5;
    let cy = px.y as f32 + px.h as f32 * 0.5;
    let radius = px.w as f32 * 0.5;
    let dx = x as f32 + 0.5 - cx;
    let dy = y as f32 + 0.5 - cy;
    dx * dx + dy * dy <= radius * radius
}

/// Premultiplied BGRA. The shine is drawn at rest and on hover.
pub fn paint_bubble(buf: &mut [u8], diameter: i32, hover: bool, which: Arrow) {
    buf.fill(0);
    let d = diameter.max(1);
    let stride = d.saturating_mul(4);
    let radius = d as f32 * 0.5;
    let cx = radius;
    let cy = radius;
    let alpha = if hover { EDGE_HOVER_ALPHA } else { EDGE_ALPHA };
    for y in 0..d {
        for x in 0..d {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > radius {
                continue;
            }
            let shine = dist >= radius * 0.72 && dist <= radius * 0.92 && dx < 0.0 && dy < 0.0;
            if shine {
                put(buf, d, stride, x, y, 1.0, 1.0, 1.0, 0.92);
            } else {
                put(buf, d, stride, x, y, EDGE_RED, EDGE_GREEN, EDGE_BLUE, alpha);
            }
        }
    }
    draw_chevron(buf, d, stride, which);
}

fn draw_chevron(buf: &mut [u8], d: i32, stride: i32, which: Arrow) {
    let cx = d / 2;
    let cy = d / 2;
    let dir: i32 = match which {
        Arrow::Back => 1,
        Arrow::Forward => -1,
    };
    let point = cx - dir * 6;
    for i in 0..8 {
        put(buf, d, stride, point + dir * i, cy - i, 0.12, 0.16, 0.2, 1.0);
        put(buf, d, stride, point + dir * i, cy + i, 0.12, 0.16, 0.2, 1.0);
    }
}

fn put(buf: &mut [u8], d: i32, stride: i32, x: i32, y: i32, red: f32, green: f32, blue: f32, alpha: f32) {
    if x < 0 || y < 0 || x >= d || y >= d {
        return;
    }
    let i = (y * stride + x * 4) as usize;
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

    const W: i32 = 1920;
    const H: i32 = 1080;

    fn at_top() -> EdgeRead {
        EdgeRead::Web { scroll_top: 0.0, scroll_height: 5000.0, client_height: 900.0 }
    }

    fn at_bottom() -> EdgeRead {
        EdgeRead::Web { scroll_top: 4100.0, scroll_height: 5000.0, client_height: 900.0 }
    }

    fn short_page() -> EdgeRead {
        EdgeRead::Web { scroll_top: 0.0, scroll_height: 800.0, client_height: 900.0 }
    }

    fn middle() -> EdgeRead {
        EdgeRead::Web { scroll_top: 400.0, scroll_height: 5000.0, client_height: 900.0 }
    }

    fn which(arrows: &DocumentArrows) -> Vec<Arrow> {
        arrows.bubbles(W, H).into_iter().map(|paint| paint.which).collect()
    }

    fn center(which: Arrow) -> (i32, i32) {
        let paint = bubble(which, W, H, None).unwrap();
        (paint.origin_x + paint.diameter / 2, paint.origin_y + paint.diameter / 2)
    }

    #[test]
    fn document_arrow_ends_place_the_bubbles() {
        let mut arrows = DocumentArrows::new();
        arrows.note_neighbors(true, true);
        arrows.note_read(at_top());
        let shown = arrows.bubbles(W, H);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].which, Arrow::Back);
        assert_eq!(shown[0].origin_x, BUBBLE_PX);
        assert_eq!(shown[0].origin_y, BUBBLE_PX);
        assert!(shown.iter().all(|paint| paint.which != Arrow::Forward));

        arrows.note_read(at_bottom());
        let shown = arrows.bubbles(W, H);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].which, Arrow::Forward);
        assert_eq!(shown[0].origin_x, W - 2 * BUBBLE_PX);
        assert_eq!(shown[0].origin_y, H - 2 * BUBBLE_PX);

        arrows.note_read(short_page());
        assert_eq!(which(&arrows), vec![Arrow::Back, Arrow::Forward]);

        assert!(web_ends(2.0, 5000.0, 900.0).top == Some(true));
        assert!(web_ends(3.0, 5000.0, 900.0).top == Some(false));
        assert!(web_ends(4098.0, 5000.0, 900.0).bottom == Some(true));
        assert!(web_ends(4090.0, 5000.0, 900.0).bottom == Some(false));
    }

    #[test]
    fn document_arrow_first_and_last_and_one_document() {
        let mut arrows = DocumentArrows::new();
        arrows.note_read(at_top());
        arrows.note_neighbors(false, true);
        assert!(which(&arrows).is_empty());

        arrows.note_read(at_bottom());
        arrows.note_neighbors(true, false);
        assert!(which(&arrows).is_empty());

        arrows.note_read(short_page());
        arrows.note_neighbors(false, false);
        assert!(which(&arrows).is_empty());
    }

    #[test]
    fn document_arrow_a_high_locus_is_not_the_top() {
        let mut arrows = DocumentArrows::new();
        arrows.note_neighbors(true, true);
        arrows.note_read(middle());
        assert!(!which(&arrows).contains(&Arrow::Back));
        let ends = ends_of(&middle());
        assert_eq!(show(&ends, true, true).0, false);
    }

    #[test]
    fn document_arrow_pdf_pages_and_a_failed_read() {
        let mut arrows = DocumentArrows::new();
        arrows.note_neighbors(true, true);
        arrows.note_read(EdgeRead::Pdf { page: 1, pages: Some(4) });
        assert_eq!(which(&arrows), vec![Arrow::Back]);
        arrows.note_read(EdgeRead::Pdf { page: 4, pages: Some(4) });
        assert_eq!(which(&arrows), vec![Arrow::Forward]);
        arrows.note_read(EdgeRead::Pdf { page: 1, pages: Some(1) });
        assert_eq!(which(&arrows), vec![Arrow::Back, Arrow::Forward]);
        arrows.note_read(EdgeRead::Failed);
        assert!(which(&arrows).is_empty());
        arrows.note_read(EdgeRead::Pdf { page: 0, pages: Some(3) });
        assert!(which(&arrows).is_empty());
        arrows.note_read(EdgeRead::Pdf { page: 1, pages: None });
        assert_eq!(which(&arrows), vec![Arrow::Back]);
        let unread = edge_from_report(&ReportDocEdgeBody {
            proposal_id: "e".into(),
            surface: "unread".into(),
            scroll_top: None,
            scroll_height: None,
            client_height: None,
            page: None,
            pages: None,
        });
        assert_eq!(unread, EdgeRead::Failed);
    }

    #[test]
    fn document_arrow_rest_is_faint_and_the_shine_stays() {
        assert!(EDGE_ALPHA < 0.5);
        assert!(EDGE_HOVER_ALPHA > EDGE_ALPHA);
        let d = BUBBLE_PX;
        let mut rest = vec![0u8; (d * d * 4) as usize];
        let mut hover = rest.clone();
        paint_bubble(&mut rest, d, false, Arrow::Back);
        paint_bubble(&mut hover, d, true, Arrow::Back);
        let fill = pixel(&rest, d, 30, 30);
        let hot = pixel(&hover, d, 30, 30);
        assert_eq!(fill.3, (EDGE_ALPHA * 255.0).round() as u8);
        assert_eq!(hot.3, (EDGE_HOVER_ALPHA * 255.0).round() as u8);
        assert!(hot.3 > fill.3);
        let (red, green, blue) = unpremultiply(fill);
        assert!((red - EDGE_RED).abs() < 0.02);
        assert!((green - EDGE_GREEN).abs() < 0.02);
        assert!((blue - EDGE_BLUE).abs() < 0.02);
        for color in [MarkColor::Yellow, MarkColor::Green, MarkColor::Red] {
            let (b, g, r) = color.factors();
            let away = (red - r).abs() + (green - g).abs() + (blue - b).abs();
            assert!(away > 0.4, "{color:?}");
        }
        let rest_shine = pixel(&rest, d, 10, 10);
        let hover_shine = pixel(&hover, d, 10, 10);
        assert!(rest_shine.3 > fill.3);
        assert!(hover_shine.3 > hot.3);
        assert_eq!(rest_shine.3, hover_shine.3);
        let (sr, sg, sb) = unpremultiply(rest_shine);
        assert!(sr > 0.9 && sg > 0.9 && sb > 0.9);
    }

    #[test]
    fn document_arrow_click_steps_and_a_slide_does_not() {
        assert_eq!(arrow_hotkey(Arrow::Back), Hotkey::PrevSource);
        assert_eq!(arrow_hotkey(Arrow::Forward), Hotkey::NextSource);
        assert_ne!(arrow_hotkey(Arrow::Back), Hotkey::Back);

        let mut arrows = DocumentArrows::new();
        arrows.note_neighbors(true, true);
        arrows.note_read(at_bottom());
        let (x, y) = center(Arrow::Forward);
        arrows.press_at(x, y, W, H);
        assert_eq!(arrows.release_at(x, y, W, H), Some(Arrow::Forward));
        arrows.press_at(x, y, W, H);
        assert_eq!(arrows.release_at(0, 0, W, H), None);

        arrows.note_read(at_top());
        let (x, y) = center(Arrow::Back);
        arrows.press_at(x, y, W, H);
        assert_eq!(arrows.release_at(x, y, W, H), Some(Arrow::Back));
        arrows.press_at(x, y, W, H);
        assert_eq!(arrows.release_at(x + BUBBLE_PX, y + BUBBLE_PX, W, H), None);

        arrows.note_neighbors(false, true);
        arrows.press_at(x, y, W, H);
        assert_eq!(arrows.release_at(x, y, W, H), None);
    }

    #[test]
    fn document_arrow_surface_takes_the_bubble_and_no_keyboard() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let overlay = std::fs::read_to_string(root.join("overlay.rs")).unwrap();
        assert!(!overlay.contains("get_keyboard"));
        assert_eq!(overlay.matches("region.add").count(), 1);
        let arrows = std::fs::read_to_string(root.join("overlay/arrow_surface.rs")).unwrap();
        assert!(arrows.contains("KeyboardInteractivity::None"));
        assert!(!arrows.contains("KeyboardInteractivity::Exclusive"));
        assert!(!arrows.contains("get_keyboard"));
        assert!(arrows.contains("pachakutech-arrow"));
        assert!(arrows.contains("region.add"));
        let step = std::fs::read_to_string(root.join("tour.rs")).unwrap();
        let head = step.split("mod tests").next().unwrap();
        let apply = head.split("pub fn apply_arrow_step").nth(1).unwrap();
        let apply = apply.split("\nfn ").next().unwrap();
        assert!(apply.contains("arrow_hotkey"));
        assert!(apply.contains("close_row_for_document_leave"));
        assert!(!apply.contains("Hotkey::Back"));
        assert!(!apply.contains("declare_interest"));
        assert!(!apply.contains("visible_interest"));
    }

    fn pixel(buf: &[u8], d: i32, x: i32, y: i32) -> (u8, u8, u8, u8) {
        let i = (y * d * 4 + x * 4) as usize;
        (buf[i + 2], buf[i + 1], buf[i], buf[i + 3])
    }

    fn unpremultiply(px: (u8, u8, u8, u8)) -> (f32, f32, f32) {
        let alpha = px.3 as f32 / 255.0;
        (px.0 as f32 / 255.0 / alpha, px.1 as f32 / 255.0 / alpha, px.2 as f32 / 255.0 / alpha)
    }
}
