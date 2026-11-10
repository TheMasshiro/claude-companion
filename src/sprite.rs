//! Pixel-art sprites, stored as text grids in `assets/pixel/*.txt` and baked into the binary.

use std::collections::HashMap;

/// The whole palette. One character per pixel; `.` is transparent.
pub const PALETTE: &[(char, u32)] = &[
    ('o', 0x3B1E14), // outline and ink
    ('#', 0xD97757), // body, Claude terracotta
    ('h', 0xF0A587), // highlight (light comes from the top left)
    ('s', 0xB5583A), // shade
    ('w', 0xFFF4EC), // white
    ('c', 0xFFE7D6), // spark cream
    ('m', 0x7A2E22), // inside of the mouth
    ('p', 0xF2899A), // blush, tongue
    ('r', 0xE8475F), // heart
    ('y', 0xFFD45C), // gold
    ('b', 0x9FB4FF), // sleepy blue
    ('g', 0xC9CDD2), // light gray
    ('d', 0x4A4F57), // dark gray
];

pub struct Sprite {
    pub w: i32,
    pub h: i32,
    /// Row-major colors; `None` is transparent.
    pub px: Vec<Option<u32>>,
}

impl Sprite {
    pub fn at(&self, x: i32, y: i32) -> Option<u32> {
        self.px[(y * self.w + x) as usize]
    }
}

macro_rules! sprite_files {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../assets/pixel/", $name, ".txt")))),*]
    };
}

/// To add a sprite: drop `assets/pixel/<name>.txt` in place and list it here.
const SOURCES: &[(&str, &str)] = sprite_files![
    "body", "body_squash", "body_stretch", "foot", "paw", "paw_grip",
    "eye_open", "eye_wide", "eye_squint", "eye_blink", "eye_happy", "eye_sleepy", "eye_dizzy_a", "eye_dizzy_b",
    "mouth_smile", "mouth_o", "mouth_open", "mouth_yawn", "blush", "tuft_spark",
    "heart", "heart_small", "spark", "spark_small", "z", "z_small", "exclaim", "question",
    "bubble", "puff", "puff_small", "dust", "laptop", "sweat",
];

pub struct Sprites(HashMap<&'static str, Sprite>);

impl Sprites {
    pub fn load() -> Self {
        Sprites(SOURCES.iter().map(|&(name, src)| (name, parse(name, src))).collect())
    }

    pub fn get(&self, name: &str) -> &Sprite {
        self.0.get(name).unwrap_or_else(|| panic!("unknown sprite {name:?}"))
    }
}

fn parse(name: &str, src: &str) -> Sprite {
    let rows: Vec<&str> = src.lines().map(str::trim_end).filter(|l| !l.is_empty()).collect();
    let w = rows.first().map_or(0, |r| r.chars().count());
    assert!(w > 0, "sprite {name}: empty");
    let mut px = Vec::with_capacity(w * rows.len());
    for (y, row) in rows.iter().enumerate() {
        assert_eq!(row.chars().count(), w, "sprite {name}: row {y} width differs from row 0");
        for (x, c) in row.chars().enumerate() {
            px.push(match c {
                '.' => None,
                _ => Some(
                    PALETTE
                        .iter()
                        .find(|(k, _)| *k == c)
                        .unwrap_or_else(|| panic!("sprite {name}: unknown color {c:?} at {x},{y}"))
                        .1,
                ),
            });
        }
    }
    Sprite { w: w as i32, h: rows.len() as i32, px }
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_sprites_parse() {
        super::Sprites::load();
    }
}
