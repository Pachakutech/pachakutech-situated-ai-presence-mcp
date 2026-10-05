//! Tiny column-major 4x4 / quaternion helpers. Quaternions here are glTF order
//! (x, y, z, w); `GaussianSplat.rotation` is (w, x, y, z) and is converted at
//! the boundary in `deform.rs`.

pub type Mat4 = [f32; 16];
pub type Quat = [f32; 4];
pub const MAT4_IDENTITY: Mat4 = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
pub const QUAT_IDENTITY: Quat = [0., 0., 0., 1.];

pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            o[c * 4 + r] = s;
        }
    }
    o
}

pub fn quat_normalize(q: Quat) -> Quat {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n > 1e-12 && n.is_finite() {
        [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
    } else {
        QUAT_IDENTITY
    }
}

pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [x1, y1, z1, w1] = a;
    let [x2, y2, z2, w2] = b;
    [
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
    ]
}

pub fn quat_axis_angle(axis: [f32; 3], rad: f32) -> Quat {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt().max(1e-12);
    let (s, c) = (rad * 0.5).sin_cos();
    [axis[0] / n * s, axis[1] / n * s, axis[2] / n * s, c]
}

pub fn quat_slerp(a: Quat, mut b: Quat, t: f32) -> Quat {
    let mut d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if d < 0.0 {
        d = -d;
        b = [-b[0], -b[1], -b[2], -b[3]];
    }
    if d > 0.9995 {
        return quat_normalize([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]);
    }
    let th = d.clamp(-1.0, 1.0).acos();
    let (s0, s1) = (((1.0 - t) * th).sin() / th.sin(), (t * th).sin() / th.sin());
    quat_normalize([
        a[0] * s0 + b[0] * s1,
        a[1] * s0 + b[1] * s1,
        a[2] * s0 + b[2] * s1,
        a[3] * s0 + b[3] * s1,
    ])
}

pub fn mat4_from_trs(t: [f32; 3], q: Quat) -> Mat4 {
    let [x, y, z, w] = quat_normalize(q);
    let (xx, yy, zz, xy, xz, yz, wx, wy, wz) = (x * x, y * y, z * z, x * y, x * z, y * z, w * x, w * y, w * z);
    [
        1. - 2. * (yy + zz), 2. * (xy + wz), 2. * (xz - wy), 0.,
        2. * (xy - wz), 1. - 2. * (xx + zz), 2. * (yz + wx), 0.,
        2. * (xz + wy), 2. * (yz - wx), 1. - 2. * (xx + yy), 0.,
        t[0], t[1], t[2], 1.,
    ]
}

pub fn transform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// Rotation matrix given as three column vectors -> quaternion (x,y,z,w).
pub fn quat_from_columns(c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> Quat {
    let (m00, m10, m20) = (c0[0], c0[1], c0[2]);
    let (m01, m11, m21) = (c1[0], c1[1], c1[2]);
    let (m02, m12, m22) = (c2[0], c2[1], c2[2]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    };
    quat_normalize(q)
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// None when the vector is too short or non-finite.
pub fn normalize(a: [f32; 3]) -> Option<[f32; 3]> {
    let l = dot(a, a).sqrt();
    if l > 1e-9 && l.is_finite() {
        Some([a[0] / l, a[1] / l, a[2] / l])
    } else {
        None
    }
}
