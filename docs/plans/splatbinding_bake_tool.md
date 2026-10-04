```markdown
# Mesh → Splatbind Bake Tool (Python)

Offline asset tool that turns a frozen humanoid proxy mesh into a barycentric splat binding for the Presence avatar path.

## Goal

Given a topology-frozen `humanoid_proxy.glb` (A-pose, jaw joint, MPFB viseme morph targets), produce:

- `humanoid_proxy.splatbind` — packed surface samples bound to triangles via barycentric coordinates
- a topology fingerprint that the runtime can validate before use
- optional updates / notes for `avatar_manifest.json`

The runtime never re-samples the mesh. It only loads the bind file, checks the fingerprint, deforms the proxy (morph + skin), and reconstructs each splat from its triangle + barycentrics every frame.

This tool is an **asset build step**, not part of the daemon hot path.

## Inputs

| Input | Description |
|-------|-------------|
| `--glb` | Path to `humanoid_proxy.glb` (or equivalent frozen proxy) |
| `--samples` / `--density` | Target total splat count **or** samples-per-triangle / area density |
| `--seed` | RNG seed for deterministic sampling (required for repeatability) |
| `--out` | Output path for `.splatbind` (default: next to the GLB) |
| `--fingerprint-only` | Optional: compute and print fingerprint without writing samples |

Assumptions about the GLB:

- Single primary skinned mesh (or a clearly designated primitive)
- Stable triangle/index ordering after export from Blender/MPFB
- Bind-pose positions available
- Optional but useful: vertex normals, `JOINTS_0` / `WEIGHTS_0`

## Outputs

### 1. `humanoid_proxy.splatbind` (binary)

Conceptual per-splat record (exact packing may be refined, but must be documented and versioned):

```text
struct SurfaceBoundSplat {
    uint32_t triangle_index;
    uint16_t bary_u;          // fixed-point or float16; document scale
    uint16_t bary_v;          // bary_w = 1 - bary_u - bary_v
    int16_t  normal_offset;   // optional, small displacement along normal
    int16_t  tangent_offset_u;
    int16_t  tangent_offset_v;
    // Optional inherited appearance / skinning hints for later use:
    // uint32_t patch_id;
    // float    scale or covariance hints;
    // uint16_t bone_indices[4] + weights if desired
};
```

File layout recommendation:

```text
magic:     "SPLB" (4 bytes)
version:   u32
flags:     u32
count:     u32
fingerprint: fixed-size blob (see below)
records:   SurfaceBoundSplat[count]
```

Keep the format simple and versioned so the Rust loader can evolve without silent breakage.

### 2. Topology fingerprint

Must change if the bind becomes invalid. Minimum contents:

```text
vertex_count
triangle_count
hash(index_buffer)
hash(bind_pose_positions)   # optional but recommended
```

Store the fingerprint both inside the `.splatbind` header and (optionally) as a human-readable sidecar or manifest field so the daemon can reject mismatched assets.

### 3. Console / log summary

- Mesh stats (verts, tris, selected primitive)
- Sample count and density used
- Fingerprint (hex)
- Output path
- Any warnings (degenerate triangles skipped, missing normals, etc.)

## Sampling strategy (v1)

Keep it simple and deterministic.

1. Load the chosen mesh primitive in bind pose.
2. Build a list of valid triangles (skip degenerates).
3. Compute triangle areas.
4. Sample points on the surface:
    - **Preferred for v1**: area-weighted random sampling with fixed seed, or
    - Fixed number of samples per triangle (easier to reason about density).
5. For each sample:
    - Record `triangle_index`
    - Record barycentric coordinates `(u, v)` (with `w = 1 - u - v`)
    - Optionally record a small normal offset (default 0 for a flush T-1000 look)
6. Optionally inherit nearest-vertex or barycentric-blended skin weights for future residual skeletal use (not required for the first pure reconstruct path).

Do **not** attempt fancy blue-noise or curvature-adaptive sampling in v1 unless it is trivial. Correct attachment and a stable fingerprint matter more than perfect distribution.

## Processing pipeline

```text
GLB
  → load mesh + indices + bind-pose positions (+ normals, joints if present)
  → validate / select primitive
  → compute topology fingerprint
  → sample surface (seeded)
  → pack SurfaceBoundSplat records
  → write .splatbind (header + fingerprint + records)
  → print summary
```

Re-running with the same GLB + same parameters + same seed must produce the same fingerprint and the same sample set.

## Dependencies (Python)

Minimal recommended set:

- `numpy`
- `trimesh` (mesh load, area sampling, barycentrics)  
  **or** `pygltflib` + manual geometry if you want fewer transitive deps
- Standard library only for hashing, argparse, struct packing

Avoid pulling in the full scientific stack or any neural / rendering libraries.

## CLI sketch

```bash
python bake_splatbind.py \
  --glb assets/avatar/default/humanoid_proxy.glb \
  --samples 50000 \
  --seed 42 \
  --out assets/avatar/default/humanoid_proxy.splatbind
```

Useful flags:

- `--samples N` — total target count
- `--per-triangle K` — alternative density control
- `--seed S` — required for determinism
- `--normal-offset F` — default 0.0
- `--dry-run` — fingerprint + stats only
- `--dump-ply debug_samples.ply` — optional visual check of sample positions

## Acceptance criteria

- [ ] Loads the project’s frozen `humanoid_proxy.glb` without error
- [ ] Produces a `.splatbind` whose fingerprint matches a second run with identical arguments
- [ ] Changing the mesh topology (or re-export that alters indices/positions) changes the fingerprint
- [ ] Sample positions, when reconstructed with bind-pose vertices, lie on the mesh surface (visual check via `--dump-ply`)
- [ ] Degenerate / zero-area triangles are skipped cleanly
- [ ] Binary layout is documented in the script header or a short companion note
- [ ] No daemon or Vulkan code is required to run the tool

## Non-goals (v1)

- Live / runtime rebinding
- Morph-target-aware sampling (sample the bind pose only)
- Full material / texture baking into the bind file
- Hair-card special cases beyond whatever geometry is already in the GLB
- Matching the exact GPU `AnimatedSplatGpu` layout (that conversion happens later in the Presence actor)

## Relationship to the rest of the system

```text
Blender / MPFB  →  humanoid_proxy.glb
                      ↓
              bake_splatbind.py   ← this tool
                      ↓
         humanoid_proxy.splatbind + fingerprint
                      ↓
         Presence avatar actor (Rust)
           • load + validate fingerprint
           • morph + skin proxy each frame
           • reconstruct splat positions from barycentrics
           • upload as ordinary AnimatedSplatGpu records
                      ↓
         existing DQ / projection pipeline (unchanged for non-rigged artifacts)
```

Non-rigged artifacts continue to use pure rest-pose + dual-quaternion skinning. Only the mesh-bound avatar path uses this bind file.

## Implementation notes for the author (Grok-on-Omarchy)

- Prefer clarity over cleverness.
- Document the exact binary layout and fixed-point conventions in the file header comment.
- Make the seed mandatory or default to a fixed value and print it.
- Fail loudly on missing mesh data or empty triangle lists.
- Keep the script single-file if practical so it is easy to re-run and easy to read.

Once this tool exists and the first `.splatbind` is validated against the real proxy, the avatar Presence path can consume it without further geometry inventiveness.
```