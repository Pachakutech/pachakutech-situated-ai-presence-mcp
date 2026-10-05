#version 450
layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 0) out vec4 outColor;

void main() {
    float r = length(v_uv);
    if (r > 1.0) discard;
    float a = v_color.a * exp(-r * r * 2.2);
    outColor = vec4(v_color.rgb * a, a);
}
