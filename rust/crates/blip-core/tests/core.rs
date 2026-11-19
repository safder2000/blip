//! The model, checked the way the Python tests check it (tests/test_timers.py, tests/test_notes.py)
//! plus the pet's care and life.

use blip_core::clock::{describe_when, fmt_clock, next_fire, parse_clock, parse_duration, parse_when};
use blip_core::config::Config;
use blip_core::notes::{self, Notes};
use blip_core::pet::{BodyCmd, Ev, Pet, BABY_END, CHILD_END, EGG_TIME};
use blip_core::timers::{self, Timers};
use chrono::{Datelike, Local, TimeZone};

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("blip-core-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn at(day: u32, h: u32, m: u32) -> f64 {
    Local.with_ymd_and_hms(2026, 11, day, h, m, 0).unwrap().timestamp() as f64
}

#[test]
fn parsing_like_python() {
    assert_eq!(parse_duration("25"), Some(1500.0));
    assert_eq!(parse_duration("1:30"), Some(5400.0));
    assert_eq!(parse_duration("90s"), Some(90.0));
    assert_eq!(parse_duration("2h 15m"), Some(8100.0));
    assert_eq!(parse_duration("45 min"), Some(2700.0));
    assert_eq!(parse_duration("0"), None);
    assert_eq!(parse_duration("abc"), None);
    assert_eq!(parse_clock("9:30"), Some((9, 30)));
    assert_eq!(parse_clock("930"), Some((9, 30)));
    assert_eq!(parse_clock("5pm"), Some((17, 0)));
    assert_eq!(parse_clock("5:15 pm"), Some((17, 15)));
    assert_eq!(parse_clock("12am"), Some((0, 0)));
    assert_eq!(parse_clock("25:00"), None);
    assert_eq!(fmt_clock(1499.2, true), "25:00");
    assert_eq!(fmt_clock(3725.0, false), "1:02:05");
    let now = at(19, 14, 0);
    assert_eq!(parse_when("in 20m", now), Some(now + 1200.0));
    assert_eq!(parse_when("5pm", now), Some(at(19, 17, 0)));
    assert_eq!(parse_when("17:30", now), Some(at(19, 17, 30)));
    assert_eq!(parse_when("at 9", now), Some(at(20, 9, 0)));
    assert_eq!(parse_when("tomorrow", now), Some(at(20, 9, 0)));
    assert_eq!(parse_when("tomorrow 10", now), Some(at(20, 10, 0)));
    assert_eq!(parse_when("today 9am", now), None);
    assert_eq!(parse_when("banana", now), None);
    assert_eq!(describe_when(now + 1200.0, now), "in 20 min");
    assert_eq!(describe_when(at(20, 9, 0), now), "tomorrow 09:00");
}

#[test]
fn alarms_next_fire_on_the_right_days() {
    let now = at(19, 14, 0); // a Thursday
    assert_eq!(Local.timestamp_opt(now as i64, 0).unwrap().weekday().num_days_from_monday(), 3);
    assert_eq!(next_fire(9, 0, &[], now), Some(at(20, 9, 0)));
    assert_eq!(next_fire(15, 0, &[], now), Some(at(19, 15, 0)));
    assert_eq!(next_fire(9, 0, &[0, 1, 2, 3, 4], at(21, 10, 0)), Some(at(23, 9, 0))); // Sat -> Mon
}

#[test]
fn timers_stopwatches_alarms() {
    let d = tmp("timers");
    let mut t = Timers::load(d.join("timers.json"));
    let now = 1000.0;
    let tm = t.add_timer(60.0, "tea", now);
    let sw = t.add_stopwatch("", now);
    t.add_alarm(9, 0, &[], "wake", now);
    assert_eq!(t.display(now + 10.0).unwrap().1, "00:50");
    t.toggle_pause(timers::id(&tm), now + 20.0);
    assert_eq!(Timers::remaining(t.get(timers::id(&tm)).unwrap(), now + 500.0), 40.0);
    t.toggle_pause(timers::id(&tm), now + 500.0);
    assert!(t.tick(now + 530.0).is_empty());
    let fired = t.tick(now + 541.0);
    assert_eq!(fired.len(), 1);
    assert_eq!(timers::label(&fired[0]), "tea");
    assert!(t.get(timers::id(&tm)).is_none(), "finished timers go away");
    assert_eq!(Timers::elapsed(t.get(timers::id(&sw)).unwrap(), now + 90.0), 90.0);
    // saved in the Python format
    let again = Timers::load(d.join("timers.json"));
    assert_eq!(again.items.len(), 2);
    assert!(again.items.iter().any(|i| timers::kind(i) == "alarm" && i["hh"] == 9));
}

#[test]
fn notes_and_reminders() {
    let d = tmp("notes");
    let mut n = Notes::load(d.join("notes.json"));
    let now = 5000.0;
    let plain = n.add("buy milk", None, now);
    let r = n.add("call the client", Some(now + 600.0), now);
    assert_eq!(notes::id(&n.ordered()[0]), notes::id(&r));
    assert!(n.due(now).is_empty());
    assert_eq!(n.due(now + 601.0).len(), 1);
    assert!(n.due(now + 700.0).is_empty(), "fires once");
    n.snooze(notes::id(&r), 10.0, now + 700.0);
    assert_eq!(n.due(now + 1301.0).len(), 1);
    n.complete(notes::id(&plain), true);
    let again = Notes::load(d.join("notes.json"));
    assert_eq!(again.items.len(), 2);
    assert_eq!(again.pending(), 0);
}

#[test]
fn config_defaults_and_migrations() {
    let d = tmp("config");
    let p = d.join("config.json");
    std::fs::write(&p, r#"{"cfg_version": 4, "server": "https://blip.example.com", "team": "c0ffee", "theme": "dark", "theme_chosen": true, "someday": 1}"#).unwrap();
    let c = Config::load(p.clone(), "safder");
    assert_eq!(c.str("team"), "", "old made-up team codes are dropped");
    assert_eq!(c.str("theme"), "dark", "a picked theme stays");
    assert_eq!(c.str("display_name"), "safder");
    assert_eq!(c.str("node_id").len(), 32);
    assert!(c.menu_on("care") && !c.menu_on("wander"));
    let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    assert_eq!(raw["someday"], 1, "keys we don't know are kept");
}

fn adult() -> Pet {
    let d = tmp("pet");
    let mut p = Pet::load(d.join("state.json"), 0.0);
    p.set("stage", serde_json::json!("adult"));
    p.set("age", serde_json::json!(CHILD_END + 10.0));
    p
}

#[test]
fn an_egg_hatches_and_grows_up() {
    let d = tmp("egg");
    let mut p = Pet::load(d.join("state.json"), 0.0);
    assert_eq!(p.stage(), "egg");
    assert!(p.step(EGG_TIME - 1.0, true).is_none());
    assert_eq!(p.step(2.0, true), Some(BodyCmd::Idle(2.0)));
    assert_eq!(p.stage(), "baby");
    assert!(p.events.contains(&Ev::Hatched));
    p.catch_up(BABY_END);
    assert_eq!(p.stage(), "child");
    p.catch_up(CHILD_END);
    assert_eq!(p.stage(), "adult");
    assert!(["gold", "sky", "plum"].contains(&p.s("palette")));
}

#[test]
fn care() {
    let mut p = adult();
    p.set_stat("full", 40.0);
    assert!(matches!(p.feed("meal", false), Some(BodyCmd::Eat { food: "meal", .. })));
    assert_eq!(p.stat("full"), 75.0);
    assert!(p.feed("meal", true).is_none(), "busy eating");
    p.set_stat("full", 95.0);
    assert!(p.feed("meal", false).is_none(), "too full");
    p.set_stat("energy", 50.0);
    assert!(matches!(p.play(false), Some(BodyCmd::Play { .. })));
    p.set_stat("energy", 80.0);
    assert_eq!(p.toggle_sleep(false), Some(BodyCmd::Sleep));
    assert!(p.asleep());
    assert!(p.play(false).is_none(), "asleep");
    assert_eq!(p.toggle_sleep(false), Some(BodyCmd::Wake));
    p.place_poop(100.0);
    p.place_poop(200.0);
    p.clean_one(100.0);
    assert_eq!(p.poops(), vec![200.0]);
    p.clean();
    assert!(p.poops().is_empty());
    assert_eq!(p.stat("hygiene"), 100.0);
    assert!(p.medicine(false).is_none(), "not sick");
    p.set("sick", serde_json::json!(true));
    assert!(matches!(p.medicine(false), Some(BodyCmd::Eat { food: "pill", .. })));
    assert!(!p.b("sick"));
}

#[test]
fn neglect_while_running_is_fatal_but_time_away_is_not() {
    let mut p = adult();
    p.catch_up(30.0 * 24.0 * 3600.0);
    assert!(!p.gone(), "being closed never kills it");
    assert!(p.stat("full") > 0.0 && p.stat("full") <= 15.0);
    let mut q = adult();
    for _ in 0..(48 * 60) {
        q.step(60.0, false);
        if q.gone() {
            break;
        }
    }
    assert!(q.gone(), "two days of neglect while running");
    assert!(q.events.contains(&Ev::Left));
}

#[test]
fn reads_the_python_state_file() {
    let d = tmp("pystate");
    std::fs::write(
        d.join("state.json"),
        r#"{"name": "Bun", "gen": 2, "stage": "child", "stats": {"full": 12.5}, "species": "bunny", "color": "peach", "poops": [1500.0], "x": 2384}"#,
    )
    .unwrap();
    let p = Pet::load(d.join("state.json"), 0.0);
    assert_eq!(p.s("name"), "Bun");
    assert_eq!(p.stat("full"), 12.5);
    assert_eq!(p.stat("health"), 100.0, "missing stats get defaults");
    assert_eq!(p.palette(), "peach");
    assert_eq!(p.mood_word(), "hungry");
    assert_eq!(p.poops(), vec![1500.0]);
}
