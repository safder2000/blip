//! Settings (`config.json`), with the same keys, defaults and migrations as `blip/config.py`.
//! Unknown keys are kept as they are, so the Python and Rust apps can share the file.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::store::{read_json, write_json};

pub const MENU_DEFAULTS: [(&str, bool); 7] =
    [("care", true), ("notes", true), ("focus", true), ("office", true), ("basecamp", true), ("dnd", true), ("wander", false)];

pub struct Config {
    pub path: PathBuf,
    pub v: Map<String, Value>,
}

fn defaults() -> Map<String, Value> {
    let d = json!({
        "display_name": null, "node_id": null, "passphrase": "blip-office", "network": true, "dnd": false,
        "server": "", "team": "", "relay": "", "manual_peers": "", "sound": false, "volume": 60,
        "auto_update": true, "menu": {}, "mac_dock": false, "theme": "system", "last_update_check": 0,
        "bc_token": "", "bc_refresh": "", "bc_expires": 0, "bc_client_id": "", "bc_client_secret": "",
        "bc_account": "", "bc_account_name": "", "bc_morning": "09:30", "bc_enabled": true
    });
    d.as_object().cloned().unwrap_or_default()
}

impl Config {
    pub fn load(path: PathBuf, user_name: &str) -> Config {
        let mut v = defaults();
        if let Some(Value::Object(saved)) = read_json(&path) {
            v.extend(saved);
        }
        let mut c = Config { path, v };
        let mut changed = false;
        if c.str("display_name").is_empty() {
            let name: String = user_name.chars().take(20).collect();
            c.v.insert("display_name".into(), json!(if name.is_empty() { "Someone".to_string() } else { name }));
            changed = true;
        }
        if c.str("node_id").is_empty() {
            c.v.insert("node_id".into(), json!(crate::Rng::new().hex(32)));
            changed = true;
        }
        let ver = c.f64("cfg_version").max(1.0) as i64;
        if ver < 2 {
            c.v.insert("sound".into(), json!(false));
        }
        if ver < 3 && !c.bool("theme_chosen") {
            c.v.insert("theme".into(), json!("system"));
        }
        if ver < 4 && !c.str("relay").is_empty() && c.str("server").is_empty() {
            let relay = c.str("relay").to_string();
            c.v.insert("server".into(), json!(relay));
        }
        if ver < 5 {
            c.v.insert("team".into(), json!(""));
        }
        if ver < 5 {
            c.v.insert("cfg_version".into(), json!(5));
            changed = true;
        }
        if changed {
            c.save();
        }
        c
    }

    pub fn save(&self) {
        let _ = write_json(&self.path, &Value::Object(self.v.clone()));
    }

    pub fn str(&self, k: &str) -> &str {
        self.v.get(k).and_then(Value::as_str).unwrap_or("")
    }
    pub fn bool(&self, k: &str) -> bool {
        self.v.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
    pub fn f64(&self, k: &str) -> f64 {
        self.v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
    }
    pub fn set(&mut self, k: &str, v: Value) {
        self.v.insert(k.into(), v);
        self.save();
    }

    pub fn menu_on(&self, key: &str) -> bool {
        let dflt = MENU_DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, d)| *d).unwrap_or(true);
        self.v.get("menu").and_then(|m| m.get(key)).and_then(Value::as_bool).unwrap_or(dflt)
    }
    pub fn set_menu(&mut self, key: &str, on: bool) {
        let mut m = self.v.get("menu").and_then(Value::as_object).cloned().unwrap_or_default();
        m.insert(key.into(), json!(on));
        self.set("menu", Value::Object(m));
    }
}
