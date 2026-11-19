//! The stack of pop-up cards above the pet (`CardStack` in `blip/ui.py`). Each card is its own
//! borderless window; buttons, the close X and timeouts turn into command strings for the app.

use blip_ui::card::{Card, CardHit};
use blip_ui::text::Fonts;
use blip_ui::theme::Theme;
use blip_ui::Canvas;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::overlay::Overlay;
use crate::platform;

pub struct CardWin {
    pub key: String,
    pub card: Card,
    win: Overlay,
    canvas: Canvas,
    pub close_cmd: Option<String>,
    expires: Option<f64>,
    dragged: bool,
    drag_from: Option<(f64, f64, i32, i32)>,
    dirty: bool,
}

pub struct ShowCard<'a> {
    pub key: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub kind: &'a str,
    pub icon: Option<&'static str>,
    pub buttons: Vec<(String, String)>,
    pub timeout: Option<f64>,
    /// run when the card goes away by X or timeout ("" for nothing)
    pub on_close: &'a str,
}

#[derive(Default)]
pub struct CardStack {
    pub cards: Vec<CardWin>,
}

/// What happened in a card, for the app: a command to run (button / close).
pub enum CardOut {
    Run(String),
}

impl CardStack {
    pub fn show(&mut self, el: &ActiveEventLoop, fonts: &mut Fonts, scale: f64, now: f64, c: ShowCard) {
        if !c.key.is_empty() {
            self.cards.retain(|w| w.key != c.key);
        }
        let card = Card::new(c.title, c.body, c.kind, c.icon, c.buttons, scale, fonts);
        let Some(win) = Overlay::new(el, card.w, card.h, 0, 0, false) else { return };
        let canvas = Canvas::new(card.w, card.h);
        let key = if c.key.is_empty() { format!("card{}", (now * 1000.0) as u64) } else { c.key.to_string() };
        self.cards.push(CardWin {
            key,
            card,
            win,
            canvas,
            close_cmd: (!c.on_close.is_empty()).then(|| c.on_close.to_string()),
            expires: c.timeout.map(|t| now + t),
            dragged: false,
            drag_from: None,
            dirty: true,
        });
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut CardWin> {
        self.cards.iter_mut().find(|c| c.key == key)
    }

    /// Close `key` without running its close command.
    pub fn remove(&mut self, key: &str) {
        self.cards.retain(|c| c.key != key);
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.cards.iter().any(|c| c.win.id() == id)
    }

    /// Stack the cards above the pet (newest nearest), inside `work`, and draw what changed.
    pub fn layout(&mut self, theme: &Theme, fonts: &mut Fonts, anchor_x: f64, anchor_y: f64, work: (f64, f64, f64, f64), now: f64) -> Vec<CardOut> {
        let mut out = Vec::new();
        let mut keep = Vec::new();
        for c in std::mem::take(&mut self.cards) {
            if c.expires.map_or(false, |e| now >= e) {
                if let Some(cmd) = &c.close_cmd {
                    out.push(CardOut::Run(cmd.clone()));
                }
                continue; // dropping closes the window
            }
            keep.push(c);
        }
        self.cards = keep;
        let (l, t, r, _) = work;
        let mut y = anchor_y;
        for c in self.cards.iter_mut().rev() {
            let (mut x, mut cy) = (c.win.x, c.win.y);
            if !c.dragged {
                y -= c.card.h as f64 + 4.0;
                x = ((anchor_x - c.card.w as f64 / 2.0).min(r - c.card.w as f64 - 8.0).max(l + 8.0)) as i32;
                cy = (y.max(t + 8.0)) as i32;
            }
            if c.dirty || x != c.win.x || cy != c.win.y {
                if c.dirty {
                    c.card.draw(&mut c.canvas, theme, fonts);
                    c.dirty = false;
                }
                c.win.present(&c.canvas, x, cy);
            }
        }
        out
    }

    pub fn event(&mut self, id: WindowId, ev: &WindowEvent) -> Vec<CardOut> {
        let mut out = Vec::new();
        let Some(i) = self.cards.iter().position(|c| c.win.id() == id) else { return out };
        let c = &mut self.cards[i];
        match ev {
            WindowEvent::CursorMoved { position, .. } => {
                let hit = c.card.hit(position.x as i32, position.y as i32);
                if hit != c.card.hover {
                    c.card.hover = hit;
                    c.dirty = true;
                }
                if let Some((sx, sy, wx, wy)) = c.drag_from {
                    let (mx, my) = platform::cursor();
                    let (nx, ny) = (wx + (mx - sx) as i32, wy + (my - sy) as i32);
                    if nx != c.win.x || ny != c.win.y {
                        c.dragged = true;
                        c.win.present(&c.canvas, nx, ny);
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if c.card.hover.is_some() {
                    c.card.hover = None;
                    c.dirty = true;
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if c.card.hover == Some(CardHit::Header) {
                    let (mx, my) = platform::cursor();
                    c.drag_from = Some((mx, my, c.win.x, c.win.y));
                }
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                c.drag_from = None;
                match c.card.hover {
                    Some(CardHit::Close) => {
                        let cmd = c.close_cmd.clone();
                        self.cards.remove(i);
                        if let Some(cmd) = cmd {
                            out.push(CardOut::Run(cmd));
                        }
                    }
                    Some(CardHit::Button(b)) => {
                        let cmd = c.card.buttons[b].1.clone();
                        let close = c.close_cmd.clone();
                        self.cards.remove(i);
                        out.push(CardOut::Run(cmd));
                        // a button also ends whatever the card was for (e.g. stops the ringing)
                        if let Some(close) = close {
                            out.push(CardOut::Run(close));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        out
    }

    /// Change a card's message (e.g. a call timer).
    pub fn set_body(&mut self, key: &str, body: &str, theme: &Theme, fonts: &mut Fonts, scale: f64) {
        if let Some(c) = self.get_mut(key) {
            if c.card.body != body {
                let buttons = c.card.buttons.clone();
                let hover = c.card.hover;
                c.card = Card::new(&c.card.title.clone(), body, &c.card.kind.clone(), c.card.icon, buttons, scale, fonts);
                c.card.hover = hover;
                c.canvas = Canvas::new(c.card.w, c.card.h);
                c.card.draw(&mut c.canvas, theme, fonts);
                let (x, y) = (c.win.x, c.win.y);
                c.win.present(&c.canvas, x, y);
            }
        }
    }
}
