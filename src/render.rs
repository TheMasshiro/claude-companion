//! Draws the mascot as pixel art. Hand-made sprites are placed at whole-pixel positions on
//! a GRID x GRID pixel canvas (no anti-aliasing, no free rotation, no blending), then the
//! canvas is enlarged by a whole number with nearest-neighbour sampling, so every art pixel
//! is a crisp square on screen.
//!
//! The pose's smooth numbers become pixel decisions here: squash and stretch pick a body
//! frame, bobbing moves whole pixels, leaning shears rows by a pixel or two, tumbling turns
//! the sprite in exact quarter turns, and expressions pick eye and mouth sprites.
//!
//! On a side edge the whole character is drawn turned by a quarter turn, feet against the
//! edge; hanging from the top edge it is drawn higher in the canvas, paws at the very top.

use crate::anim::{FxKind, Pose};
use crate::physics::Surface;
use crate::sprite::{Sprite, Sprites};
use std::f32::consts::FRAC_PI_2;
use tiny_skia::Pixmap;

/// The canvas is GRID x GRID art pixels.
pub const GRID: f32 = 25.0;
const N: i32 = 25;
/// Bottom-centre of the feet on the canvas.
pub const FEET: (f32, f32) = (12.0, 24.0);
/// The body's bottom edge sits on this row; the feet show below it.
const BODY_BOTTOM: i32 = 21;
/// The body tumbles about this point when thrown hard.
const SPIN_PIVOT: (f32, f32) = (12.0, 14.0);
/// Turning onto a side edge rotates the whole canvas about its middle.
const CANVAS_MIDDLE: (f32, f32) = (12.5, 12.5);
/// Hanging from the top edge, the character is drawn this many pixels higher.
const HANG_RAISE: i32 = 9;
/// The clickable box around the upright body: left, top, right, bottom.
const HIT: [f32; 4] = [3.5, 10.0, 20.5, 24.0];
const OUTLINE: u32 = 0x3B1E14;

/// The window size for a requested size: a whole number of screen pixels per art pixel.
pub fn window_size(requested: u32) -> u32 {
    N as u32 * (requested as f32 / GRID).round().max(2.0) as u32
}

/// The art-pixel canvas while the character is being put together.
struct Layer {
    px: Vec<Option<u32>>,
    /// Per-row horizontal shift: a lean is drawn by shearing rows, as pixel artists do.
    shear: [i32; N as usize],
}

impl Layer {
    fn new() -> Self {
        Layer { px: vec![None; (N * N) as usize], shear: [0; N as usize] }
    }

    fn get(&self, x: i32, y: i32) -> Option<u32> {
        if (0..N).contains(&x) && (0..N).contains(&y) { self.px[(y * N + x) as usize] } else { None }
    }

    fn set(&mut self, x: i32, y: i32, c: u32) {
        if (0..N).contains(&x) && (0..N).contains(&y) {
            self.px[(y * N + x) as usize] = Some(c);
        }
    }

    /// Character pixels go through the lean.
    fn put(&mut self, x: i32, y: i32, c: u32) {
        let shift = if (0..N).contains(&y) { self.shear[y as usize] } else { 0 };
        self.set(x + shift, y, c);
    }

    fn blit(&mut self, s: &Sprite, left: i32, top: i32) {
        for y in 0..s.h {
            for x in 0..s.w {
                if let Some(c) = s.at(x, y) {
                    self.put(left + x, top + y, c);
                }
            }
        }
    }

    /// An effect sprite with a 1-pixel outline drawn around it, behind nothing.
    fn blit_outlined(&mut self, s: &Sprite, left: i32, top: i32) {
        for y in 0..s.h {
            for x in 0..s.w {
                if s.at(x, y).is_none() {
                    continue;
                }
                for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                    let inside = (0..s.w).contains(&nx) && (0..s.h).contains(&ny) && s.at(nx, ny).is_some();
                    if !inside && self.get(left + nx, top + ny).is_none() {
                        self.set(left + nx, top + ny, OUTLINE);
                    }
                }
            }
        }
        for y in 0..s.h {
            for x in 0..s.w {
                if let Some(c) = s.at(x, y) {
                    self.set(left + x, top + y, c);
                }
            }
        }
    }

    /// Exact quarter turns about `pivot` (on a pixel corner); lossless on a pixel grid.
    fn turn(&mut self, quarters: i32, pivot: (f32, f32)) {
        if quarters == 0 {
            return;
        }
        let old = std::mem::replace(&mut self.px, vec![None; (N * N) as usize]);
        for y in 0..N {
            for x in 0..N {
                let Some(c) = old[(y * N + x) as usize] else { continue };
                let (tx, ty) = rotate((x as f32 + 0.5, y as f32 + 0.5), quarters, pivot);
                self.set(tx.floor() as i32, ty.floor() as i32, c);
            }
        }
    }
}

/// A point turned by quarter turns about `pivot` (each turn: down -> left, as in physics).
fn rotate(pt: (f32, f32), quarters: i32, pivot: (f32, f32)) -> (f32, f32) {
    let (mut dx, mut dy) = (pt.0 - pivot.0, pt.1 - pivot.1);
    for _ in 0..quarters.rem_euclid(4) {
        (dx, dy) = (-dy, dx);
    }
    (pivot.0 + dx, pivot.1 + dy)
}

fn raise(mount: Surface) -> i32 {
    if mount == Surface::Top { HANG_RAISE } else { 0 }
}

/// Where the feet are on the canvas for a mount; the window is placed so they land on the body's position.
pub fn anchor(mount: Surface) -> (f32, f32) {
    rotate((FEET.0, FEET.1 - raise(mount) as f32), mount.quarter_turns(), CANVAS_MIDDLE)
}

/// The clickable box around the body on the canvas for a mount: left, top, right, bottom.
pub fn hit_rect(mount: Surface) -> [f32; 4] {
    let r = raise(mount) as f32;
    let q = mount.quarter_turns();
    let a = rotate((HIT[0], HIT[1] - r), q, CANVAS_MIDDLE);
    let b = rotate((HIT[2], HIT[3] - r), q, CANVAS_MIDDLE);
    [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
}

/// Which body frame a pose uses, where its top-left lands, and which row the eyes sit on.
fn body_frame<'a>(p: &Pose, s: &'a Sprites) -> (&'a Sprite, i32, i32, i32) {
    let (name, eye_row) = if p.sy < 0.93 {
        ("body_squash", 2)
    } else if p.sy > 1.07 {
        ("body_stretch", 4)
    } else {
        ("body", 3)
    };
    let b = s.get(name);
    let breathe_in = (p.sy > 1.018 && name == "body") as i32; // the body rises a pixel at the top of each breath
    let left = FEET.0 as i32 + p.dx.round() as i32 - b.w / 2;
    let top = BODY_BOTTOM - raise(p.mount) + 1 - b.h + p.dy.round() as i32 - breathe_in;
    (b, left, top, eye_row)
}

fn front(p: &Pose) -> i32 {
    if p.facing > 0.0 { 1 } else { -1 }
}

/// Centre of a paw on the canvas, before the lean.
fn paw_centre(p: &Pose, body: &Sprite, left: i32, top: i32, side: i32) -> (f32, f32) {
    let angle = if side > 0 { p.arm_r } else { p.arm_l };
    let shoulder_x = left as f32 + body.w as f32 / 2.0 + side as f32 * body.w as f32 / 2.0;
    let shoulder_y = top as f32 + 5.5;
    (shoulder_x + side as f32 * angle.cos() * p.reach, shoulder_y - angle.sin() * p.reach)
}

/// A lean, drawn by shifting rows. Standing it pivots at the feet; hanging, at the paws.
fn lean_shear(p: &Pose, body_top: i32) -> [i32; N as usize] {
    let mut shear = [0; N as usize];
    if p.tilt.abs() <= 0.6 {
        let lean = (p.tilt * 7.0).round().clamp(-3.0, 3.0);
        let hanging = p.mount == Surface::Top;
        for (y, s) in shear.iter_mut().enumerate() {
            let rows = if hanging { body_top - 3 - y as i32 } else { BODY_BOTTOM - y as i32 };
            *s = (lean * rows.max(if hanging { i32::MIN } else { 0 }) as f32 / 10.0).round() as i32;
        }
    }
    shear
}

/// Where the paw it uses is, in canvas pixels. This is where it holds the mouse pointer.
pub fn hand(p: &Pose, s: &Sprites) -> (f32, f32) {
    let (b, left, top, _) = body_frame(p, s);
    let (x, y) = paw_centre(p, b, left, top, front(p));
    let row = (y as i32).clamp(0, N - 1) as usize;
    rotate((x + lean_shear(p, top)[row] as f32, y), p.mount.quarter_turns(), CANVAS_MIDDLE)
}

fn character(p: &Pose, s: &Sprites) -> Layer {
    let mut l = Layer::new();
    let (b, left, top, eye_row) = body_frame(p, s);
    l.shear = lean_shear(p, top);
    let cx = left + b.w / 2;
    let bottom = BODY_BOTTOM - raise(p.mount);

    // feet, behind the body: lifted feet tuck up under it, dangling feet show more leg
    for (i, side) in [-1, 1].into_iter().enumerate() {
        let x = cx + side * 2 - 2 + (p.stride[i] + side as f32 * 1.5 * p.spread).round() as i32;
        l.blit(s.get("foot"), x, bottom - p.legs[i].round() as i32);
    }

    // the spark on its stalk
    let lean = if p.tuft < -0.35 { -1 } else if p.tuft > 0.35 { 1 } else { 0 };
    let droop = p.tuft.abs() > 0.65;
    l.put(cx, top - 1, OUTLINE);
    let tip = if droop { (cx + lean, top - 1) } else { (cx + lean, top - 2) };
    l.put(tip.0, tip.1, OUTLINE);
    let spark_c = if droop { (tip.0 + 2 * lean, tip.1 - 1) } else { (tip.0, tip.1 - 2) };
    l.blit(s.get("tuft_spark"), spark_c.0 - 2, spark_c.1 - 2);

    l.blit(b, left, top);

    // face
    let fy = top + eye_row;
    let look_x = (p.look_x * 0.8 + p.facing * 0.7).round().clamp(-2.0, 2.0) as i32;
    let look_y = p.look_y.round().clamp(-1.0, 1.0) as i32;
    let eye = if p.dizzy > 0.5 {
        if (p.spin * 6.0) as i32 % 2 == 0 { "eye_dizzy_a" } else { "eye_dizzy_b" }
    } else if p.eye_open > 0.75 {
        if p.eye_size > 1.15 { "eye_wide" } else { "eye_open" }
    } else if p.eye_open > 0.35 {
        "eye_squint"
    } else if p.eye_curve > 0.4 {
        "eye_happy"
    } else if p.eye_curve < -0.4 {
        "eye_sleepy"
    } else {
        "eye_blink"
    };
    let e = s.get(eye);
    for side in [-1, 1] {
        let ecx = cx + side * 3 + look_x;
        l.blit(e, ecx - e.w / 2, fy + (4 - e.h) / 2 + look_y);
    }
    if p.blush > 0.5 {
        for side in [-1, 1] {
            l.blit(s.get("blush"), cx + side * 5 - 1, fy + 3);
        }
    }
    let mouth = if p.mouth > 0.6 {
        Some("mouth_yawn")
    } else if p.mouth > 0.3 {
        Some("mouth_open")
    } else if p.mouth > 0.1 {
        Some("mouth_o")
    } else if p.smile > 0.4 {
        Some("mouth_smile")
    } else {
        None
    };
    if let Some(m) = mouth.map(|m| s.get(m)) {
        let row = (fy + 4).min(top + b.h - 1 - m.h);
        l.blit(m, cx - m.w / 2 + look_x / 2, row);
    }

    // paws, in front
    for side in [-1, 1] {
        let (x, y) = paw_centre(p, b, left, top, side);
        let paw = if p.grip > 0.5 && side == front(p) { "paw_grip" } else { "paw" };
        l.blit(s.get(paw), (x - 2.0).round() as i32, (y - 2.0).round() as i32);
    }

    if p.tilt.abs() > 0.6 {
        l.turn(((p.tilt / FRAC_PI_2).round() as i32).rem_euclid(4), SPIN_PIVOT);
    }
    l.turn(p.mount.quarter_turns(), CANVAS_MIDDLE);
    l
}

pub fn render(p: &Pose, s: &Sprites, canvas: &mut Pixmap) {
    let mut l = character(p, s);
    for f in &p.fx {
        if f.alpha < 0.35 {
            continue; // pixel art does not fade: effects pop in and out
        }
        let small = f.scale < 0.75;
        let (name, outline) = match f.kind {
            FxKind::Heart => (if small { "heart_small" } else { "heart" }, true),
            FxKind::Spark => (if small { "spark_small" } else { "spark" }, true),
            FxKind::Z => (if small { "z_small" } else { "z" }, true),
            FxKind::Exclaim => ("exclaim", true),
            FxKind::Question => ("question", true),
            FxKind::Dust => ("dust", false),
            FxKind::Puff => (if f.scale < 0.6 { "puff_small" } else { "puff" }, true),
            FxKind::Bubble(_) => ("bubble", true),
            FxKind::Laptop => ("laptop", false),
            FxKind::Sweat => ("sweat", true),
        };
        let sp = s.get(name);
        // effects follow the character round onto a side edge, but stay upright themselves
        let (fx, fy) = rotate((f.x, f.y), p.mount.quarter_turns(), CANVAS_MIDDLE);
        let (left, top) = ((fx - sp.w as f32 / 2.0).round() as i32, (fy - sp.h as f32 / 2.0).round() as i32);
        if outline {
            l.blit_outlined(sp, left, top);
        } else {
            for y in 0..sp.h {
                for x in 0..sp.w {
                    if let Some(c) = sp.at(x, y) {
                        l.set(left + x, top + y, c);
                    }
                }
            }
        }
        if let FxKind::Bubble(dots) = f.kind {
            for i in 0..dots as i32 {
                l.set(left + 2 + 2 * i, top + 2, OUTLINE);
            }
        }
    }

    // nearest-neighbour enlargement by a whole number: each art pixel becomes a k x k block
    let k = (canvas.width() / N as u32).max(1) as usize;
    let width = canvas.width() as usize;
    let data = canvas.data_mut();
    data.fill(0);
    for y in 0..N as usize {
        for x in 0..N as usize {
            let Some(c) = l.px[y * N as usize + x] else { continue };
            let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
            for yy in 0..k {
                for xx in 0..k {
                    let i = ((y * k + yy) * width + x * k + xx) * 4;
                    data[i..i + 4].copy_from_slice(&rgba);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Act, Animator, Ctx};
    use crate::physics::{Body, V2, center_offset};

    #[test]
    fn every_act_renders_crisp_square_pixels_at_any_size() {
        let sprites = Sprites::load();
        for size in [50, 100, 300] {
            let mut canvas = Pixmap::new(size, size).unwrap();
            let k = (size / 25) as usize;
            let body = Body::new(V2::default());
            for (act, mount) in Act::CYCLE.into_iter().chain(Act::OTHERS).flat_map(|a| MOUNTS.map(|m| (a, m))) {
                let mut anim = Animator::new();
                for i in 0..40 {
                    let t = i as f32 * 0.1;
                    let ctx = Ctx { act, t, clock: t, vel: V2::new(60.0, 0.0), grounded: true, spinning: false, mount, turning: false, stride: t * 60.0, scale: k as f32, look: None };
                    render(&anim.frame(ctx, &body, 0.1), &sprites, &mut canvas);
                    let px = canvas.pixels();
                    assert!(px.iter().any(|p| p.alpha() > 0), "{act:?} blank at {size}");
                    // no smoothing anywhere: every pixel is fully opaque or fully clear,
                    // and every k x k block is one solid color
                    assert!(px.iter().all(|p| p.alpha() == 0 || p.alpha() == 255));
                    for by in 0..25 {
                        for bx in 0..25 {
                            let first = px[by * k * size as usize + bx * k];
                            for yy in 0..k {
                                for xx in 0..k {
                                    assert_eq!(px[(by * k + yy) * size as usize + bx * k + xx], first);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    const MOUNTS: [Surface; 4] = [Surface::Floor, Surface::LeftWall, Surface::RightWall, Surface::Top];

    #[test]
    fn the_body_sits_where_physics_puts_it_on_every_edge() {
        // physics keeps the body's middle 7 art pixels from the feet, pointing away from the edge
        let sprites = Sprites::load();
        for mount in MOUNTS {
            let mut anim = Animator::new();
            let ctx = Ctx { act: Act::Idle, t: 0.0, clock: 1.0, vel: V2::default(), grounded: true, spinning: false, mount, turning: false, stride: 0.0, scale: 4.0, look: None };
            let p = anim.frame(ctx, &Body::new(V2::default()), 0.1);
            let l = character(&p, &sprites);
            let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
            for y in 0..N {
                for x in 0..N {
                    if l.get(x, y) == Some(0xD97757) {
                        (sx, sy, n) = (sx + x as f32 + 0.5, sy + y as f32 + 0.5, n + 1.0);
                    }
                }
            }
            let (ax, ay) = anchor(mount);
            let expected = center_offset(mount, 7.0);
            let (mx, my) = (sx / n - ax, sy / n - ay);
            assert!((mx - expected.x).abs() < 1.6 && (my - expected.y).abs() < 1.6, "{mount:?}: body middle at {mx},{my}, physics expects {expected:?}");
            let r = hit_rect(mount);
            assert!(r[0] <= sx / n && sx / n <= r[2] && r[1] <= sy / n && sy / n <= r[3], "{mount:?}: clickable where the body is");
            // and every pixel stays on the canvas
            assert!((0.0..=25.0).contains(&ax) && (0.0..=25.0).contains(&ay));
        }
    }

    #[test]
    fn hard_throws_tumble_in_exact_quarter_turns() {
        let sprites = Sprites::load();
        let mut p = Animator::new().frame(
            Ctx { act: Act::Idle, t: 0.0, clock: 0.0, vel: V2::default(), grounded: true, spinning: false, mount: Surface::Floor, turning: false, stride: 0.0, scale: 4.0, look: None },
            &Body::new(V2::default()),
            0.1,
        );
        p.fx.clear();
        let count = |p: &Pose| character(p, &sprites).px.iter().filter(|c| c.is_some()).count();
        let upright = count(&p);
        for quarter in 1..4 {
            p.tilt = quarter as f32 * FRAC_PI_2;
            assert_eq!(count(&p), upright, "no pixels lost or invented at {quarter} quarter turns");
        }
    }
}
