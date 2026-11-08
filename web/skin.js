// The skinned pass: Big Chum, the soldiers, Corporal Grace and the mule as
// meshes animated by baked clips. Each model's manifest (data/mesh/*.json)
// says where everything is in its files; the core says, per actor, which
// model, which two animation rows and how far between them, the model matrix,
// and whether a rifle is drawn (game.actors(), laid out by numbers().actor).
// This file decides nothing: it uploads and draws.

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
  return shader;
}

function link(gl, vertex, fragment) {
  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vertex));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragment));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
  return program;
}

// files: { json(path), binary(path), image(path) }, each fetched from this origin
export async function makeSkin(gl, N, shaders, files, look) {
  // the scene renderer binds its vertex array once; leave it bound as found
  const sceneVao = gl.getParameter(gl.VERTEX_ARRAY_BINDING);
  const program = link(gl, shaders.vertex, shaders.fragment);
  const u = (name) => gl.getUniformLocation(program, name);
  const U = {
    vp: u("uVP"), model: u("uModel"), anim: u("uAnim"), row0: u("uRow0"), row1: u("uRow1"), blend: u("uBlend"),
    tex: u("uTex"), hasTex: u("uHasTex"), tint: u("uTint"), alpha: u("uAlphaTest"), eye: u("uEye"), look: u("uLook"),
  };
  const sceneUniforms = N.uniforms.map(([name, size]) => ({ loc: u(name), size }));
  const loc = (name) => gl.getAttribLocation(program, name);
  const A = {
    position: { at: loc("aPos"), type: gl.FLOAT, norm: false, int: false },
    normal: { at: loc("aNormal"), type: gl.BYTE, norm: true, int: false },
    uv: { at: loc("aUV"), type: gl.FLOAT, norm: false, int: false },
    joints: { at: loc("aJoints"), type: gl.UNSIGNED_BYTE, norm: false, int: true },
    weights: { at: loc("aWeights"), type: gl.UNSIGNED_BYTE, norm: true, int: false },
  };
  const wide = Uint32Array.BYTES_PER_ELEMENT;

  async function texture(path) {
    const img = await files.image(path);
    const t = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.SRGB8_ALPHA8, gl.RGBA, gl.UNSIGNED_BYTE, img);
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    return t;
  }

  async function model(name) {
    const m = await files.json(`data/mesh/${name}.json`);
    const [bin, anim] = await Promise.all([files.binary(m.bin), files.binary(m.anim)]);
    const animTex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, animTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, m.anim_width, m.anim_rows, 0, gl.RGBA, gl.HALF_FLOAT, new Uint16Array(anim));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    const parts = [];
    for (const p of m.parts) {
      if (p.vertices === 0) continue;
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      const vb = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, vb);
      gl.bufferData(gl.ARRAY_BUFFER, new Uint8Array(bin, p.offset, p.vertices * p.stride), gl.STATIC_DRAW);
      for (const [key, [size, offset]] of Object.entries(m.attrib)) {
        const a = A[key];
        if (a.at < 0) continue;
        gl.enableVertexAttribArray(a.at);
        if (a.int) gl.vertexAttribIPointer(a.at, size, a.type, p.stride, offset);
        else gl.vertexAttribPointer(a.at, size, a.type, a.norm, p.stride, offset);
      }
      const ib = gl.createBuffer();
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ib);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, new Uint8Array(bin, p.index_offset, p.indices * p.index_bytes), gl.STATIC_DRAW);
      gl.bindVertexArray(sceneVao);
      parts.push({
        vao,
        count: p.indices,
        type: p.index_bytes === wide ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT,
        tex: p.texture ? await texture(`mesh/tex/${p.texture}`) : null,
        tint: new Float32Array(p.tint),
        cutout: p.alpha_test,
        rifle: p.rifle,
      });
    }
    return { animTex, parts };
  }

  const models = await Promise.all(N.actor.models.map(model));
  const blank = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, blank);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([1, 1, 1, 1]));

  let sceneValues = null;
  return {
    setUniforms(values) {
      sceneValues = values;
    },
    draw(mats, actors) {
      if (!actors.length) return;
      const prevProgram = gl.getParameter(gl.CURRENT_PROGRAM);
      const prevVao = gl.getParameter(gl.VERTEX_ARRAY_BINDING);
      gl.useProgram(program);
      gl.disable(gl.STENCIL_TEST);
      gl.disable(gl.BLEND);
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LESS);
      gl.depthMask(true);
      gl.colorMask(true, true, true, true);
      gl.uniformMatrix4fv(U.vp, false, mats.subarray(0, N.view_values));
      gl.uniform3fv(U.eye, mats.subarray(N.view_values, N.view_values + N.eye_values));
      gl.uniform4fv(U.look, look);
      if (sceneValues) {
        let at = 0;
        for (const s of sceneUniforms) {
          if (s.loc) gl[`uniform${s.size}fv`](s.loc, sceneValues.subarray(at, at + s.size));
          at += s.size;
        }
      }
      gl.uniform1i(U.anim, 0);
      gl.uniform1i(U.tex, 1);
      const R = N.actor;
      for (const cutouts of [false, true]) {
        if (cutouts) gl.disable(gl.CULL_FACE);
        else {
          gl.enable(gl.CULL_FACE);
          gl.cullFace(gl.BACK);
        }
        gl.uniform1f(U.alpha, cutouts ? 1 : 0);
        for (let i = 0; i < actors.length; i += R.values) {
          const rec = actors.subarray(i, i + R.values);
          const m = models[rec[R.model]];
          gl.activeTexture(gl.TEXTURE0);
          gl.bindTexture(gl.TEXTURE_2D, m.animTex);
          gl.uniform1i(U.row0, rec[R.row]);
          gl.uniform1i(U.row1, rec[R.next]);
          gl.uniform1f(U.blend, rec[R.blend]);
          gl.uniformMatrix4x3fv(U.model, false, rec.subarray(R.matrix[0], R.matrix[1]));
          const armed = rec[R.armed] > 0;
          for (const p of m.parts) {
            if (p.cutout !== cutouts || (p.rifle && !armed)) continue;
            gl.activeTexture(gl.TEXTURE0 + 1);
            gl.bindTexture(gl.TEXTURE_2D, p.tex || blank);
            gl.uniform1f(U.hasTex, p.tex ? 1 : 0);
            gl.uniform3fv(U.tint, p.tint);
            gl.bindVertexArray(p.vao);
            gl.drawElements(gl.TRIANGLES, p.count, p.type, 0);
          }
        }
      }
      gl.activeTexture(gl.TEXTURE0);
      gl.bindVertexArray(prevVao);
      gl.useProgram(prevProgram);
    },
  };
}
