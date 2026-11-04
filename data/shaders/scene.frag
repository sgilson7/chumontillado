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
flat in vec3 vSize;
in vec3 vLocalNormal;
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
// T4, a surface's grit (look.rs, Grit): the rim's width (m) and darkness, a
// joint's width (m) and darkness
uniform vec4 uGrit;
// the mottle's cycles a metre and strength, the speckle's cells a metre and
// strength
uniform vec4 uGrit2;
// a tile, a brick's height and length, a plank's width (m)
uniform vec4 uCourse;
// each cell's own shade; a plank's grain along, across, and its strength
uniform vec4 uTint;
// the hash's two weights and its scale; a distance past any face (m)
uniform vec4 uHash;
// by material, one column each (the first materials in A, the rest in B,
// split where uMatSplit says): how much tile, brick and plank, and the
// pattern's scale
uniform mat4 uMaterialA;
uniform mat4 uMaterialB;
uniform float uMatSplit;
out vec4 outColor;

// T4: a value from 0 to 1 for a cell, the same every frame
float hash(vec2 p) {
  return fract(sin(dot(p, uHash.xy)) * uHash.z);
}

// smooth noise from 0 to 1 over cells of one unit
float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 u = smoothstep(0.0, 1.0, fract(p));
  vec2 x = vec2(1.0, 0.0);
  float a = mix(hash(i), hash(i + x), u.x);
  float b = mix(hash(i + x.yx), hash(i + x.xx), u.x);
  return mix(a, b, u.y);
}

// how far u is from the nearest multiple of period
float toLine(float u, float period) {
  return abs(fract(u / period + uK.x) - uK.x) * period;
}

// how dark a joint at distance d is when a pixel spans px: never thinner
// than a pixel, and lighter by as much as it was widened, so far off its
// average darkness stays true instead of shimmering
float joint(float d, float px) {
  float w = max(uGrit.z, px);
  return (1.0 - smoothstep(0.0, w, d)) * uGrit.w * min(uGrit.z / px, 1.0);
}

// how much of a pattern whose detail is `size` long to keep when a pixel
// spans px: all of it while the detail spans two pixels or more, none once
// it is under one (finer detail only aliases)
float keepAt(float size, float px) {
  return clamp(size / px - 1.0, 0.0, 1.0);
}

// T4: the brightness a map surface's texture gives this fragment: its
// material's tiles, bricks or planks, each cell its own shade, a mottle and
// a speckle over all, and a dark rim along the face's edges so where one
// surface ends shows.
float grit(vec3 n) {
  int k = int(vColor.a + uK.x);
  int split = int(uMatSplit + uK.x);
  vec4 m = k < split ? uMaterialA[k] : uMaterialB[k - split];
  vec3 an = abs(n);
  vec2 uv = an.y > max(an.x, an.z) ? vWorld.xz : (an.x > an.z ? vWorld.zy : vWorld.xy);
  uv *= m.w;
  // how far one pixel reaches across the surface here
  float px = length(fwidth(uv));
  // a pattern goes once its smallest cell is down to a few pixels
  float lineKeep = keepAt(uCourse.y * uK.x, px);
  // tiles
  float tile = joint(min(toLine(uv.x, uCourse.x), toLine(uv.y, uCourse.x)), px);
  float tileShade = (hash(floor(uv / uCourse.x)) - uK.x) * uTint.x;
  // bricks in running bond: each course set over by half a brick
  float course = floor(uv.y / uCourse.y);
  float along = uv.x + fract(course * uK.x) * uCourse.z;
  float brick = joint(min(toLine(uv.y, uCourse.y), toLine(along, uCourse.z)), px);
  float brickShade = (hash(vec2(floor(along / uCourse.z), course)) - uK.x) * uTint.x;
  // planks along u, with grain
  float row = floor(uv.y / uCourse.w);
  float plank = joint(toLine(uv.y, uCourse.w), px);
  float grain = (noise(vec2(uv.x * uTint.y, uv.y * uTint.z)) - uK.x) * uTint.w * keepAt(1.0 / uTint.z, px);
  float plankShade = (hash(vec2(row, row)) - uK.x) * uTint.x + grain;
  float lines = (m.x * tile + m.y * brick + m.z * plank) * lineKeep;
  float shade = m.x * tileShade + m.y * brickShade + m.z * plankShade;
  float mottle = (noise(uv * uGrit2.x) - uK.x) * uGrit2.y;
  float speck = (hash(floor(uv * uGrit2.z)) - uK.x) * uGrit2.w * keepAt(1.0 / uGrit2.z, px);
  // the rim: how near the face's edge, in metres along the face (the axis
  // the face looks along is ruled out)
  vec3 e = vSize * uK.x - abs(vLocal * vSize) + abs(vLocalNormal) * uHash.w;
  float rim = (1.0 - smoothstep(0.0, uGrit.x, min(e.x, min(e.y, e.z)))) * uGrit.y;
  return (1.0 + shade + mottle + speck) * (1.0 - lines) * (1.0 - rim);
}

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
  // T4: a map surface takes its grit
  if (uModeB.y > 0.0) base *= grit(n);
  float wrapped = clamp((dot(n, uLightDir) + uWrap.x) / (1.0 + uWrap.x), 0.0, 1.0);
  float key = mix(wrapped, smoothstep(uWrap.y, uWrap.z, wrapped), uWrap.w);
  vec3 fill = mix(uBounce, uSkyFill, n.y * uK.x + uK.x);
  vec3 lit = base * (fill + uSun * key);
  // the dither pass carries a fade in .a, a solid its glow, a surface its
  // material (unlit by none)
  float glow = (uModeA.w + uModeB.y) > 0.0 ? 0.0 : vColor.a;
  lit = mix(lit, base, glow);
  outColor = vec4(mix(lit, h.rgb, h.a), 1.0);
}
