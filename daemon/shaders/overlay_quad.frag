#version 450
// Disc-sized present surface; the sampled texture is the *full* screen
// capture (what actors also see). Fisheye maps that whole frame into the
// glass — same appearance as XR, where the input is the entire sensor.
// Layer position is independent: the compositor just moves this 220px
// surface; UV is always the full capture.

layout(push_constant) uniform PC {
    vec2 overlay; // this layer's pixel size (the disc)
} pc;

layout(set = 0, binding = 0) uniform sampler2D tScreen;
layout(location = 0) out vec4 outColor;

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
    uv = clamp(uv, vec2(0.001), vec2(0.999));

    vec3 scene = texture(tScreen, uv).rgb;
    float rim = smoothstep(0.82, 1.0, r);
    scene = mix(scene, vec3(0.18, 0.62, 0.78), rim * 0.45);
    float a = 1.0 - smoothstep(0.94, 1.0, r);
    outColor = vec4(scene * a, a);
}
