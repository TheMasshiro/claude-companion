//! User settings, read from `~/.config/claude-mascot/config.toml` (plain `key = value` lines).
//! `claude-mascot --print-config` prints every setting with its default.

use std::path::PathBuf;

macro_rules! settings {
    ($($(#[doc = $doc:literal])+ $name:ident: $ty:ty = $default:expr;)*) => {
        pub struct Config { $(pub $name: $ty,)* }

        impl Default for Config {
            fn default() -> Self {
                Config { $($name: $default,)* }
            }
        }

        impl Config {
            fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
                match key {
                    $(stringify!($name) => {
                        self.$name = value.parse().map_err(|_| format!("{value:?} is not a valid value for {key}"))?
                    })*
                    _ => return Err(format!("unknown setting {key:?}")),
                }
                Ok(())
            }

            pub fn to_text(&self) -> String {
                let mut out = String::new();
                $(
                    $(out.push_str(&format!("#{}\n", $doc));)+
                    out.push_str(&format!("{} = {}\n\n", stringify!($name), self.$name));
                )*
                out
            }
        }
    };
}

settings! {
    /// Size of the mascot's window in pixels, 50 to 400. Rounded to a multiple of 25 so every
    /// art pixel is a whole number of screen pixels (100 = 4x4 screen pixels per art pixel).
    size: u32 = 100;
    /// Frame rate while moving. Standing still runs at 30, sleeping at 12.
    fps: f32 = 60.0;
    /// Walk and run around the screen on its own.
    roam: bool = true;
    /// Approach, follow, chase and inspect the mouse pointer. Needs the pointer position (see README).
    cursor_play: bool = true;
    /// Eyes follow the mouse pointer.
    eye_tracking: bool = true;
    /// How busy it is, 0.1 to 3. Higher means shorter idle and sitting spells.
    activity: f32 = 1.0;
    /// 0 to 1. How drawn it is to the mouse pointer.
    curiosity: f32 = 0.6;
    /// 0 to 1. How often it runs, hops and chases.
    playfulness: f32 = 0.6;
    /// Walking speed in pixels per second.
    walk_speed: f32 = 45.0;
    /// Running speed in pixels per second.
    run_speed: f32 = 210.0;
    /// Gravity in pixels per second squared.
    gravity: f32 = 2200.0;
    /// Bounciness against the floor and screen edges, 0 to 0.9.
    bounce: f32 = 0.42;
    /// Seconds without any interaction before it falls asleep.
    sleep_after: f32 = 180.0;
    /// Now and then go on an outing up the side edges of the screen and along the top edge.
    climbing: bool = true;
    /// Which monitor it lives on: 0 = the primary one, 1, 2, ... = monitors in the order the system lists them.
    monitor: u32 = 0;
    /// Play with the mouse pointer while you are away: sneak up on it, reel it in, carry it, throw it
    /// and bat it around. Any key press or mouse movement hands it straight back.
    cursor_mischief: bool = true;
    /// Seconds without keyboard or mouse input before you count as away (at least 10).
    afk_after: f32 = 120.0;
    /// Average seconds between pointer games while you are away. Each wait is randomized.
    mischief_every: f32 = 90.0;
    /// Stop pointer games after you have been away this many more seconds, so the screen
    /// still blanks and locks as usual (moving the pointer counts as activity to the desktop).
    mischief_for: f32 = 600.0;
}

/// Like `clamp`, but a NaN typed into the file becomes `lo` instead of poisoning the physics.
fn within(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_nan() { lo } else { v.clamp(lo, hi) }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("claude-mascot").join("config.toml"))
    }

    /// Defaults, overridden by the config file when there is one. Bad lines are reported and skipped.
    pub fn load() -> Config {
        let mut cfg = Config::default();
        if let Some(path) = Self::path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                for err in cfg.apply(&text) {
                    eprintln!("claude-mascot: {}: {err}", path.display());
                }
            }
        }
        cfg
    }

    /// Applies `key = value` lines (`#` starts a comment) and returns one message per bad line.
    pub fn apply(&mut self, text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                errors.push(format!("line {}: expected `key = value`", n + 1));
                continue;
            };
            if let Err(e) = self.set(key.trim(), value.trim().trim_matches('"')) {
                errors.push(format!("line {}: {e}", n + 1));
            }
        }
        self.size = self.size.clamp(50, 400);
        self.fps = within(self.fps, 10.0, 240.0);
        self.activity = within(self.activity, 0.1, 3.0);
        self.curiosity = within(self.curiosity, 0.0, 1.0);
        self.playfulness = within(self.playfulness, 0.0, 1.0);
        self.walk_speed = within(self.walk_speed, 10.0, 1000.0);
        self.run_speed = within(self.run_speed, 10.0, 3000.0);
        self.gravity = within(self.gravity, 100.0, 20000.0);
        self.bounce = within(self.bounce, 0.0, 0.9);
        self.sleep_after = within(self.sleep_after, 10.0, f32::MAX);
        self.afk_after = within(self.afk_after, 10.0, f32::MAX);
        self.mischief_every = within(self.mischief_every, 5.0, f32::MAX);
        self.mischief_for = within(self.mischief_for, 0.0, 3600.0);
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn parses_reports_and_clamps() {
        let mut c = Config::default();
        let errs = c.apply("# comment\nroam = false\nrun_speed = 500 # fast\nbounce = 7\nnope = 1\nsize = x\ngarbage\n");
        assert!(!c.roam && c.run_speed == 500.0 && c.bounce == 0.9 && c.size == 100);
        assert_eq!(errs.len(), 3, "{errs:?}");
    }

    #[test]
    fn printed_defaults_read_back_cleanly() {
        let mut c = Config::default();
        assert!(c.apply(&Config::default().to_text()).is_empty());
    }
}
