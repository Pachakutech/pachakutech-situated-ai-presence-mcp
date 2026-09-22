#version 450
// Fullscreen triangle covering the layer-shell surface. The fragment
// shader draws the actual quad in pixel space so we don't need a
// vertex buffer or aspect-ratio math here.

void main() {
    vec2 pos = vec2(
        float((gl_VertexIndex << 1) & 2),
        float(gl_VertexIndex & 2)
    );
    gl_Position = vec4(pos * 2.0 - 1.0, 0.0, 1.0);
}
