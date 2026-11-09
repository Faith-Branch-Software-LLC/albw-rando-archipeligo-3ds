//! Rebuilds an "A Link Between Worlds - 3DS" Archipelago patch into a seed the patcher can write.
//!
//! This is the Rust side of `patch_albw_inner` in the apworld's `Patch.py`. The host's computer
//! already placed every item, so nothing is randomized here. The patch file holds the seed, the
//! options, and each location's item; this rebuilds the same `SeedInfo` the computer would and
//! leaves writing the game files to the normal patcher.

pub mod tables;
mod zip;

use modinfo::settings::{
    Cracks, Cracksanity, HintGhosts, Keysy, LogicMode, NiceItems, PedestalSetting, RaviosShop, SuperItems, TrialsDoor,
    WeatherVanes,
};
use modinfo::Settings;
use randomizer::filler::filler_item::{new_archipelago_item, new_goal, new_item, new_vane, PyRandomizable};
use randomizer::{ArchipelagoInfo, ArchipelagoItem, DashMap, SeedInfo};
use serde_json::{Map, Value};

use tables::{Progress, CHARSET, ITEMS, LOCATIONS};

/// The game name the 3DS edition apworld gives its patches.
pub const GAME: &str = "A Link Between Worlds - 3DS";
/// The original apworld's game name, to explain why its patches do not load.
const ORIGINAL_GAME: &str = "A Link Between Worlds";
/// Patch file extension, without the dot.
pub const EXTENSION: &str = "apalbw3ds";
/// The patch versions this app can rebuild, matching `PatchInfo` in the apworld's `Patch.py`.
pub const OLDEST_VERSION: (u32, u32, u32) = (0, 4, 0);
pub const NEWEST_VERSION: (u32, u32, u32) = (0, 4, 0);

/// One location's item, as the patch file describes it.
#[derive(Clone, Debug)]
pub struct ItemInfo {
    pub name: String,
    pub player_name: String,
    pub classification: u8,
}

/// The contents of `patch_info.json`.
#[derive(Clone, Debug)]
pub struct Patch {
    pub version: String,
    pub seed: u32,
    pub player_name: String,
    pub options: Map<String, Value>,
    /// Each location and the name of the item there. "AP Item" means another player's item.
    pub check_map: Vec<(String, String)>,
    pub items: Vec<(String, ItemInfo)>,
    pub hints: Vec<String>,
    pub bow_of_light_hint: String,
}

/// Look-only options the player may change on the 3DS. `None` keeps what the patch says.
#[derive(Clone, Copy, Debug, Default)]
pub struct Looks {
    pub night_mode: Option<bool>,
    pub change_freestanding_models: Option<bool>,
}

fn version_of(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.split('.').map(|part| part.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next().flatten().unwrap_or(0)))
}

fn show((major, minor, patch): (u32, u32, u32)) -> String {
    format!("{major}.{minor}.{patch}")
}

/// Reads a `.apalbw3ds` file.
pub fn read(bytes: &[u8]) -> Result<Patch, String> {
    if let Some(manifest) = zip::read(bytes, "archipelago.json")? {
        let manifest: Value = serde_json::from_slice(&manifest).map_err(|_| "The patch file's manifest is damaged.".to_owned())?;
        match manifest.get("game").and_then(Value::as_str) {
            Some(GAME) | None => {}
            Some(ORIGINAL_GAME) => {
                return Err(format!(
                    "This patch is for the original apworld. Ask the host to generate with the \"{GAME}\" apworld from albw.local."
                ))
            }
            Some(other) => return Err(format!("This patch is for {other}, not A Link Between Worlds.")),
        }
    }
    let info = zip::read(bytes, "patch_info.json")?.ok_or("The patch file has no patch_info.json.")?;
    parse(&info)
}

/// Reads `patch_info.json` itself.
pub fn parse(json: &[u8]) -> Result<Patch, String> {
    let info: Value = serde_json::from_slice(json).map_err(|err| format!("The patch info is damaged: {err}"))?;
    let text = |key: &str| info.get(key).and_then(Value::as_str).map(str::to_owned);
    let version = text("version").ok_or("The patch info has no version.")?;
    let parsed = version_of(&version).ok_or_else(|| format!("The patch version {version} is not a version number."))?;
    if parsed > NEWEST_VERSION {
        return Err(format!(
            "This patch was made with apworld {version}, which is newer than this app supports ({}). Update the app.",
            show(NEWEST_VERSION)
        ));
    }
    if parsed < OLDEST_VERSION {
        return Err(format!(
            "This patch was made with apworld {version}. This app needs apworld {} or newer.",
            show(OLDEST_VERSION)
        ));
    }
    let object = |key: &str| info.get(key).and_then(Value::as_object).ok_or_else(|| format!("The patch info has no {key}."));
    let check_map = object("check_map")?
        .iter()
        .map(|(place, item)| Ok((place.clone(), item.as_str().ok_or("A check_map item is not text.")?.to_owned())))
        .collect::<Result<_, String>>()?;
    let items = object("items")?
        .iter()
        .map(|(place, item)| {
            let field = |key: &str| item.get(key).and_then(Value::as_str).map(str::to_owned);
            (
                place.clone(),
                ItemInfo {
                    name: field("name").unwrap_or_else(|| "an Archipelago item".to_owned()),
                    player_name: field("player_name").unwrap_or_else(|| "someone".to_owned()),
                    classification: item.get("classification").and_then(Value::as_u64).unwrap_or(0) as u8,
                },
            )
        })
        .collect();
    let hints = info
        .get("hints")
        .and_then(Value::as_array)
        .ok_or("The patch info has no hints.")?
        .iter()
        .map(|hint| hint.as_str().unwrap_or_default().to_owned())
        .collect();
    Ok(Patch {
        version,
        seed: info.get("seed").and_then(Value::as_u64).ok_or("The patch info has no seed.")? as u32,
        player_name: text("player_name").unwrap_or_default(),
        options: object("options")?.clone(),
        check_map,
        items,
        hints,
        bow_of_light_hint: text("bow_of_light_hint").unwrap_or_default(),
    })
}

/// Keeps characters the game's font has and turns the rest into '?', like the apworld's `sanitize`.
pub fn sanitize(text: &str) -> String {
    text.chars().map(|c| if CHARSET.contains(c) { c } else { '?' }).collect()
}

/// An option's value, or the apworld's default when the patch leaves it out.
fn option(options: &Map<String, Value>, key: &str) -> i64 {
    let default = match key {
        "lorule_castle_requirement" => 7,
        "pedestal_requirement" | "super_items" | "initial_crack_state" | "hint_ghosts" => 1,
        "maiamai_limit" => 50,
        "trials_required" => 4,
        "treacherous_tower_floors" => 5,
        _ => 0,
    };
    options.get(key).and_then(Value::as_i64).unwrap_or(default)
}

/// The randomizer settings for the patch's options, the same as `create_randomizer_settings` in the
/// apworld's `Options.py`.
pub fn settings(options: &Map<String, Value>) -> Settings {
    let get = |key: &str| option(options, key);
    let on = |key: &str| get(key) != 0;
    let mut settings = Settings::default();
    settings.dev_mode = false;
    settings.lc_requirement = get("lorule_castle_requirement") as u8;
    settings.yuganon_requirement = get("lorule_castle_requirement") as u8;
    settings.dark_rooms_lampless = on("dark_rooms_lampless");
    settings.dungeon_prize_shuffle = on("randomize_dungeon_prizes");
    settings.shuffle_maiamai_rewards = on("shuffle_maiamai_rewards");
    settings.maiamai_limit = get("maiamai_limit") as usize;
    settings.maiamai_madness = on("maiamai_mayhem");
    settings.lamp_and_net_as_weapons = on("lamp_and_net_as_weapons");
    settings.ravios_shop = RaviosShop::Open;
    settings.bow_of_light_in_castle = on("bow_of_light_in_castle");
    settings.no_progression_enemies = on("no_progression_enemies");
    settings.progressive_bow_of_light = false;
    settings.swordless_mode = on("swordless_mode");
    settings.start_with_merge = on("start_with_merge");
    settings.start_with_pouch = on("start_with_pouch");
    settings.bell_in_shop = on("bell_in_shop");
    settings.sword_in_shop = on("sword_in_shop");
    settings.boots_in_shop = on("boots_in_shop");
    settings.assured_weapon = on("assured_weapon");
    settings.chest_size_matches_contents = on("chest_size_matches_contents");
    settings.change_freestanding_models = on("change_freestanding_models");
    settings.minigames_excluded = on("minigames_excluded");
    settings.skip_big_bomb_flower = on("skip_big_bomb_flower");
    settings.treacherous_tower_floors = get("treacherous_tower_floors") as usize;
    settings.purple_potion_bottles = on("purple_potion_bottles");
    // Compasses: 0 own dungeons, 1 anywhere, 2 start with.
    settings.start_with_compasses = get("compasses") == 2;
    settings.night_mode = on("night_mode");
    settings.user_exclusions = Default::default();
    settings.logic_mode = match get("logic_mode") {
        1 => LogicMode::Hard,
        2 => LogicMode::Glitched,
        3 => LogicMode::AdvGlitched,
        4 => LogicMode::Hell,
        _ => LogicMode::Normal,
    };
    settings.ped_requirement = if get("pedestal_requirement") == 0 { PedestalSetting::Vanilla } else { PedestalSetting::Standard };
    settings.nice_items = match get("nice_items") {
        1 => NiceItems::Shuffled,
        2 => NiceItems::Off,
        3 => NiceItems::Upgrades,
        _ => NiceItems::Vanilla,
    };
    settings.super_items = match get("super_items") {
        0 => SuperItems::Shuffled,
        2 => SuperItems::Upgrades,
        _ => SuperItems::Off,
    };
    settings.cracks = match get("initial_crack_state") {
        0 => Cracks::Closed,
        2 => Cracks::Progressive,
        _ => Cracks::Open,
    };
    settings.cracksanity = match get("crack_shuffle") {
        1 => Cracksanity::CrossWorldPairs,
        2 => Cracksanity::AnyWorldPairs,
        3 => Cracksanity::MirroredCrossWorldPairs,
        4 => Cracksanity::MirroredAnyWorldPairs,
        _ => Cracksanity::Off,
    };
    settings.weather_vanes = match get("weather_vanes") {
        1 => WeatherVanes::Shuffled,
        2 => WeatherVanes::Convenient,
        3 => WeatherVanes::Hyrule,
        4 => WeatherVanes::Lorule,
        5 => WeatherVanes::All,
        _ => WeatherVanes::Standard,
    };
    // Keys: 0 own dungeons, 1 anywhere, 2 removed.
    settings.keysy = match (get("small_keys") == 2, get("big_keys") == 2) {
        (false, false) => Keysy::Off,
        (true, false) => Keysy::SmallKeysy,
        (false, true) => Keysy::BigKeysy,
        (true, true) => Keysy::AllKeysy,
    };
    settings.trials_door = if on("open_trials_door") {
        TrialsDoor::OpenFromBothSides
    } else {
        match get("trials_required") {
            0 => TrialsDoor::OpenFromInsideOnly,
            1 => TrialsDoor::OneTrialRequired,
            2 => TrialsDoor::TwoTrialsRequired,
            3 => TrialsDoor::ThreeTrialsRequired,
            _ => TrialsDoor::AllTrialsRequired,
        }
    };
    settings.hint_ghosts = match get("hint_ghosts") {
        0 => HintGhosts::Off,
        2 => HintGhosts::Glasses,
        _ => HintGhosts::Always,
    };
    settings
}

/// A few lines about the patch's options, for the app's confirm page.
pub fn describe(patch: &Patch) -> Vec<String> {
    let get = |key: &str| option(&patch.options, key);
    let logic = ["Normal", "Hard", "Glitched", "Advanced Glitched", "Hell"];
    let cracks = ["Off", "Cross World Pairs", "Any World Pairs", "Mirrored Cross World Pairs", "Mirrored Any World Pairs"];
    let mut lines = vec![
        format!("Logic: {}", logic.get(get("logic_mode") as usize).unwrap_or(&"Normal")),
        format!("Crack Shuffle: {}", cracks.get(get("crack_shuffle") as usize).unwrap_or(&"Off")),
    ];
    let extras: Vec<&str> = [
        ("start_with_merge", "Start with Merge"),
        ("start_with_pouch", "Start with Pouch"),
        ("bell_in_shop", "Bell in Shop"),
        ("sword_in_shop", "Sword in Shop"),
        ("boots_in_shop", "Boots in Shop"),
    ]
    .into_iter()
    .filter(|(key, _)| get(key) != 0)
    .map(|(_, name)| name)
    .collect();
    if !extras.is_empty() {
        lines.push(format!("3DS options: {}", extras.join(", ")));
    }
    lines
}

fn location_code(place: &str) -> Option<u16> {
    LOCATIONS.iter().find(|(name, _)| *name == place).and_then(|(_, code)| *code)
}

fn randomizable(place: &str, item: &str) -> Result<PyRandomizable, String> {
    if item == "AP Item" {
        let code = location_code(place).ok_or_else(|| format!("The patch puts another player's item at {place}, which has no location code."))?;
        return Ok(new_archipelago_item(code));
    }
    match ITEMS.iter().find(|(name, _)| *name == item).map(|(_, progress)| *progress) {
        Some(Progress::Item(item)) => Ok(new_item(item)),
        Some(Progress::Goal(goal)) => Ok(new_goal(goal)),
        Some(Progress::Vane(vane)) => Ok(new_vane(vane)),
        None => Err(format!("The patch names an item this app does not know: {item}")),
    }
}

/// Rebuilds the seed the host's computer made, ready for the patcher.
pub fn build(patch: &Patch, looks: Looks) -> Result<SeedInfo, String> {
    let mut items = DashMap::default();
    for (place, item) in &patch.items {
        let code = location_code(place).unwrap_or(0);
        items.insert(
            sanitize(place),
            ArchipelagoItem::new(item.name.clone(), item.player_name.clone(), item.classification, code),
        );
    }
    let info = ArchipelagoInfo { name: patch.player_name.clone(), items };
    let mut seed_info = randomizer::randomize_pre_fill(patch.seed, settings(&patch.options), Some(info));

    let mut check_map = DashMap::default();
    for (place, item) in &patch.check_map {
        check_map.insert(place.clone(), randomizable(place, item)?);
    }
    seed_info.build_layout(check_map);
    if patch.hints.len() != randomizer::hints::hint_ghost_count() {
        return Err(format!("The patch has {} hints, but the game has {} hint ghosts.", patch.hints.len(), randomizer::hints::hint_ghost_count()));
    }
    randomizer::hints::set_custom_hints(&mut seed_info, patch.hints.clone(), patch.bow_of_light_hint.clone());

    // Changed after the rebuild, so the seed hash still matches the host's.
    if let Some(on) = looks.night_mode {
        seed_info.settings.night_mode = on;
    }
    if let Some(on) = looks.change_freestanding_models {
        seed_info.settings.change_freestanding_models = on;
    }
    Ok(seed_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_outside_the_supported_range_are_refused() {
        let info = |version: &str| {
            format!(r#"{{"version":"{version}","seed":1,"player_name":"P","options":{{}},"check_map":{{}},"items":{{}},"hints":[],"bow_of_light_hint":""}}"#)
        };
        assert!(parse(info("0.4.0").as_bytes()).is_ok());
        assert!(parse(info("0.3.6").as_bytes()).unwrap_err().contains("0.4.0 or newer"));
        assert!(parse(info("0.5.0").as_bytes()).unwrap_err().contains("Update the app"));
    }

    #[test]
    fn sanitize_replaces_characters_the_font_lacks() {
        assert_eq!(sanitize("Link's Bow"), "Link's Bow");
        assert_eq!(sanitize("Bow \u{1F3F9}"), "Bow ?");
    }

    #[test]
    fn missing_options_use_the_apworld_defaults() {
        let settings = settings(&Map::new());
        assert_eq!(settings.lc_requirement, 7);
        assert_eq!(settings.maiamai_limit, 50);
        assert_eq!(settings.treacherous_tower_floors, 5);
        assert_eq!(settings.cracks, Cracks::Open);
        assert_eq!(settings.trials_door, TrialsDoor::AllTrialsRequired);
    }

    #[test]
    fn every_table_name_is_unique() {
        let mut names: Vec<_> = ITEMS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ITEMS.len());
    }

    /// Set `ALBW_AP_PATCH` to `.apalbw3ds` files, separated by commas, to rebuild real patches.
    #[test]
    fn rebuilds_real_patches_when_given() {
        let Ok(paths) = std::env::var("ALBW_AP_PATCH") else { return };
        for path in paths.split(',') {
            let patch = read(&std::fs::read(path).unwrap()).unwrap();
            let looks = Looks { night_mode: Some(true), change_freestanding_models: None };
            let seed_info = build(&patch, looks).unwrap();
            assert!(seed_info.settings.night_mode);
            let json = serde_json::to_value(&seed_info).unwrap();
            let placed: usize = json["layout"].as_object().unwrap().values()
                .flat_map(|regions| regions.as_object().unwrap().values())
                .map(|places| places.as_object().unwrap().len())
                .sum();
            assert!(placed > 400, "{path}: {placed} places");
            println!("{path}: seed {} hash {} with {placed} places", patch.seed, seed_info.hash.text());
        }
    }
}
