//! Screen/webcam ingress → billboard `AnimatedSplat`, per
//! docs/gpu-splat-pipeline.md's "webcam/screen ingress" section: a
//! captured frame doesn't need a depth model to enter the presence layer,
//! it enters as a camera-facing billboard, using the exact code path
//! `splat_projection.comp`'s `isTexturedBillboard` branch already has.
//!
//! What's real vs. not, honestly: `frame_to_billboard` and `IngressActor`
//! below are pure data transforms with no OS dependency, and are unit
//! tested in this sandbox. The two capture backends (`screen_wlr`,
//! `webcam_v4l2`) are real, compiling code — but this sandbox has no
//! Wayland compositor and no `/dev/video0`, so they've only been verified
//! to *compile*, never to actually open a display connection or a camera
//! device. That has to happen on your Omarchy box, not here. Same
//! "dlopen at runtime, fail clearly if missing" posture as `ash`'s Vulkan
//! loading — see Cargo.toml.

pub mod screen_wlr;
pub mod webcam_v4l2;

use super::gpu_layout::{AnimatedSplatGpu, FLAG_ACTIVE, FLAG_TEXTURED_BILLBOARD};
use std::collections::HashMap;

/// One decoded frame from any capture source: RGBA8, top-left origin.
/// Getting bytes into an actual Vulkan-visible texture (the atlas
/// `texture_layer` indexes into) is a separate, not-yet-built step; this
/// is the CPU-side shape that step would consume.
pub struct IngressFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

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
/// …) and keeps it refreshed every tick a new frame arrives — bumping
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
