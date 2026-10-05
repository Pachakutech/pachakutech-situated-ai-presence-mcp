//! Skinned-proxy avatar producer (docs/plans/skinned-proxy-*.md, with the
//! integration decision from docs/plans/cues-to-splats.md): deform the proxy
//! mesh on the CPU, reconstruct the barycentrically bound Gaussians, and write
//! them into the *existing* AnimatedSplatGpu buffer as rigid (skin = None)
//! splats. The projection shader and gpu_layout.rs are untouched.
//!
//! The overlay loop ticks this at 30 Hz and draws the projected discs. Speech
//! stays out: `FaceFrame` is demo motion or a socket command.
pub mod debug;
pub mod deform;
pub mod glb;
pub mod math;
pub mod rig;
pub mod splatbind;
#[cfg(test)]
mod tests;

use crate::actors::gpu_layout::{to_gpu_splat, DualQuatGpu, ProjectionUniformsGpu, FLAG_SOFT_GAUSSIAN_FALLOFF};
use crate::actors::scene_memory::GaussianSplat;
use crate::pipeline::SplatPipeline;
use crate::protocol::AvatarCommand;
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
    /// When set, the tick sways yaw and opens the jaw. Socket controls clear it.
    pub demo_motion: bool,
    time: f32,
    posed_verts: Vec<[f32; 3]>,
    pub posed: Vec<GaussianSplat>,
    pub stats: DeformStats,
}

/// Rigid camera: on +Z, looking at the origin, with the body upright.
/// `Ry(π)` turns the face (+Z) toward the camera. Translation is `-R * camera`.
fn body_camera_dq() -> DualQuatGpu {
    let q = quat_axis_angle([0.0, 1.0, 0.0], std::f32::consts::PI);
    let cam_y = 0.90_f32;
    let cam_z = 2.55_f32;
    let t = [0.0_f32, -cam_y, cam_z];
    let dual = quat_mul([t[0], t[1], t[2], 0.0], q);
    DualQuatGpu {
        real: [q[3], q[0], q[1], q[2]],
        dual: [dual[3] * 0.5, dual[0] * 0.5, dual[1] * 0.5, dual[2] * 0.5],
    }
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
            demo_motion: true, time: 0.,
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

    pub fn apply_command(&mut self, cmd: AvatarCommand) {
        match cmd {
            AvatarCommand::Rest => {
                self.demo_motion = false;
                self.rest();
                self.walk_target = None;
                self.root.yaw = 0.0;
                self.face = FaceFrame::default();
            }
            AvatarCommand::Face { jaw_open, morph_index, morph_weight } => {
                self.demo_motion = false;
                let mut morphs = vec![0.0; self.mesh.morph_names.len()];
                if let Some(i) = morph_index {
                    if let Some(slot) = morphs.get_mut(i as usize) {
                        *slot = morph_weight;
                    }
                }
                self.face = FaceFrame { jaw_open, morphs };
            }
            AvatarCommand::Walk { x, z } => {
                self.demo_motion = false;
                self.walk_to(x, z);
            }
        }
    }

    /// Advance clock and locomotion by dt seconds.
    pub fn advance(&mut self, dt: f32) {
        self.time += dt;
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
        if self.demo_motion {
            if self.walk_target.is_none() {
                self.root.yaw = 0.45 * (self.time * 0.7).sin();
            }
            self.face.jaw_open = 0.5 + 0.5 * (self.time * 1.7).sin();
        }
    }

    /// Camera for the 220px layer: body centered, facing the disc, +Y up.
    pub fn layer_camera(width: f32, height: f32, frame: u32) -> ProjectionUniformsGpu {
        let focal = width.max(height) * 1.2;
        ProjectionUniformsGpu::new(
            body_camera_dq(),
            [focal, -focal],
            [width * 0.5, height * 0.5],
            [width, height],
            0.022,
            0,
            frame,
        )
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
