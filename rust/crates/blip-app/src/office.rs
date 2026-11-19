//! Colleagues: the Office part of the menu, the people list, inbox and the commands for calls and
//! messages. The network itself arrives with the network step; until then nobody shows up.

use blip_pet::sprites::Color;
use blip_ui::menu::Item;
use blip_ui::theme::Theme;
use egui::Ui;
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::gui::{self, Kind};

pub const DEFAULT_GROUP: &str = "blip-office";

#[derive(Clone, Debug, Default)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub status: String,
    pub species: String,
    pub palette: String,
    pub stage: String,
    pub ip: String,
    pub via: String,
}

#[derive(Clone, Debug, Default)]
pub struct Msg {
    pub ts: f64,
    pub text: String,
    pub name: String,
    pub peer: String,
    pub out: bool,
}

pub fn menu_items(app: &App) -> Vec<Item> {
    if !app.cfg.menu_on("office") {
        return vec![];
    }
    vec![
        Item::Section { title: "Office".into(), badge: Some(("nobody nearby".into(), false)), link: Some(("All".into(), "window:buddies".into())) },
        Item::PanelRow { icon: "reset", label: "Look again".into(), cmd: "net:rescan".into() },
    ]
}

pub fn peers(_app: &App) -> Vec<Peer> {
    vec![]
}

pub fn inbox(_app: &App) -> Vec<Msg> {
    vec![]
}

pub fn peer_name(app: &App, id: &str) -> Option<String> {
    peers(app).into_iter().find(|p| p.id == id).map(|p| p.name)
}

/// The one-line network state and its colour.
pub fn status(app: &App) -> (String, Color) {
    if !app.cfg.bool("network") {
        return ("Office network is off (Settings).".into(), app.theme.danger);
    }
    ("0 pets nearby".into(), app.theme.warn)
}

pub fn server_status(app: &App) -> (String, Color) {
    if app.cfg.str("server").is_empty() {
        return ("Not connected. Your pet only meets pets on this network.".into(), app.theme.muted);
    }
    ("Connecting\u{2026}".into(), app.theme.warn)
}

pub fn this_pc() -> String {
    let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "this computer".into());
    format!("This PC: {host}")
}

pub fn run(_app: &mut App, _el: &ActiveEventLoop, _cmd: &str) {}

pub fn tasks_ui(ui: &mut Ui, t: &Theme, _app: &mut App, _el: &ActiveEventLoop) {
    gui::empty_state(ui, t, "Basecamp isn't connected", "Connect in Settings \u{2192} Basecamp.");
}

#[allow(clippy::too_many_arguments)]
pub fn basecamp_page(ui: &mut Ui, t: &Theme, _app: &mut App, _el: &ActiveEventLoop, _tok: &mut String, _id: &mut String, _secret: &mut String, _morning: &mut String) {
    gui::card(ui, t, 14, |ui| {
        gui::h2(ui, "Connect Basecamp");
        gui::muted(ui, t, "Blip will remind you about to-dos that are due today or overdue.");
        let _ = gui::button(ui, t, "Sign in with Basecamp", Some("open"), Kind::Primary, false, false);
    });
}
