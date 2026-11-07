#version 300 es
// T15: the painted (Kuwahara) filter, off unless the player turns it on.
// Each pixel takes the mean colour of whichever of the four squares meeting
// at it varies least in brightness, so flat patches like brushwork form and
// edges stay sharp. The squares' reach grows with the screen's height; the
// numbers are the core's (api::FILTER_HEIGHT_PER_RADIUS, LUMA).
precision highp float;

uniform sampler2D uScene;
// x: screen height per pixel of reach; y: the most reach
uniform vec4 uFilter;
// how much red, green and blue count toward brightness
uniform vec3 uLuma;

out vec4 outColor;

void square(ivec2 at, ivec2 toward, int reach, ivec2 last, inout float best, inout vec3 pick) {
  vec3 sum = vec3(0.0);
  float bright = 0.0;
  float bright2 = 0.0;
  float n = 0.0;
  float weight = uLuma.x + uLuma.y + uLuma.z;
  for (int j = 0; j <= reach; j++) {
    for (int i = 0; i <= reach; i++) {
      ivec2 p = clamp(at + toward * ivec2(i, j), ivec2(0), last);
      vec3 c = texelFetch(uScene, p, 0).rgb;
      float y = dot(c, uLuma) / weight;
      sum += c;
      bright += y;
      bright2 += y * y;
      n += 1.0;
    }
  }
  float mean = bright / n;
  float spread = bright2 / n - mean * mean;
  if (best < 0.0 || spread < best) {
    best = spread;
    pick = sum / n;
  }
}

void main() {
  ivec2 size = textureSize(uScene, 0);
  ivec2 at = ivec2(gl_FragCoord.xy);
  ivec2 last = size - ivec2(1);
  int reach = clamp(int(float(size.y) / uFilter.x), 1, int(uFilter.y));
  float best = -1.0;
  vec3 pick = vec3(0.0);
  square(at, ivec2(-1, -1), reach, last, best, pick);
  square(at, ivec2(1, -1), reach, last, best, pick);
  square(at, ivec2(-1, 1), reach, last, best, pick);
  square(at, ivec2(1, 1), reach, last, best, pick);
  outColor = vec4(pick, 1.0);
}
