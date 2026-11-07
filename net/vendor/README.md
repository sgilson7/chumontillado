# Vendored transport bundle

This directory holds one third-party file: a Trystero strategy bundle that introduces browsers over public Nostr relays. It is copied into the repository, pinned by hash, and never fetched at build time or at load. Builder's online kit (`kits/online/web/net/vendor/README.md`) says how it was made and how to upgrade it.

| File | Version | Licence | sha256 |
| --- | --- | --- | --- |
| `trystero-nostr-0.25.4.js` | 0.25.4 | MIT | `6bfce15d72a64384cc66c2693917994e1b900f4f98c9b7a2d54e4e86f5202906` |

The file was copied from Sam's Level Twenty (`web/net/vendor/`), where it carries the same sum; Raiders of the Lost Mart, Floodline, Vagrancy, Mandible and Ribbit King use the same file. It was made by esm.sh's build service from the published npm package:

```bash
curl -o trystero-nostr-0.25.4.js \
  https://esm.sh/@trystero-p2p/nostr@0.25.4/es2022/nostr.bundle.mjs
```

It is a self-contained ES module: no `import` of another file, nothing fetched when it is imported. `web/net/rtc.js` imports it only when a player hosts or joins by room code, and passes the relays from `web/net/config.js`, so the bundle's own default relay list is never used.

`scripts/build.sh` refuses to build when the file differs from `SHA256SUMS`. An upgrade is a decision recorded in `SECOND-ORDER.md`.
