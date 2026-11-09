//! Writes the player's Archipelago options file from the app's rules.
//!
//! The rules pages already cover nearly every option the "A Link Between Worlds - 3DS" apworld has,
//! so the options file is made from them instead of a second set of pages. Options the apworld has
//! but the rules lack keep the apworld's defaults. Rules the apworld lacks, such as a closed Ravio's
//! shop or excluded checks, are left out and listed by `unsupported`.

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
pub fn options_file(settings: &Settings, player: &str, preset: &str) -> String {
    let mut text = format!(
        "# Archipelago options made by the ALBW Randomizer 3DS app from the \"{}\" rules.\n\
         # Needs the \"{GAME}\" apworld on the computer that generates the multiworld.\n\
         name: {}\n\
         description: Made on the ALBW Randomizer 3DS app\n\
         game: {GAME}\n\
         requires:\n  version: 0.6.3\n\
         {GAME}:\n",
        preset.replace(['\n', '"'], " "),
        clean_name(player),
    );
    for (key, value) in options(settings) {
        text.push_str(&format!("  {key}: {value}\n"));
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
        let file = options_file(&settings, "Link", "Standard");
        assert!(file.contains("name: Link\n"));
        assert!(file.contains("game: A Link Between Worlds - 3DS\n"));
        assert!(file.contains("\nA Link Between Worlds - 3DS:\n"));
        assert_eq!(options(&settings).len(), 38);
        for (key, _) in options(&settings) {
            assert!(file.contains(&format!("  {key}: ")), "{key}");
        }
    }

    /// Set `ALBW_YAML_OUT` to a folder to write options files for each built-in preset, to test
    /// them with Archipelago.
    #[test]
    fn writes_preset_files_when_asked() {
        let Ok(folder) = std::env::var("ALBW_YAML_OUT") else { return };
        for (index, preset) in crate::presets::built_in().iter().enumerate() {
            let rules = crate::rules::Rules::with_overrides(&preset.overrides);
            let file = options_file(&rules.settings(), &format!("Preset{index}"), &preset.name);
            std::fs::write(format!("{folder}/preset{index}.yaml"), file).unwrap();
        }
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
}
