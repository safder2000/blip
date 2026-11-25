//! Settings -> Keep to an area: a stretch of the taskbar the pet wanders in. People can still
//! throw it anywhere; once it lands it walks back (across screens if it has to).
//!
//! The area is set with a strip along the bottom of every screen: drag on it to draw a new
//! stretch, drag the stretch's ends to resize it or its middle to move it, then Done.
//! Saved as `area: [x0, x1, floor_y]` (the pet's centre x range, physical px) and `area_on`.

use blip_pet::body::{Action, Body, Rect};
use blip_ui::Canvas;
use serde_json::{json, Value};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::app::App;
use crate::overlay::Overlay;
use crate::platform;

struct Strip {
    win: Overlay,
    work: Rect,
    s: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Left,
    Right,
    Move(f64),
    New(f64),
}

/// The open area editor.
pub struct Gizmo {
    strips: Vec<Strip>,
    drag: Option<Drag>,
    done: Option<(i32, i32, i32, i32)>,
}

pub fn on(app: &App) -> bool {
    app.cfg.bool("area_on")
}

/// The area: (x0, x1, floor y) of the pet's centre, when it's on.
pub fn get(app: &App) -> Option<(f64, f64, f64)> {
    if !on(app) {
        return None;
    }
    let a = app.cfg.v.get("area")?.as_array()?;
    let f = |i: usize| a.get(i).and_then(Value::as_f64);
    Some((f(0)?, f(1)?, f(2)?))
}

fn set(app: &mut App, x0: f64, x1: f64, y: f64) {
    let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    app.cfg.v.insert("area".into(), json!([a.round(), b.round(), y.round()]));
}

fn same(a: &Rect, b: &Rect) -> bool {
    (a.left - b.left).abs() < 1.0 && (a.top - b.top).abs() < 1.0 && (a.right - b.right).abs() < 1.0 && (a.bottom - b.bottom).abs() < 1.0
}

/// Turn it on or off. On for the first time: a stretch around where the pet is, and the editor.
pub fn switch(app: &mut App, el: &ActiveEventLoop, on: bool) {
    app.cfg.set("area_on", json!(on));
    if on {
        if app.cfg.v.get("area").and_then(Value::as_array).is_none() {
            let cx = app.body.x + app.geo.w as f64 / 2.0;
            let half = 260.0 * app.geo.s;
            let (l, r) = (app.work.left + app.geo.w as f64 / 2.0, app.work.right - app.geo.w as f64 / 2.0);
            let (a, b) = ((cx - half).max(l), (cx + half).min(r));
            set(app, a, b, app.work.bottom);
            app.cfg.save();
        }
        if app.gizmo.is_none() {
            open(app, el);
        }
    } else {
        app.gizmo = None;
        app.body.area = None;
    }
}

pub fn open(app: &mut App, el: &ActiveEventLoop) {
    let mut strips = vec![];
    for m in el.available_monitors() {
        let (pos, size) = (m.position(), m.size());
        let (cx, cy) = (pos.x as f64 + size.width as f64 / 2.0, pos.y as f64 + size.height as f64 / 2.0);
        let (work, scale) = platform::work_area_at(cx, cy);
        if strips.iter().any(|s: &Strip| same(&s.work, &work)) {
            continue;
        }
        let s = scale.max(1.0);
        let h = (40.0 * s) as i32;
        if let Some(win) = Overlay::new(el, (work.right - work.left) as i32, h, work.left as i32, work.bottom as i32 - h, true) {
            strips.push(Strip { win, work, s });
        }
    }
    app.gizmo = Some(Gizmo { strips, drag: None, done: None });
    crate::log("area editor open");
}

pub fn close(app: &mut App) {
    if app.gizmo.take().is_some() {
        app.cfg.save();
        app.windows_dirty = true;
        crate::log(&format!("area set: {:?}", get(app)));
    }
}

pub fn owns(app: &App, id: WindowId) -> bool {
    app.gizmo.as_ref().map_or(false, |g| g.strips.iter().any(|s| s.win.id() == id))
}

pub fn event(app: &mut App, id: WindowId, event: &WindowEvent) {
    let WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } = event else { return };
    if *button == MouseButton::Right {
        close(app);
        return;
    }
    if *button != MouseButton::Left {
        return;
    }
    let (mx, my) = platform::cursor();
    let area = get(app);
    let Some(g) = app.gizmo.as_mut() else { return };
    let Some(strip) = g.strips.iter().find(|s| s.win.id() == id) else { return };
    let (work, s) = (strip.work, strip.s);
    if let Some((x0, y0, x1, y1)) = g.done {
        if (mx as i32) >= x0 && (mx as i32) < x1 && (my as i32) >= y0 && (my as i32) < y1 {
            close(app);
            return;
        }
    }
    let grip = 12.0 * s;
    let drag = match area {
        Some((a, b, fy)) if same(&platform::work_area_at((a + b) / 2.0, fy - 2.0).0, &work) => {
            if (mx - a).abs() <= grip && (mx - a).abs() <= (mx - b).abs() {
                Drag::Left
            } else if (mx - b).abs() <= grip {
                Drag::Right
            } else if mx > a && mx < b {
                Drag::Move(mx - a)
            } else {
                Drag::New(mx)
            }
        }
        _ => Drag::New(mx),
    };
    if let Drag::New(x) = drag {
        set(app, x, x, work.bottom);
    }
    if let Some(g) = app.gizmo.as_mut() {
        g.drag = Some(drag);
    }
}

/// Every frame: keep the pet to its area, and run the editor if it's open.
pub fn tick(app: &mut App) {
    keep(app);
    let Some(g) = app.gizmo.as_ref() else { return };
    // dragging
    if let Some(d) = g.drag {
        let (mx, _) = platform::cursor();
        if let Some((a, b, fy)) = get(app) {
            let work = platform::work_area_at((a + b) / 2.0, fy - 2.0).0;
            let half = app.geo.w as f64 / 2.0;
            let (lo, hi) = (work.left + half * 0.6, work.right - half * 0.6);
            let mx = mx.clamp(lo, hi);
            let (a, b) = match d {
                Drag::Left => (mx.min(b), b),
                Drag::Right => (a, mx.max(a)),
                Drag::Move(off) => {
                    let w = b - a;
                    let a2 = (mx - off).clamp(lo, (hi - w).max(lo));
                    (a2, a2 + w)
                }
                Drag::New(from) => (from.min(mx), from.max(mx)),
            };
            set(app, a, b, fy);
        }
        if !platform::left_button_down() {
            if let Some(g) = app.gizmo.as_mut() {
                g.drag = None;
            }
            app.cfg.save();
        }
    }
    draw(app);
}

/// Premultiplied fill with alpha.
fn fill(cv: &mut Canvas, x0: i32, y0: i32, x1: i32, y1: i32, c: u32, a: u32) {
    let (x0, x1) = (x0.max(0), x1.min(cv.w));
    let (y0, y1) = (y0.max(0), y1.min(cv.h));
    let m = |v: u32| v * a / 255;
    let px = (a << 24) | (m((c >> 16) & 255) << 16) | (m((c >> 8) & 255) << 8) | m(c & 255);
    for y in y0..y1 {
        for x in x0..x1 {
            cv.px[(y * cv.w + x) as usize] = px;
        }
    }
}

fn draw(app: &mut App) {
    let area = get(app);
    let accent = app.theme.accent;
    let Some(g) = app.gizmo.as_mut() else { return };
    g.done = None;
    for (si, st) in g.strips.iter_mut().enumerate() {
        let (w, h) = (st.win.w.max((st.work.right - st.work.left) as i32), (40.0 * st.s) as i32);
        let mut cv = Canvas::new(w, h);
        let u = (2.0 * st.s).round().max(1.0) as i32;
        // the ground: a faint strip (it also catches the clicks)
        fill(&mut cv, 0, 0, w, h, 0x2b2340, 0x30);
        fill(&mut cv, 0, h - u, w, h, 0xffffff, 0x60);
        let here = area.filter(|(a, b, fy)| same(&platform::work_area_at((a + b) / 2.0, fy - 2.0).0, &st.work));
        if let Some((a, b, _)) = here {
            let (l, r) = ((a - st.work.left) as i32, (b - st.work.left) as i32);
            fill(&mut cv, l, 0, r.max(l + u), h, accent, 0x70);
            fill(&mut cv, l, 0, r.max(l + u), u, accent, 0xd0);
            // the two ends: pills to grab
            for x in [l, r] {
                let (gw, gh) = (5 * u, h - 6 * u);
                let (gx, gy) = (x - gw / 2, 3 * u);
                fill(&mut cv, gx - u, gy - u, gx + gw + u, gy + gh + u, accent, 0xff);
                fill(&mut cv, gx, gy, gx + gw, gy + gh, 0xffffff, 0xff);
                for k in 0..3 {
                    let yy = gy + gh / 2 - 3 * u + k * 3 * u;
                    fill(&mut cv, gx + u + u / 2, yy, gx + gw - u - u / 2, yy + u, accent, 0xff);
                }
            }
            // Done, at the right end of this screen's strip
            let lp = (u / 2).max(1) * 2;
            let label_w = 4 * 6 * lp;
            let (bw, bh) = (label_w + 8 * u, h - 8 * u);
            let (bx, by) = (w - bw - 8 * u, 4 * u);
            fill(&mut cv, bx, by, bx + bw, by + bh, accent, 0xff);
            cv.pixel_text(bx + 4 * u, by + (bh - 7 * lp) / 2, "Done", lp, 0xffffff);
            g.done = Some((st.work.left as i32 + bx, st.work.bottom as i32 - h + by, st.work.left as i32 + bx + bw, st.work.bottom as i32 - h + by + bh));
        }
        let (x, y) = (st.work.left as i32, st.work.bottom as i32 - h);
        st.win.present(&cv, x, y);
        crate::shot::canvas_every(&format!("area-{si}"), &cv, 0.5);
    }
}

/// Keep the pet to its area: tell its body where it may wander, and when the area is on another
/// screen, let it walk off this one's edge and on to that one.
fn keep(app: &mut App) {
    app.body.area = None;
    let Some((x0, x1, fy)) = get(app) else { return };
    if crate::basket::pet_inside(app) {
        return;
    }
    let (aw, ascale) = platform::work_area_at((x0 + x1) / 2.0, fy - 2.0);
    let half = app.geo.w as f64 / 2.0;
    let (lo, hi) = Body::bounds(&app.geo, &app.work);
    if same(&aw, &app.work) {
        let a = (x0 - half).clamp(lo, hi);
        let b = (x1 - half).clamp(lo, hi).max(a);
        app.body.area = Some((a, b));
        return;
    }
    // its area is on another screen: head that way, and cross over at the edge
    let right = (aw.left + aw.right) / 2.0 >= (app.work.left + app.work.right) / 2.0;
    app.body.area = Some(if right { (hi + 1e6, hi + 1e6) } else { (lo - 1e6, lo - 1e6) });
    let at_edge = if right { app.body.x >= hi - 0.5 } else { app.body.x <= lo + 0.5 };
    if at_edge && app.body.action == Action::Walk && !app.body.airborne {
        let g = app.geo_for(ascale);
        app.work = aw;
        if (g.s - app.geo.s).abs() > 0.01 {
            app.refit(g);
        }
        let (lo2, hi2) = Body::bounds(&app.geo, &aw);
        app.body.x = if right { lo2 } else { hi2 };
        app.body.y = Body::ground(&app.geo, &aw);
        crate::log("walked over to its area's screen");
    }
}
