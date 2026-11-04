#version 300 es
precision highp float;
// The passes (the core says which, as flags in uModeA and uModeB): the sky,
// coloured by direction; solids, lit as Protection of the Spheres lights
// them (light wrapped past the terminator with a gentle ramp, a sky fill
// above and a ground bounce below, no hard shadows); dithered solids, the
// same on the pixels the ordered dither keeps; ink, the outline, flat; soft
// shadows, blended; and halos, added. Far things fade into the colour of the
// sky behind them (Iron Vector's haze). No number lives here: every constant
// is a uniform from the core (crates/core/src/look.rs, sky.rs). CHUMWORLD's
// (d9cb5b5), less its grass, troughs and wind (SECOND-ORDER.md R2.6).
in vec3 vWorld;
in vec3 vNormal;
in vec4 vColor;
in vec3 vLocal;
flat in float vFade;
uniform vec3 uEye;
uniform vec4 uModeA;
uniform vec4 uModeB;
uniform float uQ;
uniform vec3 uLightDir;
uniform vec3 uSun;
uniform vec3 uSkyFill;
uniform vec3 uBounce;
uniform vec3 uHorizon;
uniform vec3 uZenith;
uniform vec3 uSunGlow;
// where the haze starts (m), how thick it is per metre (times ONE), the most
// it hides, how far the sun's glow tints it
uniform vec4 uHazeShape;
// (sky.rs): where the haze is made whole (m, m), the tint's share; the
// tint; where it rises, is whole, falls, is gone (m)
uniform vec4 uHazeEdge;
uniform vec3 uAerial;
uniform vec4 uAerialK;
// light wrapped past the terminator, the ramp's start and end, how much of the ramp
uniform vec4 uWrap;
// the sky's curve, the sun disc's inner and outer cosines, the sun glow's power
uniform vec4 uSky;
// a half, the halo's power, how far ink fades into the haze, the shadow's strength
uniform vec4 uK;
// the ordered dither's thresholds, column x, row y, and its size
uniform mat4 uBayer;
uniform float uBayerSize;
out vec4 outColor;

// The sky's colour looking along dir: the horizon's colour rising to the
// zenith's, warmed toward the sun.
vec3 sky(vec3 dir) {
  float up = clamp(dir.y, 0.0, 1.0);
  vec3 c = mix(uHorizon, uZenith, pow(up, uSky.x));
  float s = max(dot(dir, uLightDir), 0.0);
  return c + uSunGlow * pow(s, uSky.w) * (1.0 - up);
}

// The haze's colour and share at this fragment: the sky at the horizon
// behind it.
vec4 haze() {
  vec3 to = vWorld - uEye;
  float d = length(to);
  vec3 flat_dir = normalize(vec3(to.x, 0.0, to.z) + vec3(0.0, uQ, 0.0));
  float f = min(1.0 - exp(-max(d - uHazeShape.x, 0.0) * uHazeShape.y * uQ), uHazeShape.z);
  // whole before the far plane; tinted cool over the middle distance
  f = max(f, smoothstep(uHazeEdge.x, uHazeEdge.y, d));
  vec3 c = mix(uHorizon, sky(flat_dir), uHazeShape.w);
  float k = smoothstep(uAerialK.x, uAerialK.y, d) * (1.0 - smoothstep(uAerialK.z, uAerialK.w, d));
  c = mix(c, uAerial, k * uHazeEdge.z);
  return vec4(c, f);
}

// Whether the ordered dither keeps this pixel at fade f (look::keeps).
bool keep(float f) {
  vec2 p = mod(floor(gl_FragCoord.xy), uBayerSize);
  float t = uBayer[int(p.x)][int(p.y)];
  return f >= 0.0 ? t < f : t >= f + 1.0;
}

void main() {
  if (uModeA.x > 0.0) {
    vec3 dir = normalize(vWorld - uEye);
    vec3 c = sky(dir);
    float s = dot(dir, uLightDir);
    c = mix(c, uSun + uSunGlow, smoothstep(uSky.z, uSky.y, s));
    outColor = vec4(c, 1.0);
    return;
  }
  if ((uModeA.y + uModeA.w) > 0.0 && !keep(vFade)) discard;
  vec4 h = haze();
  vec3 base = vColor.rgb;
  if (uModeA.y > 0.0) {
    outColor = vec4(mix(base, h.rgb, h.a * uK.z), 1.0);
    return;
  }
  vec3 n = normalize(vNormal);
  if (uModeA.z > 0.0) {
    vec3 v = normalize(uEye - vWorld);
    float a = pow(abs(dot(n, v)), uK.y) * vColor.a * (1.0 - h.a);
    outColor = vec4(vColor.rgb * a, 1.0);
    return;
  }
  if (uModeB.x > 0.0) {
    float r = length(vLocal.xz) / uK.x;
    float a = uK.w * vColor.a * clamp(1.0 - r * r, 0.0, 1.0) * (1.0 - h.a);
    outColor = vec4(vColor.rgb, a);
    return;
  }
  float wrapped = clamp((dot(n, uLightDir) + uWrap.x) / (1.0 + uWrap.x), 0.0, 1.0);
  float key = mix(wrapped, smoothstep(uWrap.y, uWrap.z, wrapped), uWrap.w);
  vec3 fill = mix(uBounce, uSkyFill, n.y * uK.x + uK.x);
  vec3 lit = base * (fill + uSun * key);
  // the dither pass carries a fade in .a, a solid its glow
  float glow = uModeA.w > 0.0 ? 0.0 : vColor.a;
  lit = mix(lit, base, glow);
  outColor = vec4(mix(lit, h.rgb, h.a), 1.0);
}
