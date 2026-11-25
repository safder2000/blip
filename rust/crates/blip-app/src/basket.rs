//! The basket (from the shop): somewhere to put the pet away. It sits on the taskbar; drop or
//! throw the pet in (or pick "Into the basket" and it hops in itself), and it curls up inside
//! with just its head peeking over the rim.
//!
//! Grab the basket anywhere and it hangs from that point under the mouse, swinging with how you
//! move it. Let go and it flies with that momentum, spinning; it lands upright, on its side or
//! upside down depending on how it comes down, and if it tips far enough the pet tumbles out.

use blip_pet::body::{Action, Body, STEP};
use blip_pet::sprites::{self, Color, Sprite, GH};
use blip_ui::Canvas;
use serde_json::{json, Value};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::app::App;
use crate::overlay::Overlay;
use crate::platform;

/// half width / half height of the basket, in sprite cells
const HW: f64 = 13.0;
const HH: f64 = 6.0;
/// how far the composite (basket and pet) can reach from its centre, in cells
const R: i32 = 27;
const OUT: Color = 0x2b2340;
const RIM: Color = 0xe0a868;
const RIM_D: Color = 0xc08648;
const WEAVE_A: Color = 0xd09454;
const WEAVE_B: Color = 0xb47a40;
const STAKE: Color = 0x8e5a2c;
const INSIDE: Color = 0x6e4524;

#[derive(Clone, Copy, PartialEq, Debug)]
enum St {
    Rest,
    Held,
    Flying,
    /// rocking into place after landing
    Settle { from: f64, to: f64, t0: f64 },
}

pub struct Basket {
    win: Overlay,
    /// centre (physical screen px), velocity (px/s), angle (rad, clockwise) and spin (rad/s)
    pub x: f64,
    pub y: f64,
    vx: f64,
    vy: f64,
    pub ang: f64,
    av: f64,
    st: St,
    /// where it's held, in its own unrotated px from the centre
    grab: (f64, f64),
    piv: (f64, f64),
    piv_v: (f64, f64),
    press_at: (f64, f64),
    moved: bool,
    bounces: u32,
    last: f64,
    pub pet_in: bool,
    /// the pet is walking over to hop in
    pub walking: bool,
    /// test: a scripted hold (no real mouse): started at
    script: Option<f64>,
    /// let go without dragging: a pat for the pet inside
    clicked: bool,
}

fn rot(a: f64, (x, y): (f64, f64)) -> (f64, f64) {
    let (s, c) = a.sin_cos();
    (x * c - y * s, x * s + y * c)
}

/// Angle in (-pi, pi].
fn norm(a: f64) -> f64 {
    let mut a = a % std::f64::consts::TAU;
    if a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    } else if a <= -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

/// How far its lowest point is below its centre at angle `a`.
fn reach_down(a: f64, p: f64) -> f64 {
    (HW * p * a.sin()).abs() + (HH * p * a.cos()).abs()
}

pub fn pet_inside(app: &App) -> bool {
    app.basket.as_ref().map_or(false, |b| b.pet_in)
}

/// Is it out on the taskbar (owned, put out in the shop)?
pub fn wanted(app: &App) -> bool {
    app.pet.owns("basket") && app.pet.st.get("basket_out").and_then(Value::as_bool).unwrap_or(true) && app.coins_on()
}

pub fn owns(app: &App, id: WindowId) -> bool {
    app.basket.as_ref().map_or(false, |b| b.win.id() == id)
}

/// The size setting changed: same spot, new size.
pub fn refit(app: &mut App) {
    let p = app.geo.p as f64;
    if let Some(b) = app.basket.as_mut() {
        if b.st == St::Rest {
            let floor = platform::work_area_at(b.x, b.y).0.bottom;
            b.y = floor - reach_down(b.ang, p);
        }
    }
}

fn save(app: &mut App) {
    if let Some(b) = app.basket.as_ref() {
        let v = json!({"x": b.x.round(), "y": b.y.round(), "ang": b.ang, "in": b.pet_in});
        app.pet.set("basket", v);
    }
}

/// Put it out or away (from the shop).
pub fn set_out(app: &mut App, out: bool) {
    app.pet.set("basket_out", json!(out));
    if !out && pet_inside(app) {
        hop_out(app);
    }
    app.save();
}

/// Every frame: put it out / away, move it, and catch the pet when it drops in.
pub fn tick(app: &mut App, el: &ActiveEventLoop) {
    let want = wanted(app) && app.visible_pub() && !app.gone();
    if !want {
        if app.basket.is_some() {
            if pet_inside(app) {
                hop_out(app);
            }
            save(app);
            app.basket = None;
        }
        return;
    }
    let now = app.now();
    if app.basket.is_none() {
        let p = app.geo.p as f64;
        let saved = app.pet.st.get("basket").cloned().unwrap_or(Value::Null);
        let f = |k: &str| saved.get(k).and_then(Value::as_f64);
        let cx = app.body.x + app.geo.w as f64 / 2.0;
        let x = f("x").unwrap_or(cx + 150.0 * app.geo.s);
        let ang = f("ang").map(norm).unwrap_or(0.0);
        let (work, _) = platform::work_area_at(x, f("y").unwrap_or(app.work.bottom - 10.0));
        let x = x.clamp(work.left + HW * p, work.right - HW * p);
        let y = work.bottom - reach_down(ang, p);
        let side = ((2 * R + 1) as f64 * p) as i32;
        let Some(win) = Overlay::new(el, side, side, x as i32, y as i32, true) else { return };
        let pet_in = saved.get("in").and_then(Value::as_bool).unwrap_or(false) && ang.abs() < 0.3;
        app.basket = Some(Basket {
            win,
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            ang,
            av: 0.0,
            st: St::Rest,
            grab: (0.0, 0.0),
            piv: (0.0, 0.0),
            piv_v: (0.0, 0.0),
            press_at: (0.0, 0.0),
            moved: false,
            bounces: 0,
            last: now,
            pet_in,
            walking: false,
            script: None,
            clicked: false,
        });
        crate::log(&format!("basket out at {x:.0},{y:.0} (pet in: {pet_in})"));
    }
    let dt = {
        let b = app.basket.as_mut().unwrap();
        let dt = (now - b.last).clamp(0.0, 0.05);
        b.last = now;
        dt
    };
    step(app, now, dt);
    if std::mem::take(&mut app.basket.as_mut().unwrap().clicked) && pet_inside(app) {
        app.pet.pat(blip_core::now());
        app.flush_events(el);
    }
    catch(app, now);
    if pet_inside(app) && app.body.action == Action::Walk {
        app.body.set(Action::Idle, 3.0, now); // no strolling in a basket
    }
    if let Some(b) = app.basket.as_ref() {
        if b.pet_in {
            // the pet sits where it's drawn: menus, cards and bubbles find it there
            let (fx, fy) = feet(app);
            app.body.x = fx - app.geo.w as f64 / 2.0;
            app.body.y = fy - app.geo.h as f64;
        }
    }
    draw(app, now);
}

/// Where the pet's feet are inside the basket (screen px), standing upright.
fn feet(app: &App) -> (f64, f64) {
    let b = app.basket.as_ref().unwrap();
    let p = app.geo.p as f64;
    let (dx, dy) = rot(b.ang, (0.0, feet_row(app) as f64 * p + p / 2.0));
    (b.x + dx, b.y + dy)
}

/// The row (in basket cells, 0 = centre) its feet sit on: its eyes and the top of it show over
/// the rim, the rest is tucked in.
fn feet_row(app: &App) -> i32 {
    let h = pet_height(app);
    -(HH as i32) + (h as f64 * 0.36).round() as i32
}

/// The pet's body height in cells (without ears).
fn pet_height(app: &App) -> i32 {
    match app.pet.stage() {
        "baby" | "egg" => 9,
        "child" => 12,
        _ => 15,
    }
}

// ------------------------------------------------ moving
fn step(app: &mut App, now: f64, dt: f64) {
    let p = app.geo.p as f64;
    let s = app.geo.s;
    let g = 1700.0 * s;
    let Some(b) = app.basket.as_mut() else { return };
    let mut tumble = false;
    let mut settled = false;
    match b.st {
        St::Rest => {}
        St::Held => {
            let (mx, my) = scripted(b, now).unwrap_or_else(platform::cursor);
            if !b.moved && (mx - b.press_at.0).abs() + (my - b.press_at.1).abs() > 4.0 {
                b.moved = true;
            }
            let n = 4;
            let h = dt / n as f64;
            if h > 0.0 {
                let raw = ((mx - b.piv.0) / dt, (my - b.piv.1) / dt);
                let prev = b.piv_v;
                b.piv_v = (prev.0 * 0.5 + raw.0 * 0.5, prev.1 * 0.5 + raw.1 * 0.5);
                let lim = 30000.0 * s;
                let acc = (((b.piv_v.0 - prev.0) / dt).clamp(-lim, lim), ((b.piv_v.1 - prev.1) / dt).clamp(-lim, lim));
                let k2 = ((HW * p).powi(2) + (HH * p).powi(2)) / 3.0;
                for _ in 0..n {
                    // it hangs from the grabbed point: a pendulum whose pivot is the mouse
                    let r = rot(b.ang, b.grab);
                    let (rx, ry) = (-r.0, -r.1); // pivot -> centre
                    let l = rx.hypot(ry);
                    if l > 1.0 {
                        let phi = rx.atan2(ry);
                        let phi_dd = -l * ((g - acc.1) * phi.sin() + acc.0 * phi.cos()) / (k2 + l * l);
                        b.av -= phi_dd * h;
                    }
                    b.av *= (-1.8 * h).exp();
                    b.ang += b.av * h;
                }
            }
            b.piv = (mx, my);
            let r = rot(b.ang, b.grab);
            b.x = mx - r.0;
            b.y = my - r.1;
            if b.pet_in && norm(b.ang).abs() > 1.75 {
                tumble = true;
            }
            let done = if b.script.is_some() { scripted(b, now).is_none() } else { !platform::left_button_down() };
            if done {
                b.script = None;
                if !b.moved {
                    // a click, not a drag: pat the pet in it
                    b.st = St::Rest;
                    b.clicked = true;
                } else {
                    // let go: it flies off with the swing it had
                    let (dx, dy) = rot(b.ang + std::f64::consts::FRAC_PI_2, b.grab);
                    let v = (b.piv_v.0 - b.av * dx, b.piv_v.1 - b.av * dy);
                    let vmax = 5000.0 * s;
                    let k = (vmax / v.0.hypot(v.1).max(1.0)).min(1.0);
                    (b.vx, b.vy) = (v.0 * k, v.1 * k);
                    b.av = b.av.clamp(-25.0, 25.0);
                    b.bounces = 0;
                    b.st = St::Flying;
                }
            }
        }
        St::Flying => {
            let n = 4;
            let h = dt / n as f64;
            for _ in 0..n {
                b.vy += g * h;
                b.x += b.vx * h;
                b.y += b.vy * h;
                b.ang += b.av * h;
                b.av *= (-0.4 * h).exp();
                let (work, _) = platform::work_area_at(b.x, b.y.min(b.y - 1.0));
                let ext = (HW * p * b.ang.cos()).abs() + (HH * p * b.ang.sin()).abs();
                // walls, unless there's another screen on that side
                if b.x - ext < work.left && same_screen(work.left - 4.0, b.y, &work) {
                    b.x = work.left + ext;
                    b.vx = b.vx.abs() * 0.45;
                    b.av *= -0.6;
                } else if b.x + ext > work.right && same_screen(work.right + 4.0, b.y, &work) {
                    b.x = work.right - ext;
                    b.vx = -b.vx.abs() * 0.45;
                    b.av *= -0.6;
                }
                let down = reach_down(b.ang, p);
                if b.y + down >= work.bottom {
                    b.y = work.bottom - down;
                    if b.vy > 420.0 * s && b.bounces < 3 {
                        b.vy = -b.vy * 0.28;
                        b.vx *= 0.7;
                        // hitting the ground on a corner tips it a little more the way it leans
                        b.av = b.av * 0.5 + norm(b.ang).sin() * 3.0;
                        b.bounces += 1;
                    } else {
                        // which way does it come to rest?
                        let a = norm(b.ang + b.av * 0.08);
                        let crit = (HW / HH).atan();
                        let rest = if a.abs() < crit {
                            0.0
                        } else if a.abs() > std::f64::consts::PI - crit {
                            std::f64::consts::PI
                        } else {
                            a.signum() * std::f64::consts::FRAC_PI_2
                        };
                        let to = b.ang + norm(rest - b.ang);
                        b.st = St::Settle { from: b.ang, to, t0: now };
                        b.vx = 0.0;
                        b.vy = 0.0;
                        b.av = 0.0;
                        if rest != 0.0 && b.pet_in {
                            tumble = true;
                        }
                        crate::log(&format!("basket lands {}", if rest == 0.0 { "upright" } else if rest.abs() > 3.0 { "upside down" } else { "on its side" }));
                        break;
                    }
                }
                if b.pet_in && norm(b.ang).abs() > 1.75 {
                    tumble = true;
                }
            }
        }
        St::Settle { from, to, t0 } => {
            let k = ((now - t0) / 0.32).min(1.0);
            // ease out with a small rock past and back
            let e = 1.0 - (1.0 - k).powi(3) + (k * std::f64::consts::PI).sin() * 0.06 * (1.0 - k);
            b.ang = from + (to - from) * e;
            let floor = platform::work_area_at(b.x, b.y).0.bottom;
            b.y = floor - reach_down(b.ang, p);
            if k >= 1.0 {
                b.ang = norm(to);
                b.y = floor - reach_down(b.ang, p);
                b.st = St::Rest;
                settled = true;
            }
        }
    }
    if settled {
        save(app);
    }
    if tumble {
        tumble_out(app, now);
    }
}

fn same_screen(x: f64, y: f64, work: &blip_pet::body::Rect) -> bool {
    let (w, _) = platform::work_area_at(x, y);
    (w.left - work.left).abs() < 1.0 && (w.right - work.right).abs() < 1.0
}

/// Test hook: a scripted hold. Grabbed by its left end, swung right, up and around, then let
/// go with a flick (the mouse path a person might make).
fn scripted(b: &Basket, now: f64) -> Option<(f64, f64)> {
    let t0 = b.script?;
    let t = now - t0;
    if t > 1.6 {
        return None;
    }
    let (x0, y0) = b.press_at;
    let k = t / 1.6;
    Some((x0 + 380.0 * k + 60.0 * (t * 9.0).sin(), y0 - 420.0 * k.powf(0.6)))
}

// ------------------------------------------------ the pet going in and out
/// Catch the pet when it drops into the open basket (thrown, dropped or hopping in).
fn catch(app: &mut App, now: f64) {
    let p = app.geo.p as f64;
    let Some(b) = app.basket.as_ref() else { return };
    if b.pet_in || b.st != St::Rest || norm(b.ang).abs() > 0.2 || app.body.action == Action::Held || !app.pet.hatched() {
        return;
    }
    let pcx = app.body.x + app.geo.w as f64 / 2.0;
    let bottom = app.body.y + app.geo.h as f64;
    let rim = b.y - HH * p;
    let falling = (app.body.airborne || app.body.ball) && app.body.vy > 0.0;
    let over = (pcx - b.x).abs() < (HW - 3.0) * p;
    let walked_in = b.walking && app.body.vy > 0.0 && app.body.airborne && over;
    if over && falling && bottom > rim - 4.0 * p && bottom < b.y + HH * p || walked_in {
        put_in(app, now);
    }
}

fn put_in(app: &mut App, now: f64) {
    if let Some(b) = app.basket.as_mut() {
        b.pet_in = true;
        b.walking = false;
    }
    app.body.vx = 0.0;
    app.body.vy = 0.0;
    app.body.airborne = false;
    app.body.ball = false;
    app.body.goal = None;
    app.body.set(Action::Idle, 2.0, now);
    app.spawn_pub("dust", 2);
    app.play_sound("pop", false);
    crate::log("pet is in the basket");
    save(app);
    app.windows_dirty = true;
}

/// Out it comes, the way the basket's opening points, with the basket's own speed.
fn tumble_out(app: &mut App, now: f64) {
    if !pet_inside(app) {
        return;
    }
    let (fx, fy) = feet(app);
    let Some(b) = app.basket.as_mut() else { return };
    b.pet_in = false;
    let (ox, oy) = rot(b.ang, (0.0, -1.0));
    let (bvx, bvy) = (b.vx, b.vy);
    let s = app.geo.s;
    let ph = pet_height(app) as f64 * app.geo.p as f64;
    let (cx, cy) = (fx + ox * ph * 0.6, fy + oy * ph * 0.6);
    app.body.x = cx - app.geo.w as f64 / 2.0;
    app.body.y = (cy - app.geo.h as f64 + ph * 0.5).min(Body::ground(&app.geo, &app.work));
    let (vx, vy) = (bvx + ox * 320.0 * s, bvy + oy * 320.0 * s - 120.0 * s);
    app.body.fling(vx * STEP, vy * STEP, &app.geo, now);
    crate::log("pet tumbles out of the basket");
    save(app);
    app.windows_dirty = true;
}

/// Hop out next to it (asked to, or something needs the pet: an alarm, a game).
pub fn hop_out(app: &mut App) {
    let now = app.now();
    let Some(b) = app.basket.as_mut() else { return };
    if !b.pet_in {
        return;
    }
    b.pet_in = false;
    b.walking = false;
    let p = app.geo.p as f64;
    let side = if b.x - app.work.left > app.work.right - b.x { -1.0 } else { 1.0 };
    let x = b.x + side * (HW * p + 10.0 * p) - app.geo.w as f64 / 2.0;
    let (lo, hi) = Body::bounds(&app.geo, &app.work);
    app.body.x = x.clamp(lo, hi);
    app.body.y = b.y - HH * p - app.geo.h as f64 * 0.6;
    app.body.vx = side * 2.0 * app.geo.s;
    app.body.vy = -5.0 * app.geo.s;
    app.body.airborne = true;
    app.body.set(Action::Idle, 1.5, now);
    crate::log("pet hops out of the basket");
    save(app);
    app.windows_dirty = true;
}

/// "Into the basket": walk over and hop in.
pub fn go_in(app: &mut App) {
    let now = app.now();
    let Some(b) = app.basket.as_mut() else { return };
    if b.pet_in || norm(b.ang).abs() > 0.2 {
        return;
    }
    b.walking = true;
    let goal = b.x - app.geo.w as f64 / 2.0;
    let (lo, hi) = Body::bounds(&app.geo, &app.work);
    app.body.goal = Some(goal.clamp(lo, hi));
    app.body.set(Action::Walk, 30.0, now);
}

/// Arrived next to it: one hop, and it drops in (caught by `catch`).
pub fn arrived(app: &mut App) -> bool {
    let walking = app.basket.as_ref().map_or(false, |b| b.walking);
    if walking && std::mem::take(&mut app.body.arrived) {
        app.body.jump();
        return true;
    }
    if walking && app.body.action != Action::Walk && !app.body.airborne {
        if let Some(b) = app.basket.as_mut() {
            b.walking = false; // interrupted
        }
    }
    false
}

// ------------------------------------------------ mouse
pub fn event(app: &mut App, el: &ActiveEventLoop, event: &WindowEvent) {
    let WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } = event else { return };
    let (mx, my) = platform::cursor();
    match button {
        MouseButton::Right => app.open_menu(el, mx, my),
        MouseButton::Left => {
            app.close_menu();
            let now = app.now();
            if let Some(b) = app.basket.as_mut() {
                grab(b, mx, my, now);
            }
        }
        _ => {}
    }
}

fn grab(b: &mut Basket, mx: f64, my: f64, now: f64) {
    b.grab = rot(-b.ang, (mx - b.x, my - b.y));
    b.piv = (mx, my);
    b.piv_v = (0.0, 0.0);
    b.press_at = (mx, my);
    b.moved = false;
    if b.st != St::Held {
        b.av = 0.0;
    }
    b.st = St::Held;
    b.last = now;
}

/// Test hook ("basket:swing"): grab it by its left end and swing it with a scripted mouse path.
pub fn test_swing(app: &mut App) {
    let p = app.geo.p as f64;
    let now = app.now();
    if let Some(b) = app.basket.as_mut() {
        let (gx, gy) = (b.x - HW * p * 0.85, b.y - HH * p * 0.5);
        grab(b, gx, gy, now);
        b.script = Some(now);
        b.moved = true;
    }
}

// ------------------------------------------------ drawing
fn basket_cells(g: &mut Sprite, pet_in: bool) {
    let (hw, hh) = (HW as i32, HH as i32);
    // the back of the rim, seen over the front: the inside
    if !pet_in {
        for x in -(hw - 2)..(hw - 1) {
            g.insert((x, -hh - 1), INSIDE);
        }
    }
    // a rolled rim on top, a little wider than the woven body
    for y in -hh..(-hh + 3) {
        for x in -hw..hw {
            let edge = x == -hw || x == hw - 1 || y == -hh || y == -hh + 2;
            let c = if edge { OUT } else if (x + 40) % 3 == 0 { RIM_D } else { RIM };
            g.insert((x, y), c);
        }
    }
    // the woven body: horizontal bands over upright stakes, narrowing and rounded at the bottom
    for y in (-hh + 3)..hh {
        let k = (y + hh - 3) as f64 / (2 * hh - 4) as f64;
        let mut half = hw - 1 - (k * 2.0).round() as i32;
        if y == hh - 1 {
            half -= 1;
        }
        for x in -half..half {
            let edge = x == -half || x == half - 1 || y == hh - 1;
            let band = (y + 40) / 2;
            let c = if edge {
                OUT
            } else if (x + 40) % 4 == 0 {
                STAKE
            } else if ((x + 40) / 2 + band) % 2 == 0 {
                WEAVE_A
            } else {
                WEAVE_B
            };
            g.insert((x, y), c);
        }
    }
}

fn draw(app: &mut App, now: f64) {
    let p = app.geo.p;
    let mut comp: Sprite = Sprite::new();
    let pet_in = pet_inside(app);
    if pet_in {
        // the pet, sitting low inside so a bit over half of it shows
        let mood = app.face_pub(now);
        let look = app.look();
        let egg = (app.pet.stage() == "egg").then(|| (app.pet.f("age") / blip_core::pet::EGG_TIME).min(1.0));
        let g = app.body.sprite_as(&look, &app.geo, mood, egg);
        let fr = feet_row(app);
        let back: Vec<_> = (-(HW as i32 - 2)..(HW as i32 - 1)).map(|x| (x, -(HH as i32) - 1)).collect();
        for c in back {
            comp.insert(c, INSIDE);
        }
        for ((x, y), c) in g {
            comp.insert((x - 16, y - (GH - 1) + fr), c);
        }
    }
    basket_cells(&mut comp, pet_in);
    let Some(b) = app.basket.as_mut() else { return };
    // turn it: each output cell takes the colour from where it was before turning
    let mut out = Sprite::new();
    let (sn, cs) = (-b.ang).sin_cos();
    let upright = norm(b.ang).abs() < 1e-3;
    if upright {
        out = comp;
    } else {
        for oy in -R..=R {
            for ox in -R..=R {
                let (fx, fy) = (ox as f64, oy as f64);
                let (sx, sy) = (fx * cs - fy * sn, fx * sn + fy * cs);
                if let Some(c) = comp.get(&(sx.round() as i32, sy.round() as i32)) {
                    out.insert((ox, oy), *c);
                }
            }
        }
    }
    let side = (2 * R + 1) * p;
    let mut cv = Canvas::new(side, side);
    cv.sprite(&out, R * p, R * p, p);
    let (x, y) = ((b.x - (R * p) as f64 - p as f64 / 2.0) as i32, (b.y - (R * p) as f64 - p as f64 / 2.0) as i32);
    b.win.present(&cv, x, y);
    crate::shot::canvas_every("basket", &cv, 0.1);
    if std::env::var_os("BLIP_BASKET_SEQ").is_some() && b.st != St::Rest {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < 600 {
            crate::shot::canvas(&format!("basketseq-{n:04}"), &cv);
        }
    }
    let _ = sprites::GW;
}

/// Is the basket upright and still (so the pet's bubbles can show over it)?
pub fn calm(app: &App) -> bool {
    app.basket.as_ref().map_or(false, |b| b.st == St::Rest && norm(b.ang).abs() < 0.05)
}
