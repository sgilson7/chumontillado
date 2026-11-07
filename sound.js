// Sound (M6): the page plays what the core says to, and decides nothing.
// Each frame the core gives a list of sounds, each a recipe's name, a gain
// and a place from left to right (crates/core/src/sound.rs); this file
// synthesizes the recipe (data/sounds.json) with the Web Audio API: a burst
// of the core's white noise through a filter, and a tone that slides, each
// fading away. A browser lets a page make sound only after the reader has
// clicked or pressed a key, so nothing plays until then.
export function makeSound(recipes, noiseSamples) {
  const K = recipes.constants;
  let ctx = null;
  let master = null;
  let noise = null;

  function unlock() {
    if (ctx) {
      if (ctx.state === "suspended") ctx.resume();
      return;
    }
    const Context = window.AudioContext || window.webkitAudioContext;
    if (!Context) return;
    ctx = new Context();
    master = ctx.createGain();
    master.gain.value = K.master;
    master.connect(ctx.destination);
    noise = ctx.createBuffer(1, noiseSamples.length, K.sample_rate);
    noise.copyToChannel(noiseSamples, 0);
  }

  // a gain that rises at once to `peak` and fades over `ms`
  function envelope(peak, ms, at) {
    const g = ctx.createGain();
    g.gain.setValueAtTime(K.silence, at);
    g.gain.linearRampToValueAtTime(peak, at + K.attack_ms / K.thousand);
    g.gain.exponentialRampToValueAtTime(K.silence, at + ms / K.thousand);
    return g;
  }

  function play(list) {
    if (!ctx || ctx.state !== "running") return;
    const at = ctx.currentTime;
    for (const p of list) {
      const r = recipes.voices[p.voice];
      if (!r) continue;
      const out = ctx.createGain();
      out.gain.value = p.gain / K.thousand;
      const pan = ctx.createStereoPanner();
      pan.pan.value = p.pan / K.thousand;
      out.connect(pan).connect(master);
      if (r.noise > 0) {
        const src = ctx.createBufferSource();
        src.buffer = noise;
        const filter = ctx.createBiquadFilter();
        filter.type = r.filter;
        filter.frequency.value = r.filter_hz;
        filter.Q.value = r.q;
        src.connect(filter).connect(envelope(r.noise, r.noise_ms, at)).connect(out);
        src.start(at);
        src.stop(at + r.noise_ms / K.thousand);
      }
      if (r.tone > 0) {
        const osc = ctx.createOscillator();
        osc.type = r.wave;
        osc.frequency.setValueAtTime(r.tone_hz, at);
        osc.frequency.exponentialRampToValueAtTime(r.tone_to_hz, at + r.tone_ms / K.thousand);
        osc.connect(envelope(r.tone, r.tone_ms, at)).connect(out);
        osc.start(at);
        osc.stop(at + r.tone_ms / K.thousand);
      }
    }
  }

  return { unlock, play };
}
