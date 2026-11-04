// The page draws what the core returns and decides nothing.
//
// Chumworld Arena's page (e78945b) less its save, health bars, watch mode
// and online play (M1). It loads the data files, starts the core through
// the shim, and each display frame passes the clock, the held actions and
// the mouse and wheel movement in, then draws the frame that comes out
// (gl.js). The screen's panels and the lines over play are built from the
// core's view model (`ui`) without knowing what any panel means; every
// button sends its action back through `act`, the core's one door. Esc, a
// lost pointer and a hidden tab are reported to the core as they happen;
// the core decides what each means.
import init, { Game, numbers } from "./pkg/tool_shim.js";
import { makeRenderer } from "./gl.js";
import { makeInput } from "./input.js";

const el = (id) => document.getElementById(id);
const make = (tag, text, className) => {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
};
const body = document.body;

let hints = [];

// A label whose text is a glossary term shows the definition on hover.
function mark(node) {
  const hint = hints.find((h) => h.term === node.textContent);
  if (hint) {
    node.dataset.term = hint.term;
    node.title = hint.definition;
  }
}

function ready() {
  el("loading").hidden = true;
  body.dataset.ready = "true";
}

async function main() {
  const strings = await (await fetch("data/strings.json")).text();
  el("loading-text").textContent = JSON.parse(strings).loading;
  const [glossary, look, controls, display, vertex, fragment] = await Promise.all([
    fetch("data/glossary.md").then((r) => r.text()),
    fetch("data/look.json").then((r) => r.text()),
    fetch("data/controls.json").then((r) => r.json()),
    fetch("data/display.json").then((r) => r.json()),
    fetch("data/shaders/scene.vert").then((r) => r.text()),
    fetch("data/shaders/scene.frag").then((r) => r.text()),
  ]);
  await init();
  const N = JSON.parse(numbers());

  // chance for this session, drawn once and handed to the core (rng.rs)
  const seed = new Uint32Array(1);
  crypto.getRandomValues(seed);
  let game;
  try {
    game = new Game(strings, glossary, look, seed[0]);
  } catch (e) {
    el("message").textContent = String(e);
    ready();
    return;
  }

  const canvas = el("view");
  const meshes = JSON.parse(game.meshes());
  const renderer = makeRenderer(canvas, N, { vertex, fragment, software: display.software }, meshes, (id) => game.mesh(id));
  if (!renderer) {
    body.dataset.webgl = "none";
    el("message").textContent = JSON.parse(game.ui()).ok.no_webgl;
    ready();
    return;
  }
  body.dataset.webgl = "2";
  body.dataset.software = String(renderer.software);
  if (renderer.software) game.act("display.software", "true");
  renderer.setUniforms(game.uniforms(), game.clear_color());

  let ui = null;
  const input = makeInput(canvas, controls, N.actions, () => {});
  const params = new URLSearchParams(location.search);

  // Sends one action through the core's door and draws what comes back.
  // `gesture`: the act came from a key or click, so if the core now wants
  // the pointer the page may take it again.
  function act(name, value, gesture) {
    const result = JSON.parse(game.act(name, value));
    if (result.error) {
      el("message").textContent = result.error;
      return;
    }
    el("message").textContent = "";
    show(result.ok);
    if (gesture && result.ok.capture && !input.engaged()) input.engage();
  }

  function button(a) {
    const node = make("button", a.label);
    node.disabled = !a.enabled;
    node.dataset.action = a.name;
    node.dataset.value = a.value;
    node.addEventListener("click", () => {
      // the browser grants the pointer only inside a click
      if (a.engage) input.engage();
      node.blur();
      act(a.name, a.value);
    });
    return node;
  }

  function panel(p) {
    const box = make("section", undefined, "panel");
    box.id = p.id;
    if (p.heading) box.append(make("h1", p.heading));
    if (p.text) box.append(make("p", p.text));
    (p.settings || []).forEach((s) => {
      const row = make("div", undefined, "setting");
      row.id = `setting-${s.id}`;
      const value = make("output", s.value);
      row.append(make("span", s.label, "label"), value, ...s.actions.map(button));
      box.append(row);
    });
    (p.actions || []).forEach((a) => box.append(button(a)));
    if (p.rows) {
      if (p.rows_heading) box.append(make("h2", p.rows_heading));
      const table = make("table", undefined, "controls");
      for (const r of p.rows) {
        const row = table.insertRow();
        row.insertCell().append(make("kbd", r.key));
        row.insertCell().append(make("span", r.does));
      }
      box.append(table);
    }
    return box;
  }

  // Builds the screen from the core's view model.
  let shownPanels = "";
  function show(u) {
    const changed = !ui || ui.screen !== u.screen;
    ui = u;
    hints = u.hints;
    document.title = u.title;
    body.dataset.screen = u.screen;
    // (what a reader has typed or pasted into a box stays through a redraw)
    // T11: the panels are rebuilt only when they change, so a view that
    // changes every tick leaves its buttons under the pointer
    const panels = JSON.stringify(u.panels);
    if (panels !== shownPanels) {
      shownPanels = panels;
      const typed = new Map([...el("overlay").querySelectorAll("input, textarea")].filter((n) => !n.readOnly).map((n) => [n.id, n.value]));
      el("overlay").replaceChildren(...u.panels.map(panel));
      typed.forEach((value, id) => {
        if (el(id) && !el(id).readOnly) el(id).value = value;
      });
    }
    el("overlay").hidden = u.panels.length === 0;
    for (const item of u.hud) {
      let node = el(`hud-${item.id}`);
      if (!node) {
        node = make("p", item.text, "hud-item");
        node.id = `hud-${item.id}`;
        el("hud").append(node);
      }
      node.textContent = item.text;
      node.classList.toggle("shown", item.shown);
    }
    document.querySelectorAll("#overlay h1, #overlay h2, #overlay td span, #overlay button, #overlay .label").forEach(mark);
    if (!u.capture) input.release();
    // a screen of buttons opens with its first one focused, for the keyboard
    const first = el("overlay").querySelector("button");
    if (changed && first) first.focus({ preventScroll: true });
  }

  // Esc, a lost pointer and a hidden tab: the core decides what they mean.
  window.addEventListener("keydown", (e) => {
    if (e.code === "Escape") act("key.escape", "");
  });
  let locked = false;
  document.addEventListener("pointerlockchange", () => {
    const now = Boolean(document.pointerLockElement);
    // only a lock that was held and is now lost: a browser that grants the
    // lock and drops it at once plays on without it
    if (locked && !now) act("pointer.lost", "");
    locked = now;
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) act("page.hidden", "");
  });
  // in play, a click on the view takes the pointer again if it was let go
  canvas.addEventListener("mousedown", () => {
    if (ui && ui.capture && !input.engaged()) input.engage();
  });

  // a reader's look speed from the address (?look=<percent>), and test
  // settings, each through the core's door like any setting: ?act=name:value
  // any act
  const lookSpeed = params.get("look");
  if (lookSpeed) act("look.speed", lookSpeed);
  for (const asked of params.getAll("act")) {
    const [name, value] = asked.split(":");
    act(name, value || "");
  }
  show(JSON.parse(game.ui()).ok);

  function resize() {
    const [w, h] = game.resize(canvas.clientWidth, canvas.clientHeight, window.devicePixelRatio || 1);
    renderer.resize(w, h);
  }
  window.addEventListener("resize", resize);
  resize();

  let staticVersion = 0;
  let uiVersion = 0;
  function frame(now) {
    const { bits, dx, dy, wheel } = input.read();
    game.advance(now, bits, dx, dy, wheel);
    if (game.static_version() !== staticVersion) {
      staticVersion = game.static_version();
      renderer.setStatic(game.static_values());
    }
    renderer.frame(game.view(), game.instances(), game.draws());
    if (game.ui_version() !== uiVersion) {
      uiVersion = game.ui_version();
      show(JSON.parse(game.ui()).ok);
    }
    if (body.dataset.ready !== "true") ready();
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main();
