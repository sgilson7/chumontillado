#version 300 es
// T34: a lifelike figure's part, in the painted room's light: the scene's
// sun wrapped past the terminator, its sky fill and ground bounce, and its
// haze with distance (scene.frag's, with the same uniforms from the core).
// No ink: the person asked for very realistic. Colour from the part's
// picture times its tint; brows, lashes and hair are cut out by alpha. A hit
// mixes it toward the flash's colour, as a painted figure's parts are
// repainted. No number lives here.
precision highp float;
in vec3 vWorld;
in vec3 vNormal;
in vec2 vUV;
uniform vec3 uEye;
uniform float uQ;
uniform vec3 uLightDir;
uniform vec3 uSun;
uniform vec3 uSkyFill;
uniform vec3 uBounce;
uniform vec3 uHorizon;
uniform vec3 uZenith;
uniform vec3 uSunGlow;
uniform vec4 uHazeShape;
uniform vec4 uHazeEdge;
uniform vec3 uAerial;
uniform vec4 uAerialK;
uniform vec4 uWrap;
uniform vec4 uSky;
uniform vec4 uK;
uniform sampler2D uTex;
uniform vec3 uTint;
uniform float uCutout;
uniform vec4 uFlash;    // the flash's colour, and in .a how far toward it
out vec4 outColor;

vec3 sky(vec3 dir) {
  float up = clamp(dir.y, 0.0, 1.0);
  vec3 c = mix(uHorizon, uZenith, pow(up, uSky.x));
  float s = max(dot(dir, uLightDir), 0.0);
  return c + uSunGlow * pow(s, uSky.w) * (1.0 - up);
}

vec4 haze() {
  vec3 to = vWorld - uEye;
  float d = length(to);
  vec3 flat_dir = normalize(vec3(to.x, 0.0, to.z) + vec3(0.0, uQ, 0.0));
  float f = min(1.0 - exp(-max(d - uHazeShape.x, 0.0) * uHazeShape.y * uQ), uHazeShape.z);
  f = max(f, smoothstep(uHazeEdge.x, uHazeEdge.y, d));
  vec3 c = mix(uHorizon, sky(flat_dir), uHazeShape.w);
  float k = smoothstep(uAerialK.x, uAerialK.y, d) * (1.0 - smoothstep(uAerialK.z, uAerialK.w, d));
  c = mix(c, uAerial, k * uHazeEdge.z);
  return vec4(c, f);
}

void main() {
  vec4 t = texture(uTex, vUV);
  if (uCutout > 0.0 && t.a < uK.x) discard;
  vec3 base = t.rgb * uTint;
  vec3 n = normalize(vNormal);
  if (!gl_FrontFacing) n = -n;
  float wrapped = clamp((dot(n, uLightDir) + uWrap.x) / (1.0 + uWrap.x), 0.0, 1.0);
  float key = mix(wrapped, smoothstep(uWrap.y, uWrap.z, wrapped), uWrap.w);
  vec3 fill = mix(uBounce, uSkyFill, n.y * uK.x + uK.x);
  vec3 lit = mix(base * (fill + uSun * key), uFlash.rgb, uFlash.a);
  vec4 h = haze();
  outColor = vec4(mix(lit, h.rgb, h.a), 1.0);
}
