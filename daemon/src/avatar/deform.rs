//! Proxy-mesh deformation (morph -> LBS) and barycentric Gaussian
//! reconstruction with triangle-frame transport. CPU reference implementation;
//! see docs/plans/skinned-proxy-triangle-barycentric-gaussian-avatar-spec.md.
//! Compile- and unit-test-verified only; not run on a GPU.
use super::glb::Glb;
use super::math::*;
use super::splatbind::SplatBinding;
use crate::actors::scene_memory::GaussianSplat;

pub struct ProxyMesh {
    pub rest: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub morph_names: Vec<String>,
    pub morph_deltas: Vec<Vec<[f32; 3]>>,
}

impl ProxyMesh {
    pub fn from_glb(g: &Glb) -> Result<ProxyMesh, String> {
        let prim = &g.json["meshes"][0]["primitives"][0];
        let at = |k: &str| prim["attributes"][k].as_u64().map(|v| v as usize);
        let rest = g.vec3s(at("POSITION").ok_or("no POSITION")?)?;
        let indices = g.indices(prim["indices"].as_u64().ok_or("no indices")? as usize)?;
        let ja = g.accessor(at("JOINTS_0").ok_or("mesh has no JOINTS_0 (not skinned)")?)?;
        let wa = g.accessor(at("WEIGHTS_0").ok_or("mesh has no WEIGHTS_0")?)?;
        if ja.count != rest.len() || wa.count != rest.len() { return Err("skin attribute count mismatch".into()); }
        let joints = ja.data.chunks_exact(4).map(|c| [c[0] as u16, c[1] as u16, c[2] as u16, c[3] as u16]).collect();
        let weights = wa.data.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
        let names: Vec<String> = g.json["meshes"][0]["extras"]["targetNames"].as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
        let mut morph_deltas = vec![];
        for t in prim["targets"].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
            morph_deltas.push(g.vec3s(t["POSITION"].as_u64().ok_or("target without POSITION")? as usize)?);
        }
        Ok(ProxyMesh { rest, indices, joints, weights, morph_names: names, morph_deltas })
    }

    /// Morph (weights default to zero: the splatbind is baked on base POSITION,
    /// so glTF's non-zero default mesh weights are deliberately ignored) then LBS.
    pub fn skin(&self, morph_w: &[f32], mats: &[Mat4], out: &mut Vec<[f32; 3]>) {
        out.clear();
        out.reserve(self.rest.len());
        for (i, r) in self.rest.iter().enumerate() {
            let mut p = *r;
            for (m, w) in morph_w.iter().enumerate() {
                if *w != 0.0 { if let Some(d) = self.morph_deltas.get(m) { for c in 0..3 { p[c] += w * d[i][c]; } } }
            }
            let (j, w) = (&self.joints[i], &self.weights[i]);
            let mut o = [0.0f32; 3];
            let mut ws = 0.0;
            for k in 0..4 {
                if w[k] > 0.0 {
                    let q = transform_point(&mats[j[k] as usize], p);
                    for c in 0..3 { o[c] += w[k] * q[c]; }
                    ws += w[k];
                }
            }
            out.push(if ws > 1e-6 { [o[0] / ws, o[1] / ws, o[2] / ws] } else { p });
        }
    }
}

/// Orthonormal triangle frame, columns (e1, e2, n), per the spec:
/// e1 = norm(v1-v0), n = norm((v1-v0)x(v2-v0)), e2 = norm(n x e1).
pub type Frame = [[f32; 3]; 3];
pub const IDENTITY_FRAME: Frame = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];

pub fn tri_frame(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> Option<Frame> {
    let a = sub(v1, v0);
    let e1 = normalize(a)?;
    let n = normalize(cross(a, sub(v2, v0)))?;
    let e2 = normalize(cross(n, e1))?;
    Some([e1, e2, n])
}

#[derive(Default, Debug, Clone, Copy)]
pub struct DeformStats {
    pub degenerate_triangles: usize,
    pub nonfinite_repaired: usize,
}

pub struct Reconstructor {
    pub rest_frames: Vec<Frame>,
    pub colors: Vec<[f32; 4]>,
    pub scale: [f32; 3],
}

const PALETTE: [[f32; 3]; 10] = [
    [0.90, 0.45, 0.40], [0.40, 0.70, 0.90], [0.55, 0.85, 0.50], [0.95, 0.80, 0.35], [0.75, 0.55, 0.90],
    [0.35, 0.85, 0.80], [0.95, 0.60, 0.80], [0.70, 0.70, 0.70], [0.60, 0.45, 0.30], [0.95, 0.95, 0.60],
];

impl Reconstructor {
    pub fn new(mesh: &ProxyMesh, b: &SplatBinding) -> Reconstructor {
        let tri_count = mesh.indices.len() / 3;
        let v = |i: usize| mesh.rest[mesh.indices[i] as usize];
        let mut rest_frames = Vec::with_capacity(tri_count);
        let mut area = 0.0f64;
        for t in 0..tri_count {
            rest_frames.push(tri_frame(v(3 * t), v(3 * t + 1), v(3 * t + 2)).unwrap_or(IDENTITY_FRAME));
            let c = cross(sub(v(3 * t + 1), v(3 * t)), sub(v(3 * t + 2), v(3 * t)));
            area += 0.5 * (dot(c, c) as f64).sqrt();
        }
        // Splat size from sampling density: spacing = sqrt(area / N). Disc-like Gaussians.
        let spacing = ((area / b.records.len().max(1) as f64).sqrt()) as f32;
        let s = 0.6 * spacing;
        // Debug region colour: dominant joint across the triangle's three vertices.
        let mut colors = Vec::with_capacity(tri_count);
        for t in 0..tri_count {
            let mut acc = std::collections::HashMap::<u16, f32>::new();
            for k in 0..3 {
                let vi = mesh.indices[3 * t + k] as usize;
                for c in 0..4 { *acc.entry(mesh.joints[vi][c]).or_insert(0.0) += mesh.weights[vi][c]; }
            }
            let j = acc.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap().then(b.0.cmp(a.0))).map(|(j, _)| *j).unwrap_or(0);
            let p = PALETTE[j as usize % PALETTE.len()];
            colors.push([p[0], p[1], p[2], 1.0]);
        }
        Reconstructor { rest_frames, colors, scale: [s, s, s * 0.25] }
    }

    /// Reconstruct every bound splat from the deformed triangle: centre =
    /// barycentric surface point + F'.o_s; rotation = quat(F') (= R*quat(F0),
    /// R = F'F0^T). Degenerate triangles fall back to the rest frame and are
    /// counted; no NaN/Inf can leave this function.
    pub fn reconstruct(&self, b: &SplatBinding, indices: &[u32], posed: &[[f32; 3]], rest: &[[f32; 3]], out: &mut Vec<GaussianSplat>) -> DeformStats {
        out.clear();
        out.reserve(b.records.len());
        let mut st = DeformStats::default();
        let mut degenerate_tris = std::collections::HashSet::new();
        for r in &b.records {
            let t = r.tri as usize;
            let (i0, i1, i2) = (indices[3 * t] as usize, indices[3 * t + 1] as usize, indices[3 * t + 2] as usize);
            let (v0, v1, v2) = (posed[i0], posed[i1], posed[i2]);
            let b0 = (1.0 - r.b1 - r.b2).max(0.0);
            let mut p = [0.0f32; 3];
            for c in 0..3 { p[c] = b0 * v0[c] + r.b1 * v1[c] + r.b2 * v2[c]; }
            let f = match tri_frame(v0, v1, v2) {
                Some(f) => f,
                None => { degenerate_tris.insert(t); self.rest_frames[t] }
            };
            for c in 0..3 { p[c] += r.off[0] * f[2][c] + r.off[1] * f[0][c] + r.off[2] * f[1][c]; }
            let q = quat_from_columns(f[0], f[1], f[2]);
            let mut rot = [q[3], q[0], q[1], q[2]]; // (w,x,y,z)
            if !(p.iter().all(|x| x.is_finite()) && rot.iter().all(|x| x.is_finite())) {
                st.nonfinite_repaired += 1;
                let (a, bb, cc) = (rest[i0], rest[i1], rest[i2]);
                for c in 0..3 { p[c] = b0 * a[c] + r.b1 * bb[c] + r.b2 * cc[c]; }
                rot = [1., 0., 0., 0.];
            }
            out.push(GaussianSplat { position: p, scale: self.scale, rotation: rot, color: self.colors[t] });
        }
        st.degenerate_triangles = degenerate_tris.len();
        st
    }
}
