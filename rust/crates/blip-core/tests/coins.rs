//! Coins, the shop and swaps (docs/coins.md).

use blip_core::pet::{Ev, Pet};
use serde_json::json;

fn grown_pet(name: &str) -> Pet {
    let dir = std::env::temp_dir().join(format!("blip-coins-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::remove_file(dir.join("state.json"));
    let mut p = Pet::load(dir.join("state.json"), 1000.0);
    p.set("stage", json!("adult"));
    p.set("age", json!(200000.0));
    for k in ["full", "happy", "energy", "hygiene", "health"] {
        p.set_stat(k, 80.0);
    }
    p
}

#[test]
fn buying_wearing_and_pills() {
    let mut p = grown_pet("buy");
    assert!(p.buy("pick1").is_err(), "no coins yet");
    p.add_coins(120);
    p.buy("pick1").unwrap();
    assert_eq!(p.pick(), 1);
    assert!(p.buy("pick1").is_err(), "already has it");
    p.buy("shades").unwrap();
    assert_eq!(p.wearing(), vec!["shades".to_string()], "a new thing is put on right away");
    p.buy("party").unwrap();
    assert_eq!(p.wearing(), vec!["party".to_string(), "shades".to_string()]);
    assert_eq!(p.coins(), 120 - 15 - 30 - 40);
    p.take_off("face");
    assert_eq!(p.wearing(), vec!["party".to_string()]);
    p.buy("energy").unwrap();
    p.set_stat("energy", 20.0);
    p.use_pill("energy").unwrap();
    assert_eq!(p.stat("energy"), 60.0);
    assert!(p.use_pill("energy").is_err(), "none left");
}

#[test]
fn swaps_are_one_for_one_wearables_only() {
    let mut p = grown_pet("swap");
    p.add_coins(500);
    p.buy("shades").unwrap();
    p.buy("pick2").unwrap_or(());
    assert!(!p.swap_item("pick1", "party"), "pickaxes can't be swapped");
    assert!(p.swap_item("shades", "party"));
    assert!(!p.owns("shades") && p.owns("party"));
    assert!(!p.swap_item("shades", "crown"), "can't give what it doesn't have");
}

#[test]
fn coins_come_only_while_cared_for_and_someone_is_there() {
    let mut p = grown_pet("earn");
    p.coins_due_now();
    p.earn_ok = false;
    p.step(1.0, true);
    assert!(!p.events.iter().any(|e| matches!(e, Ev::CoinFound)), "nobody at the PC: nothing");
    assert!(!p.paused_why.is_empty());
    p.earn_ok = true;
    p.set_stat("happy", 10.0);
    p.step(1.0, true);
    assert_eq!(p.paused_why, "bored");
    assert!(!p.events.iter().any(|e| matches!(e, Ev::CoinFound)));
    p.set_stat("happy", 80.0);
    p.step(1.0, true);
    assert_eq!(p.paused_why, "");
    assert!(p.events.iter().any(|e| matches!(e, Ev::CoinFound)), "cared for and someone there: a coin");
    assert!(!p.events.iter().any(|e| matches!(e, Ev::Dig { .. })), "no pickaxe, no digging");
}

#[test]
fn the_bag_survives_a_new_egg() {
    let mut p = grown_pet("egg");
    p.add_coins(50);
    p.buy("shades").unwrap();
    p.new_egg(2000.0);
    assert_eq!(p.coins(), 20);
    assert!(p.owns("shades"));
}

#[test]
fn game_coins_are_one_a_basket_and_capped_each_hour() {
    let mut p = grown_pet("game");
    let t = 7200.0 * 10.0;
    assert_eq!(p.game_coins_left(t), blip_core::coins::GAME_COINS_PER_HOUR);
    let mut paid = 0;
    for _ in 0..30 {
        if p.game_coin(t) {
            paid += 1;
        }
    }
    assert_eq!(paid, blip_core::coins::GAME_COINS_PER_HOUR);
    assert_eq!(p.coins(), paid);
    assert_eq!(p.game_coins_left(t + 10.0), 0);
    assert!(p.game_coin(t + 3600.0), "a new hour pays again");
    assert!(p.game_best(14) && !p.game_best(9) && p.hoops_best() == 14);
}
