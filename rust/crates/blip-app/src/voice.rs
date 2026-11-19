//! Voice calls (filled in with the voice step): until then calls use quick replies only.

use serde_json::{Map, Value};
use winit::event_loop::ActiveEventLoop;

use crate::app::App;

pub fn available() -> bool {
    false
}

pub fn can_call(_app: &App, _pid: &str) -> bool {
    false
}

pub fn packet(_app: &mut App, _src: &str, _pkt: &[u8]) {}

pub fn answer(_app: &mut App, _el: &ActiveEventLoop, _src: &str, _name: &str, _cid: &str) {}

pub fn start_from_reply(_app: &mut App, _el: &ActiveEventLoop, _src: &str, _name: &str, _cid: &str, _v: &Map<String, Value>) -> bool {
    false
}

pub fn remote_end(_app: &mut App, _cid: &str) {}

pub fn run(_app: &mut App, _el: &ActiveEventLoop, _cmd: &str) {}
