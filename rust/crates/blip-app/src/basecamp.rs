//! Basecamp to-dos (filled in with the Basecamp step).

use blip_ui::theme::Theme;
use egui::Ui;
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::gui::{self, Kind};

pub fn tasks_ui(ui: &mut Ui, t: &Theme, _app: &mut App, _el: &ActiveEventLoop) {
    gui::empty_state(ui, t, "Basecamp isn't connected", "Connect in Settings \u{2192} Basecamp.");
}

#[allow(clippy::too_many_arguments)]
pub fn settings_page(ui: &mut Ui, t: &Theme, _app: &mut App, _el: &ActiveEventLoop, _tok: &mut String, _id: &mut String, _secret: &mut String, _morning: &mut String) {
    gui::card(ui, t, 14, |ui| {
        gui::h2(ui, "Connect Basecamp");
        gui::muted(ui, t, "Blip will remind you about to-dos that are due today or overdue.");
        let _ = gui::button(ui, t, "Sign in with Basecamp", Some("open"), Kind::Primary, false, false);
    });
}
