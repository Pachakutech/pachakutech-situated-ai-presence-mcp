#version 450
// One isotropic disc per projected splat. Six vertices per instance, packed
// into a single draw: vertex = slot * 6 + corner. The compute projection
// already wrote screen-space center, radius, depth, and color.

struct ProjectedSplat {
    vec2 screen_center;
    float radius_pixels;
    float depth;
    uint splat_id;
    vec4 color;
};

layout(std430, set = 0, binding = 0) readonly buffer Projected {
    ProjectedSplat splats[];
};

layout(push_constant) uniform Push {
    vec2 extent;
    uint base_slot;
    uint _pad;
} pc;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;

void main() {
    uint corner = uint(gl_VertexIndex) % 6u;
    uint slot = pc.base_slot + uint(gl_VertexIndex) / 6u;
    ProjectedSplat s = splats[slot];

    vec2 quad[6] = vec2[](
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)
    );
    vec2 c = quad[corner];
    v_uv = c;
    v_color = s.color;

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
