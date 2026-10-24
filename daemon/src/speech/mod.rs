//! Speech: text -> local TTS (Piper CLI) -> Rhubarb CLI -> faceanim, played
//! against the audio clock. See docs/plans/speech_epic.md.
//!
//! Threading: Piper and Rhubarb each have a worker thread and exchange owned
//! data over channels, so the Vulkan/render thread (SplatPipeline is !Send)
//! never blocks. The render thread only calls `Speaker::poll` once per avatar
//! tick. Piper for the next sentence starts while Rhubarb is still on this one.
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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct SpeechConfig {
    pub piper_bin: PathBuf,
    pub piper_model: Option<PathBuf>,
    pub rhubarb_bin: PathBuf,
    /// `phonetic` (default; finishes inside a clip) or `pocketSphinx`.
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
            rhubarb_recognizer: env("PRESENCE_RHUBARB_RECOGNIZER").unwrap_or_else(|| "phonetic".into()),
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

enum PiperOut {
    Cached { id: u64, index: usize, bundle: Bundle },
    Wav { id: u64, index: usize, draft: pipeline::WavDraft },
    Failed { id: u64, index: usize, error: String },
}
enum MouthOut {
    Ready { id: u64, index: usize, bundle: Bundle },
    Failed { id: u64, index: usize, error: String },
}
struct PiperJob { id: u64, index: usize, text: String }
struct MouthJob { id: u64, index: usize, draft: pipeline::WavDraft }

struct Playing {
    index: usize,
    face: faceanim::FaceAnim,
    start: Instant,
    child: Option<Child>,
}

/// One utterance broken into sentence clips.
/// One Piper, one wav, one Rhubarb, and one ready bundle. A cache hit that
/// arrives while an earlier sentence is still being mouthed sits in `cached`.
struct Course {
    id: u64,
    sentences: Vec<String>,
    /// Next sentence index to send to Piper.
    next_index: usize,
    piper_inflight: Option<usize>,
    /// A wav whose mouth has not started. At most one.
    wav_held: Option<MouthJob>,
    rhubarb_inflight: Option<usize>,
    /// A finished bundle that is later than the sentence still in Rhubarb.
    cached: Option<(usize, Bundle)>,
    ready: Option<(usize, Bundle)>,
    /// A clip has started. Gaps after that keep the mouth in the utterance.
    started: bool,
    /// The clip that most recently started. A gap keeps reporting this index,
    /// so a follower does not look two sentences ahead while the next one cooks.
    last_index: Option<usize>,
    done: bool,
}

/// What `querySpeech` returns. The render thread publishes it; the socket reads it.
#[derive(Clone, Debug)]
pub struct SpeechView {
    pub generation: u64,
    pub phase: &'static str,
    pub index: Option<usize>,
    pub count: usize,
    pub sentence: String,
    pub sentences: Vec<String>,
}

impl SpeechView {
    fn idle() -> Self {
        Self { generation: 0, phase: "idle", index: None, count: 0, sentence: String::new(), sentences: Vec::new() }
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "generation": self.generation,
            "phase": self.phase,
            "index": self.index,
            "count": self.count,
            "sentence": self.sentence,
            "sentences": self.sentences,
        })
    }
}

#[derive(Clone)]
pub struct SpeechBoard {
    inner: Arc<Mutex<SpeechView>>,
}

impl SpeechBoard {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(SpeechView::idle())) }
    }

    fn publish(&self, view: SpeechView) {
        *self.inner.lock().expect("speech board") = view;
    }

    pub fn json(&self) -> serde_json::Value {
        self.inner.lock().expect("speech board").json()
    }
}

impl Default for SpeechBoard {
    fn default() -> Self { Self::new() }
}

pub enum SpeechPoll {
    Idle,
    Active(FaceFrame),
    /// The utterance ended (or was interrupted) since the last poll; face should go neutral.
    Finished,
}

pub struct Speaker {
    cfg: Arc<SpeechConfig>,
    piper_tx: Option<Sender<PiperJob>>,
    piper_rx: Option<Receiver<PiperOut>>,
    mouth_tx: Option<Sender<MouthJob>>,
    mouth_rx: Option<Receiver<MouthOut>>,
    latest: Arc<AtomicU64>,
    next_id: u64,
    playing: Option<Playing>,
    course: Option<Course>,
    board: SpeechBoard,
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
        Speaker {
            cfg: Arc::new(cfg),
            piper_tx: None, piper_rx: None, mouth_tx: None, mouth_rx: None,
            latest: Arc::new(AtomicU64::new(0)), next_id: 1,
            playing: None, course: None, board: SpeechBoard::new(), finished_pending: false,
            warned_no_visemes: false, last_error: None,
        }
    }

    pub fn board(&self) -> SpeechBoard {
        self.board.clone()
    }

    fn ensure_workers(&mut self) {
        if self.piper_tx.is_some() { return; }
        let (ptx, prx) = channel::<PiperJob>();
        let (potx, porx) = channel::<PiperOut>();
        let (mtx, mrx) = channel::<MouthJob>();
        let (motx, morx) = channel::<MouthOut>();
        let cfg = self.cfg.clone();
        let latest = self.latest.clone();
        let piper_cfg = cfg.clone();
        let piper_latest = latest.clone();
        std::thread::Builder::new().name("speech-piper".into()).spawn(move || {
            while let Ok(job) = prx.recv() {
                if piper_latest.load(Ordering::SeqCst) != job.id { continue; }
                let cancelled = || piper_latest.load(Ordering::SeqCst) != job.id;
                let msg = match pipeline::synthesize(&piper_cfg, &job.text, &cancelled) {
                    Ok(pipeline::Synth::Cached(bundle)) => PiperOut::Cached { id: job.id, index: job.index, bundle },
                    Ok(pipeline::Synth::Wav(draft)) => PiperOut::Wav { id: job.id, index: job.index, draft },
                    Err(error) => PiperOut::Failed { id: job.id, index: job.index, error },
                };
                if potx.send(msg).is_err() { break; }
            }
        }).expect("spawn speech-piper");
        std::thread::Builder::new().name("speech-rhubarb".into()).spawn(move || {
            while let Ok(job) = mrx.recv() {
                if latest.load(Ordering::SeqCst) != job.id { continue; }
                let cancelled = || latest.load(Ordering::SeqCst) != job.id;
                let msg = match pipeline::mouth(&cfg, job.draft, &cancelled) {
                    Ok(bundle) => MouthOut::Ready { id: job.id, index: job.index, bundle },
                    Err(error) => MouthOut::Failed { id: job.id, index: job.index, error },
                };
                if motx.send(msg).is_err() { break; }
            }
        }).expect("spawn speech-rhubarb");
        self.piper_tx = Some(ptx);
        self.piper_rx = Some(porx);
        self.mouth_tx = Some(mtx);
        self.mouth_rx = Some(morx);
    }

    /// End the clip that is playing. `utterance_done` asks the next poll to
    /// return the face to neutral. A sentence boundary does not.
    fn end_clip(&mut self, utterance_done: bool) {
        if let Some(mut p) = self.playing.take() {
            if let Some(c) = p.child.as_mut() { let _ = c.kill(); let _ = c.wait(); }
        }
        if utterance_done { self.finished_pending = true; }
    }

    fn publish(&mut self) {
        let view = match &self.course {
            Some(c) => {
                let phase = if c.done {
                    "idle"
                } else if self.playing.is_some() {
                    "play"
                } else if c.started {
                    "gap"
                } else {
                    "synth"
                };
                let cooking = c.rhubarb_inflight.or(c.wav_held.as_ref().map(|w| w.index)).or(c.piper_inflight);
                let index = self.playing.as_ref().map(|p| p.index).or(if c.started { c.last_index } else { cooking });
                let sentence = index.and_then(|i| c.sentences.get(i)).cloned().unwrap_or_default();
                SpeechView {
                    generation: c.id, phase, index, count: c.sentences.len(), sentence,
                    sentences: c.sentences.clone(),
                }
            }
            None => SpeechView { generation: self.latest.load(Ordering::SeqCst), ..SpeechView::idle() },
        };
        self.board.publish(view);
    }

    /// Hand a held wav to Rhubarb when the mouth is free and the ready slot is open,
    /// then start Piper when that wav slot is empty. A finished bundle waiting to
    /// play, with the mouth idle, holds the next Piper so bundles do not pile up.
    fn pump(&mut self) {
        let (hand, start) = {
            let Some(course) = self.course.as_mut() else { return };
            if course.done { return; }
            if course.ready.is_none() && course.rhubarb_inflight.is_none() {
                if let Some(cached) = course.cached.take() { course.ready = Some(cached); }
            }
            let hand = if course.rhubarb_inflight.is_none() && course.ready.is_none() { course.wav_held.take() } else { None };
            if let Some(job) = &hand { course.rhubarb_inflight = Some(job.index); }
            let slot_full = course.wav_held.is_some() || course.cached.is_some();
            let caught_up = course.ready.is_some() && course.rhubarb_inflight.is_none() && !slot_full;
            let start = if course.piper_inflight.is_none() && !slot_full && !caught_up && course.next_index < course.sentences.len() {
                let index = course.next_index;
                let text = course.sentences[index].clone();
                let id = course.id;
                course.next_index += 1;
                course.piper_inflight = Some(index);
                Some(PiperJob { id, index, text })
            } else {
                None
            };
            (hand, start)
        };
        if let Some(job) = hand {
            if let Some(tx) = &self.mouth_tx { let _ = tx.send(job); }
        }
        if let Some(job) = start {
            if let Some(tx) = &self.piper_tx { let _ = tx.send(job); }
        }
    }

    fn finish_course(&mut self) {
        if let Some(course) = self.course.as_mut() {
            course.done = true;
            course.piper_inflight = None;
            course.wav_held = None;
            course.rhubarb_inflight = None;
            course.cached = None;
            course.ready = None;
        }
        self.finished_pending = true;
    }

    /// Queue an utterance. Any current audio, face motion, or pending job is cancelled first.
    /// The text is split into sentences. The first clip plays while the next one is prepared.
    pub fn speak(&mut self, text: &str) -> Result<(), String> {
        let normalized = pipeline::normalize_text(text)?;
        let sentences = pipeline::split_sentences(&normalized);
        if sentences.is_empty() { return Err("empty speech text".into()); }
        self.end_clip(self.playing.is_some());
        let id = self.next_id;
        self.next_id += 1;
        self.latest.store(id, Ordering::SeqCst);
        self.ensure_workers();
        self.last_error = None;
        self.course = Some(Course {
            id, sentences, next_index: 0, piper_inflight: None, wav_held: None, rhubarb_inflight: None,
            cached: None, ready: None, started: false, last_index: None, done: false,
        });
        self.pump();
        self.publish();
        Ok(())
    }

    /// Interrupt: stops audio and face together and cancels any in-flight synthesis.
    pub fn stop(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        self.latest.store(id, Ordering::SeqCst); // no job carries this id: cancels pending work
        self.end_clip(self.playing.is_some());
        self.course = None;
        // Even a stop during synthesis ends the utterance, so a speech-held tint can drop.
        self.finished_pending = true;
        self.publish();
    }

    pub fn audio_running(&mut self) -> bool {
        self.playing.as_mut().and_then(|p| p.child.as_mut()).map(|c| matches!(c.try_wait(), Ok(None))).unwrap_or(false)
    }

    fn start_playback(&mut self, index: usize, bundle: Bundle) {
        let child = match player_argv(&self.cfg) {
            Some(argv) => match Command::new(&argv[0]).args(&argv[1..]).arg(&bundle.wav).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
                Ok(c) => Some(c),
                Err(e) => { self.report(format!("audio player '{}' failed to start: {e}; animating silently", argv[0])); None }
            },
            None => { self.report("no audio player found (pw-play, paplay, aplay, ffplay); set PRESENCE_AUDIO_PLAYER; animating silently".into()); None }
        };
        let count = self.course.as_ref().map(|c| c.sentences.len()).unwrap_or(1);
        println!(
            "[speech] sentence {}/{} playing {} ({:.2}s, cache {})",
            index + 1, count, bundle.dir.display(), bundle.face.duration, if bundle.cache_hit { "hit" } else { "miss" }
        );
        if let Some(course) = self.course.as_mut() {
            course.started = true;
            course.last_index = Some(index);
        }
        self.playing = Some(Playing { index, face: bundle.face, start: Instant::now(), child });
    }

    fn report(&mut self, e: String) {
        eprintln!("[speech] {e}");
        self.last_error = Some(e);
    }

    fn offer_bundle(&mut self, index: usize, bundle: Bundle) {
        let Some(course) = self.course.as_mut() else { return };
        if course.done { return; }
        let earlier_mouth = course.rhubarb_inflight.is_some_and(|i| i < index) || course.wav_held.as_ref().is_some_and(|w| w.index < index);
        if earlier_mouth || course.ready.is_some() {
            if course.cached.is_none() { course.cached = Some((index, bundle)); }
        } else if course.ready.is_none() {
            course.ready = Some((index, bundle));
        }
    }

    fn accept_messages(&mut self) {
        let mut piper_in = vec![];
        let mut mouth_in = vec![];
        if let Some(rx) = &self.piper_rx { while let Ok(m) = rx.try_recv() { piper_in.push(m); } }
        if let Some(rx) = &self.mouth_rx { while let Ok(m) = rx.try_recv() { mouth_in.push(m); } }
        let current = self.latest.load(Ordering::SeqCst);
        for m in piper_in {
            match m {
                PiperOut::Cached { id, index, bundle } if id == current => {
                    if let Some(course) = self.course.as_mut() {
                        if course.piper_inflight == Some(index) { course.piper_inflight = None; }
                    }
                    self.offer_bundle(index, bundle);
                }
                PiperOut::Wav { id, index, draft } if id == current => {
                    if let Some(course) = self.course.as_mut() {
                        if course.piper_inflight == Some(index) { course.piper_inflight = None; }
                        if !course.done && course.wav_held.is_none() {
                            course.wav_held = Some(MouthJob { id, index, draft });
                        }
                    }
                }
                PiperOut::Failed { id, index, error } if id == current => {
                    if let Some(course) = self.course.as_mut() {
                        if course.piper_inflight == Some(index) { course.piper_inflight = None; }
                    }
                    let count = self.course.as_ref().map(|c| c.sentences.len()).unwrap_or(0);
                    self.report(format!("sentence {}/{} failed: {error}", index + 1, count));
                }
                _ => {}
            }
        }
        for m in mouth_in {
            match m {
                MouthOut::Ready { id, index, bundle } if id == current => {
                    if let Some(course) = self.course.as_mut() {
                        if course.rhubarb_inflight == Some(index) { course.rhubarb_inflight = None; }
                    }
                    self.offer_bundle(index, bundle);
                }
                MouthOut::Failed { id, index, error } if id == current => {
                    if let Some(course) = self.course.as_mut() {
                        if course.rhubarb_inflight == Some(index) { course.rhubarb_inflight = None; }
                    }
                    let count = self.course.as_ref().map(|c| c.sentences.len()).unwrap_or(0);
                    self.report(format!("sentence {}/{} failed: {error}", index + 1, count));
                }
                _ => {}
            }
        }
        self.pump();
    }

    fn clip_finished(&mut self) -> bool {
        let Some(p) = self.playing.as_mut() else { return false };
        let elapsed = (p.start.elapsed().as_secs_f32() - self.cfg.latency.as_secs_f32()).max(0.0);
        let audio_alive = p.child.as_mut().map(|c| matches!(c.try_wait(), Ok(None)));
        let ended_early = audio_alive == Some(false) && elapsed + 0.15 < p.face.duration;
        let done = elapsed >= p.face.duration + 0.05 && (audio_alive != Some(true) || elapsed > p.face.duration + 1.5);
        ended_early || done
    }

    /// Advance the sentence course. A finished clip does not end the utterance
    /// while another sentence is queued or still being prepared.
    fn drive(&mut self) {
        if self.clip_finished() { self.end_clip(false); }
        let ready = self.course.as_mut().and_then(|c| if c.done { None } else { c.ready.take() });
        if self.playing.is_none() {
            if let Some((index, bundle)) = ready {
                self.start_playback(index, bundle);
            }
        } else if let Some(course) = self.course.as_mut() {
            // Put an unused bundle back. Playback of the current clip is still going.
            if course.ready.is_none() { course.ready = ready; }
        }
        self.pump();
        let finished = match &self.course {
            Some(c) if !c.done
                && self.playing.is_none()
                && c.piper_inflight.is_none()
                && c.wav_held.is_none()
                && c.rhubarb_inflight.is_none()
                && c.cached.is_none()
                && c.ready.is_none()
                && c.next_index >= c.sentences.len() => true,
            _ => false,
        };
        if finished { self.finish_course(); }
    }

    fn active_frame(&mut self, morph_names: &[String], elapsed: f32) -> SpeechPoll {
        let sample = self.playing.as_ref().map(|p| p.face.sample(elapsed)).unwrap_or_default();
        let mut morphs = vec![0.0; morph_names.len()];
        let mut hit = sample.visemes.is_empty();
        for (name, w) in &sample.visemes {
            if let Some(i) = morph_names.iter().position(|n| n == name) { morphs[i] = w.clamp(0.0, 1.0); hit = true; }
        }
        if !hit && !self.warned_no_visemes && self.playing.is_some() {
            self.warned_no_visemes = true;
            eprintln!("[speech] GLB has no viseme_* morph targets; driving the jaw only");
        }
        SpeechPoll::Active(FaceFrame { jaw_open: sample.jaw.clamp(0.0, 1.0), morphs })
    }

    /// Called once per avatar tick on the render thread. Never blocks.
    pub fn poll(&mut self, morph_names: &[String]) -> SpeechPoll {
        self.accept_messages();
        self.drive();
        self.publish();
        if let Some(p) = &self.playing {
            let elapsed = (p.start.elapsed().as_secs_f32() - self.cfg.latency.as_secs_f32()).max(0.0);
            return self.active_frame(morph_names, elapsed);
        }
        let in_gap = self.course.as_ref().map(|c| c.started && !c.done).unwrap_or(false);
        if in_gap {
            return self.active_frame(morph_names, f32::MAX);
        }
        if self.finished_pending {
            self.finished_pending = false;
            return SpeechPoll::Finished;
        }
        SpeechPoll::Idle
    }
}

impl Drop for Speaker {
    fn drop(&mut self) { self.stop(); }
}
