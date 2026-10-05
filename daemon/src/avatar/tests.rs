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
    a.demo_motion = false;
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

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>().sqrt()
}

/// This GLB has no animation clips. Body motion is the root; the face is the jaw.
#[test]
fn rest_pose_is_a_standing_body() {
    let a = rest_actor();
    assert!(finite_all(&a));
    assert_eq!(a.stats.nonfinite_repaired, 0);
    assert!(a.rig.clips.is_empty(), "this mesh has no clips; the demo must not invent them");
    assert!(a.jaw_joint.is_some(), "jaw joint missing");
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for s in &a.posed {
        for c in 0..3 {
            lo[c] = lo[c].min(s.position[c]);
            hi[c] = hi[c].max(s.position[c]);
        }
    }
    let height = hi[1] - lo[1];
    assert!(height > 1.4 && height < 2.0, "body height {height}");
}

#[test]
fn root_yaw_swings_the_body() {
    let rest = rest_actor();
    let mut a = rest_actor();
    a.root.yaw = std::f32::consts::FRAC_PI_2;
    a.evaluate();
    let i = rest
        .posed
        .iter()
        .enumerate()
        .max_by(|(_, s), (_, t)| s.position[0].total_cmp(&t.position[0]))
        .unwrap()
        .0;
    let before = rest.posed[i].position;
    let after = a.posed[i].position;
    assert!(before[0] > 0.15, "expected a +X extremity, got {before:?}");
    // Ry(+90°): (x, y, z) -> (z, y, -x). The whole body follows the root.
    assert!((after[0] - before[2]).abs() < 1e-3, "yaw x: {before:?} -> {after:?}");
    assert!((after[1] - before[1]).abs() < 1e-3, "yaw y: {before:?} -> {after:?}");
    assert!((after[2] + before[0]).abs() < 1e-3, "yaw z: {before:?} -> {after:?}");
}

#[test]
fn jaw_opens_chin_without_moving_forehead() {
    let rest = rest_actor();
    let mut a = rest_actor();
    a.set_face(FaceFrame { jaw_open: 1.0, morphs: vec![] });
    a.evaluate();
    let hi_y = rest.posed.iter().map(|s| s.position[1]).fold(f32::MIN, f32::max);
    let brow_y = hi_y - 0.04;
    let head_y = hi_y - 0.25;
    let mut brow_move = 0.0f32;
    let mut brow_n = 0.0f32;
    let mut chin_drop = 0.0f32;
    let mut chin_n = 0.0f32;
    for (i, r) in rest.posed.iter().enumerate() {
        let moved = dist3(r.position, a.posed[i].position);
        if r.position[1] > brow_y {
            brow_move += moved;
            brow_n += 1.0;
        }
        // Lower face, in front of the head, and actually displaced by the hinge.
        if r.position[1] > head_y && r.position[1] < brow_y - 0.05 && r.position[2] > 0.02 && moved > 0.002 {
            chin_drop += r.position[1] - a.posed[i].position[1];
            chin_n += 1.0;
        }
    }
    assert!(brow_n > 10.0, "no forehead splats above {brow_y}");
    let br = brow_move / brow_n;
    assert!(br < 0.004, "forehead moved {br} m");
    assert!(chin_n > 10.0, "jaw axis did not move a lower-face band (moved splats {chin_n})");
    let dy = chin_drop / chin_n;
    assert!(dy > 0.004, "chin dropped only {dy} m across {chin_n} splats");
}

/// Same rigid transform the projection shader applies (`transform_point_dq`).
fn camera_space(p: [f32; 3]) -> [f32; 3] {
    let dq = super::body_camera_dq();
    let q = [dq.real[1], dq.real[2], dq.real[3]];
    let w = dq.real[0];
    let dual = [dq.dual[1], dq.dual[2], dq.dual[3]];
    let dw = dq.dual[0];
    let cross_dq = [
        dual[1] * q[2] - dual[2] * q[1],
        dual[2] * q[0] - dual[0] * q[2],
        dual[0] * q[1] - dual[1] * q[0],
    ];
    let t = [
        2.0 * (dual[0] * w - dw * q[0] - cross_dq[0]),
        2.0 * (dual[1] * w - dw * q[1] - cross_dq[1]),
        2.0 * (dual[2] * w - dw * q[2] - cross_dq[2]),
    ];
    let tmp = [
        q[1] * p[2] - q[2] * p[1] + w * p[0],
        q[2] * p[0] - q[0] * p[2] + w * p[1],
        q[0] * p[1] - q[1] * p[0] + w * p[2],
    ];
    let c2 = [
        q[1] * tmp[2] - q[2] * tmp[1],
        q[2] * tmp[0] - q[0] * tmp[2],
        q[0] * tmp[1] - q[1] * tmp[0],
    ];
    [p[0] + 2.0 * c2[0] + t[0], p[1] + 2.0 * c2[1] + t[1], p[2] + 2.0 * c2[2] + t[2]]
}

#[test]
fn layer_camera_frames_the_body_upright() {
    let chest = camera_space([0.0, 0.90, 0.0]);
    assert!(chest[0].abs() < 1e-3 && chest[1].abs() < 1e-3, "chest not centered: {chest:?}");
    assert!((chest[2] - 2.55).abs() < 1e-3, "chest depth: {chest:?}");
    let nose = camera_space([0.0, 0.90, 0.08]);
    assert!(nose[2] < chest[2] - 0.05, "face (+Z) should be nearer the camera");
    let focal = 220.0_f32 * 1.2;
    let project = |p: [f32; 3]| {
        let c = camera_space(p);
        [focal * c[0] / c[2] + 110.0, -focal * c[1] / c[2] + 110.0]
    };
    let head = project([0.0, 1.62, 0.0]);
    let mid = project([0.0, 0.90, 0.0]);
    let feet = project([0.0, 0.05, 0.0]);
    for (name, s) in [("head", head), ("chest", mid), ("feet", feet)] {
        assert!(s[0] > 4.0 && s[0] < 216.0 && s[1] > 4.0 && s[1] < 216.0, "{name} off the 220 disc: {s:?}");
    }
    assert!(head[1] < mid[1] && mid[1] < feet[1], "head {head:?} chest {mid:?} feet {feet:?} should stack top to bottom");
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
    a.demo_motion = false;
    a.walk_to(-1.0, -1.0);
    for _ in 0..120 { a.advance(1.0 / 30.0); }
    let d = (a.root.pos[0] + 1.0).hypot(a.root.pos[2] + 1.0);
    assert!(d < 0.1, "did not arrive: {d}");
}
