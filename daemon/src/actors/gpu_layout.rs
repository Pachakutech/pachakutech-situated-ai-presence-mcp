//! The std430 layout `DynamicSplatBuffer` expects on the GPU side (the
//! `AnimatedSplat` struct in the projection/eviction/converter compute
//! shaders). This is the seam between Scene Memory (CPU, canonical,
//! rest-pose) and the presence runtime's actual render buffer (GPU,
//! animated, lifecycle-managed) — nothing here runs on GPU yet, but the
//! byte layout is real and tested so the eventual upload code has a
//! single source of truth instead of two independently-hand-written
//! structs drifting apart.
//!
//! Flag bits (kept in one place on purpose — the shaders currently
//! duplicate this convention across three files):
//!   bit 0 (0x1) = ACTIVE
//!   bit 1 (0x2) = TEXTURED_BILLBOARD
//!   bit 2 (0x4) = USE_SOFT_GAUSSIAN_FALLOFF
//!
//! `padding` is packed, not idle: low byte = density (0-255, 255 = fully
//! solid) per the projection shader's stochastic-thinning read of
//! `s.padding & 0xFFu`; bits 16-31 = a texture-atlas layer index, per the
//! eviction shader's `(padding >> 16u) & 0xFFFFu` read. Bits 8-15 are
//! genuinely unused today.

use super::scene_memory::{GaussianSplat, SkinningWeights};

pub const FLAG_ACTIVE: u32 = 1 << 0;
#[allow(dead_code)] // set once billboard ingress (webcam/screen) is wired in
pub const FLAG_TEXTURED_BILLBOARD: u32 = 1 << 1;
#[allow(dead_code)] // set once a converter/ingress path opts into soft falloff
pub const FLAG_SOFT_GAUSSIAN_FALLOFF: u32 = 1 << 2;

/// Byte-for-byte the GLSL `AnimatedSplat`: vec4 + vec4 + vec4 + uvec4 + vec4
/// + 4x uint = 96 bytes, no implicit padding (every member is already
/// 16-byte or 4-byte aligned in std430 as laid out here).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AnimatedSplatGpu {
    /// xyz = world position, w = "confidence" for a loose cloud or physical
    /// width for a textured billboard (the shader overloads this field —
    /// see docs/gpu-splat-pipeline.md).
    pub position_and_confidence: [f32; 4],
    /// Rotation quaternion (w, x, y, z) — the *rest-pose* rotation. The
    /// compute shader is the thing that blends this against bone
    /// transforms each frame; nothing here does that blending.
    pub rotation: [f32; 4],
    /// For a plain splat: solid RGBA. For a textured billboard, the
    /// projection shader repurposes this as a packed 2x2 affine matrix —
    /// this struct always carries plain color; that repacking is a
    /// GPU-side concern, not something to precompute here.
    pub color: [f32; 4],
    pub joint_ids: [u32; 4],
    pub weights: [f32; 4],
    pub owner_id: u32,
    pub last_visible_frame: u32,
    pub flags: u32,
    pub padding: u32,
}

/// Converts one canonical splat into its GPU-resident form. `skin` is the
/// per-splat entry from `CanonicalCloud.skinning` if the artifact has been
/// rigged; `None` means "rigid" — bound entirely to bone 0 with weight
/// 1.0, so the whole artifact moves as a single body under whatever
/// transform the Presence actor puts in `anim.bones[0]` (including
/// identity, i.e. genuinely static) until real per-splat rigging exists.
pub fn to_gpu_splat(
    splat: &GaussianSplat,
    skin: Option<&SkinningWeights>,
    owner_id: u32,
    current_frame: u32,
    density: u8,
) -> AnimatedSplatGpu {
    let (joint_ids, weights) = match skin {
        Some(s) => (
            [
                s.bone_indices[0] as u32,
                s.bone_indices[1] as u32,
                s.bone_indices[2] as u32,
                s.bone_indices[3] as u32,
            ],
            s.bone_weights,
        ),
        None => ([0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
    };

    AnimatedSplatGpu {
        position_and_confidence: [splat.position[0], splat.position[1], splat.position[2], 1.0],
        rotation: splat.rotation,
        color: splat.color,
        joint_ids,
        weights,
        owner_id,
        last_visible_frame: current_frame,
        flags: FLAG_ACTIVE,
        padding: density as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn matches_std430_size() {
        // vec4*3 + uvec4 + vec4 + u32*4 = 16*5 + 4*4 = 96 bytes, no padding.
        assert_eq!(size_of::<AnimatedSplatGpu>(), 96);
    }

    #[test]
    fn rigid_default_binds_to_bone_zero() {
        let splat = GaussianSplat {
            position: [1.0, 2.0, 3.0],
            scale: [1.0, 1.0, 1.0],
            rotation: [1.0, 0.0, 0.0, 0.0],
            color: [1.0, 0.0, 0.0, 1.0],
        };
        let gpu = to_gpu_splat(&splat, None, 42, 100, 255);
        assert_eq!(gpu.joint_ids, [0, 0, 0, 0]);
        assert_eq!(gpu.weights, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(gpu.owner_id, 42);
        assert_eq!(gpu.last_visible_frame, 100);
        assert_eq!(gpu.flags & FLAG_ACTIVE, FLAG_ACTIVE);
        assert_eq!(gpu.padding, 255);
    }

    #[test]
    fn carries_real_skinning_through() {
        let splat = GaussianSplat {
            position: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            rotation: [1.0, 0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let skin = SkinningWeights { bone_indices: [3, 7, 0, 0], bone_weights: [0.6, 0.4, 0.0, 0.0] };
        let gpu = to_gpu_splat(&splat, Some(&skin), 1, 0, 255);
        assert_eq!(gpu.joint_ids, [3, 7, 0, 0]);
        assert_eq!(gpu.weights, [0.6, 0.4, 0.0, 0.0]);
    }
}
