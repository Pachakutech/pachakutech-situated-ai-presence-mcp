//! Scene Memory: sparse, per-artifact storage the Control actor writes to
//! and the Presence actor reads from. "Sparse" means literally that — this
//! holds the canonical splat cloud and skinning weights an artifact was
//! ingested with, not a dense reconstruction of anything. Parsing real
//! splat data (from whatever an artifact's `source_uri` points at) is the
//! actual unbuilt work; `ControlActor::ingest_artifact` stands in for it.

use std::collections::HashMap;

/// One Gaussian in a splat cloud: position, per-axis linear scale (already
/// exponentiated, i.e. a real standard deviation, not a stored log), a
/// normalized rotation quaternion (w, x, y, z), and RGBA.
///
/// Splats are DQ-skinned (see `gpu_layout.rs`): skinning changes a
/// splat's effective rotation every frame, so Σ = R·S·Sᵗ·Rᵗ has to be
/// recomputed post-skinning, not baked in at load time. Scale+rotation is
/// therefore the right thing to carry in Scene Memory; `covariance()` below
/// is still there for the unskinned/rest-pose case (a quick CPU-side
/// preview, say), computed on demand rather than stored.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields are read once ingest_artifact/skinning apply are real
pub struct GaussianSplat {
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: [f32; 4],
    pub color: [f32; 4],
}

impl GaussianSplat {
    /// Rest-pose covariance, upper triangle [xx, xy, xz, yy, yz, zz].
    /// See `gpu_layout` for the post-skinning equivalent computed on GPU.
    pub fn covariance(&self) -> [f32; 6] {
        super::splat_io::covariance_from_scale_rotation(self.scale, self.rotation)
    }
}

/// Per-splat skinning weights against up to 4 bones — the same shape as
/// glTF's `JOINTS_0`/`WEIGHTS_0` mesh attributes, reused here for a splat
/// cloud instead of a mesh. Absent until a real rig exists for an artifact;
/// an unskinned cloud just doesn't move when animated.
#[derive(Debug, Clone)]
#[allow(dead_code)] // read once the skinning apply step is real
pub struct SkinningWeights {
    pub bone_indices: [u16; 4],
    pub bone_weights: [f32; 4],
}

/// What's held for one artifact: the canonical (rest-pose) cloud, optional
/// skinning to deform it, and enough provenance to match what came in over
/// `addArtifact`.
#[derive(Debug, Clone)]
#[allow(dead_code)] // description/source_uri/skinning are provenance + future fields, unread today
pub struct CanonicalCloud {
    pub description: String,
    pub source_uri: Option<String>,
    pub splats: Vec<GaussianSplat>,
    pub skinning: Option<Vec<SkinningWeights>>,
}

#[derive(Default)]
pub struct SceneMemory {
    artifacts: HashMap<String, CanonicalCloud>,
}

impl SceneMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, artifact_id: &str, cloud: CanonicalCloud) {
        self.artifacts.insert(artifact_id.to_string(), cloud);
    }

    pub fn get(&self, artifact_id: &str) -> Option<&CanonicalCloud> {
        self.artifacts.get(artifact_id)
    }

    pub fn remove(&mut self, artifact_id: &str) -> Option<CanonicalCloud> {
        self.artifacts.remove(artifact_id)
    }
}
