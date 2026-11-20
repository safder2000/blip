//! Movement: the same situations the Python physics tests check (tests/test_windows.py PhysicsTests).

use blip_pet::body::{Action, Body, Geo, Gesture, Look, Rect, STEP};
use blip_pet::sprites::Stage;

const WORK: Rect = Rect { left: 0.0, top: 0.0, right: 1920.0, bottom: 1040.0 };

fn run_until_landed(b: &mut Body, geo: &Geo, now: &mut f64) -> (bool, u32) {
    let (mut chute, mut bounces) = (false, 0);
    for _ in 0..5000 {
        b.step(geo, &WORK, *now);
        *now += STEP;
        chute |= b.chute > 0;
        bounces = bounces.max(b.bounces);
        if !b.airborne && !b.ball {
            return (chute, bounces);
        }
    }
    panic!("never landed");
}

fn thrown(geo: &Geo, y: f64, mouse: &[(f64, f64, f64)], seed: u64) -> (Body, f64) {
    let mut b = Body::new(800.0, y, seed);
    let mut now = 10.0;
    b.press(mouse[0].1, mouse[0].2, now);
    for &(t, x, yy) in mouse {
        now = 10.0 + t;
        b.drag(x, yy, now);
    }
    assert_eq!(b.action, Action::Held);
    let _ = b.release(geo, now);
    (b, now)
}

#[test]
fn a_gentle_drop_bounces_and_settles() {
    let geo = Geo::new(1.0);
    let g = Body::ground(&geo, &WORK);
    // picked up and let go, barely moving
    let (mut b, mut now) = thrown(&geo, g - 100.0, &[(0.0, 900.0, 500.0), (0.05, 908.0, 494.0), (0.09, 909.0, 494.0)], 7);
    assert!(!b.ball);
    let (_, _bounces) = run_until_landed(&mut b, &geo, &mut now);
    assert_eq!(b.y, g);
    assert_eq!(b.action, Action::Idle);
}

#[test]
fn a_fast_throw_becomes_a_ball_that_ricochets_rolls_and_unfolds() {
    let geo = Geo::new(1.25);
    let (mut b, mut now) =
        thrown(&geo, 300.0, &[(0.0, 600.0, 500.0), (0.03, 700.0, 450.0), (0.06, 820.0, 390.0)], 3);
    assert!(b.ball, "fast throw curls up");
    let (lo, hi) = Body::bounds(&geo, &WORK);
    let mut hit_edge = false;
    for _ in 0..8000 {
        b.step(&geo, &WORK, now);
        now += STEP;
        hit_edge |= b.x <= lo || b.x >= hi;
        if !b.ball {
            break;
        }
    }
    assert!(!b.ball);
    assert!(hit_edge, "ricocheted off a screen edge");
    assert!(b.bounces >= 2);
    assert_eq!(b.action, Action::Unfold);
    b.anim(now + 2.0);
    assert_eq!(b.action, Action::Idle);
}

#[test]
fn landings_vary_and_never_repeat_twice_in_a_row() {
    let geo = Geo::new(1.0);
    let mut seen = std::collections::HashSet::new();
    for seed in 1..40u64 {
        let (mut b, mut now) =
            thrown(&geo, 300.0, &[(0.0, 600.0, 500.0), (0.03, 700.0, 450.0), (0.06, 820.0, 390.0)], seed);
        for _ in 0..8000 {
            b.step(&geo, &WORK, now);
            now += STEP;
            if b.action == blip_pet::body::Action::Unfold {
                break;
            }
        }
        assert_eq!(b.action, Action::Unfold);
        seen.insert(format!("{:?}", b.landing));
        // a second landing on the same body is never the same reaction
        let first = b.landing;
        let (mut c, mut t2) = (b, now);
        c.anim(t2 + 3.0);
        t2 += 3.0;
        c.y -= 200.0;
        c.fling(30.0, -5.0, &geo, t2);
        for _ in 0..8000 {
            c.step(&geo, &WORK, t2);
            t2 += STEP;
            if c.action == Action::Unfold {
                break;
            }
        }
        if c.action == Action::Unfold {
            assert_ne!(first, c.landing);
        }
    }
    assert!(seen.len() >= 5, "only saw {seen:?}");
}

#[test]
fn a_tap_is_a_pat_and_a_long_press_is_the_menu() {
    let geo = Geo::new(1.0);
    let mut b = Body::new(500.0, Body::ground(&geo, &WORK), 1);
    b.press(600.0, 900.0, 1.0);
    assert_eq!(b.release(&geo, 1.1), Some(Gesture::Pat));
    b.press(600.0, 900.0, 2.0);
    let mut now = 2.0;
    let mut menu = false;
    while now < 3.0 {
        b.step(&geo, &WORK, now);
        menu |= b.take_menu();
        now += STEP;
    }
    assert!(menu, "holding opens the menu");
    assert_eq!(b.release(&geo, now), None, "letting go afterwards does nothing");
}

#[test]
fn walking_stays_on_screen() {
    let geo = Geo::new(1.0);
    let mut b = Body::new(10.0, Body::ground(&geo, &WORK), 99);
    let (lo, hi) = Body::bounds(&geo, &WORK);
    let mut now = 0.0;
    let mut walked = false;
    for i in 0..20_000 {
        b.step(&geo, &WORK, now);
        if i % 8 == 0 {
            b.anim(now);
        }
        walked |= b.action == Action::Walk;
        assert!(b.x >= lo && b.x <= hi);
        assert_eq!(b.y, Body::ground(&geo, &WORK));
        now += STEP;
    }
    assert!(walked);
}

#[test]
fn sprites_for_every_state_draw() {
    let geo = Geo::new(1.5);
    let look = Look { species: "bunny".into(), palette: "peach".into(), stage: Stage::Child, wear: vec!["crown".into(), "shades".into()], pick: 0 };
    let mut b = Body::new(500.0, 200.0, 5);
    for chute in [0, 7] {
        b.chute = chute;
        b.action = Action::Fall;
        b.airborne = true;
        assert!(!b.sprite(&look, &geo, 0.0).is_empty());
    }
    b.ball = true;
    b.vx = 30.0;
    assert!(b.sprite(&look, &geo, 0.0).len() > 100, "ball plus flame trail");
}
