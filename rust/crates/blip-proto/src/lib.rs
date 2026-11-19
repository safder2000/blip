//! Blip's wire format, byte-for-byte compatible with the Python Blip (`blip/net.py`, `blip/link.py`,
//! `blip/voice.py`), so Python and Rust pets can talk to each other while people update.
//!
//! - [`pyjson`]: JSON exactly as Python's `json.dumps(sort_keys=True, separators=(",", ":"), ensure_ascii=False)`
//! - [`envelope`]: HMAC-SHA256 signed messages (office network and server)
//! - [`team`]: end-to-end encryption for everything sent through a Blip server
//! - [`link`]: reading the server links people paste
//! - [`voice`]: encrypted call audio packets

pub mod envelope;
pub mod link;
pub mod pyjson;
pub mod team;
pub mod voice;

pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    let mut m = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key).expect("any key length works");
    m.update(data);
    m.finalize().into_bytes().into()
}

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(data).into()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time comparison.
pub(crate) fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("system randomness");
    b
}
