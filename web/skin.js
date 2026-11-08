// T34: the skinned pass: the lifelike skins as meshes animated by baked
// clips. Espiochum's web/skin.js, changed for Chumontillado: a model is
// fetched only when the core first wants it (game.wanted(), a bit a skin),
// and the core is told when it is ready (mesh.ready), so the painted game
// starts as fast as before and a figure stays painted until its model is
// here. Each model's manifest (data/mesh/<id>.json) says where everything is
// in its files; the core says, per figure, which model, which rows and
// blends, the model matrix and the flash (game.actors(), laid out by
// numbers().actor). This file decides nothing: it fetches, uploads and draws.

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

// shaders: { vertex, fragment }; onReady(k): skin k's model is uploaded
export function makeSkin(gl, N, shaders, onReady) {
  const R = N.actor;
  const program = link(gl, shaders.vertex, shaders.fragment);
  const u = (name) => gl.getUniformLocation(program, name);
  const U = {
    vp: u("uVP"), eye: u("uEye"), model: u("uModel"), anim: u("uAnim"), lower: u("uLower"), upper: u("uUpper"),
    blend: u("uBlend"), tex: u("uTex"), tint: u("uTint"), cutout: u("uCutout"), flash: u("uFlash"),
  };
  const loc = (name) => gl.getAttribLocation(program, name);
  const A = {
    position: { at: loc("aPos"), type: gl.FLOAT, norm: false, int: false },
    normal: { at: loc("aNormal"), type: gl.BYTE, norm: true, int: false },
    uv: { at: loc("aUV"), type: gl.FLOAT, norm: false, int: false },
    joints: { at: loc("aJoints"), type: gl.UNSIGNED_BYTE, norm: false, int: true },
    weights: { at: loc("aWeights"), type: gl.UNSIGNED_BYTE, norm: true, int: false },
  };
  const wide = Uint32Array.BYTES_PER_ELEMENT;
  const binary = (path) => fetch(path).then((r) => r.arrayBuffer());
  const picture = (path) => fetch(path).then((r) => r.blob()).then((b) => createImageBitmap(b));

  const blank = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, blank);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([1, 1, 1, 1]));

  function texture(img) {
    const t = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    return t;
  }

  async function load(name) {
    const m = await (await fetch(`data/mesh/${name}.json`)).json();
    const [bin, anim, images] = await Promise.all([
      binary(m.bin),
      binary(m.anim),
      Promise.all(m.parts.map((p) => (p.texture ? picture(`mesh/tex/${p.texture}`) : null))),
    ]);
    const before = gl.getParameter(gl.VERTEX_ARRAY_BINDING);
    const animTex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, animTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, m.anim_width, m.anim_rows, 0, gl.RGBA, gl.HALF_FLOAT, new Uint16Array(anim));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    const parts = m.parts.map((p, i) => {
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
      gl.bufferData(gl.ARRAY_BUFFER, new Uint8Array(bin, p.offset, p.vertices * p.stride), gl.STATIC_DRAW);
      for (const [key, [size, offset]] of Object.entries(m.attrib)) {
        const a = A[key];
        if (a.at < 0) continue;
        gl.enableVertexAttribArray(a.at);
        if (a.int) gl.vertexAttribIPointer(a.at, size, a.type, p.stride, offset);
        else gl.vertexAttribPointer(a.at, size, a.type, a.norm, p.stride, offset);
      }
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, gl.createBuffer());
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, new Uint8Array(bin, p.index_offset, p.indices * p.index_bytes), gl.STATIC_DRAW);
      return {
        vao,
        count: p.indices,
        type: p.index_bytes === wide ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT,
        tex: images[i] ? texture(images[i]) : null,
        tint: new Float32Array(p.tint),
        cutout: p.alpha_test,
      };
    });
    gl.bindVertexArray(before);
    return { animTex, parts, boundary: m.boundary, flags: m.flags_row };
  }

  const models = [];
  const asked = new Set();
  let sceneValues = null;
  return {
    // the core's wish list: fetch each model wanted and not yet asked for
    want(mask) {
      R.models.forEach((name, k) => {
        if (!(mask & (1 << k)) || asked.has(k)) return;
        asked.add(k);
        load(name).then((m) => {
          models[k] = m;
          onReady(k);
        });
      });
    },
    setUniforms(values) {
      sceneValues = values;
    },
    // the records in one depth layer, with that layer's matrix and the eye
    draw(vp, eye, actors) {
      if (!actors.length) return;
      const prevProgram = gl.getParameter(gl.CURRENT_PROGRAM);
      const prevVao = gl.getParameter(gl.VERTEX_ARRAY_BINDING);
      gl.useProgram(program);
      if (sceneValues) {
        let at = 0;
        for (const [name, call, length, matrix] of N.uniforms) {
          const l = u(name);
          const data = sceneValues.subarray(at, at + length);
          if (l && matrix) gl[call](l, false, data);
          else if (l) gl[call](l, data);
          at += length;
        }
      }
      gl.disable(gl.BLEND);
      gl.enable(gl.DEPTH_TEST);
      gl.depthMask(true);
      gl.uniformMatrix4fv(U.vp, false, vp);
      gl.uniform3fv(U.eye, eye);
      gl.uniform1i(U.anim, 0);
      gl.uniform1i(U.tex, 1);
      for (let i = 0; i < actors.length; i += R.values) {
        const rec = actors.subarray(i, i + R.values);
        const m = models[rec[R.model]];
        if (!m) continue;
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, m.animTex);
        const [lowerNow, lowerNext, lowerBlend] = R.lower.map((k) => rec[k]);
        const [upperNow, upperNext, upperBlend] = R.upper.map((k) => rec[k]);
        gl.uniform3i(U.lower, lowerNow, lowerNext, m.boundary);
        gl.uniform3i(U.upper, upperNow, upperNext, m.flags);
        gl.uniform2f(U.blend, lowerBlend, upperBlend);
        gl.uniformMatrix4x3fv(U.model, false, rec.subarray(R.matrix[0], R.matrix[1]));
        gl.uniform4f(U.flash, ...rec.subarray(R.flash_rgb[0], R.flash_rgb[1]), rec[R.flash]);
        for (const p of m.parts) {
          if (p.cutout) gl.disable(gl.CULL_FACE);
          else {
            gl.enable(gl.CULL_FACE);
            gl.cullFace(gl.BACK);
          }
          gl.uniform1f(U.cutout, p.cutout ? 1 : 0);
          gl.activeTexture(gl.TEXTURE0 + 1);
          gl.bindTexture(gl.TEXTURE_2D, p.tex || blank);
          gl.uniform3fv(U.tint, p.tint);
          gl.bindVertexArray(p.vao);
          gl.drawElements(gl.TRIANGLES, p.count, p.type, 0);
        }
      }
      gl.activeTexture(gl.TEXTURE0);
      gl.bindVertexArray(prevVao);
      gl.useProgram(prevProgram);
    },
  };
}
