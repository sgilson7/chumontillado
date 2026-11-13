#version 300 es
// T34: one lifelike figure's part, skinned. Espiochum's skinned pass (its
// data/shaders/skin.vert), changed for Chumontillado's split body: the core
// names two rows of the animation texture and the blend between them for the
// lower body, and two and a blend for the upper (lifelike.rs); a joint the
// texture's last row flags as above the boundary is drawn as the lower
// rows' boundary joint times its own upper matrix. Bone matrices are three
// texels a joint (a 3x4 matrix in rows), one texture row a frame. Positions
// are relative to the origin and drawn from the eye (render.rs). No number
// lives here.
precision highp float;
precision highp int;
in vec3 aPos;
in vec3 aNormal;
in vec2 aUV;
in uvec4 aJoints;
in vec4 aWeights;
uniform mat4 uVP;
uniform vec3 uEye;
uniform mat4x3 uModel;
uniform highp sampler2D uAnim;
uniform ivec3 uLower;   // row now, row next; and in .z the boundary joint
uniform ivec3 uUpper;   // row now, row next; and in .z the flags' row
uniform vec2 uBlend;    // the lower rows' blend, the upper rows'
out vec3 vWorld;
out vec3 vNormal;
out vec2 vUV;

mat4 bone(int row, int joint) {
  int x = joint + joint + joint;
  vec4 r0 = texelFetch(uAnim, ivec2(x, row), 0);
  vec4 r1 = texelFetch(uAnim, ivec2(x + 1, row), 0);
  vec4 r2 = texelFetch(uAnim, ivec2(x + 1 + 1, row), 0);
  return mat4(r0.x, r1.x, r2.x, 0.0,
              r0.y, r1.y, r2.y, 0.0,
              r0.z, r1.z, r2.z, 0.0,
              r0.w, r1.w, r2.w, 1.0);
}

mat4 lower(int joint) {
  return bone(uLower.x, joint) * (1.0 - uBlend.x) + bone(uLower.y, joint) * uBlend.x;
}

mat4 upper(int joint) {
  return bone(uUpper.x, joint) * (1.0 - uBlend.y) + bone(uUpper.y, joint) * uBlend.y;
}

mat4 joint(uint j, mat4 boundary) {
  int k = int(j);
  float above = texelFetch(uAnim, ivec2(k + k + k, uUpper.z), 0).x;
  if (above > 0.0) return boundary * upper(k);
  return lower(k);
}

void main() {
  mat4 b = lower(uLower.z);
  mat4 s = joint(aJoints.x, b) * aWeights.x + joint(aJoints.y, b) * aWeights.y
         + joint(aJoints.z, b) * aWeights.z + joint(aJoints.w, b) * aWeights.w;
  vec3 world = uModel * vec4((s * vec4(aPos, 1.0)).xyz, 1.0);
  vNormal = mat3(uModel) * (mat3(s) * aNormal);
  vUV = aUV;
  vWorld = world;
  gl_Position = uVP * vec4(world - uEye, 1.0);
}
