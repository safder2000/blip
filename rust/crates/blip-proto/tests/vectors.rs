//! The Rust wire format must match the Python Blip byte for byte.
//! vectors.json comes from `python rust/tools/make_vectors.py`.

use blip_proto::{envelope, link, team, voice};
use serde_json::{Map, Value};

fn vectors() -> Value {
    serde_json::from_str(include_str!("vectors.json")).unwrap()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn group_key() {
    let v = vectors();
    assert_eq!(hexs(&envelope::key_from_passphrase("blip-office")), v["group_key"].as_str().unwrap());
}

#[test]
fn envelopes_canon_and_signatures() {
    let v = vectors();
    let key = unhex(v["group_key"].as_str().unwrap());
    for e in v["envelopes"].as_array().unwrap() {
        let env: Map<String, Value> = serde_json::from_str(e["input"].as_str().unwrap()).unwrap();
        assert_eq!(envelope::canon(&env), e["canon"].as_str().unwrap());
        let sealed = envelope::seal(&key, env.clone());
        assert_eq!(sealed["sig"], e["sig"]);
        assert!(envelope::check(&key, &sealed));
        let mut bad = sealed.clone();
        bad.insert("nonce".into(), Value::String("tampered".into()));
        assert!(!envelope::check(&key, &bad));
        assert!(!envelope::check(&envelope::key_from_passphrase("other"), &sealed));
    }
}

#[test]
fn teams_rooms_and_boxes() {
    let v = vectors();
    for t in v["teams"].as_array().unwrap() {
        let k = team::team_key(t["code"].as_str().unwrap());
        assert_eq!(hexs(&k), t["key"].as_str().unwrap());
        assert_eq!(team::room_id(&k), t["room"].as_str().unwrap());
        // a box Python sealed opens in Rust...
        let plain = team::open_box(&k, t["box"].as_str().unwrap()).expect("python box opens");
        assert_eq!(String::from_utf8(plain).unwrap(), t["plain"].as_str().unwrap());
        // ...a box Rust seals round-trips, and other teams can't open it
        let mine = team::seal_box(&k, b"from rust");
        assert_eq!(team::open_box(&k, &mine).unwrap(), b"from rust");
        assert!(team::open_box(&team::team_key("someone-else"), &mine).is_none());
    }
}

#[test]
fn links() {
    let v = vectors();
    for l in v["links"].as_array().unwrap() {
        let text = l["text"].as_str().unwrap();
        let got = link::parse(text);
        match &l["parsed"] {
            Value::Null => assert!(got.is_none(), "{text}"),
            p => {
                let g = got.unwrap_or_else(|| panic!("{text} should parse"));
                assert_eq!(g.url, p["url"].as_str().unwrap(), "{text}");
                assert_eq!(g.team, p["team"].as_str().unwrap(), "{text}");
                assert_eq!(g.web, p["web"].as_str().unwrap(), "{text}");
                assert_eq!(g.host, p["host"].as_str().unwrap(), "{text}");
            }
        }
        assert_eq!(link::team_for(text, ""), l["team_for"].as_str().unwrap(), "{text}");
    }
}

#[test]
fn voice_packets() {
    let v = &vectors()["voice"];
    let key = unhex(vectors()["group_key"].as_str().unwrap());
    let k = voice::session_keys(&key, v["call_id"].as_str().unwrap(), &unhex(v["nonce"].as_str().unwrap()));
    assert_eq!(hexs(&k.enc), v["enc"].as_str().unwrap());
    assert_eq!(hexs(&k.mac), v["mac"].as_str().unwrap());
    let pcm = unhex(v["pcm"].as_str().unwrap());
    let pkt = voice::seal(&k, v["dir"].as_u64().unwrap() as u8, v["seq"].as_u64().unwrap() as u32, &pcm);
    assert_eq!(hexs(&pkt), v["packet"].as_str().unwrap());
    assert_eq!(voice::open(&k, &pkt), Some((1, 77, pcm)));
    let mut bad = pkt.clone();
    bad[20] ^= 1;
    assert!(voice::open(&k, &bad).is_none());
}
