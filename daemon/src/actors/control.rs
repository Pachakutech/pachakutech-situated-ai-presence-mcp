//! The Control actor — the easy one, per docs/architecture.md. Its only job
//! is turning an `addArtifact`/`retireArtifact` proposal into an entry (or
//! absence of one) in Scene Memory.

use super::scene_memory::{CanonicalCloud, SceneMemory};

#[derive(Default)]
pub struct ControlActor;

impl ControlActor {
    pub fn new() -> Self {
        Self
    }

    /// Ingests a splat cloud into Scene Memory. Real parsing of whatever
    /// `source_uri` points at (a .splat/.ply file, presumably) is the
    /// unbuilt part — this stands in with an empty canonical cloud so the
    /// rest of the pipeline (spawning/animating against a held artifact)
    /// has something real to look up rather than nothing at all.
    pub fn ingest_artifact(
        &self,
        memory: &mut SceneMemory,
        artifact_id: &str,
        description: &str,
        source_uri: Option<&str>,
    ) {
        println!(
            "[control] ingest {artifact_id}: \"{description}\" ({source_uri:?}) — \
             TODO: parse real splat data at source_uri instead of storing empty"
        );
        memory.insert(
            artifact_id,
            CanonicalCloud {
                description: description.to_string(),
                source_uri: source_uri.map(str::to_string),
                splats: Vec::new(),
                skinning: None,
            },
        );
    }

    pub fn retire_artifact(&self, memory: &mut SceneMemory, artifact_id: &str) -> Result<(), String> {
        memory
            .remove(artifact_id)
            .map(|_| ())
            .ok_or_else(|| format!("no held artifact with id {artifact_id}"))
    }
}
