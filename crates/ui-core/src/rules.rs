//! The table of settings the menus show, and the current values.
//!
//! Values are kept as JSON under the same keys the randomizer's `Settings` uses. That keeps one
//! table as the only list of options, and matches the preset file format.

use modinfo::Settings;
use serde_json::{json, Map, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Toggle,
    /// Pairs of the stored value and the label to show.
    Choice(&'static [(&'static str, &'static str)]),
    Number { min: i64, max: i64, step: i64 },
}

#[derive(Debug)]
pub struct OptionDef {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub category: usize,
    pub kind: Kind,
}

pub const CATEGORIES: [(&str, &str); 6] = [
    ("Logic", "What the seed expects you to know, and what ends the game."),
    ("World", "How cracks, Weather Vanes, and the Trials Door behave."),
    ("Items", "Which items are shuffled, and how keys and Maiamai work."),
    ("Start and Shop", "What you begin with, and what Ravio is sure to sell."),
    ("Removed Checks", "Minigames and obstacles that the seed can skip."),
    ("Comfort", "Hints and visual changes. These do not change item logic."),
];

const fn option(key: &'static str, label: &'static str, category: usize, kind: Kind, help: &'static str) -> OptionDef {
    OptionDef { key, label, help, category, kind }
}

/// Settings with no menu entry. A test checks that every other field is in `OPTIONS`.
pub const HIDDEN: [&str; 5] = [
    "dev_mode",
    // The randomizer rejects a value that differs from `lc_requirement`, so it follows that option.
    "yuganon_requirement",
    "ravios_shop",
    // Conflicts with `bow_of_light_in_castle`, and the upstream example file does not offer it.
    "progressive_bow_of_light",
    // Needs a list screen of about 300 locations. Preset files can still set it.
    "user_exclusions",
];

pub const OPTIONS: &[OptionDef] = &[
    option("logic_mode", "Logic", 0, Kind::Choice(&[("Normal", "Normal"), ("Hard", "Hard"), ("Glitched", "Glitched"), ("AdvGlitched", "Adv. Glitched"), ("Hell", "Hell"), ("NoLogic", "No Logic")]), "The tricks the seed may require. No Logic can make a seed impossible."),
    option("lc_requirement", "Portraits for Castle", 0, Kind::Number { min: 0, max: 7, step: 1 }, "Sage Portraits needed to enter Lorule Castle and fight Yuga Ganon."),
    option("ped_requirement", "Pedestal Pendants", 0, Kind::Choice(&[("Vanilla", "2 Pendants"), ("Standard", "3 Pendants")]), "Pendants needed to reach the Master Sword Pedestal."),
    option("dungeon_prize_shuffle", "Shuffle Dungeon Prizes", 0, Kind::Toggle, "Shuffles the Pendants and Portraits among the dungeons."),
    option("dark_rooms_lampless", "Dark Rooms No Lamp", 0, Kind::Toggle, "The seed may expect you to cross dark rooms without the Lamp."),
    option("lamp_and_net_as_weapons", "Lamp and Net Weapons", 0, Kind::Toggle, "Treats the basic Lamp and Net as logical weapons. For advanced players."),
    option("no_progression_enemies", "No Key Enemies", 0, Kind::Toggle, "Removes dungeon enemies that progress is built on, such as Bawbs. The seed then expects your items instead."),
    option("cracks", "Cracks", 1, Kind::Choice(&[("Closed", "Closed"), ("Open", "Open"), ("Progressive", "Progressive")]), "Whether cracks between Hyrule and Lorule start open."),
    option("cracksanity", "Crack Shuffle", 1, Kind::Choice(&[("Off", "Off"), ("CrossWorldPairs", "Cross World"), ("AnyWorldPairs", "Any World"), ("MirroredCrossWorldPairs", "Mirrored Cross"), ("MirroredAnyWorldPairs", "Mirrored Any")]), "Shuffles where cracks lead. Mirrored pairs match between the worlds."),
    option("weather_vanes", "Weather Vanes", 1, Kind::Choice(&[("Standard", "Standard"), ("Shuffled", "Shuffled"), ("Convenient", "Convenient"), ("Hyrule", "Hyrule"), ("Lorule", "Lorule"), ("All", "All")]), "Which Weather Vanes start active, and what they unlock."),
    option("trials_door", "Trials Door", 1, Kind::Choice(&[("OpenFromInsideOnly", "Inside Only"), ("OneTrialRequired", "1 Trial"), ("TwoTrialsRequired", "2 Trials"), ("ThreeTrialsRequired", "3 Trials"), ("AllTrialsRequired", "4 Trials"), ("OpenFromBothSides", "Both Sides")]), "What opens the door to the Lorule Castle Trials."),
    option("nice_items", "Nice Items", 2, Kind::Choice(&[("Vanilla", "Vanilla"), ("Shuffled", "Shuffled"), ("Off", "Off"), ("Upgrades", "Upgrades")]), "How the upgraded items from Mother Maiamai are handled."),
    option("super_items", "Super Lamp and Net", 2, Kind::Choice(&[("Shuffled", "Shuffled"), ("Off", "Off"), ("Upgrades", "Upgrades")]), "How the Super Lamp and Super Net are handled."),
    option("keysy", "Keysy", 2, Kind::Choice(&[("Off", "Off"), ("SmallKeysy", "Small Keys"), ("BigKeysy", "Big Keys"), ("AllKeysy", "All Keys")]), "Removes the chosen dungeon keys and opens their doors."),
    option("swordless_mode", "Swordless", 2, Kind::Toggle, "Removes ALL Swords from the game. Turns off Sword in Shop."),
    option("bow_of_light_in_castle", "Bow of Light in Castle", 2, Kind::Toggle, "Keeps the Bow of Light somewhere in Lorule Castle."),
    option("shuffle_maiamai_rewards", "Shuffle Maiamai Gifts", 2, Kind::Toggle, "Shuffles the items that Mother Maiamai gives."),
    option("maiamai_madness", "Maiamai Madness", 2, Kind::Toggle, "Shuffles every Maiamai into the pool, which adds 100 locations."),
    option("maiamai_limit", "Maiamai Limit", 2, Kind::Number { min: 0, max: 100, step: 5 }, "The most Maiamai a seed may require you to collect."),
    option("start_with_merge", "Start with Merge", 3, Kind::Toggle, "Merge into walls from the start, without Ravio's Bracelet."),
    option("start_with_pouch", "Start with Pouch", 3, Kind::Toggle, "Start with the Pouch and a usable X Button."),
    option("start_with_compasses", "Start with Compasses", 3, Kind::Toggle, "Start with every dungeon's Compass."),
    option("bell_in_shop", "Bell in Shop", 3, Kind::Toggle, "Ravio's Shop sells the Bell."),
    option("sword_in_shop", "Sword in Shop", 3, Kind::Toggle, "Ravio's Shop sells a Sword. Turns off Swordless and Assured Weapon."),
    option("boots_in_shop", "Boots in Shop", 3, Kind::Toggle, "Ravio's Shop sells the Pegasus Boots. Turns off Assured Weapon."),
    option("assured_weapon", "Assured Weapon", 3, Kind::Toggle, "Ravio's Shop is sure to sell a weapon. Turns off Sword and Boots in Shop."),
    option("minigames_excluded", "Exclude Minigames", 4, Kind::Toggle, "Cucco Ranch, Rupee Rush, Treacherous Tower, Octoball Derby, and Hyrule Hotfoot hold only junk."),
    option("skip_big_bomb_flower", "Skip Big Bomb Flower", 4, Kind::Toggle, "Removes the 5 Big Rocks in Lorule Field."),
    option("treacherous_tower_floors", "Treacherous Floors", 4, Kind::Number { min: 2, max: 66, step: 1 }, "How many floors Treacherous Tower has."),
    option("hint_ghosts", "Hint Ghosts", 5, Kind::Choice(&[("Off", "Off"), ("Always", "Always"), ("Glasses", "With Glasses")]), "When Hint Ghosts can be seen."),
    option("chest_size_matches_contents", "Chest Size Hints", 5, Kind::Toggle, "Large chests hold progression or unique items, and small chests hold the rest."),
    option("change_freestanding_models", "True Item Models", 5, Kind::Toggle, "WARNING: Crashes the game at launch for now. Leave this off. Loose items look like what they really are."),
    option("purple_potion_bottles", "Purple Potion Bottles", 5, Kind::Toggle, "Fills every empty Bottle with a free Purple Potion."),
    option("night_mode", "Night Mode", 5, Kind::Toggle, "Experimental. Hyrule uses night colors until you visit Lorule."),
];

pub fn option_index(key: &str) -> Option<usize> {
    OPTIONS.iter().position(|option| option.key == key)
}

/// Every setting at its randomizer default, as JSON.
pub fn default_values() -> Map<String, Value> {
    let settings: Settings = serde_json::from_str("{}").expect("default settings");
    match serde_json::to_value(settings).expect("settings serialize") {
        Value::Object(map) => map,
        _ => unreachable!("settings serialize to an object"),
    }
}

#[derive(Clone, Debug)]
pub struct Rules {
    values: Map<String, Value>,
}

impl Default for Rules {
    fn default() -> Self {
        Self { values: default_values() }
    }
}

impl Rules {
    /// Starts from the defaults and applies a preset's values.
    pub fn with_overrides(overrides: &Map<String, Value>) -> Self {
        let mut rules = Self::default();
        for (key, value) in overrides {
            rules.values.insert(key.clone(), value.clone());
            rules.resolve_conflicts(key);
        }
        rules
    }

    pub fn value_text(&self, option: &OptionDef) -> String {
        let value = self.values.get(option.key);
        match option.kind {
            Kind::Toggle => if value.and_then(Value::as_bool).unwrap_or(false) { "On" } else { "Off" }.to_owned(),
            Kind::Choice(choices) => {
                let stored = value.and_then(Value::as_str).unwrap_or("");
                choices.iter().find(|(key, _)| *key == stored).map_or_else(|| stored.to_owned(), |(_, label)| (*label).to_owned())
            }
            Kind::Number { .. } => value.and_then(Value::as_i64).unwrap_or(0).to_string(),
        }
    }

    /// The option's choices and the one picked, for drawing as pills. Toggles are Off and On.
    pub fn pills(&self, option: &OptionDef) -> crate::menu::Pills {
        use crate::menu::Pills;
        let value = self.values.get(option.key);
        match option.kind {
            Kind::Toggle => Pills::Choices {
                names: vec!["Off".to_owned(), "On".to_owned()],
                selected: usize::from(value.and_then(Value::as_bool).unwrap_or(false)),
            },
            Kind::Choice(choices) => {
                let stored = value.and_then(Value::as_str).unwrap_or("");
                Pills::Choices {
                    names: choices.iter().map(|(_, label)| (*label).to_owned()).collect(),
                    selected: choices.iter().position(|(key, _)| *key == stored).unwrap_or(0),
                }
            }
            Kind::Number { min, max, .. } => Pills::Number { value: value.and_then(Value::as_i64).unwrap_or(min), min, max },
        }
    }

    /// Picks a toggle's or choice's value by its place in `pills`. Numbers are left alone.
    pub fn select(&mut self, option: &OptionDef, index: usize) {
        let next = match option.kind {
            Kind::Toggle => json!(index == 1),
            Kind::Choice(choices) => match choices.get(index) {
                Some((key, _)) => json!(key),
                None => return,
            },
            Kind::Number { .. } => return,
        };
        self.values.insert(option.key.to_owned(), next);
        self.resolve_conflicts(option.key);
    }

    /// Moves an option to its next (`delta` 1) or previous (`delta` -1) value, wrapping at the ends.
    pub fn step(&mut self, option: &OptionDef, delta: i32) {
        let current = self.values.get(option.key);
        let next = match option.kind {
            Kind::Toggle => json!(!current.and_then(Value::as_bool).unwrap_or(false)),
            Kind::Choice(choices) => {
                let stored = current.and_then(Value::as_str).unwrap_or("");
                let index = choices.iter().position(|(key, _)| *key == stored).unwrap_or(0) as i32;
                json!(choices[(index + delta).rem_euclid(choices.len() as i32) as usize].0)
            }
            Kind::Number { min, max, step } => {
                let value = current.and_then(Value::as_i64).unwrap_or(min);
                let moved = value + step * i64::from(delta);
                json!(if moved > max { min } else if moved < min { max } else { moved })
            }
        };
        self.values.insert(option.key.to_owned(), next);
        self.resolve_conflicts(option.key);
    }

    fn turn_off(&mut self, key: &str) {
        self.values.insert(key.to_owned(), json!(false));
    }

    fn is_on(&self, key: &str) -> bool {
        self.values.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    /// Keeps the rules that `validate_settings` enforces. The option just turned on wins.
    fn resolve_conflicts(&mut self, changed: &str) {
        if !self.is_on(changed) {
            return;
        }
        match changed {
            "swordless_mode" => self.turn_off("sword_in_shop"),
            "sword_in_shop" => {
                self.turn_off("swordless_mode");
                self.turn_off("assured_weapon");
            }
            "boots_in_shop" => self.turn_off("assured_weapon"),
            "assured_weapon" => {
                self.turn_off("sword_in_shop");
                self.turn_off("boots_in_shop");
            }
            "bow_of_light_in_castle" => self.turn_off("progressive_bow_of_light"),
            "progressive_bow_of_light" => self.turn_off("bow_of_light_in_castle"),
            _ => {}
        }
    }

    /// The settings for the randomizer.
    pub fn settings(&self) -> Settings {
        let mut values = self.values.clone();
        // `Settings` never serializes this field, so it always comes from the Lorule Castle option.
        values.insert("yuganon_requirement".to_owned(), values.get("lc_requirement").cloned().unwrap_or(json!(7)));
        values.remove("ravios_shop");
        serde_json::from_value(Value::Object(values)).expect("values come from the table or a checked preset")
    }

    /// Only the values that differ from the defaults, which is what a preset file stores.
    pub fn overrides(&self) -> Map<String, Value> {
        let defaults = default_values();
        self.values.iter().filter(|(key, value)| defaults.get(*key) != Some(*value)).map(|(key, value)| (key.clone(), value.clone())).collect()
    }

    /// Lines naming each option that differs from the defaults.
    pub fn summary(&self) -> Vec<String> {
        let defaults = Rules::default();
        OPTIONS
            .iter()
            .filter(|option| self.values.get(option.key) != defaults.values.get(option.key))
            .map(|option| format!("{}: {}", option.label, self.value_text(option)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_has_a_menu_entry_or_is_listed_as_hidden() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        // These are only written when set, so turn them on to see their keys.
        settings.dev_mode = true;
        settings.progressive_bow_of_light = true;
        for key in default_values().keys().chain(serde_json::to_value(settings).unwrap().as_object().unwrap().keys()) {
            assert!(option_index(key).is_some() || HIDDEN.contains(&key.as_str()), "{key} is missing from the table");
        }
    }

    #[test]
    fn every_table_key_is_a_real_setting() {
        for option in OPTIONS {
            let probe = json!({ option.key: Rules::default().values.get(option.key).cloned().unwrap_or(Value::Null) });
            serde_json::from_value::<Settings>(probe).unwrap_or_else(|err| panic!("{}: {err}", option.key));
            assert!(option.category < CATEGORIES.len());
        }
    }

    #[test]
    fn every_value_of_every_option_converts_to_settings() {
        for option in OPTIONS {
            let mut rules = Rules::default();
            let start = rules.value_text(option);
            let mut seen = 0;
            loop {
                rules.step(option, 1);
                rules.settings();
                seen += 1;
                if rules.value_text(option) == start || seen > 120 {
                    break;
                }
            }
            assert!(seen <= 120, "{} never wrapped", option.key);
        }
    }

    #[test]
    fn stepping_back_undoes_a_step() {
        for option in OPTIONS {
            let mut rules = Rules::default();
            let start = rules.value_text(option);
            rules.step(option, 1);
            rules.step(option, -1);
            assert_eq!(rules.value_text(option), start, "{}", option.key);
        }
    }

    #[test]
    fn conflicting_options_turn_each_other_off() {
        let mut rules = Rules::default();
        let by_key = |key: &str| &OPTIONS[option_index(key).unwrap()];
        if !rules.is_on("sword_in_shop") {
            rules.step(by_key("sword_in_shop"), 1);
        }
        rules.values.insert("swordless_mode".into(), json!(false));
        rules.step(by_key("swordless_mode"), 1);
        let settings = rules.settings();
        assert!(settings.swordless_mode && !settings.sword_in_shop);

        rules.values.insert("assured_weapon".into(), json!(false));
        rules.step(by_key("assured_weapon"), 1);
        rules.step(by_key("boots_in_shop"), 1);
        assert!(!rules.settings().assured_weapon);
    }

    #[test]
    fn yuga_requirement_follows_the_castle_requirement() {
        let mut rules = Rules::default();
        rules.step(&OPTIONS[option_index("lc_requirement").unwrap()], -1);
        let settings = rules.settings();
        assert_eq!(settings.yuganon_requirement, settings.lc_requirement);
    }
}
