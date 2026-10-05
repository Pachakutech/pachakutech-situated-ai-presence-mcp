use super::faceanim::*;
use super::pipeline::*;
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const RHUBARB_JSON: &str = r#"{"metadata":{"soundFile":"x.wav","duration":1.00},"mouthCues":[
 {"start":0.00,"end":0.10,"value":"X"},{"start":0.10,"end":0.30,"value":"D"},
 {"start":0.30,"end":0.50,"value":"A"},{"start":0.50,"end":0.70,"value":"F"},{"start":0.70,"end":1.00,"value":"X"}]}"#;

fn silent_wav(path: &Path, secs: f32) {
    let rate = 8000u32;
    let n = (rate as f32 * secs) as u32 * 2;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF"); b.extend_from_slice(&(36 + n).to_le_bytes()); b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes()); b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes()); b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data"); b.extend_from_slice(&n.to_le_bytes()); b.extend(std::iter::repeat(0u8).take(n as usize));
    std::fs::write(path, b).unwrap();
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Fake piper/rhubarb/player that log every invocation to calls.log.
struct Fake { dir: PathBuf, cfg: SpeechConfig }
fn fake(name: &str, with_model: bool) -> Fake {
    let dir = std::env::temp_dir().join(format!("speech-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    silent_wav(&dir.join("fixture.wav"), 1.0);
    std::fs::write(dir.join("fixture.json"), RHUBARB_JSON).unwrap();
    std::fs::write(dir.join("model.onnx"), b"model").unwrap();
    let log = dir.join("calls.log");
    script(&dir.join("piper"), &format!("echo piper >> {log:?}\nwhile [ $# -gt 0 ]; do [ \"$1\" = -f ] && out=$2; shift; done\ncat >/dev/null\ncp {:?} \"$out\"", dir.join("fixture.wav")));
    script(&dir.join("rhubarb"), &format!("echo rhubarb >> {log:?}\nwhile [ $# -gt 0 ]; do [ \"$1\" = -o ] && out=$2; shift; done\ncp {:?} \"$out\"", dir.join("fixture.json")));
    script(&dir.join("player"), &format!("echo player >> {log:?}\nsleep 5"));
    let cfg = SpeechConfig {
        piper_bin: dir.join("piper"), piper_model: if with_model { Some(dir.join("model.onnx")) } else { None },
        rhubarb_bin: dir.join("rhubarb"), rhubarb_recognizer: "phonetic".into(), cache_dir: dir.join("cache"),
        engine_id: "fake".into(), player: Some(vec![dir.join("player").to_string_lossy().into_owned()]), latency: Duration::ZERO,
    };
    Fake { dir, cfg }
}
fn calls(f: &Fake, what: &str) -> usize {
    std::fs::read_to_string(f.dir.join("calls.log")).unwrap_or_default().lines().filter(|l| *l == what).count()
}

fn morph_names() -> Vec<String> {
    ["body0", "viseme_aa", "viseme_PP", "viseme_U", "viseme_sil"].iter().map(|s| s.to_string()).collect()
}

#[test]
fn rhubarb_semantics_follow_upstream_not_the_old_plan_table() {
    // A is closed lips (P/B/M): jaw shut + PP. B is slightly open clenched teeth.
    let a = shape_target('A'); let b = shape_target('B'); let d = shape_target('D');
    assert_eq!(a.viseme, "viseme_PP"); assert_eq!(a.jaw, 0.0);
    assert!(b.jaw > a.jaw && b.jaw < 0.3);
    assert!(d.jaw > 0.7 && d.viseme == "viseme_aa");
    assert_eq!(shape_target('X').weight, 0.0);
}

#[test]
fn faceanim_samples_cues_with_crossfade_and_neutral_ends() {
    let f = FaceAnim::from_rhubarb_json(RHUBARB_JSON).unwrap();
    assert_eq!(f.sample(0.05), FaceSample::default().tap_jaw(0.0));
    let open = f.sample(0.25); // inside D, past the 60 ms fade
    assert!((open.jaw - 0.85).abs() < 1e-4 && open.visemes.iter().any(|(n, w)| *n == "viseme_aa" && *w > 0.99));
    let mid = f.sample(0.10 + 0.03); // halfway through fade X->D
    assert!(mid.jaw > 0.1 && mid.jaw < 0.8, "fade jaw {}", mid.jaw);
    let closed = f.sample(0.4); // A: lips closed
    assert!(closed.jaw < 1e-4 && closed.visemes.iter().any(|(n, _)| *n == "viseme_PP"));
    assert_eq!(f.sample(1.5), FaceSample::default());
    assert_eq!(f.sample(-0.1), FaceSample::default());
    // json round trip
    let g = FaceAnim::from_json(&f.to_json()).unwrap();
    assert_eq!(g.cues, f.cues);
}
trait Tap { fn tap_jaw(self, j: f32) -> Self; }
impl Tap for FaceSample { fn tap_jaw(mut self, j: f32) -> Self { self.jaw = j; self } }

#[test]
fn bad_rhubarb_json_is_rejected() {
    assert!(FaceAnim::from_rhubarb_json("{}").is_err());
    assert!(FaceAnim::from_rhubarb_json(r#"{"mouthCues":[]}"#).is_err());
    assert!(FaceAnim::from_rhubarb_json(r#"{"mouthCues":[{"start":0.5,"end":0.6,"value":"A"},{"start":0.1,"end":0.2,"value":"B"}]}"#).is_err());
}

#[test]
fn text_normalization_and_cache_key() {
    assert_eq!(normalize_text("  Hello \n  world ").unwrap(), "Hello world");
    assert!(normalize_text("   ").is_err());
    assert!(normalize_text(&"x".repeat(MAX_TEXT_CHARS + 1)).is_err());
    let f = fake("key", true);
    let k1 = cache_key(&f.cfg, "Hello world");
    assert_eq!(k1, cache_key(&f.cfg, "Hello world"));
    assert_ne!(k1, cache_key(&f.cfg, "Hello there"));
    let mut other = f.cfg.clone(); other.engine_id = "other".into();
    assert_ne!(k1, cache_key(&other, "Hello world"));
    let mut rec = f.cfg.clone(); rec.rhubarb_recognizer = "pocketSphinx".into();
    assert_ne!(k1, cache_key(&rec, "Hello world"));
}

#[test]
fn wav_duration_reads_header() {
    let p = std::env::temp_dir().join(format!("dur-{}.wav", std::process::id()));
    silent_wav(&p, 1.5);
    assert!((wav_duration(&p).unwrap() - 1.5).abs() < 1e-3);
    std::fs::write(&p, b"junk").unwrap();
    assert!(wav_duration(&p).is_err());
}

#[test]
fn second_identical_request_hits_cache_without_tts_or_rhubarb() {
    let f = fake("cache", true);
    let b1 = prepare(&f.cfg, "Hello  from the presence layer.", &|| false).unwrap();
    assert!(!b1.cache_hit);
    assert_eq!((calls(&f, "piper"), calls(&f, "rhubarb")), (1, 1));
    for file in ["speech.wav", "rhubarb.json", "faceanim.json", "speech.json"] { assert!(b1.dir.join(file).is_file(), "{file}"); }
    assert!((b1.face.duration - 1.0).abs() < 1e-3);
    let b2 = prepare(&f.cfg, "Hello from the presence layer.", &|| false).unwrap(); // whitespace-normalized
    assert!(b2.cache_hit);
    assert_eq!((calls(&f, "piper"), calls(&f, "rhubarb")), (1, 1));
    let _ = prepare(&f.cfg, "Different text.", &|| false).unwrap();
    assert_eq!(calls(&f, "piper"), 2);
}

#[test]
fn missing_voice_or_binary_is_a_clear_error_and_leaves_no_partial_cache() {
    let f = fake("nomodel", false);
    assert!(prepare(&f.cfg, "hi", &|| false).unwrap_err().contains("PRESENCE_PIPER_MODEL"));
    let mut g = fake("nobin", true);
    g.cfg.piper_bin = g.dir.join("does-not-exist");
    assert!(prepare(&g.cfg, "hi", &|| false).unwrap_err().contains("cannot start"));
    let leftovers: Vec<_> = std::fs::read_dir(&g.cfg.cache_dir).map(|d| d.flatten().collect()).unwrap_or_default();
    assert!(leftovers.is_empty(), "partial bundle left behind");
}

#[test]
fn cancellation_kills_the_subprocess() {
    let mut f = fake("cancel", true);
    script(&f.dir.join("piper"), "sleep 30");
    f.cfg.piper_bin = f.dir.join("piper");
    let t = Instant::now();
    let flag = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| { std::thread::sleep(Duration::from_millis(150)); flag.store(true, Ordering::SeqCst); });
        let e = prepare(&f.cfg, "hello", &|| flag.load(Ordering::SeqCst)).unwrap_err();
        assert_eq!(e, "cancelled");
    });
    assert!(t.elapsed() < Duration::from_secs(3));
}

fn poll_until<F: Fn(&SpeechPoll) -> bool>(sp: &mut Speaker, names: &[String], limit: Duration, ok: F) -> Option<SpeechPoll> {
    let t = Instant::now();
    while t.elapsed() < limit {
        let p = sp.poll(names);
        if ok(&p) { return Some(p); }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn speaker_plays_syncs_face_and_interrupt_stops_audio_and_face_together() {
    let f = fake("speaker", true);
    let names = morph_names();
    let mut sp = Speaker::new(f.cfg.clone());
    sp.speak("Hello from the presence layer.").unwrap();
    // face becomes active once the bundle is ready
    let first = poll_until(&mut sp, &names, Duration::from_secs(5), |p| matches!(p, SpeechPoll::Active(_))).expect("never became active");
    assert!(sp.audio_running(), "player should be running");
    let t0 = Instant::now();
    while calls(&f, "player") == 0 && t0.elapsed() < Duration::from_secs(2) { std::thread::sleep(Duration::from_millis(10)); }
    assert_eq!(calls(&f, "player"), 1);
    // over the next ~0.8 s the jaw must both open (D cue) and close (A cue); visemes map by name
    let (mut max_jaw, mut saw_aa, mut saw_closed_pp) = (0.0f32, false, false);
    let mut frame = match first { SpeechPoll::Active(fr) => fr, _ => unreachable!() };
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(700) {
        if let SpeechPoll::Active(fr) = sp.poll(&names) { frame = fr; }
        max_jaw = max_jaw.max(frame.jaw_open);
        saw_aa |= frame.morphs[1] > 0.9;
        saw_closed_pp |= frame.jaw_open < 0.02 && frame.morphs[2] > 0.9;
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(max_jaw > 0.8 && saw_aa && saw_closed_pp, "jaw {max_jaw} aa {saw_aa} pp {saw_closed_pp}");
    // interrupt
    sp.stop();
    assert!(!sp.audio_running(), "audio must stop with the face");
    assert!(matches!(sp.poll(&names), SpeechPoll::Finished));
    assert!(matches!(sp.poll(&names), SpeechPoll::Idle));
}

#[test]
fn new_utterance_supersedes_old_and_stop_cancels_pending_synthesis() {
    let f = fake("supersede", true);
    let names = morph_names();
    let mut sp = Speaker::new(f.cfg.clone());
    sp.speak("first").unwrap();
    sp.speak("second").unwrap();
    poll_until(&mut sp, &names, Duration::from_secs(5), |p| matches!(p, SpeechPoll::Active(_))).expect("active");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(calls(&f, "player"), 1, "only the latest utterance may play");
    sp.stop();
    sp.speak("third").unwrap();
    sp.stop(); // cancel before it can start
    std::thread::sleep(Duration::from_millis(400));
    for _ in 0..10 { let _ = sp.poll(&names); }
    assert!(!sp.audio_running());
    assert_eq!(calls(&f, "player"), 1);
}

#[test]
fn silent_animation_when_no_player_and_jaw_only_without_viseme_targets() {
    let mut f = fake("noplayer", true);
    f.cfg.player = Some(vec!["/nonexistent/player".into()]);
    let mut sp = Speaker::new(f.cfg.clone());
    let names = vec!["body0".to_string()]; // no viseme_* targets
    sp.speak("hi").unwrap();
    let p = poll_until(&mut sp, &names, Duration::from_secs(5), |p| matches!(p, SpeechPoll::Active(_))).expect("active");
    assert!(sp.last_error.as_deref().unwrap_or("").contains("animating silently"));
    if let SpeechPoll::Active(fr) = p { assert_eq!(fr.morphs.len(), 1); }
}

#[test]
fn real_piper_and_rhubarb_end_to_end_when_installed() {
    // Opt-in: PRESENCE_E2E_MODEL=/path/voice.onnx [PRESENCE_RHUBARB_BIN=...]
    let Ok(model) = std::env::var("PRESENCE_E2E_MODEL") else { eprintln!("skipped: PRESENCE_E2E_MODEL not set"); return; };
    let mut cfg = SpeechConfig::from_env();
    cfg.piper_model = Some(model.into());
    cfg.cache_dir = std::env::temp_dir().join(format!("speech-e2e-{}", std::process::id()));
    cfg.player = Some(vec!["true".into()]);
    let b = prepare(&cfg, "Hello from the presence layer.", &|| false).unwrap();
    assert!(b.face.duration > 0.8 && b.face.cues.len() > 5, "{} cues", b.face.cues.len());
    assert!(b.face.cues.iter().any(|c| c.shape == "A") && b.face.cues.iter().any(|c| c.shape != "X"));
    let again = prepare(&cfg, "Hello from the presence layer.", &|| false).unwrap();
    assert!(again.cache_hit);
}

#[test]
fn avatar_speak_and_rest_commands_route_to_speaker() {
    use crate::protocol::AvatarCommand;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
    let f = fake("avatar", true);
    let mut a = crate::avatar::AvatarActor::load(&manifest).unwrap();
    a.speech = Speaker::new(f.cfg.clone());
    assert!(a.mesh.morph_names.iter().any(|n| n == "viseme_aa"), "GLB lacks viseme_aa: {:?}", a.mesh.morph_names);
    a.apply_command(AvatarCommand::Speak { presence_id: None, text: "Hello from the presence layer.".into() });
    let t = Instant::now();
    let mut max_jaw = 0.0f32; let mut max_viseme = 0.0f32;
    while t.elapsed() < Duration::from_millis(1500) {
        a.advance(0.02);
        max_jaw = max_jaw.max(a.face.jaw_open);
        let aa = a.mesh.morph_names.iter().position(|n| n == "viseme_aa").unwrap();
        max_viseme = max_viseme.max(a.face.morphs.get(aa).copied().unwrap_or(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(max_jaw > 0.8 && max_viseme > 0.9, "jaw {max_jaw} viseme {max_viseme}");
    assert!(!a.demo_motion);
    a.evaluate(); // face deforms the real mesh without NaNs
    assert!(a.posed.iter().all(|s| s.position.iter().all(|v| v.is_finite())));
    a.apply_command(AvatarCommand::Rest);
    a.advance(0.02);
    assert!(!a.speech.audio_running());
    assert_eq!(a.face.jaw_open, 0.0);
    assert!(a.face.morphs.iter().all(|w| *w == 0.0));
}

#[test]
fn socket_json_for_speak_and_stop_parses() {
    use crate::protocol::Proposal;
    let p: Proposal = serde_json::from_str(r#"{"kind":"avatarSpeak","proposalId":"s","text":"Hello"}"#).unwrap();
    assert!(matches!(p, Proposal::AvatarSpeak { ref text, .. } if text == "Hello"));
    let p: Proposal = serde_json::from_str(r#"{"kind":"avatarStop","proposalId":"x"}"#).unwrap();
    assert_eq!(p.proposal_id(), "x");
}

#[test]
fn presence_scoped_bind_speak_stop_unbind() {
    use crate::protocol::AvatarCommand as C;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
    let f = fake("bind", true);
    let mut a = crate::avatar::AvatarActor::load(&manifest).unwrap();
    a.speech = Speaker::new(f.cfg.clone());
    a.autoload = false; a.visible = false; a.demo_motion = false;
    // not bound: presence-scoped speech is ignored
    a.apply_command(C::Speak { presence_id: Some("p-a".into()), text: "hi".into() });
    assert!(!a.audio_playing_for_test());
    a.apply_command(C::Bind { presence_id: "p-a".into() });
    assert!(a.visible && a.bound_presence.as_deref() == Some("p-a"));
    // another presence cannot drive this body
    a.apply_command(C::Speak { presence_id: Some("p-b".into()), text: "intruder".into() });
    a.apply_command(C::Speak { presence_id: Some("p-a".into()), text: "Hello from the presence layer.".into() });
    let t = Instant::now();
    while !a.audio_playing_for_test() && t.elapsed() < Duration::from_secs(5) { a.advance(0.02); std::thread::sleep(Duration::from_millis(20)); }
    assert!(a.audio_playing_for_test());
    // stop for the wrong presence does nothing; right one stops audio + face
    a.apply_command(C::StopSpeech { presence_id: Some("p-b".into()) });
    assert!(a.audio_playing_for_test());
    a.apply_command(C::StopSpeech { presence_id: Some("p-a".into()) });
    assert!(!a.audio_playing_for_test());
    assert_eq!(a.face.jaw_open, 0.0);
    // speak again, then retire: speech stops and the body hides (non-autoload)
    a.apply_command(C::Speak { presence_id: Some("p-a".into()), text: "again".into() });
    let t = Instant::now();
    while !a.audio_playing_for_test() && t.elapsed() < Duration::from_secs(5) { a.advance(0.02); std::thread::sleep(Duration::from_millis(20)); }
    a.apply_command(C::Unbind { presence_id: "p-b".into() });
    assert!(a.visible, "unbind for a non-owner must not hide the body");
    a.apply_command(C::Unbind { presence_id: "p-a".into() });
    assert!(!a.visible && a.bound_presence.is_none() && !a.audio_playing_for_test());
    assert_eq!(a.face.jaw_open, 0.0);
}

#[test]
fn autoload_keeps_body_visible_after_unbind() {
    use crate::protocol::AvatarCommand as C;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
    let mut a = crate::avatar::AvatarActor::load(&manifest).unwrap();
    a.autoload = true; a.visible = true; a.demo_motion = true;
    a.apply_command(C::Bind { presence_id: "p".into() });
    assert!(!a.demo_motion);
    a.apply_command(C::Unbind { presence_id: "p".into() });
    assert!(a.visible && a.demo_motion);
}
