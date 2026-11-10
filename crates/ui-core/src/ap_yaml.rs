//! The player's Archipelago options, and the options file made from them.
//!
//! The Archipelago Options pages edit every option the "A Link Between Worlds - 3DS" apworld has,
//! from a table that `apworld/gen_tables.py` copies out of the apworld's Options.py. The player can
//! also start from the app's rules: `options` maps them to apworld values, and `unsupported` lists
//! the rules the apworld lacks, such as a closed Ravio's shop or excluded checks.

use modinfo::settings::{
    Cracks, Cracksanity, HintGhosts, Keysy, LogicMode, NiceItems, PedestalSetting, RaviosShop, SuperItems, TrialsDoor,
    WeatherVanes,
};
use modinfo::Settings;

/// The game name the 3DS edition apworld uses.
pub const GAME: &str = "A Link Between Worlds - 3DS";

/// Archipelago slot names allow at most 16 characters, and no `{` or `}`.
pub fn clean_name(name: &str) -> String {
    let name: String = name.trim().chars().filter(|c| !matches!(c, '{' | '}' | '\n' | '\r' | ':' | '"')).take(16).collect();
    if name.trim().is_empty() {
        "Player".to_owned()
    } else {
        name.trim().to_owned()
    }
}

fn yes(on: bool) -> &'static str {
    if on {
        "true"
    } else {
        "false"
    }
}

/// How an apworld option is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ApKind {
    Toggle,
    Choice(&'static [ApChoice]),
    Range { min: i64, max: i64, step: i64 },
}

/// One value of a choice option.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApChoice {
    /// The value as the options file writes it.
    pub key: &'static str,
    pub label: &'static str,
    /// What this value does, from the apworld's description.
    pub help: &'static str,
}

/// One apworld option.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApOptionDef {
    pub key: &'static str,
    pub label: &'static str,
    /// Its page, as an index into `AP_CATEGORIES`.
    pub category: usize,
    pub help: &'static str,
    pub kind: ApKind,
    /// For a toggle 0 or 1, for a choice the index of its value, and for a range the number.
    pub default: i64,
}

pub use crate::ap_option_table::{AP_CATEGORIES, AP_OPTIONS};

/// The value of every apworld option, in `AP_OPTIONS` order.
#[derive(Clone, Debug, PartialEq)]
pub struct ApOptions {
    values: Vec<i64>,
}

impl Default for ApOptions {
    fn default() -> Self {
        Self { values: AP_OPTIONS.iter().map(|option| option.default).collect() }
    }
}

impl ApOptions {
    /// The options that match these rules. Options the rules lack keep the apworld's defaults.
    pub fn from_rules(settings: &Settings) -> Self {
        let mut found = Self::default();
        for (key, value) in options(settings) {
            found.set(key, &value);
        }
        found
    }

    /// Reads options saved by `save`. Unknown keys and values are skipped, so a file from an older
    /// app still loads.
    pub fn load(text: &str) -> Self {
        let mut found = Self::default();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once(':') {
                found.set(key.trim(), value.trim());
            }
        }
        found
    }

    /// The options as `key: value` lines, the way the options file lists them.
    pub fn save(&self) -> String {
        (0..AP_OPTIONS.len()).map(|index| format!("{}: {}\n", AP_OPTIONS[index].key, self.text(index))).collect()
    }

    /// Sets the option called `key` from its options-file text. Returns false if either is unknown.
    pub fn set(&mut self, key: &str, text: &str) -> bool {
        let Some(index) = AP_OPTIONS.iter().position(|option| option.key == key) else { return false };
        let value = match AP_OPTIONS[index].kind {
            ApKind::Toggle => match text {
                "true" | "on" | "1" => 1,
                "false" | "off" | "0" => 0,
                _ => return false,
            },
            ApKind::Choice(choices) => match choices.iter().position(|choice| choice.key == text) {
                Some(found) => found as i64,
                None => return false,
            },
            ApKind::Range { min, max, .. } => match text.parse::<i64>() {
                Ok(number) => number.clamp(min, max),
                Err(_) => return false,
            },
        };
        self.values[index] = value;
        true
    }

    /// The value of option `index` as the options file writes it.
    pub fn text(&self, index: usize) -> String {
        let value = self.values[index];
        match AP_OPTIONS[index].kind {
            ApKind::Toggle => yes(value != 0).to_owned(),
            ApKind::Choice(choices) => choices[value as usize].key.to_owned(),
            ApKind::Range { .. } => value.to_string(),
        }
    }

    /// The value of option `index` as the menu shows it.
    pub fn label(&self, index: usize) -> String {
        let value = self.values[index];
        match AP_OPTIONS[index].kind {
            ApKind::Toggle => if value != 0 { "On" } else { "Off" }.to_owned(),
            ApKind::Choice(choices) => choices[value as usize].label.to_owned(),
            ApKind::Range { .. } => value.to_string(),
        }
    }

    /// The option's description, plus what its current value does.
    pub fn help(&self, index: usize) -> String {
        let option = &AP_OPTIONS[index];
        match option.kind {
            ApKind::Choice(choices) if !choices[self.values[index] as usize].help.is_empty() => {
                let choice = &choices[self.values[index] as usize];
                format!("{} {}: {}", option.help, choice.label, choice.help)
            }
            _ => option.help.to_owned(),
        }
    }

    /// The raw value: 0 or 1 for a toggle, the choice's index, or the number.
    pub fn value(&self, index: usize) -> i64 {
        self.values[index]
    }

    /// Moves option `index` by `delta` presses. Toggles and choices wrap. Numbers stop at the ends,
    /// and move by the option's step.
    pub fn step(&mut self, index: usize, delta: i32) {
        let value = &mut self.values[index];
        match AP_OPTIONS[index].kind {
            ApKind::Toggle => *value = (*value + delta as i64).rem_euclid(2),
            ApKind::Choice(choices) => *value = (*value + delta as i64).rem_euclid(choices.len() as i64),
            ApKind::Range { min, max, step } => *value = (*value + delta as i64 * step).clamp(min, max),
        }
    }

    /// Picks a toggle's or choice's value by its place among the pills. Numbers are left alone.
    pub fn select(&mut self, index: usize, choice: usize) {
        let count = match AP_OPTIONS[index].kind {
            ApKind::Toggle => 2,
            ApKind::Choice(choices) => choices.len(),
            ApKind::Range { .. } => return,
        };
        if choice < count {
            self.values[index] = choice as i64;
        }
    }
}

/// Each option as the apworld names it, with the value for these rules.
pub fn options(settings: &Settings) -> Vec<(&'static str, String)> {
    let text = |value: &str| value.to_owned();
    let logic = match settings.logic_mode {
        LogicMode::Normal | LogicMode::NoLogic => "normal",
        LogicMode::Hard => "hard",
        LogicMode::Glitched => "glitched",
        LogicMode::AdvGlitched => "adv_glitched",
        LogicMode::Hell => "hell",
    };
    let nice = match settings.nice_items {
        NiceItems::Vanilla => "vanilla",
        NiceItems::Shuffled => "shuffled",
        NiceItems::Off => "off",
        NiceItems::Upgrades => "upgrades",
    };
    let super_items = match settings.super_items {
        SuperItems::Shuffled => "shuffled",
        SuperItems::Off => "off",
        SuperItems::Upgrades => "upgrades",
    };
    let cracks = match settings.cracks {
        Cracks::Closed => "closed",
        Cracks::Open => "open",
        Cracks::Progressive => "progressive",
    };
    let crack_shuffle = match settings.cracksanity {
        Cracksanity::Off => "off",
        Cracksanity::CrossWorldPairs => "cross_world_pairs",
        Cracksanity::AnyWorldPairs => "any_world_pairs",
        Cracksanity::MirroredCrossWorldPairs => "mirrored_cross_world_pairs",
        Cracksanity::MirroredAnyWorldPairs => "mirrored_any_world_pairs",
    };
    let vanes = match settings.weather_vanes {
        WeatherVanes::Standard => "standard",
        WeatherVanes::Shuffled => "shuffled",
        WeatherVanes::Convenient => "convenient",
        WeatherVanes::Hyrule => "hyrule",
        WeatherVanes::Lorule => "lorule",
        WeatherVanes::All => "all",
    };
    let (small_keys, big_keys) = match settings.keysy {
        Keysy::Off => ("own_dungeons", "own_dungeons"),
        Keysy::SmallKeysy => ("remove", "own_dungeons"),
        Keysy::BigKeysy => ("own_dungeons", "remove"),
        Keysy::AllKeysy => ("remove", "remove"),
    };
    let trials_required = match settings.trials_door {
        TrialsDoor::OpenFromInsideOnly => 0,
        TrialsDoor::OneTrialRequired => 1,
        TrialsDoor::TwoTrialsRequired => 2,
        TrialsDoor::ThreeTrialsRequired => 3,
        TrialsDoor::AllTrialsRequired | TrialsDoor::OpenFromBothSides => 4,
    };
    let hint_ghosts = match settings.hint_ghosts {
        HintGhosts::Off => "off",
        HintGhosts::Always => "always",
        HintGhosts::Glasses => "glasses",
    };
    vec![
        ("logic_mode", text(logic)),
        ("randomize_dungeon_prizes", text(yes(settings.dungeon_prize_shuffle))),
        ("progression_bosses", text("false")),
        ("lorule_castle_requirement", settings.lc_requirement.min(7).to_string()),
        (
            "pedestal_requirement",
            text(match settings.ped_requirement {
                PedestalSetting::Vanilla => "vanilla",
                PedestalSetting::Standard => "standard",
            }),
        ),
        ("nice_items", text(nice)),
        ("super_items", text(super_items)),
        ("shuffle_maiamai_rewards", text(yes(settings.shuffle_maiamai_rewards))),
        ("maiamai_limit", settings.maiamai_limit.min(100).to_string()),
        ("small_keys", text(small_keys)),
        ("big_keys", text(big_keys)),
        ("compasses", text(if settings.start_with_compasses { "startwith" } else { "own_dungeons" })),
        ("key_rings", text("false")),
        ("lamp_and_net_as_weapons", text(yes(settings.lamp_and_net_as_weapons))),
        ("no_progression_enemies", text(yes(settings.no_progression_enemies))),
        ("assured_weapon", text(yes(settings.assured_weapon))),
        ("maiamai_mayhem", text(yes(settings.maiamai_madness))),
        ("initial_crack_state", text(cracks)),
        ("crack_shuffle", text(crack_shuffle)),
        ("minigames_excluded", text(yes(settings.minigames_excluded))),
        ("skip_big_bomb_flower", text(yes(settings.skip_big_bomb_flower))),
        ("trials_required", trials_required.to_string()),
        ("open_trials_door", text(yes(settings.trials_door == TrialsDoor::OpenFromBothSides))),
        ("bow_of_light_in_castle", text(yes(settings.bow_of_light_in_castle))),
        ("weather_vanes", text(vanes)),
        ("dark_rooms_lampless", text(yes(settings.dark_rooms_lampless))),
        ("swordless_mode", text(yes(settings.swordless_mode))),
        ("hint_ghosts", text(hint_ghosts)),
        ("chest_size_matches_contents", text(yes(settings.chest_size_matches_contents))),
        ("treacherous_tower_floors", settings.treacherous_tower_floors.clamp(2, 66).to_string()),
        ("purple_potion_bottles", text(yes(settings.purple_potion_bottles))),
        ("start_with_merge", text(yes(settings.start_with_merge))),
        ("start_with_pouch", text(yes(settings.start_with_pouch))),
        ("bell_in_shop", text(yes(settings.bell_in_shop))),
        ("sword_in_shop", text(yes(settings.sword_in_shop))),
        ("boots_in_shop", text(yes(settings.boots_in_shop))),
        ("night_mode", text(yes(settings.night_mode))),
        ("change_freestanding_models", text(yes(settings.change_freestanding_models))),
    ]
}

/// Rules the apworld cannot use, so the options file leaves them out.
pub fn unsupported(settings: &Settings) -> Vec<&'static str> {
    let mut found = Vec::new();
    if settings.progressive_bow_of_light {
        found.push("Progressive Bow of Light");
    }
    if settings.ravios_shop != RaviosShop::Open {
        found.push("Closed Ravio's Shop");
    }
    if settings.logic_mode == LogicMode::NoLogic {
        found.push("No Logic (Normal is used)");
    }
    if !settings.user_exclusions.is_empty() {
        found.push("Excluded checks");
    }
    found
}

/// The whole options file.
pub fn options_file(options: &ApOptions, player: &str) -> String {
    let mut text = format!(
        "# Archipelago options made by the ALBW Randomizer 3DS app.\n\
         # Needs the \"{GAME}\" apworld on the computer that generates the multiworld.\n\
         name: {}\n\
         description: Made on the ALBW Randomizer 3DS app\n\
         game: {GAME}\n\
         requires:\n  version: 0.6.3\n\
         {GAME}:\n",
        clean_name(player),
    );
    // Choice values are quoted: YAML would read a bare `off` as false.
    for (index, option) in AP_OPTIONS.iter().enumerate() {
        let value = match option.kind {
            ApKind::Choice(_) => format!("'{}'", options.text(index)),
            _ => options.text(index),
        };
        text.push_str(&format!("  {}: {value}\n", option.key));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_archipelago_rules() {
        assert_eq!(clean_name("  Link  "), "Link");
        assert_eq!(clean_name("A{very}long:name that goes on"), "Averylongname th");
        assert_eq!(clean_name("   "), "Player");
    }

    #[test]
    fn the_file_names_the_game_and_every_option() {
        let settings = crate::rules::Rules::default().settings();
        let file = options_file(&ApOptions::from_rules(&settings), "Link");
        assert!(file.contains("name: Link\n"));
        assert!(file.contains("game: A Link Between Worlds - 3DS\n"));
        assert!(file.contains("\nA Link Between Worlds - 3DS:\n"));
        assert_eq!(options(&settings).len(), 38);
        for option in AP_OPTIONS {
            assert!(file.contains(&format!("  {}: ", option.key)), "{}", option.key);
        }
    }

    /// Set `ALBW_YAML_OUT` to a folder to write options files for each built-in preset, to test
    /// them with Archipelago.
    #[test]
    fn writes_preset_files_when_asked() {
        let Ok(folder) = std::env::var("ALBW_YAML_OUT") else { return };
        for (index, preset) in crate::presets::built_in().iter().enumerate() {
            let rules = crate::rules::Rules::with_overrides(&preset.overrides);
            let file = options_file(&ApOptions::from_rules(&rules.settings()), &format!("Preset{index}"));
            std::fs::write(format!("{folder}/preset{index}.yaml"), file).unwrap();
        }
        // Every option at its last value, to test the apworld's conflict rules.
        let mut options = ApOptions::default();
        for index in 0..AP_OPTIONS.len() {
            options.step(index, -1);
            if let ApKind::Range { max, .. } = AP_OPTIONS[index].kind {
                options.set(AP_OPTIONS[index].key, &max.to_string());
            }
        }
        std::fs::write(format!("{folder}/last.yaml"), options_file(&options, "Last")).unwrap();
        std::fs::write(format!("{folder}/defaults.yaml"), options_file(&ApOptions::default(), "Defaults")).unwrap();
    }

    #[test]
    fn rules_map_to_apworld_values() {
        let mut settings = crate::rules::Rules::default().settings();
        settings.cracksanity = Cracksanity::MirroredAnyWorldPairs;
        settings.keysy = Keysy::SmallKeysy;
        settings.trials_door = TrialsDoor::OpenFromBothSides;
        settings.start_with_merge = true;
        let options = options(&settings);
        let get = |key: &str| options.iter().find(|(name, _)| *name == key).unwrap().1.clone();
        assert_eq!(get("crack_shuffle"), "mirrored_any_world_pairs");
        assert_eq!(get("small_keys"), "remove");
        assert_eq!(get("big_keys"), "own_dungeons");
        assert_eq!(get("open_trials_door"), "true");
        assert_eq!(get("start_with_merge"), "true");
    }

    #[test]
    fn every_rule_value_is_one_the_apworld_has() {
        for preset in crate::presets::built_in() {
            let settings = crate::rules::Rules::with_overrides(&preset.overrides).settings();
            for (key, value) in options(&settings) {
                assert!(ApOptions::default().set(key, &value), "{}: {key}: {value}", preset.name);
            }
        }
    }

    #[test]
    fn options_save_and_load() {
        let mut options = ApOptions::default();
        let death_link = AP_OPTIONS.iter().position(|option| option.key == "death_link").unwrap();
        let crack_shuffle = AP_OPTIONS.iter().position(|option| option.key == "crack_shuffle").unwrap();
        let balancing = AP_OPTIONS.iter().position(|option| option.key == "progression_balancing").unwrap();
        options.step(death_link, 1);
        options.step(crack_shuffle, -1);
        options.step(balancing, 20);
        assert_eq!(options.text(death_link), "true");
        assert_eq!(options.text(crack_shuffle), "mirrored_any_world_pairs");
        assert_eq!(options.text(balancing), "99");
        assert_eq!(ApOptions::load(&options.save()), options);
        assert_eq!(ApOptions::load("death_link: maybe\nnot_an_option: 3\n"), ApOptions::default());
    }
}
