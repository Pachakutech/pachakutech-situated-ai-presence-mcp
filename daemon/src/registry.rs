//! Wires the socket protocol to Control/Presence actors and, now, to the
//! live `SplatPipeline`. GPU work on ingest is a bump-allocated upload of
//! the canonical cloud; pose/skinning apply is still TODO on animate.

use crate::actors::gpu_layout::to_gpu_splat;
use crate::actors::{ControlActor, PresenceActor, SceneMemory};
use crate::highlight::{InterestBook, UvBox, MAX_SECS};
use crate::pipeline::{SplatPipeline, INACTIVE_SPLAT};
use std::collections::HashMap;
use std::time::Instant;

struct GpuRange {
    start: u32,
    count: u32,
}

pub struct Registry {
    memory: SceneMemory,
    control: ControlActor,
    presence: PresenceActor,
    next_slot: u32,
    next_owner: u32,
    frame: u32,
    gpu_ranges: HashMap<String, GpuRange>,
    /// Whether the daemon has a loaded avatar asset to hand out.
    avatar_available: bool,
    /// The single avatar body belongs to at most one presence at a time.
    avatar_owner: Option<String>,
    /// One screen box per presence. The mark fades; the box stays until retire.
    interests: InterestBook,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            memory: SceneMemory::new(),
            control: ControlActor::new(),
            presence: PresenceActor::new(),
            next_slot: 0,
            next_owner: 1,
            frame: 1,
            gpu_ranges: HashMap::new(),
            avatar_available: false,
            avatar_owner: None,
            interests: InterestBook::new(),
        }
    }

    /// The presence must already be in the daemon map. It does not need the body.
    /// `secs` is clamped to 1..=60. The returned box is the one that will be drawn.
    pub fn declare_interest(
        &mut self,
        presence_id: &str,
        description: &str,
        raw: UvBox,
        secs: u32,
        now: Instant,
    ) -> Result<(UvBox, u32), String> {
        if !self.has_presence(presence_id) {
            return Err(format!("no live presence with id {presence_id}"));
        }
        let secs = secs.clamp(1, MAX_SECS);
        let got = self.interests.declare(presence_id, raw, secs, now)?;
        println!(
            "[registry] highlight {presence_id} uv {:.3},{:.3} {:.3}x{:.3} for {secs}s — {description}",
            got.u, got.v, got.w, got.h
        );
        Ok((got, secs))
    }

    /// Still the declared box after the mark has faded. Gesture sampling reads this.
    #[allow(dead_code)]
    pub fn interest(&self, presence_id: &str) -> Option<UvBox> {
        self.interests.get(presence_id)
    }

    pub fn visible_interests(&self, now: Instant) -> Vec<(String, UvBox)> {
        self.interests.visible(now)
    }

    pub fn set_avatar_available(&mut self, available: bool) {
        self.avatar_available = available;
    }

    /// Spawns the presence. Returns true if it was granted the avatar body
    /// (one body exists; later presences are tracked but have no body).
    pub fn spawn_presence(
        &mut self,
        presence_id: &str,
        source_context: &str,
        style_hint: Option<&str>,
        artifact_id: Option<&str>,
    ) -> bool {
        self.presence.spawn(&self.memory, presence_id, source_context, style_hint, artifact_id);
        if self.avatar_available && self.avatar_owner.is_none() {
            self.avatar_owner = Some(presence_id.to_string());
        }
        self.has_avatar(presence_id)
    }

    pub fn has_presence(&self, presence_id: &str) -> bool {
        self.presence.contains(presence_id)
    }

    pub fn has_avatar(&self, presence_id: &str) -> bool {
        self.avatar_owner.as_deref() == Some(presence_id) && self.presence.contains(presence_id)
    }

    pub fn avatar_owner(&self) -> Option<&str> {
        self.avatar_owner.as_deref()
    }

    pub fn animate_presence(&mut self, presence_id: &str, text: &str) -> Result<(), String> {
        self.presence.animate(presence_id, text)
    }

    /// Retires the presence. Returns true if it owned the avatar body (now released).
    pub fn retire_presence(&mut self, presence_id: &str) -> Result<bool, String> {
        self.presence.retire(presence_id)?;
        self.interests.retire(presence_id);
        let owned = self.avatar_owner.as_deref() == Some(presence_id);
        if owned {
            self.avatar_owner = None;
        }
        Ok(owned)
    }

    pub fn add_artifact(
        &mut self,
        artifact_id: &str,
        description: &str,
        source_uri: Option<&str>,
        pipeline: &SplatPipeline,
    ) {
        self.control.ingest_artifact(&mut self.memory, artifact_id, description, source_uri);
        self.upload_artifact(artifact_id, pipeline);
    }

    pub fn retire_artifact(&mut self, artifact_id: &str, pipeline: &SplatPipeline) -> Result<(), String> {
        self.deactivate_artifact(artifact_id, pipeline);
        self.control.retire_artifact(&mut self.memory, artifact_id)
    }

    fn upload_artifact(&mut self, artifact_id: &str, pipeline: &SplatPipeline) {
        let Some(cloud) = self.memory.get(artifact_id) else {
            return;
        };
        if cloud.splats.is_empty() {
            return;
        }
        let ceiling = pipeline.artifact_slot_limit();
        let remaining = ceiling.saturating_sub(self.next_slot);
        if remaining == 0 {
            eprintln!("[registry] artifact slot range full — {artifact_id} held in Scene Memory only");
            return;
        }
        let n = (cloud.splats.len() as u32).min(remaining);
        if n < cloud.splats.len() as u32 {
            eprintln!(
                "[registry] {artifact_id}: truncated {} -> {n} splats (artifact slots {ceiling})",
                cloud.splats.len()
            );
        }
        let owner = self.next_owner;
        self.next_owner = self.next_owner.wrapping_add(1);
        self.frame = self.frame.saturating_add(1);
        let start = self.next_slot;
        for (i, splat) in cloud.splats.iter().take(n as usize).enumerate() {
            let skin = cloud.skinning.as_ref().and_then(|s| s.get(i));
            pipeline.write_splat(
                start + i as u32,
                &to_gpu_splat(splat, skin, owner, self.frame, 255),
            );
        }
        self.next_slot += n;
        self.gpu_ranges.insert(artifact_id.to_string(), GpuRange { start, count: n });
        if let Err(e) = pipeline.tick(self.frame, 600) {
            eprintln!("[registry] tick after ingest {artifact_id}: {e}");
        } else {
            println!("[registry] uploaded {n} splats for {artifact_id} at slot {start}");
        }
    }

    fn deactivate_artifact(&mut self, artifact_id: &str, pipeline: &SplatPipeline) {
        let Some(range) = self.gpu_ranges.remove(artifact_id) else {
            return;
        };
        for i in 0..range.count {
            pipeline.write_splat(range.start + i, &INACTIVE_SPLAT);
        }
        self.frame = self.frame.saturating_add(1);
        if let Err(e) = pipeline.tick(self.frame, 600) {
            eprintln!("[registry] tick after retire {artifact_id}: {e}");
        }
    }
}
