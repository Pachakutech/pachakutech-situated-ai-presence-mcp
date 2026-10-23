//! Perceptual memory of the words on the glass.
//!
//! The capture clock stays 10 Hz. This actor is not on that clock. It takes
//! one reused shared-memory copy when it is free, runs Tesseract, and posts
//! one frame-coherent message: columns of equal length, every word from that
//! single copy. The overlay loop does not wait. A query reads the latest
//! complete message, and a message older than [`MAX_AGE`] is not offered.

use crate::actors::ingress::screen_wlr::WlrScreenSource;
use crate::actors::ingress::{FrameSource, IngressFrame};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// How long a posted message may be offered. The pixels are older by `workedMs`.
pub const MAX_AGE: Duration = Duration::from_secs(8);
/// Do not start another copy sooner than the engine's capture interval.
const MIN_GAP: Duration = Duration::from_millis(100);

/// Disc rectangle and the engine capture counter. The overlay writes these.
/// The worker only reads them.
pub struct GlassStamp {
    pub frame: AtomicU64,
    pub disc_x: AtomicI32,
    pub disc_y: AtomicI32,
    pub disc_side: AtomicI32,
}

impl GlassStamp {
    pub fn new() -> Self {
        Self {
            frame: AtomicU64::new(0),
            disc_x: AtomicI32::new(0),
            disc_y: AtomicI32::new(0),
            disc_side: AtomicI32::new(0),
        }
    }
}

/// One finished reading. Columns share one length and one capture.
#[derive(Clone)]
pub struct Reading {
    pub seq: u64,
    pub engine_frame: u64,
    pub at: Instant,
    pub worked: Duration,
    pub width: u32,
    pub height: u32,
    pub text: Vec<String>,
    pub x: Vec<i32>,
    pub y: Vec<i32>,
    pub w: Vec<i32>,
    pub h: Vec<i32>,
    pub conf: Vec<f32>,
}

impl Reading {
    pub fn coherent(&self) -> bool {
        let n = self.text.len();
        self.x.len() == n && self.y.len() == n && self.w.len() == n && self.h.len() == n && self.conf.len() == n
    }
}

pub struct TextMemory {
    reading: Mutex<Option<Reading>>,
    glass: Arc<GlassStamp>,
}

impl TextMemory {
    pub fn new(glass: Arc<GlassStamp>) -> Self {
        Self { reading: Mutex::new(None), glass }
    }

    pub fn glass(&self) -> &GlassStamp {
        &self.glass
    }

    /// Replace the message whole. An uneven column set is refused.
    pub fn publish(&self, reading: Reading) -> bool {
        if !reading.coherent() {
            return false;
        }
        *self.reading.lock().expect("screen text poisoned") = Some(reading);
        true
    }

    pub fn clear(&self) {
        *self.reading.lock().expect("screen text poisoned") = None;
    }

    pub fn snapshot(&self, now: Instant) -> Option<Reading> {
        let guard = self.reading.lock().expect("screen text poisoned");
        let reading = guard.as_ref()?;
        if now.saturating_duration_since(reading.at) > MAX_AGE {
            return None;
        }
        Some(reading.clone())
    }

    pub fn snapshot_json(&self, now: Instant) -> Value {
        let engine_now = self.glass.frame.load(Ordering::Relaxed);
        let Some(reading) = self.snapshot(now) else {
            return json!({
                "fresh": false,
                "seq": 0,
                "engineFrame": 0,
                "engineFrameNow": engine_now,
                "ageMs": null,
                "workedMs": null,
                "width": 0,
                "height": 0,
                "text": [],
                "x": [],
                "y": [],
                "w": [],
                "h": [],
                "conf": [],
            });
        };
        json!({
            "fresh": true,
            "seq": reading.seq,
            "engineFrame": reading.engine_frame,
            "engineFrameNow": engine_now,
            "ageMs": now.saturating_duration_since(reading.at).as_millis() as u64,
            "workedMs": reading.worked.as_millis() as u64,
            "width": reading.width,
            "height": reading.height,
            "text": reading.text,
            "x": reading.x,
            "y": reading.y,
            "w": reading.w,
            "h": reading.h,
            "conf": reading.conf,
        })
    }
}

/// Paint the disc flat so the body is not read as words. A side under 8, or a
/// rect that misses the frame, leaves the pixels alone.
pub fn blank_disc(rgba: &mut [u8], width: u32, height: u32, x: i32, y: i32, side: i32) {
    if side < 8 || width == 0 || height == 0 {
        return;
    }
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = ((x.saturating_add(side)).max(0) as u32).min(width);
    let y1 = ((y.saturating_add(side)).max(0) as u32).min(height);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for row in y0..y1 {
        for col in x0..x1 {
            let i = ((row * width + col) * 4) as usize;
            if i + 3 >= rgba.len() {
                return;
            }
            rgba[i] = 128;
            rgba[i + 1] = 128;
            rgba[i + 2] = 128;
            rgba[i + 3] = 255;
        }
    }
}

/// Tesseract TSV, tab-split, level 5. The word is every field from column 12 on,
/// so a quote inside a token stays one word.
pub fn parse_tsv(tsv: &str) -> (Vec<String>, Vec<i32>, Vec<i32>, Vec<i32>, Vec<i32>, Vec<f32>) {
    let mut text = Vec::new();
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut w = Vec::new();
    let mut h = Vec::new();
    let mut conf = Vec::new();
    for raw in tsv.split(['\n', '\r']) {
        if raw.is_empty() || raw.starts_with("level\t") {
            continue;
        }
        let cell: Vec<&str> = raw.split('\t').collect();
        if cell.len() < 12 || cell[0] != "5" {
            continue;
        }
        let word = cell[11..].join("\t").trim().to_string();
        if word.is_empty() || word.chars().count() > 40 || word.contains('\n') {
            continue;
        }
        let (Some(xi), Some(yi), Some(wi), Some(hi)) = (
            cell[6].parse::<i32>().ok(),
            cell[7].parse::<i32>().ok(),
            cell[8].parse::<i32>().ok(),
            cell[9].parse::<i32>().ok(),
        ) else {
            continue;
        };
        if wi <= 0 || hi <= 0 {
            continue;
        }
        let c = cell[10].parse::<f32>().unwrap_or(-1.0);
        text.push(word);
        x.push(xi);
        y.push(yi);
        w.push(wi);
        h.push(hi);
        conf.push(c);
    }
    (text, x, y, w, h, conf)
}

fn write_ppm(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    write!(file, "P6\n{width} {height}\n255\n")?;
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for px in rgba.chunks_exact(4) {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }
    file.write_all(&rgb)
}

fn ocr_frame(frame: &IngressFrame, dir: &std::path::Path) -> Result<Reading, String> {
    let ppm = dir.join("frame.ppm");
    let base = dir.join("frame");
    write_ppm(&ppm, frame.width, frame.height, &frame.rgba).map_err(|e| format!("ppm: {e}"))?;
    let output = Command::new("tesseract")
        .arg(&ppm)
        .arg(&base)
        .args(["--psm", "6", "tsv"])
        .output()
        .map_err(|e| format!("tesseract: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tesseract exited {}: {err}", output.status));
    }
    let tsv_path = PathBuf::from(format!("{}.tsv", base.display()));
    let tsv = std::fs::read_to_string(&tsv_path).map_err(|e| format!("tsv: {e}"))?;
    let (text, x, y, w, h, conf) = parse_tsv(&tsv);
    Ok(Reading {
        seq: 0,
        engine_frame: 0,
        at: Instant::now(),
        worked: Duration::ZERO,
        width: frame.width,
        height: frame.height,
        text,
        x,
        y,
        w,
        h,
        conf,
    })
}

/// Lazy worker. One copy, one recognizer pass, one posted message. The thread
/// owns its Wayland connection and drops it while the session is hidden.
pub fn spawn(memory: Arc<TextMemory>, enabled: Arc<AtomicBool>) {
    let _ = thread::Builder::new().name("screen-text".into()).spawn(move || {
        let dir = std::env::temp_dir().join("presence-screen-text");
        let _ = std::fs::create_dir_all(&dir);
        let mut src: Option<WlrScreenSource> = None;
        let mut seq = 0u64;
        loop {
            if !enabled.load(Ordering::Relaxed) {
                memory.clear();
                src = None;
                thread::sleep(Duration::from_millis(200));
                continue;
            }
            if src.is_none() {
                match WlrScreenSource::connect() {
                    Ok(next) => src = Some(next),
                    Err(error) => {
                        eprintln!("[screen-text] {error}");
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                }
            }
            let started = Instant::now();
            let engine_frame = memory.glass().frame.load(Ordering::Relaxed);
            let captured = src.as_mut().unwrap().next_frame();
            let frame = match captured {
                Ok(Some(mut frame)) => {
                    let glass = memory.glass();
                    blank_disc(
                        &mut frame.rgba,
                        frame.width,
                        frame.height,
                        glass.disc_x.load(Ordering::Relaxed),
                        glass.disc_y.load(Ordering::Relaxed),
                        glass.disc_side.load(Ordering::Relaxed),
                    );
                    frame
                }
                Ok(None) => {
                    eprintln!("[screen-text] capture returned no frame");
                    thread::sleep(Duration::from_millis(400));
                    continue;
                }
                Err(error) => {
                    eprintln!("[screen-text] {error}");
                    src = None;
                    thread::sleep(Duration::from_millis(400));
                    continue;
                }
            };
            match ocr_frame(&frame, &dir) {
                Ok(mut reading) => {
                    seq += 1;
                    reading.seq = seq;
                    reading.engine_frame = engine_frame;
                    reading.at = Instant::now();
                    reading.worked = started.elapsed();
                    let n = reading.text.len();
                    let ms = reading.worked.as_millis();
                    if memory.publish(reading) {
                        eprintln!("[screen-text] {n} words in {ms}ms (engine frame {engine_frame})");
                    }
                }
                Err(error) => {
                    eprintln!("[screen-text] {error}");
                    thread::sleep(Duration::from_millis(400));
                }
            }
            let spent = started.elapsed();
            if spent < MIN_GAP {
                thread::sleep(MIN_GAP - spent);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quoted_token_stays_one_word() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t10\t20\t30\t11\t88.5\t\"John\n";
        let (text, x, y, w, h, conf) = parse_tsv(tsv);
        assert_eq!(text, vec!["\"John".to_string()]);
        assert_eq!((x[0], y[0], w[0], h[0]), (10, 20, 30, 11));
        assert!((conf[0] - 88.5).abs() < 0.01);
    }

    #[test]
    fn an_uneven_message_is_refused_and_a_stale_one_is_hidden() {
        let glass = Arc::new(GlassStamp::new());
        let memory = TextMemory::new(glass);
        let bad = Reading {
            seq: 1,
            engine_frame: 1,
            at: Instant::now(),
            worked: Duration::from_millis(10),
            width: 10,
            height: 10,
            text: vec!["a".into()],
            x: vec![1],
            y: vec![1],
            w: vec![1],
            h: vec![],
            conf: vec![1.0],
        };
        assert!(!memory.publish(bad));
        assert!(memory.snapshot(Instant::now()).is_none());

        let old = Reading {
            seq: 2,
            engine_frame: 3,
            at: Instant::now() - Duration::from_secs(30),
            worked: Duration::from_millis(20),
            width: 8,
            height: 8,
            text: vec!["kept".into()],
            x: vec![1],
            y: vec![2],
            w: vec![3],
            h: vec![4],
            conf: vec![90.0],
        };
        assert!(memory.publish(old));
        assert!(memory.snapshot(Instant::now()).is_none());

        let fresh = Reading {
            seq: 3,
            engine_frame: 4,
            at: Instant::now(),
            worked: Duration::from_millis(20),
            width: 8,
            height: 8,
            text: vec!["kept".into()],
            x: vec![1],
            y: vec![2],
            w: vec![3],
            h: vec![4],
            conf: vec![90.0],
        };
        assert!(memory.publish(fresh));
        let got = memory.snapshot(Instant::now()).unwrap();
        assert_eq!(got.text, vec!["kept".to_string()]);
        assert!(got.coherent());
    }

    #[test]
    fn the_disc_is_grey_and_the_rest_of_the_frame_is_not() {
        let mut rgba = vec![0u8; 16 * 16 * 4];
        // Under eight pixels is not a body, so the copy stays as it was.
        blank_disc(&mut rgba, 16, 16, 2, 3, 4);
        let i = (3 * 16 + 2) * 4;
        assert_eq!(&rgba[i..i + 4], &[0, 0, 0, 0]);

        blank_disc(&mut rgba, 16, 16, 2, 3, 8);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[i..i + 4], &[128, 128, 128, 255]);
        let outside = (3 * 16 + 10) * 4;
        assert_eq!(&rgba[outside..outside + 4], &[0, 0, 0, 0]);

        blank_disc(&mut rgba, 16, 16, 0, 0, 0);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[i..i + 4], &[128, 128, 128, 255]);
    }
}
