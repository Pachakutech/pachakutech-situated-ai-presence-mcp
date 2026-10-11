//! Perceptual memory of how hot the screen has been.
//!
//! Screen ingress is a picture. The heat field is one capture, a dense
//! transform of that picture. This memory keeps the area under each cell
//! for a short recent span and answers queries against that rectified
//! model. Placement reads it when deciding whether to leave. The model is
//! the cells, not the glide: another actor can read the same span.

use std::collections::VecDeque;

/// How long a sample stays in the model. A general span, not a deadline.
const SPAN_SECS: f32 = 0.8;

struct Frame {
    dt: f32,
    /// NaN where this capture did not observe the cell.
    heat: Vec<f32>,
}

/// Time-rectified heat. Unobserved cells (under the disc, or not yet seen)
/// stay out of the mean rather than counting as cold.
pub struct QuietudeMemory {
    cols: u32,
    rows: u32,
    frames: VecDeque<Frame>,
    span: f32,
    acc: Vec<f32>,
    weight: Vec<f32>,
    mean: Vec<f32>,
}

impl QuietudeMemory {
    pub fn new() -> Self {
        Self {
            cols: 0,
            rows: 0,
            frames: VecDeque::new(),
            span: 0.0,
            acc: Vec::new(),
            weight: Vec::new(),
            mean: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.span = 0.0;
        self.acc.fill(0.0);
        self.weight.fill(0.0);
        self.mean.fill(f32::NAN);
    }

    pub fn span(&self) -> f32 {
        self.span
    }

    /// The span is full enough to reason over. A shorter stretch is still
    /// only a transient.
    pub fn ready(&self) -> bool {
        self.span() + 1e-3 >= SPAN_SECS
    }

    pub fn means(&self) -> &[f32] {
        &self.mean
    }

    /// Rectified heat of one cell. `None` when the span holds no sample of it.
    /// Placement reads the whole field; this is the single-cell query.
    #[allow(dead_code)]
    pub fn cell(&self, col: u32, row: u32) -> Option<f32> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        let v = self.mean[(row * self.cols + col) as usize];
        v.is_finite().then_some(v)
    }

    /// Fold one capture into the span. `observed` is false where the disc
    /// covered the cell: that zero is the layer, not the desktop, and it
    /// must not pull the memory toward cold.
    pub fn observe(&mut self, cols: u32, rows: u32, heat: &[f32], observed: &[bool], dt: f32) {
        let n = (cols as usize).saturating_mul(rows as usize);
        if dt <= 0.0 || heat.len() != n || observed.len() != n || n == 0 {
            return;
        }
        if self.cols != cols || self.rows != rows || self.mean.len() != n {
            self.cols = cols;
            self.rows = rows;
            self.frames.clear();
            self.span = 0.0;
            self.acc = vec![0.0; n];
            self.weight = vec![0.0; n];
            self.mean = vec![f32::NAN; n];
        }
        let mut sample = vec![f32::NAN; n];
        for i in 0..n {
            if observed[i] && heat[i].is_finite() {
                sample[i] = heat[i];
            }
        }
        self.apply(&sample, dt, 1.0);
        self.frames.push_back(Frame { dt, heat: sample });
        self.span += dt;
        while self.frames.len() > 1 && self.span - self.frames[0].dt >= SPAN_SECS {
            let old = self.frames.pop_front().unwrap();
            self.span -= old.dt;
            self.apply(&old.heat, old.dt, -1.0);
        }
    }

    fn apply(&mut self, heat: &[f32], dt: f32, sign: f32) {
        for i in 0..heat.len() {
            let h = heat[i];
            if !h.is_finite() {
                continue;
            }
            self.acc[i] += sign * dt * h;
            self.weight[i] += sign * dt;
            if self.weight[i] <= 1e-5 {
                self.acc[i] = 0.0;
                self.weight[i] = 0.0;
                self.mean[i] = f32::NAN;
            } else {
                self.mean[i] = self.acc[i] / self.weight[i];
            }
        }
    }
}

impl Default for QuietudeMemory {
    fn default() -> Self {
        Self::new()
    }
}
