#version 450
// Disc-sized present surface. The feed is the center square of the capture,
// fisheyed into the glass. Where that look lands inside our own disc, the
// sample would be a copy of this lens. Those fragments draw an aperture
// instead. The fisheye is the only warp.

layout(push_constant) uniform PC {
    vec2 overlay;     // layer size, pixels
    float yInvert;    // 1 when the dmabuf is y-inverted
    float time;       // seconds, for the breath
    vec2 origin;      // current top-left of the layer, screen pixels
    vec2 prevOrigin;  // previous top-left, so a glide does not leave an echo
    vec2 screen;      // output size, pixels
} pc;

layout(set = 0, binding = 0) uniform sampler2D tScreen;

struct HeatCell {
    float heat;
    float luma;
    float cb;
    float cr;
};
// Written by the seat dispatch. 192*108, one past the last real cell.
layout(std430, set = 1, binding = 0) buffer HeatSeat {
    HeatCell heat_cells[];
};
const uint SEAT = 192u * 108u;

layout(location = 0) out vec4 outColor;

// The capture is a photograph, about a tenth of a second old, and the ball
// glides at 280 px/s. This extra radius covers where that older disc can
// still be sitting in the picture. 280 * 0.28 s.
const float LAG_PX = 80.0;

float cover_radius() {
    return min(pc.overlay.x, pc.overlay.y) * 0.5 + LAG_PX;
}

// The glass only looks at the center square. On a landscape screen the
// square already spans the full height, so a bubble at the top or bottom
// is cut by exactly one disc-radius of inset: the lag cap sits off the
// square, and that is the amount of iris that should show. Past either
// side of the square the echo leaves the picture, but the iris stays on
// that same edge and keeps the bubble's vertical place. A portrait screen
// swaps the axes.
vec2 aperture_center(vec2 origin) {
    vec2 center = origin + pc.overlay * 0.5;
    float side = max(min(pc.screen.x, pc.screen.y), 1.0);
    vec2 square0 = (pc.screen - vec2(side)) * 0.5;
    float inset = min(pc.overlay.x, pc.overlay.y) * 0.5;
    vec2 lo = square0 + vec2(inset);
    vec2 hi = square0 + vec2(side) - vec2(inset);
    return clamp(center, lo, max(hi, lo));
}

bool in_our_disc(vec2 pixel, vec2 origin) {
    return length(pixel - aperture_center(origin)) <= cover_radius();
}

// Six straight plates. local is -1..1 across the widened disc.
// Each plate is a triangle cropped by the housing circle. Its outer
// corner is a fixed pin on that circle; the inner edge is one side of
// the opening. As the opening breathes, those pins stay put and the
// plates turn about them, overlapping the next plate as they close.
const float BLADES = 6.0;
const float TAU = 6.2831853;
const float HOUSING = 0.97;

float segdist(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a;
    float den = dot(ab, ab);
    float t = den < 1e-8 ? 0.0 : clamp(dot(p - a, ab) / den, 0.0, 1.0);
    return length(p - (a + t * ab));
}

vec3 aperture(vec2 local) {
    float rho = length(local);
    if (rho > 1.04) {
        return vec3(0.07);
    }
    // Enclosing ring. Everything outside it is the barrel, not a blade.
    float ring = min(abs(rho - HOUSING), abs(rho - 1.0));
    if (rho > HOUSING) {
        return mix(vec3(0.0), vec3(0.13, 0.13, 0.14), smoothstep(0.0, 0.012, ring));
    }

    // 0 is a quiet seat (open), 1 is stopped down. Eased on the GPU.
    float seat = clamp(heat_cells[SEAT].luma, 0.0, 1.0);
    float h = mix(0.50, 0.15, seat);
    h += 0.02 * sin(pc.time * 0.55);
    h = clamp(h, 0.11, 0.55);
    float sector = TAU / BLADES;
    float rv = h / cos(3.14159265 / BLADES);

    bool in_hole = true;
    for (int i = 0; i < 6; i++) {
        float mid = (float(i) + 0.5) * sector;
        if (dot(local, vec2(cos(mid), sin(mid))) > h) {
            in_hole = false;
        }
    }
    if (in_hole) {
        float core = exp(-dot(local, local) / max(h * h, 0.02) * 1.35);
        return vec3(1.0, 0.66, 0.12) * (0.30 + 1.15 * core);
    }

    float ang = atan(local.y, local.x);
    int mask = 0;
    float overlap = sector * 0.38;
    for (int i = 0; i < 6; i++) {
        float mid = (float(i) + 0.5) * sector;
        float d = mod(ang - mid + 3.14159265, TAU) - 3.14159265;
        if (abs(d) < sector * 0.5 + overlap) {
            mask |= 1 << i;
        }
    }
    int start = 0;
    bool found = false;
    for (int i = 0; i < 6; i++) {
        int prev = (i + 5) % 6;
        if ((mask & (1 << i)) != 0 && (mask & (1 << prev)) == 0) {
            start = i;
            found = true;
        }
    }
    if (!found) {
        float a = mod(ang, TAU);
        start = int(floor(a / sector)) % 6;
    }
    int w = start;
    int k = start;
    for (int n = 0; n < 6; n++) {
        if ((mask & (1 << k)) == 0) break;
        w = k;
        k = (k + 1) % 6;
    }

    // One metal. Nothing here depends on which plate won, so a seam
    // cannot be a light half against a dark half. The black edge is
    // the only line. The wash follows the disc, and it is continuous.
    vec3 col = vec3(0.58, 0.59, 0.62);
    float towardHole = smoothstep(HOUSING, max(h, 0.04), rho);
    col *= mix(0.86, 1.04, towardHole);
    float rake = clamp(dot(normalize(local), normalize(vec2(-0.2, 0.9))), 0.0, 1.0);
    col += vec3(0.045, 0.048, 0.05) * rake * rake;

    float aw = float(w) * sector;
    float an = float((w + 1) % 6) * sector;
    vec2 v0 = vec2(cos(aw), sin(aw)) * rv;
    vec2 v1 = vec2(cos(an), sin(an)) * rv;
    vec2 p0 = vec2(cos(aw + 0.55 * sector), sin(aw + 0.55 * sector)) * HOUSING;
    vec2 p1 = vec2(cos(an + 0.55 * sector), sin(an + 0.55 * sector)) * HOUSING;
    float bd = min(segdist(local, v0, p0), segdist(local, v1, p1));
    bd = min(bd, segdist(local, v0, v1));
    bd = min(bd, HOUSING - rho);
    return mix(vec3(0.0), col, smoothstep(0.0, 0.018, bd));
}

void main() {
    vec2 frag = vec2(gl_FragCoord.x, pc.overlay.y - gl_FragCoord.y);
    vec2 localUV = frag / max(pc.overlay, vec2(1.0));
    vec2 p = (localUV - 0.5) * 2.0;
    float r = length(p);
    if (r > 1.0) {
        outColor = vec4(0.0);
        return;
    }

    float theta = asin(clamp(r, 0.0, 1.0));
    float phi = atan(p.y, p.x);
    float dist = theta * 0.63662;
    vec2 uv = vec2(
        1.0 - (dist * cos(phi) * 0.5 + 0.5),
        dist * sin(phi) * 0.5 + 0.5
    );

    float side = max(min(pc.screen.x, pc.screen.y), 1.0);
    vec2 square0 = (pc.screen - vec2(side)) * 0.5;
    vec2 pixel = square0 + clamp(uv, vec2(0.0), vec2(1.0)) * side;
    vec2 texUv = pixel / max(pc.screen, vec2(1.0));
    if (pc.yInvert > 0.5) {
        texUv.y = 1.0 - texUv.y;
    }
    texUv = clamp(texUv, vec2(0.001), vec2(0.999));

    float rad = max(cover_radius(), 1.0);
    bool here = in_our_disc(pixel, pc.origin);
    bool was = in_our_disc(pixel, pc.prevOrigin);
    vec3 scene;
    if ((here || was) && pc.screen.x > 1.0) {
        vec2 local = (pixel - aperture_center(pc.origin)) / rad;
        scene = aperture(local);
    } else {
        scene = texture(tScreen, texUv).rgb;
        float rim = smoothstep(0.82, 1.0, r);
        scene = mix(scene, vec3(0.18, 0.62, 0.78), rim * 0.45);
    }
    float a = 1.0 - smoothstep(0.94, 1.0, r);
    outColor = vec4(scene * a, a);
}
