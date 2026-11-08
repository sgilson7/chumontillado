#version 300 es
// One skinned character part. Bone matrices come from the animation texture
// the bake wrote (three texels per bone, a 3x4 matrix in rows; one texture row
// per frame). The core names the two frame rows and the blend between them,
// and the model matrix (uModel, 4 columns of 3).
precision highp float;
precision highp int;
in vec3 aPos;
in vec3 aNormal;
in vec2 aUV;
in uvec4 aJoints;
in vec4 aWeights;
uniform mat4 uVP;
uniform mat4x3 uModel;
uniform highp sampler2D uAnim;
uniform int uRow0;
uniform int uRow1;
uniform float uBlend;
out vec3 vWorld;
out vec3 vNormal;
out vec2 vUV;

mat4 bone(int row, uint joint) {
  int x = int(joint) + int(joint) + int(joint);
  vec4 r0 = texelFetch(uAnim, ivec2(x, row), 0);
  vec4 r1 = texelFetch(uAnim, ivec2(x + 1, row), 0);
  vec4 r2 = texelFetch(uAnim, ivec2(x + 1 + 1, row), 0);
  return mat4(r0.x, r1.x, r2.x, 0.0,
              r0.y, r1.y, r2.y, 0.0,
              r0.z, r1.z, r2.z, 0.0,
              r0.w, r1.w, r2.w, 1.0);
}

mat4 skinAt(int row) {
  return bone(row, aJoints.x) * aWeights.x + bone(row, aJoints.y) * aWeights.y
       + bone(row, aJoints.z) * aWeights.z + bone(row, aJoints.w) * aWeights.w;
}

void main() {
  mat4 s = skinAt(uRow0) * (1.0 - uBlend) + skinAt(uRow1) * uBlend;
  vec4 local = s * vec4(aPos, 1.0);
  vec3 world = uModel * vec4(local.xyz, 1.0);
  vNormal = mat3(uModel) * (mat3(s) * aNormal);
  vUV = aUV;
  vWorld = world;
  gl_Position = uVP * vec4(world, 1.0);
}
