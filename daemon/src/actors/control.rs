//! The Control actor — the easy one, per docs/architecture.md. Its only job
//! is turning an `addArtifact`/`retireArtifact` proposal into an entry (or
//! absence of one) in Scene Memory.

use super::scene_memory::{CanonicalCloud, SceneMemory};
use super::splat_io;
use std::path::Path;

#[derive(Default)]
pub struct ControlActor;

impl ControlActor {
    pub fn new() -> Self {
        Self
    }

    /// Ingests a splat cloud into Scene Memory. `source_uri` is expected to
    /// be a local `.splat` or `.ply` file path — see `splat_io` for the
    /// real parsers. Remote URLs aren't fetched (no HTTP client wired in
    /// for this yet) and land in the same "couldn't load" fallback as any
    /// other read failure: log clearly, keep an empty cloud rather than
    /// fail the whole `addArtifact` call. A held artifact with zero splats
    /// is a legitimate (if useless) state; refusing the call outright for
    /// a bad path is not — the client already has an artifactId by the
    /// time this runs.
    pub fn ingest_artifact(
        &self,
        memory: &mut SceneMemory,
        artifact_id: &str,
        description: &str,
        source_uri: Option<&str>,
    ) {
        let splats = match source_uri {
            Some(uri) => match splat_io::load_from_path(Path::new(uri)) {
                Ok(splats) => {
                    println!("[control] ingest {artifact_id}: loaded {} splats from {uri}", splats.len());
                    splats
                }
                Err(e) => {
                    eprintln!("[control] ingest {artifact_id}: couldn't load '{uri}' ({e}) — holding an empty cloud instead");
                    Vec::new()
                }
            },
            None => {
                println!("[control] ingest {artifact_id}: \"{description}\" with no source_uri — holding an empty cloud");
                Vec::new()
            }
        };
        memory.insert(
            artifact_id,
            CanonicalCloud {
                description: description.to_string(),
                source_uri: source_uri.map(str::to_string),
                splats,
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
