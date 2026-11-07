// The transport: browsers, one reliable channel each, and no server of ours.
//
// Chumworld Arena's (e78945b), changed for four seats (M4, R8.2): by room
// code every browser links to every other in the room, guest to guest as
// well as guest to host, and only two hosts refuse each other. The core
// passes on through the host what a pair that cannot link would have sent.
//
// Written to the design in Builder's online kit (kits/online/README.md,
// "The transport design", which is Peter's from Mandible, gathering lessons
// from Floodline and Vagrancy), with Sam's Level Twenty transport read for
// reference. It decides nothing about the match: it opens links, carries
// text, and reports three events (a peer arrived, a peer left, text from a
// peer) and errors as keys for strings in data/. web/online.js is the one
// page file that imports it, and runs only the commands the core gives.
//
// Two introductions reach the same thing, one RTCPeerConnection with one
// negotiated, ordered data channel on a fixed stream id (CONFIG.channelId):
//
//   * a room code: the vendored Trystero bundle, imported only now, meets
//     the other browser over the public relays in config.js;
//   * pasted codes: the host's invitation and the guest's reply carry the
//     whole handshake, compressed, with ICE gathering finished first. No
//     relay is contacted.
//
// From the channel down both are the same. A frame is text whose first
// character says what it is: a hello (with the sender's role), its
// acknowledgement, or data. A message sent before the other side has made
// its half of a negotiated channel is lost, so each side repeats its hello
// until it is acknowledged, and sends no data until the other's hello has
// come; data that beats the hello is held and handed on after it.
//
// The page scan (crates/core/tests/boundary.rs) holds this file to no
// number but 0 and 1, no Math and no address; every timing is config.js's.

import { CONFIG, iceServers } from "./config.js?v=61feafe46b5f";

const HELLO = "h";
const ACK = "a";
const DATA = "d";
const HOST = "H";
const GUEST = "G";

export function open({ isHost, mode, room, onPeer, onLeft, onMessage, onError }) {
  const s = { closed: false, trystero: null, pending: null, nextId: 1, timers: [] };
  const rtcConfig = () => ({ iceServers: iceServers() });

  // A peer is the other browser, whatever connections come and go under it:
  // Trystero may replace a connection to the same browser, and the new link
  // goes under the same peer with its queue of unsent text.
  const peers = new Map();
  const byId = new Map();
  function peerFor(key) {
    if (!peers.has(key)) {
      const peer = { id: s.nextId, key, link: null, queue: [], said: false, left: false };
      s.nextId += 1;
      peers.set(key, peer);
      byId.set(peer.id, peer);
    }
    return peers.get(key);
  }

  const usable = (link) => link && !link.gone && link.ready && link.ch.readyState === "open";

  function flush(peer) {
    while (peer.queue.length && usable(peer.link)) {
      try {
        peer.link.ch.send(DATA + peer.queue[0]);
      } catch (e) {
        return;
      }
      peer.queue.shift();
    }
  }

  function attach(pc, mine, key) {
    const peer = peerFor(key);
    const ch = pc.createDataChannel("match", { negotiated: true, id: CONFIG.channelId, ordered: true });
    const link = { pc, mine, ch, ready: false, acked: false, gone: false, timer: null, early: [] };
    const sayHello = () => {
      if (link.gone || link.acked || ch.readyState !== "open") return;
      try {
        ch.send(HELLO + (isHost ? HOST : GUEST));
      } catch (e) {
        // closing
      }
    };
    ch.onopen = () => {
      sayHello();
      link.timer = setInterval(sayHello, CONFIG.helloRepeatMs);
    };
    ch.onmessage = (e) => {
      const text = String(e.data);
      const kind = text.charAt(0);
      const rest = text.slice(1);
      if (kind === ACK) {
        link.acked = true;
        clearInterval(link.timer);
        return;
      }
      if (kind === HELLO) {
        try {
          ch.send(ACK);
        } catch (err) {
          // closing
        }
        if (link.ready) return; // a repeat
        // every browser in a room talks to every other, but two hosts (M4)
        const ok = rest === GUEST || (rest === HOST && !isHost);
        if (!ok) {
          hangUp(link);
          return;
        }
        link.ready = true;
        if (peer.link && peer.link !== link) hangUp(peer.link);
        peer.link = link;
        flush(peer);
        if (!peer.said) {
          peer.said = true;
          onPeer(peer.id);
        }
        for (const t of link.early.splice(0)) onMessage(peer.id, t);
        return;
      }
      if (kind === DATA) {
        if (link.ready) onMessage(peer.id, rest);
        else link.early.push(rest);
      }
    };
    const lost = () => {
      if (link.gone) return;
      link.gone = true;
      clearInterval(link.timer);
      if (peer.link === link) peer.link = null;
      // By pasted code there is no relay to say a peer left: a closed link
      // is the leaving. By room code Trystero says so (onPeerLeave), and a
      // closed link may be a connection being replaced.
      if (mode === "code" || link.mine) leave(peer);
    };
    ch.onclose = lost;
    pc.addEventListener("connectionstatechange", () => {
      if (pc.connectionState === "failed" || pc.connectionState === "closed") lost();
    });
    return link;
  }

  function leave(peer) {
    if (peer.said && !peer.left) {
      peer.left = true;
      onLeft(peer.id);
    }
  }

  function hangUp(link) {
    link.gone = true;
    clearInterval(link.timer);
    try {
      link.ch.close();
      if (link.mine) link.pc.close();
    } catch (e) {
      // already closing
    }
  }

  // ---- a room code, through Trystero ----

  async function startRoom() {
    let lib;
    try {
      lib = await import(new URL(CONFIG.bundle, import.meta.url).href);
    } catch (e) {
      onError("online.no_bundle");
      return;
    }
    if (s.closed) return;
    let r;
    try {
      r = lib.joinRoom(
        { appId: CONFIG.appId, password: CONFIG.password, rtcConfig: rtcConfig(), relayConfig: { urls: CONFIG.relays } },
        room,
        {
          onJoinError: (d) => {
            // a failed attempt under a working link is Trystero retrying
            const known = d && d.peerId && peers.get(d.peerId);
            if (known && known.said && !known.left) return;
            onError(d && d.peerId ? "online.no_path" : "online.no_relay");
          },
        },
      );
    } catch (e) {
      onError("online.no_relay");
      return;
    }
    s.trystero = r;
    r.onPeerJoin = (pid) => {
      const pc = r.getPeers()[pid];
      if (pc) attach(pc, false, pid);
    };
    r.onPeerLeave = (pid) => {
      const peer = peers.get(pid);
      if (peer) leave(peer);
    };
    s.timers.push(
      setTimeout(() => {
        if (s.closed || [...peers.values()].some((p) => p.said)) return;
        let up = false;
        try {
          const sockets = lib.getRelaySockets();
          for (const k in sockets) if (sockets[k] && sockets[k].readyState === CONFIG.socketOpen) up = true;
        } catch (e) {
          // the strategy may not expose them
        }
        if (!up) onError("online.no_relay");
      }, CONFIG.relayTimeoutMs),
    );
  }

  // ---- pasted codes ----

  function gathered(pc) {
    if (pc.iceGatheringState === "complete") return Promise.resolve();
    return new Promise((resolve) => {
      pc.addEventListener("icegatheringstatechange", () => {
        if (pc.iceGatheringState === "complete") resolve();
      });
      s.timers.push(setTimeout(resolve, CONFIG.gatherTimeoutMs));
    });
  }

  async function invite() {
    const pc = new RTCPeerConnection(rtcConfig());
    s.pending = attach(pc, true, "code");
    await pc.setLocalDescription(await pc.createOffer());
    await gathered(pc);
    return pack("O", pc.localDescription.sdp);
  }

  async function reply(code) {
    const sdp = await unpack("O", code);
    const pc = new RTCPeerConnection(rtcConfig());
    s.pending = attach(pc, true, "code");
    await pc.setRemoteDescription({ type: "offer", sdp });
    await pc.setLocalDescription(await pc.createAnswer());
    await gathered(pc);
    return pack("A", pc.localDescription.sdp);
  }

  async function accept(code) {
    const sdp = await unpack("A", code);
    if (!s.pending) throw new BadCode("no invitation");
    await s.pending.pc.setRemoteDescription({ type: "answer", sdp });
  }

  if (mode === "room") startRoom();

  return {
    invite,
    reply,
    accept,
    // what cannot go now waits for the peer's open link
    send(id, text) {
      const peer = byId.get(id);
      if (!peer) return;
      peer.queue.push(text);
      flush(peer);
    },
    close() {
      s.closed = true;
      s.timers.forEach(clearTimeout);
      if (s.trystero) {
        try {
          s.trystero.leave();
        } catch (e) {
          // gone
        }
      }
      peers.forEach((p) => p.link && hangUp(p.link));
      if (s.pending) hangUp(s.pending);
      peers.clear();
      byId.clear();
    },
  };
}

// ---- a code: a data channel's description is nearly all boilerplate ----
//
// Only a handful of lines say anything the other end cannot infer, so a
// code carries those and `grow` builds a whole description round them
// (after Floodline's shrink and grow, as Level Twenty has them).

const KEEP = [
  ["a=ice-ufrag:", "u"],
  ["a=ice-pwd:", "p"],
  ["a=fingerprint:", "f"],
  ["a=setup:", "s"],
  ["a=mid:", "m"],
  ["a=sctp-port:", "P"],
  ["a=max-message-size:", "M"],
];

function shrink(sdp) {
  const out = [];
  for (const line of sdp.split(/\r\n|\n/)) {
    const kept = KEEP.find(([prefix]) => line.startsWith(prefix));
    if (kept) out.push(kept[1] + line.slice(kept[0].length));
    else if (line.startsWith("a=candidate:") && / (udp|UDP) /.test(line)) out.push("c" + line.slice("a=candidate:".length));
  }
  return out.join("\n");
}

function grow(short) {
  const v = { u: "", p: "", f: "", s: "actpass", m: "0", P: "5000", M: "262144" };
  const candidates = [];
  for (const line of short.split("\n")) {
    const k = line.charAt(0);
    if (k === "c") candidates.push("a=candidate:" + line.slice(1));
    else if (k in v) v[k] = line.slice(1);
  }
  return [
    "v=0",
    "o=- 0 0 IN IP4 127.0.0.1",
    "s=-",
    "t=0 0",
    "a=group:BUNDLE " + v.m,
    "a=msid-semantic: WMS",
    "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
    "c=IN IP4 0.0.0.0",
    ...candidates,
    "a=ice-ufrag:" + v.u,
    "a=ice-pwd:" + v.p,
    "a=ice-options:trickle",
    "a=fingerprint:" + v.f,
    "a=setup:" + v.s,
    "a=mid:" + v.m,
    "a=sctp-port:" + v.P,
    "a=max-message-size:" + v.M,
    "",
  ].join("\r\n");
}

const b64url = (bytes) => btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const unb64url = (text) => Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));

async function through(stream, bytes) {
  const w = stream.writable.getWriter();
  w.write(bytes);
  w.close();
  return new Uint8Array(await new Response(stream.readable).arrayBuffer());
}

// "O" for an invitation, "A" for a reply, outside the compressed part, so a
// code pasted into the wrong box is refused.
async function pack(kind, sdp) {
  return kind + b64url(await through(new CompressionStream("deflate-raw"), new TextEncoder().encode(shrink(sdp))));
}

export class BadCode extends Error {}

async function unpack(kind, code) {
  const text = (code || "").replace(/\s+/g, "");
  if (text.charAt(0) !== kind) throw new BadCode("wrong kind");
  try {
    return grow(new TextDecoder().decode(await through(new DecompressionStream("deflate-raw"), unb64url(text.slice(1)))));
  } catch (e) {
    throw new BadCode(String(e));
  }
}
