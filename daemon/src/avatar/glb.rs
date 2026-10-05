//! Minimal GLB container + accessor reader (dense and sparse). Hand-rolled on
//! serde_json to keep the daemon's no-build-time-deps pattern.
use serde_json::Value;

pub struct Glb {
    pub json: Value,
    pub bin: Vec<u8>,
}

/// Accessor contents converted to f32 (integers are cast, not normalized).
pub struct Accessor {
    pub data: Vec<f32>,
    pub ncomp: usize,
    pub count: usize,
}

fn rd_u32(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "GLB truncated".to_string())
}

impl Glb {
    pub fn load(path: &std::path::Path) -> Result<Glb, String> {
        let d = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&d)
    }

    pub fn parse(d: &[u8]) -> Result<Glb, String> {
        if d.len() < 20 || &d[0..4] != b"glTF" {
            return Err("not a GLB".into());
        }
        let mut off = 12;
        let (mut json, mut bin) = (None, Vec::new());
        while off + 8 <= d.len() {
            let len = rd_u32(d, off)? as usize;
            let ty = &d[off + 4..off + 8];
            let body = d.get(off + 8..off + 8 + len).ok_or("GLB chunk overruns file")?;
            if ty == b"JSON" {
                json = Some(serde_json::from_slice::<Value>(body).map_err(|e| format!("GLB json: {e}"))?);
            } else if ty == b"BIN\0" {
                bin = body.to_vec();
            }
            off += 8 + len;
        }
        Ok(Glb { json: json.ok_or("GLB has no JSON chunk")?, bin })
    }

    fn comp_info(ct: u64) -> Result<(usize, bool), String> {
        Ok(match ct {
            5126 => (4, true),
            5125 => (4, false),
            5123 => (2, false),
            5122 => (2, false),
            5121 => (1, false),
            5120 => (1, false),
            _ => return Err(format!("unsupported componentType {ct}")),
        })
    }

    fn read_comp(&self, ct: u64, o: usize) -> Result<f32, String> {
        let b = &self.bin;
        let g = |n: usize| b.get(o..o + n).ok_or_else(|| "accessor out of range".to_string());
        Ok(match ct {
            5126 => { let s = g(4)?; f32::from_le_bytes([s[0], s[1], s[2], s[3]]) }
            5125 => { let s = g(4)?; u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 }
            5123 => { let s = g(2)?; u16::from_le_bytes([s[0], s[1]]) as f32 }
            5122 => { let s = g(2)?; i16::from_le_bytes([s[0], s[1]]) as f32 }
            5121 => g(1)?[0] as f32,
            5120 => g(1)?[0] as i8 as f32,
            _ => return Err(format!("unsupported componentType {ct}")),
        })
    }

    fn view_base(&self, view: usize) -> Result<(usize, Option<usize>), String> {
        let bv = &self.json["bufferViews"][view];
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = bv["byteStride"].as_u64().map(|s| s as usize);
        if bv.is_null() { return Err(format!("bad bufferView {view}")); }
        Ok((off, stride))
    }

    pub fn accessor(&self, idx: usize) -> Result<Accessor, String> {
        let a = &self.json["accessors"][idx];
        if a.is_null() { return Err(format!("no accessor {idx}")); }
        let ct = a["componentType"].as_u64().ok_or("componentType")?;
        let count = a["count"].as_u64().ok_or("count")? as usize;
        let ncomp = match a["type"].as_str().ok_or("type")? {
            "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, "MAT4" => 16,
            t => return Err(format!("unsupported accessor type {t}")),
        };
        let (csz, _) = Self::comp_info(ct)?;
        let mut data = vec![0.0f32; count * ncomp];
        if let Some(v) = a["bufferView"].as_u64() {
            let (bo, stride) = self.view_base(v as usize)?;
            let base = bo + a["byteOffset"].as_u64().unwrap_or(0) as usize;
            let stride = stride.unwrap_or(csz * ncomp);
            for i in 0..count {
                for c in 0..ncomp {
                    data[i * ncomp + c] = self.read_comp(ct, base + i * stride + c * csz)?;
                }
            }
        }
        if let Some(sp) = a.get("sparse") {
            let n = sp["count"].as_u64().ok_or("sparse.count")? as usize;
            let ict = sp["indices"]["componentType"].as_u64().ok_or("sparse ict")?;
            let (isz, _) = Self::comp_info(ict)?;
            let (ibo, _) = self.view_base(sp["indices"]["bufferView"].as_u64().ok_or("sparse iv")? as usize)?;
            let ibase = ibo + sp["indices"]["byteOffset"].as_u64().unwrap_or(0) as usize;
            let (vbo, _) = self.view_base(sp["values"]["bufferView"].as_u64().ok_or("sparse vv")? as usize)?;
            let vbase = vbo + sp["values"]["byteOffset"].as_u64().unwrap_or(0) as usize;
            for k in 0..n {
                let target = self.read_comp(ict, ibase + k * isz)? as usize;
                if target >= count { return Err("sparse index out of range".into()); }
                for c in 0..ncomp {
                    data[target * ncomp + c] = self.read_comp(ct, vbase + (k * ncomp + c) * csz)?;
                }
            }
        }
        Ok(Accessor { data, ncomp, count })
    }

    pub fn vec3s(&self, idx: usize) -> Result<Vec<[f32; 3]>, String> {
        let a = self.accessor(idx)?;
        if a.ncomp != 3 { return Err("expected VEC3".into()); }
        Ok(a.data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
    }

    pub fn indices(&self, idx: usize) -> Result<Vec<u32>, String> {
        Ok(self.accessor(idx)?.data.into_iter().map(|f| f as u32).collect())
    }
}
