//! Rules for sharing a game's mod slots with ModMoon.
//!
//! When ModMoon closes, it renames the selected slot to `/luma/titles/<title id>`, and it renames
//! that folder back when it opens. So the selected slot's number looks free while ModMoon is
//! closed, but putting anything there makes both renames fail. ModMoon also stops counting slots
//! at the first missing number, apart from the selected one, so new slots must not leave a gap.

/// The folder that ModMoon uses when its settings do not name one.
pub const DEFAULT_MODS_FOLDER: &str = "/3ds/ModMoon/";

/// Reads a `Name{value}` line from ModMoon's `settings.txt`.
fn value<'a>(settings: &'a str, name: &str) -> Option<&'a str> {
    let start = settings.find(&format!("{name}{{"))? + name.len() + 1;
    let rest = &settings[start..];
    Some(&rest[..rest.find('}')?])
}

/// The folder that holds ModMoon's title folders, ending in `/`.
pub fn mods_folder(settings: &str) -> String {
    let folder = value(settings, "ModsFolder").map(str::trim).filter(|folder| !folder.is_empty());
    let folder = folder.unwrap_or(DEFAULT_MODS_FOLDER);
    if folder.ends_with('/') {
        folder.to_owned()
    } else {
        format!("{folder}/")
    }
}

/// The slot of `title_id` that ModMoon has moved to `/luma/titles`, if any.
pub fn active_slot(settings: &str, title_id: u64) -> Option<u32> {
    if value(settings, "ModsEnabled")? != "True" {
        return None;
    }
    let position: usize = value(settings, "SelectedTitleIDPos")?.trim().parse().ok()?;
    let title = value(settings, "ActiveTitleIDs")?.split(", ").nth(position)?;
    if u64::from_str_radix(title.trim(), 16).ok()? != title_id {
        return None;
    }
    let slot: u32 = value(settings, "TitleIDSlots")?.split(", ").nth(position)?.trim().parse().ok()?;
    (slot > 0).then_some(slot)
}

/// The lowest slot number that has no folder and is not the slot moved out by ModMoon.
pub fn next_free_slot(active: Option<u32>, exists: impl Fn(u32) -> bool) -> u32 {
    (1u32..).find(|&slot| Some(slot) != active && !exists(slot)).expect("a free slot exists")
}

/// The contents of a slot's `desc.txt`. ModMoon shows only the first line and squeezes long text
/// to fit, so the first line stays short and the details go on the lines after it.
pub fn slot_description(seed: u32, preset: &str, hash: &str, mods: &[String]) -> String {
    let mut text = format!("Seed {seed}\nALBW Randomizer\nPreset: {preset}\nHash: {hash}\n");
    for line in mods {
        text.push_str(line);
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const TITLE: u64 = 0x0004_0000_000E_C300;

    fn settings(enabled: &str, position: u32, slots: &str) -> String {
        format!(
            "ModsEnabled{{{enabled}}}\nModsFolder{{/3ds/ModMoon/}}\nActiveTitleIDs{{0, 40000000ec300, 40000000b8b00}}\n\
             TitleIDSlots{{{slots}}}\nSelectedTitleIDPos{{{position}}}\n"
        )
    }

    #[test]
    fn finds_the_selected_slot() {
        assert_eq!(active_slot(&settings("True", 1, "0, 3, 2"), TITLE), Some(3));
    }

    #[test]
    fn no_slot_is_active_when_mods_are_off_or_another_game_is_selected() {
        assert_eq!(active_slot(&settings("False", 1, "0, 3, 2"), TITLE), None);
        assert_eq!(active_slot(&settings("True", 2, "0, 3, 2"), TITLE), None);
        assert_eq!(active_slot(&settings("True", 1, "0, 0, 2"), TITLE), None);
        assert_eq!(active_slot("", TITLE), None);
    }

    #[test]
    fn never_uses_the_active_slot() {
        // Slots 1 and 2 are in the folder, and slot 3 is in /luma/titles.
        assert_eq!(next_free_slot(Some(3), |slot| slot <= 2), 4);
        // Slot 4 also exists, past the moved slot.
        assert_eq!(next_free_slot(Some(3), |slot| slot <= 2 || slot == 4), 5);
    }

    #[test]
    fn fills_the_first_gap_when_no_slot_is_active() {
        assert_eq!(next_free_slot(None, |_| false), 1);
        assert_eq!(next_free_slot(None, |slot| slot <= 2), 3);
    }

    #[test]
    fn reads_the_mods_folder() {
        assert_eq!(mods_folder("ModsFolder{/mods}"), "/mods/");
        assert_eq!(mods_folder("ModsFolder{/3ds/ModMoon/}"), "/3ds/ModMoon/");
        assert_eq!(mods_folder(""), DEFAULT_MODS_FOLDER);
    }

    #[test]
    fn first_line_of_the_description_is_short() {
        let text = slot_description(u32::MAX, "A very long preset name indeed", "(Ravio) (L) (Ravio) (R) (Ravio)", &["Model: Wolf".to_owned()]);
        assert!(text.ends_with("Model: Wolf\n"));
        assert_eq!(text.lines().next(), Some("Seed 4294967295"));
    }
}
