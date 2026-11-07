#version 300 es
// T15: the painted filter's pass covers the screen with two triangles; the
// corners come from the core (api::numbers, "post").
in vec2 aCorner;

void main() {
  gl_Position = vec4(aCorner, 0.0, 1.0);
}
