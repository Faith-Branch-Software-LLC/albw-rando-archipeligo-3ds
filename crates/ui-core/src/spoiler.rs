//! Reads the randomizer's spoiler log into flat entries the menu can list and search.
//!
//! The log is the JSON that `SeedInfo` serializes to. Each entry is a place, what it holds, and the
//! group it belongs to, such as a region or a playthrough sphere.

use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub group: String,
    pub place: String,
    pub item: String,
}

#[derive(Clone, Debug, Default)]
pub struct Spoiler {
    pub seed: u32,
    pub hash: String,
    pub entries: Vec<Entry>,
}

impl Spoiler {
    /// Indices of the entries that hold every word of `query`, ignoring case. An empty query finds
    /// every entry.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let text = format!("{} {} {}", entry.group, entry.place, entry.item).to_lowercase();
                words.iter().all(|word| text.contains(word.as_str()))
            })
            .map(|(index, _)| index)
            .collect()
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(text).collect::<Vec<_>>().join(", "),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn object<'a>(value: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    value.get(key).and_then(Value::as_object)
}

fn push(entries: &mut Vec<Entry>, group: impl Into<String>, place: impl Into<String>, item: impl Into<String>) {
    entries.push(Entry { group: group.into(), place: place.into(), item: item.into() });
}

/// Adds each pair of a map whose two sides differ. Unshuffled cracks and vanes map to themselves.
fn pairs(entries: &mut Vec<Entry>, group: &str, map: Option<&Map<String, Value>>) {
    for (place, item) in map.into_iter().flatten() {
        let item = text(item);
        if *place != item {
            push(entries, group, place.as_str(), item);
        }
    }
}

pub fn parse(json: &str) -> Result<Spoiler, String> {
    let log: Value = serde_json::from_str(json).map_err(|err| format!("This is not a spoiler log: {err}"))?;
    let layout = object(&log, "layout").ok_or("This file has no item layout, so it is not a spoiler log.")?;
    let mut entries = Vec::new();

    // Item locations, by world and region.
    for (world, regions) in layout {
        for (region, places) in regions.as_object().into_iter().flatten() {
            for (place, item) in places.as_object().into_iter().flatten() {
                push(&mut entries, format!("{world}: {region}"), place.as_str(), text(item));
            }
        }
    }

    // The log's playthrough spheres and hint texts are left out: the list is for finding where
    // an item is, and those entries would repeat places or fill the item picker with sentences.

    pairs(&mut entries, "Cracks", object(&log, "crack_map"));
    pairs(&mut entries, "Weather Vanes", object(&log, "weather_vane_map"));

    for (number, floors) in log.get("treacherous_tower_floors").and_then(Value::as_array).into_iter().flatten().enumerate() {
        push(&mut entries, "Treacherous Tower", format!("Floor Set {}", number + 1), text(floors).split_whitespace().collect::<Vec<_>>().join(" "));
    }
    for (trial, on) in object(&log, "trials_config").into_iter().flatten() {
        let name = trial.split('_').map(|word| word[..1].to_uppercase() + &word[1..]).collect::<Vec<_>>().join(" ");
        push(&mut entries, "Lorule Castle Trials", name, if on.as_bool() == Some(true) { "Required" } else { "Skipped" });
    }

    if entries.is_empty() {
        return Err("This spoiler log lists no items.".to_owned());
    }
    Ok(Spoiler {
        seed: log.get("seed").and_then(Value::as_u64).unwrap_or(0) as u32,
        hash: log.get("hash").map(text).unwrap_or_default(),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "seed": 1234,
        "hash": "(B) (Y) (Bow)",
        "layout": {
            "Hyrule": { "Death Mountain": { "Bouldering Guy": "Heart Container", "Vane": "Vane" } },
            "Dungeons": { "Eastern Palace": { "[EP] (1F) Entrance": "Bow+" } }
        },
        "metrics": { "playthrough": { "Sphere 00": { "items": { "Ravio's Gift": "Glove+" }, "goals": {} } } },
        "hints": {
            "path_hints": [{ "hint": "Kakariko is on the path to Zaganaga", "ghosts": ["Ghost A", "Ghost B"] }],
            "bow_of_light_hint": null
        },
        "crack_map": { "A Crack": "A Crack", "B Crack": "C Crack" },
        "trials_config": { "tile_trial": true, "bomb_trial": false }
    }"#;

    #[test]
    fn reads_each_kind_of_entry() {
        let spoiler = parse(SAMPLE).unwrap();
        assert_eq!(spoiler.seed, 1234);
        assert_eq!(spoiler.hash, "(B) (Y) (Bow)");
        let has = |group: &str, place: &str, item: &str| {
            spoiler.entries.iter().any(|entry| entry.group == group && entry.place == place && entry.item == item)
        };
        assert!(has("Hyrule: Death Mountain", "Bouldering Guy", "Heart Container"));
        assert!(has("Dungeons: Eastern Palace", "[EP] (1F) Entrance", "Bow+"));
        // Playthrough spheres and hints are not listed.
        assert!(!spoiler.entries.iter().any(|entry| entry.group.starts_with("Playthrough") || entry.group.ends_with("Hints")));
        assert!(has("Cracks", "B Crack", "C Crack"));
        assert!(has("Lorule Castle Trials", "Tile Trial", "Required"));
        // Cracks that lead to themselves are not news.
        assert!(!spoiler.entries.iter().any(|entry| entry.place == "A Crack"));
    }

    #[test]
    fn search_needs_every_word_and_ignores_case() {
        let spoiler = parse(SAMPLE).unwrap();
        let found = |query: &str| spoiler.search(query).into_iter().map(|index| spoiler.entries[index].place.clone()).collect::<Vec<_>>();
        assert_eq!(found("bow eastern"), ["[EP] (1F) Entrance"]);
        assert_eq!(found("HEART"), ["Bouldering Guy"]);
        assert!(found("nothing like this").is_empty());
        assert_eq!(spoiler.search("").len(), spoiler.entries.len());
    }

    #[test]
    fn rejects_other_files() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"seed": 1}"#).is_err());
    }

    /// Set `ALBW_SPOILER` to a log the randomizer wrote to check the real format.
    #[test]
    fn reads_a_real_log_when_one_is_given() {
        let Ok(path) = std::env::var("ALBW_SPOILER") else { return };
        let spoiler = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(spoiler.entries.len() > 200, "{} entries", spoiler.entries.len());
        assert!(!spoiler.hash.is_empty());
        assert!(!spoiler.entries.iter().any(|entry| entry.group.starts_with("Playthrough")));
    }
}
