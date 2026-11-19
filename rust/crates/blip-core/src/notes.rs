//! Notes with optional reminders (`blip/notes.py`), stored in `notes.json`.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::store::{read_json, write_json};
use crate::Rng;

pub type Note = Map<String, Value>;

pub struct Notes {
    pub path: PathBuf,
    pub items: Vec<Note>,
    rng: Rng,
}

pub fn id(n: &Note) -> &str {
    n.get("id").and_then(Value::as_str).unwrap_or("")
}
pub fn text(n: &Note) -> &str {
    n.get("text").and_then(Value::as_str).unwrap_or("")
}
pub fn remind(n: &Note) -> Option<f64> {
    n.get("remind").and_then(Value::as_f64)
}
pub fn done(n: &Note) -> bool {
    n.get("done").and_then(Value::as_bool).unwrap_or(false)
}
pub fn fired(n: &Note) -> bool {
    n.get("fired").and_then(Value::as_bool).unwrap_or(false)
}
fn created(n: &Note) -> f64 {
    n.get("created").and_then(Value::as_f64).unwrap_or(0.0)
}

impl Notes {
    pub fn load(path: PathBuf) -> Notes {
        let items = match read_json(&path) {
            Some(Value::Array(a)) => a.into_iter().filter_map(|v| v.as_object().cloned()).filter(|n| !id(n).is_empty()).collect(),
            _ => vec![],
        };
        Notes { path, items, rng: Rng::new() }
    }

    pub fn save(&self) {
        let _ = write_json(&self.path, &Value::Array(self.items.iter().cloned().map(Value::Object).collect()));
    }

    fn get_mut(&mut self, nid: &str) -> Option<&mut Note> {
        self.items.iter_mut().find(|n| id(n) == nid)
    }

    pub fn add(&mut self, text: &str, remind: Option<f64>, now: f64) -> Note {
        let t: String = text.trim().chars().take(2000).collect();
        let n = json!({"id": self.rng.hex(10), "text": t, "created": now, "remind": remind, "fired": false, "done": false});
        let n = n.as_object().cloned().unwrap_or_default();
        self.items.push(n.clone());
        self.save();
        n
    }

    pub fn set_reminder(&mut self, nid: &str, ts: Option<f64>) {
        if let Some(n) = self.get_mut(nid) {
            n.insert("remind".into(), json!(ts));
            n.insert("fired".into(), json!(false));
            if ts.is_some() {
                n.insert("done".into(), json!(false));
            }
        }
        self.save();
    }

    pub fn snooze(&mut self, nid: &str, minutes: f64, now: f64) {
        self.set_reminder(nid, Some(now + minutes * 60.0));
    }

    pub fn complete(&mut self, nid: &str, is_done: bool) {
        if let Some(n) = self.get_mut(nid) {
            n.insert("done".into(), json!(is_done));
        }
        self.save();
    }

    pub fn delete(&mut self, nid: &str) {
        self.items.retain(|n| id(n) != nid);
        self.save();
    }

    pub fn clear_done(&mut self) {
        self.items.retain(|n| !done(n));
        self.save();
    }

    /// Reminders whose time has come; each is returned once.
    pub fn due(&mut self, now: f64) -> Vec<Note> {
        let mut out = Vec::new();
        for n in self.items.iter_mut() {
            if remind(n).map_or(false, |r| r <= now) && !fired(n) && !done(n) {
                n.insert("fired".into(), json!(true));
                out.push(n.clone());
            }
        }
        if !out.is_empty() {
            self.save();
        }
        out
    }

    pub fn pending(&self) -> usize {
        self.items.iter().filter(|n| remind(n).is_some() && !fired(n) && !done(n)).count()
    }

    /// Upcoming reminders first (soonest first), then other open notes (newest first), then done.
    pub fn ordered(&self) -> Vec<Note> {
        let mut v = self.items.clone();
        let key = |n: &Note| -> (u8, f64) {
            if done(n) {
                (2, -created(n))
            } else if remind(n).is_some() && !fired(n) {
                (0, remind(n).unwrap_or(0.0))
            } else {
                (1, -created(n))
            }
        };
        v.sort_by(|a, b| {
            let (ka, kb) = (key(a), key(b));
            ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        });
        v
    }
}
