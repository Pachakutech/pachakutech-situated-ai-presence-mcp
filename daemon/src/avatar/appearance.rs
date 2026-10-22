//! Soft body regions and the scheduler that points each one at a rectangle
//! of one ingress picture. This module never sees pixels: a rectangle is
//! four integers plus which source it came from, and the GPU copies that
//! rectangle into an atlas tile.
//!
//! Atlas layout (must match `screen_patch.comp` and `splat_disc.frag`):
//! `MAX_REGIONS` columns by 2 rows of `TILE`×`TILE` pixels.

use super::deform::ProxyMesh;
use super::rig::Rig;
use super::splatbind::SplatBinding;

pub const TILE: u32 = 64;
pub const MAX_REGIONS: usize = 16;
pub const ATLAS_W: u32 = TILE * MAX_REGIONS as u32;
pub const ATLAS_H: u32 = TILE * 2;
pub const MAX_PATCH_SPLATS: usize = 50_000;
pub const FADE_SECS: f32 = 0.45;
/// Gap before the next region. One region fades at a time, so the gap stays
/// just above `FADE_SECS`. After its first snapshot, the head waits in the
/// same lottery as the body.
pub const WAIT_MIN: f32 = 0.48;
pub const WAIT_SPAN: f32 = 0.10;
pub const CHROME: [f32; 3] = [0.78, 0.81, 0.84];
/// How far a face sample moves toward chrome grey. The captured desktop
/// stays in front: 0.22 is a cool tint, not a grey wash.
pub const CHROME_MIX: f32 = 0.22;
/// Webcam rectangle scale on the body region. One covers the region with the
/// stored rectangle, at its own aspect. The atlas copy stays a uniform fit.
pub const WEBCAM_PLACE_SCALE: f32 = 1.0;
/// Webcam grab, inclusive, in camera pixels. Triple the desktop 32..=128
/// range. 384 still fits in the 640×480 frame, so the rectangle never
/// becomes the whole picture. 512 would be taller than 480.
pub const WEBCAM_SAMPLE_MIN: u32 = 96;
pub const WEBCAM_SAMPLE_MAX: u32 = 384;

pub const FLAG_FACE: u32 = 1;
pub const FLAG_SHOWN: u32 = 2;
pub const FLAG_INCOMING: u32 = 4;

/// Desktop capture. A later picture source takes the next id.
pub const SOURCE_DESKTOP: u32 = 0;
/// Webcam, YUYV on the GPU. Absent from the scheduler until a frame is current.
pub const SOURCE_WEBCAM: u32 = 1;

/// One picture the scheduler can copy a rectangle from. The rectangle is
/// in this source's pixels. A source with no frame is left out of the list,
/// so one live source gets every sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngressSource {
    pub id: u32,
    pub width: u32,
    pub height: u32,
}

impl IngressSource {
    pub fn desktop(width: u32, height: u32) -> Self {
        Self { id: SOURCE_DESKTOP, width, height }
    }

    pub fn webcam(width: u32, height: u32) -> Self {
        Self { id: SOURCE_WEBCAM, width, height }
    }
}

const BODY_BINS: usize = 11;

/// std430, 48 bytes. `uvec2` aligns to 8, so the struct is padded to 48
/// and declared `align(8)` — a 40-byte Rust struct would stride at 40
/// while GLSL would stride at 48.
///
/// `shown_content` / `incoming_content` are the texels of that page that
/// hold a webcam rectangle, anchored at the tile origin. `[0, 0]` means
/// the page is a desktop fill: the disc shader stretches the whole tile.
/// A webcam page is placed once, at the rectangle's own aspect, and cropped
/// to the region. It is not repeated.
/// `bbox_aspect` is the region's rest-pose width divided by its height.
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug)]
pub struct RegionGpu {
    pub fade: f32,
    pub shown_page: u32,
    pub incoming_page: u32,
    pub flags: u32,
    pub shown_content: [u32; 2],
    pub incoming_content: [u32; 2],
    pub bbox_aspect: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

impl RegionGpu {
    const fn zero() -> Self {
        Self {
            fade: 0.0,
            shown_page: 0,
            incoming_page: 0,
            flags: 0,
            shown_content: [0, 0],
            incoming_content: [0, 0],
            bbox_aspect: 1.0,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }
}

/// std430, 16 bytes. One per avatar splat, parallel to the upload order.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SplatPatchGpu {
    pub region: u32,
    pub u: f32,
    pub v: f32,
    pub _pad: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct PatchJob {
    pub region: u32,
    pub page: u32,
    pub rect: Rect,
    /// `SOURCE_DESKTOP` or `SOURCE_WEBCAM`. The rect is in that picture.
    pub source: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RegionState {
    pub is_face: bool,
    pub area: f32,
    pub fade: f32,
    pub shown_page: u32,
    pub incoming_page: u32,
    pub shown_valid: bool,
    pub incoming_valid: bool,
    /// Texels of the shown page that hold a webcam rectangle. `[0, 0]` is a desktop fill.
    pub shown_content: [u32; 2],
    /// Texels of the incoming page. `[0, 0]` is a desktop fill.
    pub incoming_content: [u32; 2],
    /// Rest-pose bbox width / height. The disc shader uses this so a webcam
    /// rectangle keeps square pixels on a non-square region.
    pub bbox_aspect: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SplatPatch {
    pub region: u32,
    pub u: f32,
    pub v: f32,
}

pub struct Appearance {
    pub regions: Vec<RegionState>,
    pub patches: Vec<SplatPatch>,
    rng: u32,
    wait: f32,
    pub show_bake: bool,
}

impl RegionState {
    fn fresh(is_face: bool, area: f32) -> Self {
        Self {
            is_face,
            area,
            fade: 1.0,
            shown_page: 0,
            incoming_page: 0,
            shown_valid: false,
            incoming_valid: false,
            shown_content: [0, 0],
            incoming_content: [0, 0],
            bbox_aspect: 1.0,
        }
    }
}

impl Appearance {
    pub fn from_mesh(mesh: &ProxyMesh, rig: &Rig, binding: &SplatBinding) -> Self {
        let njoints = rig.names.len();
        let tri_count = mesh.indices.len() / 3;
        let mut dominant = vec![0u16; tri_count];
        let mut face_tri = vec![false; tri_count];
        let mut joint_area = vec![0f32; njoints];
        let mut face_area = 0.0f32;
        for t in 0..tri_count {
            let joint = dominant_joint(mesh, t);
            dominant[t] = joint;
            let area = triangle_area(mesh, t);
            let face = (joint as usize) < njoints && is_face_joint(&rig.names, &rig.parent, joint as usize);
            face_tri[t] = face;
            if face {
                face_area += area;
            } else if (joint as usize) < njoints {
                joint_area[joint as usize] += area;
            }
        }

        let mut regions = Vec::new();
        let face_region = if face_area > 0.0 {
            regions.push(RegionState::fresh(true, face_area));
            Some(0u16)
        } else {
            None
        };

        let mut bin_area = [0.0f32; BODY_BINS];
        let mut joint_bin: Vec<Option<usize>> = vec![None; njoints];
        let mut order: Vec<usize> = (0..njoints).filter(|&j| joint_area[j] > 0.0).collect();
        order.sort_by(|&a, &b| joint_area[b].total_cmp(&joint_area[a]).then(a.cmp(&b)));
        // breast.L and breast.R hang on spine02. That spine is the surface
        // between them, so the three share one tile instead of three pictures.
        let mut breasts = Vec::new();
        for joint in order {
            if is_breast(&rig.names[joint]) {
                breasts.push(joint);
                continue;
            }
            let bin = (0..BODY_BINS)
                .min_by(|&a, &b| bin_area[a].total_cmp(&bin_area[b]).then(a.cmp(&b)))
                .unwrap();
            bin_area[bin] += joint_area[joint];
            joint_bin[joint] = Some(bin);
        }
        let chest_bin = (0..njoints).find_map(|j| {
            if is_chest_span(&rig.names[j]) {
                joint_bin[j]
            } else {
                None
            }
        });
        for joint in breasts {
            let bin = chest_bin.unwrap_or_else(|| {
                (0..BODY_BINS)
                    .min_by(|&a, &b| bin_area[a].total_cmp(&bin_area[b]).then(a.cmp(&b)))
                    .unwrap()
            });
            bin_area[bin] += joint_area[joint];
            joint_bin[joint] = Some(bin);
        }
        let mut bin_region: [Option<u16>; BODY_BINS] = [None; BODY_BINS];
        for bin in 0..BODY_BINS {
            if bin_area[bin] > 0.0 && regions.len() < MAX_REGIONS {
                let id = regions.len() as u16;
                bin_region[bin] = Some(id);
                regions.push(RegionState::fresh(false, bin_area[bin]));
            }
        }
        if regions.is_empty() {
            regions.push(RegionState::fresh(false, 0.0));
        }

        let mut tri_region = vec![0u16; tri_count];
        for t in 0..tri_count {
            tri_region[t] = if face_tri[t] {
                face_region.unwrap_or(0)
            } else {
                let joint = dominant[t] as usize;
                joint_bin
                    .get(joint)
                    .and_then(|bin| bin.and_then(|b| bin_region[b]))
                    .unwrap_or(0)
            };
        }

        let mut min_xy = vec![[f32::MAX; 2]; regions.len()];
        let mut max_xy = vec![[f32::MIN; 2]; regions.len()];
        let mut points = Vec::with_capacity(binding.records.len());
        for record in &binding.records {
            let tri = record.tri as usize;
            let region = if tri < tri_region.len() { tri_region[tri] as usize } else { 0 };
            let region = region.min(regions.len().saturating_sub(1));
            let p = rest_point(mesh, record.tri, record.b1, record.b2);
            let xy = [p[0], p[1]];
            points.push((region, xy));
            min_xy[region][0] = min_xy[region][0].min(xy[0]);
            min_xy[region][1] = min_xy[region][1].min(xy[1]);
            max_xy[region][0] = max_xy[region][0].max(xy[0]);
            max_xy[region][1] = max_xy[region][1].max(xy[1]);
        }
        for (i, region) in regions.iter_mut().enumerate() {
            if min_xy[i][0] <= max_xy[i][0] && min_xy[i][1] <= max_xy[i][1] {
                let dx = (max_xy[i][0] - min_xy[i][0]).max(1e-5);
                let dy = (max_xy[i][1] - min_xy[i][1]).max(1e-5);
                region.bbox_aspect = dx / dy;
            }
        }
        let patches = points
            .into_iter()
            .map(|(region, xy)| {
                let dx = max_xy[region][0] - min_xy[region][0];
                let dy = max_xy[region][1] - min_xy[region][1];
                let u = if dx < 1e-5 { 0.5 } else { ((xy[0] - min_xy[region][0]) / dx).clamp(0.0, 1.0) };
                let v = if dy < 1e-5 { 0.5 } else { ((xy[1] - min_xy[region][1]) / dy).clamp(0.0, 1.0) };
                SplatPatch { region: region as u32, u, v }
            })
            .collect();

        let face_at = regions.iter().position(|r| r.is_face);
        println!(
            "[appearance] {} screen-capture regions (face region {})",
            regions.len(),
            face_at.map(|i| i.to_string()).unwrap_or_else(|| "none".into())
        );
        Self {
            regions,
            patches,
            rng: 0xA341_316C,
            wait: 0.0,
            show_bake: bake_from_env(),
        }
    }

    /// `count` regions, `face_index` marked as the face (ignored if out of range).
    pub fn new_regions(count: usize, face_index: usize, seed: u32) -> Self {
        let count = count.clamp(1, MAX_REGIONS);
        let regions = (0..count).map(|i| RegionState::fresh(i == face_index, 1.0)).collect();
        Self { regions, patches: Vec::new(), rng: seed, wait: 0.0, show_bake: false }
    }

    /// Advance fades. When at least one source has a frame and the timer
    /// has elapsed, queue one rectangle for one region. The source is an
    /// even draw over the live sources. No source: the timer does not move.
    pub fn tick(&mut self, dt: f32, sources: &[IngressSource]) -> Option<PatchJob> {
        let dt = dt.max(0.0);
        for region in &mut self.regions {
            if region.incoming_valid && region.fade < 1.0 {
                region.fade = (region.fade + dt / FADE_SECS).min(1.0);
                if region.fade >= 1.0 {
                    region.shown_page = region.incoming_page;
                    region.shown_content = region.incoming_content;
                    region.shown_valid = true;
                }
            }
        }
        if self.show_bake || self.regions.is_empty() {
            return None;
        }
        let available: Vec<IngressSource> = sources
            .iter()
            .copied()
            .filter(|s| s.width > 0 && s.height > 0)
            .collect();
        if available.is_empty() {
            return None;
        }
        if self.wait > 0.0 {
            self.wait = (self.wait - dt).max(0.0);
            if self.wait > 0.0 {
                return None;
            }
        }
        let eligible: Vec<usize> = self
            .regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.fade >= 1.0 || !r.incoming_valid)
            .map(|(i, _)| i)
            .collect();
        if eligible.is_empty() {
            return None;
        }
        // The head takes the opening sample so it does not sit on the debug
        // palette while the body starts. After that it is one region among the rest.
        let pick = eligible
            .iter()
            .copied()
            .find(|&i| self.regions[i].is_face && !self.regions[i].shown_valid)
            .unwrap_or_else(|| eligible[self.next_u32() as usize % eligible.len()]);
        let source = available[self.next_u32() as usize % available.len()];
        let rect = if source.id == SOURCE_WEBCAM {
            random_webcam_rect(&mut self.rng, source.width, source.height)
        } else {
            random_screen_rect(&mut self.rng, source.width, source.height)
        };
        let region = &mut self.regions[pick];
        let page = if region.shown_valid { 1 - (region.shown_page & 1) } else { 0 };
        region.incoming_page = page;
        region.incoming_content = if source.id == SOURCE_WEBCAM {
            webcam_content_texels(rect.w, rect.h)
        } else {
            [0, 0]
        };
        region.incoming_valid = true;
        region.fade = 0.0;
        let jitter = self.next_u32() as f32 / u32::MAX as f32;
        self.wait = WAIT_MIN + jitter * WAIT_SPAN;
        Some(PatchJob { region: pick as u32, page, rect, source: source.id })
    }

    /// The copy for this page never landed (the camera was released first).
    /// Keep the region on the tile it already shows.
    pub fn revert_incoming(&mut self, region: u32, page: u32) {
        let Some(region) = self.regions.get_mut(region as usize) else { return };
        if region.incoming_valid && region.incoming_page == page && region.fade < 1.0 {
            region.incoming_valid = false;
            region.incoming_content = [0, 0];
            region.fade = 1.0;
        }
    }

    pub fn region_uniforms(&self) -> [RegionGpu; MAX_REGIONS] {
        let mut out = [RegionGpu::zero(); MAX_REGIONS];
        for (i, region) in self.regions.iter().enumerate().take(MAX_REGIONS) {
            let mut flags = 0;
            if region.is_face {
                flags |= FLAG_FACE;
            }
            if region.shown_valid {
                flags |= FLAG_SHOWN;
            }
            if region.incoming_valid {
                flags |= FLAG_INCOMING;
            }
            out[i] = RegionGpu {
                fade: region.fade,
                shown_page: region.shown_page,
                incoming_page: region.incoming_page,
                flags,
                shown_content: region.shown_content,
                incoming_content: region.incoming_content,
                bbox_aspect: region.bbox_aspect,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
            };
        }
        out
    }

    pub fn patch_gpu(&self) -> Vec<SplatPatchGpu> {
        self.patches
            .iter()
            .take(MAX_PATCH_SPLATS)
            .map(|p| SplatPatchGpu { region: p.region, u: p.u, v: p.v, _pad: 0.0 })
            .collect()
    }

    fn next_u32(&mut self) -> u32 {
        xorshift(&mut self.rng)
    }
}

/// Shader mix of a sampled color toward chrome grey. The live path does
/// this in `splat_disc.frag` with the same constants; this is the tested copy.
pub fn blend_with_chrome(color: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| color[i] * (1.0 - CHROME_MIX) + CHROME[i] * CHROME_MIX)
}

/// Texels a webcam rectangle occupies in a tile. Uniform fit, never an
/// upscale: a 40×30 rect stays 40×30, and an 80×40 rect becomes 64×32.
/// The disc shader reads only this rectangle and places it once.
pub fn webcam_content_texels(rect_w: u32, rect_h: u32) -> [u32; 2] {
    let w = rect_w.max(1) as f32;
    let h = rect_h.max(1) as f32;
    let fit = (TILE as f32 / w).min(TILE as f32 / h).min(1.0);
    [
        (w * fit).round().clamp(1.0, TILE as f32) as u32,
        (h * fit).round().clamp(1.0, TILE as f32) as u32,
    ]
}

/// 0..1 point inside a webcam tile's content for one region UV.
/// One placement, uniform scale, centered, then scaled by
/// `WEBCAM_PLACE_SCALE`. The camera's first row is the top of the frame and
/// region `v` grows with mesh +Y, which is up on screen, so `v` is flipped
/// here — one subtract, at the same moment the disc is shaded. The part of
/// the rectangle that does not fit is cropped. Lens UVs outside 0..1 clamp,
/// so the picture does not repeat. Matches `sample_tile` in `splat_disc.frag`.
pub fn webcam_cover_local(uv: [f32; 2], bbox_aspect: f32, content_w: u32, content_h: u32) -> [f32; 2] {
    let region_aspect = bbox_aspect.max(1e-4);
    let rect_aspect = content_w.max(1) as f32 / content_h.max(1) as f32;
    let stamp_h = WEBCAM_PLACE_SCALE * 1.0f32.max(region_aspect / rect_aspect);
    let stamp_w = stamp_h * rect_aspect;
    let origin_x = (region_aspect - stamp_w) * 0.5;
    let origin_y = (1.0 - stamp_h) * 0.5;
    let upright_y = 1.0 - uv[1];
    let lx = ((uv[0] * region_aspect - origin_x) / stamp_w).clamp(0.0, 1.0);
    let ly = ((upright_y - origin_y) / stamp_h).clamp(0.0, 1.0);
    [lx, ly]
}

/// Origin and side of the centered square: the shorter side of the frame,
/// applied to the longer side, sitting in the middle. A 1920×1080 screen
/// is `(420, 0, 1080)`.
pub fn center_square(width: u32, height: u32) -> (u32, u32, u32) {
    let side = width.min(height).max(1);
    let x = width.saturating_sub(side) / 2;
    let y = height.saturating_sub(side) / 2;
    (x, y, side)
}

/// Desktop samples stay inside [`center_square`]. The webcam still uses its whole frame.
pub fn random_screen_rect(rng: &mut u32, width: u32, height: u32) -> Rect {
    let (x0, y0, side) = center_square(width, height);
    let w = span(rng, side);
    let h = span(rng, side);
    let inner = place_rect(rng, side, side, w, h);
    Rect { x: x0 + inner.x, y: y0 + inner.y, w: inner.w, h: inner.h }
}

fn random_webcam_rect(rng: &mut u32, width: u32, height: u32) -> Rect {
    let w = webcam_span(rng, width);
    let h = webcam_span(rng, height);
    place_rect(rng, width, height, w, h)
}

fn place_rect(rng: &mut u32, width: u32, height: u32, w: u32, h: u32) -> Rect {
    let x = if width > w { xorshift(rng) % (width - w + 1) } else { 0 };
    let y = if height > h { xorshift(rng) % (height - h + 1) } else { 0 };
    Rect { x, y, w, h }
}

fn span(rng: &mut u32, extent: u32) -> u32 {
    sized_span(rng, extent, 32, 128)
}

fn webcam_span(rng: &mut u32, extent: u32) -> u32 {
    sized_span(rng, extent, WEBCAM_SAMPLE_MIN, WEBCAM_SAMPLE_MAX)
}

fn sized_span(rng: &mut u32, extent: u32, min: u32, max: u32) -> u32 {
    let choices = max - min + 1;
    let want = min + (xorshift(rng) % choices);
    let extent = extent.max(1);
    if want > extent { extent } else { want }
}

fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    if x == 0 {
        x = 0xA341_316C;
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn bake_from_env() -> bool {
    matches!(std::env::var("PRESENCE_APPEARANCE_BAKE").as_deref(), Ok("1") | Ok("true") | Ok("yes"))
}

/// `PRESENCE_DESKTOP=0` leaves the desktop out of the source list. A webcam
/// frame then supplies every sample. Unset keeps the desktop in the draw.
pub fn desktop_ingress_enabled() -> bool {
    !matches!(
        std::env::var("PRESENCE_DESKTOP").as_deref(),
        Ok("0") | Ok("false") | Ok("no") | Ok("off")
    )
}

fn face_name(name: &str) -> bool {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == "head" || token == "jaw")
}

fn name_token(name: &str, want: &str) -> bool {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == want)
}

/// `breast.L` / `breast.R`. They share the chest tile.
fn is_breast(name: &str) -> bool {
    name_token(name, "breast")
}

/// The spine the breasts parent to, and the surface between them.
fn is_chest_span(name: &str) -> bool {
    name_token(name, "spine02")
}

fn is_face_joint(names: &[String], parent: &[Option<usize>], joint: usize) -> bool {
    let mut i = joint;
    for _ in 0..=names.len() {
        if i >= names.len() {
            return false;
        }
        if face_name(&names[i]) {
            return true;
        }
        match parent.get(i).copied().flatten() {
            Some(p) => i = p,
            None => return false,
        }
    }
    false
}

/// Summed joint weight across the triangle's three vertices. Ties keep the
/// smaller joint id, same rule as the debug palette.
fn dominant_joint(mesh: &ProxyMesh, tri: usize) -> u16 {
    let mut acc: Vec<(u16, f32)> = Vec::new();
    for k in 0..3 {
        let vi = mesh.indices[3 * tri + k] as usize;
        if vi >= mesh.joints.len() {
            continue;
        }
        for c in 0..4 {
            let joint = mesh.joints[vi][c];
            let weight = mesh.weights[vi][c];
            if let Some(slot) = acc.iter_mut().find(|(j, _)| *j == joint) {
                slot.1 += weight;
            } else {
                acc.push((joint, weight));
            }
        }
    }
    acc.into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(joint, _)| joint)
        .unwrap_or(0)
}

fn triangle_area(mesh: &ProxyMesh, tri: usize) -> f32 {
    let v = |k: usize| {
        let i = mesh.indices[3 * tri + k] as usize;
        mesh.rest.get(i).copied().unwrap_or([0.0; 3])
    };
    let a = v(0);
    let b = v(1);
    let c = v(2);
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
}

fn rest_point(mesh: &ProxyMesh, tri: u32, b1: f32, b2: f32) -> [f32; 3] {
    let t = tri as usize;
    if 3 * t + 2 >= mesh.indices.len() {
        return [0.0; 3];
    }
    let at = |k: usize| {
        let i = mesh.indices[3 * t + k] as usize;
        mesh.rest.get(i).copied().unwrap_or([0.0; 3])
    };
    let b0 = (1.0 - b1 - b2).max(0.0);
    let (p0, p1, p2) = (at(0), at(1), at(2));
    [
        b0 * p0[0] + b1 * p1[0] + b2 * p2[0],
        b0 * p0[1] + b1 * p1[1] + b2 * p2[1],
        b0 * p0[2] + b1 * p1[2] + b2 * p2[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transitioning(a: &Appearance) -> usize {
        a.regions.iter().filter(|r| r.incoming_valid && r.fade < 1.0).count()
    }

    #[test]
    fn gpu_structs_match_std430() {
        assert_eq!(std::mem::size_of::<RegionGpu>(), 48);
        assert_eq!(std::mem::align_of::<RegionGpu>(), 8);
        let sample = RegionGpu {
            shown_content: [1, 2],
            incoming_content: [3, 4],
            bbox_aspect: 1.5,
            ..RegionGpu::zero()
        };
        let base = &sample as *const RegionGpu as usize;
        assert_eq!(&sample.shown_content as *const [u32; 2] as usize - base, 16);
        assert_eq!(&sample.incoming_content as *const [u32; 2] as usize - base, 24);
        assert_eq!(&sample.bbox_aspect as *const f32 as usize - base, 32);
        assert_eq!(std::mem::size_of::<SplatPatchGpu>(), 16);
        assert_eq!(ATLAS_W, 1024);
        assert_eq!(ATLAS_H, 128);
    }

    #[test]
    fn webcam_content_fits_without_upscale_or_stretch() {
        assert_eq!(webcam_content_texels(80, 40), [64, 32]);
        assert_eq!(webcam_content_texels(40, 30), [40, 30]);
        assert_eq!(webcam_content_texels(100, 100), [64, 64]);
        let wide = webcam_content_texels(80, 40);
        let aspect = wide[0] as f32 / wide[1] as f32;
        assert!((aspect - 2.0).abs() < 0.02, "{aspect}");
    }

    #[test]
    fn webcam_placement_is_one_patch_and_does_not_repeat() {
        // A matching aspect at scale 1 covers the region with the whole rectangle.
        // v = 0 is the feet, so it reads a lower camera row than v = 1.
        assert_eq!(webcam_cover_local([0.0, 0.0], 2.0, 64, 32), [0.0, 1.0]);
        assert_eq!(webcam_cover_local([1.0, 1.0], 2.0, 64, 32), [1.0, 0.0]);
        assert_eq!(webcam_cover_local([0.5, 0.5], 1.0, 40, 40), [0.5, 0.5]);
        let feet = webcam_cover_local([0.5, 0.0], 1.0, 64, 64);
        let head = webcam_cover_local([0.5, 1.0], 1.0, 64, 64);
        assert!(feet[1] > head[1], "camera up follows the figure: feet {feet:?} head {head:?}");
        // A square stamp on a region twice as wide is cropped, not stretched or tiled.
        let low = webcam_cover_local([0.0, 0.0], 2.0, 64, 64);
        let high = webcam_cover_local([1.0, 1.0], 2.0, 64, 64);
        assert!((low[0] - 0.0).abs() < 1e-5 && (low[1] - 0.75).abs() < 1e-5, "{low:?}");
        assert!((high[0] - 1.0).abs() < 1e-5 && (high[1] - 0.25).abs() < 1e-5, "{high:?}");
        // Past the region holds the edge. It does not wrap.
        assert_eq!(webcam_cover_local([-0.25, 1.25], 1.0, 64, 64), [0.0, 0.0]);
        assert_eq!(webcam_cover_local([-2.0, -2.0], 1.0, 64, 64), [0.0, 1.0]);
    }

    #[test]
    fn webcam_tick_stores_content_and_desktop_tick_does_not() {
        let mut cam = Appearance::new_regions(1, 0, 5);
        let job = cam.tick(0.0, &[IngressSource::webcam(640, 480)]).unwrap();
        let content = cam.regions[0].incoming_content;
        assert!(content[0] > 0 && content[1] > 0 && content[0] <= TILE && content[1] <= TILE);
        let rect_aspect = job.rect.w as f32 / job.rect.h as f32;
        let content_aspect = content[0] as f32 / content[1] as f32;
        assert!((content_aspect - rect_aspect).abs() / rect_aspect < 0.15, "{content:?} vs {}", job.rect.w);
        assert_eq!(cam.region_uniforms()[0].incoming_content, content);
        cam.tick(FADE_SECS, &[IngressSource::webcam(640, 480)]);
        assert_eq!(cam.regions[0].shown_content, content);

        let mut desk = Appearance::new_regions(1, 0, 5);
        let _ = desk.tick(0.0, &[IngressSource::desktop(1920, 1080)]).unwrap();
        assert_eq!(desk.regions[0].incoming_content, [0, 0]);
        assert_eq!(desk.region_uniforms()[0].bbox_aspect, 1.0);
    }

    #[test]
    fn chrome_tint_leaves_most_of_the_sample() {
        let out = blend_with_chrome([1.0, 0.0, 0.0]);
        let keep = 1.0 - CHROME_MIX;
        assert!((out[0] - (keep + CHROME_MIX * CHROME[0])).abs() < 1e-5, "{out:?}");
        assert!((out[1] - CHROME_MIX * CHROME[1]).abs() < 1e-5, "{out:?}");
        assert!((out[2] - CHROME_MIX * CHROME[2]).abs() < 1e-5, "{out:?}");
        assert!(out[0] > 0.9, "the face should stay mostly the captured sample");
    }

    #[test]
    fn desktop_samples_come_from_the_center_square() {
        assert_eq!(center_square(1920, 1080), (420, 0, 1080));
        assert_eq!(center_square(1080, 1920), (0, 420, 1080));
        let mut rng = 1u32;
        for _ in 0..40 {
            let rect = random_screen_rect(&mut rng, 1920, 1080);
            assert!(rect.x >= 420 && rect.x + rect.w <= 1500, "{rect:?}");
            assert!(rect.y + rect.h <= 1080, "{rect:?}");
            assert!((32..=128).contains(&rect.w) && (32..=128).contains(&rect.h), "{rect:?}");
        }
    }

    #[test]
    fn rects_stay_inside_and_repeat_for_a_seed() {
        let mut a = Appearance::new_regions(1, 0, 42);
        let mut b = Appearance::new_regions(1, 0, 42);
        let ja = a.tick(0.0, &[IngressSource::desktop(800, 600)]).unwrap();
        let jb = b.tick(0.0, &[IngressSource::desktop(800, 600)]).unwrap();
        assert_eq!(ja.rect, jb.rect);
        assert!((WAIT_MIN..=WAIT_MIN + WAIT_SPAN).contains(&a.wait), "wait {}", a.wait);

        for _ in 0..30 {
            a.wait = 0.0;
            for region in &mut a.regions {
                region.fade = 1.0;
            }
            let job = a.tick(0.0, &[IngressSource::desktop(100, 80)]).unwrap();
            let (x0, y0, side) = center_square(100, 80);
            assert!(job.rect.x >= x0 && job.rect.y >= y0, "{:?}", job.rect);
            assert!(job.rect.x + job.rect.w <= x0 + side, "{:?}", job.rect);
            assert!(job.rect.y + job.rect.h <= y0 + side, "{:?}", job.rect);
            assert!((32..=side).contains(&job.rect.w), "{:?}", job.rect);
            assert!((32..=side).contains(&job.rect.h), "{:?}", job.rect);
            assert_eq!(transitioning(&a), 1);
        }
    }

    #[test]
    fn one_region_per_tick_and_the_timer_waits_for_a_screen() {
        let mut a = Appearance::new_regions(4, 0, 7);
        assert!(a.tick(0.0, &[]).is_none(), "no capture, no snapshot");
        assert_eq!(a.wait, 0.0, "the timer does not run without a screen");
        let first = a.tick(0.0, &[IngressSource::desktop(1920, 1080)]).unwrap();
        assert_eq!(transitioning(&a), 1);
        assert!(a.tick(0.05, &[IngressSource::desktop(1920, 1080)]).is_none());
        assert_eq!(transitioning(&a), 1);
        assert!((0.0..1.0).contains(&a.regions[first.region as usize].fade));
        a.tick(0.40, &[IngressSource::desktop(1920, 1080)]);
        assert!(a.regions[first.region as usize].shown_valid);
        assert_eq!(transitioning(&a), 0);
        assert!(a.tick(0.02, &[IngressSource::desktop(1920, 1080)]).is_none(), "still inside the wait");
        let second = a.tick(WAIT_MIN + WAIT_SPAN, &[IngressSource::desktop(1920, 1080)]).unwrap();
        assert_eq!(transitioning(&a), 1);
        let _ = second;
        let mut live = 0;
        a.tick(0.0, &[IngressSource::desktop(1920, 1080)]);
        for region in &a.regions {
            if region.incoming_valid && region.fade < 1.0 {
                live += 1;
            }
        }
        assert_eq!(live, 1);
    }

    #[test]
    fn bake_mode_queues_nothing() {
        let mut a = Appearance::new_regions(3, 1, 1);
        a.show_bake = true;
        assert!(a.tick(1.0, &[IngressSource::desktop(640, 480)]).is_none());
        assert!(a.regions.iter().all(|r| !r.incoming_valid));
    }

    #[test]
    fn face_takes_the_first_sample_then_waits_with_the_body() {
        let mut a = Appearance::new_regions(8, 4, 3);
        let first = a.tick(0.0, &[IngressSource::desktop(1920, 1080)]).unwrap();
        assert_eq!(first.region, 4, "the head is the first snapshot");

        let mut face = 0;
        let mut body = 0;
        for _ in 0..80 {
            a.wait = 0.0;
            for region in &mut a.regions {
                region.fade = 1.0;
                region.shown_valid = true;
            }
            let job = a.tick(0.0, &[IngressSource::desktop(1920, 1080)]).unwrap();
            if job.region == 4 { face += 1; } else { body += 1; }
        }
        assert!(face > 0 && body > 0, "face {face} body {body}");
        assert!(face * 3 < body * 2, "the face took too many turns: face {face} body {body}");
    }

    #[test]
    fn face_flag_reaches_the_uniform() {
        let a = Appearance::new_regions(3, 1, 1);
        let u = a.region_uniforms();
        assert_eq!(u[1].flags & FLAG_FACE, FLAG_FACE);
        assert_eq!(u[0].flags & FLAG_FACE, 0);
    }

    #[test]
    fn samples_are_split_evenly_across_live_sources() {
        let desktop = IngressSource::desktop(1920, 1080);
        let webcam = IngressSource::webcam(640, 480);

        let mut none = Appearance::new_regions(2, 0, 1);
        assert!(none.tick(0.0, &[IngressSource::desktop(0, 10), IngressSource::webcam(0, 0)]).is_none());
        assert_eq!(none.wait, 0.0, "an empty source list does not start the timer");

        let mut only_desk = Appearance::new_regions(4, 0, 9);
        for _ in 0..20 {
            only_desk.wait = 0.0;
            for region in &mut only_desk.regions {
                region.fade = 1.0;
            }
            let job = only_desk.tick(0.0, &[desktop]).unwrap();
            assert_eq!(job.source, SOURCE_DESKTOP);
            let (x0, y0, side) = center_square(desktop.width, desktop.height);
            assert!(job.rect.x >= x0 && job.rect.y >= y0, "{:?}", job.rect);
            assert!(job.rect.x + job.rect.w <= x0 + side, "{:?}", job.rect);
            assert!(job.rect.y + job.rect.h <= y0 + side, "{:?}", job.rect);
        }

        let mut only_cam = Appearance::new_regions(4, 1, 9);
        for _ in 0..20 {
            only_cam.wait = 0.0;
            for region in &mut only_cam.regions {
                region.fade = 1.0;
            }
            let job = only_cam.tick(0.0, &[webcam]).unwrap();
            assert_eq!(job.source, SOURCE_WEBCAM);
            assert!((WEBCAM_SAMPLE_MIN..=WEBCAM_SAMPLE_MAX).contains(&job.rect.w), "{:?}", job.rect);
            assert!((WEBCAM_SAMPLE_MIN..=WEBCAM_SAMPLE_MAX).contains(&job.rect.h), "{:?}", job.rect);
            assert!(job.rect.x + job.rect.w <= webcam.width, "{:?}", job.rect);
            assert!(job.rect.y + job.rect.h <= webcam.height, "{:?}", job.rect);
        }

        let mut both = Appearance::new_regions(4, 2, 11);
        let mut saw_desk = 0;
        let mut saw_cam = 0;
        for _ in 0..90 {
            both.wait = 0.0;
            for region in &mut both.regions {
                region.fade = 1.0;
            }
            let job = both.tick(0.0, &[desktop, webcam]).unwrap();
            let (width, height) = match job.source {
                SOURCE_DESKTOP => {
                    saw_desk += 1;
                    let (x0, y0, side) = center_square(desktop.width, desktop.height);
                    assert!(job.rect.x >= x0 && job.rect.y >= y0, "{:?}", job.rect);
                    assert!(job.rect.x + job.rect.w <= x0 + side, "{:?}", job.rect);
                    assert!(job.rect.y + job.rect.h <= y0 + side, "{:?}", job.rect);
                    (desktop.width, desktop.height)
                }
                SOURCE_WEBCAM => {
                    saw_cam += 1;
                    (webcam.width, webcam.height)
                }
                other => panic!("unknown source {other}"),
            };
            assert!(job.rect.x + job.rect.w <= width, "{:?} source {}", job.rect, job.source);
            assert!(job.rect.y + job.rect.h <= height, "{:?} source {}", job.rect, job.source);
        }
        assert!(saw_desk > 0 && saw_cam > 0, "desktop {saw_desk} webcam {saw_cam}");
    }

    #[test]
    fn real_mesh_has_a_face_and_balanced_regions() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
        let actor = crate::avatar::AvatarActor::load(&path).expect("avatar");
        let app = &actor.appearance;
        assert!((8..=MAX_REGIONS).contains(&app.regions.len()), "regions {}", app.regions.len());
        assert_eq!(app.regions.iter().filter(|r| r.is_face).count(), 1);
        let face = app.regions.iter().position(|r| r.is_face).unwrap();
        assert!(app.regions[face].bbox_aspect > 0.0, "face bbox aspect");
        assert!(app.patches.iter().any(|p| p.region == face as u32));
        assert_eq!(app.patches.len(), actor.binding.records.len());
        for patch in &app.patches {
            assert!(patch.region < app.regions.len() as u32);
            assert!((0.0..=1.0).contains(&patch.u) && (0.0..=1.0).contains(&patch.v));
        }
        let mut areas: Vec<f32> = app.regions.iter().filter(|r| !r.is_face).map(|r| r.area).collect();
        areas.retain(|a| *a > 0.0);
        areas.sort_by(|a, b| a.total_cmp(b));
        let ratio = areas.last().copied().unwrap_or(1.0) / areas.first().copied().unwrap_or(1.0).max(1e-6);
        assert!(ratio < 8.0, "body region area ratio {ratio} areas {areas:?}");

        let mut app = actor.appearance;
        app.show_bake = false;
        let job = app.tick(0.0, &[IngressSource::desktop(1920, 1080)]).unwrap();
        assert_eq!(job.region, face as u32, "the head is the first snapshot");
        assert_eq!(job.source, SOURCE_DESKTOP);
        assert_eq!(transitioning(&app), 1);
        assert!(app.tick(0.016, &[IngressSource::desktop(1920, 1080)]).is_none());
        assert!(job.rect.w >= 32 && job.rect.h >= 32);
        assert!(job.rect.x + job.rect.w <= 1920 && job.rect.y + job.rect.h <= 1080);
    }

    #[test]
    fn breasts_share_the_chest_patch() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
        let actor = crate::avatar::AvatarActor::load(&path).expect("avatar");
        let mut region_of = std::collections::HashMap::<String, u32>::new();
        for (rec, patch) in actor.binding.records.iter().zip(actor.appearance.patches.iter()) {
            let joint = dominant_joint(&actor.mesh, rec.tri as usize) as usize;
            let name = actor.rig.names.get(joint).map(String::as_str).unwrap_or("");
            if !is_breast(name) && !is_chest_span(name) {
                continue;
            }
            if let Some(prev) = region_of.get(name).copied() {
                assert_eq!(prev, patch.region, "{name} split across patches");
            } else {
                region_of.insert(name.to_string(), patch.region);
            }
        }
        let chest = region_of["spine02"];
        assert_eq!(region_of["breast.L"], chest);
        assert_eq!(region_of["breast.R"], chest);
    }
}
