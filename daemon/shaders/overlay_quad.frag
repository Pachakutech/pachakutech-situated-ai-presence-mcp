#version 450
// Pixel-space axis-aligned quad in the center of the overlay. Premultiplied
// alpha so Wayland's PRE_MULTIPLIED composite-alpha path blends correctly.
// Everything outside the quad is fully transparent — combined with an empty
// wl_region input, clicks fall through to whatever is underneath.

layout(push_constant) uniform PC {
    vec2 screen; // surface size in pixels
    vec2 quad;   // colored rectangle size in pixels
} pc;

layout(location = 0) out vec4 outColor;

void main() {
    vec2 c = pc.screen * 0.5;
    vec2 h = pc.quad * 0.5;
    vec2 d = abs(gl_FragCoord.xy - c) - h;
    float inside = float(d.x <= 0.0 && d.y <= 0.0);
    // Obvious "this is the presence overlay" marker — cyan glass, not opaque.
    vec3 rgb = vec3(0.15, 0.72, 0.88);
    float a = 0.40 * inside;
    outColor = vec4(rgb * a, a);
}
