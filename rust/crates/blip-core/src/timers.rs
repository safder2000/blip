//! Countdown timers, stopwatches and clock alarms (`blip/timers.py`), stored in `timers.json`
//! with the same fields, so the Python and Rust apps read each other's timers.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::clock::{describe_days, fmt_clock, next_fire};
use crate::store::{read_json, write_json};
use crate::Rng;

pub type Item = Map<String, Value>;

pub struct Timers {
    pub path: PathBuf,
    pub items: Vec<Item>,
    rng: Rng,
}

fn f(it: &Item, k: &str) -> f64 {
    it.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}
fn opt(it: &Item, k: &str) -> Option<f64> {
    it.get(k).and_then(Value::as_f64)
}
pub fn kind(it: &Item) -> &str {
    it.get("kind").and_then(Value::as_str).unwrap_or("")
}
pub fn id(it: &Item) -> &str {
    it.get("id").and_then(Value::as_str).unwrap_or("")
}
pub fn label(it: &Item) -> &str {
    it.get("label").and_then(Value::as_str).unwrap_or("")
}
pub fn days(it: &Item) -> Vec<u32> {
    it.get("days").and_then(Value::as_array).map(|a| a.iter().filter_map(|d| d.as_u64().map(|d| d as u32)).collect()).unwrap_or_default()
}

impl Timers {
    pub fn load(path: PathBuf) -> Timers {
        let items = match read_json(&path) {
            Some(Value::Array(a)) => a.into_iter().filter_map(|v| v.as_object().cloned()).collect(),
            _ => vec![],
        };
        Timers { path, items, rng: Rng::new() }
    }

    pub fn save(&self) {
        let _ = write_json(&self.path, &Value::Array(self.items.iter().cloned().map(Value::Object).collect()));
    }

    pub fn get(&self, iid: &str) -> Option<&Item> {
        self.items.iter().find(|i| id(i) == iid)
    }
    fn get_mut(&mut self, iid: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| id(i) == iid)
    }

    fn push(&mut self, v: Value) -> Item {
        let it = v.as_object().cloned().unwrap_or_default();
        self.items.push(it.clone());
        self.save();
        it
    }

    pub fn add_timer(&mut self, secs: f64, label: &str, now: f64) -> Item {
        let id = self.rng.hex(10);
        self.push(json!({"id": id, "kind": "timer", "label": label, "dur": secs, "end": now + secs, "left": null}))
    }

    pub fn add_stopwatch(&mut self, label: &str, now: f64) -> Item {
        let id = self.rng.hex(10);
        self.push(json!({"id": id, "kind": "stopwatch", "label": label, "start": now, "acc": 0.0, "running": true}))
    }

    pub fn add_alarm(&mut self, hh: u32, mm: u32, days: &[u32], label: &str, now: f64) -> Item {
        let mut d = days.to_vec();
        d.sort_unstable();
        d.dedup();
        let id = self.rng.hex(10);
        let next = next_fire(hh, mm, &d, now);
        self.push(json!({"id": id, "kind": "alarm", "label": label, "hh": hh, "mm": mm, "days": d,
                         "enabled": true, "next": next, "snooze": null}))
    }

    pub fn remaining(it: &Item, now: f64) -> f64 {
        opt(it, "left").unwrap_or_else(|| (f(it, "end") - now).max(0.0))
    }

    pub fn elapsed(it: &Item, now: f64) -> f64 {
        f(it, "acc") + if it.get("running").and_then(Value::as_bool).unwrap_or(false) { now - f(it, "start") } else { 0.0 }
    }

    pub fn describe(it: &Item, now: f64) -> String {
        match kind(it) {
            "timer" => format!(
                "Timer {}{}",
                fmt_clock(Self::remaining(it, now), true),
                if opt(it, "left").is_some() { " (paused)" } else { "" }
            ),
            "stopwatch" => format!(
                "Stopwatch {}{}",
                fmt_clock(Self::elapsed(it, now), false),
                if it.get("running").and_then(Value::as_bool).unwrap_or(false) { "" } else { " (paused)" }
            ),
            _ => {
                let enabled = it.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                let mut st = if enabled { describe_days(&days(it)) } else { "off".into() };
                if opt(it, "snooze").is_some() {
                    st.push_str(", snoozed");
                }
                format!("Alarm {:02}:{:02} ({st})", f(it, "hh") as u32, f(it, "mm") as u32)
            }
        }
    }

    /// (icon, text, id) for the pixel sign over the pet.
    pub fn display(&self, now: f64) -> Option<(&'static str, String, String)> {
        let mut running: Vec<&Item> = self.items.iter().filter(|i| kind(i) == "timer" && opt(i, "left").is_none()).collect();
        running.sort_by(|a, b| f(a, "end").total_cmp(&f(b, "end")));
        if let Some(t) = running.first() {
            return Some(("hourglass", fmt_clock(Self::remaining(t, now), true), id(t).into()));
        }
        if let Some(t) = self.items.iter().find(|i| kind(i) == "timer") {
            return Some(("hourglass", fmt_clock(f(t, "left"), true), id(t).into()));
        }
        let sws: Vec<&Item> = self.items.iter().filter(|i| kind(i) == "stopwatch").collect();
        if !sws.is_empty() {
            let sw = sws.iter().find(|s| s.get("running").and_then(Value::as_bool).unwrap_or(false)).unwrap_or(&sws[0]);
            return Some(("stopwatch", fmt_clock(Self::elapsed(sw, now), false), id(sw).into()));
        }
        let snoozed = self.items.iter().filter(|i| kind(i) == "alarm" && opt(i, "snooze").is_some()).min_by(|a, b| f(a, "snooze").total_cmp(&f(b, "snooze")));
        snoozed.map(|a| ("bell", fmt_clock(f(a, "snooze") - now, true), id(a).to_string()))
    }

    pub fn active_count(&self) -> usize {
        self.items.iter().filter(|i| matches!(kind(i), "timer" | "stopwatch")).count()
    }

    pub fn toggle_pause(&mut self, iid: &str, now: f64) {
        let Some(it) = self.get_mut(iid) else { return };
        match kind(it).to_string().as_str() {
            "timer" => {
                if opt(it, "left").is_none() {
                    let left = (f(it, "end") - now).max(0.0);
                    it.insert("left".into(), json!(left));
                } else {
                    let left = f(it, "left");
                    it.insert("end".into(), json!(now + left));
                    it.insert("left".into(), Value::Null);
                }
            }
            "stopwatch" => {
                if it.get("running").and_then(Value::as_bool).unwrap_or(false) {
                    let acc = f(it, "acc") + now - f(it, "start");
                    it.insert("acc".into(), json!(acc));
                    it.insert("running".into(), json!(false));
                } else {
                    it.insert("start".into(), json!(now));
                    it.insert("running".into(), json!(true));
                }
            }
            "alarm" => {
                let on = !it.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                it.insert("enabled".into(), json!(on));
                it.insert("snooze".into(), Value::Null);
                let next = if on { next_fire(f(it, "hh") as u32, f(it, "mm") as u32, &days(it), now) } else { None };
                it.insert("next".into(), json!(next));
            }
            _ => {}
        }
        self.save();
    }

    pub fn reset(&mut self, iid: &str, now: f64) {
        if let Some(it) = self.get_mut(iid) {
            match kind(it) {
                "stopwatch" => {
                    it.insert("start".into(), json!(now));
                    it.insert("acc".into(), json!(0.0));
                }
                "timer" => {
                    let dur = f(it, "dur");
                    it.insert("end".into(), json!(now + dur));
                    it.insert("left".into(), Value::Null);
                }
                _ => {}
            }
        }
        self.save();
    }

    pub fn remove(&mut self, iid: &str) {
        self.items.retain(|i| id(i) != iid);
        self.save();
    }

    pub fn snooze(&mut self, iid: &str, mins: f64, now: f64) {
        let (k, lab) = self.get(iid).map(|i| (kind(i).to_string(), label(i).to_string())).unwrap_or(("timer".into(), String::new()));
        if k == "alarm" {
            if let Some(it) = self.get_mut(iid) {
                it.insert("snooze".into(), json!(now + mins * 60.0));
            }
            self.save();
        } else if k == "timer" {
            self.add_timer(mins * 60.0, &lab, now);
        }
    }

    /// Advance; returns what just went off. Finished timers are removed.
    pub fn tick(&mut self, now: f64) -> Vec<Item> {
        let mut fired = Vec::new();
        let mut keep = Vec::new();
        let mut changed = false;
        for mut it in std::mem::take(&mut self.items) {
            match kind(&it) {
                "timer" if opt(&it, "left").is_none() && f(&it, "end") <= now => {
                    fired.push(it);
                    changed = true;
                    continue;
                }
                "alarm" => {
                    let enabled = it.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                    if opt(&it, "snooze").map_or(false, |s| s <= now) {
                        it.insert("snooze".into(), Value::Null);
                        fired.push(it.clone());
                        changed = true;
                    } else if enabled && opt(&it, "next").map_or(false, |n| n <= now) {
                        fired.push(it.clone());
                        changed = true;
                        let d = days(&it);
                        if d.is_empty() {
                            it.insert("enabled".into(), json!(false));
                            it.insert("next".into(), Value::Null);
                        } else {
                            let next = next_fire(f(&it, "hh") as u32, f(&it, "mm") as u32, &d, now);
                            it.insert("next".into(), json!(next));
                        }
                    } else if enabled && opt(&it, "next").is_none() {
                        let next = next_fire(f(&it, "hh") as u32, f(&it, "mm") as u32, &days(&it), now);
                        it.insert("next".into(), json!(next));
                        changed = true;
                    }
                }
                _ => {}
            }
            keep.push(it);
        }
        self.items = keep;
        if changed {
            self.save();
        }
        fired
    }
}
