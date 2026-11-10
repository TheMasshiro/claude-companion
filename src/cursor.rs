//! Where the mouse pointer is, how long you have been away, and (for pointer games) a way
//! to move the pointer.
//!
//! On X11 the X server provides all of that (see x11.rs). On a Wayland session (GNOME), X11
//! apps only see the pointer while it is over an X11 window and may not move it at all, so
//! the GNOME Shell extension in `gnome-extension/` does it from inside the desktop: it streams
//! "x y away_ms" to the socket `CursorFeed` listens on, and moves the pointer when asked on its
//! own socket. It refuses to move it unless you have been away for at least 10 seconds, and it
//! never clicks or types.

use crate::physics::V2;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The extension sends on every move plus a heartbeat about once a second.
const FEED_TIMEOUT: Duration = Duration::from_secs(3);

pub struct CursorFeed {
    sock: Option<UnixDatagram>,
    path: PathBuf,
    pos: Option<V2>,
    /// Seconds since your last keyboard or mouse input, as of `last_msg`.
    away: Option<f32>,
    last_msg: Option<Instant>,
}

impl CursorFeed {
    fn runtime_dir() -> PathBuf {
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from)
    }

    pub fn path() -> PathBuf {
        Self::runtime_dir().join("claude-mascot-cursor.sock")
    }

    /// Where the extension listens for "move x y".
    fn control_path() -> PathBuf {
        Self::runtime_dir().join("claude-mascot-control.sock")
    }

    pub fn open() -> Self {
        let path = Self::path();
        let _ = std::fs::remove_file(&path); // left over from a run that was killed
        let sock = UnixDatagram::bind(&path).and_then(|s| s.set_nonblocking(true).map(|_| s)).ok();
        CursorFeed { sock, path, pos: None, away: None, last_msg: None }
    }

    /// Latest pointer position from the extension, if it has spoken recently.
    pub fn poll(&mut self, now: Instant) -> Option<V2> {
        let mut buf = [0u8; 64];
        while let Some(Ok(n)) = self.sock.as_ref().map(|s| s.recv(&mut buf)) {
            // "x y away_ms" in screen pixels and milliseconds; anything else is ignored
            let text = std::str::from_utf8(&buf[..n]).unwrap_or("");
            let mut nums = text.split_whitespace().map(str::parse::<f32>);
            if let (Some(Ok(x)), Some(Ok(y))) = (nums.next(), nums.next()) {
                if x.is_finite() && y.is_finite() {
                    self.pos = Some(V2::new(x, y));
                    self.away = nums.next().and_then(Result::ok).filter(|a| a.is_finite() && *a >= 0.0).map(|ms| ms / 1000.0);
                    self.last_msg = Some(now);
                }
            }
        }
        self.pos.filter(|_| self.fresh(now))
    }

    fn fresh(&self, now: Instant) -> bool {
        self.last_msg.is_some_and(|t| now - t < FEED_TIMEOUT)
    }

    /// Seconds since your last keyboard or mouse input, if the extension reports it.
    /// Also tells whether the pointer can be moved: only the extension can do that on Wayland.
    pub fn away(&self, now: Instant) -> Option<f32> {
        // count the time since the message, but not for long: a silent feed means "unknown"
        let since = self.last_msg.map(|t| (now - t).as_secs_f32())?;
        self.away.filter(|_| since < 1.5).map(|a| a + since)
    }

    /// Asks the extension to put the pointer at `p`. It ignores this unless you are away.
    pub fn move_pointer(&self, p: V2) {
        if let Some(sock) = &self.sock {
            let _ = sock.send_to(format!("move {:.0} {:.0}", p.x, p.y).as_bytes(), Self::control_path());
        }
    }
}

impl Drop for CursorFeed {
    fn drop(&mut self) {
        if self.sock.is_some() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Smooths raw pointer samples into a position, a velocity and how long it has been still.
#[derive(Default)]
pub struct CursorTrack {
    pub pos: Option<V2>,
    pub vel: V2,
    /// Seconds since the pointer last moved.
    pub still: f32,
}

impl CursorTrack {
    pub fn update(&mut self, sample: Option<V2>, dt: f32) {
        match (self.pos, sample) {
            (Some(prev), Some(now)) => {
                let moved = now - prev;
                let k = 1.0 - (-20.0 * dt).exp();
                self.vel += (moved * (1.0 / dt.max(1e-3)) - self.vel) * k;
                self.still = if moved.len() > 0.5 { 0.0 } else { self.still + dt };
            }
            _ => {
                self.vel = V2::default();
                self.still += dt;
            }
        }
        self.pos = sample;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_measures_speed_and_stillness() {
        let mut t = CursorTrack::default();
        for i in 0..60 {
            t.update(Some(V2::new(i as f32 * 10.0, 0.0)), 1.0 / 60.0);
        }
        assert!((t.vel.x - 600.0).abs() < 5.0 && t.still == 0.0);
        for _ in 0..60 {
            t.update(t.pos, 1.0 / 60.0);
        }
        assert!(t.vel.x.abs() < 1.0 && t.still > 0.9);
    }
}
