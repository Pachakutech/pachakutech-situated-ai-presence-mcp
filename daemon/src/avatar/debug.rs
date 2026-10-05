//! `presence-daemon avatar-debug <avatar_manifest.json> <outdir>`: headless
//! dump of the pose-test suite from the skinned-proxy spec (no GPU, no
//! compositor). Writes <outdir>/<name>.splats (u32 count, then per splat 11 f32:
//! pos xyz, rot wxyz, rgba) and prints sanity stats. Render with
//! scripts/render_avatar_debug.py.
use super::{AvatarActor, FaceFrame};
use std::io::Write;
use std::path::Path;

fn dump(a: &AvatarActor, dir: &Path, name: &str) -> Result<(), String> {
    let mut buf = Vec::with_capacity(4 + a.posed.len() * 44);
    buf.extend_from_slice(&(a.posed.len() as u32).to_le_bytes());
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut bad = 0;
    for s in &a.posed {
        for v in s.position.iter().chain(s.rotation.iter()).chain(s.scale.iter()).chain(s.color.iter()) {
            if !v.is_finite() { bad += 1; }
        }
        for c in 0..3 { lo[c] = lo[c].min(s.position[c]); hi[c] = hi[c].max(s.position[c]); }
        for v in s.position.iter().chain(s.rotation.iter()).chain(s.color.iter()) { buf.extend_from_slice(&v.to_le_bytes()); }
    }
    std::fs::File::create(dir.join(format!("{name}.splats"))).and_then(|mut f| f.write_all(&buf)).map_err(|e| e.to_string())?;
    println!("{name:<24} splats={} nonfinite={} degenerate_tris={} repaired={} bbox=({:.2},{:.2},{:.2})..({:.2},{:.2},{:.2})",
        a.posed.len(), bad, a.stats.degenerate_triangles, a.stats.nonfinite_repaired, lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]);
    Ok(())
}

pub fn run(args: &[String]) -> Result<(), String> {
    let manifest = args.get(0).ok_or("usage: avatar-debug <avatar_manifest.json> <outdir>")?;
    let out = Path::new(args.get(1).ok_or("missing outdir")?);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut a = AvatarActor::load(Path::new(manifest))?;
    println!("loaded: {} splats, {} joints, {} clips, fingerprint {}", a.splat_count(), a.rig.names.len(), a.rig.clips.len(), a.binding.fingerprint());

    a.rest(); a.evaluate(); dump(&a, out, "01_rest_apose")?;
    a.play("idle", true); a.set_clip_time(1.0); a.evaluate(); dump(&a, out, "02_idle")?;
    for (i, t) in [0.0f32, 0.25, 0.5, 0.75].iter().enumerate() {
        a.play("walk", true); a.set_clip_time(*t); a.evaluate(); dump(&a, out, &format!("03_walk_{i}"))?;
    }
    for (n, c) in [("04_arm_raise_90", "pose_arm_raise_90"), ("05_elbow_flex_120", "pose_elbow_flex_120"),
                   ("06_forearm_twist_170", "pose_forearm_twist_170"), ("07_head_yaw_pitch", "pose_head_yaw_pitch"),
                   ("10_point_left", "pose_point_left"), ("10_point_center", "pose_point_center"), ("10_point_right", "pose_point_right")] {
        a.play(c, false); a.set_clip_time(1.0); a.evaluate(); dump(&a, out, n)?;
    }
    a.rest(); a.set_face(FaceFrame { jaw_open: 1.0, morphs: vec![] }); a.evaluate(); dump(&a, out, "08_jaw_open")?;
    a.set_face(FaceFrame::default());
    // 9: root turn while walking toward a point behind-left
    a.root.pos = [0., 0., 0.]; a.root.yaw = 0.;
    a.walk_to(-1.5, -1.0);
    for _ in 0..45 { a.advance(1.0 / 30.0); }
    a.evaluate();
    println!("root after walk: pos=({:.2},{:.2}) yaw={:.2} rad", a.root.pos[0], a.root.pos[2], a.root.yaw);
    dump(&a, out, "09_root_turn_walk")?;
    Ok(())
}
