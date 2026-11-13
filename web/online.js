// Online play, as the page sees it (M4, Chumworld Arena's at e78945b): the
// one page file that imports the transport (web/net/rtc.js;
// online_fence.rs holds it to that). It runs the
// commands the core's view model gives (`ui.online.cmds`: open a link, make
// an invitation, reply to one, accept a reply, close), each once and in
// order; it hands the core every peer, departure and message through the
// shim; and it sends what the core leaves in its outbox. It decides
// nothing. app.js imports it only once the view model holds a command, so
// loading and idling never fetch it.
import * as rtc from "./net/rtc.js";

export function makeOnline(game, act) {
  let link = null;
  let relays = null;
  let done = 0;

  function pump() {
    for (const [id, text] of JSON.parse(game.online_outbox())) if (link) link.send(id, text);
    // T38: the lobby list's frames
    for (const [to, text] of JSON.parse(game.lobby_outbox())) if (relays) relays.send(to, text);
  }

  // a code the transport made, or its refusal of one pasted in
  function made(name, promise) {
    promise.then(
      (code) => act(name, code),
      (e) => act("online.error", e instanceof rtc.BadCode ? "online.bad_code" : "online.signal_failed"),
    );
  }

  const run = {
    open(c) {
      run.close();
      link = rtc.open({
        isHost: c.host,
        mode: c.mode,
        room: c.room,
        onPeer: (id) => {
          game.online_connected(id);
          pump();
        },
        onLeft: (id) => {
          game.online_left(id);
          pump();
        },
        onMessage: (id, text) => {
          game.online_message(id, text);
          pump();
        },
        onError: (key) => act("online.error", key),
      });
    },
    invite() {
      if (link) made("online.offer", link.invite());
    },
    reply(c) {
      if (link) made("online.answer", link.reply(c.code));
    },
    accept(c) {
      if (link) link.accept(c.code).catch((e) => act("online.error", e instanceof rtc.BadCode ? "online.bad_code" : "online.signal_failed"));
    },
    close() {
      // the last words (a bye) go before the link closes
      pump();
      if (link) link.close();
      link = null;
    },
    // T38: the lobby list's link to the relays
    lobby_open() {
      if (relays) return;
      relays = rtc.openRelays({
        ready: (bytes, wall, behind) => game.lobby_ready(bytes, wall, behind),
        onOpen: (k) => {
          game.lobby_opened(k);
          pump();
        },
        onText: (text) => {
          game.lobby_message(text);
          pump();
        },
      });
    },
    lobby_close() {
      // the last record (closed) goes before the link closes
      pump();
      if (relays) relays.close();
      relays = null;
    },
  };

  return {
    // each command the core has given and this page has not yet run
    follow(online) {
      for (const c of online.cmds) {
        if (c.id <= done) continue;
        done = c.id;
        if (run[c.act]) run[c.act](c);
      }
    },
    pump,
  };
}
