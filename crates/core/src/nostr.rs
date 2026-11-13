//! T38: just enough of Nostr (NIP-01) to post and read lobby records on the
//! public relays the online kit already lists: an event, its id, its
//! Schnorr signature, a little proof of work (NIP-13), and the three
//! frames a client sends and the one it reads.
//!
//! Countless Days' ledger (BrowserOSU, `ledger.rs`) is the model: a record
//! is signed with a key the browser holds, named by the hash of what it
//! says, and checked before it is kept. Nostr fixes the key type (BIP-340
//! Schnorr on secp256k1) and the hash (SHA-256 of a canonical array).
//!
//! The relays check signatures too, but this module never relies on that:
//! `check` recomputes the id and verifies the signature itself.

use k256::schnorr::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Most nonces tried before a record is posted with less work than asked
/// (2^20; at 12 bits the expected count is 2^12).
const MAX_TRIES: u64 = 1 << 20;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(text.get(k..k + 2)?, 16).ok())
        .collect()
}

/// A browser's signing key, made from bytes the page drew.
#[derive(Clone)]
pub struct Key(SigningKey);

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Key({})", self.public())
    }
}

impl Key {
    /// From any bytes: hashed until they make a valid secret (one hash in
    /// almost every case), so no input is refused.
    pub fn from_bytes(bytes: &[u8]) -> Key {
        let mut seed: [u8; 32] = Sha256::digest(bytes).into();
        loop {
            if let Ok(k) = SigningKey::from_bytes(&seed) {
                return Key(k);
            }
            seed = Sha256::digest(seed).into();
        }
    }

    /// The x-only public key, in hex: the record's `pubkey`.
    pub fn public(&self) -> String {
        hex(&self.0.verifying_key().to_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub pubkey: String,
    pub created_at: i64,
    pub kind: u32,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

impl Event {
    /// The first value of the first tag named `name`.
    pub fn tag(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.first().map(String::as_str) == Some(name))
            .and_then(|t| t.get(1))
            .map(String::as_str)
    }
}

/// NIP-01: the id is the SHA-256 of `[0, pubkey, created_at, kind, tags,
/// content]` written as JSON without spaces.
pub fn event_id(pubkey: &str, created_at: i64, kind: u32, tags: &[Vec<String>], content: &str) -> [u8; 32] {
    let text = json!([0, pubkey, created_at, kind, tags, content]).to_string();
    Sha256::digest(text.as_bytes()).into()
}

/// NIP-13: leading zero bits of an id.
pub fn work(id: &[u8]) -> u32 {
    let mut n = 0;
    for b in id {
        if *b == 0 {
            n += 8;
        } else {
            n += b.leading_zeros();
            break;
        }
    }
    n
}

/// Makes and signs an event, mining a `nonce` tag (NIP-13) until its id has
/// `bits` leading zero bits.
pub fn sign(
    key: &Key,
    created_at: i64,
    kind: u32,
    mut tags: Vec<Vec<String>>,
    content: &str,
    bits: u32,
) -> Event {
    let pubkey = key.public();
    let mut id = event_id(&pubkey, created_at, kind, &tags, content);
    if bits > 0 {
        tags.push(vec!["nonce".into(), String::new(), bits.to_string()]);
        let at = tags.len() - 1;
        for n in 0..MAX_TRIES {
            tags[at][1] = n.to_string();
            id = event_id(&pubkey, created_at, kind, &tags, content);
            if work(&id) >= bits {
                break;
            }
        }
    }
    // BIP-340 allows any auxiliary bytes; the id itself keeps signing
    // deterministic, as the core must be
    let sig = key
        .0
        .sign_raw(&id, &id)
        .map(|s| hex(&s.to_bytes()))
        .unwrap_or_default();
    Event {
        id: hex(&id),
        pubkey,
        created_at,
        kind,
        tags,
        content: content.into(),
        sig,
    }
}

/// Whether an event's id is the hash of what it says and its signature is
/// its pubkey's.
pub fn check(e: &Event) -> bool {
    let id = event_id(&e.pubkey, e.created_at, e.kind, &e.tags, &e.content);
    if hex(&id) != e.id {
        return false;
    }
    let (Some(pk), Some(sig)) = (unhex(&e.pubkey), unhex(&e.sig)) else {
        return false;
    };
    let (Ok(vk), Ok(sig)) = (VerifyingKey::from_bytes(&pk), Signature::try_from(sig.as_slice())) else {
        return false;
    };
    vk.verify_raw(&id, &sig).is_ok()
}

/// What a client sends a relay.
pub fn publish(e: &Event) -> String {
    json!(["EVENT", e]).to_string()
}

pub fn subscribe(sub: &str, filter: Value) -> String {
    json!(["REQ", sub, filter]).to_string()
}

pub fn unsubscribe(sub: &str) -> String {
    json!(["CLOSE", sub]).to_string()
}

/// An event a relay sent for subscription `sub`, if the text is one; every
/// other frame (EOSE, OK, NOTICE, CLOSED, anything garbled) is `None`.
pub fn received(text: &str) -> Option<(String, Event)> {
    let v: Value = serde_json::from_str(text).ok()?;
    let a = v.as_array()?;
    if a.len() != 3 || a[0].as_str()? != "EVENT" {
        return None;
    }
    let sub = a[1].as_str()?.to_string();
    let e: Event = serde_json::from_value(a[2].clone()).ok()?;
    Some((sub, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> Key {
        Key::from_bytes(&[n; 32])
    }

    #[test]
    fn bip340_test_vector_0_signs_as_published() {
        // BIP-340 test-vectors.csv, index 0
        let sk = SigningKey::from_bytes(&unhex(&format!("{:064x}", 3)).unwrap()).unwrap();
        assert_eq!(
            hex(&sk.verifying_key().to_bytes()),
            "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"
        );
        let sig = sk.sign_raw(&[0; 32], &[0; 32]).unwrap();
        assert_eq!(
            hex(&sig.to_bytes()),
            "e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1cE2dca821525f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0"
                .to_ascii_lowercase()
        );
    }

    #[test]
    fn an_events_id_is_the_hash_of_its_canonical_array() {
        let e = sign(
            &key(1),
            1_700_000_000,
            30078,
            vec![vec!["d".into(), "x".into()]],
            "{\"a\":1}",
            0,
        );
        let text = format!(
            "[0,\"{}\",1700000000,30078,[[\"d\",\"x\"]],\"{{\\\"a\\\":1}}\"]",
            e.pubkey
        );
        assert_eq!(e.id, hex(&Sha256::digest(text.as_bytes())));
        assert!(check(&e));
    }

    #[test]
    fn a_changed_word_or_a_borrowed_signature_fails_the_check() {
        let e = sign(&key(1), 1_700_000_000, 30078, vec![], "seats 1", 0);
        let mut said = e.clone();
        said.content = "seats 3".into();
        assert!(!check(&said), "content changed under the same id");
        let mut renamed = said.clone();
        renamed.id = hex(&event_id(
            &said.pubkey,
            said.created_at,
            said.kind,
            &said.tags,
            &said.content,
        ));
        assert!(
            !check(&renamed),
            "id fixed up but the signature is the old text's"
        );
        let mut other = e.clone();
        other.pubkey = key(2).public();
        other.id = hex(&event_id(
            &other.pubkey,
            other.created_at,
            other.kind,
            &other.tags,
            &other.content,
        ));
        assert!(!check(&other), "someone else's key on this signature");
    }

    #[test]
    fn mining_reaches_the_work_asked_and_says_so_in_its_nonce() {
        let e = sign(&key(4), 1_700_000_000, 30078, vec![], "x", 12);
        assert!(work(&unhex(&e.id).unwrap()) >= 12);
        // and counted without `work`: twelve zero bits are three zero digits
        assert!(e.id.starts_with("000"), "{}", e.id);
        assert_eq!(work(&[0, 0, 0x1f]), 19);
        assert!(e.tag("nonce").is_some());
        assert_eq!(e.tags.last().unwrap()[2], "12");
        assert!(check(&e));
    }

    #[test]
    fn frames_go_out_and_come_back() {
        let e = sign(&key(1), 5, 30078, vec![], "x", 0);
        let out = publish(&e);
        assert!(out.starts_with("[\"EVENT\","));
        let back = format!("[\"EVENT\",\"s\",{}]", serde_json::to_string(&e).unwrap());
        assert_eq!(received(&back), Some(("s".into(), e)));
        assert_eq!(received("[\"EOSE\",\"s\"]"), None);
        assert_eq!(received("not json"), None);
        assert_eq!(
            subscribe("s", json!({"kinds": [1]})),
            "[\"REQ\",\"s\",{\"kinds\":[1]}]"
        );
        assert_eq!(unsubscribe("s"), "[\"CLOSE\",\"s\"]");
    }
}
