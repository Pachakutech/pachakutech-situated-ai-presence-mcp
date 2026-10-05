//! Speech: text -> local TTS (Piper CLI) -> Rhubarb CLI -> faceanim, played
//! against the audio clock. See docs/plans/speech_epic.md.
//!
//! Threading: all slow work (TTS, Rhubarb, cache IO) runs on one worker thread
//! that exchanges only owned data over channels, so the Vulkan/render thread
//! (SplatPipeline is !Send) never blocks. The render thread only calls
//! `Speaker::poll` once per avatar tick.
//!
//! Audio clock: the player is an out-of-process CLI (pw-play/paplay/aplay/
//! ffplay). The face clock starts when the player process is spawned, minus a
//! tunable `PRESENCE_AUDIO_LATENCY_MS`. This is not sample-accurate; see
//! docs/plans/avatar-speech-epic-done.md.
pub mod faceanim;
pub mod pipeline;
#[cfg(test)]
mod tests;

use crate::avatar::FaceFrame;
use pipeline::Bundle;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct SpeechConfig {
    pub piper_bin: PathBuf,
    pub piper_model: Option<PathBuf>,
    pub rhubarb_bin: PathBuf,
    /// `pocketSphinx` (English, default) or `phonetic` (language-independent).
    pub rhubarb_recognizer: String,
    pub cache_dir: PathBuf,
    /// Free-form engine/version label folded into the cache key.
    pub engine_id: String,
    /// Player argv prefix; the WAV path is appended. None = autodetect.
    pub player: Option<Vec<String>>,
    pub latency: Duration,
}

impl SpeechConfig {
    pub fn from_env() -> SpeechConfig {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let cache_dir = env("PRESENCE_SPEECH_CACHE").map(PathBuf::from).unwrap_or_else(|| {
            let base = env("XDG_CACHE_HOME").map(PathBuf::from)
                .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".cache")))
                .unwrap_or_else(|| PathBuf::from("cache"));
            base.join("pachakutech").join("speech")
        });
        SpeechConfig {
            piper_bin: env("PRESENCE_PIPER_BIN").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("piper")),
            piper_model: env("PRESENCE_PIPER_MODEL").map(PathBuf::from),
            rhubarb_bin: env("PRESENCE_RHUBARB_BIN").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("rhubarb")),
            rhubarb_recognizer: env("PRESENCE_RHUBARB_RECOGNIZER").unwrap_or_else(|| "pocketSphinx".into()),
            cache_dir,
            engine_id: env("PRESENCE_TTS_ENGINE_ID").unwrap_or_else(|| "piper".into()),
            player: env("PRESENCE_AUDIO_PLAYER").map(|s| s.split_whitespace().map(String::from).collect()),
            latency: Duration::from_millis(env("PRESENCE_AUDIO_LATENCY_MS").and_then(|v| v.parse().ok()).unwrap_or(0)),
        }
    }

    /// Voice identity for the cache key: model file name + size.
    pub fn voice_id(&self) -> String {
        match &self.piper_model {
            Some(p) => {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                format!("{}:{size}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
            None => "none".into(),
        }
    }
}

enum Msg {
    Ready { id: u64, bundle: Bundle },
    Failed { id: u64, error: String },
}
struct Job { id: u64, text: String }

struct Playing {
    face: faceanim::FaceAnim,
    start: Instant,
    child: Option<Child>,
}

pub enum SpeechPoll {
    Idle,
    Active(FaceFrame),
    /// The utterance ended (or was interrupted) since the last poll; face should go neutral.
    Finished,
}

pub struct Speaker {
    cfg: Arc<SpeechConfig>,
    tx: Option<Sender<Job>>,
    rx: Option<Receiver<Msg>>,
    latest: Arc<AtomicU64>,
    next_id: u64,
    playing: Option<Playing>,
    finished_pending: bool,
    warned_no_visemes: bool,
    pub last_error: Option<String>,
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|c| c.is_file()))
}

fn player_argv(cfg: &SpeechConfig) -> Option<Vec<String>> {
    if let Some(p) = &cfg.player { return Some(p.clone()); }
    for cand in [vec!["pw-play"], vec!["paplay"], vec!["aplay", "-q"], vec!["ffplay", "-nodisp", "-autoexit", "-loglevel", "quiet"]] {
        if find_in_path(cand[0]).is_some() { return Some(cand.into_iter().map(String::from).collect()); }
    }
    None
}

impl Speaker {
    pub fn new(cfg: SpeechConfig) -> Speaker {
        Speaker { cfg: Arc::new(cfg), tx: None, rx: None, latest: Arc::new(AtomicU64::new(0)), next_id: 1, playing: None, finished_pending: false, warned_no_visemes: false, last_error: None }
    }

    fn ensure_worker(&mut self) {
        if self.tx.is_some() { return; }
        let (jtx, jrx) = channel::<Job>();
        let (mtx, mrx) = channel::<Msg>();
        let cfg = self.cfg.clone();
        let latest = self.latest.clone();
        std::thread::Builder::new().name("speech-worker".into()).spawn(move || {
            while let Ok(job) = jrx.recv() {
                if latest.load(Ordering::SeqCst) != job.id { continue; } // superseded while queued
                let cancelled = || latest.load(Ordering::SeqCst) != job.id;
                let msg = match pipeline::prepare(&cfg, &job.text, &cancelled) {
                    Ok(bundle) => Msg::Ready { id: job.id, bundle },
                    Err(error) => Msg::Failed { id: job.id, error },
                };
                if mtx.send(msg).is_err() { break; }
            }
        }).expect("spawn speech worker");
        self.tx = Some(jtx);
        self.rx = Some(mrx);
    }

    fn stop_playback(&mut self) {
        if let Some(mut p) = self.playing.take() {
            if let Some(c) = p.child.as_mut() { let _ = c.kill(); let _ = c.wait(); }
            self.finished_pending = true;
        }
    }

    /// Queue an utterance. Any current audio, face motion, or pending job is cancelled first.
    pub fn speak(&mut self, text: &str) -> Result<(), String> {
        let normalized = pipeline::normalize_text(text)?;
        self.stop_playback();
        let id = self.next_id;
        self.next_id += 1;
        self.latest.store(id, Ordering::SeqCst);
        self.ensure_worker();
        self.last_error = None;
        self.tx.as_ref().expect("worker").send(Job { id, text: normalized }).map_err(|_| "speech worker is gone".to_string())
    }

    /// Interrupt: stops audio and face together and cancels any in-flight synthesis.
    pub fn stop(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        self.latest.store(id, Ordering::SeqCst); // no job carries this id: cancels pending work
        self.stop_playback();
    }

    pub fn audio_running(&mut self) -> bool {
        self.playing.as_mut().and_then(|p| p.child.as_mut()).map(|c| matches!(c.try_wait(), Ok(None))).unwrap_or(false)
    }

    fn start_playback(&mut self, bundle: Bundle) {
        let child = match player_argv(&self.cfg) {
            Some(argv) => match Command::new(&argv[0]).args(&argv[1..]).arg(&bundle.wav).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
                Ok(c) => Some(c),
                Err(e) => { self.report(format!("audio player '{}' failed to start: {e}; animating silently", argv[0])); None }
            },
            None => { self.report("no audio player found (pw-play, paplay, aplay, ffplay); set PRESENCE_AUDIO_PLAYER; animating silently".into()); None }
        };
        println!("[speech] playing {} ({:.2}s, cache {})", bundle.dir.display(), bundle.face.duration, if bundle.cache_hit { "hit" } else { "miss" });
        self.playing = Some(Playing { face: bundle.face, start: Instant::now(), child });
    }

    fn report(&mut self, e: String) {
        eprintln!("[speech] {e}");
        self.last_error = Some(e);
    }

    /// Called once per avatar tick on the render thread. Never blocks.
    pub fn poll(&mut self, morph_names: &[String]) -> SpeechPoll {
        let mut incoming = vec![];
        if let Some(rx) = &self.rx { while let Ok(m) = rx.try_recv() { incoming.push(m); } }
        for m in incoming {
            match m {
                Msg::Ready { id, bundle } if id == self.latest.load(Ordering::SeqCst) => self.start_playback(bundle),
                Msg::Failed { id, error } if id == self.latest.load(Ordering::SeqCst) => self.report(error),
                _ => {} // stale
            }
        }
        if let Some(p) = self.playing.as_mut() {
            let elapsed = (p.start.elapsed().as_secs_f32() - self.cfg.latency.as_secs_f32()).max(0.0);
            let audio_alive = p.child.as_mut().map(|c| matches!(c.try_wait(), Ok(None)));
            let ended_early = audio_alive == Some(false) && elapsed + 0.15 < p.face.duration;
            let done = elapsed >= p.face.duration + 0.05 && (audio_alive != Some(true) || elapsed > p.face.duration + 1.5);
            if ended_early || done {
                self.stop_playback();
            } else {
                let s = p.face.sample(elapsed);
                let mut morphs = vec![0.0; morph_names.len()];
                let mut hit = s.visemes.is_empty();
                for (name, w) in &s.visemes {
                    if let Some(i) = morph_names.iter().position(|n| n == name) { morphs[i] = w.clamp(0.0, 1.0); hit = true; }
                }
                if !hit && !self.warned_no_visemes {
                    self.warned_no_visemes = true;
                    eprintln!("[speech] GLB has no viseme_* morph targets; driving the jaw only");
                }
                return SpeechPoll::Active(FaceFrame { jaw_open: s.jaw.clamp(0.0, 1.0), morphs });
            }
        }
        if self.finished_pending { self.finished_pending = false; return SpeechPoll::Finished; }
        SpeechPoll::Idle
    }
}

impl Drop for Speaker {
    fn drop(&mut self) { self.stop_playback(); }
}
