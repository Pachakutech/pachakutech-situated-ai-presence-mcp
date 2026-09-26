//! Screen/webcam ingress → billboard `AnimatedSplat`, per
//! docs/gpu-splat-pipeline.md's "webcam/screen ingress" section: a
//! captured frame doesn't need a depth model to enter the presence layer,
//! it enters as a camera-facing billboard, using the exact code path
//! `splat_projection.comp`'s `isTexturedBillboard` branch already has.
//!
//! Two capture backends live here:
//! - `screen_wlr` — SHM-based wlr-screencopy (CPU readback path, fallback)
//! - `screen_dmabuf` — one-copy wlr-screencopy with dmabuf → Vulkan import
//!   (the preferred path when the compositor and GPU driver both support it).
//!   Uses GBM-allocated dmabufs that the compositor copies frames into —
//!   one GPU-to-GPU copy, no CPU readback.

pub mod screen_dmabuf;
pub mod screen_wlr;
pub mod webcam_v4l2;

use super::gpu_layout::{AnimatedSplatGpu, FLAG_ACTIVE, FLAG_TEXTURED_BILLBOARD};
use std::collections::HashMap;
use std::os::fd::OwnedFd;

/// One decoded frame from any SHM capture source: RGBA8, top-left origin.
/// Getting bytes into an actual Vulkan-visible texture (the atlas
/// `texture_layer` indexes into) is a separate, not-yet-built step; this
/// is the CPU-side shape that step would consume.
pub struct IngressFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One plane of a DMA-BUF. The `fd` is owned by us until it is
/// handed to Vulkan via `VkImportMemoryFdInfoKHR` — at which point
/// ownership transfers to the Vulkan driver and must NOT be closed.
/// In the screencopy-dmabuf path, the fd comes from a GBM-allocated
/// buffer that the compositor has copied the frame into.
pub struct DmabufPlane {
    pub fd: OwnedFd,
    pub stride: u32,
    pub offset: u32,
    pub plane_index: u32,
}

/// A captured frame as a DMA-BUF. This is the GPU-side equivalent of
/// `IngressFrame`: instead of CPU-side RGBA bytes, it carries the dmabuf
/// file descriptor(s) + format/modifier/plane metadata needed to import
/// the buffer directly into a Vulkan `VkImage`.
///
/// In the screencopy-dmabuf path, the dmabuf is client-allocated (via
/// GBM) and the compositor copies the frame into it — one GPU-to-GPU
/// copy, no CPU readback.
///
/// **Initial constraints** (see the correct_screen_ingress PDF):
/// - Accept only one exported object / one fd (single-plane).
/// - Accept only single-plane XRGB/ARGB/XBGR/ABGR formats.
/// - Accept only DRM modifiers the Vulkan physical device reports as
///   importable for that format.
/// Multi-plane YUV / multi-object support should be added deliberately.
pub struct DmabufFrame {
    pub width: u32,
    pub height: u32,
    pub drm_format: u32,
    pub modifier: u64,
    pub planes: Vec<DmabufPlane>,
}

impl DmabufFrame {
    /// `true` when the source DRM format has no meaningful alpha channel
    /// (XRGB/XBGR). The shader should force alpha to 1.0 for these rather
    /// than treating undefined alpha as image data.
    pub fn opaque_alpha(&self) -> bool {
        matches!(
            self.drm_format,
            DRM_FORMAT_XRGB8888 | DRM_FORMAT_XBGR8888
        )
    }
}

/// DRM fourcc codes — little-endian byte order, matching the kernel's
/// `fourcc_code('X','R','2','4')` macro. Defined here rather than pulling
/// in the `drm_fourcc` crate for a handful of constants.
pub const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // 'X','R','2','4'
pub const DRM_FORMAT_ARGB8888: u32 = 0x34325241; // 'A','R','2','4'
pub const DRM_FORMAT_XBGR8888: u32 = 0x34324258; // 'X','B','2','4'
pub const DRM_FORMAT_ABGR8888: u32 = 0x34324241; // 'A','B','2','4'
pub const DRM_FORMAT_MOD_INVALID: u64 = 0x00FFFFFFFFFFFFFF;

/// A capture backend: `next_frame` returns the latest frame if one is
/// ready, or `None` without blocking if not (an ingress actor should keep
/// re-showing the last billboard rather than stall the whole tick waiting
/// on a slow source).
pub trait FrameSource {
    fn next_frame(&mut self) -> Result<Option<IngressFrame>, String>;
}

/// Packs a density (0-255, low byte) and a texture-atlas layer index
/// (16 bits) into `AnimatedSplat.padding`, matching the read side in
/// `splat_projection.comp` (`padding & 0xFFu`) and `splat_eviction.comp`
/// (`(padding >> 16u) & 0xFFFFu`).
pub fn pack_padding(density: u8, texture_layer: u16) -> u32 {
    (density as u32) | ((texture_layer as u32) << 16)
}

/// Turns one captured frame into a camera-facing billboard `AnimatedSplat`.
/// `position` and `rotation` place and orient the billboard in world
/// space (e.g. "directly in front of the tracked camera pose, facing it"
/// for a screen capture); `physical_width_m` is the billboard's real-world
/// width, matching the projection shader's use of
/// `position_and_confidence.w`. Aspect ratio is derived from the frame's
/// own pixel dimensions and packed into `color.w`, per the projection
/// shader's convention.
pub fn frame_to_billboard(
    frame: &IngressFrame,
    position: [f32; 3],
    rotation: [f32; 4],
    physical_width_m: f32,
    owner_id: u32,
    current_frame: u32,
    texture_layer: u16,
) -> AnimatedSplatGpu {
    let aspect_ratio = frame.width as f32 / frame.height.max(1) as f32;
    AnimatedSplatGpu {
        position_and_confidence: [position[0], position[1], position[2], physical_width_m],
        rotation,
        color: [1.0, 1.0, 1.0, aspect_ratio],
        joint_ids: [0, 0, 0, 0],
        weights: [1.0, 0.0, 0.0, 0.0],
        owner_id,
        last_visible_frame: current_frame,
        flags: FLAG_ACTIVE | FLAG_TEXTURED_BILLBOARD,
        padding: pack_padding(255, texture_layer),
    }
}

/// Holds one billboard slot per named ingress source ("screen", "webcam",
/// "…") and keeps it refreshed every tick a new frame arrives — bumping
/// `last_visible_frame` every call is what keeps `splat_eviction.comp`
/// from reclaiming a still-live feed (see its `absolute_max_age` check).
/// This is the CPU-side mirror of what would actually be written into the
/// GPU's `DynamicSplatBuffer`; the upload step itself doesn't exist yet
/// (no compute-dispatch loop in main.rs today).
#[derive(Default)]
pub struct IngressActor {
    slots: HashMap<String, AnimatedSplatGpu>,
}

impl IngressActor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_slot(
        &mut self,
        name: &str,
        frame: &IngressFrame,
        position: [f32; 3],
        rotation: [f32; 4],
        physical_width_m: f32,
        owner_id: u32,
        current_frame: u32,
        texture_layer: u16,
    ) {
        let billboard = frame_to_billboard(
            frame, position, rotation, physical_width_m, owner_id, current_frame, texture_layer,
        );
        self.slots.insert(name.to_string(), billboard);
    }

    pub fn get(&self, name: &str) -> Option<&AnimatedSplatGpu> {
        self.slots.get(name)
    }

    pub fn slot_names(&self) -> impl Iterator<Item = &String> {
        self.slots.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_frame(width: u32, height: u32) -> IngressFrame {
        IngressFrame { width, height, rgba: vec![0u8; (width * height * 4) as usize] }
    }

    #[test]
    fn padding_round_trips_density_and_layer() {
        let packed = pack_padding(200, 4321);
        assert_eq!(packed & 0xFF, 200);
        assert_eq!((packed >> 16) & 0xFFFF, 4321);
    }

    #[test]
    fn billboard_carries_aspect_ratio_and_flags() {
        // A 1920x1080 screen capture -> aspect ratio 16:9.
        let frame = synthetic_frame(1920, 1080);
        let gpu = frame_to_billboard(&frame, [0.0, 0.0, 2.0], [1.0, 0.0, 0.0, 0.0], 0.6, 99, 42, 3);
        assert!((gpu.color[3] - 1920.0 / 1080.0).abs() < 1e-4, "aspect ratio, got {}", gpu.color[3]);
        assert_eq!(gpu.flags & FLAG_ACTIVE, FLAG_ACTIVE);
        assert_eq!(gpu.flags & FLAG_TEXTURED_BILLBOARD, FLAG_TEXTURED_BILLBOARD);
        assert_eq!(gpu.position_and_confidence, [0.0, 0.0, 2.0, 0.6]);
        assert_eq!(gpu.owner_id, 99);
        assert_eq!(gpu.last_visible_frame, 42);
        assert_eq!((gpu.padding >> 16) & 0xFFFF, 3);
    }

    #[test]
    fn ingress_actor_tracks_named_slots_and_refreshes_last_visible_frame() {
        let mut ingress = IngressActor::new();
        let frame = synthetic_frame(640, 480);
        ingress.update_slot("webcam", &frame, [0.0; 3], [1.0, 0.0, 0.0, 0.0], 0.3, 1, 10, 0);
        assert_eq!(ingress.get("webcam").unwrap().last_visible_frame, 10);

        // A later tick with a fresh frame should bump last_visible_frame
        // so LRU eviction never reclaims a still-live source.
        ingress.update_slot("webcam", &frame, [0.0; 3], [1.0, 0.0, 0.0, 0.0], 0.3, 1, 11, 0);
        assert_eq!(ingress.get("webcam").unwrap().last_visible_frame, 11);

        assert!(ingress.get("screen").is_none());
        assert_eq!(ingress.slot_names().count(), 1);
    }
}
