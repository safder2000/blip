//! Signed envelopes: every message between pets is a JSON object with an HMAC-SHA256 `sig`
//! over the rest of the object (see `seal`/`check` in the Python `blip/net.py`).

use serde_json::{Map, Value};

use crate::{hex, hmac_sha256, pyjson, same, sha256};

/// The office group key: SHA-256 of "blip-office:" + passphrase (default passphrase "blip-office").
pub fn key_from_passphrase(passphrase: &str) -> [u8; 32] {
    sha256(format!("blip-office:{passphrase}").as_bytes())
}

/// The exact text that gets signed: the object without `sig`, Python-style JSON.
pub fn canon(env: &Map<String, Value>) -> String {
    let mut m = env.clone();
    m.remove("sig");
    pyjson::dumps(&Value::Object(m))
}

pub fn signature(key: &[u8], env: &Map<String, Value>) -> String {
    hex(&hmac_sha256(key, canon(env).as_bytes()))
}

/// Add (or replace) the signature.
pub fn seal(key: &[u8], mut env: Map<String, Value>) -> Map<String, Value> {
    env.remove("sig");
    let sig = signature(key, &env);
    env.insert("sig".into(), Value::String(sig));
    env
}

/// True if `sig` is a valid signature for this key.
pub fn check(key: &[u8], env: &Map<String, Value>) -> bool {
    match env.get("sig") {
        Some(Value::String(sig)) => same(sig.as_bytes(), signature(key, env).as_bytes()),
        _ => false,
    }
}
