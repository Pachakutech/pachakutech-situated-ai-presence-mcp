//! Wires the socket protocol to Control/Presence actors and, now, to the
//! live `SplatPipeline`. GPU work on ingest is a bump-allocated upload of
//! the canonical cloud; pose/skinning apply is still TODO on animate.

use crate::actors::gpu_layout::to_gpu_splat;
use crate::actors::{ControlActor, PresenceActor, SceneMemory};
use crate::pipeline::{SplatPipeline, INACTIVE_SPLAT};
use crate::vulkan::VulkanContext;
use std::collections::HashMap;

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
        }
    }

    pub fn highlight_region(&self, vk: &VulkanContext, description: &str, duration_secs: u32) {
        // TODO: resolve `description` against a captured frame and draw a
        // wlr-layer-shell overlay. For now, this just proves the daemon
        // received the proposal and has a live GPU to eventually draw with.
        println!(
            "[registry] highlightRegion on {}: \"{description}\" for {duration_secs}s",
            vk.device_name
        );
    }

    pub fn spawn_presence(
        &mut self,
        presence_id: &str,
        source_context: &str,
        style_hint: Option<&str>,
        artifact_id: Option<&str>,
    ) {
        self.presence.spawn(&self.memory, presence_id, source_context, style_hint, artifact_id);
    }

    pub fn animate_presence(&mut self, presence_id: &str, text: &str) -> Result<(), String> {
        self.presence.animate(presence_id, text)
    }

    pub fn retire_presence(&mut self, presence_id: &str) -> Result<(), String> {
        self.presence.retire(presence_id)
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
        let remaining = pipeline.capacity().saturating_sub(self.next_slot);
        if remaining == 0 {
            eprintln!("[registry] GPU buffer full — {artifact_id} held in Scene Memory only");
            return;
        }
        let n = (cloud.splats.len() as u32).min(remaining);
        if n < cloud.splats.len() as u32 {
            eprintln!(
                "[registry] {artifact_id}: truncated {} -> {n} splats (capacity {})",
                cloud.splats.len(),
                pipeline.capacity()
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
