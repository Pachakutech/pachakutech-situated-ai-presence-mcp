//! Skinned-proxy avatar producer (docs/plans/skinned-proxy-*.md, with the
//! integration decision from docs/plans/cues-to-splats.md): deform the proxy
//! mesh on the CPU, reconstruct the barycentrically bound Gaussians, and write
//! them into the *existing* AnimatedSplatGpu buffer as rigid (skin = None)
//! splats. The projection shader and gpu_layout.rs are untouched.
//!
//! Status: compile- and unit-test-verified only. Nothing rasterizes projected
//! splats to the screen yet, and the daemon has no >=30 Hz tick, so this is
//! not wired into the run loop.
pub mod debug;
pub mod deform;
pub mod glb;
pub mod math;
pub mod rig;
pub mod splatbind;
#[cfg(test)]
mod tests;

use crate::actors::gpu_layout::{to_gpu_splat, FLAG_SOFT_GAUSSIAN_FALLOFF};
use crate::actors::scene_memory::GaussianSplat;
use crate::pipeline::SplatPipeline;
use deform::{DeformStats, ProxyMesh, Reconstructor};
use math::*;
use rig::Rig;
use splatbind::SplatBinding;
use std::path::Path;

/// Time-varying face input. Written by whatever drives speech later
/// (text_to_cues); the avatar only consumes it. `morphs[i]` weights mesh
/// morph target i (empty = all zero).
#[derive(Clone, Default, Debug)]
pub struct FaceFrame {
    /// 0.0 closed .. 1.0 fully open (manifest jaw_max_open_deg).
    pub jaw_open: f32,
    pub morphs: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct RootState {
    pub pos: [f32; 3],
    /// Radians about +Y; 0 faces +Z.
    pub yaw: f32,
}

pub struct AvatarActor {
    pub mesh: ProxyMesh,
    pub rig: Rig,
    pub binding: SplatBinding,
    recon: Reconstructor,
    pub jaw_joint: Option<usize>,
    pub jaw_axis: [f32; 3],
    pub jaw_max_rad: f32,
    pub ground_offset: f32,
    pub face: FaceFrame,
    pub root: RootState,
    clip: Option<String>,
    clip_time: f32,
    clip_loop: bool,
    walk_target: Option<[f32; 2]>,
    posed_verts: Vec<[f32; 3]>,
    pub posed: Vec<GaussianSplat>,
    pub stats: DeformStats,
}

impl AvatarActor {
    /// Load from the manifest (assets/avatar_manifest.json). Fails hard on any
    /// fingerprint mismatch; never renders a binding against the wrong mesh.
    pub fn load(manifest_path: &Path) -> Result<AvatarActor, String> {
        let dir = manifest_path.parent().unwrap_or(Path::new("."));
        let man: serde_json::Value = serde_json::from_slice(&std::fs::read(manifest_path).map_err(|e| format!("manifest: {e}"))?)
            .map_err(|e| format!("manifest json: {e}"))?;
        let glb = glb::Glb::load(&dir.join(man["asset"].as_str().ok_or("manifest.asset")?))?;
        let mesh = ProxyMesh::from_glb(&glb)?;
        let rig = Rig::from_glb(&glb)?;
        let bind_bytes = std::fs::read(dir.join(man["splatbind"].as_str().ok_or("manifest.splatbind")?)).map_err(|e| format!("splatbind: {e}"))?;
        let binding = SplatBinding::parse(&bind_bytes)?;
        binding.validate(&mesh.rest, &mesh.indices)?;
        let recon = Reconstructor::new(&mesh, &binding);
        let axis = man["jaw_axis"].as_array().map(|a| [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.) as f32)).unwrap_or([1., 0., 0.]);
        let mut a = AvatarActor {
            jaw_joint: man["jaw_joint"].as_str().and_then(|n| rig.joint(n)),
            jaw_axis: axis,
            jaw_max_rad: (man["jaw_max_open_deg"].as_f64().unwrap_or(25.0) as f32).to_radians(),
            ground_offset: man["ground_offset_y"].as_f64().unwrap_or(0.0) as f32,
            mesh, rig, binding, recon,
            face: FaceFrame::default(),
            root: RootState { pos: [0.; 3], yaw: 0. },
            clip: None, clip_time: 0., clip_loop: true, walk_target: None,
            posed_verts: vec![], posed: vec![], stats: DeformStats::default(),
        };
        a.play("idle", true);
        a.evaluate();
        Ok(a)
    }

    pub fn splat_count(&self) -> usize { self.binding.records.len() }
    pub fn play(&mut self, clip: &str, looping: bool) {
        if self.rig.clip(clip).is_some() && (self.clip.as_deref() != Some(clip)) {
            self.clip = Some(clip.to_string());
            self.clip_time = 0.;
        }
        self.clip_loop = looping;
    }
    pub fn rest(&mut self) { self.clip = None; }
    pub fn set_clip_time(&mut self, t: f32) { self.clip_time = t; }
    pub fn set_face(&mut self, f: FaceFrame) { self.face = f; }
    pub fn walk_to(&mut self, x: f32, z: f32) { self.walk_target = Some([x, z]); }

    /// Advance clock and locomotion by dt seconds.
    pub fn advance(&mut self, dt: f32) {
        self.clip_time += dt;
        if let Some([tx, tz]) = self.walk_target {
            let (dx, dz) = (tx - self.root.pos[0], tz - self.root.pos[2]);
            let dist = (dx * dx + dz * dz).sqrt();
            if dist < 0.05 {
                self.walk_target = None;
                self.play("idle", true);
            } else {
                let want = dx.atan2(dz); // yaw 0 faces +Z
                let mut d = (want - self.root.yaw + std::f32::consts::PI).rem_euclid(2. * std::f32::consts::PI) - std::f32::consts::PI;
                d = d.clamp(-3.0 * dt, 3.0 * dt);
                self.root.yaw += d;
                self.play("walk", true);
                let step = (1.2 * dt).min(dist);
                self.root.pos[0] += self.root.yaw.sin() * step;
                self.root.pos[2] += self.root.yaw.cos() * step;
            }
        }
    }

    /// Run clip -> face -> FK -> morph/LBS -> barycentric reconstruction.
    pub fn evaluate(&mut self) -> &[GaussianSplat] {
        let mut pose = match self.clip.as_deref().and_then(|n| self.rig.clip(n)) {
            Some(c) => self.rig.sample(c, self.clip_time, self.clip_loop),
            None => self.rig.rest_pose(),
        };
        if let Some(j) = self.jaw_joint {
            let ang = self.face.jaw_open.clamp(0., 1.) * self.jaw_max_rad;
            if ang != 0.0 { pose.r[j] = quat_mul(pose.r[j], quat_axis_angle(self.jaw_axis, ang)); }
        }
        let mut root = mat4_from_trs(self.root.pos, quat_axis_angle([0., 1., 0.], self.root.yaw));
        root[13] -= self.ground_offset; // soles to y = 0
        let mats = self.rig.skin_matrices(&pose, &root);
        self.mesh.skin(&self.face.morphs, &mats, &mut self.posed_verts);
        self.stats = self.recon.reconstruct(&self.binding, &self.mesh.indices, &self.posed_verts, &self.mesh.rest, &mut self.posed);
        &self.posed
    }

    /// Write posed splats into pipeline slots [slot_base, ..), clipped to capacity.
    pub fn upload(&self, pipeline: &SplatPipeline, slot_base: u32, owner_id: u32, frame: u32) -> u32 {
        let room = pipeline.capacity().saturating_sub(slot_base) as usize;
        let n = self.posed.len().min(room);
        for (i, s) in self.posed.iter().take(n).enumerate() {
            let mut g = to_gpu_splat(s, None, owner_id, frame, 255);
            g.flags |= FLAG_SOFT_GAUSSIAN_FALLOFF;
            pipeline.write_splat(slot_base + i as u32, &g);
        }
        n as u32
    }
}
