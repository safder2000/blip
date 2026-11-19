//! Every pose must draw exactly the pixels the Python Blip draws.
//! sprites.json comes from `python rust/tools/make_vectors.py`.

use blip_pet::sprites::{self, PetLook, Stage};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn fp(g: &sprites::Sprite) -> String {
    let h = Sha256::digest(sprites::fingerprint(g).as_bytes());
    h.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn data() -> Value {
    serde_json::from_str(include_str!("sprites.json")).unwrap()
}

#[test]
fn every_pet_pose_matches_python() {
    let d = data();
    let mut bad = Vec::new();
    for p in d["pets"].as_array().unwrap() {
        let look = PetLook {
            species: p[0].as_str().unwrap(),
            stage: Stage::from_name(p[1].as_str().unwrap()),
            palette: p[2].as_str().unwrap(),
            mood: p[3].as_str().unwrap(),
            frame: p[4].as_u64().unwrap() as u32,
            facing: p[5].as_i64().unwrap() as i32,
            squash: p[6].as_bool().unwrap(),
            blink: p[7].as_bool().unwrap(),
        };
        if fp(&sprites::build_pet(&look)) != p[8].as_str().unwrap() {
            bad.push(format!("{} {} {} {} f{} face{} sq{} bl{}", p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]));
        }
    }
    assert!(bad.is_empty(), "{} of {} poses differ, e.g. {:?}", bad.len(), d["pets"].as_array().unwrap().len(), &bad[..bad.len().min(8)]);
}

#[test]
fn eggs_and_balls_match_python() {
    let d = data();
    for e in d["eggs"].as_array().unwrap() {
        let g = sprites::build_egg(e[0].as_u64().unwrap() as u32, e[1].as_f64().unwrap());
        assert_eq!(fp(&g), e[2].as_str().unwrap(), "egg {e}");
    }
    for b in d["balls"].as_array().unwrap() {
        let pal = sprites::palette(b[0].as_str().unwrap());
        let g = sprites::build_ball(pal, b[1].as_f64().unwrap(), b[3].as_bool().unwrap(), b[2].as_f64().unwrap());
        assert_eq!(fp(&g), b[4].as_str().unwrap(), "ball {b}");
    }
}
