// Keyboard and mouse, turned into the action bits the core defines. Part of
// the realtime kit (kits/realtime/README.md), after the input layer shared by
// Pursuer and Long Time Comin'. This project adds the wheel, which adds up
// between reads as the mouse does (SECOND-ORDER.md R2.1).
//
// data/controls.json names an action for each key code ("KeyW": "forward",
// "Mouse0": "fire"); the core's numbers() gives each action its bit. This
// file decides nothing about what an action does.
//
// Two things it keeps for the core:
//   - a press released before the next frame reads it still reaches the
//     core once (`tapped`);
//   - mouse movement adds up between reads, as movementX and movementY.
//
// Pointer lock is tried with raw movement, then plain. Where neither is
// granted (an embedded frame, a refused request), or where the page was
// opened with `?pointer=free`, the fallback takes the keyboard and unlocked
// movement instead. The browser check opens the page with `?pointer=free`,
// because a driven browser may grant the lock and drop it at once.

export function makeInput(canvas, controls, actions, onEngage) {
  const state = { held: 0, tapped: 0, dx: 0, dy: 0, wheel: 0, fallback: false };
  const bitOf = (code) => actions[controls[code]] || 0;
  const locked = () => document.pointerLockElement === canvas;
  const engaged = () => locked() || state.fallback;

  function release() {
    state.held = 0;
    state.tapped = 0;
    state.fallback = false;
  }

  function letGo() {
    release();
    if (locked() && document.exitPointerLock) document.exitPointerLock();
  }

  function fallBack() {
    state.fallback = true;
    onEngage();
  }

  // A mode any reader can choose, so the browser check runs a path a
  // person can also open. The template bans testing for automation itself.
  const free = new URLSearchParams(location.search).get("pointer") === "free";

  // Called from a click or key press; browsers grant the lock only then.
  function engage() {
    if (free || !canvas.requestPointerLock) return fallBack();
    const plain = () => {
      try {
        const asked = canvas.requestPointerLock();
        if (asked && asked.catch) asked.catch(fallBack);
      } catch (e) {
        fallBack();
      }
    };
    try {
      const raw = canvas.requestPointerLock({ unadjustedMovement: true });
      if (raw && raw.catch) raw.catch(plain);
    } catch (e) {
      plain();
    }
    onEngage();
  }

  document.addEventListener("pointerlockchange", () => {
    if (!locked() && !state.fallback) release();
  });
  document.addEventListener("pointerlockerror", () => {
    state.fallback = true;
  });

  function press(bit, e) {
    if (!bit || !engaged()) return;
    state.held |= bit;
    state.tapped |= bit;
    e.preventDefault();
  }

  window.addEventListener("keydown", (e) => {
    // Escape always lets go, of a real lock and of the fallback.
    if (e.code === "Escape") return letGo();
    if (e.repeat) return;
    press(bitOf(e.code), e);
  });
  window.addEventListener("keyup", (e) => {
    state.held &= ~bitOf(e.code);
  });
  canvas.addEventListener("mousedown", (e) => press(bitOf(`Mouse${e.button}`), e));
  window.addEventListener("mouseup", (e) => {
    state.held &= ~bitOf(`Mouse${e.button}`);
  });
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  window.addEventListener("mousemove", (e) => {
    if (!engaged()) return;
    state.dx += e.movementX;
    state.dy += e.movementY;
  });
  window.addEventListener(
    "wheel",
    (e) => {
      if (engaged()) state.wheel += e.deltaY;
    },
    { passive: true },
  );
  window.addEventListener("blur", release);

  return {
    engage,
    engaged,
    release: letGo,
    // Once per display frame: the bits held or tapped since the last read,
    // and the movement since then. The core latches what a frame with no
    // tick could not use (crates/core/src/ticks.rs).
    read() {
      const out = { bits: state.held | state.tapped, dx: state.dx, dy: state.dy, wheel: state.wheel };
      state.tapped = 0;
      state.dx = 0;
      state.dy = 0;
      state.wheel = 0;
      return out;
    },
  };
}
