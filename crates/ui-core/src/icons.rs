//! Which of the game's own textures stand for each option. The app reads these from the player's
//! game, so no Nintendo art ships with it.

/// Each archive under `Archive/`, with the textures to read from it.
pub const SOURCES: &[(&str, &[&str])] = &[
    ("Lyt_Collect.arc", &[
        "CIS_Boots_00",
        "CIS_KaigaMayple_00",
        "CIS_Kouseki_00",
        "CIS_Pendant_00",
        "CIS_Pendant_01",
        "CIS_Sword_00",
        "CIS_Torisetsu_00",
        "CIS_Udewa_01",
        "II_Kinsuta32_00",
    ]),
    ("Lyt_Common.arc", &[
        "II_Bakudan32_01",
        "II_BinMessage32_00",
        "II_BinPurple32_00",
        "II_BombR32_00",
        "II_Cllect32_00",
        "II_FireRod32_01",
        "II_FruitEscape32_00",
        "II_Glass32_00",
        "II_Kantera32_00",
        "II_Kantera32_01",
        "II_Mushiami32_00",
        "II_SwordA32_00",
        "II_TornadoA32_00",
        "II_Yumi32_02",
    ]),
    ("Lyt_Dungeon.arc", &[
        "MapIconBossDoor",
    ]),
    ("Lyt_Field.arc", &[
        "KinstaIcon",
        "PinSmallR_00",
    ]),
    ("Lyt_Game.arc", &[
        "II_GameCoin32_00",
        "II_Warp32_01",
        "KeySmall_00",
        "SRC_JiisannIcon_00",
        "SRC_Link32_00",
        "SRC_Link32_03",
    ]),
    ("Lyt_Light.arc", &[
        "WarpIconLNew_00",
    ]),
    ("Lyt_Menu.arc", &[
        "SageIconRcy_00",
    ]),
];

/// How many quarter turns counterclockwise to add when drawing a texture, for art the game shows
/// at an angle that reads poorly in a list.
pub fn extra_turns(texture: &str) -> u8 {
    match texture {
        // The bell is stored tilted mouth up.
        "II_Warp32_01" => 2,
        _ => 0,
    }
}

/// The texture for a setting, if it has one.
pub fn for_option(key: &str) -> Option<&'static str> {
    Some(match key {
        "logic_mode" => "SRC_Link32_00",
        "lc_requirement" => "SageIconRcy_00",
        "ped_requirement" => "CIS_Pendant_00",
        "dungeon_prize_shuffle" => "CIS_Pendant_01",
        "dark_rooms_lampless" => "II_Kantera32_00",
        "lamp_and_net_as_weapons" => "II_Mushiami32_00",
        "no_progression_enemies" => "II_BombR32_00",
        "cracks" => "II_BinMessage32_00",
        "cracksanity" => "CIS_Torisetsu_00",
        "trials_door" => "MapIconBossDoor",
        "nice_items" => "II_FireRod32_01",
        "super_items" => "II_Kantera32_01",
        "keysy" => "KeySmall_00",
        "swordless_mode" => "CIS_Sword_00",
        "bow_of_light_in_castle" => "II_Yumi32_02",
        "shuffle_maiamai_rewards" => "SRC_JiisannIcon_00",
        "maiamai_madness" => "II_Kinsuta32_00",
        "maiamai_limit" => "KinstaIcon",
        "start_with_merge" => "CIS_Udewa_01",
        "sword_in_shop" => "II_SwordA32_00",
        "boots_in_shop" => "CIS_Boots_00",
        "assured_weapon" => "II_TornadoA32_00",
        "minigames_excluded" => "II_GameCoin32_00",
        "skip_big_bomb_flower" => "II_Bakudan32_01",
        "hint_ghosts" => "II_Glass32_00",
        "purple_potion_bottles" => "II_BinPurple32_00",
        "weather_vanes" => "WarpIconLNew_00",
        "start_with_pouch" => "II_Cllect32_00",
        "treacherous_tower_floors" => "SRC_Link32_03",
        "change_freestanding_models" => "II_FruitEscape32_00",
        "night_mode" => "CIS_KaigaMayple_00",
        "bell_in_shop" => "II_Warp32_01",
        "start_with_compasses" => "PinSmallR_00",
        "chest_size_matches_contents" => "CIS_Kouseki_00",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::OPTIONS;

    #[test]
    fn every_icon_has_a_source() {
        let known: Vec<&str> = SOURCES.iter().flat_map(|(_, names)| names.iter().copied()).collect();
        for option in OPTIONS {
            if let Some(icon) = for_option(option.key) {
                assert!(known.contains(&icon), "{icon} has no archive");
            }
        }
    }

    #[test]
    fn every_source_is_used() {
        for name in SOURCES.iter().flat_map(|(_, names)| names.iter().copied()) {
            assert!(OPTIONS.iter().any(|option| for_option(option.key) == Some(name)), "{name} is unused");
        }
    }
}
