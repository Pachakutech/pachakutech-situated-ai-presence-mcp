//! Slide the 220 layer toward a quiet part of the screen.
//!
//! The GPU writes one heat value per cell. [`crate::actors::quietude`] keeps
//! the area under that field for a short span. This module reads that memory
//! when deciding to leave, and reads the latest capture once a glide has
//! started. The score of a spot is the mean heat in a window of one and a
//! half times the disc's area, centered on the disc, excluding cells the
//! disc itself covers.
//!
//! At rest another spot has to be 15% under the rectified field. A spike
//! that does not survive the span does not move the layer, and cooling
//! argues for staying. Once a glide has started, that span is not a second
//! wait: the layer eases until the aimed spot has stayed a standard
//! deviation below the latest field for about 400 ms. A new capture
//! redirects it only when another spot, more than a cell away, stays at
//! least 15% cooler for about 200 ms.

use crate::actors::quietude::QuietudeMemory;

/// Layer edge, in output pixels. Matches `BUBBLE_PX` in the overlay.
pub const DISC_PX: i32 = 220;
/// Side of the square whose area is one and a half times the disc.
/// 220 * sqrt(1.5), rounded. Tight enough that a small calm rectangle
/// is not averaged together with the text beside it.
pub const NEIGHBOR_SIDE: i32 = 269;
const STABLE_SECS: f32 = 0.4;
/// How long a better spot has to remain better before a glide changes course.
/// One capture is noise. This is not the 800 ms departure window.
const RETARGET_SECS: f32 = 0.2;
const DEADBAND: f32 = 0.15;
const SLIDE_PX_PER_SEC: f32 = 280.0;
const SLIDE_MIN_SECS: f32 = 0.35;

/// One finished heat dispatch. `mask_a` and `mask_b` are disc top-lefts the
/// shader treated as the layer (current and previous). Cells under them are
/// not the desktop, and the score skips them.
pub struct HeatGrid {
    pub cols: u32,
    pub rows: u32,
    pub cell_px: u32,
    pub screen_w: u32,
    pub screen_h: u32,
    pub heat: Vec<f32>,
    pub mask_a: (i32, i32),
    pub mask_b: (i32, i32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Settled,
    Seeking,
}

#[derive(Clone, Copy)]
struct Spot {
    x: i32,
    y: i32,
    score: f32,
}

pub struct Placement {
    phase: Phase,
    pos: [f32; 2],
    from: [f32; 2],
    to: [f32; 2],
    lerp_t: f32,
    lerp_dur: f32,
    memory: QuietudeMemory,
    since_sample: f32,
    /// How long the aimed spot has stayed a standard deviation under the field.
    aim_cool_secs: f32,
    hold_x: i32,
    hold_y: i32,
    hold_secs: f32,
    was_idle: bool,
}

impl Placement {
    pub fn at(x: i32, y: i32) -> Self {
        let pos = [x as f32, y as f32];
        Self {
            phase: Phase::Settled,
            pos,
            from: pos,
            to: pos,
            lerp_t: 0.0,
            lerp_dur: SLIDE_MIN_SECS,
            memory: QuietudeMemory::new(),
            since_sample: 0.0,
            aim_cool_secs: 0.0,
            hold_x: x,
            hold_y: y,
            hold_secs: 0.0,
            was_idle: false,
        }
    }

    /// Drop a glide and the integral. Used when the layer was parked.
    pub fn reset(&mut self, x: i32, y: i32) {
        *self = Self::at(x, y);
    }

    #[cfg(test)]
    pub fn is_settled(&self) -> bool {
        self.phase == Phase::Settled
    }

    /// `grid` is a newly finished capture. `None` only advances the glide.
    /// Idle does not read the grid. A glide already in motion runs to its
    /// current target and then holds, so a half-eased layer is not frozen.
    pub fn tick(
        &mut self,
        grid: Option<&HeatGrid>,
        dt: f32,
        idle: bool,
        out_w: u32,
        out_h: u32,
    ) -> (i32, i32) {
        let dt = dt.clamp(0.0, 0.25);
        if idle {
            self.memory.clear();
            self.since_sample = 0.0;
            self.was_idle = true;
            if self.phase == Phase::Seeking {
                self.advance_lerp(dt);
                if self.lerp_done() {
                    self.settle();
                }
            }
            return self.clamped(out_w, out_h);
        }
        if self.was_idle {
            self.was_idle = false;
            self.memory.clear();
            self.aim_cool_secs = 0.0;
            self.hold_secs = 0.0;
        }
        self.since_sample += dt;
        if let Some(grid) = grid {
            let sample_dt = self.since_sample.clamp(1.0 / 120.0, 0.5);
            self.since_sample = 0.0;
            self.observe(grid, sample_dt, out_w, out_h);
        }
        self.advance_lerp(dt);
        // The aimed spot has to stay cool. The winning cell does not: on a
        // real desktop it trades with its neighbor every capture.
        if self.phase == Phase::Seeking && self.lerp_done() && self.aim_cool_secs >= STABLE_SECS {
            self.settle();
        }
        self.clamped(out_w, out_h)
    }

    fn observe(&mut self, grid: &HeatGrid, sample_dt: f32, out_w: u32, out_h: u32) {
        self.memory.observe(
            grid.cols,
            grid.rows,
            &grid.heat,
            &observed_cells(grid),
            sample_dt,
        );
        if self.phase == Phase::Settled {
            self.consider_leaving(grid, out_w, out_h);
            return;
        }

        let Some(best) = coolest(grid, self.aim(), out_w, out_h) else {
            return;
        };
        let (mean, std) = field_stats(grid, out_w, out_h);
        let cool = best.score <= mean - std;

        let aimed = neighborhood_mean(grid, self.to[0].round() as i32, self.to[1].round() as i32)
            .unwrap_or(f32::MAX);
        let aimed_cool = aimed <= mean - std;
        if aimed_cool {
            self.aim_cool_secs += sample_dt;
        } else {
            self.aim_cool_secs = 0.0;
        }
        // One cell either way is the same neighborhood. The grid step is a
        // cell, so the old half-cell test restarted the ease every capture.
        if same_spot(self.to, best.x, best.y, grid.cell_px) {
            self.hold_secs = 0.0;
            return;
        }
        let clearly = best.score + 1e-3 < aimed * (1.0 - DEADBAND);
        // The aim warmed up (or was never an outlier) and somewhere else is.
        // Still has to hold for the short beat, or a one-frame spike redirects.
        let aim_missed = !aimed_cool && cool;
        if !clearly && !aim_missed {
            self.hold_secs = 0.0;
            return;
        }
        if self.hold_secs > 0.0 && same_spot([self.hold_x as f32, self.hold_y as f32], best.x, best.y, grid.cell_px) {
            self.hold_secs += sample_dt;
        } else {
            self.hold_x = best.x;
            self.hold_y = best.y;
            self.hold_secs = sample_dt;
        }
        if self.hold_secs + 1e-3 >= RETARGET_SECS {
            self.hold_secs = 0.0;
            self.begin_seek(best.x, best.y);
        }
    }

    /// Departure reads the rectified field, not this one capture. A cold
    /// blip elsewhere, or a hot blip here, is a small part of the span.
    fn consider_leaving(&mut self, grid: &HeatGrid, out_w: u32, out_h: u32) {
        if !self.memory.ready() {
            return;
        }
        let scored = self.rectified(grid);
        let Some(best) = coolest(&scored, self.pos, out_w, out_h) else {
            return;
        };
        let Some(here) = neighborhood_mean(&scored, self.pos[0].round() as i32, self.pos[1].round() as i32)
        else {
            return;
        };
        // A thousandth keeps a mean that landed on the deadband, after
        // adding the cell heats, from counting as clearly cooler.
        let clear = best.score + 1e-3 < here * (1.0 - DEADBAND);
        let elsewhere = !same_spot(self.pos, best.x, best.y, grid.cell_px);
        if clear && elsewhere {
            self.begin_seek(best.x, best.y);
        }
    }

    fn rectified(&self, grid: &HeatGrid) -> HeatGrid {
        let mut heat = self.memory.means().to_vec();
        if heat.len() != grid.heat.len() {
            heat = vec![f32::NAN; grid.heat.len()];
        }
        HeatGrid {
            cols: grid.cols,
            rows: grid.rows,
            cell_px: grid.cell_px,
            screen_w: grid.screen_w,
            screen_h: grid.screen_h,
            heat,
            mask_a: grid.mask_a,
            mask_b: grid.mask_b,
        }
    }

    fn begin_seek(&mut self, x: i32, y: i32) {
        let dest = [x as f32, y as f32];
        if self.phase == Phase::Seeking && near(self.to, x, y, 1) {
            return;
        }
        let dx = dest[0] - self.pos[0];
        let dy = dest[1] - self.pos[1];
        let dist = (dx * dx + dy * dy).sqrt();
        eprintln!(
            "[placement] seek {:.0},{:.0} -> {x},{y}",
            self.pos[0], self.pos[1]
        );
        self.phase = Phase::Seeking;
        self.from = self.pos;
        self.to = dest;
        self.lerp_t = 0.0;
        self.lerp_dur = (dist / SLIDE_PX_PER_SEC).max(SLIDE_MIN_SECS);
        self.aim_cool_secs = 0.0;
        self.hold_secs = 0.0;
    }

    fn advance_lerp(&mut self, dt: f32) {
        if self.phase != Phase::Seeking {
            return;
        }
        let dur = self.lerp_dur.max(1e-4);
        self.lerp_t = (self.lerp_t + dt).min(dur);
        let e = quad_in_out(self.lerp_t / dur);
        self.pos[0] = self.from[0] + (self.to[0] - self.from[0]) * e;
        self.pos[1] = self.from[1] + (self.to[1] - self.from[1]) * e;
    }

    fn lerp_done(&self) -> bool {
        self.lerp_t + 1e-4 >= self.lerp_dur
    }

    fn settle(&mut self) {
        if self.phase == Phase::Settled {
            return;
        }
        self.pos = self.to;
        self.from = self.to;
        self.phase = Phase::Settled;
        self.aim_cool_secs = 0.0;
        self.hold_secs = 0.0;
        eprintln!("[placement] settle {:.0},{:.0}", self.pos[0], self.pos[1]);
    }

    fn aim(&self) -> [f32; 2] {
        if self.phase == Phase::Seeking { self.to } else { self.pos }
    }

    fn clamped(&self, out_w: u32, out_h: u32) -> (i32, i32) {
        let max_x = out_w.saturating_sub(DISC_PX as u32) as i32;
        let max_y = out_h.saturating_sub(DISC_PX as u32) as i32;
        (
            (self.pos[0].round() as i32).clamp(0, max_x.max(0)),
            (self.pos[1].round() as i32).clamp(0, max_y.max(0)),
        )
    }
}

/// Ease used for the glide. Slow at both ends, linear through the middle.
pub fn quad_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        let u = -2.0 * t + 2.0;
        1.0 - (u * u) * 0.5
    }
}

fn observed_cells(grid: &HeatGrid) -> Vec<bool> {
    let n = grid.heat.len();
    let mut out = vec![false; n];
    if grid.cols == 0 || (grid.cols as usize).saturating_mul(grid.rows as usize) != n {
        return out;
    }
    let cell = grid.cell_px.max(1) as f32;
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let i = (row * grid.cols + col) as usize;
            let cx = (col as f32 + 0.5) * cell;
            let cy = (row as f32 + 0.5) * cell;
            out[i] = grid.heat[i].is_finite()
                && !disc_covers(cx, cy, grid.mask_a)
                && !disc_covers(cx, cy, grid.mask_b);
        }
    }
    out
}

fn near(pos: [f32; 2], x: i32, y: i32, cell: u32) -> bool {
    let slop = (cell.max(1) as f32) * 0.5;
    (pos[0] - x as f32).abs() <= slop.max(0.5) && (pos[1] - y as f32).abs() <= slop.max(0.5)
}

/// Chebyshev distance of one cell. Adjacent grid points are the same place:
/// their twice-area windows overlap almost completely.
fn same_spot(pos: [f32; 2], x: i32, y: i32, cell: u32) -> bool {
    let slop = cell.max(1) as f32;
    (pos[0] - x as f32).abs() <= slop && (pos[1] - y as f32).abs() <= slop
}

fn screen_box(grid: &HeatGrid, out_w: u32, out_h: u32) -> (u32, u32) {
    (grid.screen_w.min(out_w.max(1)), grid.screen_h.min(out_h.max(1)))
}

fn origins(w: u32, h: u32, step: u32) -> Vec<(i32, i32)> {
    let max_x = w.saturating_sub(DISC_PX as u32) as i32;
    let max_y = h.saturating_sub(DISC_PX as u32) as i32;
    if max_x < 0 || max_y < 0 {
        return Vec::new();
    }
    let step = step.max(1) as i32;
    let mut xs = Vec::new();
    let mut x = 0;
    while x < max_x {
        xs.push(x);
        x += step;
    }
    xs.push(max_x);
    let mut ys = Vec::new();
    let mut y = 0;
    while y < max_y {
        ys.push(y);
        y += step;
    }
    ys.push(max_y);
    let mut out = Vec::with_capacity(xs.len() * ys.len());
    for y in ys {
        for x in &xs {
            out.push((*x, y));
        }
    }
    out
}

fn field_stats(grid: &HeatGrid, out_w: u32, out_h: u32) -> (f32, f32) {
    let (w, h) = screen_box(grid, out_w, out_h);
    let mut vals = Vec::new();
    for (x, y) in origins(w, h, grid.cell_px) {
        if let Some(score) = neighborhood_mean(grid, x, y) {
            if score.is_finite() {
                vals.push(score);
            }
        }
    }
    if vals.is_empty() {
        return (0.0, 0.0);
    }
    let n = vals.len() as f32;
    let mean = vals.iter().sum::<f32>() / n;
    let var = vals.iter().map(|v| {
        let d = v - mean;
        d * d
    }).sum::<f32>() / n;
    let std = var.sqrt();
    // A flat desktop has a tiny spread from sampling noise. Treating that as
    // a real standard deviation would keep the layer seeking forever.
    let floor = (mean.abs() * 0.02).max(1e-4);
    let std = if std < floor { 0.0 } else { std };
    (mean, std)
}

fn coolest(grid: &HeatGrid, prefer: [f32; 2], out_w: u32, out_h: u32) -> Option<Spot> {
    let (w, h) = screen_box(grid, out_w, out_h);
    let mut best: Option<Spot> = None;
    for (x, y) in origins(w, h, grid.cell_px) {
        let Some(score) = neighborhood_mean(grid, x, y) else { continue };
        if !score.is_finite() {
            continue;
        }
        let replace = match &best {
            None => true,
            Some(cur) => {
                score < cur.score - 1e-5
                    || ((score - cur.score).abs() <= 1e-5 && dist2(x, y, prefer) < dist2(cur.x, cur.y, prefer))
            }
        };
        if replace {
            best = Some(Spot { x, y, score });
        }
    }
    best
}

fn dist2(x: i32, y: i32, prefer: [f32; 2]) -> f32 {
    let dx = x as f32 - prefer[0];
    let dy = y as f32 - prefer[1];
    dx * dx + dy * dy
}

/// Mean heat of cells whose centers lie in the twice-area window and not in
/// either masked disc. `None` when the window has no such cell.
fn neighborhood_mean(grid: &HeatGrid, x: i32, y: i32) -> Option<f32> {
    let cell = grid.cell_px.max(1) as i32;
    let expect = (grid.cols as usize).saturating_mul(grid.rows as usize);
    if grid.heat.len() != expect || grid.cols == 0 || grid.rows == 0 {
        return None;
    }
    let (wx, wy) = window_origin(x, y);
    let side = NEIGHBOR_SIDE as f32;
    let mut sum = 0.0;
    let mut n = 0.0;
    let c0 = ((wx / cell as f32).floor() as i32).clamp(0, grid.cols as i32);
    let r0 = ((wy / cell as f32).floor() as i32).clamp(0, grid.rows as i32);
    let c1 = (((wx + side) / cell as f32).ceil() as i32).clamp(0, grid.cols as i32);
    let r1 = (((wy + side) / cell as f32).ceil() as i32).clamp(0, grid.rows as i32);
    for row in r0..r1 {
        for col in c0..c1 {
            let cx = (col as f32 + 0.5) * cell as f32;
            let cy = (row as f32 + 0.5) * cell as f32;
            if cx < wx || cy < wy || cx >= wx + side || cy >= wy + side {
                continue;
            }
            if disc_covers(cx, cy, grid.mask_a) || disc_covers(cx, cy, grid.mask_b) {
                continue;
            }
            let heat = grid.heat[(row as u32 * grid.cols + col as u32) as usize];
            if !heat.is_finite() {
                continue;
            }
            sum += heat;
            n += 1.0;
        }
    }
    (n > 0.0).then_some(sum / n)
}

fn window_origin(x: i32, y: i32) -> (f32, f32) {
    let cx = x as f32 + DISC_PX as f32 * 0.5;
    let cy = y as f32 + DISC_PX as f32 * 0.5;
    (cx - NEIGHBOR_SIDE as f32 * 0.5, cy - NEIGHBOR_SIDE as f32 * 0.5)
}

fn disc_covers(px: f32, py: f32, origin: (i32, i32)) -> bool {
    px >= origin.0 as f32
        && py >= origin.1 as f32
        && px < (origin.0 + DISC_PX) as f32
        && py < (origin.1 + DISC_PX) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop(fill: f32) -> HeatGrid {
        let cols = 48u32;
        let rows = 27u32;
        HeatGrid {
            cols,
            rows,
            cell_px: 40,
            screen_w: 1920,
            screen_h: 1080,
            heat: vec![fill; (cols * rows) as usize],
            mask_a: (850, 430),
            mask_b: (850, 430),
        }
    }

    fn paint(grid: &mut HeatGrid, c0: u32, r0: u32, c1: u32, r1: u32, v: f32) {
        for row in r0..r1.min(grid.rows) {
            for col in c0..c1.min(grid.cols) {
                grid.heat[(row * grid.cols + col) as usize] = v;
            }
        }
    }

    fn cold_corner() -> HeatGrid {
        let mut grid = desktop(1.0);
        paint(&mut grid, 0, 0, 8, 8, 0.0);
        grid
    }

    fn feed(place: &mut Placement, grid: &HeatGrid, steps: usize, dt: f32, idle: bool) -> (i32, i32) {
        let mut pos = (0, 0);
        for _ in 0..steps {
            pos = place.tick(Some(grid), dt, idle, 1920, 1080);
        }
        pos
    }

    #[test]
    fn neighbor_covers_one_and_a_half_times_the_disc() {
        let side = (DISC_PX as f32 * 1.5f32.sqrt()).round() as i32;
        assert_eq!(side, NEIGHBOR_SIDE);
        let ratio = (NEIGHBOR_SIDE * NEIGHBOR_SIDE) as f32 / (DISC_PX * DISC_PX) as f32;
        assert!((ratio - 1.5).abs() < 0.02, "{ratio}");
    }

    #[test]
    fn quad_in_out_is_slow_at_both_ends() {
        assert_eq!(quad_in_out(0.0), 0.0);
        assert_eq!(quad_in_out(1.0), 1.0);
        assert!((quad_in_out(0.5) - 0.5).abs() < 1e-5);
        assert!((quad_in_out(0.25) - 0.125).abs() < 1e-5);
        assert!((quad_in_out(0.75) - 0.875).abs() < 1e-5);
    }

    #[test]
    fn the_disc_is_left_out_of_its_own_score() {
        let mut grid = desktop(1.0);
        // The layer's own pixels would be hot if we counted them.
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cx = (col as f32 + 0.5) * 40.0;
                let cy = (row as f32 + 0.5) * 40.0;
                if disc_covers(cx, cy, (850, 430)) {
                    grid.heat[(row * grid.cols + col) as usize] = 100.0;
                }
            }
        }
        let masked = neighborhood_mean(&grid, 850, 430).unwrap();
        assert!(masked < 2.0, "masked ring should stay near the desktop, got {masked}");
        grid.mask_a = (-4000, -4000);
        grid.mask_b = (-4000, -4000);
        let raw = neighborhood_mean(&grid, 850, 430).unwrap();
        assert!(raw > 20.0, "without the mask the disc dominates, got {raw}");
    }

    #[test]
    fn a_spike_inside_the_window_does_not_leave() {
        let mut place = Placement::at(850, 430);
        let calm = desktop(0.5);
        let mut spiked = desktop(0.5);
        // One hot capture in the ring around the disc. The other spots stay at 0.5,
        // which is not 15% under the average of seven calm captures plus this one.
        paint(&mut spiked, 18, 8, 30, 20, 1.0);
        feed(&mut place, &calm, 7, 0.1, false);
        let pos = place.tick(Some(&spiked), 0.1, false, 1920, 1080);
        assert!(place.is_settled(), "a single hot capture should stay in the average");
        assert_eq!(pos, (850, 430));
    }

    #[test]
    fn a_cold_blip_elsewhere_does_not_leave() {
        let mut place = Placement::at(850, 430);
        let calm = desktop(0.5);
        feed(&mut place, &calm, 8, 0.1, false);
        assert!(place.is_settled());
        let mut blip = desktop(0.5);
        paint(&mut blip, 0, 0, 8, 8, 0.0);
        let pos = place.tick(Some(&blip), 0.1, false, 1920, 1080);
        assert!(place.is_settled(), "one cold capture is not a reason to leave");
        assert_eq!(pos, (850, 430));
    }

    #[test]
    fn sustained_heat_leaves_and_a_narrow_gap_does_not() {
        let hot = desktop(1.0);
        let mut barely = desktop(1.0);
        paint(&mut barely, 0, 0, 8, 8, 0.85);
        let mut clear = desktop(1.0);
        paint(&mut clear, 0, 0, 8, 8, 0.84);

        let mut stay = Placement::at(850, 430);
        feed(&mut stay, &barely, 8, 0.1, false);
        assert!(stay.is_settled(), "exactly the deadband should not move");

        let mut go = Placement::at(850, 430);
        feed(&mut go, &clear, 8, 0.1, false);
        assert!(!go.is_settled(), "past the deadband should start a glide");

        let mut leave = Placement::at(850, 430);
        feed(&mut leave, &hot, 8, 0.1, false);
        // Uniform heat is not a cooler spot.
        assert!(leave.is_settled());
    }

    #[test]
    fn cooling_keeps_the_layer_put() {
        let mut place = Placement::at(850, 430);
        // The corner stays at 0.9. While the neighborhood is hot that is not
        // 15% cooler, so we hold. After the neighborhood cools to 0.5 the
        // corner is warmer than where we are, and the cooled window is what
        // the decision sees.
        let mut warm = desktop(1.0);
        paint(&mut warm, 0, 0, 8, 8, 0.9);
        feed(&mut place, &warm, 8, 0.1, false);
        assert!(place.is_settled(), "0.9 against 1.0 is inside the deadband");
        let mut cooled = desktop(0.5);
        paint(&mut cooled, 0, 0, 8, 8, 0.9);
        feed(&mut place, &cooled, 8, 0.1, false);
        assert!(
            place.is_settled(),
            "once the window has cooled, the warmer corner should not pull us"
        );
    }

    #[test]
    fn a_glide_finds_the_cold_corner_without_another_wait() {
        let mut place = Placement::at(850, 430);
        let grid = cold_corner();
        let mut pos = (850, 430);
        let mut frames = 0;
        for _ in 0..160 {
            pos = place.tick(Some(&grid), 0.05, false, 1920, 1080);
            frames += 1;
            if place.is_settled() && frames > 20 {
                break;
            }
        }
        assert!(place.is_settled(), "should settle once the corner stays cool");
        assert!(pos.0 < 200 && pos.1 < 200, "settled at {pos:?}, expected the cold corner");
        // 800 ms to leave, then a glide. It must not sit through a second 800 ms
        // after it has already arrived.
        assert!(frames < 120, "took {frames} frames of 50 ms");
    }

    #[test]
    fn idle_finishes_the_glide_and_does_not_chase() {
        let grid = cold_corner();
        let mut held = Placement::at(850, 430);
        let mut live = Placement::at(850, 430);
        for _ in 0..16 {
            held.tick(Some(&grid), 0.05, false, 1920, 1080);
            live.tick(Some(&grid), 0.05, false, 1920, 1080);
        }
        assert!(!held.is_settled() && !live.is_settled());

        let mut other = desktop(1.0);
        paint(&mut other, 40, 19, 48, 27, 0.0);
        for _ in 0..160 {
            held.tick(Some(&other), 0.05, true, 1920, 1080);
            live.tick(Some(&other), 0.05, false, 1920, 1080);
        }
        let held_pos = held.tick(None, 0.0, true, 1920, 1080);
        let live_pos = live.tick(None, 0.0, false, 1920, 1080);
        assert!(held.is_settled(), "idle should hold once the current glide ends");
        assert!(held_pos.0 < 400 && held_pos.1 < 400, "idle held {held_pos:?}");
        assert!(live_pos.0 > held_pos.0 + 200, "a live layer should retarget, live {live_pos:?} held {held_pos:?}");
    }

    #[test]
    fn one_cell_away_is_the_same_spot() {
        assert!(same_spot([100.0, 100.0], 140, 100, 40));
        assert!(same_spot([100.0, 100.0], 140, 140, 40));
        assert!(!same_spot([100.0, 100.0], 180, 100, 40));
    }

    #[test]
    fn a_close_rival_does_not_restart_the_glide() {
        let mut place = Placement::at(850, 430);
        let mut home = desktop(1.0);
        paint(&mut home, 0, 0, 10, 10, 0.40);
        feed(&mut place, &home, 12, 0.1, false);
        assert!(!place.is_settled(), "the cold corner should have started a glide");

        // 0.36 is not 15% under 0.40 (the line is 0.34). One capture of it,
        // then the original spot again. A spike must not change course.
        let mut rival = desktop(1.0);
        paint(&mut rival, 0, 0, 10, 10, 0.40);
        paint(&mut rival, 38, 17, 48, 27, 0.36);
        place.tick(Some(&rival), 0.1, false, 1920, 1080);
        feed(&mut place, &home, 60, 0.1, false);
        assert!(place.is_settled(), "the original glide should finish and settle");
        let pos = place.tick(None, 0.0, false, 1920, 1080);
        assert!(pos.0 < 400 && pos.1 < 400, "stayed with the first cold spot, at {pos:?}");
    }

    #[test]
    fn a_sustained_better_spot_redirects_without_the_long_wait() {
        let mut place = Placement::at(850, 430);
        let mut home = desktop(1.0);
        paint(&mut home, 0, 0, 10, 10, 0.50);
        feed(&mut place, &home, 12, 0.1, false);
        assert!(!place.is_settled());

        let mut better = desktop(1.0);
        paint(&mut better, 0, 0, 10, 10, 0.50);
        paint(&mut better, 38, 17, 48, 27, 0.20);
        // Two tenths is enough to change course. Not another 800 ms.
        let pos = feed(&mut place, &better, 90, 0.1, false);
        assert!(pos.0 > 1000, "should have redirected to the colder corner, at {pos:?}");
    }

    #[test]
    fn adjacent_winners_do_not_restart_the_glide() {
        let mut home = desktop(1.0);
        paint(&mut home, 0, 0, 16, 16, 0.20);
        let first = coolest(&home, [850.0, 430.0], 1920, 1080).expect("cold block");
        let mut place = Placement::at(850, 430);
        feed(&mut place, &home, 12, 0.1, false);
        assert!(!place.is_settled());

        // The aimed window keeps a hot strip the next window does not, so the
        // winner steps one cell and clears the deadband. That is still the
        // same neighborhood, and the ease must not restart.
        let mut biased = desktop(1.0);
        let neighbor = (first.x + 40, first.y);
        for row in 0..biased.rows {
            for col in 0..biased.cols {
                let cx = (col as f32 + 0.5) * 40.0;
                let cy = (row as f32 + 0.5) * 40.0;
                let in_aim = window_contains(first.x, first.y, cx, cy);
                let in_next = window_contains(neighbor.0, neighbor.1, cx, cy);
                let v = if in_aim && !in_next {
                    1.0
                } else if in_aim || in_next {
                    0.0
                } else {
                    1.0
                };
                biased.heat[(row * biased.cols + col) as usize] = v;
            }
        }
        let winner = coolest(&biased, [first.x as f32, first.y as f32], 1920, 1080).expect("biased");
        let aimed = neighborhood_mean(&biased, first.x, first.y).expect("aim score");
        let dx = (winner.x - first.x).abs();
        let dy = (winner.y - first.y).abs();
        assert!(
            dx <= 40 && dy <= 40 && (dx > 0 || dy > 0),
            "want a one-cell hop, winner {},{} score {} aim {aimed} first {},{}",
            winner.x, winner.y, winner.score, first.x, first.y
        );
        assert!(
            winner.score + 1e-3 < aimed * (1.0 - DEADBAND),
            "the hop should be past the deadband (winner {} aim {aimed}) so the neighborhood rule is what holds the glide",
            winner.score
        );

        feed(&mut place, &biased, 60, 0.1, false);
        assert!(place.is_settled(), "a one-cell wobble should still let the glide finish");
        let pos = place.tick(None, 0.0, false, 1920, 1080);
        assert!(
            (pos.0 - first.x).abs() <= 40 && (pos.1 - first.y).abs() <= 40,
            "settled at {pos:?}, expected the original aim {},{}",
            first.x, first.y
        );
    }

    fn window_contains(origin_x: i32, origin_y: i32, cx: f32, cy: f32) -> bool {
        let (wx, wy) = window_origin(origin_x, origin_y);
        cx >= wx && cy >= wy && cx < wx + NEIGHBOR_SIDE as f32 && cy < wy + NEIGHBOR_SIDE as f32
    }
}
