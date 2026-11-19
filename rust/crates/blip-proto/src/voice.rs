//! Call audio packets (`session_keys`/`seal`/`open_packet` in `blip/voice.py`).
//!
//! Packet = header ("BV1", direction u8, sequence u32 big-endian) + encrypted audio + 8-byte tag.

use crate::{hmac_sha256, same};

pub const MAGIC: &[u8; 3] = b"BV1";
pub const HEADER: usize = 8;
pub const TAG: usize = 8;

#[derive(Clone)]
pub struct Keys {
    pub enc: [u8; 32],
    pub mac: [u8; 32],
}

/// Per-call keys from the group (or team) key, the call id and the answerer's random nonce.
pub fn session_keys(group_key: &[u8], call_id: &str, nonce: &[u8]) -> Keys {
    let mut data = b"blip-voice|".to_vec();
    data.extend_from_slice(call_id.as_bytes());
    data.push(b'|');
    data.extend_from_slice(nonce);
    let base = hmac_sha256(group_key, &data);
    Keys { enc: hmac_sha256(&base, b"enc"), mac: hmac_sha256(&base, b"mac") }
}

fn keystream(key: &[u8], direction: u8, seq: u32, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n + 64);
    let mut block: u16 = 0;
    while out.len() < n {
        let mut b = vec![direction];
        b.extend_from_slice(&seq.to_be_bytes());
        b.extend_from_slice(&block.to_be_bytes());
        out.extend_from_slice(blake2b_simd::Params::new().hash_length(64).key(key).hash(&b).as_bytes());
        block += 1;
    }
    out.truncate(n);
    out
}

fn tag(mac: &[u8], head: &[u8], ct: &[u8]) -> Vec<u8> {
    let mut st = blake2b_simd::Params::new().hash_length(TAG).key(mac).to_state();
    st.update(head);
    st.update(ct);
    st.finalize().as_bytes().to_vec()
}

pub fn seal(keys: &Keys, direction: u8, seq: u32, payload: &[u8]) -> Vec<u8> {
    let mut head = MAGIC.to_vec();
    head.push(direction);
    head.extend_from_slice(&seq.to_be_bytes());
    let ct: Vec<u8> = payload.iter().zip(keystream(&keys.enc, direction, seq, payload.len())).map(|(a, b)| a ^ b).collect();
    let t = tag(&keys.mac, &head, &ct);
    let mut out = head;
    out.extend_from_slice(&ct);
    out.extend_from_slice(&t);
    out
}

/// (direction, sequence, payload), or None if it isn't ours or was tampered with.
pub fn open(keys: &Keys, packet: &[u8]) -> Option<(u8, u32, Vec<u8>)> {
    if packet.len() < HEADER + TAG || &packet[..3] != MAGIC {
        return None;
    }
    let (head, rest) = packet.split_at(HEADER);
    let (ct, t) = rest.split_at(rest.len() - TAG);
    if !same(t, &tag(&keys.mac, head, ct)) {
        return None;
    }
    let direction = head[3];
    let seq = u32::from_be_bytes([head[4], head[5], head[6], head[7]]);
    let pcm = ct.iter().zip(keystream(&keys.enc, direction, seq, ct.len())).map(|(a, b)| a ^ b).collect();
    Some((direction, seq, pcm))
}
