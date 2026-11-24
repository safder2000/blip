//! The pet's life: needs that drop over time, care, getting sick, sleeping, growing up and (if
//! nobody cares for it) floating off to the pet planet. Ported from the simulation and care parts
//! of `blip/app.py`; stored in `state.json` with the same fields.
//!
//! Nothing here draws or moves anything: methods return [`Ev`]ents (chirp, signboard, sparkles,
//! sound, a new poop...) and [`BodyCmd`]s (eat, play, sleep) for the app to show.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::store::{read_json, write_json};
use crate::coins::{self, Kind};
use crate::Rng;

pub const HOUR: f64 = 3600.0;
pub const EGG_TIME: f64 = 90.0;
pub const BABY_END: f64 = 12.0 * HOUR;
pub const CHILD_END: f64 = 48.0 * HOUR;
pub const SPECIES: [&str; 7] = ["blip", "kitty", "bunny", "bear", "pup", "chick", "bot"];
pub const COLORS: [&str; 10] = ["auto", "mint", "sky", "gold", "plum", "peach", "pink", "snow", "cocoa", "lime"];

#[derive(Clone, Debug, PartialEq)]
pub enum Ev {
    Chirp { mood: &'static str, icon: Option<&'static str>, secs: f64 },
    Board(String, f64),
    Spawn(&'static str, u32),
    Sound(&'static str),
    /// a new poop should appear next to the pet
    Poop,
    /// all poops were cleaned
    PoopsGone,
    Happy(f64),
    Hatched,
    Grew,
    /// the pet left (health ran out)
    Left,
    /// it fell asleep / woke up on its own
    Asleep(bool),
    /// it found a coin: drop one on the taskbar next to it
    CoinFound,
    /// time to dig: swing the pickaxe, then these go to the wallet
    Dig { coins: u64, find: Option<&'static str> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum BodyCmd {
    Eat { food: &'static str, secs: f64 },
    Play { secs: f64 },
    Sleep,
    Wake,
    Idle(f64),
}

pub struct Pet {
    pub path: PathBuf,
    pub st: Map<String, Value>,
    pub rng: Rng,
    pub events: Vec<Ev>,
    last_pat: f64,
    last_toss: f64,
    /// seconds of earning time until the next find / dig
    find_in: f64,
    dig_in: f64,
    /// set by the app each tick: coins are on, someone is at the PC, nothing fullscreen, no call
    pub earn_ok: bool,
    /// why coins aren't coming right now ("" when they are), for the shop's header
    pub paused_why: &'static str,
}

pub fn fresh_state(gen: i64, name: &str, now: f64) -> Map<String, Value> {
    json!({"name": name, "gen": gen, "stage": "egg", "palette": "baby", "age": 0.0,
           "stats": {"full": 80.0, "happy": 80.0, "energy": 90.0, "hygiene": 100.0, "health": 100.0},
           "sick": false, "asleep": false, "poops": [], "next_poop": null, "snacks": 0.0,
           "care_good": 0.0, "care_total": 0.0, "gone": false, "x": null,
           "wander": true, "wall": now, "bc_seen": {}, "bc_summary_day": "",
           "species": "blip", "color": "auto"})
    .as_object()
    .cloned()
    .unwrap_or_default()
}

impl Pet {
    pub fn load(path: PathBuf, now: f64) -> Pet {
        let mut st = fresh_state(1, "Blip", now);
        if let Some(Value::Object(saved)) = read_json(&path) {
            let mut stats = st["stats"].as_object().cloned().unwrap_or_default();
            if let Some(Value::Object(s)) = saved.get("stats") {
                stats.extend(s.clone());
            }
            st.extend(saved);
            st.insert("stats".into(), Value::Object(stats));
        }
        let mut rng = Rng::new();
        let find_in = coins::FIND_EVERY.0 + rng.f() * (coins::FIND_EVERY.1 - coins::FIND_EVERY.0);
        let dig_in = coins::DIG_EVERY.0 + rng.f() * (coins::DIG_EVERY.1 - coins::DIG_EVERY.0);
        Pet { path, st, rng, events: vec![], last_pat: 0.0, last_toss: 0.0, find_in, dig_in, earn_ok: false, paused_why: "" }
    }

    pub fn save(&self) {
        let _ = write_json(&self.path, &Value::Object(self.st.clone()));
    }

    // ------------------------------------------------ fields
    pub fn s(&self, k: &str) -> &str {
        self.st.get(k).and_then(Value::as_str).unwrap_or("")
    }
    pub fn b(&self, k: &str) -> bool {
        self.st.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
    pub fn f(&self, k: &str) -> f64 {
        self.st.get(k).and_then(Value::as_f64).unwrap_or(0.0)
    }
    pub fn set(&mut self, k: &str, v: Value) {
        self.st.insert(k.into(), v);
    }
    pub fn stat(&self, k: &str) -> f64 {
        self.st.get("stats").and_then(|s| s.get(k)).and_then(Value::as_f64).unwrap_or(0.0)
    }
    pub fn set_stat(&mut self, k: &str, v: f64) {
        if let Some(Value::Object(s)) = self.st.get_mut("stats") {
            s.insert(k.into(), json!(v.clamp(0.0, 100.0)));
        }
    }
    fn add_stat(&mut self, k: &str, d: f64) {
        let v = self.stat(k) + d;
        self.set_stat(k, v);
    }
    pub fn poops(&self) -> Vec<f64> {
        self.st.get("poops").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default()
    }
    pub fn set_poops(&mut self, p: &[f64]) {
        self.set("poops", json!(p));
    }

    pub fn stage(&self) -> &str {
        self.s("stage")
    }
    pub fn gone(&self) -> bool {
        self.b("gone")
    }
    pub fn asleep(&self) -> bool {
        self.b("asleep")
    }
    pub fn hatched(&self) -> bool {
        !self.gone() && self.stage() != "egg"
    }
    pub fn species(&self) -> &str {
        let s = self.s("species");
        if SPECIES.contains(&s) {
            s
        } else {
            "blip"
        }
    }

    /// The colour set to draw with (`palette()` in app.py).
    pub fn palette(&self) -> String {
        if self.gone() {
            return "ghost".into();
        }
        if self.b("sick") {
            return "sick".into();
        }
        let c = self.s("color");
        if c != "auto" && COLORS.contains(&c) {
            return c.into();
        }
        match self.stage() {
            "baby" | "egg" => "baby".into(),
            "child" => "child".into(),
            _ => self.s("palette").into(),
        }
    }

    pub fn stage_name(&self) -> &'static str {
        match self.stage() {
            "egg" => "Egg",
            "baby" => "Baby",
            "child" => "Kid",
            _ => "Grown-up",
        }
    }

    pub fn mood_word(&self) -> &'static str {
        if self.gone() {
            return "on the pet planet";
        }
        if self.stage() == "egg" {
            return "still an egg";
        }
        if self.asleep() {
            return "sleeping";
        }
        if self.b("sick") {
            return "feeling sick";
        }
        let low = [("hungry", self.stat("full")), ("bored", self.stat("happy")), ("tired", self.stat("energy")), ("messy", self.stat("hygiene"))]
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        if low.1 < 25.0 {
            return low.0;
        }
        if self.stat("happy") > 60.0 {
            "happy"
        } else {
            "okay"
        }
    }

    /// The face to draw when nothing special is going on (the app adds held/eat/alert...).
    pub fn mood(&self) -> &'static str {
        if self.gone() {
            return "normal";
        }
        if self.asleep() {
            return "sleep";
        }
        if self.b("sick") {
            return "sick";
        }
        if self.stat("full").min(self.stat("happy")).min(self.stat("hygiene")) < 20.0 || self.stat("energy") < 10.0 {
            return "sad";
        }
        "normal"
    }

    fn chirp(&mut self, mood: &'static str, icon: Option<&'static str>, secs: f64) {
        self.events.push(Ev::Chirp { mood, icon, secs });
    }

    // ------------------------------------------------ time passing
    /// `dt` seconds while Blip runs. `idle` = the pet is idling or walking (it may doze off).
    pub fn step(&mut self, dt: f64, idle: bool) -> Option<BodyCmd> {
        if self.gone() {
            return None;
        }
        self.set("age", json!(self.f("age") + dt));
        if self.stage() == "egg" {
            if self.f("age") >= EGG_TIME {
                return Some(self.hatch());
            }
            return None;
        }
        let h = dt / HOUR;
        let poops = self.poops().len() as f64;
        if self.asleep() {
            self.add_stat("full", -4.0 * h);
            self.add_stat("happy", -1.0 * h);
            // a sleep pill makes this nap recover 3x faster
            self.add_stat("energy", if self.b("dream") { 90.0 } else { 30.0 } * h);
            self.add_stat("hygiene", -1.0 * h);
        } else {
            self.add_stat("full", -10.0 * h);
            self.add_stat("happy", -8.0 * h);
            self.add_stat("energy", -7.0 * h);
            self.add_stat("hygiene", -(2.0 + 6.0 * poops) * h);
        }
        let sick = self.b("sick");
        if sick {
            self.add_stat("health", -6.0 * h);
            self.add_stat("happy", -4.0 * h);
        }
        if self.stat("full") <= 0.0 {
            self.add_stat("health", -15.0 * h);
        }
        if self.stat("hygiene") < 15.0 {
            self.add_stat("health", -6.0 * h);
        }
        if self.stat("happy") <= 0.0 {
            self.add_stat("health", -4.0 * h);
        }
        if !sick && self.stat("full") > 40.0 && self.stat("happy") > 30.0 && self.stat("hygiene") > 40.0 {
            self.add_stat("health", 6.0 * h);
        }
        self.set("snacks", json!((self.f("snacks") - h).max(0.0)));
        if !sick
            && (self.stat("hygiene") < 25.0 || self.stat("full") < 15.0 || self.f("snacks") > 3.0)
            && self.rng.f() < 0.6 * h
        {
            self.set("sick", json!(true));
            self.chirp("sad", Some("pill"), 5.0);
        }
        self.due_poop();
        let mut cmd = None;
        if !self.asleep() && self.stat("energy") < 8.0 && idle {
            self.set("asleep", json!(true));
            self.chirp("sleepy", Some("moon"), 3.0);
            self.events.push(Ev::Asleep(true));
            cmd = Some(BodyCmd::Sleep);
        }
        if self.asleep() && self.stat("energy") >= 100.0 {
            self.set("asleep", json!(false));
            self.chirp("happy", None, 3.0);
            self.events.push(Ev::Asleep(false));
            cmd = Some(BodyCmd::Wake);
        }
        if matches!(self.stage(), "baby" | "child") {
            self.set("care_total", json!(self.f("care_total") + dt));
            if self.stat("full").min(self.stat("happy")).min(self.stat("hygiene")) > 35.0 && !self.b("sick") {
                self.set("care_good", json!(self.f("care_good") + dt));
            }
        }
        if !self.asleep() && self.b("dream") {
            self.set("dream", json!(false));
        }
        self.coin_step(dt, idle);
        self.check_growth();
        if self.stat("health") <= 0.0 {
            self.leave();
        }
        cmd
    }

    /// Time that passed while Blip was closed: gentler, never fatal.
    pub fn catch_up(&mut self, gap: f64) -> Option<BodyCmd> {
        if self.gone() {
            return None;
        }
        self.set("age", json!(self.f("age") + gap));
        if self.stage() == "egg" {
            if self.f("age") >= EGG_TIME {
                return Some(self.hatch());
            }
            return None;
        }
        let h = gap.min(24.0 * HOUR) / HOUR;
        let floor = 15.0;
        let rates: &[(&str, f64)] = if self.asleep() {
            self.add_stat("energy", 30.0 * h);
            &[("full", 1.5), ("happy", 0.5), ("hygiene", 0.5)]
        } else {
            &[("full", 3.3), ("happy", 2.7), ("energy", 2.3), ("hygiene", 1.0)]
        };
        for (k, rate) in rates {
            let v = self.stat(k);
            self.set_stat(k, v.min(floor).max(v - rate * h));
        }
        self.due_poop();
        if self.asleep() && self.stat("energy") >= 100.0 {
            self.set("asleep", json!(false));
        }
        if matches!(self.stage(), "baby" | "child") {
            self.set("care_total", json!(self.f("care_total") + gap));
            self.set("care_good", json!(self.f("care_good") + gap * 0.5));
        }
        self.check_growth();
        if gap > 600.0 {
            self.chirp("love", Some("heart"), 5.0);
            self.events.push(Ev::Happy(3.0));
        }
        None
    }

    fn due_poop(&mut self) {
        if let Some(np) = self.st.get("next_poop").and_then(Value::as_f64) {
            if self.f("age") >= np {
                self.set("next_poop", Value::Null);
                self.add_poop();
            }
        }
    }

    fn add_poop(&mut self) {
        if self.poops().len() >= 6 {
            self.add_stat("hygiene", -10.0);
            return;
        }
        self.add_stat("hygiene", -5.0);
        self.events.push(Ev::Poop); // the app picks the spot and calls `place_poop`
        self.chirp("oops", None, 3.0);
    }

    pub fn place_poop(&mut self, x: f64) {
        let mut p = self.poops();
        p.push(x);
        self.set_poops(&p);
    }

    fn check_growth(&mut self) {
        if self.gone() {
            return;
        }
        if self.stage() == "baby" && self.f("age") >= BABY_END {
            self.set("stage", json!("child"));
            self.grew();
        }
        if self.stage() == "child" && self.f("age") >= CHILD_END {
            let ratio = self.f("care_good") / self.f("care_total").max(1.0);
            let pal = if ratio > 0.75 { "gold" } else if ratio > 0.45 { "sky" } else { "plum" };
            self.set("palette", json!(pal));
            self.set("stage", json!("adult"));
            self.grew();
        }
    }

    fn grew(&mut self) {
        self.events.extend([Ev::Spawn("spark", 8), Ev::Board("Grew up!".into(), 5.0), Ev::Sound("done"), Ev::Happy(3.0), Ev::Grew]);
        self.chirp("wow", None, 3.0);
        self.save();
    }

    fn hatch(&mut self) -> BodyCmd {
        self.set("stage", json!("baby"));
        self.set("age", json!(self.f("age").max(EGG_TIME)));
        self.events.extend([Ev::Spawn("spark", 8), Ev::Sound("done"), Ev::Happy(4.0), Ev::Hatched]);
        self.chirp("wow", Some("heart"), 5.0);
        self.save();
        BodyCmd::Idle(2.0)
    }

    fn leave(&mut self) {
        self.set("gone", json!(true));
        self.set("asleep", json!(false));
        self.set("sick", json!(false));
        self.set_poops(&[]);
        self.events.extend([Ev::PoopsGone, Ev::Sound("sad"), Ev::Left]);
        self.save();
    }

    pub fn new_egg(&mut self, now: f64) {
        let gen = self.f("gen") as i64 + 1;
        let name = self.s("name").to_string();
        let (x, wander) = (self.st.get("x").cloned().unwrap_or(Value::Null), self.st.get("wander").cloned().unwrap_or(json!(true)));
        let bag = self.st.get("bag").cloned();
        self.st = fresh_state(gen, &name, now);
        if let Some(bag) = bag {
            // the wallet, wardrobe and finds belong to the person, not the pet
            self.st.insert("bag".into(), bag);
        }
        self.surprise_look();
        self.set("x", x);
        self.set("wander", wander);
        self.events.extend([Ev::PoopsGone, Ev::Board("A new egg!".into(), 5.0)]);
        self.save();
    }

    /// New pets get a random look, so every desk's pet is different from day one.
    pub fn surprise_look(&mut self) {
        if self.b("look_chosen") {
            return;
        }
        if (self.s("species") == "blip" || self.s("species").is_empty()) && matches!(self.s("color"), "auto" | "") {
            let sp = *self.rng.pick(&SPECIES);
            let c = *self.rng.pick(&COLORS[1..]);
            self.set("species", json!(sp));
            self.set("color", json!(c));
        }
        self.set("look_chosen", json!(true));
    }

    // ------------------------------------------------ care
    /// Why care can't happen right now (and the little reaction), or None if it can.
    /// `busy` = the pet is eating, being held or ringing.
    fn unable(&mut self, what: &str, busy: bool) -> bool {
        if self.gone() {
            self.chirp("happy", None, 3.0);
            return true;
        }
        if self.stage() == "egg" {
            self.events.push(Ev::Board("Still an egg".into(), 3.5));
            return true;
        }
        if self.asleep() && what != "wake" {
            self.chirp("sleepy", None, 3.0);
            return true;
        }
        busy
    }

    pub fn feed(&mut self, kind: &'static str, busy: bool) -> Option<BodyCmd> {
        if self.unable("feed", busy) {
            return None;
        }
        if kind == "meal" {
            if self.stat("full") >= 92.0 {
                self.chirp("no", Some("meal"), 3.0);
                return None;
            }
            self.add_stat("full", 35.0);
            self.add_stat("happy", 3.0);
            if self.st.get("next_poop").map_or(true, Value::is_null) {
                let t = self.f("age") + self.rng.range(25.0, 70.0) * 60.0;
                self.set("next_poop", json!(t));
            }
        } else {
            self.add_stat("full", 10.0);
            self.add_stat("happy", 15.0);
            self.set("snacks", json!(self.f("snacks") + 1.0));
            if self.f("snacks") > 3.0 {
                self.add_stat("health", -6.0);
            }
            if self.st.get("next_poop").map_or(true, Value::is_null) && self.rng.f() < 0.3 {
                let t = self.f("age") + self.rng.range(20.0, 60.0) * 60.0;
                self.set("next_poop", json!(t));
            }
        }
        Some(BodyCmd::Eat { food: kind, secs: 2.5 })
    }

    /// The pet finished eating what `feed`/`medicine` gave it.
    pub fn finished_eating(&mut self, food: &str) {
        if food == "pill" {
            self.chirp("happy", Some("heart"), 3.0);
        } else if food == "snack" && self.f("snacks") > 3.0 {
            self.chirp("sad", Some("pill"), 3.0);
        } else {
            self.chirp("love", None, 3.0);
            self.events.extend([Ev::Happy(2.0), Ev::Spawn("heart", 1)]);
        }
    }

    pub fn play(&mut self, busy: bool) -> Option<BodyCmd> {
        if self.unable("play", busy) {
            return None;
        }
        if self.b("sick") {
            self.chirp("no", Some("pill"), 3.0);
            return None;
        }
        if self.stat("energy") < 15.0 {
            self.chirp("no", Some("moon"), 3.0);
            return None;
        }
        if self.stat("full") < 10.0 {
            self.chirp("no", Some("meal"), 3.0);
            return None;
        }
        self.add_stat("happy", 20.0);
        self.add_stat("energy", -10.0);
        self.add_stat("full", -5.0);
        self.add_stat("hygiene", -3.0);
        self.events.extend([Ev::Spawn("heart", 2), Ev::Sound("pop")]);
        self.chirp("wow", None, 3.0);
        Some(BodyCmd::Play { secs: 3.5 })
    }

    pub fn clean(&mut self) {
        if self.gone() {
            return;
        }
        let n = self.poops().len();
        self.set_poops(&[]);
        let was = self.stat("hygiene");
        self.set_stat("hygiene", 100.0);
        self.events.extend([Ev::PoopsGone, Ev::Spawn("spark", 6)]);
        self.chirp(if n > 0 || was < 70.0 { "happy" } else { "ask" }, None, 3.0);
    }

    /// One poop clicked away.
    pub fn clean_one(&mut self, x: f64) {
        let mut p = self.poops();
        if let Some(i) = p.iter().position(|v| (v - x).abs() < 0.5) {
            p.remove(i);
        }
        self.set_poops(&p);
        self.add_stat("hygiene", 8.0);
        self.events.push(Ev::Spawn("spark", 2));
    }

    pub fn medicine(&mut self, busy: bool) -> Option<BodyCmd> {
        if self.unable("medicine", busy) {
            return None;
        }
        if !self.b("sick") {
            self.chirp("no", Some("pill"), 3.0);
            self.add_stat("happy", -5.0);
            return None;
        }
        self.set("sick", json!(false));
        self.add_stat("health", 25.0);
        Some(BodyCmd::Eat { food: "pill", secs: 2.0 })
    }

    pub fn toggle_sleep(&mut self, busy: bool) -> Option<BodyCmd> {
        if self.unable("wake", busy) {
            return None;
        }
        if self.asleep() {
            self.set("asleep", json!(false));
            self.chirp(if self.stat("energy") < 40.0 { "sleepy" } else { "happy" }, None, 3.0);
            Some(BodyCmd::Wake)
        } else {
            if self.stat("energy") > 85.0 {
                self.chirp("no", Some("moon"), 3.0);
                return None;
            }
            self.set("asleep", json!(true));
            self.chirp("sleepy", Some("moon"), 3.0);
            Some(BodyCmd::Sleep)
        }
    }

    /// A tap on the pet.
    pub fn pat(&mut self, now: f64) {
        if self.gone() {
            self.chirp("happy", None, 3.0);
            return;
        }
        if self.stage() == "egg" {
            return;
        }
        if self.asleep() {
            self.chirp("sleepy", None, 3.0);
            return;
        }
        self.events.extend([Ev::Happy(1.5), Ev::Spawn("heart", 1)]);
        if now - self.last_pat > 2.0 {
            self.add_stat("happy", 4.0);
        }
        self.last_pat = now;
        if self.rng.f() < 0.35 {
            self.chirp("love", None, 2.0);
        }
    }

    /// Thrown (or flung into a ball): that's playing too. A few seconds apart count, so
    /// throwing it around a bit fills the Fun bar, spamming doesn't max it out.
    pub fn tossed(&mut self, ball: bool, now: f64) {
        if !self.hatched() || self.gone() || self.asleep() {
            return;
        }
        if now - self.last_toss > 3.0 {
            self.add_stat("happy", if ball { 8.0 } else { 5.0 });
            self.add_stat("energy", -1.0);
            if self.rng.f() < 0.3 {
                self.chirp("wow", None, 2.0);
            }
        }
        self.last_toss = now;
    }

    /// Being picked up wakes it, and it's not pleased.
    pub fn woken_by_drag(&mut self) {
        if self.asleep() && !self.gone() {
            self.set("asleep", json!(false));
            self.add_stat("happy", -5.0);
            self.chirp("angry", None, 3.0);
        }
    }

    /// Something it wants, said now and then (`nag` in app.py, minus Basecamp).
    pub fn nag(&mut self) -> bool {
        if self.gone() || self.asleep() || self.stage() == "egg" {
            return false;
        }
        if self.b("sick") {
            self.chirp("sad", Some("pill"), 3.0);
        } else if self.poops().len() >= 3 {
            self.chirp("no", Some("clean"), 3.0);
        } else {
            let needs = [(self.stat("full"), "ask", "meal"), (self.stat("happy"), "ask", "play"), (self.stat("energy"), "sleepy", "moon"), (self.stat("hygiene"), "sad", "clean")];
            let (val, mood, icon) = needs.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
            if val < 25.0 {
                self.chirp(mood, Some(icon), 3.0);
            } else if self.rng.f() < 0.06 {
                self.chirp("music", None, 3.0);
            } else {
                return false;
            }
        }
        true
    }
}

// ------------------------------------------------ coins, the shop, finds (docs/coins.md)
impl Pet {
    fn bag(&self) -> Map<String, Value> {
        self.st.get("bag").and_then(Value::as_object).cloned().unwrap_or_default()
    }
    fn set_bag(&mut self, k: &str, v: Value) {
        let mut b = self.bag();
        b.insert(k.into(), v);
        self.st.insert("bag".into(), Value::Object(b));
    }
    fn bag_u64(&self, k: &str) -> u64 {
        self.bag().get(k).and_then(Value::as_u64).unwrap_or(0)
    }
    pub fn coins(&self) -> u64 {
        self.bag_u64("coins")
    }
    /// Add to the wallet.
    pub fn add_coins(&mut self, n: u64) {
        self.set_bag("coins", json!(self.coins() + n));
        self.save();
    }
    /// Pickaxe tier, 0 = none.
    pub fn pick(&self) -> u8 {
        self.bag_u64("pick").min(4) as u8
    }
    /// Wearables owned (ids).
    pub fn owned(&self) -> Vec<String> {
        self.bag().get("owned").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
    }
    pub fn owns(&self, id: &str) -> bool {
        self.owned().iter().any(|o| o == id)
    }
    fn set_owned(&mut self, v: Vec<String>) {
        self.set_bag("owned", json!(v));
    }
    /// What it wears in a slot ("head" / "face").
    pub fn worn(&self, slot: &str) -> Option<String> {
        self.bag().get(slot).and_then(Value::as_str).map(str::to_string).filter(|s| self.owns(s))
    }
    /// Everything it wears (head first).
    pub fn wearing(&self) -> Vec<String> {
        ["head", "face"].iter().filter_map(|s| self.worn(s)).collect()
    }
    pub fn pills(&self, id: &str) -> u64 {
        self.bag().get("pills").and_then(|p| p.get(id)).and_then(Value::as_u64).unwrap_or(0)
    }
    fn set_pills(&mut self, id: &str, n: u64) {
        let mut p = self.bag().get("pills").and_then(Value::as_object).cloned().unwrap_or_default();
        p.insert(id.into(), json!(n));
        self.set_bag("pills", Value::Object(p));
    }
    /// Finds on the shelf: id -> how many.
    pub fn finds(&self) -> Map<String, Value> {
        self.bag().get("finds").and_then(Value::as_object).cloned().unwrap_or_default()
    }
    pub fn add_find(&mut self, id: &str) {
        let mut f = self.finds();
        let n = f.get(id).and_then(Value::as_u64).unwrap_or(0) + 1;
        f.insert(id.into(), json!(n));
        self.set_bag("finds", Value::Object(f));
        if id == "gem" {
            self.set_bag("coins", json!(self.coins() + coins::GEM_COINS));
        }
        self.save();
    }

    /// Buy an item. Err = why not (shown in the shop).
    pub fn buy(&mut self, id: &str) -> Result<(), String> {
        let item = coins::item(id).ok_or("No such thing")?;
        if self.coins() < item.price {
            return Err("Not enough coins yet".into());
        }
        match item.kind {
            Kind::Pick(t) => {
                if self.pick() >= t {
                    return Err("Already has a pickaxe as good".into());
                }
                self.set_bag("pick", json!(t));
            }
            Kind::Head | Kind::Face => {
                if self.owns(id) {
                    return Err("Already yours".into());
                }
                let mut o = self.owned();
                o.push(id.into());
                self.set_owned(o);
            }
            Kind::Pill => self.set_pills(id, self.pills(id) + 1),
            Kind::Game => {
                if self.owns(id) {
                    return Err("Already yours".into());
                }
                let mut o = self.owned();
                o.push(id.into());
                self.set_owned(o);
            }
        }
        self.set_bag("coins", json!(self.coins() - item.price));
        if matches!(item.kind, Kind::Head | Kind::Face) {
            self.wear(id);
        }
        self.events.push(Ev::Sound("pop"));
        self.save();
        Ok(())
    }

    /// Put on something it owns (replacing what's in that slot).
    pub fn wear(&mut self, id: &str) {
        let Some(item) = coins::item(id) else { return };
        let slot = match item.kind {
            Kind::Head => "head",
            Kind::Face => "face",
            _ => return,
        };
        if !self.owns(id) {
            return;
        }
        let mut ever: Vec<String> = self.bag().get("ever_worn").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
        self.set_bag(slot, json!(id));
        if !ever.iter().any(|w| w == id) {
            // the first time it wears something new: one happy chirp, nothing more
            ever.push(id.into());
            self.set_bag("ever_worn", json!(ever));
            if self.hatched() && !self.asleep() {
                self.chirp("love", Some("heart"), 3.0);
            }
        }
        self.save();
    }

    pub fn take_off(&mut self, slot: &str) {
        self.set_bag(slot, Value::Null);
        self.save();
    }

    /// Swap a wearable away for another (a trade both sides agreed to).
    pub fn swap_item(&mut self, give: &str, get: &str) -> bool {
        if !self.owns(give) || self.owns(get) || !coins::tradeable(give) || !coins::tradeable(get) {
            return false;
        }
        for slot in ["head", "face"] {
            if self.worn(slot).as_deref() == Some(give) {
                self.set_bag(slot, Value::Null);
            }
        }
        let mut o: Vec<String> = self.owned().into_iter().filter(|x| x != give).collect();
        o.push(get.into());
        self.set_owned(o);
        self.wear(get);
        self.save();
        true
    }

    /// Use a pill. Err = why not.
    pub fn use_pill(&mut self, id: &str) -> Result<(), String> {
        if self.pills(id) == 0 {
            return Err("None left".into());
        }
        if !self.hatched() {
            return Err("Not now".into());
        }
        match id {
            "energy" => {
                if self.stat("energy") > 90.0 {
                    return Err("It isn't tired".into());
                }
                self.add_stat("energy", 40.0);
                self.events.extend([Ev::Spawn("spark", 3), Ev::Happy(2.0)]);
            }
            "dream" => {
                if !self.asleep() {
                    return Err("Use it while it sleeps".into());
                }
                if self.b("dream") {
                    return Err("Already sleeping deeply".into());
                }
                self.set("dream", json!(true));
                self.events.push(Ev::Spawn("z", 2));
            }
            _ => return Err("Can't use that".into()),
        }
        self.set_pills(id, self.pills(id) - 1);
        self.save();
        Ok(())
    }

    /// Why coins aren't coming, or "" when they are.
    fn coin_pause(&self, idle: bool) -> &'static str {
        if !self.hatched() {
            "not hatched yet"
        } else if !self.earn_ok {
            "you're away or busy"
        } else if self.asleep() {
            "asleep"
        } else if self.b("sick") {
            "sick"
        } else if self.stat("full") < coins::CARED {
            "hungry"
        } else if self.stat("happy") < coins::CARED {
            "bored"
        } else if self.stat("hygiene") < coins::CARED {
            "needs a clean"
        } else if !idle {
            "busy"
        } else {
            ""
        }
    }

    fn coin_step(&mut self, dt: f64, idle: bool) {
        self.paused_why = self.coin_pause(idle);
        if !self.paused_why.is_empty() || dt <= 0.0 || dt > 120.0 {
            return;
        }
        self.find_in -= dt;
        if self.find_in <= 0.0 {
            self.find_in = coins::FIND_EVERY.0 + self.rng.f() * (coins::FIND_EVERY.1 - coins::FIND_EVERY.0);
            self.events.push(Ev::CoinFound);
        }
        if self.pick() > 0 {
            self.dig_in -= dt;
            if self.dig_in <= 0.0 {
                self.dig_in = coins::DIG_EVERY.0 + self.rng.f() * (coins::DIG_EVERY.1 - coins::DIG_EVERY.0);
                let find = (self.rng.f() < coins::FIND_CHANCE).then(|| coins::FINDS[(self.rng.f() * coins::FINDS.len() as f64) as usize % coins::FINDS.len()].0);
                self.events.push(Ev::Dig { coins: coins::dig_yield(self.pick()), find });
            }
        }
    }

    /// Best game of Hoops so far.
    pub fn hoops_best(&self) -> u64 {
        self.bag_u64("hoops_best")
    }

    /// A game of Hoops ended: a coin a basket (at most a few an hour), and the best score.
    /// Returns (coins given, new best?).
    pub fn game_over(&mut self, score: u64, now: f64) -> (u64, bool) {
        let hour = (now / 3600.0) as u64;
        let (h, got) = (self.bag_u64("game_hour"), self.bag_u64("game_hour_coins"));
        let got = if h == hour { got } else { 0 };
        let give = score.min(coins::GAME_COINS_PER_HOUR.saturating_sub(got));
        self.set_bag("game_hour", json!(hour));
        self.set_bag("game_hour_coins", json!(got + give));
        let best = score > self.hoops_best();
        if best {
            self.set_bag("hoops_best", json!(score));
        }
        self.set_bag("coins", json!(self.coins() + give));
        self.save();
        (give, best)
    }

    /// Test hook: make the next find / dig due now.
    pub fn coins_due_now(&mut self) {
        self.find_in = 0.0;
        self.dig_in = 0.0;
    }
}
