#version 450
// Sample the region's static screen tile. Constants match appearance.rs:
// 64px tiles, 16 columns, 2 pages, chrome mix 0.22 toward (0.78, 0.81, 0.84).
// FLAG_FACE = 1, FLAG_SHOWN = 2, FLAG_INCOMING = 4.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) in vec2 v_patch_uv;
layout(location = 3) flat in uint v_region;
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

void main() {
    float r = length(v_uv);
    if (r > 1.0) discard;

    vec3 rgb = v_color.rgb;
    if ((pc.flags & 1u) == 0u) {
        Region reg = regions[min(v_region, 15u)];
        bool shown = (reg.flags & 2u) != 0u;
        bool incoming = (reg.flags & 4u) != 0u;
        vec3 src = shown ? sample_tile(v_region, reg.shown_page, v_patch_uv) : v_color.rgb;
        vec3 dst = incoming ? sample_tile(v_region, reg.incoming_page, v_patch_uv) : src;
        rgb = mix(src, dst, reg.fade);
        if ((reg.flags & 1u) != 0u) {
            float captured = 0.0;
            if (shown) captured += 1.0 - reg.fade;
            if (incoming) captured += reg.fade;
            vec3 chromed = mix(rgb, CHROME, CHROME_MIX);
            rgb = mix(rgb, chromed, clamp(captured, 0.0, 1.0));
        }
    }

    float a = v_color.a * exp(-r * r * 2.2);
    outColor = vec4(rgb * a, a);
}
