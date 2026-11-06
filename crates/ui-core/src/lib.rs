//! Menu logic for the app. Nothing here touches the console, so all of it is tested on a computer.

pub mod display;
pub mod flim;
pub mod icons;
pub mod menu;
pub mod modmoon;
pub mod picture;
pub mod presets;
pub mod rules;
pub mod sarc;

/// How many seed numbers to try when one cannot produce a working game.
pub const MAX_SEED_ATTEMPTS: u32 = 10;
