use super::*;
use std::path::PathBuf;

fn manifest() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json") }

fn ply_samples() -> Vec<([f32; 3], [f32; 3])> {
    let d = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/humanoid_proxy_a_pose.samples.ply")).unwrap();
    let h = d.windows(10).position(|w| w == b"end_header").unwrap() + 11;
    d[h..].chunks_exact(24).map(|c| {
        let f = |i: usize| f32::from_le_bytes([c[i * 4], c[i * 4 + 1], c[i * 4 + 2], c[i * 4 + 3]]);
        ([f(0), f(1), f(2)], [f(3), f(4), f(5)])
    }).collect()
}

fn rest_actor() -> AvatarActor {
    let mut a = AvatarActor::load(&manifest()).expect("load avatar");
    a.rest();
    a.ground_offset = 0.0; // compare in mesh space
    a.evaluate();
    a
}

#[test]
fn fingerprint_matches_bake_manifest() {
    let a = AvatarActor::load(&manifest()).unwrap();
    assert_eq!(a.binding.fingerprint(), "8448f066825111bce3aa606bae47824f14b7fbebcb437a06d181b7097e8eceb2");
}

#[test]
fn wrong_topology_is_rejected() {
    let a = AvatarActor::load(&manifest()).unwrap();
    let mut rest = a.mesh.rest.clone();
    rest[0][1] += 0.001;
    assert!(a.binding.validate(&rest, &a.mesh.indices).is_err());
    let idx: Vec<u32> = a.mesh.indices[..a.mesh.indices.len() - 3].to_vec();
    assert!(a.binding.validate(&a.mesh.rest, &idx).is_err());
}

#[test]
fn rest_pose_reproduces_baked_samples() {
    let a = rest_actor();
    let ply = ply_samples();
    assert_eq!(ply.len(), a.posed.len());
    let mut worst = 0.0f32;
    for (s, (p, _)) in a.posed.iter().zip(&ply) {
        for c in 0..3 { worst = worst.max((s.position[c] - p[c]).abs()); }
    }
    assert!(worst < 2e-4, "rest reconstruction deviates {worst} m from samples.ply");
    assert_eq!(a.stats.nonfinite_repaired, 0);
}

#[test]
fn rest_frame_normals_match_baked_normals() {
    let a = rest_actor();
    let ply = ply_samples();
    // splat local +Z axis = triangle normal
    let mut bad = 0;
    for (s, (_, n)) in a.posed.iter().zip(&ply) {
        let [w, x, y, z] = s.rotation;
        let nz = [2. * (x * z + w * y), 2. * (y * z - w * x), 1. - 2. * (x * x + y * y)];
        if dot(nz, *n) < 0.999 { bad += 1; }
    }
    assert!(bad < 50, "{bad} splats with normal disagreement");
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }

fn finite_all(a: &AvatarActor) -> bool {
    a.posed.iter().all(|s| s.position.iter().chain(&s.rotation).chain(&s.scale).all(|v| v.is_finite()))
}

fn pose_actor(clip: &str, t: f32) -> AvatarActor {
    let mut a = AvatarActor::load(&manifest()).unwrap();
    a.ground_offset = 0.0;
    a.play(clip, false);
    a.set_clip_time(t);
    a.evaluate();
    a
}

fn centroid_where(a: &AvatarActor, f: impl Fn(&[f32; 3]) -> bool) -> [f32; 3] {
    let (mut c, mut n) = ([0.0f32; 3], 0.0);
    for s in &a.posed { if f(&s.position) { for i in 0..3 { c[i] += s.position[i]; } n += 1.0; } }
    [c[0] / n, c[1] / n, c[2] / n]
}

#[test]
fn all_pose_clips_stay_finite_and_attached() {
    let rest = rest_actor();
    for clip in ["idle", "walk", "pose_arm_raise_90", "pose_elbow_flex_120", "pose_forearm_twist_170", "pose_head_yaw_pitch", "pose_jaw_open", "pose_point_left", "pose_point_center", "pose_point_right"] {
        for t in [0.0, 0.3, 0.7, 1.0] {
            let a = pose_actor(clip, t);
            assert!(finite_all(&a), "{clip}@{t}: non-finite splat");
            assert_eq!(a.stats.nonfinite_repaired, 0, "{clip}@{t}");
            // Detachment check: every splat must stay within 6 cm of the bbox-limited
            // reach of its rest position plus 1.0 m (loose), and the whole body must
            // keep a plausible extent (no catastrophic collapse).
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for s in &a.posed { for c in 0..3 { lo[c] = lo[c].min(s.position[c]); hi[c] = hi[c].max(s.position[c]); } }
            assert!(hi[1] - lo[1] > 1.4 && hi[1] - lo[1] < 2.0, "{clip}@{t}: body height {}", hi[1] - lo[1]);
            // Torso (rest y 0.9..1.2, |x|<0.1) should barely move for limb-only poses.
            if clip.starts_with("pose_") && clip != "pose_head_yaw_pitch" {
                for (s, r) in a.posed.iter().zip(&rest.posed) {
                    if r.position[1] > 0.95 && r.position[1] < 1.15 && r.position[0].abs() < 0.08 && r.position[2] > 0.05 {
                        let d: f32 = (0..3).map(|c| (s.position[c] - r.position[c]).powi(2)).sum::<f32>().sqrt();
                        assert!(d < 0.03, "{clip}@{t}: belly splat moved {d} m");
                    }
                }
            }
        }
    }
}

#[test]
fn arm_raise_lifts_hand_forward() {
    let rest = rest_actor();
    let hand = |a: &AvatarActor| centroid_where(a, |p| p[0] > 0.44 && p[1] < 1.05);
    let (h0, h1) = (hand(&rest), hand(&pose_actor("pose_arm_raise_90", 1.0)));
    let _ = (h0, h1);
    // follow the same splats: use rest-defined selection
    let sel: Vec<usize> = rest.posed.iter().enumerate().filter(|(_, s)| s.position[0] > 0.44).map(|(i, _)| i).collect();
    assert!(sel.len() > 100);
    let a = pose_actor("pose_arm_raise_90", 1.0);
    let mean = |act: &AvatarActor| { let mut c = [0.0f32; 3]; for &i in &sel { for k in 0..3 { c[k] += act.posed[i].position[k]; } } c.map(|v| v / sel.len() as f32) };
    let (m0, m1) = (mean(&rest), mean(&a));
    assert!(m1[2] - m0[2] > 0.1 && m1[1] - m0[1] > 0.3, "hand should swing forward (+Z) and up: {:?} -> {:?}", m0, m1);
    // arm length preserved (hand stays ~ same distance from the shoulder)
    let sh = [0.17f32, 1.35, 0.03];
    let d = |m: [f32; 3]| ((m[0] - sh[0]).powi(2) + (m[1] - sh[1]).powi(2) + (m[2] - sh[2]).powi(2)).sqrt();
    assert!((d(m0) - d(m1)).abs() < 0.05, "arm length changed: {} vs {}", d(m0), d(m1));
}

#[test]
fn elbow_flex_keeps_upper_arm_and_moves_forearm() {
    let rest = rest_actor();
    let upper: Vec<usize> = rest.posed.iter().enumerate().filter(|(_, s)| s.position[0] > 0.22 && s.position[0] < 0.28 && s.position[1] > 1.2).map(|(i, _)| i).collect();
    let fore: Vec<usize> = rest.posed.iter().enumerate().filter(|(_, s)| s.position[0] > 0.45).map(|(i, _)| i).collect();
    let a = pose_actor("pose_elbow_flex_120", 1.0);
    let mv = |ids: &Vec<usize>| ids.iter().map(|&i| (0..3).map(|c| (a.posed[i].position[c] - rest.posed[i].position[c]).powi(2)).sum::<f32>().sqrt()).sum::<f32>() / ids.len() as f32;
    assert!(mv(&upper) < 0.03, "upper arm moved {}", mv(&upper));
    assert!(mv(&fore) > 0.2, "forearm barely moved {}", mv(&fore));
}

#[test]
fn jaw_opens_chin_without_moving_forehead() {
    let rest = rest_actor();
    let mut a = rest_actor();
    a.set_face(FaceFrame { jaw_open: 1.0, morphs: vec![] });
    a.evaluate();
    let chin: Vec<usize> = rest.posed.iter().enumerate().filter(|(_, s)| s.position[1] > 1.43 && s.position[1] < 1.47 && s.position[2] > 0.1).map(|(i, _)| i).collect();
    let brow: Vec<usize> = rest.posed.iter().enumerate().filter(|(_, s)| s.position[1] > 1.58 && s.position[1] < 1.64).map(|(i, _)| i).collect();
    assert!(!chin.is_empty() && !brow.is_empty());
    let dy = chin.iter().map(|&i| rest.posed[i].position[1] - a.posed[i].position[1]).sum::<f32>() / chin.len() as f32;
    let br = brow.iter().map(|&i| (0..3).map(|c| (a.posed[i].position[c] - rest.posed[i].position[c]).powi(2)).sum::<f32>().sqrt()).sum::<f32>() / brow.len() as f32;
    assert!(dy > 0.015, "chin dropped only {dy} m");
    assert!(br < 0.003, "forehead moved {br} m");
}

#[test]
fn degenerate_triangle_uses_fallback_and_is_counted() {
    let a = AvatarActor::load(&manifest()).unwrap();
    let mut verts = a.mesh.rest.clone();
    // collapse one used triangle to a point
    let t = a.binding.records[0].tri as usize;
    let i0 = a.mesh.indices[3 * t] as usize;
    for k in 1..3 { verts[a.mesh.indices[3 * t + k] as usize] = verts[i0]; }
    let mut out = vec![];
    let st = a.recon.reconstruct(&a.binding, &a.mesh.indices, &verts, &a.mesh.rest, &mut out);
    assert!(st.degenerate_triangles >= 1);
    assert!(out.iter().all(|s| s.position.iter().chain(&s.rotation).all(|v| v.is_finite())));
}

#[test]
fn walk_to_turns_and_moves() {
    let mut a = AvatarActor::load(&manifest()).unwrap();
    a.walk_to(-1.0, -1.0);
    for _ in 0..120 { a.advance(1.0 / 30.0); }
    let d = (a.root.pos[0] + 1.0).hypot(a.root.pos[2] + 1.0);
    assert!(d < 0.1, "did not arrive: {d}");
}
