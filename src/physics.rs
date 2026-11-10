//! The mascot's body: momentum, gravity, collisions with the screen edges, climbing on
//! them, and the springs that make it lean, wobble and squash. Units are screen pixels and
//! seconds. The simulation runs in fixed steps, so it behaves the same at any frame rate.
//!
//! The body is either airborne or attached to one of four surfaces: the floor (bottom edge),
//! the left and right edges (climbing), or the top edge (hanging by its paws). `pos` is
//! where its feet are. Going around a corner, or letting go of a wall, changes which way the
//! feet point; there the centre of the body is what moves continuously, so nothing jumps.

use std::f32::consts::PI;
use std::ops::{Add, AddAssign, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

impl V2 {
    pub const fn new(x: f32, y: f32) -> Self {
        V2 { x, y }
    }
    pub fn len(self) -> f32 {
        self.x.hypot(self.y)
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2::new(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        *self = *self + o;
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        V2::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, k: f32) -> V2 {
        V2::new(self.x * k, self.y * k)
    }
}

/// Fixed simulation step.
pub const STEP: f32 = 1.0 / 120.0;
const MAX_SPEED: f32 = 5000.0;
/// Landings slower than this stop instead of bouncing.
const REST_SPEED: f32 = 260.0;
/// Anticipation: how long it crouches before a jump leaves the surface.
const CROUCH_SECS: f32 = 0.1;
/// How long it takes to get around a corner.
const TURN_SECS: f32 = 0.45;
/// A corner can be taken from this close to the end of a surface.
const CORNER_REACH: f32 = 4.0;

/// The desktop area it lives in: screen edges in pixels.
pub struct Bounds {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// Where it can be, relative to the edges. The lengths come from how the sprite sits in
/// its window (see main.rs), so the window never leaves the screen while it climbs.
pub struct Params {
    pub gravity: f32,
    pub bounce: f32,
    /// Half the body's width.
    pub half_w: f32,
    /// From the feet to the edge they stand on.
    pub foot_gap: f32,
    /// How close to the top and bottom edges it can climb on a side edge.
    pub wall_top: f32,
    pub wall_bottom: f32,
    /// How far below the top edge its feet are while hanging; also the highest the feet go in the air.
    pub hang_drop: f32,
    /// From the feet to the middle of the body.
    pub center: f32,
}

/// The edge it is on. Each makes its feet point a different way.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Surface {
    Floor,
    LeftWall,
    RightWall,
    /// Hanging from the top edge by its paws.
    Top,
}

impl Surface {
    /// Quarter turns of the sprite: feet down, left, (up is unused), right.
    pub fn quarter_turns(self) -> i32 {
        match self {
            Surface::Floor | Surface::Top => 0,
            Surface::LeftWall => 1,
            Surface::RightWall => 3,
        }
    }

    /// The left or right edge.
    pub fn is_side(self) -> bool {
        matches!(self, Surface::LeftWall | Surface::RightWall)
    }
}

/// Rotates a vector by quarter turns the way the sprite is turned (each turn: down -> left).
pub fn turn(v: V2, quarters: i32) -> V2 {
    (0..quarters.rem_euclid(4)).fold(v, |v, _| V2::new(-v.y, v.x))
}

/// A world vector in the body's own frame (as if it stood on the floor).
pub fn to_local(surface: Surface, v: V2) -> V2 {
    turn(v, -surface.quarter_turns())
}

/// From the feet to the middle of the body, on a surface.
pub fn center_offset(surface: Surface, center: f32) -> V2 {
    turn(V2::new(0.0, -center), surface.quarter_turns())
}

/// The range it can move along a surface: x on the floor and top edge, y on the side edges.
pub fn span(p: &Params, b: &Bounds, s: Surface) -> (f32, f32) {
    if s.is_side() {
        (b.top + p.wall_top, b.bottom - p.wall_bottom)
    } else {
        (b.left + p.half_w, b.right - p.half_w)
    }
}

/// Position along a surface.
pub fn along(s: Surface, pos: V2) -> f32 {
    if s.is_side() { pos.y } else { pos.x }
}

/// Feet position at `a` along a surface.
fn anchor(p: &Params, b: &Bounds, s: Surface, a: f32) -> V2 {
    match s {
        Surface::Floor => V2::new(a, b.bottom - p.foot_gap),
        Surface::Top => V2::new(a, b.top + p.hang_drop),
        Surface::LeftWall => V2::new(b.left + p.foot_gap, a),
        Surface::RightWall => V2::new(b.right - p.foot_gap, a),
    }
}

/// The corner shared by two surfaces: its point, and where each surface ends there.
fn corner(p: &Params, b: &Bounds, from: Surface, to: Surface) -> Option<(V2, f32, f32)> {
    let end = |s: Surface, at_start: bool| if at_start { span(p, b, s).0 } else { span(p, b, s).1 };
    let (floorish, wall) = if from.is_side() { (to, from) } else { (from, to) };
    if !wall.is_side() || floorish.is_side() {
        return None;
    }
    let left = wall == Surface::LeftWall;
    let floor = floorish == Surface::Floor;
    let point = V2::new(if left { b.left } else { b.right }, if floor { b.bottom } else { b.top });
    let (floorish_end, wall_end) = (end(floorish, left), end(wall, !floor));
    Some(if from.is_side() { (point, wall_end, floorish_end) } else { (point, floorish_end, wall_end) })
}

/// Going around a corner: the body's middle swings around the corner point.
#[derive(Clone, Copy, Debug)]
struct Turn {
    from: Surface,
    to: Surface,
    point: V2,
    start: V2,
    end: V2,
    /// Where the feet end up.
    landing: V2,
    u: f32,
}

/// What the legs try to do while attached: reach `along` speed (x on the floor and top
/// edge, y on side edges), changing it by at most `accel` px/s².
pub struct Drive {
    pub along: f32,
    pub accel: f32,
}

pub struct Body {
    /// Feet position on screen.
    pub pos: V2,
    pub vel: V2,
    /// What it stands, climbs or hangs on; `None` while airborne.
    pub surface: Option<Surface>,
    turn: Option<Turn>,
    /// Lean in radians (positive = clockwise in its own frame), driven by a spring toward a target.
    pub tilt: f32,
    tilt_vel: f32,
    /// Tumbling freely after a hard throw; settles back upright on landing.
    pub spinning: bool,
    /// Positive = squashed flat, negative = stretched tall.
    pub squash: f32,
    squash_vel: f32,
    /// Distance travelled along surfaces; drives the leg cycle so feet never slide.
    pub stride: f32,
    /// A jump waiting for its crouch to finish: launch velocity and seconds left.
    pending_jump: Option<(V2, f32)>,
}

/// Damped spring: pulls `x` toward `target`, `k` = stiffness, `c` = damping.
fn spring(x: &mut f32, v: &mut f32, target: f32, k: f32, c: f32, dt: f32) {
    *v += ((target - *x) * k - *v * c) * dt;
    *x += *v * dt;
}

fn wrap_angle(a: f32) -> f32 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}

impl Body {
    pub fn new(pos: V2) -> Self {
        Body {
            pos,
            vel: V2::default(),
            surface: Some(Surface::Floor),
            turn: None,
            tilt: 0.0,
            tilt_vel: 0.0,
            spinning: false,
            squash: 0.0,
            squash_vel: 0.0,
            stride: 0.0,
            pending_jump: None,
        }
    }

    /// Standing on the floor (not mid-corner).
    pub fn grounded(&self) -> bool {
        self.surface == Some(Surface::Floor) && self.turn.is_none()
    }

    pub fn turning(&self) -> bool {
        self.turn.is_some()
    }

    /// Which way its feet point, i.e. how the sprite is drawn. Airborne it is upright.
    pub fn mount(&self) -> Surface {
        match self.turn {
            Some(t) => if t.u < 0.5 { t.from } else { t.to },
            None => self.surface.unwrap_or(Surface::Floor),
        }
    }

    /// Its velocity as it feels it: in its own frame, as if it stood on the floor.
    pub fn local_vel(&self) -> V2 {
        to_local(self.mount(), self.vel)
    }

    /// Is anything still visibly moving? Used to pick the frame rate.
    pub fn is_moving(&self) -> bool {
        self.surface != Some(Surface::Floor)
            || self.turn.is_some()
            || self.pending_jump.is_some()
            || self.vel.len() > 1.0
            || self.tilt_vel.abs() > 0.05
            || self.squash_vel.abs() > 0.05
    }

    /// Crouches briefly, then leaps off whatever it is on with `vel`.
    pub fn jump(&mut self, vel: V2) {
        if self.surface.is_some() && self.turn.is_none() && self.pending_jump.is_none() {
            self.pending_jump = Some((vel, CROUCH_SECS));
        }
    }

    /// Loses or releases its grip and falls with `vel`.
    pub fn let_go(&mut self, vel: V2, p: &Params) {
        if self.surface.is_some() {
            self.detach(vel, p);
        }
    }

    /// Becomes airborne. Coming off a side edge it turns upright; its middle stays put.
    fn detach(&mut self, vel: V2, p: &Params) {
        let middle = self.pos + center_offset(self.mount(), p.center);
        self.surface = None;
        self.turn = None;
        self.pending_jump = None;
        self.pos = middle - center_offset(Surface::Floor, p.center);
        self.vel = vel;
    }

    /// Let go after dragging: keep the hand's velocity, and tumble if it was a hard throw.
    pub fn throw(&mut self, vel: V2) {
        self.pending_jump = None;
        self.surface = None;
        self.turn = None;
        self.vel = vel;
        if vel.len() > 1400.0 {
            self.spinning = true;
            self.tilt_vel = vel.x * 0.006;
        }
    }

    /// Starts going around the corner onto `to`, if it is at the end of its surface next to it.
    pub fn transfer(&mut self, to: Surface, p: &Params, b: &Bounds) -> bool {
        let (Some(from), None) = (self.surface, self.turn) else { return false };
        let Some((point, from_end, to_end)) = corner(p, b, from, to) else { return false };
        if (along(from, self.pos) - from_end).abs() > CORNER_REACH {
            return false;
        }
        let landing = anchor(p, b, to, to_end);
        self.turn = Some(Turn {
            from,
            to,
            point,
            start: self.pos + center_offset(from, p.center),
            end: landing + center_offset(to, p.center),
            landing,
            u: 0.0,
        });
        self.vel = V2::default();
        self.pending_jump = None;
        true
    }

    /// Advances one STEP. `held` is where the grabbing hand wants the feet to be.
    /// Returns the speed of a floor impact during this step, or 0.
    pub fn step(&mut self, p: &Params, b: &Bounds, drive: &Drive, held: Option<V2>) -> f32 {
        let dt = STEP;
        let old_local = self.local_vel();
        let mut impact = 0.0;

        if let Some(target) = held {
            if self.surface.is_some() {
                self.detach(self.vel, p);
            }
            // the hand pulls with a stiff, slightly underdamped spring: responsive but elastic
            const K: f32 = 900.0;
            let acc = (target - self.pos) * K - self.vel * (2.0 * K.sqrt() * 0.55);
            self.vel += acc * dt;
            self.spinning = false;
            self.pos += self.vel * dt;
            self.keep_in_air_bounds(p, b, false);
        } else if let Some(mut t) = self.turn {
            // the middle of the body swings around the corner point; feet follow
            t.u = (t.u + dt / TURN_SECS).min(1.0);
            let e = t.u * t.u * (3.0 - 2.0 * t.u);
            let (a0, a1) = ((t.start - t.point).y.atan2((t.start - t.point).x), (t.end - t.point).y.atan2((t.end - t.point).x));
            let a = a0 + wrap_angle(a1 - a0) * e;
            let r = (t.start - t.point).len() + ((t.end - t.point).len() - (t.start - t.point).len()) * e;
            let middle = t.point + V2::new(a.cos(), a.sin()) * r;
            self.turn = Some(t);
            self.pos = middle - center_offset(self.mount(), p.center);
            if t.u >= 1.0 {
                self.turn = None;
                self.surface = Some(t.to);
                self.pos = t.landing;
            }
        } else if let Some(s) = self.surface {
            // moving along the surface it clings to
            let dir = if s.is_side() { V2::new(0.0, 1.0) } else { V2::new(1.0, 0.0) };
            let mut v = along(s, self.vel);
            v += (drive.along - v).clamp(-drive.accel * dt, drive.accel * dt);
            let (lo, hi) = span(p, b, s);
            let a = along(s, self.pos) + v * dt;
            if a <= lo || a >= hi {
                v = 0.0;
            }
            self.pos = anchor(p, b, s, a.clamp(lo, hi));
            self.vel = dir * v;
            self.stride += v.abs() * dt;
            if let Some((launch, left)) = self.pending_jump {
                if left <= dt {
                    self.detach(launch, p);
                    self.squash_vel -= 3.0; // stretch on take-off
                } else {
                    self.pending_jump = Some((launch, left - dt));
                }
            }
        } else {
            self.vel.y += p.gravity * dt;
            self.vel.x -= self.vel.x * 0.35 * dt; // air drag
            let speed = self.vel.len();
            if speed > MAX_SPEED {
                self.vel = self.vel * (MAX_SPEED / speed);
            }
            self.pos += self.vel * dt;
            impact = self.keep_in_air_bounds(p, b, true);
        }

        let local = self.local_vel();
        if self.spinning {
            self.tilt += self.tilt_vel * dt;
            self.tilt_vel *= 1.0 - 0.6 * dt;
        } else {
            let accel = (local.x - old_local.x) / dt;
            let lean = if held.is_some() || self.mount() == Surface::Top && self.turn.is_none() {
                // hangs and swings behind the motion
                (-local.x * 0.0007).clamp(-0.7, 0.7)
            } else if self.surface.is_some() {
                // leans into its stride, and rocks back when it brakes
                (local.x / 2400.0 + accel / 12000.0).clamp(-0.3, 0.3)
            } else {
                (self.vel.x / 3000.0).clamp(-0.4, 0.4)
            };
            spring(&mut self.tilt, &mut self.tilt_vel, lean, 170.0, 12.0, dt);
        }
        let shape = if self.pending_jump.is_some() || self.turn.is_some() {
            0.2 // crouched: ready to leap, or gripping its way around a corner
        } else if self.surface.is_some() {
            0.0
        } else {
            -(self.vel.y.abs() / 2600.0).min(0.2) // stretched along a fall or a leap
        };
        spring(&mut self.squash, &mut self.squash_vel, shape, 260.0, 9.0, dt);
        self.squash = self.squash.clamp(-0.35, 0.45);
        impact
    }

    /// Keeps a free (airborne or held) body inside the desktop, bouncing off the edges.
    /// Returns the speed of a floor impact, or 0.
    fn keep_in_air_bounds(&mut self, p: &Params, b: &Bounds, free: bool) -> f32 {
        let mut impact = 0.0;
        let floor = b.bottom - p.foot_gap;
        if self.pos.y >= floor {
            self.pos.y = floor;
            if self.vel.y > 0.0 {
                impact = self.vel.y;
                if !free {
                    self.vel.y = 0.0;
                } else {
                    self.squash_vel += self.vel.y * 0.004;
                    self.vel.x *= 0.8; // floor friction on impact
                    if self.vel.y > REST_SPEED {
                        self.vel.y = -self.vel.y * p.bounce;
                        self.tilt_vel *= 0.6;
                    } else {
                        self.vel.y = 0.0;
                        self.surface = Some(Surface::Floor);
                        if self.spinning {
                            self.spinning = false;
                            self.tilt = wrap_angle(self.tilt); // spring back upright the short way
                        }
                    }
                }
            }
        }
        if self.pos.y < b.top + p.hang_drop {
            self.pos.y = b.top + p.hang_drop;
            if self.vel.y < 0.0 {
                self.vel.y = -self.vel.y * p.bounce;
            }
        }
        for (edge, side) in [(b.left + p.half_w, 1.0), (b.right - p.half_w, -1.0)] {
            if (self.pos.x - edge) * side < 0.0 {
                self.pos.x = edge;
                if self.vel.x * side < 0.0 {
                    if free {
                        self.squash_vel -= self.vel.x.abs() * 0.002; // squeezed sideways = taller
                        self.tilt_vel -= self.vel.x * 0.002;
                    }
                    self.vel.x = -self.vel.x * p.bounce;
                }
            }
        }
        impact
    }
}

/// How the mascot is handling the mouse pointer it plays with.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Grip {
    /// Flying or rolling freely.
    Loose,
    /// Reeled in toward the hand, smoothly.
    Pull(V2),
    /// Held in the hand.
    Hold(V2),
}

/// The mouse pointer while the mascot plays with it: a light ball with gravity, bounces
/// off the screen edges and rolling friction on the floor.
pub struct Toy {
    pub pos: V2,
    pub vel: V2,
}

impl Toy {
    const GRAVITY: f32 = 1800.0;
    const BOUNCE: f32 = 0.55;

    pub fn new(pos: V2) -> Self {
        Toy { pos, vel: V2::default() }
    }

    /// Advances one STEP inside `screen`.
    pub fn step(&mut self, grip: Grip, screen: &Bounds) {
        let dt = STEP;
        let pull = |target: V2, k: f32, toy: &Toy| (target - toy.pos) * k - toy.vel * (2.0 * k.sqrt() * 0.9);
        match grip {
            Grip::Pull(hand) => self.vel += pull(hand, 30.0, self) * dt,
            Grip::Hold(hand) => self.vel += pull(hand, 500.0, self) * dt,
            Grip::Loose => {
                self.vel.y += Self::GRAVITY * dt;
                self.vel.x -= self.vel.x * 0.4 * dt;
            }
        }
        self.pos += self.vel * dt;
        if self.pos.y >= screen.bottom {
            self.pos.y = screen.bottom;
            self.vel.y = if self.vel.y > 150.0 { -self.vel.y * Self::BOUNCE } else { self.vel.y.min(0.0) };
            self.vel.x *= (-3.0 * dt).exp(); // rolling friction
        }
        if self.pos.y <= screen.top {
            self.pos.y = screen.top;
            self.vel.y = self.vel.y.abs() * Self::BOUNCE;
        }
        if self.pos.x <= screen.left || self.pos.x >= screen.right {
            self.pos.x = self.pos.x.clamp(screen.left, screen.right);
            self.vel.x = -self.vel.x * Self::BOUNCE;
        }
    }

    #[cfg(test)]
    pub fn at_rest(&self, screen: &Bounds) -> bool {
        self.pos.y >= screen.bottom - 1.0 && self.vel.len() < 20.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the shapes the real app uses at 4 screen pixels per art pixel
    const P: Params = Params {
        gravity: 2200.0,
        bounce: 0.42,
        half_w: 32.0,
        foot_gap: 4.0,
        wall_top: 48.0,
        wall_bottom: 52.0,
        hang_drop: 60.0,
        center: 28.0,
    };
    const B: Bounds = Bounds { left: 0.0, right: 1000.0, top: 0.0, bottom: 800.0 };
    const STAND: Drive = Drive { along: 0.0, accel: 1400.0 };

    fn middle(b: &Body) -> V2 {
        b.pos + center_offset(b.mount(), P.center)
    }

    /// Steps the body, checking it stays on the desktop and its middle never jumps.
    fn run(body: &mut Body, secs: f32, drive: &Drive) -> f32 {
        let mut max_impact: f32 = 0.0;
        let mut last = middle(body);
        for _ in 0..(secs / STEP) as usize {
            max_impact = max_impact.max(body.step(&P, &B, drive, None));
            let m = middle(body);
            assert!((m - last).len() < 45.0, "the body's middle jumped from {last:?} to {m:?}");
            last = m;
            assert!((B.left..=B.right).contains(&body.pos.x) && (B.top..=B.bottom).contains(&body.pos.y), "{:?}", body.pos);
        }
        max_impact
    }

    #[test]
    fn dropped_body_bounces_then_settles_upright() {
        let mut b = Body::new(V2::new(500.0, 200.0));
        b.surface = None;
        let impact = run(&mut b, 4.0, &STAND);
        assert!(impact > 1500.0, "fell fast: {impact}");
        assert!(b.grounded() && b.pos.y == B.bottom - P.foot_gap);
        assert!(b.tilt.abs() < 0.01 && b.squash.abs() < 0.01, "settled: {} {}", b.tilt, b.squash);
    }

    #[test]
    fn hard_throw_hits_wall_tumbles_and_recovers() {
        let mut b = Body::new(V2::new(500.0, 600.0));
        b.throw(V2::new(3000.0, -1200.0));
        assert!(b.spinning);
        run(&mut b, 0.4, &STAND);
        assert!(b.vel.x < 0.0, "bounced off the right edge");
        run(&mut b, 6.0, &STAND);
        assert!(b.grounded() && !b.spinning && b.vel.len() < 1.0 && b.tilt.abs() < 0.01);
    }

    #[test]
    fn walking_accelerates_smoothly_and_leans_forward() {
        let mut b = Body::new(V2::new(200.0, B.bottom - P.foot_gap));
        let walk = Drive { along: 300.0, accel: 900.0 };
        b.step(&P, &B, &walk, None);
        assert!(b.vel.x > 0.0 && b.vel.x < 10.0, "no instant jump to full speed");
        run(&mut b, 1.0, &walk);
        assert!((b.vel.x - 300.0).abs() < 0.01 && b.tilt > 0.05 && b.stride > 100.0);
    }

    #[test]
    fn jumps_crouch_first_then_leave_the_ground() {
        let mut b = Body::new(V2::new(500.0, B.bottom - P.foot_gap));
        b.jump(V2::new(0.0, -800.0));
        run(&mut b, 0.07, &STAND);
        assert!(b.grounded() && b.squash > 0.03, "crouching: {}", b.squash);
        run(&mut b, 0.05, &STAND);
        assert!(b.surface.is_none() && b.vel.y < 0.0, "launched");
    }

    #[test]
    fn climbs_all_the_way_round_the_screen_without_jumping() {
        let mut b = Body::new(V2::new(500.0, B.bottom - P.foot_gap));
        let go = |along| Drive { along, accel: 1500.0 };
        // walk to the left corner, round onto the left edge, up to the top, along the top
        // to the right edge, down it and back onto the floor
        let route = [(-200.0, Surface::LeftWall), (-150.0, Surface::Top), (150.0, Surface::RightWall), (150.0, Surface::Floor)];
        for (speed, next) in route {
            let before = b.surface;
            run(&mut b, 12.0, &go(speed));
            assert!(b.transfer(next, &P, &B), "from {before:?} onto {next:?} at {:?}", b.pos);
            assert!(!b.transfer(next, &P, &B), "one corner at a time");
            run(&mut b, 1.0, &STAND);
            assert_eq!(b.surface, Some(next));
        }
        assert!(b.grounded());
    }

    #[test]
    fn corners_only_where_surfaces_meet() {
        let mut b = Body::new(V2::new(500.0, B.bottom - P.foot_gap));
        assert!(!b.transfer(Surface::LeftWall, &P, &B), "not at the corner yet");
        assert!(!b.transfer(Surface::Top, &P, &B), "floor and top edge do not meet");
    }

    #[test]
    fn jumping_off_a_wall_turns_upright_flies_and_lands() {
        let mut b = Body::new(V2::new(P.half_w, B.bottom - P.foot_gap));
        assert!(b.transfer(Surface::LeftWall, &P, &B));
        run(&mut b, 1.0, &STAND);
        run(&mut b, 3.0, &Drive { along: -100.0, accel: 1500.0 });
        b.jump(V2::new(350.0, -400.0));
        run(&mut b, 0.2, &STAND);
        assert!(b.surface.is_none() && b.mount() == Surface::Floor);
        let impact = run(&mut b, 4.0, &STAND);
        assert!(impact > 300.0 && b.grounded() && b.pos.x > 100.0);
    }

    #[test]
    fn slipping_off_the_top_edge_falls_to_the_floor() {
        let mut b = Body::new(V2::new(500.0, B.bottom - P.foot_gap));
        b.surface = Some(Surface::Top);
        b.pos = V2::new(500.0, B.top + P.hang_drop);
        b.let_go(V2::default(), &P);
        let impact = run(&mut b, 3.0, &STAND);
        assert!(impact > 1500.0 && b.grounded());
    }

    #[test]
    fn thrown_pointer_flies_bounces_and_comes_to_rest_on_screen() {
        let mut toy = Toy::new(V2::new(500.0, 700.0));
        toy.vel = V2::new(1500.0, -1200.0);
        let mut hit_wall = false;
        for _ in 0..(6.0 / STEP) as usize {
            toy.step(Grip::Loose, &B);
            hit_wall |= toy.vel.x < 0.0;
            assert!((B.left..=B.right).contains(&toy.pos.x) && (B.top..=B.bottom).contains(&toy.pos.y));
        }
        assert!(hit_wall && toy.at_rest(&B));
    }

    #[test]
    fn pulled_pointer_glides_into_the_hand() {
        let mut toy = Toy::new(V2::new(100.0, 100.0));
        let hand = V2::new(600.0, 750.0);
        let mut last = (toy.pos - hand).len();
        for _ in 0..(2.0 / STEP) as usize {
            toy.step(Grip::Pull(hand), &B);
            let d = (toy.pos - hand).len();
            assert!(d <= last + 0.5, "no overshoot or jerk");
            last = d;
        }
        assert!(last < 5.0);
    }

    #[test]
    fn held_body_follows_the_hand() {
        let mut b = Body::new(V2::new(200.0, B.bottom - P.foot_gap));
        for _ in 0..240 {
            b.step(&P, &B, &STAND, Some(V2::new(400.0, 300.0)));
        }
        assert!((b.pos - V2::new(400.0, 300.0)).len() < 1.0);
    }
}
