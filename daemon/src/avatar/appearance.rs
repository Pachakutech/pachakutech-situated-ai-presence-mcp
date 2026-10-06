//! Soft body regions and the scheduler that points each one at a screen
//! rectangle. This module never sees pixels: a rectangle is four integers,
//! and the GPU copies that rectangle into an atlas tile.
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
/// If the face has worn one tile this long, the next snapshot is the face
/// again. Otherwise it waits in the same lottery as the body and the head
/// sits on the debug palette.
pub const FACE_STALE_SECS: f32 = 6.0;
pub const CHROME: [f32; 3] = [0.78, 0.81, 0.84];
/// How far a face sample moves toward chrome grey. The captured desktop
/// stays in front: 0.22 is a cool tint, not a grey wash.
pub const CHROME_MIX: f32 = 0.22;

pub const FLAG_FACE: u32 = 1;
pub const FLAG_SHOWN: u32 = 2;
pub const FLAG_INCOMING: u32 = 4;

const BODY_BINS: usize = 11;

/// std430, 16 bytes. `flags` is `FLAG_FACE | FLAG_SHOWN | FLAG_INCOMING`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RegionGpu {
    pub fade: f32,
    pub shown_page: u32,
    pub incoming_page: u32,
    pub flags: u32,
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
    /// Seconds since this region was last chosen.
    pub idle: f32,
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
            idle: 0.0,
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
        for joint in order {
            let bin = (0..BODY_BINS)
                .min_by(|&a, &b| bin_area[a].total_cmp(&bin_area[b]).then(a.cmp(&b)))
                .unwrap();
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

    /// Advance fades. When `screen` is a real capture and the timer has
    /// elapsed, queue one rectangle for one region. No screen: the timer
    /// does not move.
    pub fn tick(&mut self, dt: f32, screen: Option<(u32, u32)>) -> Option<PatchJob> {
        let dt = dt.max(0.0);
        for region in &mut self.regions {
            if region.incoming_valid && region.fade < 1.0 {
                region.fade = (region.fade + dt / FADE_SECS).min(1.0);
                if region.fade >= 1.0 {
                    region.shown_page = region.incoming_page;
                    region.shown_valid = true;
                }
            }
        }
        if self.show_bake || self.regions.is_empty() {
            return None;
        }
        let Some((width, height)) = screen.filter(|&(w, h)| w > 0 && h > 0) else {
            return None;
        };
        for region in &mut self.regions {
            region.idle += dt;
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
        // The head leaves the debug palette on the first capture, and is
        // chosen again once it has been idle, instead of waiting out the
        // full round of body regions.
        let pick = eligible
            .iter()
            .copied()
            .find(|&i| {
                let region = &self.regions[i];
                region.is_face && (!region.shown_valid || region.idle >= FACE_STALE_SECS)
            })
            .unwrap_or_else(|| eligible[self.next_u32() as usize % eligible.len()]);
        let rect = random_screen_rect(&mut self.rng, width, height);
        let region = &mut self.regions[pick];
        let page = if region.shown_valid { 1 - (region.shown_page & 1) } else { 0 };
        region.incoming_page = page;
        region.incoming_valid = true;
        region.fade = 0.0;
        region.idle = 0.0;
        let jitter = self.next_u32() as f32 / u32::MAX as f32;
        self.wait = 2.0 + jitter;
        Some(PatchJob { region: pick as u32, page, rect })
    }

    pub fn region_uniforms(&self) -> [RegionGpu; MAX_REGIONS] {
        let mut out = [RegionGpu { fade: 0.0, shown_page: 0, incoming_page: 0, flags: 0 }; MAX_REGIONS];
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

pub fn random_screen_rect(rng: &mut u32, width: u32, height: u32) -> Rect {
    let w = span(rng, width);
    let h = span(rng, height);
    let x = if width > w { xorshift(rng) % (width - w + 1) } else { 0 };
    let y = if height > h { xorshift(rng) % (height - h + 1) } else { 0 };
    Rect { x, y, w, h }
}

fn span(rng: &mut u32, extent: u32) -> u32 {
    let want = 32 + (xorshift(rng) % 97);
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

fn face_name(name: &str) -> bool {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == "head" || token == "jaw")
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
    fn gpu_structs_are_16_bytes() {
        assert_eq!(std::mem::size_of::<RegionGpu>(), 16);
        assert_eq!(std::mem::size_of::<SplatPatchGpu>(), 16);
        assert_eq!(ATLAS_W, 1024);
        assert_eq!(ATLAS_H, 128);
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
    fn rects_stay_inside_and_repeat_for_a_seed() {
        let mut a = Appearance::new_regions(1, 0, 42);
        let mut b = Appearance::new_regions(1, 0, 42);
        let ja = a.tick(0.0, Some((800, 600))).unwrap();
        let jb = b.tick(0.0, Some((800, 600))).unwrap();
        assert_eq!(ja.rect, jb.rect);
        assert!((2.0..=3.0).contains(&a.wait), "wait {}", a.wait);

        for _ in 0..30 {
            a.wait = 0.0;
            for region in &mut a.regions {
                region.fade = 1.0;
            }
            let job = a.tick(0.0, Some((100, 80))).unwrap();
            assert!(job.rect.x + job.rect.w <= 100, "{:?}", job.rect);
            assert!(job.rect.y + job.rect.h <= 80, "{:?}", job.rect);
            assert!((32..=100).contains(&job.rect.w), "{:?}", job.rect);
            assert!((32..=80).contains(&job.rect.h), "{:?}", job.rect);
            assert_eq!(transitioning(&a), 1);
        }
    }

    #[test]
    fn one_region_per_tick_and_the_timer_waits_for_a_screen() {
        let mut a = Appearance::new_regions(4, 0, 7);
        assert!(a.tick(0.0, None).is_none(), "no capture, no snapshot");
        assert_eq!(a.wait, 0.0, "the timer does not run without a screen");
        let first = a.tick(0.0, Some((1920, 1080))).unwrap();
        assert_eq!(transitioning(&a), 1);
        assert!(a.tick(0.1, Some((1920, 1080))).is_none());
        assert_eq!(transitioning(&a), 1);
        assert!((0.0..1.0).contains(&a.regions[first.region as usize].fade));
        a.tick(0.5, Some((1920, 1080)));
        assert!(a.regions[first.region as usize].shown_valid);
        assert_eq!(transitioning(&a), 0);
        assert!(a.tick(0.2, Some((1920, 1080))).is_none(), "still inside the 2s wait");
        let second = a.tick(3.0, Some((1920, 1080))).unwrap();
        assert_eq!(transitioning(&a), 1);
        let _ = second;
        let mut live = 0;
        a.tick(0.0, Some((1920, 1080)));
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
        assert!(a.tick(1.0, Some((640, 480))).is_none());
        assert!(a.regions.iter().all(|r| !r.incoming_valid));
    }

    #[test]
    fn face_leaves_the_debug_palette_first_and_comes_back() {
        let mut a = Appearance::new_regions(8, 4, 3);
        let first = a.tick(0.0, Some((1920, 1080))).unwrap();
        assert_eq!(first.region, 4, "the face is the first snapshot");
        a.tick(0.5, Some((1920, 1080)));
        assert!(a.regions[4].shown_valid);

        a.wait = 0.0;
        a.regions[4].idle = FACE_STALE_SECS;
        a.regions[4].fade = 1.0;
        let again = a.tick(0.0, Some((1920, 1080))).unwrap();
        assert_eq!(again.region, 4, "a stale face is chosen ahead of the body");
    }

    #[test]
    fn face_flag_reaches_the_uniform() {
        let a = Appearance::new_regions(3, 1, 1);
        let u = a.region_uniforms();
        assert_eq!(u[1].flags & FLAG_FACE, FLAG_FACE);
        assert_eq!(u[0].flags & FLAG_FACE, 0);
    }

    #[test]
    fn real_mesh_has_a_face_and_balanced_regions() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
        let actor = crate::avatar::AvatarActor::load(&path).expect("avatar");
        let app = &actor.appearance;
        assert!((8..=MAX_REGIONS).contains(&app.regions.len()), "regions {}", app.regions.len());
        assert_eq!(app.regions.iter().filter(|r| r.is_face).count(), 1);
        let face = app.regions.iter().position(|r| r.is_face).unwrap();
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
        let job = app.tick(0.0, Some((1920, 1080))).unwrap();
        assert_eq!(job.region, face as u32, "the face is the first snapshot");
        assert_eq!(transitioning(&app), 1);
        assert!(app.tick(0.016, Some((1920, 1080))).is_none());
        assert!(job.rect.w >= 32 && job.rect.h >= 32);
        assert!(job.rect.x + job.rect.w <= 1920 && job.rect.y + job.rect.h <= 1080);
    }
}
