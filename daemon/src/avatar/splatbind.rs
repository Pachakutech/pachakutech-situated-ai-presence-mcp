//! Reader for the "SPLB" v1 surface-binding file written by
//! scripts/bake_splatbind.py, plus fingerprint validation against the mesh it
//! was baked from. A mismatch is a hard error: a binding must never be
//! rendered against a topology it was not baked for (docs/plans/text_to_cues.md).
use sha2::{Digest, Sha256};

pub struct SplatRecord {
    pub tri: u32,
    pub b1: f32,
    pub b2: f32,
    /// Offsets in meters: (along normal, along tangent_u, along tangent_v).
    pub off: [f32; 3],
}

pub struct SplatBinding {
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub degenerate_count: u32,
    pub index_sha: [u8; 32],
    pub position_sha: [u8; 32],
    pub records: Vec<SplatRecord>,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }
fn u16le(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
fn i16le(b: &[u8], o: usize) -> i16 { i16::from_le_bytes([b[o], b[o + 1]]) }

impl SplatBinding {
    pub fn parse(d: &[u8]) -> Result<SplatBinding, String> {
        if d.len() < 128 || &d[0..4] != b"SPLB" { return Err("not a SPLB file".into()); }
        if u32le(d, 4) != 1 { return Err(format!("unsupported SPLB version {}", u32le(d, 4))); }
        let flags = u32le(d, 8);
        if flags & 0x6 != 0x6 { return Err(format!("unsupported SPLB flags {flags:#x} (need unorm16 bary + snorm16 offsets)")); }
        let n = u32le(d, 12) as usize;
        let offset_scale = f32::from_le_bytes([d[32], d[33], d[34], d[35]]);
        if u32le(d, 36) != 16 || u32le(d, 40) != 128 { return Err("unexpected SPLB record/header size".into()); }
        if d.len() != 128 + 16 * n { return Err(format!("SPLB length {} != 128 + 16*{n}", d.len())); }
        let mut records = Vec::with_capacity(n);
        for i in 0..n {
            let o = 128 + 16 * i;
            records.push(SplatRecord {
                tri: u32le(d, o),
                b1: u16le(d, o + 4) as f32 / 65535.0,
                b2: u16le(d, o + 6) as f32 / 65535.0,
                off: [
                    i16le(d, o + 8) as f32 / 32767.0 * offset_scale,
                    i16le(d, o + 10) as f32 / 32767.0 * offset_scale,
                    i16le(d, o + 12) as f32 / 32767.0 * offset_scale,
                ],
            });
        }
        let mut index_sha = [0u8; 32];
        let mut position_sha = [0u8; 32];
        index_sha.copy_from_slice(&d[64..96]);
        position_sha.copy_from_slice(&d[96..128]);
        Ok(SplatBinding { vertex_count: u32le(d, 16), triangle_count: u32le(d, 20), degenerate_count: u32le(d, 44), index_sha, position_sha, records })
    }

    /// SHA-256(vertex_count_u32le || triangle_count_u32le || index_sha || position_sha).
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.vertex_count.to_le_bytes());
        h.update(self.triangle_count.to_le_bytes());
        h.update(self.index_sha);
        h.update(self.position_sha);
        hex(&h.finalize())
    }

    /// Verify against the mesh this runtime loaded (base POSITION + indices).
    pub fn validate(&self, rest: &[[f32; 3]], indices: &[u32]) -> Result<(), String> {
        if rest.len() as u32 != self.vertex_count || (indices.len() / 3) as u32 != self.triangle_count {
            return Err(format!("topology mismatch: mesh {}v/{}t vs binding {}v/{}t",
                rest.len(), indices.len() / 3, self.vertex_count, self.triangle_count));
        }
        let mut h = Sha256::new();
        for i in indices { h.update(i.to_le_bytes()); }
        if h.finalize().as_slice() != self.index_sha { return Err("index fingerprint mismatch".into()); }
        let mut h = Sha256::new();
        for p in rest { for c in p { h.update(c.to_le_bytes()); } }
        if h.finalize().as_slice() != self.position_sha { return Err("bind-pose position fingerprint mismatch".into()); }
        for r in &self.records {
            if r.tri >= self.triangle_count { return Err("record triangle index out of range".into()); }
        }
        Ok(())
    }
}
