//! Animation: turns what the mascot is doing and how its body is moving into a pose.
//!
//! The face is a set of continuous parameters (eye openness and curve, eye size, mouth,
//! smile, blush), never a swapped image, so every change of expression is a smooth morph
//! of the same face. Every frame combines:
//! 1. locomotion, from the physics body: a two-legged walk or run cycle driven by distance
//!    travelled (feet never slide), and flailing while airborne;
//! 2. the current activity: expression, arms, effects;
//! 3. physics springs (lean, wobble, squash) and the spark's follow-through, added by `Animator`.
//! The Animator eases every number toward its target, so nothing ever snaps.
//!
//! Effect positions are art pixels on the 25x25 canvas (see `render::GRID`); the feet stand at (12, 24).

use crate::physics::{Body, Surface, V2};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Act {
    Idle,
    Look,
    Walk,
    Run,
    Sit,
    Stretch,
    Think,
    Work,
    Happy,
    Sleep,
    Surprised,
    Dizzy,
    Love,
    Wave,
    Curious,
    Chase,
    Follow,
    /// An outing up the side and top edges of the screen.
    Climb,
    /// Losing its grip on an edge, then falling.
    Slip,
    Notice,
    Fly,
    Held,
    // pointer games while you are away
    Sneak,
    Tug,
    Carry,
    Throw,
    Bat,
}

impl Act {
    /// Acts you can pin with right-click, in order.
    pub const CYCLE: [Act; 18] = [
        Act::Idle,
        Act::Look,
        Act::Walk,
        Act::Run,
        Act::Sit,
        Act::Stretch,
        Act::Think,
        Act::Work,
        Act::Happy,
        Act::Sleep,
        Act::Surprised,
        Act::Dizzy,
        Act::Love,
        Act::Wave,
        Act::Curious,
        Act::Chase,
        Act::Follow,
        Act::Climb,
    ];

    /// Acts that only happen in context (reactions, being thrown or held, pointer games).
    pub const OTHERS: [Act; 9] = [Act::Notice, Act::Fly, Act::Held, Act::Sneak, Act::Tug, Act::Carry, Act::Throw, Act::Bat, Act::Slip];

    pub fn name(self) -> &'static str {
        match self {
            Act::Idle => "idle",
            Act::Look => "looking",
            Act::Walk => "walking",
            Act::Run => "running",
            Act::Sit => "sitting",
            Act::Stretch => "stretching",
            Act::Think => "thinking",
            Act::Work => "working",
            Act::Happy => "happy",
            Act::Sleep => "sleeping",
            Act::Surprised => "surprised",
            Act::Dizzy => "dizzy",
            Act::Love => "love",
            Act::Wave => "wave",
            Act::Curious => "curious",
            Act::Chase => "chasing",
            Act::Follow => "following",
            Act::Climb => "climbing",
            Act::Slip => "slipping",
            Act::Notice => "noticing",
            Act::Fly => "flying",
            Act::Held => "held",
            Act::Sneak => "sneaking",
            Act::Tug => "tugging",
            Act::Carry => "carrying",
            Act::Throw => "throwing",
            Act::Bat => "batting",
        }
    }
}

#[derive(Clone, Copy)]
pub enum FxKind {
    Heart,
    Spark,
    Z,
    Exclaim,
    Question,
    Dust,
    Puff,
    /// A thought bubble holding this many dots.
    Bubble(u8),
    Laptop,
    Sweat,
}

/// A floating effect, centered on (x, y) of the canvas.
#[derive(Clone)]
pub struct Fx {
    pub kind: FxKind,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub alpha: f32,
}

fn fx(kind: FxKind, x: f32, y: f32, scale: f32, alpha: f32) -> Fx {
    Fx { kind, x, y, scale, alpha: alpha.clamp(0.0, 1.0) }
}

/// Everything the renderer needs for one frame. Lengths are art pixels.
#[derive(Clone)]
pub struct Pose {
    /// Small offsets of the body from its feet.
    pub dx: f32,
    pub dy: f32,
    /// Squash and stretch, anchored at the feet.
    pub sx: f32,
    pub sy: f32,
    /// Lean in radians, positive = clockwise, about the middle of the body.
    pub tilt: f32,
    /// -1 (left) to 1 (right): the face slides toward the side it faces, so turning reads as a head turn.
    pub facing: f32,
    /// Where the eyes look, each -1 to 1.
    pub look_x: f32,
    pub look_y: f32,
    /// 1 = open, 0 = closed.
    pub eye_open: f32,
    /// Shape of closed eyes: 1 = happy ^, 0 = flat, -1 = sleepy u.
    pub eye_curve: f32,
    pub eye_size: f32,
    /// Above 0.5 the eyes are spirals.
    pub dizzy: f32,
    /// 0 = closed, 1 = wide open (a yawn).
    pub mouth: f32,
    pub smile: f32,
    pub blush: f32,
    /// Arm angles in radians: 0 = straight out, positive = raised.
    pub arm_l: f32,
    pub arm_r: f32,
    /// Foot lift (negative dangles) and forward offset for the left and right leg.
    pub legs: [f32; 2],
    pub stride: [f32; 2],
    /// Feet planted wider apart (sitting).
    pub spread: f32,
    /// Angle of the spark on top of the head.
    pub tuft: f32,
    /// 1 = the front paw is closed around the mouse pointer.
    pub grip: f32,
    /// How far the paws reach from the shoulders.
    pub reach: f32,
    /// The edge it is on, which sets which way its feet point (not eased: it turns in one go).
    pub mount: Surface,
    /// Seconds since start; turns the dizzy spirals.
    pub spin: f32,
    pub fx: Vec<Fx>,
}

/// What the animation layer reads each frame.
pub struct Ctx {
    pub act: Act,
    /// Seconds in the current act, and since start.
    pub t: f32,
    pub clock: f32,
    pub vel: V2,
    /// Standing, climbing or hanging on an edge (not airborne, not mid-corner).
    pub grounded: bool,
    pub spinning: bool,
    /// The edge whose way its feet point, and whether it is rounding a corner.
    pub mount: Surface,
    pub turning: bool,
    /// Distance walked or climbed, in screen pixels. Velocity and look are in its own frame,
    /// as if it stood on the floor.
    pub stride: f32,
    /// Screen pixels per canvas unit.
    pub scale: f32,
    /// From the head to whatever it is looking at, in screen pixels.
    pub look: Option<V2>,
}

const ARM_REST: f32 = -1.0;
const REACH: f32 = 3.2;

fn osc(t: f32, period: f32) -> f32 {
    (t * TAU / period).sin()
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}

/// 0 below `lo`, 1 above `hi`, smooth in between.
fn smooth(x: f32, lo: f32, hi: f32) -> f32 {
    let u = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Fade in quickly, fade out slowly over a 0..1 phase.
fn fade(ph: f32) -> f32 {
    (ph * 5.0).min((1.0 - ph) * 2.5).clamp(0.0, 1.0)
}

/// Blinks at irregular intervals (3-6 s apart), sometimes twice.
fn blinking(clock: f32) -> bool {
    let cycle = clock / 4.3;
    let n = cycle.floor();
    let at = fract(cycle) * 4.3 - fract(n * 0.618) * 1.5;
    (0.0..0.14).contains(&at) || (n % 3.0 == 1.0 && (0.3..0.44).contains(&at))
}

/// The pose the mascot is heading for this frame, before easing and physics.
/// `facing` is the side it faced last frame (1 or -1).
pub fn target(c: &Ctx, facing: f32) -> Pose {
    let t = c.t;
    let breath = osc(c.clock, 3.6);
    let mut p = Pose {
        dx: 0.0,
        dy: 0.0,
        sx: 1.0 - 0.012 * breath,
        sy: 1.0 + 0.025 * breath,
        tilt: 0.0,
        facing,
        look_x: 0.0,
        look_y: 0.0,
        eye_open: if blinking(c.clock) { 0.0 } else { 1.0 },
        eye_curve: 0.0,
        eye_size: 1.0,
        dizzy: 0.0,
        mouth: 0.0,
        smile: 0.0,
        blush: 0.0,
        arm_l: ARM_REST + 0.05 * breath,
        arm_r: ARM_REST + 0.05 * breath,
        legs: [0.0; 2],
        stride: [0.0; 2],
        spread: 0.0,
        tuft: 0.05 * osc(c.clock, 5.0),
        grip: 0.0,
        reach: REACH,
        mount: c.mount,
        spin: c.clock,
        fx: Vec::new(),
    };

    // face where it is going, else what it is looking at
    let speed = c.vel.x.abs() / c.scale; // canvas units per second
    if c.grounded && speed > 1.5 {
        p.facing = c.vel.x.signum();
    } else if let Some(l) = c.look.filter(|l| l.x.abs() > 3.0 * c.scale) {
        p.facing = l.x.signum();
    }
    if let Some(l) = c.look {
        let d = l * (1.0 / l.len().max(1.0));
        p.look_x = (d.x * 1.5).clamp(-1.0, 1.0);
        p.look_y = (d.y * 1.5).clamp(-1.0, 1.0);
    } else if speed > 1.5 {
        p.look_x = p.facing;
    }

    // locomotion: two feet alternate, each swinging forward while lifted
    let walk = smooth(speed, 1.0, 5.0);
    let run = smooth(speed, 14.0, 26.0);
    if c.grounded {
        let phase = c.stride / c.scale / (6.0 + 4.0 * run) * TAU;
        let (a, b) = (phase.sin(), phase.cos());
        let lift = 1.0 * walk + 0.8 * run;
        let reach = (1.0 * walk + 1.3 * run) * c.vel.x.signum();
        p.legs = [a.max(0.0) * lift, (-a).max(0.0) * lift];
        p.stride = [-b * reach, b * reach];
        p.dy -= a.abs() * (0.4 * walk + 0.5 * run);
        let swing = a * (0.45 * walk + 0.4 * run);
        p.arm_l += swing;
        p.arm_r -= swing;
        if run > 0.5 {
            for i in 0..2 {
                let q = fract(phase / TAU + i as f32 * 0.5);
                let alpha = (1.0 - q) * 0.7 * (run - 0.5) * 2.0;
                p.fx.push(fx(FxKind::Dust, 12.0 - p.facing * (6.0 + q * 3.0), 23.0 - q, 1.0, alpha));
            }
        }
    } else if c.act != Act::Held {
        // airborne: feet dangle, arms flap for balance
        p.legs = [-0.6; 2];
        p.arm_l = 1.0 + 0.4 * osc(c.clock, 0.2);
        p.arm_r = 1.0 + 0.4 * osc(c.clock + 0.1, 0.2);
        let falling = smooth(c.vel.y, 300.0, 900.0);
        p.eye_size += 0.25 * falling;
        p.mouth = 0.35 * falling;
    }

    // the edge it is on
    if c.turning {
        // rounding a corner: tucked into a ball, both paws gripping
        p.sy *= 0.9;
        p.sx *= 1.08;
        p.arm_l = -1.3;
        p.arm_r = -1.3;
        p.legs = [0.3; 2];
        p.eye_open = p.eye_open.min(0.6);
    } else if c.grounded {
        match c.mount {
            Surface::LeftWall | Surface::RightWall => {
                // clinging low to a side edge, paws reaching alternately as it climbs
                let phase = c.stride / c.scale / 6.0 * TAU;
                let climbing = smooth(speed, 1.0, 5.0);
                p.dy += 0.6;
                p.sy *= 0.95;
                p.arm_l = -1.1 + 0.6 * phase.sin() * climbing;
                p.arm_r = -1.1 - 0.6 * phase.sin() * climbing;
                p.tuft = -0.3 * p.facing;
            }
            Surface::Top => {
                // hanging by the paws, hand over hand as it moves; feet dangle and swing
                let phase = c.stride / c.scale / 5.0 * TAU;
                let moving = smooth(speed, 1.0, 5.0);
                p.reach = 6.0;
                p.arm_l = FRAC_PI_2 + 0.25 * phase.sin() * moving;
                p.arm_r = FRAC_PI_2 - 0.25 * phase.sin() * moving;
                let swing = 0.4 * osc(c.clock, 1.1);
                p.legs = [-0.8 + swing, -0.8 - swing];
                p.stride = [0.0; 2];
                p.dy = 0.0;
                p.tuft = 0.75 * p.facing; // the spark droops to one side
            }
            Surface::Floor => {}
        }
    }

    // the activity
    match c.act {
        Act::Idle => {
            if c.look.is_none() && speed < 1.0 {
                p.look_x = [0.0, 0.0, 0.8, 0.0, 0.0, -0.8][(t / 2.4) as usize % 6]; // glance around
            }
        }
        Act::Look => {
            p.facing = if fract(t / 2.6) < 0.5 { facing } else { -facing };
            p.look_x = p.facing;
            p.look_y = if fract(t / 1.3) > 0.6 { -0.8 } else { 0.0 };
            p.tilt = 0.04 * p.facing;
        }
        Act::Walk | Act::Run => {}
        Act::Follow => p.smile = 0.6,
        Act::Sit => {
            p.dy += 1.6;
            p.sy *= 0.96;
            p.sx *= 1.03;
            p.spread = 1.0;
            p.legs = [-0.2; 2];
            p.arm_l = -1.3;
            p.arm_r = -1.3;
            p.tilt = 0.025 * osc(t, 5.0);
            p.smile = 0.3;
        }
        Act::Stretch => {
            let up = smooth(t, 0.0, 1.0) - smooth(t, 1.6, 2.4);
            p.arm_l += (2.6 - p.arm_l) * up;
            p.arm_r += (2.6 - p.arm_r) * up;
            p.sy += 0.12 * up;
            p.sx -= 0.06 * up;
            p.dy -= 0.5 * up;
            p.eye_open = 1.0 - up;
            p.eye_curve = -0.6;
            p.mouth = smooth(t, 0.9, 1.3) - smooth(t, 1.9, 2.3); // yawn
            p.tuft = 0.15 * up * osc(t, 1.2);
        }
        Act::Sleep => {
            let b = osc(t, 4.5);
            p.eye_open = 0.0;
            p.eye_curve = -1.0;
            p.sy = 0.93 + 0.03 * b;
            p.sx = 1.05 - 0.015 * b;
            p.dy += 1.6;
            p.spread = 0.8;
            p.legs = [-0.2; 2];
            p.arm_l = -1.3;
            p.arm_r = -1.3;
            p.look_x = 0.0;
            p.look_y = 0.0;
            p.tuft = 0.7 * p.facing + 0.05 * b; // the spark droops
            p.tilt = 0.05 * p.facing;
            for i in 0..2 {
                // two Zs drift up and away from the head, small then big, never touching
                let ph = fract(t / 3.6 + i as f32 / 2.0);
                p.fx.push(fx(FxKind::Z, 19.0 + ph * 3.0, 10.0 - ph * 8.0, 0.6 + ph * 0.6, fade(ph)));
            }
        }
        Act::Think => {
            p.tilt = 0.05 * osc(t, 2.6);
            if c.look.is_none() {
                p.look_x = 0.7;
                p.look_y = -0.9;
            }
            p.arm_r = 0.4 + 0.1 * osc(t, 1.3); // hand to chin
            p.tuft = 0.25 * osc(t, 1.8);
            p.fx.push(fx(FxKind::Puff, 17.5, 8.5, 0.45, 1.0));
            p.fx.push(fx(FxKind::Puff, 19.0, 6.5, 0.65, 1.0));
            p.fx.push(fx(FxKind::Bubble(((t * 2.0) as usize % 4) as u8), 19.5, 3.0, 1.0, 1.0));
        }
        Act::Work => {
            p.dy -= 0.25 * (t * PI * 3.0).sin().abs();
            p.look_x = 0.0;
            p.look_y = 0.8;
            p.arm_l = -0.25 + 0.25 * osc(t, 0.22);
            p.arm_r = -0.25 + 0.25 * osc(t + 0.11, 0.22);
            p.fx.push(fx(FxKind::Laptop, 12.0, 21.0, 1.0, 1.0));
        }
        Act::Happy => {
            p.eye_open = 0.0;
            p.eye_curve = 1.0;
            p.mouth = 0.35;
            p.arm_l = 1.5 + 0.3 * osc(t, 0.4);
            p.arm_r = 1.5 + 0.3 * osc(t + 0.2, 0.4);
            for i in 0..3 {
                let a = i as f32 * TAU / 3.0 + t * 0.9;
                let tw = (t * 5.0 + i as f32 * 1.7).sin().abs();
                p.fx.push(fx(FxKind::Spark, 12.0 + 11.5 * a.cos(), 13.0 + 7.0 * a.sin(), 0.6 + 0.4 * tw, 0.3 + 0.7 * tw));
            }
        }
        Act::Surprised | Act::Notice => {
            let big = c.act == Act::Surprised;
            let shock = if big { 1.0 } else { 1.0 - smooth(t, 0.25, 0.6) };
            p.eye_size = 1.0 + 0.35 * shock;
            p.eye_open = 1.0;
            p.mouth = 0.55 * shock;
            p.arm_l = 1.6 * shock + ARM_REST * (1.0 - shock);
            p.arm_r = p.arm_l;
            p.tuft = 0.2 * osc(t, 0.15) * shock;
            let pop = (t * 6.0).min(1.0);
            let size = if big { 1.0 } else { 0.7 };
            p.fx.push(fx(FxKind::Exclaim, 17.5, 6.0, size * pop, pop));
        }
        Act::Dizzy => {
            let w = osc(t, 0.9);
            p.dizzy = 1.0;
            p.mouth = 0.2;
            p.tilt = 0.14 * w;
            p.dx = 0.6 * w;
            p.arm_l = -0.6 + 0.5 * w;
            p.arm_r = -0.6 - 0.5 * w;
            p.tuft = -0.5 * w;
            for i in 0..3 {
                let a = t * 3.5 + i as f32 * TAU / 3.0;
                let depth = a.sin();
                p.fx.push(fx(FxKind::Spark, 12.0 + 6.0 * a.cos(), 8.5 + 1.5 * depth, 0.75 + 0.15 * depth, 1.0));
            }
        }
        Act::Love => {
            p.eye_open = 0.0;
            p.eye_curve = 1.0;
            p.blush = 1.0;
            p.smile = 1.0;
            p.tilt = 0.06 * osc(t, 2.4);
            p.arm_l = -0.2 + 0.1 * osc(t, 1.2);
            p.arm_r = -0.2 + 0.1 * osc(t + 0.6, 1.2);
            p.tuft = 0.2 * osc(t, 2.4);
            for (i, x) in [6.0, 18.0, 12.0].into_iter().enumerate() {
                let ph = fract(t / 2.6 + i as f32 / 3.0);
                p.fx.push(fx(FxKind::Heart, x + 0.8 * osc(t + i as f32, 1.6), 10.0 - ph * 8.0, 0.6 + 0.4 * ph, fade(ph)));
            }
        }
        Act::Wave => {
            p.eye_open = 0.0;
            p.eye_curve = 1.0;
            p.smile = 1.0;
            p.tilt = -0.05 * p.facing;
            let wave = 1.9 + 0.45 * osc(t, 0.5);
            if p.facing > 0.0 { p.arm_r = wave } else { p.arm_l = wave }
        }
        Act::Curious => {
            p.eye_size = 1.1;
            if speed < 1.0 {
                // stands and inspects: head cocked, reaching toward the pointer when it is above
                p.tilt = 0.08 * p.facing;
                p.tuft = 0.3 * p.facing;
                if c.look.is_some_and(|l| l.y < 0.0) {
                    let reach = 1.2 + 0.25 * osc(t, 0.7);
                    if p.facing > 0.0 { p.arm_r = reach } else { p.arm_l = reach }
                }
                let shown = smooth(t, 0.6, 1.1);
                p.fx.push(fx(FxKind::Question, 12.0 + 6.0 * p.facing, 5.0 + 0.6 * osc(t, 1.4), 1.0, shown));
            }
        }
        Act::Chase => {
            p.smile = 0.7;
            p.eye_size = 1.1;
            if !c.grounded {
                p.eye_open = 0.0;
                p.eye_curve = 1.0;
                p.mouth = 0.4;
                p.arm_l = 2.2;
                p.arm_r = 2.2;
            }
        }
        Act::Climb => {
            if c.look.is_none() && speed < 1.0 && !c.turning {
                p.look_x = [0.0, 0.8, 0.0, -0.8, 0.0, 0.0][(t / 1.6) as usize % 6]; // stops to look around
                p.look_y = if fract(t / 3.1) > 0.7 { 0.8 } else { 0.0 };
            }
        }
        Act::Slip => {
            p.eye_size = 1.3;
            p.eye_open = 1.0;
            p.mouth = 0.3;
            if c.grounded && c.mount != Surface::Floor {
                p.dx = 0.4 * osc(t, 0.09); // trembling grip
            }
            let drip = fract(t * 1.8);
            p.fx.push(fx(FxKind::Sweat, 17.0, 9.0 + drip * 3.0, 1.0, 1.0 - drip * 0.5));
        }
        Act::Fly => {
            if c.spinning {
                p.dizzy = 1.0;
            }
        }
        Act::Sneak => {
            // creeping up on the pointer: low, slow, eyes narrowed, up to something
            p.sy *= 0.88;
            p.sx *= 1.06;
            p.dy += 0.8;
            p.eye_open = p.eye_open.min(0.55);
            p.smile = 0.6;
            p.tuft = -0.3 * p.facing;
            p.arm_l = -0.2;
            p.arm_r = -0.2;
        }
        Act::Tug => {
            // reaching for the pointer and reeling it in, leaning back with the effort
            let aim = c.look.map_or(1.6, |l| (-l.y).atan2(l.x.abs()));
            set_front(&mut p, aim, 0.3);
            p.grip = 1.0;
            p.tilt = -0.12 * p.facing;
            p.spread = 0.5;
            p.eye_open = p.eye_open.min(0.6);
            p.mouth = 0.15;
            p.dx = 0.15 * osc(t, 0.12);
        }
        Act::Carry => {
            // proudly holding it up
            set_front(&mut p, 2.3, 1.2 + 0.3 * osc(t, 0.5));
            p.grip = 1.0;
            p.eye_open = 0.0;
            p.eye_curve = 1.0;
            p.smile = 1.0;
            p.tuft = 0.25 * osc(t, 0.6);
        }
        Act::Throw => {
            if t < 0.45 {
                // wind-up: crouch and swing the paw back
                set_front(&mut p, 2.3 - 3.0 * smooth(t, 0.0, 0.4), -0.8);
                p.grip = 1.0;
                p.dy += 0.8;
                p.sy *= 0.92;
                p.eye_open = p.eye_open.min(0.6);
                p.smile = 0.5;
            } else {
                // release and follow through
                set_front(&mut p, 2.6, 0.8);
                p.eye_open = 0.0;
                p.eye_curve = 1.0;
                p.mouth = 0.4;
            }
        }
        Act::Bat => {
            // a quick swipe each time it hits the pointer, then watching where it rolls
            set_front(&mut p, 1.4 - 1.6 * smooth(t, 0.05, 0.2), -0.6);
            p.eye_size = 1.1;
            p.smile = 0.8;
        }
        Act::Held => {
            // dangling from the hand: feet swing, arms up, enjoying the ride unless it is wild
            let swing = 0.35 * osc(c.clock, 0.8);
            p.legs = [-0.8 + swing, -0.8 - swing];
            p.arm_l = 1.9;
            p.arm_r = 1.9;
            let wild = smooth(c.vel.len(), 900.0, 1800.0);
            p.eye_open = wild;
            p.eye_curve = 1.0;
            p.eye_size = 1.0 + 0.3 * wild;
            p.smile = 1.0 - wild;
            p.mouth = 0.45 * wild;
        }
    }
    p
}

/// Sets the arm on the side it faces (the one it uses) and the other arm.
fn set_front(p: &mut Pose, front: f32, back: f32) {
    if p.facing > 0.0 {
        (p.arm_r, p.arm_l) = (front, back);
    } else {
        (p.arm_l, p.arm_r) = (front, back);
    }
}

/// Eases the pose between frames, adds the physics springs on top, and swings the spark
/// on the head with follow-through.
pub struct Animator {
    cur: Option<Pose>,
    /// Which way it faces (1 or -1).
    facing: f32,
    tuft: f32,
    tuft_vel: f32,
    last_vel: V2,
}

impl Animator {
    pub fn new() -> Self {
        Animator { cur: None, facing: 1.0, tuft: 0.0, tuft_vel: 0.0, last_vel: V2::default() }
    }

    pub fn frame(&mut self, c: Ctx, body: &Body, dt: f32) -> Pose {
        let mut target = target(&c, self.facing);
        self.facing = target.facing.signum();
        let fx = std::mem::take(&mut target.fx);
        let cur = self.cur.get_or_insert_with(|| target.clone());
        // frame-rate independent exponential easing; eyes and feet move quicker so blinks
        // and steps keep their snap, the face slides around slower for a gentle head turn
        let ease = |a: &mut f32, b: f32, rate: f32| *a += (b - *a) * (1.0 - (-rate * dt).exp());
        for (a, b) in [
            (&mut cur.dx, target.dx),
            (&mut cur.dy, target.dy),
            (&mut cur.sx, target.sx),
            (&mut cur.sy, target.sy),
            (&mut cur.tilt, target.tilt),
            (&mut cur.look_x, target.look_x),
            (&mut cur.look_y, target.look_y),
            (&mut cur.eye_curve, target.eye_curve),
            (&mut cur.eye_size, target.eye_size),
            (&mut cur.dizzy, target.dizzy),
            (&mut cur.mouth, target.mouth),
            (&mut cur.smile, target.smile),
            (&mut cur.blush, target.blush),
            (&mut cur.arm_l, target.arm_l),
            (&mut cur.arm_r, target.arm_r),
            (&mut cur.spread, target.spread),
            (&mut cur.grip, target.grip),
            (&mut cur.reach, target.reach),
        ] {
            ease(a, b, 12.0);
        }
        ease(&mut cur.eye_open, target.eye_open, 35.0);
        ease(&mut cur.facing, target.facing, 7.0);
        for i in 0..2 {
            ease(&mut cur.legs[i], target.legs[i], 30.0);
            ease(&mut cur.stride[i], target.stride[i], 30.0);
        }
        cur.spin = target.spin;
        cur.mount = target.mount;

        // follow-through: the spark lags behind the body's acceleration and settles on a spring
        let accel = (body.vel.x - self.last_vel.x) / dt.max(1e-3) / c.scale; // canvas units/s²
        self.last_vel = body.vel;
        let force = (target.tuft - self.tuft) * 90.0 - self.tuft_vel * 7.0 - (accel * 0.06).clamp(-60.0, 60.0);
        self.tuft_vel += force * dt;
        self.tuft = (self.tuft + self.tuft_vel * dt).clamp(-1.1, 1.1);

        let mut out = Pose { fx, ..cur.clone() };
        out.tuft = self.tuft - body.tilt * 0.5;
        out.tilt += body.tilt;
        out.sx *= 1.0 + body.squash;
        out.sy *= 1.0 - body.squash;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(act: Act, t: f32, vel: V2, grounded: bool) -> Ctx {
        Ctx { act, t, clock: t, vel, grounded, spinning: false, mount: Surface::Floor, turning: false, stride: t * vel.x.abs(), scale: 3.33, look: Some(V2::new(120.0, -90.0)) }
    }

    fn numbers(p: &Pose) -> Vec<f32> {
        let mut v = vec![p.dx, p.dy, p.sx, p.sy, p.tilt, p.facing, p.look_x, p.look_y, p.eye_open, p.eye_curve];
        v.extend([p.eye_size, p.dizzy, p.mouth, p.smile, p.blush, p.arm_l, p.arm_r, p.spread, p.tuft, p.grip, p.reach]);
        v.extend(p.legs.iter().chain(&p.stride));
        v
    }

    #[test]
    fn poses_are_finite_for_every_act_and_motion() {
        for &act in Act::CYCLE.iter().chain(&Act::OTHERS) {
            for vel in [V2::new(0.0, 0.0), V2::new(45.0, 0.0), V2::new(-200.0, 0.0), V2::new(150.0, 700.0)] {
                for i in 0..300 {
                    let p = target(&ctx(act, i as f32 * 0.05, vel, vel.y == 0.0), 1.0);
                    assert!(numbers(&p).iter().all(|v| v.is_finite()), "{act:?} {vel:?}");
                    assert!(p.fx.iter().all(|f| f.x.is_finite() && f.y.is_finite() && (0.0..=1.0).contains(&f.alpha)));
                }
            }
        }
    }

    #[test]
    fn changes_never_snap() {
        // switching between any two acts: in the first frame every number covers only part of
        // the way to its new target; a snap would cover all of it
        let body = Body::new(V2::default());
        let acts: Vec<Act> = Act::CYCLE.iter().chain(&Act::OTHERS).copied().collect();
        let still = |act, t| ctx(act, t, V2::default(), true);
        for &from in &acts {
            for &to in &acts {
                let mut a = Animator::new();
                let mut before = Vec::new();
                for i in 0..60 {
                    before = numbers(&a.frame(still(from, i as f32 / 60.0), &body, 1.0 / 60.0));
                }
                let goal = numbers(&target(&still(to, 0.0), a.facing));
                let first = numbers(&a.frame(still(to, 0.0), &body, 1.0 / 60.0));
                for i in (0..before.len()).filter(|&i| i != 8) {
                    // (8 = eye openness: blinks are meant to be quick)
                    let (step, gap) = ((first[i] - before[i]).abs(), (goal[i] - before[i]).abs());
                    assert!(step <= 0.45 * gap + 0.08, "{from:?} -> {to:?}: value {i} jumped {} -> {}", before[i], first[i]);
                }
            }
        }
    }

    #[test]
    fn turning_around_slides_the_face_across() {
        let mut a = Animator::new();
        let body = Body::new(V2::default());
        a.frame(ctx(Act::Walk, 0.0, V2::new(45.0, 0.0), true), &body, 1.0 / 60.0);
        let mut p = a.frame(ctx(Act::Walk, 0.0, V2::new(-45.0, 0.0), true), &body, 1.0 / 60.0);
        assert!(p.facing > 0.7, "no instant flip: {}", p.facing);
        for _ in 0..90 {
            p = a.frame(ctx(Act::Walk, 0.0, V2::new(-45.0, 0.0), true), &body, 1.0 / 60.0);
        }
        assert!(p.facing < -0.95);
    }
}
