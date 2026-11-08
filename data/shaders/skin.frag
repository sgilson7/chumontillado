#version 300 es
// The new style's light on a character part: a low sun (wrapped a little,
// so skin and cloth soften past the terminator), a sky fill from above, a
// warm bounce from the ground, a cool rim against the sky, and the scene's
// fog. Colour from the part's texture (sRGB, sampled linear) times its tint,
// or the tint alone. Hair, brows and lashes are cut out by alpha.
precision highp float;
in vec3 vWorld;
in vec3 vNormal;
in vec2 vUV;
uniform sampler2D uTex;
uniform float uHasTex;
uniform vec3 uTint;
uniform float uAlphaTest;
uniform vec3 uLightDir;
uniform vec3 uSky;
uniform vec3 uFog;
uniform vec3 uLight;
uniform vec2 uFogRange;
uniform vec3 uEye;
uniform vec4 uLook;   // wrap, sky fill, ground bounce, rim
out vec4 outColor;

vec3 toLinear(vec3 c) { return pow(c, vec3(2.2)); }

void main() {
  vec4 t = texture(uTex, vUV);
  if (uAlphaTest > 0.5 && t.a < 0.5) discard;
  vec3 tint = toLinear(uTint);
  vec3 base = mix(tint, t.rgb * tint, uHasTex);
  vec3 n = normalize(vNormal);
  if (!gl_FrontFacing) n = -n;
  vec3 l = normalize(uLightDir);
  vec3 v = normalize(uEye - vWorld);
  float wrap = uLook.x;
  float sun = max((dot(n, l) + wrap) / (1.0 + wrap), 0.0);
  float up = n.y * 0.5 + 0.5;
  vec3 sky = toLinear(uSky);
  vec3 sunCol = toLinear(uLight);
  vec3 lit = base * (sunCol * sun + sky * uLook.y * up + sunCol * uLook.z * (1.0 - up));
  float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * uLook.w;
  lit += sky * rim;
  float d = length(uEye - vWorld);
  float f = clamp((d - uFogRange.x) / max(uFogRange.y - uFogRange.x, 1.0), 0.0, 1.0);
  vec3 c = mix(lit, toLinear(uFog), f);
  outColor = vec4(pow(c, vec3(1.0 / 2.2)), 1.0);
}
