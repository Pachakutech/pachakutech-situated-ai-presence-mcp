//! Skeleton, clips and forward kinematics read from a skinned glTF.
use super::glb::Glb;
use super::math::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Path { Translation, Rotation }

pub struct Channel {
    pub joint: usize,
    pub path: Path,
    pub step: bool,
    pub times: Vec<f32>,
    pub values: Vec<f32>, // 3 or 4 per key
}

pub struct Clip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<Channel>,
}

pub struct Rig {
    pub names: Vec<String>,
    pub parent: Vec<Option<usize>>,
    pub rest_t: Vec<[f32; 3]>,
    pub rest_r: Vec<Quat>,
    pub ibm: Vec<Mat4>,
    /// Joint indices ordered so parents precede children.
    pub order: Vec<usize>,
    pub clips: Vec<Clip>,
}

#[derive(Clone)]
pub struct Pose {
    pub t: Vec<[f32; 3]>,
    pub r: Vec<Quat>,
}

impl Rig {
    pub fn from_glb(g: &Glb) -> Result<Rig, String> {
        let skin = g.json["skins"].get(0).ok_or("GLB has no skin (not rigged)")?;
        let joint_nodes: Vec<usize> = skin["joints"].as_array().ok_or("skin.joints")?.iter().map(|v| v.as_u64().unwrap_or(0) as usize).collect();
        let n = joint_nodes.len();
        let nodes = &g.json["nodes"];
        let node_to_joint = |node: usize| joint_nodes.iter().position(|&j| j == node);
        let mut names = vec![];
        let mut parent = vec![None; n];
        let (mut rest_t, mut rest_r) = (vec![], vec![]);
        for (ji, &nd) in joint_nodes.iter().enumerate() {
            let node = &nodes[nd];
            names.push(node["name"].as_str().unwrap_or("").to_string());
            let t = node["translation"].as_array().map(|a| [a[0].as_f64().unwrap_or(0.) as f32, a[1].as_f64().unwrap_or(0.) as f32, a[2].as_f64().unwrap_or(0.) as f32]).unwrap_or([0.; 3]);
            let r = node["rotation"].as_array().map(|a| quat_normalize([a[0].as_f64().unwrap_or(0.) as f32, a[1].as_f64().unwrap_or(0.) as f32, a[2].as_f64().unwrap_or(0.) as f32, a[3].as_f64().unwrap_or(1.) as f32])).unwrap_or(QUAT_IDENTITY);
            rest_t.push(t);
            rest_r.push(r);
            if let Some(ch) = node["children"].as_array() {
                for c in ch {
                    if let Some(cj) = node_to_joint(c.as_u64().unwrap_or(u64::MAX) as usize) { parent[cj] = Some(ji); }
                }
            }
        }
        let ibm_acc = g.accessor(skin["inverseBindMatrices"].as_u64().ok_or("skin has no inverseBindMatrices")? as usize)?;
        if ibm_acc.ncomp != 16 || ibm_acc.count != n { return Err("inverseBindMatrices size mismatch".into()); }
        let ibm: Vec<Mat4> = ibm_acc.data.chunks_exact(16).map(|c| { let mut m = [0.; 16]; m.copy_from_slice(c); m }).collect();
        // parents-first order
        let mut order = vec![]; let mut done = vec![false; n];
        while order.len() < n {
            let before = order.len();
            for j in 0..n {
                if !done[j] && parent[j].map_or(true, |p| done[p]) { done[j] = true; order.push(j); }
            }
            if order.len() == before { return Err("skeleton has a cycle".into()); }
        }
        let mut clips = vec![];
        for a in g.json["animations"].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
            let mut channels = vec![]; let mut duration = 0.0f32;
            for c in a["channels"].as_array().ok_or("animation.channels")? {
                let Some(joint) = node_to_joint(c["target"]["node"].as_u64().unwrap_or(u64::MAX) as usize) else { continue };
                let path = match c["target"]["path"].as_str() { Some("translation") => Path::Translation, Some("rotation") => Path::Rotation, _ => continue };
                let s = &a["samplers"][c["sampler"].as_u64().ok_or("sampler")? as usize];
                let interp = s["interpolation"].as_str().unwrap_or("LINEAR");
                if interp == "CUBICSPLINE" { return Err("CUBICSPLINE animation not supported".into()); }
                let times = g.accessor(s["input"].as_u64().ok_or("input")? as usize)?.data;
                let values = g.accessor(s["output"].as_u64().ok_or("output")? as usize)?.data;
                if let Some(t) = times.last() { duration = duration.max(*t); }
                channels.push(Channel { joint, path, step: interp == "STEP", times, values });
            }
            clips.push(Clip { name: a["name"].as_str().unwrap_or("").to_string(), duration, channels });
        }
        Ok(Rig { names, parent, rest_t, rest_r, ibm, order, clips })
    }

    pub fn joint(&self, name: &str) -> Option<usize> { self.names.iter().position(|n| n == name) }
    pub fn clip(&self, name: &str) -> Option<&Clip> { self.clips.iter().find(|c| c.name == name) }

    pub fn rest_pose(&self) -> Pose { Pose { t: self.rest_t.clone(), r: self.rest_r.clone() } }

    /// Sample `clip` at time `t` (seconds) over the rest pose. `looping` wraps time.
    pub fn sample(&self, clip: &Clip, t: f32, looping: bool) -> Pose {
        let mut p = self.rest_pose();
        let tt = if looping && clip.duration > 0. { t.rem_euclid(clip.duration) } else { t.clamp(0., clip.duration) };
        for ch in &clip.channels {
            if ch.times.is_empty() { continue; }
            let nk = ch.times.len();
            let k = ch.times.iter().rposition(|&x| x <= tt).unwrap_or(0);
            let k1 = (k + 1).min(nk - 1);
            let f = if k1 == k || ch.step { 0. } else { ((tt - ch.times[k]) / (ch.times[k1] - ch.times[k]).max(1e-9)).clamp(0., 1.) };
            match ch.path {
                Path::Translation => {
                    let g = |i: usize, c: usize| ch.values[i * 3 + c];
                    p.t[ch.joint] = [0, 1, 2].map(|c| g(k, c) + (g(k1, c) - g(k, c)) * f);
                }
                Path::Rotation => {
                    let g = |i: usize| [ch.values[i * 4], ch.values[i * 4 + 1], ch.values[i * 4 + 2], ch.values[i * 4 + 3]];
                    p.r[ch.joint] = quat_slerp(g(k), g(k1), f);
                }
            }
        }
        p
    }

    /// World matrices (with `root_xf` applied above all roots) -> skin matrices (world * IBM).
    pub fn skin_matrices(&self, pose: &Pose, root_xf: &Mat4) -> Vec<Mat4> {
        let n = self.names.len();
        let mut world = vec![MAT4_IDENTITY; n];
        let mut out = vec![MAT4_IDENTITY; n];
        for &j in &self.order {
            let local = mat4_from_trs(pose.t[j], pose.r[j]);
            world[j] = match self.parent[j] { Some(p) => mat4_mul(&world[p], &local), None => mat4_mul(root_xf, &local) };
            out[j] = mat4_mul(&world[j], &self.ibm[j]);
        }
        out
    }
}
