//! Where your architecture goes next. `highlight_region` is still a thin
//! stand-in — it just proves the daemon received the proposal and has a
//! live GPU to eventually draw with. Presence/artifact bookkeeping now runs
//! through the real Control/Presence actor split (`actors::`) instead of a
//! flat set of ids, matching docs/architecture.md. This struct is
//! deliberately thin: it wires the socket protocol to those actors and
//! owns nothing itself. The actual GPU work — importing a dma_buf, writing
//! into a persistent splat buffer, applying skinning on the device — is
//! still all TODO inside the actors it delegates to.

use crate::actors::{ControlActor, PresenceActor, SceneMemory};
use crate::vulkan::VulkanContext;

pub struct Registry {
    memory: SceneMemory,
    control: ControlActor,
    presence: PresenceActor,
}

impl Registry {
    pub fn new() -> Self {
        Self { memory: SceneMemory::new(), control: ControlActor::new(), presence: PresenceActor::new() }
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

    pub fn add_artifact(&mut self, artifact_id: &str, description: &str, source_uri: Option<&str>) {
        self.control.ingest_artifact(&mut self.memory, artifact_id, description, source_uri);
    }

    pub fn retire_artifact(&mut self, artifact_id: &str) -> Result<(), String> {
        self.control.retire_artifact(&mut self.memory, artifact_id)
    }
}
