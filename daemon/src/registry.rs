//! Where your architecture goes next. This is intentionally a thin stand-in
//! for the real Perception/State/Presence actor registry — it proves the
//! protocol and the Vulkan context are wired together, and logs what a real
//! actor would do. Replace the bodies here with actual GPU work (import a
//! dma_buf, write into a persistent splat buffer, etc.) as you build it.

use crate::vulkan::VulkanContext;
use std::collections::HashSet;

pub struct Registry {
    live_presences: HashSet<String>,
    live_artifacts: HashSet<String>,
}

impl Registry {
    pub fn new() -> Self {
        Self { live_presences: HashSet::new(), live_artifacts: HashSet::new() }
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
        self.live_presences.insert(presence_id.to_string());
        println!(
            "[registry] spawnPresence {presence_id} from \"{source_context}\" (style={style_hint:?}, artifact={artifact_id:?})"
        );
    }

    pub fn animate_presence(&self, presence_id: &str, text: &str) -> Result<(), String> {
        if !self.live_presences.contains(presence_id) {
            return Err(format!("no live presence with id {presence_id}"));
        }
        println!("[registry] animatePresence {presence_id}: \"{text}\"");
        Ok(())
    }

    pub fn retire_presence(&mut self, presence_id: &str) -> Result<(), String> {
        if !self.live_presences.remove(presence_id) {
            return Err(format!("no live presence with id {presence_id}"));
        }
        println!("[registry] retirePresence {presence_id}");
        Ok(())
    }

    pub fn add_artifact(&mut self, artifact_id: &str, description: &str, source_uri: Option<&str>) {
        self.live_artifacts.insert(artifact_id.to_string());
        println!("[registry] addArtifact {artifact_id}: \"{description}\" ({source_uri:?})");
    }

    pub fn retire_artifact(&mut self, artifact_id: &str) -> Result<(), String> {
        if !self.live_artifacts.remove(artifact_id) {
            return Err(format!("no held artifact with id {artifact_id}"));
        }
        println!("[registry] retireArtifact {artifact_id}");
        Ok(())
    }
}
