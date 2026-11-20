//! Render a full sample menu to menu_dark.bmp / menu_light.bmp, to look at without running the app.
//!     cargo run -p blip-ui --example menu_preview -- <out-dir> [scale]

use blip_ui::menu::{CareButton, Item, Menu, Person, PetCard, Tool};
use blip_ui::text::Fonts;
use blip_ui::{theme, Canvas};

fn save_bmp(cv: &Canvas, path: &str) {
    let (w, h) = (cv.w as u32, cv.h as u32);
    let mut out = Vec::new();
    let size = 54 + w * h * 4;
    out.extend_from_slice(b"BM");
    for v in [size, 0, 54, 40] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    for v in [0u32, w * h * 4, 2835, 2835, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for px in &cv.px {
        // see-through pixels shown over a desktop-ish grey
        let a = px >> 24;
        let bg = 0x4a4f5a;
        let c = if a == 255 { *px } else if a == 0 { bg } else { *px };
        out.extend_from_slice(&(c | 0xff00_0000).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).cloned().unwrap_or_else(|| ".".into());
    let scale: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let mut fonts = Fonts::system().expect("a system font");
    let card = PetCard {
        name: "Blip".into(),
        status: "Kid  \u{b7}  okay".into(),
        species: "bunny".into(),
        palette: "peach".into(),
        stage: "child".into(),
        mood: "normal".into(),
        bars: vec![("meal", 70.0, 0xf0a020), ("play", 55.0, 0xef5b8b), ("moon", 80.0, 0x6b7fd7)],
        wear: vec!["party".into(), "shades".into()],
        coins: Some(240),
    };
    let care: Vec<CareButton> = ["meal:Feed", "snack:Snack", "clean:Clean", "pill:Meds", "moon:Sleep", "shop:Shop"]
        .iter()
        .map(|s| {
            let (icon, label) = s.split_once(':').unwrap();
            let icon: &'static str = Box::leak(icon.to_string().into_boxed_str());
            CareButton { icon, label: label.into(), cmd: format!("care:{label}"), enabled: true }
        })
        .collect();
    let person = |name: &str, species: &str, palette: &str, internet: bool, busy: bool| {
        Item::Person(Person {
            name: name.into(),
            species: species.into(),
            palette: palette.into(),
            stage: "adult".into(),
            busy,
            internet,
            call_cmd: format!("call:{name}"),
            msg_cmd: format!("msg:{name}"),
        })
    };
    let items = vec![
        Item::Row { icon: "note", label: "Notes".into(), hint: "1 reminder".into(), check: None, cmd: "notes".into(), enabled: true },
        Item::Tools(vec![
            Tool { icon: "hourglass", label: "Timer".into(), cmd: "timer".into(), badge: String::new() },
            Tool { icon: "bell", label: "Alarm".into(), cmd: "alarm".into(), badge: String::new() },
            Tool { icon: "stopwatch", label: "Stopwatch".into(), cmd: "stopwatch".into(), badge: String::new() },
        ]),
        Item::Sep,
        Item::Section { title: "Office".into(), badge: Some(("3 nearby".into(), true)), link: Some(("All".into(), "buddies".into())) },
        person("Aiswarya Madhavan", "kitty", "sky", false, false),
        person("shaba", "bear", "cocoa", false, false),
        person("viss", "bunny", "snow", true, true),
        Item::Sep,
        Item::toggle("dnd", "Do not disturb", false, "dnd"),
        Item::toggle("walk", "Wander around", true, "wander"),
        Item::Sep,
        Item::row("gear", "Settings", "settings"),
        Item::row("power", "Quit Blip", "quit"),
    ];
    for (name, t) in [("dark", theme::DARK), ("light", theme::LIGHT)] {
        let mut m = Menu::new(Some(card.clone()), care.clone(), items.clone(), scale);
        m.hover = Some(blip_ui::menu::Hit::Row(4));
        let (w, h) = m.size();
        let mut cv = Canvas::new(w, h);
        m.draw(&mut cv, &t, &mut fonts);
        save_bmp(&cv, &format!("{dir}/menu_{name}.bmp"));
    }
    println!("wrote menu_dark.bmp and menu_light.bmp to {dir}");
}
