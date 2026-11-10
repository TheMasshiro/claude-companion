//! Wiring: window, input and the frame loop. Each frame the mind decides, the physics
//! moves the body in fixed steps, the animator poses it, and the window follows the body.

mod anim;
mod behavior;
mod config;
mod cursor;
mod physics;
mod render;
mod sprite;
mod x11;

use anim::{Act, Animator, Ctx, Pose};
use behavior::{Mind, Pointer, Senses};
use config::Config;
use cursor::{CursorFeed, CursorTrack};
use physics::{Body, Bounds, Drive, Grip, Params, STEP, Surface, Toy, V2, center_offset, to_local};
use sprite::Sprites;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tiny_skia::{Pixmap, PixmapPaint, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::x11::{EventLoopBuilderExtX11, WindowAttributesExtX11, WindowType};
use winit::window::{Window, WindowId, WindowLevel};
use x11::X11;

/// From the feet to the middle of the body, in art pixels.
const BODY_CENTER: f32 = 7.0;
const STILL_FPS: f32 = 30.0;
const SLEEP_FPS: f32 = 12.0;
const DOUBLE_CLICK: Duration = Duration::from_millis(350);
/// Pointer must travel this far with the button held before it counts as a drag.
const DRAG_THRESHOLD_PX: f32 = 4.0;
const MAX_THROW: f32 = 4000.0;

struct Win {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    x11: Option<X11>,
    pos: PhysicalPosition<i32>,
    /// The edge the click-through region was last shaped for.
    shaped_for: Surface,
}

struct App {
    cfg: Config,
    sprites: Sprites,
    canvas: Pixmap,
    win: Option<Win>,
    body: Body,
    mind: Mind,
    anim: Animator,
    pose: Option<Pose>,
    bounds: Bounds,
    /// The whole monitor: where the mouse pointer can fly during a pointer game.
    screen: Bounds,
    params: Params,
    /// The mouse pointer while a pointer game has it.
    toy: Option<Toy>,
    feed: CursorFeed,
    track: CursorTrack,
    /// On Wayland the X server only knows the pointer while it is over an X11 window.
    wayland: bool,
    last_tick: Instant,
    next_frame: Instant,
    acc: f32,
    clock: f32,
    /// Pointer position inside our window, in window pixels.
    local: Option<V2>,
    hovered: bool,
    press: Option<V2>,
    /// While dragging: feet position minus pointer position at the moment of grabbing.
    grab: Option<V2>,
    last_click: Option<Instant>,
    /// MASCOT_DEBUG=1 logs every change of activity to stderr.
    debug: bool,
}

impl App {
    fn new(cfg: Config) -> Self {
        let now = Instant::now();
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let size = render::window_size(cfg.size);
        let s = size as f32 / render::GRID;
        App {
            sprites: Sprites::load(),
            canvas: Pixmap::new(size, size).unwrap(),
            win: None,
            body: Body::new(V2::default()),
            mind: Mind::new(&cfg, seed),
            anim: Animator::new(),
            pose: None,
            bounds: Bounds { left: 0.0, right: 1920.0, top: 0.0, bottom: 1080.0 },
            screen: Bounds { left: 0.0, right: 1919.0, top: 0.0, bottom: 1079.0 },
            params: Self::params(&cfg, s),
            toy: None,
            feed: CursorFeed::open(),
            track: CursorTrack::default(),
            wayland: std::env::var_os("WAYLAND_DISPLAY").is_some(),
            last_tick: now,
            next_frame: now,
            acc: 0.0,
            clock: 0.0,
            local: None,
            hovered: false,
            press: None,
            grab: None,
            last_click: None,
            debug: std::env::var_os("MASCOT_DEBUG").is_some(),
            cfg,
        }
    }

    /// Screen pixels per art pixel.
    fn scale(&self) -> f32 {
        self.canvas.width() as f32 / render::GRID
    }

    /// Where it may go, derived from how the sprite sits in its window on each edge, so the
    /// window stays on screen while it climbs.
    fn params(cfg: &Config, s: f32) -> Params {
        let (_, floor_y) = render::anchor(Surface::Floor);
        let (_, left_y) = render::anchor(Surface::LeftWall);
        let (_, right_y) = render::anchor(Surface::RightWall);
        let (_, hang_y) = render::anchor(Surface::Top);
        let edge = |y: f32| y.max(render::GRID - y);
        Params {
            gravity: cfg.gravity,
            bounce: cfg.bounce,
            half_w: 8.0 * s,
            foot_gap: (render::GRID - floor_y) * s,
            wall_top: edge(left_y).max(edge(right_y)) * s,
            wall_bottom: edge(left_y).max(edge(right_y)) * s,
            hang_drop: hang_y * s,
            center: BODY_CENTER * s,
        }
    }

    /// Top-left of the window that puts the sprite's feet on the body's position.
    fn window_origin(&self) -> V2 {
        let (ax, ay) = render::anchor(self.body.mount());
        self.body.pos - V2::new(ax, ay) * self.scale()
    }

    fn inside_body(&self, local: V2) -> bool {
        let s = self.scale();
        let r = render::hit_rect(self.body.mount());
        local.x >= r[0] * s && local.x <= r[2] * s && local.y >= r[1] * s && local.y <= r[3] * s
    }

    /// Pointer position on screen, from the best source available right now.
    fn pointer(&mut self, now: Instant) -> Option<V2> {
        let win = self.win.as_ref()?;
        let feed = self.feed.poll(now);
        // over our own window (or while it holds the pointer for a drag) the X server knows exactly
        let x11_live = !self.wayland || self.hovered || self.grab.is_some() || self.press.is_some();
        let from_x11 = win.x11.as_ref().filter(|_| x11_live).and_then(X11::pointer);
        let from_window = self.local.map(|l| V2::new(win.pos.x as f32, win.pos.y as f32) + l);
        from_x11.or(feed).or(from_window.filter(|_| self.hovered || self.grab.is_some()))
    }

    fn update(&mut self, now: Instant, dt: f32) {
        let s = self.scale();
        let cursor = self.pointer(now);
        self.track.update(cursor, dt);
        let away = if self.wayland {
            self.feed.away(now) // only the GNOME extension knows, and only it can move the pointer
        } else {
            self.win.as_mut().and_then(|w| w.x11.as_mut()).and_then(X11::away)
        };
        let (hx, hy) = self.pose.as_ref().map_or(render::anchor(Surface::Floor), |p| render::hand(p, &self.sprites));
        let hand = self.window_origin() + V2::new(hx * s, hy * s);

        let senses = Senses {
            pos: self.body.pos,
            vel: self.body.vel,
            grounded: self.body.grounded(),
            surface: self.body.surface,
            turning: self.body.turning(),
            bounds: &self.bounds,
            params: &self.params,
            cursor: self.track.pos,
            cursor_still: self.track.still,
            hovered: self.hovered,
            away,
            can_move_pointer: away.is_some(),
            toy: self.toy.as_ref().map(|t| t.pos),
            hand,
            scale: s,
        };
        let before = self.mind.act;
        let intent = self.mind.update(dt, &senses, &self.cfg);
        if self.debug && self.mind.act != before {
            eprintln!(
                "{:7.1}s {:>10} -> {:<10} at {:.0},{:.0} on {:?} energy {:.2}",
                self.clock, before.name(), self.mind.act.name(), self.body.pos.x, self.body.pos.y, self.body.surface, self.mind.energy
            );
        }
        if let Some(to) = intent.transfer {
            self.body.transfer(to, &self.params, &self.bounds);
        }
        if let Some(v) = intent.let_go {
            self.body.let_go(v, &self.params);
        }
        if let Some(v) = intent.jump {
            self.body.jump(v);
        }

        // the mouse pointer, while a pointer game has it, is a little physics toy
        match intent.pointer {
            Pointer::Leave => self.toy = None,
            p => {
                let toy = self.toy.get_or_insert_with(|| Toy::new(self.track.pos.unwrap_or(hand)));
                if let Pointer::Fling(v) = p {
                    toy.vel = v;
                }
            }
        }
        let grip = match intent.pointer {
            Pointer::Pull => Grip::Pull(hand),
            Pointer::Hold => Grip::Hold(hand),
            _ => Grip::Loose,
        };

        let held = self.grab.and_then(|off| self.track.pos.map(|c| c + off));
        let drive = Drive { along: intent.along, accel: intent.accel };
        self.acc += dt;
        while self.acc >= STEP {
            if let Some(toy) = &mut self.toy {
                toy.step(grip, &self.screen);
            }
            let impact = self.body.step(&self.params, &self.bounds, &drive, held);
            if impact > 60.0 {
                if self.debug {
                    eprintln!("{:7.1}s landed at {impact:.0} px/s", self.clock);
                }
                self.mind.landed(impact);
            }
            self.acc -= STEP;
        }

        let mount = self.body.mount();
        let head = self.body.pos + center_offset(mount, (BODY_CENTER + 1.0) * s);
        let ctx = Ctx {
            act: self.mind.act,
            t: self.mind.t,
            clock: self.clock,
            vel: self.body.local_vel(),
            grounded: self.body.surface.is_some() && !self.body.turning(),
            mount,
            turning: self.body.turning(),
            spinning: self.body.spinning,
            stride: self.body.stride,
            scale: s,
            look: intent.look.filter(|_| self.cfg.eye_tracking).map(|l| to_local(mount, l - head)),
        };
        self.pose = Some(self.anim.frame(ctx, &self.body, dt));

        let Some(win) = &mut self.win else { return };
        if let Some(toy) = &self.toy {
            if self.wayland {
                self.feed.move_pointer(toy.pos);
            } else if let Some(x) = &mut win.x11 {
                x.move_pointer(toy.pos);
            }
        }
        // the window is a camera that follows the body
        let top_left = self.window_origin();
        let Some(win) = &mut self.win else { return };
        let pos = PhysicalPosition::new(top_left.x.round() as i32, top_left.y.round() as i32);
        if pos != win.pos {
            win.window.set_outer_position(pos);
            win.pos = pos;
        }
        // only the character takes clicks; the region turns with it on the side edges
        if mount != win.shaped_for {
            win.shaped_for = mount;
            if let Some(x) = &win.x11 {
                let r = render::hit_rect(mount).map(|v| (v * s) as i32);
                x.set_input_rect(r[0], r[1], (r[2] - r[0]) as u32, (r[3] - r[1]) as u32);
            }
        }
    }

    fn draw(&mut self) {
        let (Some(win), Some(pose)) = (&mut self.win, &self.pose) else { return };
        render::render(pose, &self.sprites, &mut self.canvas);
        let Ok(mut buf) = win.surface.buffer_mut() else { return };
        // softbuffer wants 0xAARRGGBB; on a 32-bit visual the top byte is (premultiplied) alpha
        for (dst, px) in buf.iter_mut().zip(self.canvas.pixels()) {
            *dst = (px.alpha() as u32) << 24 | (px.red() as u32) << 16 | (px.green() as u32) << 8 | px.blue() as u32;
        }
        let _ = buf.present();
    }

    fn start_drag(&mut self, now: Instant) {
        self.press = None;
        if self.debug {
            eprintln!("{:7.1}s grabbed", self.clock);
        }
        if let Some(c) = self.pointer(now) {
            self.track.update(Some(c), 1.0 / 60.0);
            self.grab = Some(self.body.pos - c);
            self.mind.grab();
        }
    }

    fn release(&mut self) {
        if self.grab.take().is_some() {
            let v = self.track.vel;
            let v = if v.len() > MAX_THROW { v * (MAX_THROW / v.len()) } else { v };
            self.body.throw(v);
            self.mind.release(v.len());
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        let size = self.canvas.width();
        let attrs = Window::default_attributes()
            .with_title("Claude")
            .with_inner_size(PhysicalSize::new(size, size))
            .with_transparent(true)
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_x11_window_type(vec![WindowType::Utility]);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("claude-mascot: could not create window: {e}");
                return event_loop.exit();
            }
        };
        let x11 = X11::new(&window);

        // the playground: the chosen monitor (the primary one by default) minus panels and docks
        let s = self.scale();
        let (mut ax, mut ay, mut aw, mut ah) = (0, 0, 1920, 1080);
        let chosen = match self.cfg.monitor {
            0 => event_loop.primary_monitor(),
            n => event_loop.available_monitors().nth(n as usize - 1),
        };
        if let Some(m) = chosen.or_else(|| event_loop.primary_monitor()).or_else(|| event_loop.available_monitors().next()) {
            (ax, ay, aw, ah) = (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32);
        }
        self.screen = Bounds { left: ax as f32, right: (ax + aw - 1) as f32, top: ay as f32, bottom: (ay + ah - 1) as f32 };
        if let Some([wx, wy, ww, wh]) = x11.as_ref().and_then(X11::work_area) {
            let (l, t) = (ax.max(wx), ay.max(wy));
            let (r, b) = ((ax + aw).min(wx + ww), (ay + ah).min(wy + wh));
            if r - l > 200 && b - t > 200 {
                (ax, ay, aw, ah) = (l, t, r - l, b - t);
            }
        }
        self.bounds = Bounds { left: ax as f32, right: (ax + aw) as f32, top: ay as f32, bottom: (ay + ah) as f32 };
        self.body = Body::new(V2::new(self.bounds.right - 35.0 * s, self.bounds.bottom - self.params.foot_gap));
        let top_left = self.window_origin();
        let pos = PhysicalPosition::new(top_left.x.round() as i32, top_left.y.round() as i32);
        window.set_outer_position(pos);

        if let Some(x) = &x11 {
            x.set_hints();
            let r = render::hit_rect(Surface::Floor).map(|v| (v * s) as i32);
            x.set_input_rect(r[0], r[1], (r[2] - r[0]) as u32, (r[3] - r[1]) as u32);
        }
        window.set_visible(true);
        // winit only sends the always-on-top request to the window manager once the window is mapped
        window.set_window_level(WindowLevel::AlwaysOnTop);

        let surface = softbuffer::Context::new(window.clone()).and_then(|ctx| softbuffer::Surface::new(&ctx, window.clone()));
        let mut surface = match surface {
            Ok(s) => s,
            Err(e) => {
                eprintln!("claude-mascot: could not create drawing surface: {e}");
                return event_loop.exit();
            }
        };
        let _ = surface.resize(NonZeroU32::new(size).unwrap(), NonZeroU32::new(size).unwrap());
        self.win = Some(Win { window, surface, x11, pos, shaped_for: Surface::Floor });
        self.last_tick = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let now = Instant::now();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Moved(pos) => {
                if let Some(win) = &mut self.win {
                    win.pos = pos;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let local = V2::new(position.x as f32, position.y as f32);
                self.local = Some(local);
                self.hovered = self.grab.is_some() || self.inside_body(local);
                if self.press.is_some_and(|p| (local - p).len() > DRAG_THRESHOLD_PX) {
                    self.start_drag(now);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if self.grab.is_none() {
                    self.hovered = false;
                    self.local = None;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let on_body = self.local.is_some_and(|l| self.inside_body(l));
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) if on_body => self.press = self.local,
                    (MouseButton::Left, ElementState::Released) => {
                        self.release();
                        if self.press.take().is_some() {
                            if self.last_click.is_some_and(|c| now - c < DOUBLE_CLICK) {
                                self.mind.double_click();
                                self.last_click = None;
                            } else {
                                self.mind.click();
                                self.last_click = Some(now);
                            }
                        }
                    }
                    (MouseButton::Right, ElementState::Pressed) if on_body => self.mind.cycle(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_none() {
            return;
        }
        let now = Instant::now();
        if now >= self.next_frame {
            // clamp so a stall (suspend, heavy load) never fires the body across the screen
            let dt = (now - self.last_tick).as_secs_f32().min(0.1);
            self.last_tick = now;
            self.clock += dt;
            self.update(now, dt);
            let fps = if self.body.is_moving() || self.grab.is_some() {
                self.cfg.fps
            } else if self.mind.act == Act::Sleep {
                SLEEP_FPS
            } else {
                STILL_FPS
            };
            self.next_frame = now + Duration::from_secs_f32(1.0 / fps);
            if let Some(win) = &self.win {
                win.window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}

/// Renders every animation to `<dir>/<name>.png`, a horizontal strip of frames at 15 fps.
fn export_frames(dir: &str, cfg: &Config) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let sprites = Sprites::load();
    let size = render::window_size(cfg.size);
    let s = size as f32 / render::GRID;
    let mut canvas = Pixmap::new(size, size).unwrap();
    let body = Body::new(V2::default());
    let (walk, climb) = (V2::new(cfg.walk_speed, 0.0), V2::new(-0.75 * cfg.walk_speed, 0.0));
    // (name, act, edge, velocity in its own frame, attached, rounding a corner)
    let mut shots: Vec<(&str, Act, Surface, V2, bool, bool)> = Act::CYCLE
        .into_iter()
        .chain(Act::OTHERS)
        .filter(|a| !matches!(a, Act::Climb | Act::Slip))
        .map(|act| {
            let (vel, grounded) = match act {
                Act::Walk | Act::Follow | Act::Bat => (walk, true),
                Act::Sneak => (walk * 0.5, true),
                Act::Run | Act::Chase => (V2::new(cfg.run_speed, 0.0), true),
                Act::Fly => (V2::new(150.0, 700.0), false),
                Act::Held => (V2::default(), false),
                _ => (V2::default(), true),
            };
            (act.name(), act, Surface::Floor, vel, grounded, false)
        })
        .collect();
    shots.extend([
        ("climbing", Act::Climb, Surface::LeftWall, climb, true, false),
        ("clinging", Act::Climb, Surface::RightWall, V2::default(), true, false),
        ("hanging", Act::Climb, Surface::Top, V2::default(), true, false),
        ("shimmying", Act::Climb, Surface::Top, walk * 0.6, true, false),
        ("cornering", Act::Climb, Surface::LeftWall, V2::default(), false, true),
        ("slipping", Act::Slip, Surface::RightWall, V2::default(), true, false),
    ]);
    for (name, act, mount, vel, grounded, turning) in shots {
        let look = matches!(act, Act::Curious | Act::Follow | Act::Chase | Act::Notice | Act::Love | Act::Tug | Act::Sneak).then_some(V2::new(80.0, -90.0));
        let secs = match act {
            Act::Idle => 6.6,
            Act::Sleep => 7.2,
            Act::Look => 5.2,
            Act::Think | Act::Love => 4.8,
            Act::Notice => 1.0,
            Act::Surprised => 1.2,
            _ => 3.6,
        };
        let n = (secs * 15.0) as u32;
        let mut strip = Pixmap::new(size * n, size).unwrap();
        let mut anim = Animator::new();
        for i in 0..n {
            let t = i as f32 / 15.0;
            let ctx = Ctx { act, t, clock: t, vel, grounded, spinning: false, mount, turning, stride: vel.x.abs() * t, scale: s, look };
            render::render(&anim.frame(ctx, &body, 1.0 / 15.0), &sprites, &mut canvas);
            strip.draw_pixmap((i * size) as i32, 0, canvas.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        let path = format!("{dir}/{name}.png");
        strip.save_png(&path).map_err(std::io::Error::other)?;
        println!("{path}: {n} frames");
    }
    Ok(())
}

fn main() {
    let cfg = Config::load();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--export-frames") => {
            let dir = args.get(1).map_or("frames", String::as_str);
            if let Err(e) = export_frames(dir, &cfg) {
                eprintln!("claude-mascot: export failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        Some("--print-config") => {
            print!("{}", Config::default().to_text());
            return;
        }
        Some(_) => {
            println!("usage: claude-mascot                         run the desktop mascot");
            println!("       claude-mascot --print-config          print every setting with its default");
            println!("       claude-mascot --export-frames [dir]   render every animation to PNG strips");
            if let Some(path) = Config::path() {
                println!("settings file: {}", path.display());
            }
            return;
        }
        None => {}
    }

    let event_loop = match EventLoop::builder().with_x11().build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("claude-mascot: cannot connect to X11/Xwayland ({e}). Is DISPLAY set?");
            std::process::exit(1);
        }
    };
    println!("Claude mascot is running. Ctrl+C (or closing this terminal) stops it.");
    if let Err(e) = event_loop.run_app(&mut App::new(cfg)) {
        eprintln!("claude-mascot: {e}");
        std::process::exit(1);
    }
}
