architectural decision for integrating the skinned-proxy avatar with the existing `AnimatedSplatGpu` / pure-DQ path.

**keep the existing pure-DQ contract intact for non-rigged artifacts, and treat mesh+barycentric as an upstream producer that feeds the same `AnimatedSplatGpu` buffer.** You do not need to rewrite the projection/eviction shaders or break the current layout for v1.

### What the current contract already assumes

From `gpu_layout.rs` and the pipeline docs:

- Canonical side: `GaussianSplat` (rest-pose position, rotation, scale, color) + optional `SkinningWeights`.
- GPU side: fixed 96-byte `AnimatedSplatGpu` with `joint_ids[4]` / `weights[4]`.
- `to_gpu_splat(..., skin: None, ...)` → rigid bind to bone 0 (weight 1).
- With skinning → the **projection compute shader** does dual-quaternion blending each frame against the bone palette.
- Non-rigged artifacts and billboard ingress already live happily in this model.

That path stays correct and should remain the default for ordinary artifacts and screen/webcam billboards.

### Where mesh + barycentric sits relative to that

The avatar plan is different in one important respect:

- Visemes / jaw are **mesh morph targets + a jaw joint**, not independent per-splat rest positions.
- Splats are surface samples: `triangleIndex + barycentrics (+ optional offset)`.
- Each frame the *proxy surface* deforms first (morph → skin), then each splat is reconstructed from the deformed triangle.

Pure rest-pose DQ on fixed splat positions cannot express those live morph deltas by itself. So the mesh path is not “already covered” by the current projection shader; it needs a small, explicit step **before** the existing GPU layout is written.

### Recommended fusion (minimal change, dual-mode)

Keep one buffer and one projection path. Differentiate only in how you *fill* the buffer:

| Kind of content              | How positions get into `AnimatedSplatGpu`                          | Skinning on GPU                          |
|-----------------------------|---------------------------------------------------------------------|------------------------------------------|
| Non-rigged artifact         | Rest-pose positions from the cloud                                  | Rigid bone 0 (existing)                  |
| Bone-rigged artifact (no morphs) | Rest-pose + `SkinningWeights`                                  | Full DQB in projection shader (existing) |
| Mesh-bound avatar (visemes) | Deform proxy mesh → reconstruct from barycentrics → write **final** (or near-final) positions | Rigid / identity (or light residual)     |

Concrete steps for the avatar:

1. **Asset side (unchanged from earlier advice)**
    - `humanoid_proxy.glb` (topology + morphs + jaw + skin weights).
    - Offline bake → `humanoid_proxy.splatbind` (triangle + bary + optional offset, plus fingerprint).
    - Optionally store inherited skin weights per sample if you ever want residual skeletal motion after reconstruction.

2. **Presence / avatar actor (new component, not a rewrite of projection)**
    - Own the proxy mesh buffers + morph target deltas + current faceanim (jaw + viseme weights) + body pose.
    - Each frame:
        - Apply morph weights to rest vertices.
        - Apply skeletal skinning (CPU or a small compute pass) including the jaw joint.
        - For each entry in the splatbind:  
          \( p_s = b_0 p_{i0} + b_1 p_{i1} + b_2 p_{i2} + o_s \)  
          (and derive a reasonable rotation / local frame from the deformed triangle if you need oriented splats).
        - Emit ordinary `GaussianSplat`-like records (or directly `AnimatedSplatGpu`) with those positions.
    - Call the existing `to_gpu_splat` path with `skin: None` (or a trivial rigid bind). The projection shader then treats them exactly like any other active splat.

3. **Projection / eviction / `gpu_layout.rs`**
    - No structural change required for the first version.
    - The 96-byte layout, flags, bone palette, and DQB path continue to serve non-rigged and purely skeletal artifacts unchanged.

This is the cleanest fusion: the barycentric binding is an **authoring + evaluation** concern for the avatar Presence actor; the existing splat contract remains the transport and the GPU skinning path stays pure DQ for everything else.

### Do you need a new projection-layer component?

- **Not for the core projection shader** — keep it pure DQ + billboard as it is.
- **Yes, a modest new piece in the actor / Scene Memory side**:
    - Something like a `MeshBoundAvatar` or `ProxySurfaceDeformer` that holds the GLB-derived mesh, morph targets, jaw, and the loaded `.splatbind`.
    - It is responsible for turning faceanim + body pose into a list of already-deformed splat positions that then go through the normal upload path (`to_gpu_splat` → dynamic buffer).

You can later add a GPU compute variant of the reconstruction if the splat count grows large, but that is an optimization, not a requirement to make the architecture correct.

### Why this preserves your goals

- Non-rigged artifacts keep the simple, fast pure-DQ path.
- The avatar gets correct viseme + jaw motion driven by the mesh you already rigged in Blender/MPFB.
- One `AnimatedSplatGpu` buffer, one projection dispatch, one eviction policy.
- Offline splatbind stays the source of truth for surface attachment; runtime only deforms and reconstructs.
- No forced “everything must be mesh-based” conversion for ordinary artifacts.

**Bottom line:** fuse by making the mesh+barycentric path a specialized *producer* of `AnimatedSplatGpu` records, not by changing the consumer (projection layer). Spec the avatar actor’s deformation + reconstruction step; leave the existing DQ contract and `gpu_layout.rs` alone for non-rigged content. That is the smallest change that satisfies both assumptions.