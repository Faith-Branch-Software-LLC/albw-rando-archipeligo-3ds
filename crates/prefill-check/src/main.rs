//! Prints what `randomize_pre_fill` makes for a fixed set of seeds and settings, one line each.
//!
//! Archipelago patching re-runs `randomize_pre_fill` on the console with the seed that the host's
//! computer used, so its output must not depend on the pointer width. Run this on a 64-bit host and
//! on a 32-bit target, then compare the two outputs line by line. Pass `--fill` to also build
//! whole seeds.

use modinfo::settings::{cracksanity::Cracksanity, weather_vanes::WeatherVanes};
use modinfo::Settings;

fn settings_sets() -> Vec<(&'static str, Settings)> {
    // `Settings::default()` fails validation, so start from the app's default rules.
    let plain: Settings = ui_core::rules::Rules::default().settings();
    let mut shuffled = plain.clone();
    shuffled.cracksanity = Cracksanity::AnyWorldPairs;
    shuffled.weather_vanes = WeatherVanes::Shuffled;
    shuffled.treacherous_tower_floors = 20;
    shuffled.maiamai_madness = true;
    let mut mirrored = plain.clone();
    mirrored.cracksanity = Cracksanity::MirroredCrossWorldPairs;
    mirrored.treacherous_tower_floors = 66;
    vec![("plain", plain), ("shuffled", shuffled), ("mirrored", mirrored)]
}

fn main() {
    println!("pointer width {}", usize::BITS);
    for (name, settings) in settings_sets() {
        for seed in [0u32, 1, 2, 1234, 99999, 0x7fff_ffff, u32::MAX] {
            let info = randomizer::randomize_pre_fill(seed, settings.clone(), None);
            let mut json = serde_json::to_value(&info).expect("SeedInfo serializes");
            // The version and settings are inputs, not results.
            if let Some(map) = json.as_object_mut() {
                map.remove("settings");
                map.remove("version");
            }
            println!("{name} {seed} {json} text={:?}", info.text);
        }
    }
    // A full fill also walks the item pools and hints, so check a few whole seeds too.
    if std::env::args().any(|arg| arg == "--fill") {
        for (name, settings) in settings_sets() {
            for seed in [1u32, 1234] {
                // Some settings fail to fill for some seeds. The failure must match too.
                let info = match randomizer::build_seed(seed, settings.clone()) {
                    Ok(info) => info,
                    Err(err) => {
                        println!("fill-{name} {seed} {{\"error\":{:?}}} text=", format!("{err:?}"));
                        continue;
                    }
                };
                let mut json = serde_json::to_value(&info).expect("SeedInfo serializes");
                if let Some(map) = json.as_object_mut() {
                    map.remove("settings");
                    map.remove("version");
                }
                println!("fill-{name} {seed} {json} text={:?}", info.text);
            }
        }
    }
}
