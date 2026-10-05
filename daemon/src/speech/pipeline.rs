//! Utterance preparation: normalize -> cache lookup -> Piper (WAV) -> Rhubarb
//! (cues) -> faceanim. Runs on the worker thread only; spawns the TTS and
//! Rhubarb *CLIs* as child processes so no GPL code is linked into the daemon.
use super::faceanim::{FaceAnim, MAPPING_VERSION};
use super::SpeechConfig;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_TEXT_CHARS: usize = 2000;

#[derive(Debug)]
pub struct Bundle {
    pub dir: PathBuf,
    pub wav: PathBuf,
    pub face: FaceAnim,
    pub cache_hit: bool,
}

/// Collapse whitespace; reject empty or oversized text.
pub fn normalize_text(text: &str) -> Result<String, String> {
    let n = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() { return Err("empty speech text".into()); }
    if n.chars().count() > MAX_TEXT_CHARS { return Err(format!("speech text longer than {MAX_TEXT_CHARS} characters")); }
    Ok(n)
}

/// Cache key: normalized text, voice id, rate, engine id, viseme-map version.
pub fn cache_key(cfg: &SpeechConfig, normalized: &str) -> String {
    let mut h = Sha256::new();
    for part in [
        "speech-bundle-v1".to_string(),
        normalized.to_string(),
        cfg.voice_id(),
        "rate=1.0".to_string(),
        format!("engine={}", cfg.engine_id),
        format!("recognizer={}", cfg.rhubarb_recognizer),
        format!("mapping={MAPPING_VERSION}"),
    ] {
        h.update(part.as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
}

/// Duration in seconds of a PCM WAV file (RIFF/fmt/data).
pub fn wav_duration(path: &Path) -> Result<f32, String> {
    let d = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"WAVE" { return Err("not a WAV file".into()); }
    let (mut off, mut byte_rate, mut data_len) = (12usize, 0u32, None);
    while off + 8 <= d.len() {
        let id = &d[off..off + 4];
        let len = u32::from_le_bytes([d[off + 4], d[off + 5], d[off + 6], d[off + 7]]) as usize;
        if id == b"fmt " && off + 8 + 12 <= d.len() {
            byte_rate = u32::from_le_bytes([d[off + 16], d[off + 17], d[off + 18], d[off + 19]]);
        } else if id == b"data" {
            data_len = Some(len.min(d.len().saturating_sub(off + 8)));
            break;
        }
        off += 8 + len + (len & 1);
    }
    match (byte_rate, data_len) {
        (br, Some(n)) if br > 0 => Ok(n as f32 / br as f32),
        _ => Err("WAV has no usable fmt/data chunk".into()),
    }
}

/// Run a child process to completion, polling `cancelled` so a superseded
/// utterance kills its subprocess promptly.
fn run(mut cmd: Command, stdin_text: Option<&str>, what: &str, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    cmd.stdin(if stdin_text.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd.spawn().map_err(|e| format!("{what}: cannot start ({e}); is it installed and configured?"))?;
    if let (Some(text), Some(mut stdin)) = (stdin_text, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() { return Ok(()); }
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() { use std::io::Read; let _ = e.read_to_string(&mut err); }
                let tail: String = err.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
                return Err(format!("{what} failed ({status}): {tail}"));
            }
            Ok(None) => {}
            Err(e) => return Err(format!("{what}: {e}")),
        }
        if cancelled() { let _ = child.kill(); let _ = child.wait(); return Err("cancelled".into()); }
        if started.elapsed() > Duration::from_secs(120) { let _ = child.kill(); let _ = child.wait(); return Err(format!("{what} timed out")); }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn load_cached(dir: &Path) -> Option<Bundle> {
    let wav = dir.join("speech.wav");
    let face = FaceAnim::from_json(&std::fs::read_to_string(dir.join("faceanim.json")).ok()?).ok()?;
    if !wav.is_file() { return None; }
    Some(Bundle { dir: dir.to_path_buf(), wav, face, cache_hit: true })
}

/// Produce (or fetch) the bundle for `text`.
pub fn prepare(cfg: &SpeechConfig, text: &str, cancelled: &dyn Fn() -> bool) -> Result<Bundle, String> {
    let normalized = normalize_text(text)?;
    let key = cache_key(cfg, &normalized);
    let dir = cfg.cache_dir.join(&key);
    if let Some(b) = load_cached(&dir) { return Ok(b); }
    let model = cfg.piper_model.as_ref().ok_or("no TTS voice configured: set PRESENCE_PIPER_MODEL to a Piper .onnx voice")?;

    std::fs::create_dir_all(&cfg.cache_dir).map_err(|e| format!("cache dir: {e}"))?;
    let tmp = cfg.cache_dir.join(format!("{key}.tmp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("cache tmp: {e}"))?;
    let result = (|| -> Result<(), String> {
        let t0 = Instant::now();
        let wav = tmp.join("speech.wav");
        let mut piper = Command::new(&cfg.piper_bin);
        piper.arg("-m").arg(model).arg("-f").arg(&wav); // short flags: valid for piper and piper1-gpl
        run(piper, Some(&normalized), "piper", cancelled)?;
        let tts_ms = t0.elapsed().as_millis();
        let wav_secs = wav_duration(&wav)?;
        if cancelled() { return Err("cancelled".into()); }

        let dialog = tmp.join("dialog.txt");
        std::fs::write(&dialog, &normalized).map_err(|e| e.to_string())?;
        let json = tmp.join("rhubarb.json");
        let t1 = Instant::now();
        let mut rb = Command::new(&cfg.rhubarb_bin);
        rb.args(["-f", "json", "--extendedShapes", "GHX", "-q", "-r"]).arg(&cfg.rhubarb_recognizer).arg("-d").arg(&dialog).arg("-o").arg(&json).arg(&wav);
        run(rb, None, "rhubarb", cancelled)?;
        let rhubarb_ms = t1.elapsed().as_millis();
        let mut face = FaceAnim::from_rhubarb_json(&std::fs::read_to_string(&json).map_err(|e| format!("rhubarb.json: {e}"))?)?;
        // The WAV is the clock: trust its length over Rhubarb's metadata.
        face.duration = wav_secs;
        std::fs::write(tmp.join("faceanim.json"), face.to_json()).map_err(|e| e.to_string())?;
        let meta = serde_json::json!({
            "key": key, "text": text, "normalized": normalized, "voice": cfg.voice_id(), "engine": cfg.engine_id,
            "recognizer": cfg.rhubarb_recognizer, "mappingVersion": MAPPING_VERSION,
            "wavSeconds": wav_secs, "ttsMs": tts_ms, "rhubarbMs": rhubarb_ms,
        });
        std::fs::write(tmp.join("speech.json"), serde_json::to_string_pretty(&meta).unwrap_or_default()).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result { let _ = std::fs::remove_dir_all(&tmp); return Err(e); }
    if dir.exists() { let _ = std::fs::remove_dir_all(&tmp); } else if let Err(e) = std::fs::rename(&tmp, &dir) {
        let _ = std::fs::remove_dir_all(&tmp);
        if !dir.exists() { return Err(format!("cache publish: {e}")); }
    }
    let mut b = load_cached(&dir).ok_or("cache bundle unreadable after write")?;
    b.cache_hit = false;
    Ok(b)
}
