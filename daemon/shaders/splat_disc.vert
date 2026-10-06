#version 450
// One isotropic disc per projected splat. Six vertices per instance, packed
// into a single draw: vertex = slot * 6 + corner. The compute projection
// already wrote screen-space center, radius, depth, and the bake color.
// Patch UVs are local to this draw (avatar splats only).

struct ProjectedSplat {
    vec2 screen_center;
    float radius_pixels;
    float depth;
    uint splat_id;
    vec4 color;
};

struct Patch {
    uint region;
    float u;
    float v;
    float pad;
};

layout(std430, set = 0, binding = 0) readonly buffer Projected {
    ProjectedSplat splats[];
};

layout(std430, set = 0, binding = 1) readonly buffer Patches {
    Patch patches[];
};

layout(push_constant) uniform Push {
    vec2 extent;
    uint base_slot;
    uint flags; // bit 0: debug bake palette, do not sample captures
} pc;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) out vec2 v_patch_uv;
layout(location = 3) flat out uint v_region;

void main() {
    uint corner = uint(gl_VertexIndex) % 6u;
    uint local = uint(gl_VertexIndex) / 6u;
    uint slot = pc.base_slot + local;
    ProjectedSplat s = splats[slot];
    Patch p = patches[local];

    vec2 quad[6] = vec2[](
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)
    );
    vec2 c = quad[corner];
    v_uv = c;
    v_color = s.color;
    v_patch_uv = vec2(p.u, p.v);
    v_region = p.region;

    if (s.radius_pixels <= 0.5 || s.depth < 0.1) {
        gl_Position = vec4(2.0, 2.0, 0.0, 1.0);
        return;
    }

    vec2 pixel = s.screen_center + c * s.radius_pixels;
    vec2 ndc = vec2(
        pixel.x / pc.extent.x * 2.0 - 1.0,
        pixel.y / pc.extent.y * 2.0 - 1.0
    );
    float z = clamp((s.depth - 0.1) / 7.9, 0.0, 1.0);
    gl_Position = vec4(ndc, z, 1.0);
}
