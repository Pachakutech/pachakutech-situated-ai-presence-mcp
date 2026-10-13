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
layout(location = 0) out vec4 outColor;

// The capture is a photograph, about a tenth of a second old, and the ball
// glides at 280 px/s. This extra radius covers where that older disc can
// still be sitting in the picture. 280 * 0.28 s.
const float LAG_PX = 80.0;

float cover_radius() {
    return min(pc.overlay.x, pc.overlay.y) * 0.5 + LAG_PX;
}

bool in_our_disc(vec2 pixel, vec2 origin) {
    vec2 c = origin + pc.overlay * 0.5;
    return length(pixel - c) <= cover_radius();
}

// Six separate blades. local is -1..1 across the widened disc.
// The opening breathes, and the blades widen as they close so the
// gaps between them narrow.
vec3 aperture(vec2 local) {
    float openR = 0.28 + 0.18 * sin(pc.time * 0.55);
    float close = 1.0 - smoothstep(0.12, 0.46, openR);
    const float n = 6.0;
    float seg = 6.2831853 / n;
    float ang = atan(local.y, local.x);
    float fold = mod(ang + seg * 0.5, seg) - seg * 0.5;
    float rho = length(local);
    float halfAng = seg * (0.30 + 0.14 * close);

    float fromSide = halfAng - abs(fold);
    float fromInner = rho - openR;
    float fromOuter = 0.98 - rho;
    float side = smoothstep(0.0, 0.035, fromSide);
    float radial = smoothstep(0.0, 0.025, fromInner) * smoothstep(0.0, 0.02, fromOuter);
    float body = side * radial;
    float edge = body * (1.0 - smoothstep(0.0, 0.07, min(fromSide * 1.4, min(fromInner, fromOuter))));

    vec3 col = vec3(0.0);
    col = mix(col, vec3(0.58, 0.59, 0.62), body);
    col = mix(col, vec3(0.0), edge);

    float hole = 1.0 - smoothstep(openR - 0.015, openR + 0.02, rho);
    float core = exp(-rho * rho / max(openR * openR, 0.02) * 1.35);
    vec3 glow = vec3(1.0, 0.66, 0.12) * (0.30 + 1.15 * core);
    return mix(col, glow, hole);
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
        vec2 local = (pixel - (pc.origin + pc.overlay * 0.5)) / rad;
        scene = aperture(local);
    } else {
        scene = texture(tScreen, texUv).rgb;
        float rim = smoothstep(0.82, 1.0, r);
        scene = mix(scene, vec3(0.18, 0.62, 0.78), rim * 0.45);
    }
    float a = 1.0 - smoothstep(0.94, 1.0, r);
    outColor = vec4(scene * a, a);
}
