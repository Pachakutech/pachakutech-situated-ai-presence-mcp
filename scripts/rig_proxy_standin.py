#!/usr/bin/env python3
"""Offline auto-rig for the topology-frozen humanoid proxy (numpy only).

Reads the un-rigged A-pose GLB, adds a skeleton, 4-influence LBS weights
(JOINTS_0 / WEIGHTS_0), inverse bind matrices and a handful of procedural
clips, and writes a rigged GLB. POSITION and the index buffer are left
bit-identical, so the existing .splatbind fingerprint stays valid (checked
at the end of this script against the .splatbind header).

This is a *proxy-grade* rig: joint positions are hand-placed landmarks read
off the mesh silhouettes, weights come from distance to bone segments. It is
good enough to drive an invisible deformation cage for Gaussian splats; it is
not a production character rig. The clips are procedural (sinusoids), not
mocap. Replace the GLB with a real rig later as long as the joint names in
assets/avatar_manifest.json are kept.

Conventions: meters, +Y up, character faces +Z, character-left = +X.

Usage: python scripts/rig_proxy.py [--in A.glb] [--out B.glb]
"""
import argparse, hashlib, json, math, os, struct, sys
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ASSETS = os.path.join(HERE, '..', 'assets')
CT = {5126: '<f4', 5125: '<u4', 5123: '<u2', 5121: 'u1'}
NC = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}

# name, parent, rest world position (x, y, z).  L = +X.
JOINTS = [
    ('root', None, (0, 0, 0)),
    ('pelvis', 'root', (0, 0.90, 0.03)),
    ('spine', 'pelvis', (0, 1.05, 0.03)),
    ('chest', 'spine', (0, 1.22, 0.03)),
    ('neck', 'chest', (0, 1.42, 0.04)),
    ('head', 'neck', (0, 1.46, 0.04)),
    ('jaw', 'head', (0, 1.53, 0.05)),
]
for side, s in (('L', 1.0), ('R', -1.0)):
    JOINTS += [
        (f'clavicle_{side}', 'chest', (s * 0.05, 1.38, 0.03)),
        (f'upperarm_{side}', f'clavicle_{side}', (s * 0.17, 1.35, 0.03)),
        (f'lowerarm_{side}', f'upperarm_{side}', (s * 0.33, 1.17, 0.04)),
        (f'hand_{side}', f'lowerarm_{side}', (s * 0.43, 1.05, 0.06)),
        (f'upperleg_{side}', 'pelvis', (s * 0.11, 0.86, 0.03)),
        (f'lowerleg_{side}', f'upperleg_{side}', (s * 0.19, 0.46, 0.03)),
        (f'foot_{side}', f'lowerleg_{side}', (s * 0.22, 0.08, 0.02)),
        (f'toes_{side}', f'foot_{side}', (s * 0.22, 0.02, 0.15)),
    ]
NAMES = [j[0] for j in JOINTS]
IDX = {n: i for i, n in enumerate(NAMES)}
POS = np.array([j[2] for j in JOINTS], dtype=np.float64)

# Weighted bone segments: (joint, tip position).  Skinning uses joint index.
def segs():
    P = lambda n: POS[IDX[n]]
    out = [('pelvis', P('pelvis'), P('spine')), ('spine', P('spine'), P('chest')),
           ('chest', P('chest'), P('neck')), ('neck', P('neck'), P('head')),
           ('head', P('head'), np.array([0, 1.67, 0.06]))]
    for s in 'LR':
        sg = 1.0 if s == 'L' else -1.0
        out += [(f'clavicle_{s}', P(f'clavicle_{s}'), P(f'upperarm_{s}')),
                (f'upperarm_{s}', P(f'upperarm_{s}'), P(f'lowerarm_{s}')),
                (f'lowerarm_{s}', P(f'lowerarm_{s}'), P(f'hand_{s}')),
                (f'hand_{s}', P(f'hand_{s}'), np.array([sg * 0.50, 0.94, 0.09])),
                (f'upperleg_{s}', P(f'upperleg_{s}'), P(f'lowerleg_{s}')),
                (f'lowerleg_{s}', P(f'lowerleg_{s}'), P(f'foot_{s}')),
                (f'foot_{s}', P(f'foot_{s}'), P(f'toes_{s}')),
                (f'toes_{s}', P(f'toes_{s}'), np.array([sg * 0.22, 0.0, 0.22]))]
    return out

def seg_dist(p, a, b):
    ab = b - a
    t = np.clip(((p - a) @ ab) / (ab @ ab), 0, 1)
    return np.linalg.norm(p - (a + t[:, None] * ab), axis=1)

def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)

def compute_weights(pos):
    S = segs()
    n = len(pos)
    W = np.zeros((n, len(NAMES)))
    for name, a, b in S:
        d = seg_dist(pos, a, b)
        w = 1.0 / (d * d + 1e-4) ** 2.5      # sharp falloff: nearest bone dominates
        side = name[-1] if name[-2] == '_' else None
        if side:  # gate limbs to their own side (allow a small midline overlap)
            sg = 1.0 if side == 'L' else -1.0
            if name.split('_')[0] in ('upperleg', 'lowerleg', 'foot', 'toes'):
                w = w * smoothstep(-0.03, 0.03, sg * pos[:, 0])
            elif name.split('_')[0] in ('upperarm', 'lowerarm', 'hand'):
                # arms live out at |x| > ~0.1; keeps the flank/chest from picking up arm weight
                w = w * smoothstep(0.09, 0.15, sg * pos[:, 0])
            else:
                w = w * smoothstep(-0.05, 0.05, sg * pos[:, 0])
        W[:, IDX[name]] += w
    # Jaw: lower face, in front of the ear axis, above the throat.
    y, z = pos[:, 1], pos[:, 2]
    jaw = smoothstep(1.505, 1.485, y) * smoothstep(0.07, 0.11, z) * smoothstep(1.41, 1.43, y)
    jaw = jaw * (np.abs(pos[:, 0]) < 0.075)
    W /= np.maximum(W.sum(1, keepdims=True), 1e-12)
    W *= (1.0 - jaw)[:, None]          # jaw takes its share from every other bone
    W[:, IDX['jaw']] += jaw
    # top-4, renormalize
    order = np.argsort(-W, axis=1)[:, :4]
    J = order.astype(np.uint8)
    Wt = np.take_along_axis(W, order, axis=1)
    Wt /= Wt.sum(1, keepdims=True)
    return J, Wt.astype(np.float32)

# ---- quaternion helpers (x, y, z, w) -----------------------------------
def qaxis(axis, deg):
    a = np.asarray(axis, float); a /= np.linalg.norm(a)
    h = math.radians(deg) / 2
    return np.array([*(a * math.sin(h)), math.cos(h)])
def qmul(a, b):
    x1, y1, z1, w1 = a; x2, y2, z2, w2 = b
    return np.array([w1*x2 + x1*w2 + y1*z2 - z1*y2, w1*y2 - x1*z2 + y1*w2 + z1*x2,
                     w1*z2 + x1*y2 - y1*x2 + z1*w2, w1*w2 - x1*x2 - y1*y2 - z1*z2])
QI = np.array([0, 0, 0, 1.0])

def unit(v): v = np.asarray(v, float); return v / np.linalg.norm(v)

def hinge_forward(side, bone_a, bone_b):
    """Axis that flexes the bone toward +Z (forward)."""
    a = unit(POS[IDX[bone_b]] - POS[IDX[bone_a]])
    return unit(np.cross(a, [0, 0, 1.0]))

# ---- clips: dict joint -> list of (t, quat); 'root' also may carry translation
def sample_clip(dur, n, fn):
    ts = [dur * i / n for i in range(n + 1)]
    rot = {}; trans = {}
    for t in ts:
        r, tr = fn(t / dur)
        for k, q in r.items(): rot.setdefault(k, []).append((t, q))
        for k, v in tr.items(): trans.setdefault(k, []).append((t, v))
    return rot, trans

def idle(u):
    s = math.sin(2 * math.pi * u)
    return ({'chest': qaxis([1, 0, 0], 1.2 * s), 'spine': qaxis([1, 0, 0], 0.8 * s),
             'head': qaxis([0, 1, 0], 3 * math.sin(2 * math.pi * u + 1.0)),
             'upperarm_L': qaxis([0, 0, 1], 1.0 * s), 'upperarm_R': qaxis([0, 0, 1], -1.0 * s)},
            {'pelvis': np.array([0, 0.90 + 0.004 * s, 0.03])})

def walk(u):
    th = 2 * math.pi * u
    r = {}; tr = {}
    for side, ph in (('L', 0.0), ('R', math.pi)):
        sw = math.sin(th + ph)
        r[f'upperleg_{side}'] = qaxis([1, 0, 0], -28 * sw)           # forward = negative about +X
        r[f'lowerleg_{side}'] = qaxis([1, 0, 0], 38 * max(0.0, math.sin(th + ph + 1.6)) + 4)
        r[f'foot_{side}'] = qaxis([1, 0, 0], 8 * sw)
        arm = hinge_forward(side, 'upperarm_' + side, 'lowerarm_' + side)
        r[f'upperarm_{side}'] = qaxis(arm, 22 * sw)                   # opposite phase to same-side leg
        r[f'lowerarm_{side}'] = qaxis(arm, 12 + 8 * sw)
    r['pelvis'] = qaxis([0, 1, 0], 5 * math.sin(th))
    r['chest'] = qaxis([0, 1, 0], -6 * math.sin(th))
    tr['pelvis'] = np.array([0, 0.90 + 0.018 * abs(math.cos(th)), 0.03])
    return r, tr

def static(rot):
    return lambda u: ({k: v for k, v in rot.items()}, {})

def side_arm(side='L'):
    return 'upperarm_' + side, 'lowerarm_' + side

def build_clips():
    clips = {}
    clips['idle'] = sample_clip(4.0, 16, idle)
    clips['walk'] = sample_clip(1.0, 16, walk)
    # static pose clips: ramp from rest to pose over 1 s, hold to 2 s
    def pose(name, rot):
        r = {}
        for k, q in rot.items():
            r[k] = [(0.0, QI), (1.0, q), (2.0, q)]
        clips[name] = (r, {})
    ax_fwd_L = hinge_forward('L', 'upperarm_L', 'lowerarm_L')
    pose('pose_arm_raise_90', {'upperarm_L': qaxis([1, 0, 0], -90)})
    # elbow: flex forearm toward +Z about the hinge axis perpendicular to the arm
    a = unit(POS[IDX['hand_L']] - POS[IDX['lowerarm_L']])
    pose('pose_elbow_flex_120', {'lowerarm_L': qaxis(np.cross(a, [0, 0, 1.0]), 120)})
    pose('pose_forearm_twist_170', {'lowerarm_L': qaxis(a, 170)})
    pose('pose_head_yaw_pitch', {'head': qmul(qaxis([0, 1, 0], 40), qaxis([1, 0, 0], 20))})
    pose('pose_jaw_open', {'jaw': qaxis([1, 0, 0], 25)})
    # pointing with character-right arm (-X): forward raise then yaw about Y
    for nm, yaw in (('left', 45), ('center', 0), ('right', -45)):
        q = qmul(qaxis([0, 1, 0], yaw), qaxis([1, 0, 0], -85))
        pose(f'pose_point_{nm}', {'upperarm_R': q})
    return clips

# ---- GLB io -------------------------------------------------------------
def read_glb(path):
    d = open(path, 'rb').read()
    cl, = struct.unpack('<I', d[12:16]); j = json.loads(d[20:20 + cl])
    bo = 20 + cl + 8
    bl, = struct.unpack('<I', d[20 + cl:24 + cl])
    return j, bytearray(d[bo:bo + bl])

def acc_read(j, b, i):
    a = j['accessors'][i]; n = NC[a['type']]; bv = j['bufferViews'][a['bufferView']]
    o = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
    return np.frombuffer(bytes(b), dtype=CT[a['componentType']], count=a['count'] * n, offset=o).reshape(a['count'], n)

class Builder:
    def __init__(self, j, b): self.j, self.b = j, b
    def add(self, arr, ctype, typ, count, target=None, minmax=False):
        while len(self.b) % 4: self.b.append(0)
        off = len(self.b); raw = arr.tobytes(); self.b += raw
        bv = {'buffer': 0, 'byteOffset': off, 'byteLength': len(raw)}
        if target: bv['target'] = target
        self.j['bufferViews'].append(bv)
        a = {'bufferView': len(self.j['bufferViews']) - 1, 'componentType': ctype, 'count': count, 'type': typ}
        if minmax: a['min'] = arr.reshape(count, -1).min(0).tolist(); a['max'] = arr.reshape(count, -1).max(0).tolist()
        self.j['accessors'].append(a); return len(self.j['accessors']) - 1

def write_glb(path, j, b):
    while len(b) % 4: b.append(0)
    j['buffers'][0]['byteLength'] = len(b)
    js = json.dumps(j, separators=(',', ':')).encode()
    while len(js) % 4: js += b' '
    total = 12 + 8 + len(js) + 8 + len(b)
    with open(path, 'wb') as f:
        f.write(struct.pack('<4sII', b'glTF', 2, total))
        f.write(struct.pack('<I4s', len(js), b'JSON')); f.write(js)
        f.write(struct.pack('<I4s', len(b), b'BIN\0')); f.write(b)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--in', dest='inp', default=os.path.join(ASSETS, 'humanoid_proxy_a_pose.glb'))
    ap.add_argument('--out', default=os.path.join(ASSETS, 'humanoid_proxy_rigged.glb'))
    ap.add_argument('--splatbind', default=os.path.join(ASSETS, 'humanoid_proxy_a_pose.splatbind'))
    ap.add_argument('--manifest', default=os.path.join(ASSETS, 'avatar_manifest.json'))
    a = ap.parse_args()
    j, b = read_glb(a.inp)
    prim = j['meshes'][0]['primitives'][0]
    pos = acc_read(j, b, prim['attributes']['POSITION']).astype(np.float64)
    pos_bytes_before = bytes(b[:])  # for bit-identity check
    J, W = compute_weights(pos)
    bc = Builder(j, b)
    jacc = bc.add(np.concatenate([J, np.zeros((len(J), 0), np.uint8)], 1).astype(np.uint8), 5121, 'VEC4', len(J), 34962)
    wacc = bc.add(W, 5126, 'VEC4', len(W), 34962)
    prim['attributes']['JOINTS_0'] = jacc; prim['attributes']['WEIGHTS_0'] = wacc
    ibm = np.zeros((len(NAMES), 16), np.float32)
    for i, p in enumerate(POS):
        m = np.eye(4); m[:3, 3] = -p; ibm[i] = m.T.reshape(16)   # column-major
    ibacc = bc.add(ibm, 5126, 'MAT4', len(NAMES))
    # nodes: existing node 0 = mesh; joints appended
    base = len(j['nodes'])
    for i, (n, par, p) in enumerate(JOINTS):
        lp = p if par is None else tuple(np.array(p) - POS[IDX[par]])
        node = {'name': n, 'translation': [float(x) for x in lp]}
        j['nodes'].append(node)
    for i, (n, par, p) in enumerate(JOINTS):
        if par: j['nodes'][base + IDX[par]].setdefault('children', []).append(base + i)
    j['nodes'][0]['skin'] = 0
    j['skins'] = [{'name': 'proxy_skin', 'inverseBindMatrices': ibacc, 'skeleton': base,
                   'joints': [base + i for i in range(len(NAMES))]}]
    j['scenes'][0]['nodes'].append(base)
    anims = []
    for name, (rot, trans) in build_clips().items():
        samplers, channels = [], []
        for k, keys in rot.items():
            ts = np.array([t for t, _ in keys], np.float32); qs = np.array([q for _, q in keys], np.float32)
            ia = bc.add(ts, 5126, 'SCALAR', len(ts), minmax=True); oa = bc.add(qs, 5126, 'VEC4', len(qs))
            samplers.append({'input': ia, 'output': oa, 'interpolation': 'LINEAR'})
            channels.append({'sampler': len(samplers) - 1, 'target': {'node': base + IDX[k], 'path': 'rotation'}})
        for k, keys in trans.items():
            ts = np.array([t for t, _ in keys], np.float32)
            vs = np.array([v - POS[IDX[JOINTS[IDX[k]][1]]] for _, v in keys], np.float32)
            ia = bc.add(ts, 5126, 'SCALAR', len(ts), minmax=True); oa = bc.add(vs, 5126, 'VEC3', len(vs))
            samplers.append({'input': ia, 'output': oa, 'interpolation': 'LINEAR'})
            channels.append({'sampler': len(samplers) - 1, 'target': {'node': base + IDX[k], 'path': 'translation'}})
        anims.append({'name': name, 'samplers': samplers, 'channels': channels})
    j['animations'] = anims
    # explicit zero default morph weights: the splatbind is baked on the *base* POSITION
    j['meshes'][0]['weights'] = [0.0] * len(j['meshes'][0]['weights'])
    # bit-identity of POSITION + indices
    assert bytes(b[:len(pos_bytes_before)]) == pos_bytes_before
    write_glb(a.out, j, b)
    # fingerprint check against the .splatbind header
    j2, b2 = read_glb(a.out)
    p2 = acc_read(j2, b2, j2['meshes'][0]['primitives'][0]['attributes']['POSITION']).astype('<f4')
    idx = acc_read(j2, b2, j2['meshes'][0]['primitives'][0]['indices']).astype('<u4')
    hdr = open(a.splatbind, 'rb').read(128)
    ok_i = hashlib.sha256(idx.tobytes()).digest() == hdr[64:96]
    ok_p = hashlib.sha256(p2.tobytes()).digest() == hdr[96:128]
    print('index sha ok:', ok_i, ' position sha ok:', ok_p)
    if not (ok_i and ok_p): sys.exit('fingerprint mismatch: rig step altered topology/positions')
    man = {
        'asset': os.path.basename(a.out), 'splatbind': os.path.basename(a.splatbind),
        'units': 'meters', 'up': '+Y', 'forward': '+Z', 'left': '+X', 'rest_pose': 'A',
        'ground_offset_y': float(pos[:, 1].min()),
        'note_ground': 'soles sit at y=ground_offset_y in the mesh; runtime subtracts it so soles land on y=0',
        'root_joint': 'root', 'jaw_joint': 'jaw', 'jaw_axis': [1, 0, 0], 'jaw_max_open_deg': 25.0,
        'joints': NAMES,
        'mesh_morph_default_weights_policy': 'force_zero (splatbind baked on base POSITION)',
        'viseme_morphs': {},
        'viseme_morphs_note': 'The source GLB has no viseme targets; the 6 present are MPFB body-shape macros. Populate after re-export (MPFB visemes02 pack).',
        'clips': list(build_clips().keys()),
        'rig_quality': 'proxy-grade: hand-placed landmarks, distance-based weights, procedural clips',
    }
    json.dump(man, open(a.manifest, 'w'), indent=2)
    print('wrote', a.out, 'and', a.manifest, '| joints', len(NAMES), '| clips', len(anims))

if __name__ == '__main__':
    main()
