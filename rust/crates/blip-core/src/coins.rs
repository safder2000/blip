//! Coins, the shop's items and the finds shelf (see `docs/coins.md`). Kept small and calm on
//! purpose: a handful of items, slow earning, nothing that asks for attention.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// pickaxe tier 1..=4 (a better one replaces the old one)
    Pick(u8),
    Head,
    Face,
    /// used up when used
    Pill,
    /// a little game, downloaded only by people who get it
    Game,
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub price: u64,
    pub about: &'static str,
}

pub const ITEMS: [Item; 13] = [
    Item { id: "pick1", name: "Wooden pickaxe", kind: Kind::Pick(1), price: 15, about: "Digs up 2 coins now and then." },
    Item { id: "pick2", name: "Stone pickaxe", kind: Kind::Pick(2), price: 150, about: "Digs up 3 coins a time." },
    Item { id: "pick3", name: "Gold pickaxe", kind: Kind::Pick(3), price: 450, about: "Digs up 5 coins a time." },
    Item { id: "pick4", name: "Diamond pickaxe", kind: Kind::Pick(4), price: 1200, about: "Digs up 8 coins a time." },
    Item { id: "shades", name: "Sunglasses", kind: Kind::Face, price: 30, about: "Too cool." },
    Item { id: "specs", name: "Round glasses", kind: Kind::Face, price: 50, about: "Very studious." },
    Item { id: "party", name: "Party hat", kind: Kind::Head, price: 40, about: "Every day is a party." },
    Item { id: "flower", name: "Flower", kind: Kind::Head, price: 80, about: "A little flower to wear." },
    Item { id: "hardhat", name: "Hard hat", kind: Kind::Head, price: 120, about: "Safety first while digging." },
    Item { id: "crown", name: "Crown", kind: Kind::Head, price: 1500, about: "For the very dedicated." },
    Item { id: "energy", name: "Energy pill", kind: Kind::Pill, price: 20, about: "+40 Energy right away." },
    Item { id: "dream", name: "Sleep pill", kind: Kind::Pill, price: 15, about: "Use while it sleeps: this nap recovers 3x faster." },
    Item { id: "hoops", name: "Hoops", kind: Kind::Game, price: 4, about: "Sling your pet at a hoop." },
];

/// Coins a game can give in an hour at most (baskets pay 1 each).
pub const GAME_COINS_PER_HOUR: u64 = 20;

/// Curios that sometimes turn up while digging: (id, name). The gem is also worth GEM_COINS.
pub const FINDS: [(&str, &str); 5] = [("cap", "Bottle cap"), ("key", "Old key"), ("shell", "Snail shell"), ("fossil", "Fossil"), ("gem", "Gem")];
pub const GEM_COINS: u64 = 25;

pub const PICK_NAMES: [&str; 5] = ["No pickaxe", "Wooden", "Stone", "Gold", "Diamond"];

/// Coins a dig gives with pickaxe `tier`.
pub fn dig_yield(tier: u8) -> u64 {
    match tier {
        1 => 2,
        2 => 3,
        3 => 5,
        4 => 8,
        _ => 0,
    }
}

// how often, in seconds of awake, cared-for, someone-is-there time
pub const FIND_EVERY: (f64, f64) = (25.0 * 60.0, 35.0 * 60.0);
pub const DIG_EVERY: (f64, f64) = (20.0 * 60.0, 30.0 * 60.0);
pub const FIND_CHANCE: f64 = 0.03;
/// stats needed for coins to come
pub const CARED: f64 = 35.0;

pub fn item(id: &str) -> Option<&'static Item> {
    ITEMS.iter().find(|i| i.id == id)
}

pub fn find_name(id: &str) -> &'static str {
    FINDS.iter().find(|f| f.0 == id).map(|f| f.1).unwrap_or("Something")
}

/// Can this item be swapped with a buddy? Only wearables: pickaxes, pills and finds show effort.
pub fn tradeable(id: &str) -> bool {
    matches!(item(id).map(|i| i.kind), Some(Kind::Head | Kind::Face))
}
