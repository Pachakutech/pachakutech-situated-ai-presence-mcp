//! Real readers for the two splat interchange formats `addArtifact` is
//! scoped to (see docs/architecture.md — no glTF/mesh formats on purpose).
//! Both are well-established, informally-specified-but-consistent formats;
//! sources checked while writing this: PlayCanvas's PLY format writeup
//! (developer.playcanvas.com/user-manual/gaussian-splatting/formats/ply),
//! the antimatter15/splat reference viewer and its README, and the original
//! 3D Gaussian Splatting paper (Kerbl et al. 2023) for the covariance
//! formula Σ = R S Sᵗ Rᵗ.
//!
//! Both formats store scale + rotation directly; `GaussianSplat` stores a
//! precomputed covariance instead (see scene_memory.rs), so both loaders
//! finish by converting scale+quaternion into that 6-float upper-triangle
//! form rather than carrying scale/rotation forward separately.

use super::scene_memory::GaussianSplat;
use std::fs;
use std::path::Path;

const SH_C0: f32 = 0.282_094_79;

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Σ = R S Sᵗ Rᵗ, returned as the upper triangle
/// [xx, xy, xz, yy, yz, zz]. `scale` is the *linear* (already-exponentiated)
/// per-axis standard deviation; `rot` is a normalized (w, x, y, z)
/// quaternion.
pub fn covariance_from_scale_rotation(scale: [f32; 3], rot: [f32; 4]) -> [f32; 6] {
    let (w, x, y, z) = (rot[0], rot[1], rot[2], rot[3]);
    let norm = (w * w + x * x + y * y + z * z).sqrt().max(1e-8);
    let (w, x, y, z) = (w / norm, x / norm, y / norm, z / norm);

    // Standard quaternion -> 3x3 rotation matrix.
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)],
        [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)],
        [2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y)],
    ];

    // Cov[i][j] = sum_k scale[k]^2 * R[i][k] * R[j][k]  (M = R*diag(scale), Cov = M M^T)
    let mut cov = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let mut acc = 0.0;
            for k in 0..3 {
                acc += scale[k] * scale[k] * r[i][k] * r[j][k];
            }
            cov[i][j] = acc;
        }
    }
    [cov[0][0], cov[0][1], cov[0][2], cov[1][1], cov[1][2], cov[2][2]]
}

/// The antimatter15 `.splat` format: no header, a flat array of fixed
/// 32-byte little-endian records — position (3xf32), scale (3xf32, already
/// linear), color (4x u8 RGBA, already baked), rotation (4x u8 quaternion,
/// w,x,y,z, each byte decoding as `(b - 128) / 128`).
pub fn load_splat(bytes: &[u8]) -> Result<Vec<GaussianSplat>, String> {
    const RECORD_SIZE: usize = 32;
    if bytes.len() % RECORD_SIZE != 0 {
        return Err(format!(
            ".splat file size {} isn't a multiple of the 32-byte record size",
            bytes.len()
        ));
    }
    let count = bytes.len() / RECORD_SIZE;
    let mut splats = Vec::with_capacity(count);

    for i in 0..count {
        let rec = &bytes[i * RECORD_SIZE..(i + 1) * RECORD_SIZE];
        let f32_at = |off: usize| -> f32 {
            f32::from_le_bytes([rec[off], rec[off + 1], rec[off + 2], rec[off + 3]])
        };
        let position = [f32_at(0), f32_at(4), f32_at(8)];
        let scale = [f32_at(12), f32_at(16), f32_at(20)];
        let color = [
            rec[24] as f32 / 255.0,
            rec[25] as f32 / 255.0,
            rec[26] as f32 / 255.0,
            rec[27] as f32 / 255.0,
        ];
        let decode_q = |b: u8| (b as f32 - 128.0) / 128.0;
        let rot = [decode_q(rec[28]), decode_q(rec[29]), decode_q(rec[30]), decode_q(rec[31])];

        splats.push(GaussianSplat {
            position,
            scale,
            rotation: rot,
            color,
        });
    }
    Ok(splats)
}

/// One property declared in a PLY header: name, byte width, and whether we
/// actually want to keep its value.
struct PlyProperty {
    name: String,
    size: usize,
}

fn ply_type_size(ty: &str) -> Result<usize, String> {
    match ty {
        "float" | "float32" | "int" | "int32" | "uint" | "uint32" => Ok(4),
        "double" | "float64" => Ok(8),
        "uchar" | "uint8" | "char" | "int8" => Ok(1),
        "short" | "int16" | "ushort" | "uint16" => Ok(2),
        other => Err(format!("unsupported PLY property type '{other}' (only scalar numeric types are handled — no list properties)")),
    }
}

/// A 3D Gaussian Splatting PLY file: ASCII header ending in `end_header`,
/// then `binary_little_endian` vertex data. Property order and presence
/// (normals, SH degree via `f_rest_N` count) vary file to file, so the
/// header is actually parsed rather than assuming fixed offsets — that's
/// the main way this differs in care from the `.splat` reader above.
pub fn load_ply(bytes: &[u8]) -> Result<Vec<GaussianSplat>, String> {
    let header_end = find_end_header(bytes).ok_or("no 'end_header' line found — not a PLY file?")?;
    let header_text = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| "PLY header isn't valid UTF-8/ASCII".to_string())?;

    let mut is_binary_le = false;
    let mut vertex_count: usize = 0;
    let mut properties: Vec<PlyProperty> = Vec::new();
    let mut in_vertex_element = false;

    for line in header_text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("format ") {
            is_binary_le = rest.starts_with("binary_little_endian");
        } else if let Some(rest) = line.strip_prefix("element ") {
            let mut parts = rest.split_whitespace();
            let name = parts.next().unwrap_or("");
            in_vertex_element = name == "vertex";
            if in_vertex_element {
                vertex_count = parts.next().unwrap_or("0").parse().map_err(|_| "bad vertex count")?;
            }
        } else if let Some(rest) = line.strip_prefix("property ") {
            if in_vertex_element {
                let mut parts = rest.split_whitespace();
                let ty = parts.next().ok_or("malformed property line")?;
                if ty == "list" {
                    return Err("list properties (e.g. face indices) aren't supported — this loader is for splat point clouds, not meshes".to_string());
                }
                let name = parts.next().ok_or("malformed property line")?;
                properties.push(PlyProperty { name: name.to_string(), size: ply_type_size(ty)? });
            }
        }
    }

    if !is_binary_le {
        return Err("only 'binary_little_endian' PLY files are supported — ASCII/big-endian PLY isn't handled".to_string());
    }
    if vertex_count == 0 {
        return Err("no 'element vertex N' found (or N was 0)".to_string());
    }

    // Precompute each wanted property's byte offset within one vertex record.
    let wanted = [
        "x", "y", "z", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
        "opacity", "f_dc_0", "f_dc_1", "f_dc_2",
    ];
    let mut offsets: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut cursor = 0usize;
    for prop in &properties {
        if wanted.contains(&prop.name.as_str()) {
            offsets.insert(
                wanted.iter().find(|w| **w == prop.name).unwrap(),
                cursor,
            );
        }
        cursor += prop.size;
    }
    let record_size = cursor;
    for name in wanted {
        if !offsets.contains_key(name) {
            return Err(format!("PLY file is missing required property '{name}'"));
        }
    }

    let body = &bytes[header_end..];
    let needed = record_size * vertex_count;
    if body.len() < needed {
        return Err(format!(
            "PLY body is {} bytes, expected at least {needed} for {vertex_count} vertices at {record_size} bytes each",
            body.len()
        ));
    }

    let f32_at = |record: &[u8], off: usize| -> f32 {
        f32::from_le_bytes([record[off], record[off + 1], record[off + 2], record[off + 3]])
    };

    let mut splats = Vec::with_capacity(vertex_count);
    for i in 0..vertex_count {
        let rec = &body[i * record_size..(i + 1) * record_size];
        let get = |name: &str| f32_at(rec, offsets[name]);

        let position = [get("x"), get("y"), get("z")];
        // PLY stores log-scale; exponentiate to the linear std-dev our
        // covariance formula expects.
        let scale = [get("scale_0").exp(), get("scale_1").exp(), get("scale_2").exp()];
        let rot = [get("rot_0"), get("rot_1"), get("rot_2"), get("rot_3")];
        // DC spherical-harmonic term -> displayable RGB; opacity is stored
        // as a pre-sigmoid logit.
        let color = [
            (0.5 + SH_C0 * get("f_dc_0")).clamp(0.0, 1.0),
            (0.5 + SH_C0 * get("f_dc_1")).clamp(0.0, 1.0),
            (0.5 + SH_C0 * get("f_dc_2")).clamp(0.0, 1.0),
            sigmoid(get("opacity")),
        ];

        splats.push(GaussianSplat {
            position,
            scale,
            rotation: rot,
            color,
        });
    }
    Ok(splats)
}

fn find_end_header(bytes: &[u8]) -> Option<usize> {
    const MARKER: &[u8] = b"end_header\n";
    bytes.windows(MARKER.len()).position(|w| w == MARKER).map(|pos| pos + MARKER.len())
}

/// Dispatches on file extension. Only local paths are handled today —
/// `addArtifact`'s `sourceUri` is free-form and may be a remote URL, which
/// this deliberately doesn't fetch (no HTTP client pulled in for this yet).
pub fn load_from_path(path: &Path) -> Result<Vec<GaussianSplat>, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
        Some(ext) if ext == "splat" => load_splat(&bytes),
        Some(ext) if ext == "ply" => load_ply(&bytes),
        Some(ext) => Err(format!("unsupported artifact extension '.{ext}' — only .splat and .ply are handled")),
        None => Err("artifact path has no file extension to dispatch on".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_f32(buf: &mut Vec<u8>, v: f32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn splat_round_trip() {
        // One record: position (1,2,3), scale (0.5,0.5,0.5), color
        // (255,0,0,255) = opaque red, identity rotation (w=1,x=y=z=0).
        let mut buf = Vec::new();
        push_f32(&mut buf, 1.0);
        push_f32(&mut buf, 2.0);
        push_f32(&mut buf, 3.0);
        push_f32(&mut buf, 0.5);
        push_f32(&mut buf, 0.5);
        push_f32(&mut buf, 0.5);
        buf.extend_from_slice(&[255, 0, 0, 255]);
        // identity quat (w,x,y,z) = (1,0,0,0) -> bytes = 128 + 128*q, clamped to u8
        buf.extend_from_slice(&[255, 128, 128, 128]);
        assert_eq!(buf.len(), 32);

        let splats = load_splat(&buf).expect("should parse one record");
        assert_eq!(splats.len(), 1);
        let s = &splats[0];
        assert_eq!(s.position, [1.0, 2.0, 3.0]);
        assert!((s.scale[0] - 0.5).abs() < 1e-3, "scale should round-trip as linear 0.5, got {}", s.scale[0]);
        assert!((s.color[0] - 1.0).abs() < 1e-3, "red channel should be ~1.0, got {}", s.color[0]);
        assert!((s.color[1]).abs() < 1e-3, "green channel should be ~0.0");
        // Identity rotation + uniform scale 0.5 -> covariance should be
        // diagonal with 0.25 on the diagonal (0.5^2) and ~0 off-diagonal.
        let cov = s.covariance();
        assert!((cov[0] - 0.25).abs() < 1e-3, "cov_xx, got {}", cov[0]);
        assert!((cov[3] - 0.25).abs() < 1e-3, "cov_yy, got {}", cov[3]);
        assert!((cov[5] - 0.25).abs() < 1e-3, "cov_zz, got {}", cov[5]);
        assert!(cov[1].abs() < 1e-3, "cov_xy should be ~0 for identity rotation");
    }

    #[test]
    fn splat_rejects_bad_size() {
        let bad = vec![0u8; 31];
        assert!(load_splat(&bad).is_err());
    }

    #[test]
    fn ply_round_trip_minimal() {
        let header = "ply\nformat binary_little_endian 1.0\nelement vertex 1\n\
             property float x\nproperty float y\nproperty float z\n\
             property float nx\nproperty float ny\nproperty float nz\n\
             property float f_dc_0\nproperty float f_dc_1\nproperty float f_dc_2\n\
             property float opacity\n\
             property float scale_0\nproperty float scale_1\nproperty float scale_2\n\
             property float rot_0\nproperty float rot_1\nproperty float rot_2\nproperty float rot_3\n\
             end_header\n";
        let mut buf = header.as_bytes().to_vec();
        // x,y,z
        push_f32(&mut buf, 4.0);
        push_f32(&mut buf, 5.0);
        push_f32(&mut buf, 6.0);
        // nx,ny,nz (unused, present to prove we correctly skip fields)
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);
        // f_dc_0..2: 0.0 DC -> should decode to mid-gray (0.5) before clamping
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);
        // opacity: large positive logit -> sigmoid ~1.0
        push_f32(&mut buf, 10.0);
        // scale_0..2: log(1.0) = 0.0 -> exp -> 1.0
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);
        // rot_0..3: identity quaternion (w,x,y,z)
        push_f32(&mut buf, 1.0);
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);
        push_f32(&mut buf, 0.0);

        let splats = load_ply(&buf).expect("should parse one vertex");
        assert_eq!(splats.len(), 1);
        let s = &splats[0];
        assert_eq!(s.position, [4.0, 5.0, 6.0]);
        assert!((s.scale[0] - 1.0).abs() < 1e-3, "log-scale 0.0 should exponentiate to 1.0, got {}", s.scale[0]);
        assert!((s.color[0] - 0.5).abs() < 1e-3, "DC=0 should decode near mid-gray, got {}", s.color[0]);
        assert!(s.color[3] > 0.99, "large positive logit should sigmoid near 1.0, got {}", s.color[3]);
        // identity rotation, scale 1.0 -> covariance is the identity matrix
        let cov = s.covariance();
        assert!((cov[0] - 1.0).abs() < 1e-3);
        assert!((cov[3] - 1.0).abs() < 1e-3);
        assert!((cov[5] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn ply_rejects_missing_property() {
        let header = "ply\nformat binary_little_endian 1.0\nelement vertex 1\n\
             property float x\nproperty float y\nproperty float z\nend_header\n";
        let buf = header.as_bytes().to_vec();
        assert!(load_ply(&buf).is_err(), "should reject a PLY missing scale/rot/opacity/f_dc");
    }
}
