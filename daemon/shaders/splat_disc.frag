#version 450
// Sample the region's static screen tile, then shade that sample as glass.
// There is no live backdrop in this layer: the tile is the interior of the
// lens. Constants match appearance.rs.
// FLAG_FACE = 1, FLAG_SHOWN = 2, FLAG_INCOMING = 4.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) in vec2 v_patch_uv;
layout(location = 3) flat in uint v_region;
layout(location = 4) flat in vec2 v_anchor;
layout(location = 5) flat in uint v_id;
layout(location = 0) out vec4 outColor;

struct Region {
    float fade;
    uint shown_page;
    uint incoming_page;
    uint flags;
};

layout(std430, set = 0, binding = 2) readonly buffer Regions {
    Region regions[];
};

layout(set = 0, binding = 3) uniform sampler2D atlas;

layout(push_constant) uniform Push {
    vec2 extent;
    uint base_slot;
    uint flags;
    float time;
} pc;

const float TILE = 64.0;
const float ATLAS_W = 1024.0;
const float ATLAS_H = 128.0;
const vec3 CHROME = vec3(0.78, 0.81, 0.84);
const float CHROME_MIX = 0.22;

vec3 sample_tile(uint region, uint page, vec2 uv) {
    region = min(region, 15u);
    page = page & 1u;
    vec2 local = clamp(uv, vec2(0.0), vec2(1.0)) * (TILE - 1.0) + 0.5;
    vec2 px = vec2(float(region) * TILE, float(page) * TILE) + local;
    return texture(atlas, px / vec2(ATLAS_W, ATLAS_H)).rgb;
}

// Convex lens over one body region. The middle magnifies toward the region's
// center; the rim shears along the surface so the tile is not a flat stamp.
vec2 lens_uv(vec2 uv) {
    vec2 p = uv * 2.0 - 1.0;
    float rad = length(p);
    float inside = clamp(1.0 - rad, 0.0, 1.0);
    float rim = smoothstep(0.25, 1.0, rad);
    vec2 n = rad > 0.001 ? p / rad : vec2(0.0, 1.0);
    vec2 lens = mix(uv, vec2(0.5), 0.32 * inside * inside);
    lens += n * rim * 0.09;
    return lens;
}

vec2 chroma_of(vec2 uv) {
    vec2 p = uv * 2.0 - 1.0;
    float rad = length(p);
    float rim = smoothstep(0.40, 1.0, rad);
    vec2 n = rad > 0.001 ? p / rad : vec2(0.0);
    return n * rim * 0.032;
}

vec3 sample_glass(uint region, uint page, vec2 uv, vec2 chroma) {
    return vec3(
        sample_tile(region, page, uv + chroma).r,
        sample_tile(region, page, uv).g,
        sample_tile(region, page, uv - chroma).b
    );
}

void main() {
    float r = length(v_uv);
    if (r > 1.0) discard;

    vec2 p = v_patch_uv * 2.0 - 1.0;
    float rad = length(p);
    vec2 bent = lens_uv(v_patch_uv);
    vec2 chroma = chroma_of(v_patch_uv);

    vec3 rgb = v_color.rgb;
    if ((pc.flags & 1u) == 0u) {
        Region reg = regions[min(v_region, 15u)];
        bool shown = (reg.flags & 2u) != 0u;
        bool incoming = (reg.flags & 4u) != 0u;
        vec3 src = shown ? sample_glass(v_region, reg.shown_page, bent, chroma) : v_color.rgb;
        vec3 dst = incoming ? sample_glass(v_region, reg.incoming_page, bent, chroma) : src;
        rgb = mix(src, dst, reg.fade);
        if ((reg.flags & 1u) != 0u) {
            float captured = 0.0;
            if (shown) captured += 1.0 - reg.fade;
            if (incoming) captured += reg.fade;
            vec3 chromed = mix(rgb, CHROME, CHROME_MIX);
            rgb = mix(rgb, chromed, clamp(captured, 0.0, 1.0));
        }
    }

    // One light on the whole figure: brighter toward the upper left of the
    // layer, darker toward the lower right. Screen y grows downward.
    vec2 body = v_anchor - vec2(0.50, 0.32);
    float up = clamp(-body.y / 0.20, -1.0, 1.0);
    float side = clamp(-body.x / 0.18, -1.0, 1.0);
    float facing = clamp(0.50 + 0.36 * up + 0.16 * side, 0.0, 1.0);
    rgb *= mix(0.70, 1.16, facing);

    // Each region is also a pillow, so a limb rounds instead of staying flat.
    float nz = sqrt(clamp(1.0 - dot(p, p) * 0.55, 0.08, 1.0));
    vec3 N = normalize(vec3(p.x * 0.9, p.y * 0.9, nz));
    float wrap = clamp(dot(N, normalize(vec3(-0.35, 0.72, 0.60))) * 0.5 + 0.5, 0.0, 1.0);
    rgb *= mix(0.86, 1.10, wrap);
    rgb = mix(rgb, rgb * vec3(0.84, 0.91, 0.99) + vec3(0.03, 0.04, 0.06), 0.12);

    float bezel = smoothstep(0.55, 1.02, rad);
    rgb = mix(rgb, rgb * vec3(0.34, 0.40, 0.50), bezel * 0.42);
    float rim = smoothstep(0.68, 1.0, rad);
    rgb += vec3(0.82, 0.91, 1.0) * rim * 0.26;

    // Soft sheen plus a tight catchlight. Screen y grows downward.
    float dz = sqrt(clamp(1.0 - r * r, 0.0, 1.0));
    vec3 nDisc = normalize(vec3(v_uv.x, -v_uv.y, dz));
    vec3 H = normalize(normalize(vec3(-0.40, 0.78, 0.55)) + vec3(0.0, 0.0, 1.0));
    float ndh = clamp(dot(nDisc, H), 0.0, 1.0);
    float spec = pow(ndh, 14.0) * 0.22 + pow(ndh, 40.0) * 0.38;
    rgb += vec3(0.96, 0.98, 1.0) * spec;

    // One pale streak across the figure, not one per region.
    float travel = pc.time * 0.36;
    float cx = 0.50 + 0.16 * sin(travel) + 0.06 * sin(travel / 1.3);
    float cy = 0.32 + 0.12 * sin(travel * 0.47);
    vec2 q = vec2((v_anchor.x - cx) / 0.05, (v_anchor.y - cy) / 0.30);
    float glint = exp(-dot(q, q));
    rgb += vec3(1.0) * glint * 0.045;

    // A handful of points, each lit only at the peak of its own cycle.
    float star = 0.0;
    float hash = fract(sin(float(v_id) * 127.1) * 43758.5453);
    if (hash > 0.9994) {
        float phase = sin(pc.time * 3.1 + hash * 40.0);
        star = smoothstep(0.96, 1.0, phase) * smoothstep(0.22, 0.0, r);
    }
    rgb += vec3(1.0, 0.97, 0.90) * star * 0.65;

    float a = v_color.a * exp(-r * r * 2.2);
    outColor = vec4(rgb * a, a);
}
