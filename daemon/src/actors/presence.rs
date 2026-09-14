//! The Presence actor — owns live presences and the text-to-motion mapping.
//! `text_to_pose_code` is the one genuinely unbuilt, research-heavy piece
//! (see docs/architecture.md for why this shape and not per-frame
//! regeneration); everything around it here is real bookkeeping, not a
//! stand-in.

use super::scene_memory::SceneMemory;
use std::collections::HashMap;

/// A compact per-frame control signal — pose + expression + global
/// transform, following the "~94 floats" shape current animatable-Gaussian
/// research converges on (see docs/architecture.md). `Vec<f32>` rather than
/// a fixed-size array because the real dimensionality depends on the rig a
/// given artifact was skinned with, which doesn't exist yet.
#[derive(Debug, Clone)]
pub struct PoseCode {
    pub params: Vec<f32>,
}

impl PoseCode {
    /// The rest/neutral pose — an all-zero code in whatever dimension.
    /// Every spawned presence starts here until `animate` moves it.
    pub fn neutral(dim: usize) -> Self {
        Self { params: vec![0.0; dim] }
    }
}

pub struct LivePresence {
    pub source_context: String,
    pub style_hint: Option<String>,
    pub artifact_id: Option<String>,
    pub current_pose: PoseCode,
}

#[derive(Default)]
pub struct PresenceActor {
    live: HashMap<String, LivePresence>,
}

impl PresenceActor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(
        &mut self,
        memory: &SceneMemory,
        presence_id: &str,
        source_context: &str,
        style_hint: Option<&str>,
        artifact_id: Option<&str>,
    ) {
        if let Some(id) = artifact_id {
            match memory.get(id) {
                Some(cloud) => println!(
                    "[presence] {presence_id} built from artifact {id} ({} splats held)",
                    cloud.splats.len()
                ),
                None => println!(
                    "[presence] {presence_id} referenced unknown artifact {id} — \
                     spawning from source_context alone"
                ),
            }
        }
        self.live.insert(
            presence_id.to_string(),
            LivePresence {
                source_context: source_context.to_string(),
                style_hint: style_hint.map(str::to_string),
                artifact_id: artifact_id.map(str::to_string),
                current_pose: PoseCode::neutral(0),
            },
        );
    }

    pub fn animate(&mut self, presence_id: &str, text: &str) -> Result<(), String> {
        let presence = self
            .live
            .get_mut(presence_id)
            .ok_or_else(|| format!("no live presence with id {presence_id}"))?;
        let dim = presence.current_pose.params.len().max(1);
        let pose = Self::text_to_pose_code(text, dim);
        println!(
            "[presence] {presence_id}: \"{text}\" -> pose code (dim {}) -> TODO: apply via \
             LBS/DQS to the canonical cloud (both the mapping and the skinning apply step are \
             unbuilt)",
            pose.params.len()
        );
        presence.current_pose = pose;
        Ok(())
    }

    pub fn retire(&mut self, presence_id: &str) -> Result<(), String> {
        self.live
            .remove(presence_id)
            .map(|_| ())
            .ok_or_else(|| format!("no live presence with id {presence_id}"))
    }

    /// TODO: the actual model. Maps free text to a compact pose/expression
    /// code instead of regenerating splat geometry per frame — see
    /// docs/architecture.md for why. Placeholder returns a neutral code of
    /// the requested dimension so callers have something real to hold.
    fn text_to_pose_code(_text: &str, dim: usize) -> PoseCode {
        PoseCode::neutral(dim)
    }
}
