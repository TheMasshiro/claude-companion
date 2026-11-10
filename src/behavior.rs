//! The mascot's mind: a couple of needs (energy, wanting attention), two personality
//! traits, and a weighted pick of what to do next. It is a quiet companion: while you
//! seem to be working it keeps to calm things, and saves running, chasing and following
//! for when you are away from the mouse or playing with it.
//!
//! Climbing: now and then, more when it is lively, it sets off on an outing: walks to a side
//! edge of the screen, climbs it, maybe goes round the corner to hang from the top edge and
//! shimmy along it, pauses to look around or swing, and eventually climbs down, jumps off,
//! lets go, or slips. Each step of the outing is picked at random as it goes.
//!
//! Pointer games: once you have been away from keyboard and mouse for a while, it may
//! sneak up on the mouse pointer, reel it in, carry it, throw it and bat it around. The
//! moment you touch the keyboard or mouse again it lets go and gets back to its own business. It never moves the body itself:
//! each frame it returns an `Intent` (how fast to walk, an optional jump, where to look)
//! that the physics and animation layers carry out.

use crate::anim::Act;
use crate::config::Config;
use crate::physics::{Bounds, Params, Surface, V2, along, span};

/// Seconds of resting the pointer on it before that counts as petting.
const PET_SECS: f32 = 1.5;
/// The pointer moving within this many seconds means you are probably working.
const BUSY_SECS: f32 = 8.0;
/// After you click, drag or pet it, it stays in the mood to play this long.
const PLAYTIME_SECS: f32 = 60.0;

/// What the mind can perceive this frame.
pub struct Senses<'a> {
    pub pos: V2,
    pub vel: V2,
    /// Standing on the floor.
    pub grounded: bool,
    /// What it is on (`None` in the air), and whether it is going round a corner.
    pub surface: Option<Surface>,
    pub turning: bool,
    pub bounds: &'a Bounds,
    pub params: &'a Params,
    /// Global pointer position, when known.
    pub cursor: Option<V2>,
    /// Seconds since the pointer last moved.
    pub cursor_still: f32,
    pub hovered: bool,
    /// Seconds since your last keyboard or mouse input, when known.
    pub away: Option<f32>,
    /// Whether the pointer can be moved on this desktop (see cursor.rs).
    pub can_move_pointer: bool,
    /// The pointer while it is being played with, and the paw that plays with it (screen pixels).
    pub toy: Option<V2>,
    pub hand: V2,
    /// Screen pixels per canvas unit; distances scale with the mascot's size.
    pub scale: f32,
}

/// What to do with the mouse pointer this frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pointer {
    /// Hands off: the pointer is yours.
    Leave,
    /// Reel it in toward the paw.
    Pull,
    Hold,
    /// Let it fly or roll on its own.
    Loose,
    /// Send it off with this velocity (a throw or a bat), then let it fly.
    Fling(V2),
}

pub struct Intent {
    /// Speed the legs aim for along the surface it is on (x on the floor and top edge,
    /// y on the side edges), and how quickly they may change it.
    pub along: f32,
    pub accel: f32,
    /// Leap off whatever it is on (after a short crouch).
    pub jump: Option<V2>,
    /// Go round the corner onto this surface.
    pub transfer: Option<Surface>,
    /// Let go of the edge it is climbing or hanging from.
    pub let_go: Option<V2>,
    /// A screen point worth looking at.
    pub look: Option<V2>,
    pub pointer: Pointer,
}

pub struct Mind {
    pub act: Act,
    /// Seconds spent in the current act.
    pub t: f32,
    dur: f32,
    target_x: Option<f32>,
    /// Chosen by right-click: hold it until the next left click.
    pinned: bool,
    woke: bool,
    arrived: bool,
    pounces: u32,
    cooldown: f32,
    throw_speed: f32,
    /// 0 = exhausted, 1 = full of beans. Drains while awake, refills while asleep.
    pub energy: f32,
    /// Grows while nobody plays with it; makes it seek out the pointer.
    social: f32,
    since_touch: f32,
    /// Seconds since you last clicked, dragged or petted it.
    since_play: f32,
    hover_t: f32,
    last: Act,
    curiosity: f32,
    playfulness: f32,
    /// Playing a pointer game right now.
    game: bool,
    /// Seconds of you being away before the next pointer game.
    game_in: f32,
    swats: u32,
    swats_wanted: u32,
    /// Climbing outing: which side edge, where it is heading, a pause, and how many more
    /// steps before it heads home.
    outing: bool,
    wall: Surface,
    goal: Option<(Climb, f32)>,
    pause: f32,
    steps_left: u32,
    /// On a side or top edge (or rounding a corner) right now.
    clinging: bool,
    rng: u64,
}

/// One step of a climbing outing.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Climb {
    /// Head to a random spot on this edge.
    Explore,
    /// Head to the end of this edge, then round the corner onto another.
    Corner(Surface),
}

impl Mind {
    pub fn new(cfg: &Config, seed: u64) -> Self {
        let mut m = Mind {
            act: Act::Wave,
            t: 0.0,
            dur: 2.2,
            target_x: None,
            pinned: false,
            woke: false,
            arrived: false,
            pounces: 0,
            cooldown: 0.0,
            throw_speed: 0.0,
            energy: 0.8,
            social: 0.3,
            since_touch: 0.0,
            since_play: PLAYTIME_SECS,
            hover_t: 0.0,
            last: Act::Idle,
            curiosity: 0.0,
            playfulness: 0.0,
            game: false,
            game_in: 0.0,
            swats: 0,
            swats_wanted: 0,
            outing: false,
            wall: Surface::LeftWall,
            goal: None,
            pause: 0.0,
            steps_left: 0,
            clinging: false,
            rng: seed | 1,
        };
        m.game_in = m.range(0.2, 1.0) * cfg.mischief_every;
        // a slightly different personality every launch
        m.curiosity = (cfg.curiosity + m.range(-0.1, 0.1)).clamp(0.0, 1.0);
        m.playfulness = (cfg.playfulness + m.range(-0.1, 0.1)).clamp(0.0, 1.0);
        m
    }

    fn set(&mut self, act: Act, dur: f32) {
        self.last = self.act;
        self.act = act;
        self.t = 0.0;
        self.dur = dur;
        self.target_x = None;
        self.arrived = false;
        self.pounces = 0;
        self.cooldown = 0.0;
        self.outing = false;
    }

    fn touch(&mut self) {
        self.game = false;
        self.since_touch = 0.0;
        self.since_play = 0.0;
        self.pinned = false;
    }

    pub fn click(&mut self) {
        if matches!(self.act, Act::Held | Act::Fly | Act::Slip) {
            return;
        }
        let asleep = self.act == Act::Sleep;
        self.touch();
        self.social = 0.0;
        if self.clinging {
            self.set(Act::Slip, 3.0); // startled off its perch
        } else if asleep {
            self.woke = true;
            self.set(Act::Surprised, 1.0);
        } else {
            self.set(Act::Happy, 2.4);
        }
    }

    pub fn double_click(&mut self) {
        self.touch();
        self.set(Act::Wave, 2.2);
    }

    pub fn grab(&mut self) {
        self.touch();
        self.set(Act::Held, f32::INFINITY);
    }

    pub fn release(&mut self, speed: f32) {
        self.throw_speed = speed;
        self.set(Act::Fly, f32::INFINITY);
    }

    /// The body hit the floor at `speed` px/s.
    pub fn landed(&mut self, speed: f32) {
        if !matches!(self.act, Act::Fly | Act::Slip) {
            return;
        }
        if speed > 2200.0 || self.throw_speed > 1800.0 {
            self.set(Act::Dizzy, 2.6);
        } else if speed > 900.0 {
            self.set(Act::Surprised, 0.9);
        }
        // gentler landings end the flight in update() once the body is at rest
    }

    pub fn cycle(&mut self) {
        let i = Act::CYCLE.iter().position(|&a| a == self.act).map_or(0, |i| i + 1);
        self.set(Act::CYCLE[i % Act::CYCLE.len()], f32::INFINITY);
        self.pinned = true;
        self.since_touch = 0.0;
    }

    pub fn update(&mut self, dt: f32, s: &Senses, cfg: &Config) -> Intent {
        self.t += dt;
        self.cooldown -= dt;
        self.since_touch += dt;
        self.since_play += dt;
        self.social = (self.social + dt / 240.0).min(1.0);
        let drain = match self.act {
            Act::Sleep => -1.0 / 50.0,
            Act::Run | Act::Chase => 1.0 / 90.0,
            _ => 1.0 / 900.0,
        };
        self.energy = (self.energy - drain * dt).clamp(0.0, 1.0);
        if s.cursor.is_some() && s.cursor_still < 0.5 {
            self.since_touch = 0.0; // someone is at the computer
        }
        self.hover_t = if s.hovered { self.hover_t + dt } else { 0.0 };
        let k = s.scale / 6.0; // 1 at a 180-pixel window

        let calm = matches!(
            self.act,
            Act::Idle | Act::Look | Act::Sit | Act::Walk | Act::Think | Act::Work | Act::Curious | Act::Follow | Act::Stretch
        );

        // pointer games: only while you are away, and only for a while so the screen can still lock
        let away = s.away.unwrap_or(0.0);
        let may_play = cfg.cursor_mischief
            && s.can_move_pointer
            && s.cursor.is_some()
            && away >= cfg.afk_after
            && away < cfg.afk_after + cfg.mischief_for;
        if self.game && !may_play {
            self.game = false;
            self.set(Act::Surprised, 0.8); // you're back: it lets go at once, a little caught out
        } else if may_play && !self.game && !self.pinned && (calm || self.act == Act::Sleep) {
            self.game_in -= dt;
            if self.game_in <= 0.0 {
                self.game = true;
                self.game_in = self.range(0.5, 1.5) * cfg.mischief_every;
                let act = if self.rand() < 0.4 { Act::Sneak } else { Act::Curious };
                self.set(act, 30.0);
            }
        }

        if !self.pinned && calm && !self.game {
            if self.hover_t > PET_SECS {
                self.hover_t = -3.0; // a pause before the next petting counts
                self.touch();
                self.social = 0.0;
                self.set(Act::Love, 3.5);
            } else if self.since_touch > cfg.sleep_after {
                self.set(Act::Sleep, f32::INFINITY);
            }
        }

        self.clinging = s.turning || matches!(s.surface, Some(Surface::LeftWall | Surface::RightWall | Surface::Top));
        let mut it = Intent { along: 0.0, accel: 1400.0, jump: None, transfer: None, let_go: None, look: None, pointer: Pointer::Leave };
        // glance at the pointer while it moves nearby, or while it rests on us
        let near_cursor = s.cursor.filter(|&c| s.hovered || (s.cursor_still < 2.0 && (c - s.pos).len() < 700.0 * k));
        let mut done = self.t >= self.dur;
        let mut next = None;
        match self.act {
            Act::Curious | Act::Sneak if self.game => match s.cursor {
                None => done = true,
                Some(c) => {
                    it.look = Some(c);
                    // stand just beside the pointer, paw toward it
                    let side = if c.x >= s.pos.x { 1.0 } else { -1.0 };
                    let dx = c.x - side * 7.0 * s.scale - s.pos.x;
                    if dx.abs() > 6.0 * k {
                        let top = if self.act == Act::Sneak { 0.5 * cfg.walk_speed } else { cfg.walk_speed };
                        it.along = dx.signum() * top.min(dx.abs() * 2.5 + 10.0);
                        it.accel = 700.0;
                    } else if s.grounded {
                        if s.hand.y - c.y > 20.0 * k {
                            next = Some((Act::Tug, 8.0)); // out of reach: reel it in
                        } else {
                            if self.act == Act::Sneak {
                                it.jump = Some(V2::new(0.0, -(2.0 * cfg.gravity * 14.0 * k).sqrt())); // pounce!
                            }
                            let dur = self.range(1.5, 3.0);
                            next = Some((Act::Carry, dur));
                        }
                    }
                }
            },
            Act::Tug => {
                it.pointer = Pointer::Pull;
                it.look = s.toy.or(s.cursor);
                if s.toy.is_some_and(|t| (t - s.hand).len() < 5.0 * k) || self.t > 6.0 {
                    let dur = self.range(1.5, 3.0);
                    next = Some((Act::Carry, dur));
                }
            }
            Act::Carry => {
                it.pointer = Pointer::Hold;
                if s.grounded && self.t > 0.6 && self.pounces == 0 {
                    self.pounces = 1;
                    it.jump = Some(V2::new(0.0, -(2.0 * cfg.gravity * 10.0 * k).sqrt())); // a proud little hop
                }
                if done {
                    next = Some((Act::Throw, 1.4));
                }
            }
            Act::Throw => {
                it.look = s.toy;
                it.pointer = if self.t < 0.45 {
                    Pointer::Hold // wind-up
                } else if self.pounces == 0 {
                    self.pounces = 1;
                    let dir = if self.rand() < 0.5 { -1.0 } else { 1.0 };
                    Pointer::Fling(V2::new(dir * self.range(700.0, 1400.0), -self.range(700.0, 1300.0)))
                } else {
                    Pointer::Loose
                };
                if done {
                    self.swats = 0;
                    self.swats_wanted = 2 + (self.rand() * 3.0) as u32;
                    next = Some((Act::Bat, 12.0));
                }
            }
            Act::Bat => {
                it.pointer = Pointer::Loose;
                it.look = s.toy;
                if let Some(toy) = s.toy {
                    let dx = toy.x - s.pos.x;
                    let on_ground = toy.y > s.pos.y - 14.0 * s.scale;
                    if self.swats >= self.swats_wanted {
                        if self.cooldown <= -1.5 {
                            self.game = false; // that was fun
                            next = Some((Act::Happy, 2.4));
                        }
                    } else if dx.abs() > 8.0 * k {
                        it.along = dx.signum() * 0.7 * cfg.run_speed;
                        it.accel = 1800.0;
                    } else if on_ground && self.cooldown <= 0.0 {
                        let dir = if dx >= 0.0 { 1.0 } else { -1.0 };
                        it.pointer = Pointer::Fling(V2::new(dir * self.range(500.0, 900.0), -self.range(350.0, 650.0)));
                        self.swats += 1;
                        self.cooldown = 0.9;
                        self.t = 0.0; // replay the swipe
                    }
                }
            }
            Act::Walk | Act::Run => {
                let run = self.act == Act::Run;
                let tx = match self.target_x {
                    Some(x) => x,
                    None => {
                        // short strolls while you work, longer trips otherwise
                        let (near, far) = if run { (0.4, 1.0) } else if self.busy(s) { (0.05, 0.2) } else { (0.1, 0.6) };
                        let x = self.random_x(s, near, far);
                        *self.target_x.insert(x)
                    }
                };
                let dx = tx - s.pos.x;
                if dx.abs() < 6.0 {
                    if self.pinned { self.target_x = None } else { done = true }
                } else {
                    let top = if run { cfg.run_speed } else { cfg.walk_speed };
                    it.along = dx.signum() * top.min(dx.abs() * 2.5 + 20.0); // ease into the stop
                    it.accel = if run { 2600.0 } else { 900.0 };
                }
                it.look = near_cursor;
            }
            Act::Curious => match s.cursor {
                None => done = true,
                Some(c) => {
                    it.look = Some(c);
                    // stand a little to the side of the pointer, facing it
                    let side = if s.pos.x < c.x { -1.0 } else { 1.0 };
                    let dx = c.x + side * 55.0 * k - s.pos.x;
                    if dx.abs() > 150.0 * k {
                        self.arrived = false;
                    }
                    if !self.arrived && dx.abs() > 8.0 {
                        it.along = dx.signum() * cfg.walk_speed.min(dx.abs() * 2.5 + 20.0);
                        it.accel = 900.0;
                    } else {
                        self.arrived = true;
                    }
                }
            },
            Act::Chase => match s.cursor {
                None => done = true,
                Some(c) => {
                    it.look = Some(c);
                    let dx = c.x - s.pos.x;
                    if s.grounded && dx.abs() > 25.0 * k {
                        it.along = dx.signum() * cfg.run_speed;
                        it.accel = 2600.0;
                    } else if s.grounded && self.cooldown <= 0.0 {
                        if self.pounces >= 3 {
                            done = true;
                        } else {
                            let rise = s.pos.y - 7.0 * s.scale - c.y; // pointer height above its middle
                            if rise > 15.0 * k {
                                // pounce: jump just high enough, aimed at the pointer
                                let vy = (2.0 * cfg.gravity * rise.min(320.0 * k)).sqrt();
                                let vx = (dx * cfg.gravity / vy).clamp(-cfg.run_speed, cfg.run_speed);
                                it.jump = Some(V2::new(vx, -vy));
                                self.pounces += 1;
                                self.cooldown = 0.5;
                            } else {
                                next = Some((Act::Happy, 2.4)); // got it!
                            }
                        }
                    }
                }
            },
            Act::Follow => match s.cursor {
                None => done = true,
                Some(c) => {
                    it.look = Some(c);
                    let dx = c.x - s.pos.x;
                    if dx.abs() > 70.0 * k {
                        let far = dx.abs() > 450.0 * k;
                        it.along = dx.signum() * if far { cfg.run_speed } else { cfg.walk_speed };
                        it.accel = 1500.0;
                    }
                }
            },
            Act::Happy => {
                it.look = near_cursor;
                if s.grounded && self.cooldown <= 0.0 {
                    it.jump = Some(V2::new(0.0, -(2.0 * cfg.gravity * 28.0 * k).sqrt()));
                    self.cooldown = 0.3;
                }
            }
            Act::Surprised => {
                it.look = near_cursor;
                if self.t <= dt {
                    it.jump = Some(V2::new(0.0, -(2.0 * cfg.gravity * 16.0 * k).sqrt()));
                }
            }
            Act::Notice => it.look = s.cursor,
            Act::Held => {}
            Act::Fly => done = s.grounded && s.vel.len() < 20.0 && self.t > 0.3,
            Act::Climb => {
                if !self.outing {
                    self.outing = true;
                    // usually the nearer side edge
                    let nearer = if s.pos.x - s.bounds.left < s.bounds.right - s.pos.x { Surface::LeftWall } else { Surface::RightWall };
                    let other = if nearer == Surface::LeftWall { Surface::RightWall } else { Surface::LeftWall };
                    self.wall = if self.rand() < 0.7 { nearer } else { other };
                    self.steps_left = 2 + (self.rand() * 4.0) as u32;
                    self.goal = None;
                    self.pause = 0.0;
                }
                match s.surface {
                    _ if s.turning => {}
                    None => {} // in mid-air: physics has it
                    Some(Surface::Floor) => {
                        if self.steps_left == 0 && !self.pinned {
                            done = true; // back down: outing over
                        } else {
                            let (lo, hi) = span(s.params, s.bounds, Surface::Floor);
                            let dx = if self.wall == Surface::LeftWall { lo } else { hi } - s.pos.x;
                            if dx.abs() > 1.0 {
                                it.along = dx.signum() * cfg.walk_speed.min(dx.abs() * 2.5 + 15.0);
                                it.accel = 900.0;
                            } else {
                                it.transfer = Some(self.wall);
                            }
                            if self.steps_left == 0 {
                                self.steps_left = 3; // pinned: off again
                            }
                        }
                    }
                    Some(edge) => self.climb(edge, dt, s, cfg, &mut it, &mut next),
                }
            }
            Act::Slip => {
                // grip giving way: a nervous wobble, then the fall
                if self.clinging && self.t > 0.45 && !s.turning {
                    it.let_go = Some(V2::default());
                }
                done = s.grounded && s.vel.len() < 20.0 && self.t > 1.0;
            }
            Act::Sleep => done = self.energy >= 1.0 && self.since_touch < cfg.sleep_after,
            _ => it.look = near_cursor,
        }
        if let Some((act, dur)) = next {
            self.set(act, dur);
        } else if done && !self.pinned {
            self.game = false;
            self.choose_next(s, cfg);
        }
        if !self.game {
            it.pointer = Pointer::Leave;
        }
        // only climbing keeps it up on an edge; anything else and it lets go
        let stays_up = matches!(self.act, Act::Climb | Act::Slip | Act::Held);
        if self.clinging && !s.turning && !stays_up && it.let_go.is_none() {
            it.let_go = Some(V2::default());
            self.set(Act::Fly, f32::INFINITY);
        }
        it
    }

    /// One frame of a climbing outing on a side edge or the top edge.
    fn climb(&mut self, edge: Surface, dt: f32, s: &Senses, cfg: &Config, it: &mut Intent, next: &mut Option<(Act, f32)>) {
        let k = s.scale / 6.0;
        let (lo, hi) = span(s.params, s.bounds, edge);
        let here = along(edge, s.pos);
        if self.pause > 0.0 {
            // stopping to look around; hanging from the top it idly swings
            self.pause -= dt;
            if edge == Surface::Top {
                it.along = 40.0 * k * (self.t * 2.2).sin();
                it.accel = 300.0;
            }
            return;
        }
        let Some((step, goal)) = self.goal else {
            return self.decide(edge, lo, hi, here, s, it, next);
        };
        let d = goal - here;
        if d.abs() > 2.0 {
            let speed = if edge == Surface::Top { 0.6 } else { 0.75 } * cfg.walk_speed;
            it.along = d.signum() * speed.min(d.abs() * 2.5 + 10.0);
            it.accel = 700.0;
        } else {
            self.goal = None;
            if let Climb::Corner(onto) = step {
                it.transfer = Some(onto);
                if onto.is_side() {
                    self.wall = onto;
                }
            }
        }
    }

    /// Picks the next step of a climbing outing, at random, flavoured by personality.
    fn decide(&mut self, edge: Surface, lo: f32, hi: f32, here: f32, s: &Senses, it: &mut Intent, next: &mut Option<(Act, f32)>) {
        let play = self.playfulness;
        let heading_home = self.steps_left == 0;
        self.steps_left = self.steps_left.saturating_sub(1);
        // choices: 0 explore, 1 pause, 2 corner, 3 jump off / let go, 4 slip
        let weights = match (edge.is_side(), heading_home) {
            (true, false) => [0.35, 0.3, 0.2, 0.08 * play, 0.04],
            (true, true) => [0.0, 0.0, 0.6, 0.15 + 0.3 * play, 0.08],
            (false, false) => [0.35, 0.35, 0.15, 0.1, 0.03],
            (false, true) => [0.0, 0.0, 0.5, 0.5, 0.05],
        };
        let mut r = self.rand() * weights.iter().sum::<f32>();
        let choice = weights.iter().position(|w| { r -= w; r <= 0.0 }).unwrap_or(0);
        let k = s.scale / 6.0;
        match choice {
            0 => {
                let goal = self.range(lo, hi);
                self.goal = Some((Climb::Explore, goal));
            }
            1 => self.pause = self.range(1.5, 3.5),
            2 => {
                let (onto, goal) = if edge.is_side() {
                    // up onto the top edge, or down to the floor when heading home
                    if heading_home { (Surface::Floor, hi) } else { (Surface::Top, lo) }
                } else {
                    // along the top to one of the side edges
                    let left = if heading_home { here - lo < hi - here } else { self.rand() < 0.5 };
                    if left { (Surface::LeftWall, lo) } else { (Surface::RightWall, hi) }
                };
                if onto == Surface::Floor {
                    self.steps_left = 0;
                }
                self.goal = Some((Climb::Corner(onto), goal));
            }
            3 => {
                if edge.is_side() {
                    // push off into the room
                    let away = if edge == Surface::LeftWall { 1.0 } else { -1.0 };
                    let (vx, vy) = (self.range(250.0, 450.0), self.range(250.0, 500.0));
                    it.jump = Some(V2::new(away * vx * k.max(0.6), -vy));
                } else {
                    it.let_go = Some(V2::new(s.vel.x, 0.0)); // drops down
                }
                *next = Some((Act::Fly, f32::INFINITY));
            }
            _ => *next = Some((Act::Slip, 3.0)),
        }
    }

    fn choose_next(&mut self, s: &Senses, cfg: &Config) {
        match self.act {
            Act::Surprised if self.woke => {
                self.woke = false;
                return self.set(Act::Happy, 2.4);
            }
            Act::Sleep => return self.set(Act::Stretch, 2.6),
            Act::Notice if cfg.cursor_play && s.cursor.is_some() => {
                let act = if self.rand() < self.playfulness { Act::Chase } else { Act::Curious };
                let dur = self.range(6.0, 10.0);
                return self.set(act, dur);
            }
            _ => {}
        }
        let (e, play, cur, social) = (self.energy, self.playfulness, self.curiosity, self.social);
        let cursor = cfg.cursor_play && s.cursor.is_some();
        let moving = s.cursor_still < 1.5;
        let playing = self.since_play < PLAYTIME_SECS;
        // calm while you work: lively acts mostly wait until you step away or play with it
        let lively = if self.busy(s) { 0.2 } else { 1.0 };
        let roam = if cfg.roam { 1.0 } else { 0.0 };
        // (act, weight, shortest, longest duration in seconds)
        let options = [
            (Act::Idle, 1.6, 3.0, 8.0),
            (Act::Look, 0.8, 2.6, 5.2),
            (Act::Walk, roam * 1.2 * (0.4 + e), 20.0, 20.0),
            (Act::Run, roam * 0.5 * play * (0.2 + e) * lively * lively, 15.0, 15.0),
            (Act::Sit, 0.7 + 1.2 * (1.0 - e), 8.0, 18.0),
            (Act::Stretch, 0.3, 2.6, 2.6),
            (Act::Think, 0.5, 4.0, 8.0),
            (Act::Work, 0.6, 6.0, 12.0),
            (Act::Happy, (0.15 + 0.3 * play) * lively, 2.4, 2.4),
            (Act::Wave, 0.1 * lively, 2.2, 2.2),
            // an outing up the side and top edges
            (Act::Climb, if cfg.climbing && cfg.roam { 0.8 * (0.3 + e) * (0.4 + cur) * lively } else { 0.0 }, 120.0, 120.0),
            // you stepped away from a resting pointer: wander over and inspect it
            (Act::Curious, if cursor && !moving { cur * (0.4 + social) * lively } else { 0.0 }, 6.0, 10.0),
            // you are playing with it and moving the pointer: chase or follow it
            (Act::Chase, if cursor && moving && playing { 1.5 * play * e } else { 0.0 }, 6.0, 10.0),
            (Act::Follow, if cursor && moving && playing { 0.8 * cur } else { 0.0 }, 6.0, 12.0),
            (Act::Sleep, if e < 0.25 { 3.0 } else { 0.0 }, f32::INFINITY, f32::INFINITY),
        ];
        // doing the same thing twice in a row is much less likely
        let (now, last) = (self.act, self.last);
        let weight = |a: Act, w: f32| if a == now || a == last { w * 0.25 } else { w };
        let total: f32 = options.iter().map(|o| weight(o.0, o.1)).sum();
        let mut r = self.rand() * total;
        let mut pick = options[0];
        for o in options {
            r -= weight(o.0, o.1);
            if r <= 0.0 {
                pick = o;
                break;
            }
        }
        let (act, lo, hi) = (pick.0, pick.2, pick.3);
        let mut dur = if lo == hi { lo } else { self.range(lo, hi) };
        if matches!(act, Act::Idle | Act::Look | Act::Sit) {
            dur /= cfg.activity;
        }
        if act == Act::Chase {
            // anticipation: it spots the pointer first, then decides to chase or inspect it
            return self.set(Act::Notice, 0.7);
        }
        self.set(act, dur);
    }

    /// You seem to be working: the pointer is moving (or unknown), and you are not playing with it.
    fn busy(&self, s: &Senses) -> bool {
        self.since_play >= PLAYTIME_SECS && s.cursor_still < BUSY_SECS
    }

    /// A spot on the floor between `near` and `far` (fractions of the screen width) away.
    fn random_x(&mut self, s: &Senses, near: f32, far: f32) -> f32 {
        let margin = 12.0 * s.scale;
        let (lo, hi) = (s.bounds.left + margin, (s.bounds.right - margin).max(s.bounds.left + margin));
        let width = hi - lo;
        let mut x = s.pos.x.clamp(lo, hi);
        for _ in 0..12 {
            let d = self.range(near, far) * width * if self.rand() < 0.5 { -1.0 } else { 1.0 };
            if (lo..=hi).contains(&(s.pos.x + d)) {
                x = s.pos.x + d;
                break;
            }
        }
        x
    }

    fn rand(&mut self) -> f32 {
        // xorshift64
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Bounds = Bounds { left: 0.0, right: 1920.0, top: 0.0, bottom: 1000.0 };
    const P: Params = Params { gravity: 2200.0, bounce: 0.42, half_w: 48.0, foot_gap: 6.0, wall_top: 72.0, wall_bottom: 78.0, hang_drop: 90.0, center: 42.0 };
    const DT: f32 = 1.0 / 30.0;

    fn senses(cursor: Option<V2>, still: f32) -> Senses<'static> {
        Senses {
            pos: V2::new(900.0, 1000.0),
            vel: V2::default(),
            grounded: true,
            surface: Some(Surface::Floor),
            turning: false,
            bounds: &B,
            params: &P,
            cursor,
            cursor_still: still,
            hovered: false,
            away: None,
            can_move_pointer: false,
            toy: None,
            hand: V2::default(),
            scale: 6.0,
        }
    }

    fn run(m: &mut Mind, s: &Senses, cfg: &Config, secs: f32) -> Vec<Act> {
        let mut seen = vec![m.act];
        for _ in 0..(secs / DT) as usize {
            m.update(DT, s, cfg);
            if seen.last() != Some(&m.act) {
                seen.push(m.act);
            }
        }
        seen
    }

    #[test]
    fn varied_routine_without_cursor_play_never_seeks_the_pointer() {
        let cfg = Config { cursor_play: false, sleep_after: 1e9, ..Config::default() };
        let mut m = Mind::new(&cfg, 99);
        let seen = run(&mut m, &senses(Some(V2::new(300.0, 400.0)), 0.0), &cfg, 600.0);
        assert!(!seen.iter().any(|a| matches!(a, Act::Curious | Act::Chase | Act::Follow | Act::Notice)));
        let kinds: std::collections::HashSet<_> = seen.iter().collect();
        assert!(kinds.len() >= 7, "not repetitive: {kinds:?}");
    }

    #[test]
    fn inspects_a_resting_pointer_while_you_are_away() {
        let cfg = Config { sleep_after: 1e9, ..Config::default() };
        let mut m = Mind::new(&cfg, 5);
        let seen = run(&mut m, &senses(Some(V2::new(300.0, 900.0)), 60.0), &cfg, 900.0);
        assert!(seen.contains(&Act::Curious), "{seen:?}");
        assert!(!seen.iter().any(|a| matches!(a, Act::Chase | Act::Follow)), "no chasing a pointer that is not moving");
    }

    #[test]
    fn calm_while_you_work_playful_while_you_play() {
        let cfg = Config { sleep_after: 1e9, ..Config::default() };
        let working = senses(Some(V2::new(300.0, 900.0)), 0.2);
        let mut m = Mind::new(&cfg, 11);
        let seen = run(&mut m, &working, &cfg, 1800.0);
        assert!(!seen.iter().any(|a| matches!(a, Act::Chase | Act::Follow | Act::Curious | Act::Notice)), "{seen:?}");

        let mut chased = false;
        for _ in 0..40 {
            m.click(); // playing with it
            chased |= run(&mut m, &working, &cfg, 30.0).iter().any(|a| matches!(a, Act::Chase | Act::Follow));
        }
        assert!(chased);
    }

    #[test]
    fn sleeps_when_left_alone_and_wakes_happy_on_click() {
        let cfg = Config::default();
        let mut m = Mind::new(&cfg, 3);
        run(&mut m, &senses(None, 999.0), &cfg, cfg.sleep_after + 30.0);
        assert_eq!(m.act, Act::Sleep);
        m.click();
        assert_eq!(m.act, Act::Surprised);
        let seen = run(&mut m, &senses(None, 999.0), &cfg, 1.1);
        assert_eq!(seen, [Act::Surprised, Act::Happy]);
    }

    #[test]
    fn hard_throw_makes_it_dizzy_and_petting_makes_it_happy() {
        let cfg = Config::default();
        let mut m = Mind::new(&cfg, 8);
        m.grab();
        m.release(2500.0);
        m.landed(1200.0);
        assert_eq!(m.act, Act::Dizzy);

        m.set(Act::Idle, 100.0);
        let mut s = senses(None, 999.0);
        s.hovered = true;
        run(&mut m, &s, &cfg, 1.6);
        assert_eq!(m.act, Act::Love);
    }

    /// Plays the part of main.rs: moves the body and the pointer the way the mind asks.
    fn play(m: &mut Mind, cfg: &Config, away: f32, secs: f32) -> (Vec<Act>, Vec<Pointer>) {
        let screen = Bounds { left: 0.0, right: 1920.0, top: 0.0, bottom: 1079.0 };
        let mut s = senses(Some(V2::new(400.0, 300.0)), away);
        s.away = Some(away);
        s.can_move_pointer = true;
        let mut toy: Option<crate::physics::Toy> = None;
        let (mut acts, mut grips) = (vec![m.act], vec![]);
        for _ in 0..(secs / DT) as usize {
            s.hand = s.pos + V2::new(7.0 * s.scale * if s.cursor.unwrap().x > s.pos.x { 1.0 } else { -1.0 }, -10.0 * s.scale);
            s.toy = toy.as_ref().map(|t| t.pos);
            let it = m.update(DT, &s, cfg);
            s.pos.x += it.along * DT;
            let grip = match it.pointer {
                Pointer::Leave => {
                    toy = None;
                    None
                }
                Pointer::Pull => Some(crate::physics::Grip::Pull(s.hand)),
                Pointer::Hold => Some(crate::physics::Grip::Hold(s.hand)),
                Pointer::Loose => Some(crate::physics::Grip::Loose),
                Pointer::Fling(v) => {
                    toy.get_or_insert(crate::physics::Toy::new(s.hand)).vel = v;
                    Some(crate::physics::Grip::Loose)
                }
            };
            if let Some(g) = grip {
                let t = toy.get_or_insert(crate::physics::Toy::new(s.cursor.unwrap()));
                for _ in 0..4 {
                    t.step(g, &screen);
                }
                s.cursor = Some(t.pos);
            }
            if acts.last() != Some(&m.act) {
                acts.push(m.act);
            }
            if grips.last() != Some(&it.pointer) && !matches!(it.pointer, Pointer::Fling(_)) {
                grips.push(it.pointer);
            }
        }
        (acts, grips)
    }

    #[test]
    fn plays_a_whole_pointer_game_while_you_are_away() {
        let cfg = Config { mischief_every: 5.0, climbing: false, ..Config::default() };
        let mut m = Mind::new(&cfg, 21);
        let (acts, grips) = play(&mut m, &cfg, cfg.afk_after + 1.0, 90.0);
        let order = [Act::Tug, Act::Carry, Act::Throw, Act::Bat, Act::Happy];
        let mut it = acts.iter();
        assert!(order.iter().all(|a| it.any(|b| b == a)), "full game in order: {acts:?}");
        assert!(grips.contains(&Pointer::Pull) && grips.contains(&Pointer::Hold) && grips.contains(&Pointer::Loose));
    }

    #[test]
    fn lets_go_the_moment_you_come_back() {
        let cfg = Config { mischief_every: 5.0, climbing: false, ..Config::default() };
        let mut m = Mind::new(&cfg, 21);
        play(&mut m, &cfg, cfg.afk_after + 1.0, 14.0);
        assert!(m.game, "a game is under way: {:?}", m.act);
        let (acts, grips) = play(&mut m, &cfg, 0.0, 0.1);
        assert_eq!(grips, [Pointer::Leave]);
        assert_eq!(acts.last(), Some(&Act::Surprised));
        let (_, grips) = play(&mut m, &cfg, 5.0, 60.0);
        assert_eq!(grips, [Pointer::Leave], "never touches the pointer while you are around");
    }

    #[test]
    fn no_pointer_games_when_disabled_or_unsupported_or_long_gone() {
        let off = Config { cursor_mischief: false, mischief_every: 5.0, ..Config::default() };
        let (_, grips) = play(&mut Mind::new(&off, 4), &off, 600.0, 120.0);
        assert_eq!(grips, [Pointer::Leave]);
        let cfg = Config { mischief_every: 5.0, ..Config::default() };
        let (_, grips) = play(&mut Mind::new(&cfg, 4), &cfg, cfg.afk_after + cfg.mischief_for + 1.0, 120.0);
        assert_eq!(grips, [Pointer::Leave], "stops after mischief_for so the screen can lock");
    }

    #[test]
    fn climbing_outings_explore_the_edges_and_come_back_down() {
        use crate::physics::{Body, Drive, STEP};
        let cfg = Config { sleep_after: 1e9, ..Config::default() };
        let mut seen_surfaces = std::collections::HashSet::new();
        for seed in 1..6 {
            let mut m = Mind::new(&cfg, seed);
            let mut body = Body::new(V2::new(900.0, B.bottom - P.foot_gap));
            m.set(Act::Climb, 120.0);
            let mut finished = false;
            for _ in 0..(200.0 / DT) as usize {
                let s = Senses {
                    pos: body.pos,
                    vel: body.vel,
                    grounded: body.grounded(),
                    surface: body.surface,
                    turning: body.turning(),
                    bounds: &B,
                    params: &P,
                    cursor: None,
                    cursor_still: 999.0,
                    hovered: false,
                    away: None,
                    can_move_pointer: false,
                    toy: None,
                    hand: V2::default(),
                    scale: 6.0,
                };
                let it = m.update(DT, &s, &cfg);
                if let Some(to) = it.transfer {
                    body.transfer(to, &P, &B);
                }
                if let Some(v) = it.let_go {
                    body.let_go(v, &P);
                }
                if let Some(v) = it.jump {
                    body.jump(v);
                }
                for _ in 0..(DT / STEP).round() as usize {
                    let impact = body.step(&P, &B, &Drive { along: it.along, accel: it.accel }, None);
                    if impact > 60.0 {
                        m.landed(impact);
                    }
                }
                if let Some(surface) = body.surface {
                    seen_surfaces.insert(surface);
                }
                if !matches!(m.act, Act::Climb | Act::Slip | Act::Fly | Act::Surprised | Act::Dizzy) && body.grounded() {
                    finished = true;
                    break;
                }
            }
            assert!(finished && body.grounded(), "seed {seed}: outing ended back on the floor ({:?}, {:?})", m.act, body.surface);
        }
        assert!(seen_surfaces.contains(&Surface::LeftWall) || seen_surfaces.contains(&Surface::RightWall));
        assert!(seen_surfaces.contains(&Surface::Top), "hung from the top edge at least once: {seen_surfaces:?}");
    }

    #[test]
    fn a_click_startles_it_off_the_wall() {
        let cfg = Config::default();
        let mut m = Mind::new(&cfg, 2);
        m.set(Act::Climb, 120.0);
        let mut s = senses(None, 999.0);
        s.surface = Some(Surface::LeftWall);
        s.grounded = false;
        m.update(DT, &s, &cfg);
        m.click();
        assert_eq!(m.act, Act::Slip);
        let mut let_go = false;
        for _ in 0..30 {
            let_go |= m.update(DT, &s, &cfg).let_go.is_some();
        }
        assert!(let_go, "loses its grip after a nervous moment");
    }

    #[test]
    fn right_click_pins_until_left_click() {
        let cfg = Config::default();
        let mut m = Mind::new(&cfg, 1);
        m.cycle();
        let pinned = m.act;
        run(&mut m, &senses(None, 999.0), &cfg, 400.0);
        assert_eq!(m.act, pinned);
        m.click();
        assert_eq!(m.act, Act::Happy);
    }
}
