#version 300 es
// One shape: the mesh vertex placed by the instance's three columns and
// translation (iA, iB, iC; translation in .w), coloured by iD (rgb, and in
// .a the pass's own value), in metres, as floats from the shim. No number
// lives here: the pass flags and the biases are uniforms from the core
// (crates/core/src/look.rs, render.rs). CHUMWORLD's (d9cb5b5), less its
// scatter's bands, its grass and its wind (SECOND-ORDER.md R2.6).
in vec3 aPos;
in vec3 aNormal;
// the vertex's own colour: white on a primitive, a stone's colour on the ring
in vec3 aColor;
in vec4 iA;
in vec4 iB;
in vec4 iC;
in vec4 iD;
// the layer's projection times the camera's turn; positions arrive
// relative to the eye (camera-relative drawing, crates/core/src/render.rs)
uniform mat4 uVP;
uniform vec3 uEye;
uniform float uQ;
// the pass, as flags of 0 and 1: sky, ink, glow, dither; then shade
uniform vec4 uModeA;
uniform vec4 uModeB;
// the ink hull pushed back, halos and shadows pulled forward, as share of
// clip w, times the core's ONE (so scaled by uQ here): a tie in depth with a
// solid then always goes the solid's way, or the halo's (crates/core/src/flicker.rs)
uniform vec2 uBias;

out vec3 vWorld;
// the dither's fade for the whole triangle, taken from its last vertex
flat out float vFade;
out vec3 vNormal;
out vec4 vColor;
out vec3 vLocal;

void main() {
  vec3 a = iA.xyz;
  vec3 b = iB.xyz;
  vec3 c = iC.xyz;
  vec3 t = vec3(iA.w, iB.w, iC.w);
  vec3 p = aPos;
  vec3 n = aNormal;
  vec3 w = a * p.x + b * p.y + c * p.z + t;
  // normals take the inverse transpose, so a sheared shape (a ramp's
  // wedge, Chumontillado M1) lights as it is drawn
  vNormal = transpose(inverse(mat3(a, b, c))) * n;
  vWorld = w;
  vColor = vec4(iD.rgb * aColor, iD.a);
  vLocal = p;
  vFade = iD.a;
  gl_Position = uVP * vec4(w - uEye, 1.0);
  gl_Position.z += (uModeA.y * uBias.x - (uModeA.z + uModeB.x) * uBias.y) * uQ * gl_Position.w;
  // the sky dome: never clipped by the near or the far plane
  if (uModeA.x > 0.0) gl_Position.z = 0.0;
}
