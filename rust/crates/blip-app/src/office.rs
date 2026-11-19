//! Colleagues: the Office part of the menu and the commands for calls and messages.
//! The network itself arrives with the network step; until then the section shows nobody.

use blip_ui::menu::Item;
use winit::event_loop::ActiveEventLoop;

use crate::app::App;

pub fn menu_items(app: &App) -> Vec<Item> {
    if !app.cfg.menu_on("office") {
        return vec![];
    }
    vec![
        Item::Section { title: "Office".into(), badge: Some(("nobody nearby".into(), false)), link: Some(("All".into(), "window:buddies".into())) },
        Item::PanelRow { icon: "reset", label: "Look again".into(), cmd: "net:rescan".into() },
    ]
}

pub fn run(_app: &mut App, _el: &ActiveEventLoop, _cmd: &str) {}
