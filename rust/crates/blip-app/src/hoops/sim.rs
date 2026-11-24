//! Hoops' physics: the pet, curled into a ball, flung from a slingshot at a basketball hoop.
//! Screen pixels, y pointing down, seconds.

#[derive(Clone, Copy, Debug)]
pub struct Hoop {
    /// left end of the rim (the open side, facing the slingshot)
    pub x: f64,
    /// rim height
    pub y: f64,
    /// width of the opening
    pub w: f64,
    /// backboard height
    pub board_h: f64,
    /// the backboard is on the left (the hoop hangs on the screen's left edge)
    pub board_left: bool,
}

impl Hoop {
    pub fn rim_l(&self) -> (f64, f64) {
        (self.x, self.y)
    }
    pub fn rim_r(&self) -> (f64, f64) {
        (self.x + self.w, self.y)
    }
    pub fn board_x(&self) -> f64 {
        if self.board_left {
            self.x - self.w * 0.08
        } else {
            self.x + self.w + self.w * 0.08
        }
    }
    pub fn board_top(&self) -> f64 {
        self.y - self.board_h * 0.8
    }
    pub fn board_bottom(&self) -> f64 {
        self.y + self.board_h * 0.2
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ev {
    /// hit the ground this hard (px/s)
    Bounce(f64),
    Rim,
    Board,
    Score,
    /// came to rest (or took too long): next ball
    Stop,
}

#[derive(Clone, Debug)]
pub struct Sim {
    pub g: f64,
    pub r: f64,
    pub floor: f64,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub spin: f64,
    pub hoop: Hoop,
    pub flying: bool,
    pub scored: bool,
    still: f64,
    airtime: f64,
}

impl Sim {
    pub fn new(r: f64, floor: f64, left: f64, right: f64, top: f64, g: f64, hoop: Hoop) -> Sim {
        Sim { g, r, floor, left, right, top, x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, spin: 0.0, hoop, flying: false, scored: false, still: 0.0, airtime: 0.0 }
    }

    pub fn launch(&mut self, x: f64, y: f64, vx: f64, vy: f64) {
        (self.x, self.y, self.vx, self.vy) = (x, y, vx, vy);
        self.flying = true;
        self.scored = false;
        self.still = 0.0;
        self.airtime = 0.0;
    }

    /// Bounce off a point (a rim end) if the ball overlaps it.
    fn hit_point(&mut self, px: f64, py: f64, rest: f64) -> bool {
        let (dx, dy) = (self.x - px, self.y - py);
        let d = dx.hypot(dy);
        if d >= self.r || d < 1e-6 {
            return false;
        }
        let (nx, ny) = (dx / d, dy / d);
        self.x = px + nx * self.r;
        self.y = py + ny * self.r;
        let vn = self.vx * nx + self.vy * ny;
        if vn < 0.0 {
            self.vx -= (1.0 + rest) * vn * nx;
            self.vy -= (1.0 + rest) * vn * ny;
            return vn < -40.0;
        }
        false
    }

    pub fn step(&mut self, dt: f64) -> Vec<Ev> {
        let mut out = vec![];
        if !self.flying {
            return out;
        }
        let n = (dt / (1.0 / 240.0)).ceil().max(1.0) as usize;
        let h = dt / n as f64;
        for _ in 0..n {
            let prev_y = self.y;
            self.vy += self.g * h;
            self.x += self.vx * h;
            self.y += self.vy * h;
            self.spin += self.vx * h / self.r;
            self.airtime += h;
            // the rim's two ends and the backboard
            let hp = self.hoop;
            let (l, r) = (hp.rim_l(), hp.rim_r());
            if self.hit_point(l.0, l.1, 0.55) || self.hit_point(r.0, r.1, 0.55) {
                out.push(Ev::Rim);
            }
            let by = self.y.clamp(hp.board_top(), hp.board_bottom());
            if self.hit_point(hp.board_x(), by, 0.5) {
                out.push(Ev::Board);
            }
            // through the hoop from above, between the rim ends
            if !self.scored && prev_y < hp.y && self.y >= hp.y && self.vy > 0.0 && self.x > l.0 && self.x < r.0 {
                self.scored = true;
                out.push(Ev::Score);
            }
            // walls and ceiling
            if self.x - self.r < self.left {
                self.x = self.left + self.r;
                self.vx = self.vx.abs() * 0.7;
            } else if self.x + self.r > self.right {
                self.x = self.right - self.r;
                self.vx = -self.vx.abs() * 0.7;
            }
            if self.y - self.r < self.top {
                self.y = self.top + self.r;
                self.vy = self.vy.abs() * 0.6;
            }
            // the ground (the taskbar line)
            if self.y + self.r >= self.floor {
                self.y = self.floor - self.r;
                if self.vy > 260.0 {
                    out.push(Ev::Bounce(self.vy));
                    self.vy = -self.vy * 0.58;
                    self.vx *= 0.86;
                } else {
                    self.vy = 0.0;
                    self.vx *= 1.0 - 3.5 * h; // rolling to a stop
                }
            }
        }
        let resting = self.y + self.r >= self.floor - 0.5 && self.vy == 0.0 && self.vx.abs() < 25.0;
        self.still = if resting { self.still + dt } else { 0.0 };
        if self.still > 0.3 || self.airtime > 7.0 {
            self.flying = false;
            out.push(Ev::Stop);
        }
        out
    }

    /// A launch velocity (from x, y, at most `vmax`) that scores, if there is one: tries angles
    /// and powers and simulates each. Used by the auto player in tests.
    pub fn solve(&self, x: f64, y: f64, vmax: f64) -> Option<(f64, f64)> {
        self.solve_step(x, y, vmax, 1)
    }

    /// `solve` trying every `step`-th angle and power (coarser = faster).
    pub fn solve_step(&self, x: f64, y: f64, vmax: f64, step: usize) -> Option<(f64, f64)> {
        let dir = if self.hoop.x + self.hoop.w / 2.0 < x { -1.0 } else { 1.0 };
        for a in (15..80).step_by(step).map(|d| (d as f64).to_radians()) {
            for k in (20..=100).step_by(step) {
                let v = vmax * k as f64 / 100.0;
                let (vx, vy) = (dir * v * a.cos(), -v * a.sin());
                let mut s = self.clone();
                s.launch(x, y, vx, vy);
                for _ in 0..600 {
                    let ev = s.step(1.0 / 60.0);
                    if ev.contains(&Ev::Score) {
                        return Some((vx, vy));
                    }
                    if ev.contains(&Ev::Stop) || ev.contains(&Ev::Rim) || ev.contains(&Ev::Board) {
                        break;
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn court() -> Sim {
        let hoop = Hoop { x: 1300.0, y: 450.0, w: 95.0, board_h: 80.0, board_left: false };
        Sim::new(30.0, 1040.0, 0.0, 1920.0, 0.0, 2000.0, hoop)
    }

    fn run(s: &mut Sim) -> Vec<Ev> {
        let mut all = vec![];
        for _ in 0..1200 {
            let e = s.step(1.0 / 60.0);
            let stop = e.contains(&Ev::Stop);
            all.extend(e);
            if stop {
                break;
            }
        }
        all
    }

    #[test]
    fn dropped_through_the_middle_scores() {
        let mut s = court();
        s.launch(1300.0 + 47.5, 200.0, 0.0, 0.0);
        let ev = run(&mut s);
        assert!(ev.contains(&Ev::Score), "{ev:?}");
        assert!(ev.contains(&Ev::Stop));
    }

    #[test]
    fn dropped_on_the_rim_bounces_off() {
        let mut s = court();
        s.launch(1300.0 - 5.0, 200.0, 0.0, 0.0);
        let ev = run(&mut s);
        assert!(ev.contains(&Ev::Rim), "{ev:?}");
    }

    #[test]
    fn a_slingshot_shot_can_score() {
        let s = court();
        let (vx, vy) = s.solve(150.0, 950.0, 1960.0).expect("some shot scores");
        let mut s2 = court();
        s2.launch(150.0, 950.0, vx, vy);
        assert!(run(&mut s2).contains(&Ev::Score));
    }

    #[test]
    fn a_hoop_on_the_left_edge_can_be_scored_from_the_right() {
        let hoop = Hoop { x: 40.0, y: 420.0, w: 95.0, board_h: 80.0, board_left: true };
        let s = Sim::new(30.0, 1040.0, 0.0, 1920.0, 0.0, 1700.0, hoop);
        let (vx, vy) = s.solve(800.0, 950.0, 2185.0).expect("some shot scores");
        assert!(vx < 0.0, "shoots to the left");
        let mut s2 = s.clone();
        s2.launch(800.0, 950.0, vx, vy);
        assert!(run(&mut s2).contains(&Ev::Score));
    }

    #[test]
    fn a_missed_ball_comes_to_rest() {
        let mut s = court();
        s.launch(150.0, 950.0, 600.0, -300.0);
        let ev = run(&mut s);
        assert!(ev.contains(&Ev::Stop) && !ev.contains(&Ev::Score));
        assert!((s.y + s.r - s.floor).abs() < 1.0);
    }
}
