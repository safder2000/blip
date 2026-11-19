//! Blip's regular windows (Settings, Notes, Timers, Buddies...). Built in the next step; for now
//! asking for one shows a note on the signboard.

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::app::App;

pub fn open_requested(app: &mut App, _el: &ActiveEventLoop) {
    if !app.want_window.is_empty() {
        app.want_window.clear();
        app.board("Coming soon", 2.5);
    }
}

pub fn event(_app: &mut App, _el: &ActiveEventLoop, _id: WindowId, _ev: WindowEvent) {}
