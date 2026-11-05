//! Built-in presets and shareable preset files.
//!
//! A preset file is JSON: `{ "name": ..., "settings": { ...only values that differ... } }`. Reading
//! also accepts upstream preset files, which allow `//` comments and hold other top-level fields.
//! Unknown or invalid settings are skipped and reported, so old presets survive new versions.

use crate::rules::{self, Rules};
use modinfo::Settings;
use serde_json::{json, Map, Value};

#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    pub description: String,
    pub overrides: Map<String, Value>,
    /// True for presets that ship inside the app, which cannot be replaced.
    pub built_in: bool,
}

const BUILT_IN: [(&str, &str, &str); 5] = [
    ("Standard", "The randomizer's own defaults.", "{}"),
    (
        "Casual",
        "Friendly first seed: helpful starting items, clear chests, and no minigames.",
        r#"{"start_with_merge":true,"start_with_pouch":true,"bell_in_shop":true,"boots_in_shop":true,"assured_weapon":false,
            "chest_size_matches_contents":true,"change_freestanding_models":true,"minigames_excluded":true,
            "skip_big_bomb_flower":true,"purple_potion_bottles":true,"hint_ghosts":"Always","weather_vanes":"All",
            "treacherous_tower_floors":5}"#,
    ),
    (
        "Open World",
        "Cracks and Weather Vanes start open, so you can go almost anywhere early.",
        r#"{"cracks":"Open","weather_vanes":"All","start_with_merge":true,"bell_in_shop":true,"skip_big_bomb_flower":true,
            "chest_size_matches_contents":true}"#,
    ),
    (
        "Crack Shuffle",
        "Every crack leads somewhere new. Expect to get lost, in a good way.",
        r#"{"cracksanity":"CrossWorldPairs","weather_vanes":"Shuffled","chest_size_matches_contents":true}"#,
    ),
    (
        "Expert",
        "Hard logic, no Lamp needed for dark rooms, and a tall Treacherous Tower.",
        r#"{"logic_mode":"Hard","dark_rooms_lampless":true,"lamp_and_net_as_weapons":true,"treacherous_tower_floors":66,
            "chest_size_matches_contents":false}"#,
    ),
];

pub fn built_in() -> Vec<Preset> {
    BUILT_IN
        .iter()
        .map(|(name, description, values)| Preset {
            name: (*name).to_owned(),
            description: (*description).to_owned(),
            overrides: serde_json::from_str(values).expect("built-in presets are valid JSON"),
            built_in: true,
        })
        .collect()
}

/// Removes `//` comments, leaving text inside strings alone.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut in_string, mut escaped) = (false, false);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            match ch {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if ch == '/' && chars.peek() == Some(&'/') {
            for skipped in chars.by_ref() {
                if skipped == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else {
            in_string = ch == '"';
            out.push(ch);
        }
    }
    out
}

/// Removes commas left in front of a closing bracket, which upstream files can have.
fn strip_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_string {
            match ch {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == '}' || ch == ']' {
            let kept = out.trim_end().len();
            if out[..kept].ends_with(',') {
                out.truncate(kept - 1);
            }
        }
        out.push(ch);
    }
    out
}

/// Reads a preset file. `fallback_name` names presets whose file gives no name. Returns the preset
/// and the settings it had to skip.
pub fn parse(text: &str, fallback_name: &str) -> Result<(Preset, Vec<String>), String> {
    let clean = strip_trailing_commas(&strip_comments(text));
    let root: Value = serde_json::from_str(&clean).map_err(|err| format!("Not a valid preset file: {err}"))?;
    let Value::Object(mut root) = root else {
        return Err("Not a valid preset file: expected { } at the top level.".to_owned());
    };
    let name = root.get("name").and_then(Value::as_str).unwrap_or(fallback_name).to_owned();
    let description = root.get("description").and_then(Value::as_str).unwrap_or("A preset file on your SD card.").to_owned();
    let values = match root.remove("settings") {
        Some(Value::Object(settings)) => settings,
        _ => root,
    };

    let mut overrides = Map::new();
    let mut skipped = Vec::new();
    for (key, value) in values {
        let known = rules::option_index(&key).is_some() || rules::HIDDEN.contains(&key.as_str());
        let probe = json!({ key.clone(): value.clone() });
        if known && serde_json::from_value::<Settings>(probe).is_ok() {
            overrides.insert(key, value);
        } else if !matches!(key.as_str(), "name" | "description" | "seed" | "version") {
            skipped.push(key);
        }
    }
    Ok((Preset { name, description, overrides, built_in: false }, skipped))
}

/// The text to save for a preset file.
pub fn to_file_text(name: &str, rules: &Rules) -> String {
    let file = json!({
        "name": name,
        "description": "Made on a 3DS with the ALBW Randomizer app.",
        "settings": Value::Object(rules.overrides()),
    });
    serde_json::to_string_pretty(&file).expect("JSON values always serialize")
}

/// A safe file name for a preset. Returns `None` if the name has no usable characters.
pub fn file_name(name: &str) -> Option<String> {
    let safe: String = name
        .trim()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' })
        .take(40)
        .collect();
    safe.chars().any(|ch| ch.is_ascii_alphanumeric()).then(|| format!("{safe}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_presets_use_only_real_settings() {
        for preset in built_in() {
            for key in preset.overrides.keys() {
                assert!(rules::option_index(key).is_some(), "{}: {key}", preset.name);
            }
            Rules::with_overrides(&preset.overrides).settings();
        }
    }

    #[test]
    fn built_in_presets_generate_a_seed() {
        for preset in built_in() {
            // Some seed numbers cannot work with shuffled cracks, and the app retries those too.
            let worked = (1..=crate::MAX_SEED_ATTEMPTS).filter(|seed| {
                randomizer::build_seed(*seed, Rules::with_overrides(&preset.overrides).settings()).is_ok()
            });
            assert!(worked.count() >= 2, "{} rarely generates", preset.name);
        }
    }

    #[test]
    fn saved_preset_reads_back_the_same() {
        let mut rules = Rules::default();
        rules.step(&rules::OPTIONS[rules::option_index("logic_mode").unwrap()], 1);
        let (preset, skipped) = parse(&to_file_text("My Rules", &rules), "fallback").unwrap();
        assert_eq!(preset.name, "My Rules");
        assert!(skipped.is_empty());
        assert_eq!(Rules::with_overrides(&preset.overrides).overrides(), rules.overrides());
    }

    #[test]
    fn upstream_example_file_loads_and_reports_old_settings() {
        let text = include_str!("../../../vendor/z17-randomizer/presets/Example.json");
        let (preset, skipped) = parse(text, "Example").unwrap();
        assert_eq!(preset.name, "Example");
        assert_eq!(preset.overrides.get("logic_mode"), Some(&json!("Normal")));
        // That file still uses an older format for this one.
        assert!(skipped.contains(&"super_items".to_owned()), "{skipped:?}");
        Rules::with_overrides(&preset.overrides).settings();
    }

    #[test]
    fn broken_and_unknown_values_do_not_stop_a_preset() {
        let (preset, skipped) = parse(r#"{"settings":{"logic_mode":"Hard","future_option":1,"keysy":"Nonsense"}}"#, "x").unwrap();
        assert_eq!(preset.overrides.len(), 1);
        assert_eq!(skipped.len(), 2);
        assert!(parse("not json", "x").is_err());
    }

    #[test]
    fn comment_markers_inside_strings_survive() {
        let (preset, _) = parse("{\"name\":\"a//b\", // real comment\n \"settings\":{}}", "x").unwrap();
        assert_eq!(preset.name, "a//b");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("My Rules!").as_deref(), Some("My_Rules_.json"));
        assert_eq!(file_name("../../boot"), Some("______boot.json".to_owned()));
        assert_eq!(file_name("  ///  "), None);
    }
}
