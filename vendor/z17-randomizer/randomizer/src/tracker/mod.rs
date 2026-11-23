//! Data for the QoL plugin's logic map: where each check is, how to tell it was checked, and the
//! seed's logic in a form the plugin can evaluate on the console.

use crate::filler::filler_item::{Item, Randomizable};
use crate::filler::item_pools;
use crate::filler::location::Location;
use crate::filler::progress::Progress;
use crate::SeedInfo;
use game::Course;
use modinfo::settings::cracksanity::Cracksanity;
use modinfo::settings::logic::LogicMode;
use std::collections::HashMap;
use std::sync::Arc;
use trace::{Diagram, NodeRef, Tracer, Var};

mod flags;
mod places;
pub mod trace;

/// The file the plugin reads, in the seed's folder beside `code.ips`.
pub const MAP_FILE: &str = "albw-map.bin";

const MAGIC: &[u8; 8] = b"ALBWMAP\0";
const VERSION: u32 = 2;
const NONE: u16 = 0xFFFF;

/// What a check is, for how the map shows it.
#[repr(u8)]
#[derive(Clone, Copy)]
enum Kind {
    Item = 0,
    /// A Maiamai spot that still holds a Maiamai, which the map leaves out.
    Maiamai = 1,
    /// Quest steps and other checks with no place on the map.
    Hidden = 2,
}

#[derive(Default)]
struct Items {
    list: Vec<Randomizable>,
    ids: HashMap<Randomizable, u16>,
    groups: Vec<Vec<u16>>,
    group_ids: HashMap<Vec<u16>, u16>,
}

impl Items {
    fn id(&mut self, item: Randomizable) -> u16 {
        if let Some(&id) = self.ids.get(&item) {
            return id;
        }
        let id = self.list.len() as u16;
        self.list.push(item);
        self.ids.insert(item, id);
        id
    }

    fn group(&mut self, items: &[Randomizable]) -> u16 {
        let ids: Vec<u16> = items.iter().map(|&item| self.id(item)).collect();
        if let Some(&id) = self.group_ids.get(&ids) {
            return id;
        }
        let id = self.groups.len() as u16;
        self.groups.push(ids.clone());
        self.group_ids.insert(ids, id);
        id
    }
}

fn node_ref(node: NodeRef) -> u16 {
    match node {
        trace::TRUE => 0xFFFF,
        trace::FALSE => 0xFFFE,
        node => u16::try_from(node).ok().filter(|&node| node < 0xFFF0).expect("the logic fits the map file"),
    }
}

/// The logic the map's yellow checks use: glitches up to advanced ones. Seeds that already allow
/// those have no yellow checks.
fn glitched_mode(seed_info: &SeedInfo) -> LogicMode {
    match seed_info.settings.logic_mode {
        LogicMode::Normal | LogicMode::Hard | LogicMode::Glitched => LogicMode::AdvGlitched,
        mode => mode,
    }
}

/// The checks in the order the map file lists them.
fn file_checks(seed_info: &SeedInfo) -> Vec<crate::filler::check::Check> {
    let graph = &seed_info.world_graph;
    let mut locations: Vec<Location> = graph.keys().copied().collect();
    locations.sort_by_key(|location| location.to_string());
    locations.iter().flat_map(|location| graph[location].get_checks().iter().flatten().copied().collect::<Vec<_>>()).collect()
}

/// What the plugin should show for each check in the map file, worked out with the randomizer's
/// own logic, for tests: 2 checked, 1 in logic, 0 out of logic. `checked` follows the file's order.
pub fn reference_status(seed_info: &SeedInfo, checked: &[bool]) -> Vec<u8> {
    let flags: HashMap<&str, (u8, u16)> = flags::FLAGS.iter().map(|&(name, course, flag)| (name, (course, flag))).collect();
    let checks = file_checks(seed_info);
    let normal = reachable_with(seed_info, checked, seed_info.settings.logic_mode);
    let glitched = reachable_with(seed_info, checked, glitched_mode(seed_info));
    checks
        .iter()
        .enumerate()
        .map(|(i, check)| {
            let name = check.get_name();
            if flags.contains_key(name) && checked[i] {
                2
            } else if normal.contains(name) {
                1
            } else if glitched.contains(name) {
                3
            } else {
                0
            }
        })
        .collect()
}

/// The checks reachable with `mode`, the way the plugin works it out.
fn reachable_with(seed_info: &SeedInfo, checked: &[bool], mode: LogicMode) -> std::collections::HashSet<&'static str> {
    let flags: HashMap<&str, (u8, u16)> = flags::FLAGS.iter().map(|&(name, course, flag)| (name, (course, flag))).collect();
    let resolve = |item: Randomizable| match item {
        Randomizable::Vane(vane) => seed_info.vane_map.get(&vane).map_or(item, |&to| Randomizable::Vane(to)),
        item => item,
    };
    let checks = file_checks(seed_info);
    let mut progress = Progress::new(seed_info).with_logic_mode(mode);
    loop {
        let reachable: std::collections::HashSet<&'static str> =
            crate::filler::find_reachable_checks(seed_info, &progress).iter().map(|check| check.get_name()).collect();
        let mut next = Progress::new(seed_info).with_logic_mode(mode);
        for (i, check) in checks.iter().enumerate() {
            let has_flag = flags.contains_key(check.get_name());
            let in_logic = reachable.contains(check.get_name());
            if (has_flag && checked[i]) || (!has_flag && in_logic) {
                if let Some(item) = check.get_location_info().and_then(|info| seed_info.layout.get(info.name, info.subregion)) {
                    next.add_item(resolve(item));
                }
            }
            if in_logic {
                if let Some(quest) = check.get_quest() {
                    next.add_item(resolve(quest));
                }
            }
        }
        if next.get_items() == progress.get_items() {
            return reachable;
        }
        progress = next;
    }
}

/// A made-up playthrough for tests: each step checks some of the checks in logic. Returns, for
/// each step, which checks are checked and what the plugin should show.
pub fn test_steps(seed_info: &SeedInfo, steps: usize) -> Vec<(Vec<bool>, Vec<u8>)> {
    let flags: HashMap<&str, (u8, u16)> = flags::FLAGS.iter().map(|&(name, course, flag)| (name, (course, flag))).collect();
    let checks = file_checks(seed_info);
    let mut checked = vec![false; checks.len()];
    let mut out = Vec::new();
    for step in 0..steps {
        let status = reference_status(seed_info, &checked);
        out.push((checked.clone(), status.clone()));
        for (i, check) in checks.iter().enumerate() {
            // Every third check in logic, shifting each step, so the steps differ.
            if status[i] == 1 && flags.contains_key(check.get_name()) && (i + step) % 3 == 0 {
                checked[i] = true;
            }
        }
    }
    out
}

/// Builds the plugin's map file for a seed: its checks with places, flags and items, the world
/// graph with each rule recorded as a decision diagram, and the seed's cracks.
pub fn map_data(seed_info: &SeedInfo) -> Vec<u8> {
    let tracer = Arc::new(Tracer::default());
    let progress = Progress::traced(seed_info, tracer.clone());
    let glitched = Progress::traced(seed_info, tracer.clone()).with_logic_mode(glitched_mode(seed_info));
    let mut diagram = Diagram::default();
    let mut items = Items::default();
    let resolve = |item: Randomizable| match item {
        Randomizable::Vane(vane) => seed_info.vane_map.get(&vane).map_or(item, |&to| Randomizable::Vane(to)),
        item => item,
    };
    let to_items = |list: Vec<Item>| list.into_iter().map(Randomizable::from).collect::<Vec<_>>();
    let heart_groups = [
        items.group(&to_items(item_pools::get_heart_containers())),
        items.group(&to_items(item_pools::get_heart_pieces())),
    ];
    let rupee_groups = [
        items.group(&to_items(item_pools::get_purple_rupee_pool())),
        items.group(&to_items(item_pools::get_silver_rupee_pool())),
        items.group(&to_items(item_pools::get_gold_rupee_pool())),
    ];
    let places: HashMap<&str, (u8, u8, u8, u8, u8)> =
        places::PLACES.iter().map(|&(name, map, x, y, course, stage)| (name, (map, x, y, course, stage))).collect();
    let flags: HashMap<&str, (u8, u16)> = flags::FLAGS.iter().map(|&(name, course, flag)| (name, (course, flag))).collect();

    let graph = &seed_info.world_graph;
    let mut locations: Vec<Location> = graph.keys().copied().collect();
    locations.sort_by_key(|location| location.to_string());
    let index: HashMap<Location, u16> = locations.iter().enumerate().map(|(i, &location)| (location, i as u16)).collect();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut checks = Vec::new();
    let mut names = Vec::new();
    for location in &locations {
        let node = &graph[location];
        let (first_edge, first_check) = (edges.len(), checks.len());
        for path in node.get_paths().iter().flatten() {
            let logic = path.get_logic();
            let rule = diagram.record(&tracer, &|| logic.can_access(&progress));
            let glitch_rule = diagram.record(&tracer, &|| logic.can_access(&glitched));
            edges.push((index[&path.get_destination()], node_ref(rule), node_ref(glitch_rule)));
        }
        for check in node.get_checks().iter().flatten() {
            let logic = check.get_logic();
            let rule = diagram.record(&tracer, &|| logic.can_access(&progress));
            let glitch_rule = diagram.record(&tracer, &|| logic.can_access(&glitched));
            let name = check.get_name();
            let placed = check.get_location_info().and_then(|info| seed_info.layout.get(info.name, info.subregion));
            let item = placed.map(resolve).map_or(NONE, |item| items.id(item));
            let quest = check.get_quest().map(resolve).map_or(NONE, |item| items.id(item));
            let (map, x, y, course, stage) = places.get(name).copied().unwrap_or((0xFF, 0, 0, 0xFF, 0));
            let (flag_course, flag) = flags.get(name).copied().map_or((0xFE, 0), |(course, flag)| (course, flag));
            // Weather Vanes, shop shelves and dungeon prizes have no flag to show they are done.
            let unmarked = name.ends_with("Weather Vane") || name.contains("Item Shop") || name.ends_with("] Prize");
            let kind = if map == 0xFF || unmarked {
                Kind::Hidden
            } else if name.starts_with("[Mai]") && matches!(placed, Some(Randomizable::Item(item)) if format!("{item:?}").starts_with("Maiamai")) {
                Kind::Maiamai
            } else {
                Kind::Item
            };
            let name_at = names.len() as u32;
            names.extend_from_slice(name.as_bytes());
            names.push(0);
            checks.push((node_ref(rule), item, quest, flag_course, flag, map, x, y, course, stage, kind as u8, name_at, node_ref(glitch_rule)));
        }
        nodes.push((first_edge as u16, (edges.len() - first_edge) as u16, first_check as u16, (checks.len() - first_check) as u16));
    }

    // Cracks, where they lead, and the stages they join, so the plugin can see the player use one.
    let mut cracks = Vec::new();
    if seed_info.settings.cracksanity != Cracksanity::Off {
        let crack_places: HashMap<String, (u8, u8, u8, u8, u8)> = crate::regions::locations()
            .into_iter()
            .filter(|(_, _, _, patch)| patch.starts_with("Crack"))
            .filter_map(|(_, _, name, patch)| {
                let crack = patch.trim_end_matches(')').split_whitespace().last()?.to_owned();
                Some((crack, *places.get(name)?))
            })
            .collect();
        for (from, to) in &seed_info.crack_map {
            if let (Some(a), Some(b)) = (crack_places.get(&format!("{from:?}")), crack_places.get(&format!("{to:?}"))) {
                cracks.push((*a, *b));
            }
        }
    }

    // The items each question needs, now that every question is known.
    let vars: Vec<(u8, u16, u16)> = diagram
        .vars
        .clone()
        .into_iter()
        .map(|var| match var {
            Var::Has(item) => (0, items.id(item), 0),
            Var::AtLeast(list, amount) => (1, items.group(&list), amount as u16),
            Var::Hearts(quarters) => (2, 0, quarters),
            Var::Rupees(amount) => (3, 0, amount),
        })
        .collect();

    let mut out = Vec::new();
    let u16s = |out: &mut Vec<u8>, values: &[u16]| values.iter().for_each(|value| out.extend(value.to_le_bytes()));
    let u32s = |out: &mut Vec<u8>, values: &[u32]| values.iter().for_each(|value| out.extend(value.to_le_bytes()));
    out.extend(MAGIC);
    let group_items: usize = items.groups.iter().map(Vec::len).sum();
    u32s(
        &mut out,
        &[
            VERSION,
            items.list.len() as u32,
            items.groups.len() as u32,
            group_items as u32,
            vars.len() as u32,
            diagram.nodes.len() as u32,
            nodes.len() as u32,
            edges.len() as u32,
            checks.len() as u32,
            cracks.len() as u32,
            names.len() as u32,
            index[&Location::RavioShop] as u32,
        ],
    );
    u16s(&mut out, &[heart_groups[0], heart_groups[1], rupee_groups[0], rupee_groups[1], rupee_groups[2], 0]);
    // Groups: start and length in the list of group items, then that list.
    let mut start = 0u16;
    for group in &items.groups {
        u16s(&mut out, &[start, group.len() as u16]);
        start += group.len() as u16;
    }
    for group in &items.groups {
        u16s(&mut out, group);
    }
    for &(kind, a, b) in &vars {
        out.extend([kind, 0]);
        u16s(&mut out, &[a, b]);
    }
    for &(var, no, yes) in &diagram.nodes {
        u16s(&mut out, &[var as u16, node_ref(no), node_ref(yes)]);
    }
    for &(first_edge, edge_count, first_check, check_count) in &nodes {
        u16s(&mut out, &[first_edge, edge_count, first_check, check_count]);
    }
    for &(destination, rule, glitch_rule) in &edges {
        u16s(&mut out, &[destination, rule, glitch_rule]);
    }
    for &(rule, item, quest, flag_course, flag, map, x, y, course, stage, kind, name_at, glitch_rule) in &checks {
        u16s(&mut out, &[rule, item, quest, flag]);
        out.extend([flag_course, map, x, y, course, stage, kind, 0]);
        u32s(&mut out, &[name_at]);
        u16s(&mut out, &[glitch_rule]);
    }
    for &(a, b) in &cracks {
        out.extend([a.0, a.1, a.2, a.3, a.4, b.0, b.1, b.2, b.3, b.4, 0, 0]);
    }
    out.extend(&names);
    out
}

/// Where a location's patch puts it: the course, the stage as numbered in the region files
/// (starting at 1), and the object's unique id. None for patches without one place, such as
/// events.
pub fn patch_place(patch: &str, region_course: Course) -> Option<(Course, u16, u16)> {
    let (_, args) = patch.split_once('(')?;
    let mut words = args.split(|c: char| c.is_whitespace() || c == '[' || c == ']').filter(|word| !word.is_empty());
    let first = words.next()?;
    let (course, stage) = match first.parse::<u16>() {
        Ok(stage) => (region_course, stage),
        Err(_) => (course_named(first)?, words.next()?.parse().ok()?),
    };
    let unq = words.next()?.parse().ok()?;
    Some((course, stage, unq))
}

fn course_named(name: &str) -> Option<Course> {
    Course::iter().find(|course| format!("{course:?}") == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use modinfo::settings::cracks::Cracks;
    use modinfo::Settings;
    use rand::{rngs::StdRng, Rng, SeedableRng};

    fn settings(cracksanity: Cracksanity) -> Settings {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        settings.cracksanity = cracksanity;
        if cracksanity != Cracksanity::Off {
            settings.cracks = Cracks::Open;
        }
        settings.yuganon_requirement = settings.lc_requirement;
        settings
    }

    /// Every recorded rule gives the same answer as the rule itself, for random sets of items.
    #[test]
    fn recorded_rules_match_the_logic() {
        let seed_info = crate::build_seed(5, settings(Cracksanity::Off)).unwrap();
        let tracer = Arc::new(Tracer::default());
        let traced = Progress::traced(&seed_info, tracer.clone());
        let mut diagram = Diagram::default();
        let mut rules = Vec::new();
        let started = std::time::Instant::now();
        for node in seed_info.world_graph.values() {
            for path in node.get_paths().iter().flatten() {
                let logic = path.get_logic();
                rules.push((diagram.record(&tracer, &|| logic.can_access(&traced)), logic));
            }
            for check in node.get_checks().iter().flatten() {
                let logic = check.get_logic();
                rules.push((diagram.record(&tracer, &|| logic.can_access(&traced)), logic));
            }
        }
        println!("{} rules, {} nodes, {} questions, {:?}", rules.len(), diagram.nodes.len(), diagram.vars.len(), started.elapsed());
        let all: Vec<Randomizable> = diagram
            .vars
            .iter()
            .flat_map(|var| match var {
                Var::Has(item) => vec![*item],
                Var::AtLeast(list, _) => list.clone(),
                _ => Vec::new(),
            })
            .chain(item_pools::get_heart_pieces().into_iter().map(Into::into))
            .chain(item_pools::get_heart_containers().into_iter().map(Into::into))
            .chain(item_pools::get_silver_rupee_pool().into_iter().map(Into::into))
            .collect();
        let mut rng = StdRng::seed_from_u64(1);
        for round in 0..200 {
            let share = round as f64 / 200.0;
            let mut progress = Progress::new(&seed_info);
            for &item in &all {
                if rng.gen_bool(share) {
                    progress.add_item(item);
                }
            }
            let answer = |var: &Var| match var {
                Var::Has(item) => progress.has(*item),
                Var::AtLeast(list, amount) => list.iter().filter(|&&item| progress.has(item)).count() >= *amount as usize,
                Var::Hearts(quarters) => progress.hearts(*quarters as f32 / 4.0),
                Var::Rupees(amount) => progress.has_rupees(*amount),
            };
            for (node, logic) in &rules {
                if *node == trace::TRUE && !logic.can_access(&progress) {
                    continue; // a rule too big to record, kept as passing
                }
                assert_eq!(diagram.eval(*node, &answer), logic.can_access(&progress));
            }
        }
    }

    /// Lists what a seed puts on loose items (hearts, keys, rupees), for checking model crashes.
    #[test]
    #[ignore]
    fn print_freestanding_items() {
        let seed: u32 = std::env::var("SEED").ok().and_then(|seed| seed.parse().ok()).unwrap_or(4);
        // SETTINGS may hold the seed's settings as JSON.
        let mut settings = match std::env::var("SETTINGS") {
            Ok(text) => serde_json::from_str(&text).unwrap(),
            Err(_) => settings(Cracksanity::Off),
        };
        settings.change_freestanding_models = true;
        let seed_info = crate::build_seed(seed, settings).unwrap();
        for (_, _, name, patch) in crate::regions::locations() {
            if patch.starts_with("Heart") || patch.starts_with("Key") || patch.starts_with("SilverRupee") || patch.starts_with("GoldRupee") {
                println!("{name}: {:?}", seed_info.layout.get_by_name(name));
            }
        }
    }

    #[test]
    fn map_file_has_checks_and_cracks() {
        let seed_info = crate::build_seed(7, settings(Cracksanity::AnyWorldPairs)).unwrap();
        let data = map_data(&seed_info);
        assert_eq!(&data[..8], MAGIC);
        let word = |i: usize| u32::from_le_bytes(data[8 + i * 4..12 + i * 4].try_into().unwrap());
        println!("map file {} bytes, {} checks, {} cracks", data.len(), word(8), word(9));
        assert!(word(8) > 300);
        assert!(word(9) > 20);
    }

    #[test]
    fn reads_patch_places() {
        assert_eq!(patch_place("Heart (17 [95])", Course::FieldLight), Some((Course::FieldLight, 17, 95)));
        assert_eq!(patch_place("Chest (IndoorLight 19 [12])", Course::FieldLight), Some((Course::IndoorLight, 19, 12)));
        assert_eq!(patch_place("Crack (27 [68] YourHouse)", Course::FieldLight), Some((Course::FieldLight, 27, 68)));
        assert_eq!(patch_place("Event (IndoorLight / FieldLight_22_BlackSmith [0x16])", Course::FieldLight), None);
    }
}
