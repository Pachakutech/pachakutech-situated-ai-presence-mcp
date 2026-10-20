use super::*;
use std::path::PathBuf;

fn manifest() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json") }

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
    let sidecar = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/humanoid_proxy_rigged.splatbind.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
    let expected = v["fingerprint_sha256"].as_str().expect("sidecar fingerprint");
    assert_eq!(a.binding.fingerprint(), expected);
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

fn finite_all(a: &AvatarActor) -> bool {
    a.posed.iter().all(|s| s.position.iter().chain(&s.rotation).chain(&s.scale).all(|v| v.is_finite()))
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>().sqrt()
}

fn quat_dot(a: [f32; 4], b: [f32; 4]) -> f32 {
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs()
}

/// The sit is the skeleton rest, with `Breathe` and `Point` in the file.
#[test]
fn rest_pose_is_a_sitting_body() {
    let a = rest_actor();
    assert!(finite_all(&a));
    assert_eq!(a.stats.nonfinite_repaired, 0);
    assert!(a.rig.clip("Breathe").is_some(), "Breathe missing");
    assert!(a.rig.clip("Point").is_some(), "Point missing");
    assert!(a.jaw_joint.is_some(), "jaw joint missing");
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for s in &a.posed {
        for c in 0..3 {
            lo[c] = lo[c].min(s.position[c]);
            hi[c] = hi[c].max(s.position[c]);
        }
    }
    let height = hi[1] - lo[1];
    assert!(height > 0.4 && height < 2.2, "body height {height}");
}

/// Rest faces the camera. Frame 90 raises her left hand, and while she is
/// still facing us that hand is on the viewer's right and nearer. A target
/// on the viewer's right yaws +π/2, so her nose turns that way. The viewer's
/// left is frame 270 and yaw −π/2.
#[test]
fn left_hand_points_to_the_viewers_right() {
    let a = AvatarActor::load(&manifest()).unwrap();
    let point = a.rig.clip("Point").unwrap();
    let dur = point.duration;
    let l = a.rig.joint("wrist.L").unwrap();
    let rgt = a.rig.joint("wrist.R").unwrap();
    let jaw = a.rig.joint("jaw").unwrap();
    let head = a.rig.joint("head").unwrap();
    let focal = 330.0 * 1.2;
    let project = |world: [f32; 3]| {
        let c = camera_space(world);
        (focal * c[0] / c[2] + 165.0, c[2])
    };
    let place = |theta: f32| {
        let t = aim::point_time(theta, dur);
        let pose = a.rig.sample(point, t, false);
        let root = mat4_from_trs(
            [0.0, 0.0, 0.0],
            quat_axis_angle([0.0, 1.0, 0.0], aim::aim_yaw(theta) + aim::FACING_YAW),
        );
        let world = |j: usize| transform_point(&root, a.rig.joint_origin(&pose, j));
        (t, world(l), world(rgt), world(jaw), world(head))
    };
    let (_, _, _, jaw_w, head_w) = place(0.0);
    let (_, jaw_z) = project(jaw_w);
    let (_, head_z) = project(head_w);
    assert!(jaw_z < head_z - 0.01, "rest should face the camera, jaw {jaw_z} head {head_z}");

    // Facing the viewer, before the quarter turn. Rx(π) keeps +X on the viewer's right.
    let facing = |t: f32| {
        let pose = a.rig.sample(point, t, false);
        let root = mat4_from_trs(
            [0.0, 0.0, 0.0],
            quat_axis_angle([0.0, 1.0, 0.0], aim::FACING_YAW),
        );
        let world = |j: usize| transform_point(&root, a.rig.joint_origin(&pose, j));
        (world(l), world(rgt))
    };
    let (lw, rw) = facing(dur * 0.25);
    assert!(lw[1] > rw[1] + 0.05, "frame 90 should raise the left wrist");
    let (lx, lz) = project(lw);
    let (rx, rz) = project(rw);
    assert!(lx > rx + 20.0, "facing us, her left hand sits to the viewer's right: L {lx} R {rx}");
    assert!(lz < rz - 0.1, "facing us, her left hand is nearer: L {lz} R {rz}");
    let (lw, rw) = facing(dur * 0.75);
    assert!(rw[1] > lw[1] + 0.05, "frame 270 should raise the right wrist");
    let (lx, lz) = project(lw);
    let (rx, rz) = project(rw);
    assert!(rx < lx - 20.0, "facing us, her right hand sits to the viewer's left: L {lx} R {rx}");
    assert!(rz < lz - 0.1, "facing us, her right hand is nearer: L {lz} R {rz}");

    let nose_x = |theta: f32| {
        let root = mat4_from_trs(
            [0.0, 0.0, 0.0],
            quat_axis_angle([0.0, 1.0, 0.0], aim::aim_yaw(theta) + aim::FACING_YAW),
        );
        let nose = transform_point(&root, [0.0, 0.90, 0.30]);
        let chest = transform_point(&root, [0.0, 0.90, 0.0]);
        project(nose).0 - project(chest).0
    };
    let (t, lw, rw, _, _) = place(std::f32::consts::FRAC_PI_2);
    let frame = t / dur * 360.0;
    assert!((frame - 90.0).abs() < 0.5, "viewer's right should be frame 90, got {frame}");
    assert!(lw[1] > rw[1] + 0.05, "left wrist should be the raised one");
    assert!(nose_x(std::f32::consts::FRAC_PI_2) > 20.0, "she should turn toward the viewer's right");

    let (t, lw, rw, _, _) = place(-std::f32::consts::FRAC_PI_2);
    let frame = t / dur * 360.0;
    assert!((frame - 270.0).abs() < 0.5, "viewer's left should be frame 270, got {frame}");
    assert!(rw[1] > lw[1] + 0.05, "right wrist should be the raised one");
    assert!(nose_x(-std::f32::consts::FRAC_PI_2) < -20.0, "she should turn toward the viewer's left");
}

#[test]
fn point_compass_frames_place_the_wrists() {
    let a = AvatarActor::load(&manifest()).unwrap();
    let point = a.rig.clip("Point").expect("Point");
    let dur = point.duration;
    let wrist = |t: f32| {
        let pose = a.rig.sample(point, t, false);
        let l = a.rig.joint_origin(&pose, a.rig.joint("wrist.L").unwrap());
        let r = a.rig.joint_origin(&pose, a.rig.joint("wrist.R").unwrap());
        (l, r)
    };
    let (down_l, down_r) = wrist(0.0);
    // Skeleton, before the camera. A quarter of the clip raises wrist.L on +X
    // (her left hand, frames 1–150). Rx(π) keeps that hand on the viewer's right.
    let (quarter_l, quarter_r) = wrist(dur * 0.25);
    let (up_l, up_r) = wrist(dur * 0.5);
    let (three_l, three_r) = wrist(dur * 0.75);
    assert!(quarter_l[1] > quarter_r[1] + 0.05, "quarter: left wrist should be higher {quarter_l:?} {quarter_r:?}");
    assert!(quarter_l[0] > quarter_r[0] + 0.05, "quarter: left wrist should be further on +X");
    assert!(three_r[1] > three_l[1] + 0.05, "three-quarter: right wrist should be higher {three_l:?} {three_r:?}");
    assert!(three_r[0] < three_l[0] - 0.05, "three-quarter: right wrist should be further on -X");
    assert!((up_l[1] - up_r[1]).abs() < 0.05, "above: wrists should match in height {up_l:?} {up_r:?}");
    assert!(up_l[1] > down_l[1] + 0.2 && up_r[1] > down_r[1] + 0.2, "above should clear the low pose");
    let root = a.rig.joint("root").unwrap();
    let q = |t: f32| a.rig.sample(point, t, false).r[root];
    let q0 = q(0.0);
    for t in [dur * 0.25, dur * 0.5, dur * 0.75] {
        assert!(quat_dot(q0, q(t)) > 0.999, "root rotation moved at {t}");
    }
}

#[test]
fn breathe_does_not_gesture_the_upper_arm() {
    let a = AvatarActor::load(&manifest()).unwrap();
    let breathe = a.rig.clip("Breathe").expect("Breathe");
    let joint = a.rig.joint("upperarm01.L").unwrap();
    let q0 = a.rig.sample(breathe, 0.0, false).r[joint];
    for t in [breathe.duration * 0.5, breathe.duration] {
        let q = a.rig.sample(breathe, t, false).r[joint];
        assert!(quat_dot(q0, q) > 0.999, "upperarm01.L moved across Breathe at {t}");
    }
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
        // Lower face, on mesh +Z. The camera's half turn brings that side near.
        // A second half turn on the root would put the chin on −Z.
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
    let side = crate::placement::DISC_PX as f32;
    let focal = side * 1.2;
    let mid_px = side * 0.5;
    let project = |p: [f32; 3]| {
        let c = camera_space(p);
        [focal * c[0] / c[2] + mid_px, focal * c[1] / c[2] + mid_px]
    };
    let head = project([0.0, 1.62, 0.0]);
    let mid = project([0.0, 0.90, 0.0]);
    let feet = project([0.0, 0.05, 0.0]);
    let lo = side * 4.0 / 220.0;
    let hi = side * 216.0 / 220.0;
    for (name, s) in [("head", head), ("chest", mid), ("feet", feet)] {
        assert!(s[0] > lo && s[0] < hi && s[1] > lo && s[1] < hi, "{name} off the disc: {s:?}");
    }
    assert!(head[1] < mid[1] && mid[1] < feet[1], "head {head:?} chest {mid:?} feet {feet:?} should stack top to bottom");
}

/// `cargo test --bin presence-daemon -- --ignored time_the_breath_expand`
#[test]
#[ignore]
fn time_the_breath_expand() {
    let mut a = rest_actor();
    a.demo_motion = false;
    for _ in 0..10 {
        a.advance(1.0 / 30.0);
        a.evaluate();
    }
    let n = 100;
    let started = std::time::Instant::now();
    for _ in 0..n {
        a.advance(1.0 / 30.0);
        a.evaluate();
    }
    let eval_ms = started.elapsed().as_secs_f64() * 1000.0 / n as f64;
    let bytes = a.splat_count() * 96;
    let src = vec![1u8; bytes];
    let mut dst = vec![0u8; bytes];
    let copies = 100;
    let copy_started = std::time::Instant::now();
    for _ in 0..copies {
        dst.copy_from_slice(&src);
    }
    let copy_ms = copy_started.elapsed().as_secs_f64() * 1000.0 / copies as f64;
    eprintln!(
        "evaluate {eval_ms:.3} ms/call over {n}, memcpy {bytes} bytes {copy_ms:.3} ms"
    );
}

/// The old expand built a triangle frame once per splat. This is that loop,
/// kept so the one-frame-per-triangle path has to match it bit for bit.
fn per_splat_rebuild(a: &AvatarActor) -> (Vec<crate::actors::scene_memory::GaussianSplat>, usize) {
    use crate::actors::scene_memory::GaussianSplat;
    let indices = &a.mesh.indices;
    let posed = &a.posed_verts;
    let rest = &a.mesh.rest;
    let mut out = Vec::with_capacity(a.binding.records.len());
    let mut degenerate = std::collections::HashSet::new();
    for r in &a.binding.records {
        let t = r.tri as usize;
        let (i0, i1, i2) = (indices[3 * t] as usize, indices[3 * t + 1] as usize, indices[3 * t + 2] as usize);
        let (v0, v1, v2) = (posed[i0], posed[i1], posed[i2]);
        let b0 = (1.0 - r.b1 - r.b2).max(0.0);
        let mut p = [0.0f32; 3];
        for c in 0..3 {
            p[c] = b0 * v0[c] + r.b1 * v1[c] + r.b2 * v2[c];
        }
        let f = match super::deform::tri_frame(v0, v1, v2) {
            Some(f) => f,
            None => {
                degenerate.insert(t);
                a.recon.rest_frames[t]
            }
        };
        for c in 0..3 {
            p[c] += r.off[0] * f[2][c] + r.off[1] * f[0][c] + r.off[2] * f[1][c];
        }
        let q = super::math::quat_from_columns(f[0], f[1], f[2]);
        let mut rot = [q[3], q[0], q[1], q[2]];
        if !(p.iter().all(|x| x.is_finite()) && rot.iter().all(|x| x.is_finite())) {
            let (aa, bb, cc) = (rest[i0], rest[i1], rest[i2]);
            for c in 0..3 {
                p[c] = b0 * aa[c] + r.b1 * bb[c] + r.b2 * cc[c];
            }
            rot = [1., 0., 0., 0.];
        }
        out.push(GaussianSplat {
            position: p,
            scale: a.recon.scale,
            rotation: rot,
            color: a.recon.colors[t],
        });
    }
    (out, degenerate.len())
}

fn same_bits(got: &[crate::actors::scene_memory::GaussianSplat], expect: &[crate::actors::scene_memory::GaussianSplat]) {
    assert_eq!(got.len(), expect.len());
    for (i, (g, e)) in got.iter().zip(expect).enumerate() {
        for c in 0..3 {
            assert_eq!(g.position[c].to_bits(), e.position[c].to_bits(), "position {i} [{c}]");
        }
        for c in 0..4 {
            assert_eq!(g.rotation[c].to_bits(), e.rotation[c].to_bits(), "rotation {i} [{c}]");
            assert_eq!(g.color[c].to_bits(), e.color[c].to_bits(), "color {i} [{c}]");
        }
    }
}

#[test]
fn one_frame_per_triangle_matches_the_per_splat_rebuild() {
    let mut a = rest_actor();
    let (expect, degen) = per_splat_rebuild(&a);
    same_bits(&a.posed, &expect);
    assert_eq!(a.stats.degenerate_triangles, degen);
    a.demo_motion = false;
    a.advance(1.7);
    a.evaluate();
    let (expect, degen) = per_splat_rebuild(&a);
    same_bits(&a.posed, &expect);
    assert_eq!(a.stats.degenerate_triangles, degen);
    a.set_face(FaceFrame { jaw_open: 1.0, morphs: vec![] });
    a.evaluate();
    let (expect, degen) = per_splat_rebuild(&a);
    same_bits(&a.posed, &expect);
    assert_eq!(a.stats.degenerate_triangles, degen);
}

#[test]
fn degenerate_triangle_uses_fallback_and_is_counted() {
    let a = AvatarActor::load(&manifest()).unwrap();
    let mut verts = a.mesh.rest.clone();
    // collapse one used triangle to a point
    let t = a.binding.records[0].tri as usize;
    let i0 = a.mesh.indices[3 * t] as usize;
    for k in 1..3 { verts[a.mesh.indices[3 * t + k] as usize] = verts[i0]; }
    let mut a = a;
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
