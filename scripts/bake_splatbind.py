#!/usr/bin/env python3
"""Mesh → splatbind bake tool (offline asset step).

Turns a topology-frozen humanoid proxy GLB into a barycentric Gaussian-splat
binding. The runtime never re-samples the mesh: it loads this file, checks the
fingerprint, deforms the proxy, and reconstructs each splat from
(triangle_index, bary_u, bary_v) plus an optional tangent-frame offset.

This is an asset build step, not a daemon hot path. Dependency: numpy.

Example
-------
    python scripts/bake_splatbind.py \\
        --glb assets/humanoid_proxy_a_pose.glb \\
        --samples 50000 \\
        --seed 42 \\
        --out assets/humanoid_proxy_a_pose.splatbind \\
        --dump-ply assets/humanoid_proxy_a_pose.samples.ply

Binary layout (little-endian, version 1)
----------------------------------------
Header (128 bytes)::

    offset  size  field
    0       4     magic              "SPLB"
    4       4     version            u32 = 1
    8       4     flags              u32
    12      4     splat_count        u32
    16      4     vertex_count       u32
    20      4     triangle_count     u32   (index_count / 3, including degenerates)
    24      4     primitive_index    u32
    28      4     seed               u32
    32      4     offset_scale       f32   meters at snorm magnitude 1.0
    36      4     record_size        u32 = 16
    40      4     header_size        u32 = 128
    44      4     degenerate_count   u32
    48     16     reserved           zeros
    64     32     index_sha256       SHA-256 of canonical u32le index buffer
    96     32     position_sha256    SHA-256 of bind-pose f32le xyz (mesh-local)
    128     16*N  SurfaceBoundSplat records

Flags::

    0x1  records sorted by (triangle_index, bary_u, bary_v)
    0x2  bary_u / bary_v are unorm16
    0x4  offsets are snorm16, decoded as (i16 / 32767) * offset_scale

SurfaceBoundSplat (16 bytes)::

    u32 triangle_index     index into the GLB triangle list (i = 3 * triangle_index)
    u16 bary_u             unorm16 of beta1  (vertex 1)
    u16 bary_v             unorm16 of beta2  (vertex 2)
    i16 normal_offset      snorm16 along the geometric triangle normal
    i16 tangent_offset_u   snorm16 along normalize(v1 - v0)
    i16 tangent_offset_v   snorm16 along normalize(n × tangent_u)
    u16 reserved           0

Barycentric reconstruction (runtime)::

    beta1 = bary_u / 65535
    beta2 = bary_v / 65535
    beta0 = max(0, 1 - beta1 - beta2)
    p     = beta0 * v0 + beta1 * v1 + beta2 * v2
    n     = normalize((v1 - v0) × (v2 - v0))
    t_u   = normalize(v1 - v0)
    t_v   = normalize(n × t_u)
    p    += offset_scale * (normal_offset, tangent_offset_u, tangent_offset_v)_snorm
            dotted onto (n, t_u, t_v)

Topology fingerprint
--------------------
Printed as a 64-hex-digit SHA-256 of::

    vertex_count_u32le || triangle_count_u32le || index_sha256 || position_sha256

It changes if vertex count, triangle count, index order, or bind-pose positions
change. Morph-target weights are not part of the fingerprint: sampling uses the
POSITION accessor only (bind pose). Node transforms are not hashed; they are
applied only when reconstructing sample positions for ``--dump-ply``.

Sampling (v1)
-------------
Area-weighted uniform triangle sampling with ``numpy.random.Generator(PCG64)``.
On a selected triangle, barycentrics follow the standard sqrt map::

    q = sqrt(r1);  beta0 = 1 - q;  beta1 = q * (1 - r2);  beta2 = q * r2
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np

MAGIC = b"SPLB"
VERSION = 1
HEADER_SIZE = 128
RECORD_SIZE = 16
OFFSET_SCALE_M = 0.01  # 1.0 snorm = 1 cm
AREA_EPS = 1e-12
FLAG_SORTED = 0x1
FLAG_BARY_UNORM16 = 0x2
FLAG_OFFSET_SNORM16 = 0x4
DEFAULT_FLAGS = FLAG_SORTED | FLAG_BARY_UNORM16 | FLAG_OFFSET_SNORM16
DEFAULT_SEED = 42
DEFAULT_SAMPLES = 50_000

# glTF component types
_C_BYTE, _C_UBYTE = 5120, 5121
_C_SHORT, _C_USHORT = 5122, 5123
_C_UINT, _C_FLOAT = 5125, 5126
_N_COMP = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
_DTYPE = {
    _C_BYTE: np.int8,
    _C_UBYTE: np.uint8,
    _C_SHORT: np.int16,
    _C_USHORT: np.uint16,
    _C_UINT: np.uint32,
    _C_FLOAT: np.float32,
}


# ---------------------------------------------------------------------------
# GLB / glTF
# ---------------------------------------------------------------------------


def _quat_to_mat4(q: np.ndarray) -> np.ndarray:
    x, y, z, w = (float(v) for v in q)
    xx, yy, zz = x * x, y * y, z * z
    xy, xz, yz = x * y, x * z, y * z
    wx, wy, wz = w * x, w * y, w * z
    m = np.eye(4, dtype=np.float64)
    m[0, 0] = 1 - 2 * (yy + zz)
    m[0, 1] = 2 * (xy - wz)
    m[0, 2] = 2 * (xz + wy)
    m[1, 0] = 2 * (xy + wz)
    m[1, 1] = 1 - 2 * (xx + zz)
    m[1, 2] = 2 * (yz - wx)
    m[2, 0] = 2 * (xz - wy)
    m[2, 1] = 2 * (yz + wx)
    m[2, 2] = 1 - 2 * (xx + yy)
    return m


def _node_local_matrix(node: dict[str, Any]) -> np.ndarray:
    if "matrix" in node:
        return np.array(node["matrix"], dtype=np.float64).reshape(4, 4, order="F")
    t = np.array(node.get("translation", (0.0, 0.0, 0.0)), dtype=np.float64)
    r = np.array(node.get("rotation", (0.0, 0.0, 0.0, 1.0)), dtype=np.float64)
    s = np.array(node.get("scale", (1.0, 1.0, 1.0)), dtype=np.float64)
    T = np.eye(4, dtype=np.float64)
    T[:3, 3] = t
    R = _quat_to_mat4(r)
    S = np.eye(4, dtype=np.float64)
    S[0, 0], S[1, 1], S[2, 2] = s
    return T @ R @ S


def _parent_map(nodes: list[dict[str, Any]]) -> dict[int, int]:
    parents: dict[int, int] = {}
    for i, node in enumerate(nodes):
        for child in node.get("children") or []:
            parents[int(child)] = i
    return parents


def _world_matrix(nodes: list[dict[str, Any]], index: int) -> np.ndarray:
    parents = _parent_map(nodes)
    chain: list[int] = []
    cur: int | None = index
    seen: set[int] = set()
    while cur is not None:
        if cur in seen:
            raise ValueError(f"node cycle at {cur}")
        seen.add(cur)
        chain.append(cur)
        cur = parents.get(cur)
    world = np.eye(4, dtype=np.float64)
    for i in reversed(chain):
        world = world @ _node_local_matrix(nodes[i])
    return world


def load_glb(path: Path) -> tuple[dict[str, Any], bytes]:
    data = path.read_bytes()
    if len(data) < 12:
        raise ValueError(f"{path} is too small to be a GLB")
    magic, version, length = struct.unpack_from("<4sII", data, 0)
    if magic != b"glTF":
        raise ValueError(f"{path} is not a GLB (magic={magic!r})")
    if version != 2:
        raise ValueError(f"{path}: unsupported GLB version {version}")
    if length > len(data):
        raise ValueError(f"{path}: truncated GLB header length {length} > {len(data)}")
    offset = 12
    json_blob: bytes | None = None
    bin_blob = b""
    while offset + 8 <= length:
        chunk_len, chunk_type = struct.unpack_from("<I4s", data, offset)
        offset += 8
        chunk = data[offset : offset + chunk_len]
        offset += chunk_len
        if chunk_type == b"JSON":
            json_blob = chunk
        elif chunk_type == b"BIN\x00":
            bin_blob = chunk
    if json_blob is None:
        raise ValueError(f"{path}: GLB has no JSON chunk")
    gltf = json.loads(json_blob.rstrip(b" \x00").decode("utf-8"))
    return gltf, bin_blob


def _accessor_numpy(gltf: dict[str, Any], blob: bytes, accessor_index: int) -> np.ndarray:
    accessors = gltf["accessors"]
    if accessor_index < 0 or accessor_index >= len(accessors):
        raise ValueError(f"accessor {accessor_index} out of range")
    acc = accessors[accessor_index]
    if acc.get("sparse"):
        raise ValueError(f"accessor {accessor_index} is sparse; not supported")
    if "bufferView" not in acc:
        raise ValueError(f"accessor {accessor_index} has no bufferView")
    ncomp = _N_COMP.get(acc["type"])
    if ncomp is None:
        raise ValueError(f"accessor {accessor_index}: unknown type {acc['type']!r}")
    dtype = _DTYPE.get(acc["componentType"])
    if dtype is None:
        raise ValueError(
            f"accessor {accessor_index}: unsupported componentType {acc['componentType']}"
        )
    count = int(acc["count"])
    views = gltf["bufferViews"]
    view = views[int(acc["bufferView"])]
    if int(view.get("buffer", 0)) != 0:
        raise ValueError("only a single GLB BIN buffer is supported")
    item_size = int(np.dtype(dtype).itemsize) * ncomp
    stride = int(view["byteStride"]) if "byteStride" in view else item_size
    start = int(view.get("byteOffset", 0)) + int(acc.get("byteOffset", 0))
    if stride == item_size:
        nbytes = count * item_size
        buf = blob[start : start + nbytes]
        if len(buf) != nbytes:
            raise ValueError(f"accessor {accessor_index}: truncated bufferView")
        arr = np.frombuffer(buf, dtype=dtype)
    else:
        out = np.empty(count * ncomp, dtype=dtype)
        item = np.dtype(dtype).itemsize
        for i in range(count):
            off = start + i * stride
            raw = blob[off : off + item_size]
            if len(raw) != item_size:
                raise ValueError(f"accessor {accessor_index}: truncated stride at {i}")
            out[i * ncomp : (i + 1) * ncomp] = np.frombuffer(raw, dtype=dtype)
        arr = out
    if ncomp == 1:
        return arr[:count].copy()
    return arr[: count * ncomp].reshape(count, ncomp).copy()


@dataclass
class ProxyMesh:
    path: Path
    mesh_index: int
    mesh_name: str
    primitive_index: int
    node_index: int | None
    positions: np.ndarray  # (V, 3) float64 mesh-local bind pose
    world_positions: np.ndarray  # (V, 3) float64, node transform applied
    indices: np.ndarray  # (T, 3) uint32
    normals: np.ndarray | None
    joints: np.ndarray | None
    weights: np.ndarray | None
    morph_target_names: list[str]
    default_morph_weights: list[float]
    has_skin: bool
    warnings: list[str] = field(default_factory=list)

    @property
    def vertex_count(self) -> int:
        return int(self.positions.shape[0])

    @property
    def triangle_count(self) -> int:
        return int(self.indices.shape[0])


def select_primitive(
    gltf: dict[str, Any],
    mesh_index: int | None,
    primitive_index: int | None,
    mesh_name: str | None,
) -> tuple[int, int, dict[str, Any], dict[str, Any]]:
    meshes = gltf.get("meshes") or []
    if not meshes:
        raise ValueError("GLB contains no meshes")
    if mesh_name is not None:
        matches = [i for i, m in enumerate(meshes) if m.get("name") == mesh_name]
        if not matches:
            names = [m.get("name") for m in meshes]
            raise ValueError(f"no mesh named {mesh_name!r}; have {names}")
        mesh_index = matches[0]
    if mesh_index is None:
        skinned = []
        for node in gltf.get("nodes") or []:
            if node.get("mesh") is not None and node.get("skin") is not None:
                skinned.append(int(node["mesh"]))
        if len(set(skinned)) == 1:
            mesh_index = skinned[0]
        elif len(meshes) == 1:
            mesh_index = 0
        else:
            raise ValueError(
                f"GLB has {len(meshes)} meshes; pass --mesh-index or --mesh-name"
            )
    if mesh_index < 0 or mesh_index >= len(meshes):
        raise ValueError(f"mesh index {mesh_index} out of range (n={len(meshes)})")
    mesh = meshes[mesh_index]
    prims = mesh.get("primitives") or []
    if not prims:
        raise ValueError(f"mesh {mesh_index} has no primitives")
    if primitive_index is None:
        if len(prims) != 1:
            raise ValueError(
                f"mesh {mesh_index} has {len(prims)} primitives; pass --primitive"
            )
        primitive_index = 0
    if primitive_index < 0 or primitive_index >= len(prims):
        raise ValueError(f"primitive {primitive_index} out of range")
    return mesh_index, primitive_index, mesh, prims[primitive_index]


def _mesh_node_index(gltf: dict[str, Any], mesh_index: int) -> int | None:
    hits = [
        i
        for i, n in enumerate(gltf.get("nodes") or [])
        if n.get("mesh") == mesh_index
    ]
    if not hits:
        return None
    return hits[0]


def load_proxy_mesh(
    path: Path,
    mesh_index: int | None = None,
    primitive_index: int | None = None,
    mesh_name: str | None = None,
) -> ProxyMesh:
    gltf, blob = load_glb(path)
    mesh_index, primitive_index, mesh, prim = select_primitive(
        gltf, mesh_index, primitive_index, mesh_name
    )
    warnings: list[str] = []
    mode = prim.get("mode", 4)
    if mode != 4:
        raise ValueError(f"primitive mode {mode} is not TRIANGLES (4)")
    attrs = prim.get("attributes") or {}
    if "POSITION" not in attrs:
        raise ValueError("primitive has no POSITION attribute")
    if "indices" not in prim:
        raise ValueError("primitive has no indices; non-indexed meshes are not supported")
    positions = _accessor_numpy(gltf, blob, int(attrs["POSITION"])).astype(np.float64)
    if positions.ndim != 2 or positions.shape[1] != 3:
        raise ValueError(f"POSITION must be VEC3, got {positions.shape}")
    if positions.shape[0] == 0:
        raise ValueError("POSITION accessor is empty")
    raw_idx = _accessor_numpy(gltf, blob, int(prim["indices"])).astype(np.uint32)
    if raw_idx.size % 3 != 0:
        raise ValueError(f"index count {raw_idx.size} is not divisible by 3")
    indices = raw_idx.reshape(-1, 3)
    if indices.max(initial=0) >= positions.shape[0]:
        raise ValueError("index buffer references a vertex past POSITION count")

    normals = None
    if "NORMAL" in attrs:
        normals = _accessor_numpy(gltf, blob, int(attrs["NORMAL"])).astype(np.float64)
    else:
        warnings.append("missing NORMAL attribute; geometric triangle normals will be used")

    joints = weights = None
    if "JOINTS_0" in attrs:
        joints = _accessor_numpy(gltf, blob, int(attrs["JOINTS_0"]))
    else:
        warnings.append("missing JOINTS_0 (ok for bind-pose sampling)")
    if "WEIGHTS_0" in attrs:
        weights = _accessor_numpy(gltf, blob, int(attrs["WEIGHTS_0"])).astype(np.float64)
    else:
        warnings.append("missing WEIGHTS_0 (ok for bind-pose sampling)")

    targets = prim.get("targets") or []
    extras = mesh.get("extras") or {}
    morph_names = list(extras.get("targetNames") or [])
    default_weights = [float(w) for w in (mesh.get("weights") or [])]
    if targets:
        warnings.append(
            f"{len(targets)} morph targets present; sampling uses bind-pose POSITION only"
        )
        if any(abs(w) > 1e-8 for w in default_weights):
            warnings.append(
                "mesh default morph weights are non-zero and are not applied at bake time"
            )

    node_index = _mesh_node_index(gltf, mesh_index)
    world = (
        _world_matrix(gltf.get("nodes") or [], node_index)
        if node_index is not None
        else np.eye(4, dtype=np.float64)
    )
    if not np.allclose(world, np.eye(4), atol=1e-8):
        warnings.append("mesh node has a non-identity world transform; applied for ply dump")
    ones = np.ones((positions.shape[0], 1), dtype=np.float64)
    world_positions = (world @ np.hstack([positions, ones]).T).T[:, :3]

    has_skin = any(
        n.get("mesh") == mesh_index and n.get("skin") is not None
        for n in (gltf.get("nodes") or [])
    )
    if not has_skin:
        warnings.append("selected mesh is not skinned")

    return ProxyMesh(
        path=path,
        mesh_index=mesh_index,
        mesh_name=str(mesh.get("name") or f"mesh_{mesh_index}"),
        primitive_index=primitive_index,
        node_index=node_index,
        positions=positions,
        world_positions=world_positions,
        indices=indices,
        normals=normals,
        joints=joints,
        weights=weights,
        morph_target_names=morph_names,
        default_morph_weights=default_weights,
        has_skin=has_skin,
        warnings=warnings,
    )


# ---------------------------------------------------------------------------
# Fingerprint + sampling
# ---------------------------------------------------------------------------


def triangle_areas(positions: np.ndarray, indices: np.ndarray) -> np.ndarray:
    v0 = positions[indices[:, 0]]
    v1 = positions[indices[:, 1]]
    v2 = positions[indices[:, 2]]
    return 0.5 * np.linalg.norm(np.cross(v1 - v0, v2 - v0), axis=1)


def index_sha256(indices: np.ndarray) -> bytes:
    return hashlib.sha256(np.ascontiguousarray(indices, dtype=np.uint32).tobytes()).digest()


def position_sha256(positions: np.ndarray) -> bytes:
    return hashlib.sha256(
        np.ascontiguousarray(positions.astype(np.float32, copy=False)).tobytes()
    ).digest()


def combined_fingerprint(
    vertex_count: int, triangle_count: int, index_hash: bytes, position_hash: bytes
) -> bytes:
    payload = (
        struct.pack("<II", vertex_count, triangle_count) + index_hash + position_hash
    )
    return hashlib.sha256(payload).digest()


def _quantize_unorm16(x: np.ndarray) -> np.ndarray:
    q = np.rint(np.clip(x, 0.0, 1.0) * 65535.0).astype(np.int32)
    return np.clip(q, 0, 65535).astype(np.uint16)


def _quantize_snorm16(meters: float, scale: float) -> int:
    if scale <= 0:
        raise ValueError("offset_scale must be positive")
    n = meters / scale
    q = int(round(np.clip(n, -1.0, 1.0) * 32767.0))
    return int(np.clip(q, -32767, 32767))


def _decode_snorm16(q: int, scale: float) -> float:
    return (q / 32767.0) * scale


@dataclass
class BakeResult:
    mesh: ProxyMesh
    seed: int
    flags: int
    offset_scale: float
    normal_offset_m: float
    tangent_u_m: float
    tangent_v_m: float
    areas: np.ndarray
    valid_mask: np.ndarray
    triangle_index: np.ndarray
    bary_u: np.ndarray
    bary_v: np.ndarray
    normal_offset_q: int
    tangent_u_q: int
    tangent_v_q: int
    index_hash: bytes
    position_hash: bytes
    fingerprint: bytes

    @property
    def splat_count(self) -> int:
        return int(self.triangle_index.shape[0])

    @property
    def degenerate_count(self) -> int:
        return int((~self.valid_mask).sum())

    @property
    def valid_count(self) -> int:
        return int(self.valid_mask.sum())

    @property
    def surface_area(self) -> float:
        return float(self.areas[self.valid_mask].sum())


def sample_surface(
    mesh: ProxyMesh,
    *,
    seed: int,
    samples: int | None = None,
    per_triangle: int | None = None,
    density: float | None = None,
    normal_offset_m: float = 0.0,
    tangent_u_m: float = 0.0,
    tangent_v_m: float = 0.0,
    offset_scale: float = OFFSET_SCALE_M,
) -> BakeResult:
    if seed < 0 or seed > 0xFFFFFFFF:
        raise ValueError("seed must fit in uint32 (0 .. 4294967295)")
    modes = [samples is not None, per_triangle is not None, density is not None]
    if sum(modes) > 1:
        raise ValueError("pass only one of --samples, --per-triangle, --density")
    areas = triangle_areas(mesh.positions, mesh.indices)
    valid = areas > AREA_EPS
    valid_idx = np.nonzero(valid)[0]
    if valid_idx.size == 0:
        raise ValueError("no non-degenerate triangles to sample")
    valid_areas = areas[valid]

    if per_triangle is not None:
        if per_triangle < 1:
            raise ValueError("--per-triangle must be >= 1")
        n = int(per_triangle) * int(valid_idx.size)
        tri_ids = np.repeat(valid_idx, per_triangle)
        rng = np.random.Generator(np.random.PCG64(seed))
        r1 = rng.random(n)
        r2 = rng.random(n)
    else:
        if density is not None:
            if density <= 0:
                raise ValueError("--density must be > 0")
            n = int(round(float(density) * float(valid_areas.sum())))
        else:
            n = int(samples if samples is not None else DEFAULT_SAMPLES)
        if n < 1:
            raise ValueError("target sample count is < 1")
        rng = np.random.Generator(np.random.PCG64(seed))
        cdf = np.cumsum(valid_areas)
        total = float(cdf[-1])
        cdf = cdf / total
        u = rng.random(n)
        local = np.searchsorted(cdf, u, side="left")
        local = np.clip(local, 0, valid_idx.size - 1)
        tri_ids = valid_idx[local]
        r1 = rng.random(n)
        r2 = rng.random(n)

    q = np.sqrt(r1)
    beta1 = q * (1.0 - r2)
    beta2 = q * r2
    bary_u = _quantize_unorm16(beta1)
    bary_v = _quantize_unorm16(beta2)
    overflow = bary_u.astype(np.int32) + bary_v.astype(np.int32) > 65535
    if np.any(overflow):
        bary_v = bary_v.copy()
        bary_v[overflow] = (65535 - bary_u[overflow]).astype(np.uint16)

    order = np.lexsort((bary_v, bary_u, tri_ids))
    tri_ids = tri_ids[order].astype(np.uint32, copy=False)
    bary_u = bary_u[order]
    bary_v = bary_v[order]

    nq = _quantize_snorm16(normal_offset_m, offset_scale)
    tu = _quantize_snorm16(tangent_u_m, offset_scale)
    tv = _quantize_snorm16(tangent_v_m, offset_scale)

    idx_hash = index_sha256(mesh.indices)
    pos_hash = position_sha256(mesh.positions)
    fp = combined_fingerprint(mesh.vertex_count, mesh.triangle_count, idx_hash, pos_hash)
    return BakeResult(
        mesh=mesh,
        seed=seed,
        flags=DEFAULT_FLAGS,
        offset_scale=offset_scale,
        normal_offset_m=normal_offset_m,
        tangent_u_m=tangent_u_m,
        tangent_v_m=tangent_v_m,
        areas=areas,
        valid_mask=valid,
        triangle_index=tri_ids,
        bary_u=bary_u,
        bary_v=bary_v,
        normal_offset_q=nq,
        tangent_u_q=tu,
        tangent_v_q=tv,
        index_hash=idx_hash,
        position_hash=pos_hash,
        fingerprint=fp,
    )


def fingerprint_only(mesh: ProxyMesh) -> tuple[bytes, bytes, bytes, np.ndarray, np.ndarray]:
    areas = triangle_areas(mesh.positions, mesh.indices)
    valid = areas > AREA_EPS
    idx_hash = index_sha256(mesh.indices)
    pos_hash = position_sha256(mesh.positions)
    fp = combined_fingerprint(mesh.vertex_count, mesh.triangle_count, idx_hash, pos_hash)
    return fp, idx_hash, pos_hash, areas, valid


def reconstruct_positions(result: BakeResult, *, world: bool = True) -> tuple[np.ndarray, np.ndarray]:
    """Return (positions, normals) using quantized barycentrics, as the runtime would."""
    mesh = result.mesh
    pos = mesh.world_positions if world else mesh.positions
    tris = mesh.indices[result.triangle_index]
    v0 = pos[tris[:, 0]]
    v1 = pos[tris[:, 1]]
    v2 = pos[tris[:, 2]]
    b1 = result.bary_u.astype(np.float64) / 65535.0
    b2 = result.bary_v.astype(np.float64) / 65535.0
    b0 = np.maximum(0.0, 1.0 - b1 - b2)
    surface = b0[:, None] * v0 + b1[:, None] * v1 + b2[:, None] * v2
    e1 = v1 - v0
    e2 = v2 - v0
    n = np.cross(e1, e2)
    n_len = np.linalg.norm(n, axis=1, keepdims=True)
    n = np.divide(n, n_len, out=np.zeros_like(n), where=n_len > 0)
    t_u = e1.copy()
    t_len = np.linalg.norm(t_u, axis=1, keepdims=True)
    t_u = np.divide(t_u, t_len, out=np.zeros_like(t_u), where=t_len > 0)
    t_v = np.cross(n, t_u)
    t_v_len = np.linalg.norm(t_v, axis=1, keepdims=True)
    t_v = np.divide(t_v, t_v_len, out=np.zeros_like(t_v), where=t_v_len > 0)
    off_n = _decode_snorm16(result.normal_offset_q, result.offset_scale)
    off_u = _decode_snorm16(result.tangent_u_q, result.offset_scale)
    off_v = _decode_snorm16(result.tangent_v_q, result.offset_scale)
    points = surface + off_n * n + off_u * t_u + off_v * t_v
    return points, n


def reconstruction_error(result: BakeResult) -> tuple[float, float]:
    """Mean / max distance from reconstructed points to their parent triangle plane."""
    mesh = result.mesh
    pos = mesh.positions
    tris = mesh.indices[result.triangle_index]
    v0 = pos[tris[:, 0]]
    v1 = pos[tris[:, 1]]
    v2 = pos[tris[:, 2]]
    points, n = reconstruct_positions(result, world=False)
    signed = np.einsum("ij,ij->i", points - v0, n)
    expected = _decode_snorm16(result.normal_offset_q, result.offset_scale)
    err = np.abs(signed - expected)
    if err.size == 0:
        return 0.0, 0.0
    return float(err.mean()), float(err.max())


# ---------------------------------------------------------------------------
# Writers
# ---------------------------------------------------------------------------


def pack_header(result: BakeResult) -> bytes:
    header = bytearray(HEADER_SIZE)
    struct.pack_into(
        "<4s7If3I",
        header,
        0,
        MAGIC,
        VERSION,
        result.flags,
        result.splat_count,
        result.mesh.vertex_count,
        result.mesh.triangle_count,
        result.mesh.primitive_index,
        result.seed,
        float(result.offset_scale),
        RECORD_SIZE,
        HEADER_SIZE,
        result.degenerate_count,
    )
    header[64:96] = result.index_hash
    header[96:128] = result.position_hash
    return bytes(header)


def pack_records(result: BakeResult) -> bytes:
    n = result.splat_count
    rec = np.empty(
        n,
        dtype=[
            ("triangle_index", "<u4"),
            ("bary_u", "<u2"),
            ("bary_v", "<u2"),
            ("normal_offset", "<i2"),
            ("tangent_offset_u", "<i2"),
            ("tangent_offset_v", "<i2"),
            ("reserved", "<u2"),
        ],
    )
    rec["triangle_index"] = result.triangle_index
    rec["bary_u"] = result.bary_u
    rec["bary_v"] = result.bary_v
    rec["normal_offset"] = np.int16(result.normal_offset_q)
    rec["tangent_offset_u"] = np.int16(result.tangent_u_q)
    rec["tangent_offset_v"] = np.int16(result.tangent_v_q)
    rec["reserved"] = 0
    return rec.tobytes()


def write_splatbind(path: Path, result: BakeResult) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(pack_header(result) + pack_records(result))


def write_ply(path: Path, points: np.ndarray, normals: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    n = int(points.shape[0])
    header = (
        "ply\n"
        "format binary_little_endian 1.0\n"
        f"comment splatbind reconstructed samples\n"
        f"element vertex {n}\n"
        "property float x\n"
        "property float y\n"
        "property float z\n"
        "property float nx\n"
        "property float ny\n"
        "property float nz\n"
        "end_header\n"
    ).encode("ascii")
    body = np.empty(
        n,
        dtype=[
            ("x", "<f4"),
            ("y", "<f4"),
            ("z", "<f4"),
            ("nx", "<f4"),
            ("ny", "<f4"),
            ("nz", "<f4"),
        ],
    )
    body["x"] = points[:, 0]
    body["y"] = points[:, 1]
    body["z"] = points[:, 2]
    body["nx"] = normals[:, 0]
    body["ny"] = normals[:, 1]
    body["nz"] = normals[:, 2]
    path.write_bytes(header + body.tobytes())


def sidecar_dict(result: BakeResult, splatbind_path: Path, mean_err: float, max_err: float) -> dict[str, Any]:
    mesh = result.mesh
    return {
        "format": "SPLB",
        "version": VERSION,
        "glb": str(mesh.path),
        "mesh_name": mesh.mesh_name,
        "mesh_index": mesh.mesh_index,
        "primitive_index": mesh.primitive_index,
        "has_skin": mesh.has_skin,
        "vertex_count": mesh.vertex_count,
        "triangle_count": mesh.triangle_count,
        "valid_triangles": result.valid_count,
        "degenerate_skipped": result.degenerate_count,
        "splat_count": result.splat_count,
        "seed": result.seed,
        "flags": result.flags,
        "offset_scale_m": result.offset_scale,
        "normal_offset_m": result.normal_offset_m,
        "surface_area_m2": result.surface_area,
        "mean_plane_error_m": mean_err,
        "max_plane_error_m": max_err,
        "fingerprint_sha256": result.fingerprint.hex(),
        "index_sha256": result.index_hash.hex(),
        "position_sha256": result.position_hash.hex(),
        "record_size": RECORD_SIZE,
        "header_size": HEADER_SIZE,
        "morph_target_names": mesh.morph_target_names,
        "splatbind": str(splatbind_path),
        "notes_for_avatar_manifest": {
            "splatbind": str(splatbind_path.name),
            "topology_fingerprint": result.fingerprint.hex(),
            "proxy_glb": mesh.path.name,
        },
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _print_mesh_stats(mesh: ProxyMesh) -> None:
    print(f"glb              {mesh.path}")
    print(f"mesh             {mesh.mesh_index} ({mesh.mesh_name!r}) primitive {mesh.primitive_index}")
    print(f"vertices         {mesh.vertex_count}")
    print(f"triangles        {mesh.triangle_count}")
    print(f"skinned          {mesh.has_skin}")
    if mesh.morph_target_names:
        visemes = [n for n in mesh.morph_target_names if n.startswith("viseme_")]
        print(f"morph targets    {len(mesh.morph_target_names)} ({len(visemes)} visemes)")
    bbox_min = mesh.world_positions.min(axis=0)
    bbox_max = mesh.world_positions.max(axis=0)
    print(
        "world bbox       "
        f"[{bbox_min[0]:.4f}, {bbox_min[1]:.4f}, {bbox_min[2]:.4f}] .. "
        f"[{bbox_max[0]:.4f}, {bbox_max[1]:.4f}, {bbox_max[2]:.4f}]"
    )


def _print_warnings(warnings: list[str]) -> None:
    for w in warnings:
        print(f"warning          {w}")


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="bake_splatbind.py",
        description="Bake a barycentric Gaussian splat binding from a frozen proxy GLB.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="See the module docstring for the SPLB v1 binary layout.",
    )
    p.add_argument("--glb", type=Path, required=True, help="path to frozen proxy .glb")
    p.add_argument(
        "--out",
        type=Path,
        default=None,
        help="output .splatbind path (default: alongside the GLB)",
    )
    density = p.add_mutually_exclusive_group()
    density.add_argument(
        "--samples",
        type=int,
        default=None,
        help=f"total splat count (default: {DEFAULT_SAMPLES} when no density flag is set)",
    )
    density.add_argument(
        "--per-triangle",
        type=int,
        default=None,
        dest="per_triangle",
        help="fixed sample count per non-degenerate triangle",
    )
    density.add_argument(
        "--density",
        type=float,
        default=None,
        help="samples per square meter of bind-pose surface area",
    )
    p.add_argument(
        "--seed",
        type=int,
        default=DEFAULT_SEED,
        help=f"RNG seed for deterministic sampling (default: {DEFAULT_SEED})",
    )
    p.add_argument(
        "--normal-offset",
        type=float,
        default=0.0,
        dest="normal_offset",
        help="uniform displacement along triangle normal in meters (default: 0)",
    )
    p.add_argument(
        "--offset-scale",
        type=float,
        default=OFFSET_SCALE_M,
        dest="offset_scale",
        help=f"meters at snorm 1.0 (default: {OFFSET_SCALE_M})",
    )
    p.add_argument("--mesh-index", type=int, default=None, dest="mesh_index")
    p.add_argument("--mesh-name", type=str, default=None, dest="mesh_name")
    p.add_argument("--primitive", type=int, default=None)
    p.add_argument(
        "--fingerprint-only",
        action="store_true",
        help="compute and print the topology fingerprint without sampling",
    )
    p.add_argument(
        "--dry-run",
        action="store_true",
        help="sample and print stats without writing .splatbind",
    )
    p.add_argument(
        "--dump-ply",
        type=Path,
        default=None,
        help="write reconstructed sample positions as a binary PLY",
    )
    p.add_argument(
        "--no-sidecar",
        action="store_true",
        help="do not write the JSON sidecar next to --out",
    )
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    glb = args.glb.expanduser().resolve()
    if not glb.is_file():
        print(f"error: GLB not found: {glb}", file=sys.stderr)
        return 2
    try:
        mesh = load_proxy_mesh(
            glb,
            mesh_index=args.mesh_index,
            primitive_index=args.primitive,
            mesh_name=args.mesh_name,
        )
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as exc:
        print(f"error: failed to load {glb}: {exc}", file=sys.stderr)
        return 2

    _print_mesh_stats(mesh)
    _print_warnings(mesh.warnings)

    if args.fingerprint_only:
        fp, idx_hash, pos_hash, areas, valid = fingerprint_only(mesh)
        print(f"degenerate       {int((~valid).sum())} skipped")
        print(f"surface area     {float(areas[valid].sum()):.6f} m^2")
        print(f"index sha256     {idx_hash.hex()}")
        print(f"position sha256  {pos_hash.hex()}")
        print(f"fingerprint      {fp.hex()}")
        print("seed             (unused; fingerprint-only)")
        return 0

    try:
        result = sample_surface(
            mesh,
            seed=args.seed,
            samples=args.samples,
            per_triangle=args.per_triangle,
            density=args.density,
            normal_offset_m=args.normal_offset,
            offset_scale=args.offset_scale,
        )
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    mean_err, max_err = reconstruction_error(result)
    out = args.out
    if out is None:
        out = glb.with_suffix(".splatbind")
    else:
        out = out.expanduser().resolve()

    print(f"seed             {result.seed}")
    print(f"samples          {result.splat_count}")
    print(f"density          {result.splat_count / result.surface_area:.3f} / m^2")
    print(f"valid tris       {result.valid_count}")
    print(f"degenerate       {result.degenerate_count} skipped")
    print(f"surface area     {result.surface_area:.6f} m^2")
    print(f"normal offset    {result.normal_offset_m} m  (quantized {result.normal_offset_q})")
    print(f"offset scale     {result.offset_scale} m")
    print(f"mean plane err   {mean_err:.3e} m")
    print(f"max plane err    {max_err:.3e} m")
    print(f"index sha256     {result.index_hash.hex()}")
    print(f"position sha256  {result.position_hash.hex()}")
    print(f"fingerprint      {result.fingerprint.hex()}")

    if args.dump_ply is not None:
        points, normals = reconstruct_positions(result, world=True)
        ply = args.dump_ply.expanduser().resolve()
        write_ply(ply, points, normals)
        print(f"ply              {ply}")

    if args.dry_run:
        print("output           (dry-run; not written)")
        return 0

    write_splatbind(out, result)
    print(f"output           {out}")
    print(f"bytes            {HEADER_SIZE + RECORD_SIZE * result.splat_count}")
    if not args.no_sidecar:
        sidecar = Path(str(out) + ".json")
        sidecar.write_text(
            json.dumps(sidecar_dict(result, out, mean_err, max_err), indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"sidecar          {sidecar}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
