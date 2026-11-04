// Online settings: the only file in the project that names an address on
// another origin (Builder's online kit, kits/online/web/net/config.js, as
// Chumworld Arena carried it at e78945b).
//
// Read only by web/net/rtc.js, and rtc.js runs only after a player hosts or
// joins a match. Loading and idling the page never reads it.
//
// crates/core/tests/online_fence.rs holds this file to three things:
//   - it is the only file outside web/net/vendor/ that names a wss:// address;
//   - every page's CSP connect-src is 'self' plus exactly `relays` below;
//   - `protocol` equals PROTOCOL in crates/core/src/online.rs.
//
// None of the numbers here is a rule of the game: each is a transport
// timing, an id or a frame's marker.

export const CONFIG = {
  // Keeps this project's rooms apart from every other app on the same
  // public relays. The core's APP_ID names the same thing in each room.
  appId: "chumontillado",

  // The core's PROTOCOL. The room name is the app id, this tag and the
  // typed code (crates/core/src/online.rs, room_name), never the build hash.
  protocol: 1,

  // The vendored Trystero bundle, pinned in vendor/SHA256SUMS and checked
  // by scripts/build.sh. Imported only when a player uses a room code.
  bundle: "./vendor/trystero-nostr-0.25.4.js",

  // Encrypts session descriptions while they cross relays this project
  // does not run. It ships in this file, so it is not a secret. The match
  // itself is DTLS-encrypted end to end regardless.
  password: "chumontillado-match",

  // The Nostr relays that introduce players by room code. Every player must
  // use the same list, so it lives here. Raiders of the Lost Mart's list,
  // each of which answered a WebSocket upgrade in October 2026 (the month
  // relay.mostr.pub began redirecting every handshake). Check each before a
  // release; relays come and go.
  relays: [
    "wss://nos.lol",
    "wss://nostr.mom",
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://nostr-01.yakihonne.com",
    "wss://nostr.sathoarder.com",
    "wss://relay.mostro.network",
  ],

  // STUN tells a browser its public address. Without TURN, two players who
  // are both behind strict NATs cannot meet; data/strings.json says so under
  // online.no_path. A CSP connect-src does not govern these addresses.
  iceServers: [
    { urls: "stun:stun.l.google.com:19302" },
    { urls: "stun:global.stun.twilio.com:3478" },
  ],

  // The negotiated data channel's stream id. Both ends create it with this
  // id, so there is no ondatachannel event to miss. Not 0 or 1: Trystero
  // opens its own in-band channel from 0 upwards.
  channelId: 40,

  // How often a link repeats its hello until the other end acknowledges it.
  helloRepeatMs: 250,

  // How long to gather ICE candidates before a pasted code is shown anyway.
  gatherTimeoutMs: 6000,

  // How long a room code waits for any relay before reporting online.no_relay.
  relayTimeoutMs: 15000,

  // A WebSocket's readyState when it is open.
  socketOpen: 1,
};

// `?ice=none` drops the STUN servers, so tabs on one machine connect on host
// candidates alone and nothing leaves the origin. tests/browser/online.py
// uses it; a player never needs it.
export function iceServers() {
  const ice = new URLSearchParams(location.search).get("ice");
  return ice === "none" ? [] : CONFIG.iceServers;
}
