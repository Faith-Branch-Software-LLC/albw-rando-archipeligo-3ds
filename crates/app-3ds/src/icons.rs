//! Loads the game's icons for the menu. They are saved to the SD card the first time, because they
//! come from the player's own game and cannot ship with the app.

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;

use ui_core::{flim, icons::SOURCES, sarc};

use crate::{draw, job, model_view};

const CACHE: &str = "sdmc:/3ds/albw-rando/cache/icons";

/// The icons that loaded. A missing icon only leaves its row without a picture.
pub struct Icons(HashMap<&'static str, *mut c_void>);

impl Icons {
    pub fn get(&self, name: &str) -> Option<*mut c_void> {
        self.0.get(name).copied()
    }
}

fn cache_path(name: &str) -> String {
    format!("{CACHE}/{name}.bflim")
}

/// Copies any icons not yet on the SD card out of the game. Returns a note for the log.
fn fill_cache(check: job::GameCheck) -> String {
    let mut needed: Vec<&str> = SOURCES
        .iter()
        .filter(|(_, names)| names.iter().any(|name| !std::path::Path::new(&cache_path(name)).exists()))
        .map(|(archive, _)| *archive)
        .collect();
    // The model preview also needs the game's own model, which this read can fetch at the same time.
    let wants_model = model_view::needs_vanilla();
    if wants_model {
        needed.push(model_view::ARCHIVE);
    }
    if needed.is_empty() {
        job::finish_check(check);
        return "Icons were already on the SD card.".to_owned();
    }
    if let Err(err) = fs::create_dir_all(CACHE) {
        job::finish_check(check);
        return format!("Could not make the icon folder: {err}");
    }
    let archives = match job::game_archives(check, &needed) {
        Ok(archives) => archives,
        Err(err) => return format!("Could not read icons from the game: {err}"),
    };
    let mut copied = 0;
    let mut missing = Vec::new();
    let mut model_note = String::new();
    for (archive, data) in needed.iter().zip(&archives) {
        if wants_model && *archive == model_view::ARCHIVE {
            model_note = format!(" {}", model_view::cache_vanilla(data));
            continue;
        }
        let names = SOURCES.iter().find(|(name, _)| name == archive).map_or(&[][..], |(_, names)| *names);
        for name in names {
            match sarc::find(data, &format!("timg/{name}.bflim")) {
                Some(file) if fs::write(cache_path(name), file).is_ok() => copied += 1,
                _ => missing.push(*name),
            }
        }
    }
    format!("Copied {copied} icons from the game. Missing: {missing:?}{model_note}")
}

/// Loads the icons into GPU memory, reading the game only if the SD card lacks them.
pub fn load(check: job::GameCheck) -> (Icons, String) {
    let note = fill_cache(check);
    let mut icons = HashMap::new();
    let mut bad = Vec::new();
    for name in SOURCES.iter().flat_map(|(_, names)| names.iter().copied()) {
        let image = fs::read(cache_path(name)).ok().and_then(|data| flim::parse(&data).map(|flim| draw::load_image(&flim)));
        match image {
            Some(image) if !image.is_null() => {
                icons.insert(name, image);
            }
            _ => bad.push(name),
        }
    }
    let note = format!("{note} Loaded {} icons. Failed: {bad:?}", icons.len());
    (Icons(icons), note)
}
