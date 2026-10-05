//! Face-animation track: Rhubarb mouth cues sampled against the audio clock.
//!
//! Rhubarb shape semantics follow upstream (DanielSWolf/rhubarb-lip-sync README):
//! A = closed lips (P/B/M), B = slightly open, clenched teeth (K/S/T/EE),
//! C = open (EH/AE), D = wide open (AA), E = rounded (AO/ER), F = puckered
//! (UW/OW/W), G = upper teeth on lip (F/V), H = long L, X = idle/silence.
//! docs/plans/text_to_cues.md has A and B inverted; do not copy its table.
use serde::{Deserialize, Serialize};

/// Bump when SHAPE_TABLE changes so cached faceanim files are rebuilt.
pub const MAPPING_VERSION: u32 = 1;
const FADE_SECONDS: f32 = 0.06;

#[derive(Clone, Copy, Debug)]
pub struct ShapeTarget {
    pub jaw: f32,
    /// Name of the GLB morph target (Oculus-style viseme set in the proxy GLB).
    pub viseme: &'static str,
    pub weight: f32,
}

/// Rhubarb shape -> jaw opening + one viseme morph. Starting values; tune on device.
pub fn shape_target(shape: char) -> ShapeTarget {
    let t = |jaw, viseme, weight| ShapeTarget { jaw, viseme, weight };
    match shape {
        'A' => t(0.00, "viseme_PP", 1.0),
        'B' => t(0.10, "viseme_I", 0.6),
        'C' => t(0.45, "viseme_E", 0.7),
        'D' => t(0.85, "viseme_aa", 1.0),
        'E' => t(0.40, "viseme_O", 0.8),
        'F' => t(0.20, "viseme_U", 0.9),
        'G' => t(0.10, "viseme_FF", 1.0),
        'H' => t(0.35, "viseme_nn", 0.6),
        _ => t(0.00, "viseme_sil", 0.0), // X and anything unknown: neutral
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cue {
    pub start: f32,
    pub end: f32,
    pub shape: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FaceAnim {
    pub version: u32,
    pub mapping_version: u32,
    pub duration: f32,
    pub cues: Vec<Cue>,
}

/// One sampled instant: jaw opening plus named viseme weights.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceSample {
    pub jaw: f32,
    pub visemes: Vec<(&'static str, f32)>,
}

impl FaceAnim {
    /// Parse Rhubarb's `-f json` output. Rejects empty/unsorted/non-finite tracks.
    pub fn from_rhubarb_json(s: &str) -> Result<FaceAnim, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("rhubarb json: {e}"))?;
        let arr = v["mouthCues"].as_array().ok_or("rhubarb json has no mouthCues")?;
        let mut cues = Vec::with_capacity(arr.len());
        for c in arr {
            let start = c["start"].as_f64().ok_or("cue.start")? as f32;
            let end = c["end"].as_f64().ok_or("cue.end")? as f32;
            let shape = c["value"].as_str().ok_or("cue.value")?.to_string();
            if !start.is_finite() || !end.is_finite() || end < start { return Err("bad cue times".into()); }
            if let Some(prev) = cues.last() { let p: &Cue = prev; if start + 1e-4 < p.start { return Err("cues not sorted".into()); } }
            cues.push(Cue { start, end, shape });
        }
        if cues.is_empty() { return Err("rhubarb produced no cues".into()); }
        let duration = v["metadata"]["duration"].as_f64().map(|d| d as f32).unwrap_or_else(|| cues.last().map(|c| c.end).unwrap_or(0.0));
        Ok(FaceAnim { version: 1, mapping_version: MAPPING_VERSION, duration, cues })
    }

    pub fn from_json(s: &str) -> Result<FaceAnim, String> {
        let f: FaceAnim = serde_json::from_str(s).map_err(|e| format!("faceanim json: {e}"))?;
        if f.mapping_version != MAPPING_VERSION { return Err("faceanim mapping version is stale".into()); }
        Ok(f)
    }

    pub fn to_json(&self) -> String { serde_json::to_string_pretty(self).unwrap_or_default() }

    fn target_at(&self, idx: Option<usize>) -> ShapeTarget {
        idx.and_then(|i| self.cues.get(i)).and_then(|c| c.shape.chars().next()).map(shape_target).unwrap_or_else(|| shape_target('X'))
    }

    /// Sample at audio time `t` seconds, cross-fading from the previous cue.
    pub fn sample(&self, t: f32) -> FaceSample {
        if !t.is_finite() || t < 0.0 || t >= self.duration.max(self.cues.last().map(|c| c.end).unwrap_or(0.0)) {
            return FaceSample::default();
        }
        // last cue with start <= t
        let idx = match self.cues.iter().rposition(|c| c.start <= t) { Some(i) => i, None => return FaceSample::default() };
        let cur = self.target_at(Some(idx));
        let prev = self.target_at(idx.checked_sub(1));
        let len = (self.cues[idx].end - self.cues[idx].start).max(1e-3);
        let fade = FADE_SECONDS.min(len * 0.5);
        let k = ((t - self.cues[idx].start) / fade).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        let mut visemes: Vec<(&'static str, f32)> = Vec::with_capacity(2);
        let mut add = |name: &'static str, w: f32| {
            if w <= 0.0 { return; }
            if let Some(e) = visemes.iter_mut().find(|(n, _)| *n == name) { e.1 += w; } else { visemes.push((name, w)); }
        };
        add(prev.viseme, prev.weight * (1.0 - k));
        add(cur.viseme, cur.weight * k);
        FaceSample { jaw: prev.jaw + (cur.jaw - prev.jaw) * k, visemes }
    }
}
