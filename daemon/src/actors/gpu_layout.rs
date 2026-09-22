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

/// Matches GLSL's `DualQuat` (`math_utils.comp`): two vec4s, 32 bytes.
/// `real`/`dual` are each (w, x, y, z), matching the convention used
/// throughout this codebase (see `splat_io.rs`'s quaternion decode).
/// Two consecutive 16-byte fields naturally land at offsets 0 and 16 under
/// plain `#[repr(C)]` — no explicit alignment attribute needed, unlike the
/// V4L2 struct wrangling in `ingress/webcam_v4l2.rs` where an odd-sized
/// field made alignment non-obvious.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DualQuatGpu {
    pub real: [f32; 4],
    pub dual: [f32; 4],
}
pub const IDENTITY_DUAL_QUAT: DualQuatGpu =
    DualQuatGpu { real: [1.0, 0.0, 0.0, 0.0], dual: [0.0, 0.0, 0.0, 0.0] };

/// Matches `splat_projection.comp`'s `ProjectionUniforms` block under
/// std140 rules, computed by hand (no reflection tool available in this
/// sandbox to cross-check against, unlike the V4L2 struct sizes which
/// were checkable against a real C compiler):
///   world_to_camera_dq: DualQuat  -> base align 16 (struct w/ vec4s,
///                                    rounded up to 16), offset 0, size 32
///   focal:      vec2  -> align 8,  offset 32
///   principal:  vec2  -> align 8,  offset 40
///   screen_size:vec2  -> align 8,  offset 48
///   base_splat_scale: float -> align 4, offset 56
///   total_capacity:   uint  -> align 4, offset 60
///   current_frame:    uint  -> align 4, offset 64
/// True field data ends at byte 68. The trailing 12 bytes here are
/// unread padding, included defensively — some tools round a std140
/// block's own size up to its largest member's alignment (16), and
/// there's no downside to allocating a few extra unread bytes, only to
/// allocating too few. Field offsets (not the tail padding) are what
/// actually matter for correctness, and those are asserted individually
/// in this module's tests via `memoffset::offset_of!`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProjectionUniformsGpu {
    pub world_to_camera_dq: DualQuatGpu, // offset 0
    pub focal: [f32; 2],                 // offset 32
    pub principal: [f32; 2],             // offset 40
    pub screen_size: [f32; 2],           // offset 48
    pub base_splat_scale: f32,           // offset 56
    pub total_capacity: u32,             // offset 60
    pub current_frame: u32,              // offset 64
    _tail_padding: [u32; 3],             // offset 68..80
}

impl ProjectionUniformsGpu {
    pub fn new(
        world_to_camera_dq: DualQuatGpu,
        focal: [f32; 2],
        principal: [f32; 2],
        screen_size: [f32; 2],
        base_splat_scale: f32,
        total_capacity: u32,
        current_frame: u32,
    ) -> Self {
        Self {
            world_to_camera_dq,
            focal,
            principal,
            screen_size,
            base_splat_scale,
            total_capacity,
            current_frame,
            _tail_padding: [0; 3],
        }
    }
}

/// Matches `splat_eviction.comp`'s `EvictionUniforms` block. All-scalar
/// std140 members pack with no surprises: 4-byte alignment each,
/// sequential, no struct/vec4 members to force any rounding.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EvictionUniformsGpu {
    pub total_capacity: u32,
    pub current_frame: u32,
    pub absolute_max_age: u32,
    _tail_padding: u32,
}

impl EvictionUniformsGpu {
    pub fn new(total_capacity: u32, current_frame: u32, absolute_max_age: u32) -> Self {
        Self { total_capacity, current_frame, absolute_max_age, _tail_padding: 0 }
    }
}

/// Matches `splat_projection.comp`'s `ProjectedSplat`, an std430 array
/// element. `vec4 color` forces the struct's own alignment to 16 (the
/// array-element rule for std430 still rounds struct alignment up to the
/// largest contained alignment), which pushes `color` from a naive offset
/// of 20 up to 32 — 12 bytes of real, load-bearing padding in the layout,
/// not a mistake: `splat_id` legitimately ends at byte 20, and `color`
/// must start on a 16-byte boundary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProjectedSplatGpu {
    pub screen_center: [f32; 2], // offset 0
    pub radius_pixels: f32,      // offset 8
    pub depth: f32,              // offset 12
    pub splat_id: u32,           // offset 16
    _padding_before_color: [u32; 3], // offset 20..32
    pub color: [f32; 4],         // offset 32
}
const _: () = assert!(std::mem::size_of::<ProjectedSplatGpu>() == 48);

/// The eviction shader's hardcoded eviction-list bound
/// (`MAX_EVICTIONS_PER_FRAME` in `splat_eviction.comp`) — kept here too so
/// the buffer this daemon allocates for `EvictedTextures` is sized to
/// match, rather than duplicating the literal `256` a second place.
pub const MAX_EVICTIONS_PER_FRAME: u32 = 256;

/// Bone palette size this daemon allocates. An artifact's
/// `SkinningWeights` indexes into this with up to 4 bone indices per
/// splat; 64 is a generous starting budget for a single rigged
/// character, easy to raise later — nothing here assumes this exact
/// number beyond the buffer's own allocation size.
pub const MAX_BONES: u32 = 64;

#[cfg(test)]
mod tests {
    use super::*;
    use memoffset::offset_of;
    use std::mem::size_of;

    #[test]
    fn dual_quat_gpu_is_32_bytes() {
        assert_eq!(size_of::<DualQuatGpu>(), 32);
        assert_eq!(offset_of!(DualQuatGpu, real), 0);
        assert_eq!(offset_of!(DualQuatGpu, dual), 16);
    }

    #[test]
    fn projection_uniforms_offsets_match_std140() {
        assert_eq!(offset_of!(ProjectionUniformsGpu, world_to_camera_dq), 0);
        assert_eq!(offset_of!(ProjectionUniformsGpu, focal), 32);
        assert_eq!(offset_of!(ProjectionUniformsGpu, principal), 40);
        assert_eq!(offset_of!(ProjectionUniformsGpu, screen_size), 48);
        assert_eq!(offset_of!(ProjectionUniformsGpu, base_splat_scale), 56);
        assert_eq!(offset_of!(ProjectionUniformsGpu, total_capacity), 60);
        assert_eq!(offset_of!(ProjectionUniformsGpu, current_frame), 64);
        assert_eq!(size_of::<ProjectionUniformsGpu>(), 80);
    }

    #[test]
    fn eviction_uniforms_offsets_match_std140() {
        assert_eq!(offset_of!(EvictionUniformsGpu, total_capacity), 0);
        assert_eq!(offset_of!(EvictionUniformsGpu, current_frame), 4);
        assert_eq!(offset_of!(EvictionUniformsGpu, absolute_max_age), 8);
    }

    #[test]
    fn projected_splat_offsets_match_std430_array_element_rules() {
        assert_eq!(offset_of!(ProjectedSplatGpu, screen_center), 0);
        assert_eq!(offset_of!(ProjectedSplatGpu, radius_pixels), 8);
        assert_eq!(offset_of!(ProjectedSplatGpu, depth), 12);
        assert_eq!(offset_of!(ProjectedSplatGpu, splat_id), 16);
        assert_eq!(offset_of!(ProjectedSplatGpu, color), 32);
        assert_eq!(size_of::<ProjectedSplatGpu>(), 48);
    }
}

#[cfg(test)]
mod animated_splat_tests {
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
