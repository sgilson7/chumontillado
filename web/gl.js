// The WebGL 2 renderer, CHUMWORLD's (d9cb5b5) less its water program, after
// Long Time Comin's and Protection of the Spheres' (instanced, one draw per
// mesh and pass). It uploads what the core returns and draws it; every
// size, stride, offset, pass state and uniform name comes from the core's
// numbers(), so this file holds no layout and no rule of its own. Meshes and
// instances arrive as floats from the shim.
//
// The draw list names a depth layer for each draw: when the layer changes,
// the depth buffer is cleared and that layer's matrix set (the core draws
// far things first, then near ones; crates/core/src/render.rs).
//
// T15: when the core says the painted filter is on, the scene is drawn into
// a picture the size of the canvas, and that picture is drawn to the screen
// through the filter (data/shaders/post.frag).

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader));
  }
  return shader;
}

function link(gl, vertexSource, fragmentSource) {
  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vertexSource));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragmentSource));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program));
  }
  return program;
}

// Sets a list of uniforms, [name, call, length, matrix] each, from values.
function setList(gl, program, list, values) {
  let at = 0;
  for (const [name, call, length, matrix] of list) {
    const loc = gl.getUniformLocation(program, name);
    const data = values.subarray(at, at + length);
    if (matrix) gl[call](loc, false, data);
    else gl[call](loc, data);
    at += length;
  }
}

// Returns null when the browser has no WebGL 2; the page then says so.
export function makeRenderer(canvas, N, shaders, meshList, meshData) {
  const gl = canvas.getContext("webgl2", { antialias: true, alpha: false, powerPreference: "high-performance", preserveDrawingBuffer: true });
  if (!gl) return null;

  // a CPU drawing in place of a graphics card: the core then draws fewer pixels
  const info = gl.getExtension("WEBGL_debug_renderer_info");
  const name = info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "";
  const software = shaders.software.some((s) => name.includes(s));

  const vao = gl.createVertexArray();
  gl.bindVertexArray(vao);

  const program = link(gl, shaders.vertex, shaders.fragment);
  gl.useProgram(program);
  const attribute = ([attr, size, offset]) => ({ loc: gl.getAttribLocation(program, attr), size, offset });
  const vertexAttributes = N.vertex.attributes.map(attribute).filter((a) => a.loc !== -1);
  const instanceAttributes = N.instance.attributes.map(attribute).filter((a) => a.loc !== -1);
  for (const a of vertexAttributes.concat(instanceAttributes)) gl.enableVertexAttribArray(a.loc);
  for (const a of instanceAttributes) gl.vertexAttribDivisor(a.loc, 1);
  for (const a of vertexAttributes) gl.vertexAttribDivisor(a.loc, 0);
  const uVP = gl.getUniformLocation(program, "uVP");
  const uEye = gl.getUniformLocation(program, "uEye");
  // T6: where the positions are measured from, for the surfaces' texture
  const uOrigin = gl.getUniformLocation(program, "uOrigin");
  const uModeA = gl.getUniformLocation(program, "uModeA");
  const uModeB = gl.getUniformLocation(program, "uModeB");

  // T15: the painted filter's program, its corners and its picture
  const post = link(gl, shaders.postVertex, shaders.postFragment);
  const postVao = gl.createVertexArray();
  gl.bindVertexArray(postVao);
  const [cornerName, cornerSize] = N.post.attribute;
  const corner = gl.getAttribLocation(post, cornerName);
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(N.post.corners), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(corner);
  gl.vertexAttribPointer(corner, cornerSize, gl.FLOAT, false, 0, 0);
  gl.useProgram(post);
  for (const [name, call, values] of N.post.uniforms) gl[call](gl.getUniformLocation(post, name), values);
  gl.uniform1i(gl.getUniformLocation(post, "uScene"), 0);
  gl.bindVertexArray(vao);
  gl.useProgram(program);
  const picture = { fbo: null, texture: null, depth: null, width: 0, height: 0 };

  // the picture the scene is drawn into, remade when the canvas changes size
  function pictureFor(width, height) {
    if (picture.fbo && picture.width === width && picture.height === height) return picture.fbo;
    if (picture.fbo) {
      gl.deleteFramebuffer(picture.fbo);
      gl.deleteTexture(picture.texture);
      gl.deleteRenderbuffer(picture.depth);
    }
    picture.texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, picture.texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    picture.depth = gl.createRenderbuffer();
    gl.bindRenderbuffer(gl.RENDERBUFFER, picture.depth);
    gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH_COMPONENT24, width, height);
    picture.fbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, picture.fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, picture.texture, 0);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, picture.depth);
    picture.width = width;
    picture.height = height;
    return picture.fbo;
  }

  const meshes = meshList.map((m) => {
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, meshData(m.id), gl.STATIC_DRAW);
    return { buffer, vertices: m.vertices };
  });

  const buffers = [gl.createBuffer(), gl.createBuffer()];
  const passes = [];
  for (const p of N.passes) passes[p.id] = p;
  let clear = [0, 0, 0];

  // the GL state a pass needs, as the core describes it
  function enterPass(p) {
    gl.uniform4fv(uModeA, p.mode_a);
    gl.uniform4fv(uModeB, p.mode_b);
    if (p.blend) {
      gl.enable(gl.BLEND);
      gl.blendFunc(gl[p.blend[0]], gl[p.blend[1]]);
    } else {
      gl.disable(gl.BLEND);
    }
    gl.depthMask(p.depth_write);
    if (p.depth_test) gl.enable(gl.DEPTH_TEST);
    else gl.disable(gl.DEPTH_TEST);
    if (p.cull) {
      gl.enable(gl.CULL_FACE);
      gl.cullFace(gl[p.cull]);
    } else {
      gl.disable(gl.CULL_FACE);
    }
  }

  return {
    software,
    name,
    // The light, the sky and the dither: uploaded once.
    setUniforms(values, clearColor) {
      setList(gl, program, N.uniforms, values);
      clear = clearColor;
    },
    // What never moves: uploaded again only when the core's origin moves.
    setStatic(values) {
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[N.buffers.static]);
      gl.bufferData(gl.ARRAY_BUFFER, values, gl.STATIC_DRAW);
    },
    resize(width, height) {
      canvas.width = width;
      canvas.height = height;
    },
    frame(view, values, draws, filtered) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, filtered ? pictureFor(canvas.width, canvas.height) : null);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[N.buffers.dynamic]);
      gl.bufferData(gl.ARRAY_BUFFER, values, gl.STREAM_DRAW);
      gl.viewport(0, 0, canvas.width, canvas.height);
      const [r, g, b] = clear;
      gl.clearColor(r, g, b, 1);
      gl.depthMask(true);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.uniform3fv(uEye, view.subarray(...N.view.eye));
      gl.uniform3fv(uOrigin, view.subarray(...N.view.origin));
      let layer = -1;
      let pass = -1;
      for (let i = 0; i < draws.length; i += N.draw_values) {
        const [buffer, meshId, byteOffset, count, drawPass, drawLayer] = draws.subarray(i, i + N.draw_values);
        if (drawLayer !== layer) {
          if (layer !== -1) {
            gl.depthMask(true);
            gl.clear(gl.DEPTH_BUFFER_BIT);
            pass = -1;
          }
          layer = drawLayer;
          gl.uniformMatrix4fv(uVP, false, view.subarray(...N.view.layers[layer]));
        }
        if (drawPass !== pass) {
          enterPass(passes[drawPass]);
          pass = drawPass;
        }
        const mesh = meshes[meshId];
        gl.bindBuffer(gl.ARRAY_BUFFER, mesh.buffer);
        for (const a of vertexAttributes) {
          gl.vertexAttribPointer(a.loc, a.size, gl.FLOAT, false, N.vertex.stride, a.offset);
        }
        gl.bindBuffer(gl.ARRAY_BUFFER, buffers[buffer]);
        for (const a of instanceAttributes) {
          gl.vertexAttribPointer(a.loc, a.size, gl.FLOAT, false, N.instance.stride, a.offset + byteOffset);
        }
        gl.drawArraysInstanced(gl.TRIANGLES, 0, mesh.vertices, count);
      }
      gl.depthMask(true);
      if (filtered) {
        // T15: the picture to the screen, through the filter
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        gl.disable(gl.BLEND);
        gl.disable(gl.DEPTH_TEST);
        gl.disable(gl.CULL_FACE);
        gl.useProgram(post);
        gl.bindVertexArray(postVao);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, picture.texture);
        gl.drawArrays(gl.TRIANGLES, 0, N.post.vertices);
        gl.bindVertexArray(vao);
        gl.useProgram(program);
      }
    },
  };
}
