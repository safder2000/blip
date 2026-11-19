//! Team encryption for everything that goes through a Blip server (`box`/`unbox` in `blip/link.py`).
//!
//! The team key comes from the team code; the server only ever sees the room id (a hash of the key)
//! and the encrypted box. Stream cipher: BLAKE2b keystream; MAC: keyed BLAKE2b, 16 bytes.

use base64::Engine;

use crate::{hmac_sha256, random_bytes, same, sha256};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

pub fn team_key(code: &str) -> [u8; 32] {
    sha256(format!("blip-team:{code}").as_bytes())
}

/// What the server sees instead of the team: 24 hex chars.
pub fn room_id(tkey: &[u8]) -> String {
    blake2b_simd::Params::new().hash_length(12).personal(b"blip-room").hash(tkey).to_hex().to_string()
}

fn keys(tkey: &[u8]) -> ([u8; 32], [u8; 32]) {
    (hmac_sha256(tkey, b"box-enc"), hmac_sha256(tkey, b"box-mac"))
}

fn keystream(key: &[u8], nonce: &[u8], n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n + 64);
    let mut i: u32 = 0;
    while out.len() < n {
        let mut block = nonce.to_vec();
        block.extend_from_slice(&i.to_be_bytes());
        out.extend_from_slice(blake2b_simd::Params::new().hash_length(64).key(key).hash(&block).as_bytes());
        i += 1;
    }
    out.truncate(n);
    out
}

fn tag(mac: &[u8], nonce: &[u8], ct: &[u8]) -> Vec<u8> {
    let mut st = blake2b_simd::Params::new().hash_length(16).key(mac).to_state();
    st.update(nonce);
    st.update(ct);
    st.finalize().as_bytes().to_vec()
}

fn seal_with_nonce(tkey: &[u8], data: &[u8], nonce: [u8; 16]) -> String {
    let (enc, mac) = keys(tkey);
    let ct: Vec<u8> = data.iter().zip(keystream(&enc, &nonce, data.len())).map(|(a, b)| a ^ b).collect();
    let mut raw = nonce.to_vec();
    raw.extend_from_slice(&ct);
    raw.extend_from_slice(&tag(&mac, &nonce, &ct));
    B64.encode(raw)
}

/// Encrypt + authenticate; base64 text, ready for the `box` field.
pub fn seal_box(tkey: &[u8], data: &[u8]) -> String {
    seal_with_nonce(tkey, data, random_bytes::<16>())
}

/// The plaintext, or None if it isn't for this team or was tampered with.
pub fn open_box(tkey: &[u8], text: &str) -> Option<Vec<u8>> {
    let raw = B64.decode(text.trim()).ok()?;
    if raw.len() < 32 {
        return None;
    }
    let (nonce, rest) = raw.split_at(16);
    let (ct, t) = rest.split_at(rest.len() - 16);
    let (enc, mac) = keys(tkey);
    if !same(t, &tag(&mac, nonce, ct)) {
        return None;
    }
    Some(ct.iter().zip(keystream(&enc, nonce, ct.len())).map(|(a, b)| a ^ b).collect())
}
