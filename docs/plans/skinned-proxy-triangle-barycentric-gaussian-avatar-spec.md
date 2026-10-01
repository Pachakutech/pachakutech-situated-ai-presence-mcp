# Skinned Proxy Mesh With Triangle-Barycentric Gaussian Splat Binding

## Purpose

Implement a Vulkan-native animated Gaussian avatar in which a conventional humanoid skeleton deforms a hidden low-feature proxy mesh, and a visible cloud of Gaussian splats follows that mesh through fixed triangle-barycentric bindings.

The visible character is not intended to be photorealistic. It is a coherent humanoid silhouette whose splat patches periodically adopt static, abstracted captures from different desktop regions: camouflage-like replacement, with the unstable identity effect of _A Scanner Darkly_ inspectors and liquid/fragmentary machine surfaces. The skeletal body must remain mechanically stable while the material changes.

## Core Design

### Separation of concerns

Keep these systems separate:

| System | Responsibility | Must remain stable? |
|---|---|---|
| Skeleton and clips | Walking, pointing, head turns, jaw motion, root locomotion | Yes |
| Proxy mesh | Carries smooth local body deformation and a surface parameterization | Yes |
| Splat binding | Keeps every splat attached to one rest-pose mesh triangle | Yes |
| Gaussian geometry | Center, covariance/frame, opacity, base size | Mostly |
| Appearance patches | Select, abstract, and transition desktop capture regions | Intentionally changing |
| Tile renderer | Binning, visibility, compositing, and final Vulkan draw/compute work | Yes |

Do not let changing desktop imagery alter joint weights, triangle IDs, barycentric coordinates, or rest-space attachment data. Appearance instability should never cause an arm/leg/head attachment failure.

### Pipeline

```text
VRM/glTF-compatible humanoid skeleton + animation clip
        |
        v
Skin proxy-mesh vertices (LBS initially; DQS recommended if twists matter)
        |
        v
For each bound splat: fetch posed parent triangle and reconstruct local pose
        |
        v
Update Gaussian center and covariance orientation
        |
        v
Apply persistent appearance-patch material state
        |
        v
Tile/bin/cull/render with existing Vulkan Gaussian pipeline
```

## Definitions and Coordinate Conventions

Use one convention everywhere and encode it in asset metadata.

- **Rest pose:** A-pose.
- **World up:** +Y.
- **Avatar forward:** choose +Z or -Z once; record it and never infer it.
- **Ground plane:** `y = 0` at the soles of the feet in rest pose.
- **Root locomotion transform:** hips/root parent; it handles world-space travel and yaw.
- **Mesh local space:** proxy mesh rest space.
- **Splat rest space:** proxy mesh local/rest space.
- **Joint transforms:** use the same convention as the importer; store inverse bind matrices explicitly.
- **Units:** meters; record `meters_per_unit` in the asset header.

The recommended binding name is:

> **Triangle-barycentric Gaussian binding to a skinned proxy mesh**

Other acceptable names are **surface-bound splats**, **mesh-driven Gaussian skinning**, and **proxy-mesh-driven Gaussian deformation**.

## Input Asset Requirements

### Humanoid proxy mesh

Acquire or create an open-source mesh with the following qualities:

- Neutral A-pose, not T-pose if avoidable.
- One manifold-ish body surface suitable for triangle sampling.
- One conventional humanoid skeleton: hips, spine/chest, neck, head, upper/lower arms, hands, upper/lower legs, feet, and toes if available.
- A jaw joint is strongly preferred; facial blendshapes are optional.
- Good edge loops at shoulders, elbows, hips, knees, neck, and jaw.
- No requirement for visible detail, hair, clothing, fingers, accessories, or realistic facial topology.
- Approximately 2,000–10,000 triangles initially. Optimize only after deformation tests pass.
- Four joint influences per mesh vertex is a good interoperable baseline.

The proxy mesh is runtime-invisible. Its purpose is animation and splat attachment, not visual fidelity.

### Preferred interchange

Use glTF 2.0 / GLB as the canonical import format where possible:

- Mesh geometry and indexed triangles.
- Skeleton hierarchy and inverse bind matrices.
- `JOINTS_0` and `WEIGHTS_0` attributes.
- Rest pose transforms.
- Animation clips, if present.
- Named humanoid bone mapping.

Import FBX, BVH, or Mixamo clips offline, retarget them to the canonical skeleton, and export validated engine-ready clips. Avoid making FBX parsing a runtime dependency.

## Offline Build Pipeline

### Step 1: Validate and normalize the proxy asset

Write an offline importer/build step that:

1. Loads mesh vertices, indices, normals, skin weights, joint hierarchy, and inverse bind matrices.
2. Converts coordinate handedness and up/forward axes into the project convention.
3. Applies or records a uniform scale so the avatar is in meters.
4. Verifies rest pose is A-pose and feet are approximately on `y = 0`.
5. Normalizes skin weights per vertex.
6. Removes degenerate triangles and repairs or flags invalid normals.
7. Generates a stable triangle index order; triangle IDs must not change after splats are bound.
8. Emits debug assets: wireframe proxy, joint labels, bind-pose render, and vertex-weight heatmaps.

### Step 2: Choose splat distribution

Start with a modest count: 20,000–100,000 splats. Do not begin at production density.

Use a two-layer distribution:

- **Surface layer:** sample proxy triangles by area. This supplies the readable body silhouette.
- **Accent layer:** sparse offset splats around silhouette, shoulders, hands, face, and torso. This makes the body feel fragmentary/cloud-like rather than a mesh made of dots.

Triangle selection probability for the surface layer:

\[
P(t) = \frac{A_t}{\sum_k A_k},
\]

where \(A_t\) is rest-pose triangle area.

For a uniformly distributed sample on a selected triangle, generate \(r_1,r_2 \in [0,1]\), set:

\[
q = \sqrt{r_1},\qquad
\beta_0 = 1-q,\qquad
\beta_1 = q(1-r_2),\qquad
\beta_2 = qr_2.
\]

Store only \(\beta_1\) and \(\beta_2\); reconstruct \(\beta_0 = 1-\beta_1-\beta_2\).

### Step 3: Construct rest-frame splat attributes

For each sampled splat:

1. Store `triangle_id`.
2. Store barycentric coordinates `bary_u = beta_1`, `bary_v = beta_2`.
3. Compute rest center from the parent triangle.
4. Construct a rest tangent frame from triangle tangent/edge and normal.
5. Generate a small local offset in that frame, biased toward the outward normal.
6. Assign Gaussian scale/covariance, opacity, stable ID, random seed, and region/material label.
7. Assign stable material coordinates for future desktop-patch sampling.

Rest center without local offset:

\[
\mathbf{p}_{s,0} =
\beta_0\mathbf{v}_{0,0} +
\beta_1\mathbf{v}_{1,0} +
\beta_2\mathbf{v}_{2,0}.
\]

Store a small **rest tangent-frame offset** \(\mathbf{o}_s\), not merely a world-space offset. This lets shell/accent splats follow body rotation.

### Step 4: Assign anatomy/appearance regions

Assign every triangle and splat a coarse stable region ID. Suggested initial regions:

- head / face
- jaw / lower face
- neck
- chest / upper torso
- abdomen / pelvis
- left and right upper arm
- left and right forearm / hand
- left and right upper leg
- left and right lower leg / foot
- halo / detached accent layer

This data is not needed for skinning. It is needed for art direction: use different desktop captures, transition rates, opacity ranges, and patch scales by region.

### Step 5: Emit a native runtime asset

Use a versioned binary asset with independent blocks. Avoid tying the runtime directly to a web-specific GVRM loader.

```text
AvatarAssetHeader
  magic/version
  coordinate convention
  meters_per_unit
  counts and byte offsets

SkeletonBlock
  joint names/parent indices
  rest local transforms
  inverse bind matrices
  humanoid semantic mapping

ProxyMeshBlock
  rest positions
  normals/tangents
  triangle index buffer
  joint indices + weights per vertex
  optional triangle region IDs

SplatBlock
  rest covariance/scale/frame
  opacity and base material parameters
  triangle_id
  bary_u, bary_v
  local tangent-frame offset
  stable_id / seed
  region_id
  appearance-patch state

AnimationBlock or ClipReferences
  imported, retargeted skeletal clips
```

## Runtime Data Layout

### Recommended compact binding record

A practical initial GPU-side representation is:

```c
struct SplatBinding {
    uint32_t triangle_id;
    uint16_t bary_u_unorm;
    uint16_t bary_v_unorm;
    int16_t  offset_tangent_snorm;
    int16_t  offset_bitangent_snorm;
    int16_t  offset_normal_snorm;
    uint16_t region_id;
    uint32_t stable_id;
};
```

Use a per-asset scale for quantized local offsets. Decode:

```text
beta1 = bary_u_unorm / 65535
beta2 = bary_v_unorm / 65535
beta0 = max(0, 1 - beta1 - beta2)
local_offset = offset_scale * decoded_snorm3
```

Keep splats sorted by `triangle_id` or by mesh cluster at build time. This improves cache locality and enables a workgroup to reuse posed triangle information for many splats.

### Gaussian state

Keep the binding data immutable. Store dynamic state separately:

- posed center/frame, if materialized in a compute buffer
- temporal opacity/scale modifiers
- appearance patch assignment and cross-fade value
- optional per-splat drift phase

This separation makes it safe to rebuild visual style without rebinding the cloud.

## Runtime Deformation

### Step 1: Evaluate skeletal pose

For each frame:

1. Evaluate the active animation clip(s).
2. Blend layers: locomotion, upper-body gesture, head look, jaw, and optional additive motion.
3. Compute global joint transforms.
4. Apply root locomotion as a separate world transform.
5. Construct skinning transforms:

\[
\mathbf{M}_j = \mathbf{G}_j\mathbf{B}^{-1}_j,
\]

where \(\mathbf{G}_j\) is the current global joint transform and \(\mathbf{B}^{-1}_j\) is the inverse bind matrix.

### Step 2: Skin the proxy mesh

Initial implementation: standard linear blend skinning (LBS):

\[
\mathbf{v}'_i = \sum_{j=1}^{K}w_{ij}\,\mathbf{M}_j\mathbf{v}_{i,0}.
\]

For normal/tangent vectors, transform appropriately and normalize. For an initial visible prototype, recomputing posed triangle frames from skinned positions is sufficient.

Upgrade path: dual-quaternion skinning (DQS) for the proxy mesh. DQS reduces volume loss and candy-wrapper artifacts under twisting. The splat binding architecture remains unchanged; only mesh skinning changes.

### Step 3: Reconstruct each splat center

For the splat parent triangle `t = (i0, i1, i2)`, fetch the three posed vertices \(\mathbf{v}'_0,\mathbf{v}'_1,\mathbf{v}'_2\):

\[
\mathbf{p}'_{s,\mathrm{surface}} =
\beta_0\mathbf{v}'_0 +
\beta_1\mathbf{v}'_1 +
\beta_2\mathbf{v}'_2.
\]

Construct posed triangle axes:

\[
\mathbf{e}_1 = \operatorname{normalize}(\mathbf{v}'_1-\mathbf{v}'_0),
\]

\[
\mathbf{n}' = \operatorname{normalize}((\mathbf{v}'_1-\mathbf{v}'_0) \times (\mathbf{v}'_2-\mathbf{v}'_0)),
\]

\[
\mathbf{e}_2 = \operatorname{normalize}(\mathbf{n}' \times \mathbf{e}_1).
\]

Let \(\mathbf{F}'=[\mathbf{e}_1\ \mathbf{e}_2\ \mathbf{n}']\). The posed splat center is:

\[
\mathbf{p}'_s = \mathbf{p}'_{s,\mathrm{surface}} + \mathbf{F}'\mathbf{o}_s.
\]

Use a deterministic fallback frame for degenerate posed triangles; log/visualize their count rather than silently emitting NaNs.

### Step 4: Transport Gaussian orientation and covariance

Do not update only centers. An anisotropic Gaussian needs a posed frame/covariance.

Baseline approach:

\[
\mathbf{R}_s = \mathbf{F}'\mathbf{F}_0^T,
\qquad
\Sigma_s' = \mathbf{R}_s\Sigma_{s,0}\mathbf{R}_s^T.
\]

Here \(\mathbf{F}_0\) is the rest triangle frame recorded or reconstructed in the same way. Orthonormalize frames before using them as rotations.

Higher-quality approach for later:

1. Derive the local triangle deformation gradient from rest and posed edges.
2. Compute its polar decomposition \(\mathbf{A}=\mathbf{R}\mathbf{S}\).
3. Use \(\mathbf{R}\) for Gaussian orientation and optionally use bounded components of \(\mathbf{S}\) for controlled scale response.

Do not directly feed arbitrary shear/stretch into a Gaussian covariance until the result is visually validated. Clamp eigenvalues and preserve positive definiteness.

### Step 5: Choose compute strategy

For Vulkan, either strategy is valid:

| Strategy | Use when | Tradeoff |
|---|---|---|
| Pre-skin proxy vertices, then reconstruct splats | Many splats share triangles; easy to debug | Extra vertex buffer/pass |
| Skin required vertices within splat/cluster compute | Proxy is tiny or data is already local | More repeated bone fetches unless grouped |
| Precompute triangle frames after skinning | Many splats per triangle | Extra triangle buffer, excellent splat reuse |

Recommended initial layout:

1. Compute skin all proxy vertices.
2. Compute all posed triangle frames/areas/validity flags.
3. Dispatch splat deformation grouped by triangle or triangle cluster.
4. Feed posed Gaussian data to your existing tile/bin render pipeline.

## Animation Controls

### Locomotion

Use root transform movement and a normal skeletal walk clip.

1. Receive an explicit world-space target on a flat navigation plane.
2. Compute horizontal direction from root position to target.
3. Rotate root yaw toward target at a limited angular speed.
4. Blend idle to walk outside a stopping radius.
5. Move root via authored root motion or a speed matched to walk cadence.
6. Blend back to idle within the stopping radius.

Do not make screen-object recognition, obstacle avoidance, terrain adaptation, or foot IK version-1 requirements.

### Pointing

Implement in two stages:

- **Stage A:** play a predefined pointing/arm-raised clip while root/torso turns toward the target.
- **Stage B:** add a two-bone analytic IK solve for shoulder → elbow → wrist, then a hand aim orientation.

The target must be explicit in world space. If later pointing to desktop regions, define a calibrated world-space screen plane and map selected screen coordinates onto that plane.

### Jaw

Priority order:

1. Rotate a real jaw joint.
2. Drive an avatar expression/morph target.
3. Apply a local lower-face splat-group transform as a stylized fallback.

Expose `jaw_open` in `[0,1]`, map it to a limited local jaw rotation, and smooth it. Keep head/upper-face splats out of the jaw region so they do not visibly follow mouth opening.

## Desktop-Derived Appearance Patches

### Goal

The character should appear to be periodically repainted by static, abstracted screen captures—not live-screen video mapped across every splat.

A patch is a persistent material assignment, not a deformation assignment.

### Patch source model

At patch refresh time:

1. Copy or select a small source rectangle from the desktop capture buffer.
2. Downsample and blur it.
3. Apply abstraction: palette reduction, posterization, edge suppression, hue transform, or low-frequency frequency-domain filtering.
4. Save it as a texture atlas entry or compact image tile.
5. Assign the entry to a selected anatomical/splat patch.
6. Cross-fade from the old source to the new source over a deliberate transition interval.

Do not use the current desktop frame every render frame as the direct color source for all splats.

### Stable material coordinates

Each splat stores stable material coordinates \(\mathbf{u}_{s,0}\), derived from rest-triangle barycentrics, region mapping, or seeded procedural coordinates. It samples its assigned patch texture using those coordinates.

A controlled time variation may be:

\[
\mathbf{u}_s(t) = \operatorname{fract}(\mathbf{u}_{s,0}+\mathbf{v}_s t),
\]

with \(\mathbf{v}_s\) very small and tied to stable ID/region. The splat should not follow post-skin screen-space coordinates; that would create distracting sliding/shimmer during movement.

### Patch scheduling

Use a patch controller that updates only a few regions at a time.

Suggested behavior:

- Keep a patch static for several seconds.
- Refresh one region or a spatially coherent cluster at a time.
- Transition over 0.5–2 seconds using a noise/dissolve mask, not a hard swap.
- Keep face patches and hand patches more legible than torso/halo patches.
- Allow the halo/accent layer to update more frequently and with more recursion/noise.

This yields “identity-changing camouflage” while preserving an animateable, readable body.

### Face treatment

Reserve a stable head-local face patch or curved proxy submesh:

- Parent it to head space, not screen space.
- Use supplied 2D image crops, stylized screen captures, or palette-reduced source tiles.
- Split jaw/lower-face content from upper-face content if jaw animation must remain visible.
- Cross-fade or mask between face tiles deliberately.

Avoid driving the entire face from unrestricted live desktop content. The eyes/mouth/silhouette need a controlled visual anchor.

## Rendering and Performance Notes

Your renderer already has tile-based Vulkan infrastructure. Prioritize coherent data access over WebGPU-specific workarounds.

- Sort or group splat bindings by triangle ID and then by tile-friendly spatial cluster.
- Precompute a `triangle -> splat range` table.
- Use mesh-cluster bounds for early culling.
- Maintain separate LODs for halo/accent splats versus silhouette/face/hands.
- Consider one rigid transform per coarse body region at distance, full triangle binding at medium range, and full frame/covariance transport near camera.
- Keep patch texture updates asynchronous from skeletal animation; upload/atlas updates should not stall pose evaluation.
- Instrument memory bandwidth, triangle-frame reuse, splats per triangle, tile occupancy, and splat deformation cost separately.

## Debugging and Acceptance Tests

Build debug views before final materials:

- Render proxy mesh wireframe over splats.
- Render parent triangle ID as color.
- Render barycentric coordinates as RGB.
- Render region IDs as flat colors.
- Draw posed triangle frames for sampled triangles.
- Highlight degenerate or flipped triangles.
- Toggle desktop patches off, using flat per-region colors.
- Toggle local splat offsets off.
- Freeze animation and patch state independently.

### Required pose tests

Test the same cloud against every change:

1. Rest A-pose.
2. Idle loop.
3. Full walk cycle.
4. Arm raised approximately 90 degrees.
5. Elbow flexed approximately 120 degrees.
6. Forearm twist approaching 180 degrees.
7. Head yaw/pitch.
8. Jaw open/close.
9. Root turn while walking.
10. Pointing toward left, center, and right targets.

Pass criteria:

- No splats detach to a different body part.
- No NaNs/infinite covariance values.
- No catastrophic shoulder/elbow/knee collapse.
- No visible material swimming caused by frame-to-frame desktop capture.
- The head and hands remain readable at normal viewing distance.
- Patch transitions are artistic and intentional, not temporal aliasing.

## Milestones

### M0: Import and diagnostics

Deliverable: an A-pose proxy mesh and skeleton load correctly in the Vulkan engine, with wireframe, joints, and normalized weights visible.

### M1: Conventional animated proxy

Deliverable: idle and walk clips drive the proxy mesh; root moves to a clicked world-plane target. No splats yet.

### M2: Static triangle-bound splat cloud

Deliverable: 20k–100k procedural surface splats remain attached to the proxy through all required pose tests. Use flat colors only.

### M3: Gaussian frame transport

Deliverable: anisotropic splat orientation/covariance follows the posed triangle frame without obvious smear or flips. Add triangle-frame diagnostics.

### M4: Gesture controls

Deliverable: jaw parameter and a fixed-target pointing pose work. Start with a clip; add arm IK later if required.

### M5: Desktop-patch material system

Deliverable: splat regions receive static, abstracted desktop captures from an atlas and cross-fade selectively. Skeletal binding remains immutable.

### M6: Performance and art pass

Deliverable: grouped GPU deformation, tile-aware culling/LOD, face/hand priority, and a deliberately paced patch scheduler.

## Non-Goals for Version 1

Do not make these blockers:

- General automatic rigging of arbitrary point clouds.
- Per-splat direct DQ bone weights.
- Full-body inverse kinematics, foot placement, or obstacle avoidance.
- Live semantic detection of desktop objects.
- Physical cloth/hair simulation.
- Arbitrary facial capture or lip-sync quality.
- Perfect anatomical realism.
- Full screen recursion as the main material source.

## Upgrade Paths

### Improve twist quality

Replace LBS of the proxy mesh with DQS. The triangle-barycentric splat binding remains exactly the same.

### Improve local frames

Use deformation gradients and polar decomposition rather than a pure posed triangle frame. This is useful for anisotropic Gaussians and high-curvature regions.

### Improve surface continuity

Increase proxy topology where it matters—shoulders, elbows, jaw, hands—rather than globally. Bindings remain valid only if triangle IDs/topology remain unchanged; regenerate bindings after proxy remeshing.

### Add cage/volume binding

For detached halo splats, hair-like effects, or splats noticeably off the surface, use a body-region transform or a volumetric/cage embedding rather than forcing large normal offsets from a surface triangle.

### Add direct DQ splats selectively

If a few high-value regions fail under proxy attachment, use direct DQ-bound splats only there. Keep the proxy-mesh binding as the default for the general body.

## Implementation Checklist

- [ ] Select A-pose humanoid proxy asset
- [ ] Normalize asset coordinates, scale, skeleton mapping, and bind pose
- [ ] Import/retarget idle and walk clips
- [ ] Implement GPU LBS for proxy mesh
- [ ] Build surface sampler and stable triangle-barycentric binding generator
- [ ] Serialize binding, Gaussian, region, and material-coordinate data
- [ ] Precompute triangle-to-splat ranges
- [ ] Reconstruct posed splat centers and frames on GPU
- [ ] Update covariance/orientation safely
- [ ] Add wireframe, IDs, frames, and degeneracy diagnostics
- [ ] Implement root locomotion and jaw control
- [ ] Implement pointing clip, then optional two-bone IK
- [ ] Build desktop capture abstraction and texture-atlas upload path
- [ ] Assign slowly updating material patches by anatomical region
- [ ] Validate all pose tests before performance optimization
- [ ] Add tile-aware grouping, culling, and LOD

## Final Principle

The skeleton and proxy mesh define the avatar’s **body identity**. Triangle-barycentric bindings make every Gaussian inherit that identity predictably. Desktop-derived patch textures define the avatar’s **surface identity**, which can evolve, dissolve, and be replaced over time.

Keep those two identities separate. Stable mechanics make the changing appearance read as intentional art direction rather than a rendering failure.
