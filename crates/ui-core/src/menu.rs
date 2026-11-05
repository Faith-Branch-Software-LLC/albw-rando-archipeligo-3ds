//! Menu screens, focus, buttons, and touch. The app feeds in input and draws the returned view.

use crate::presets::Preset;
use crate::rules::{OptionDef, Rules, CATEGORIES, OPTIONS};

pub const ROW_TOP: f32 = 8.0;
pub const ROW_HEIGHT: f32 = 28.0;
pub const ROW_GAP: f32 = 3.0;
pub const VISIBLE_ROWS: usize = 6;
pub const ROW_LEFT: f32 = 8.0;
pub const ROW_WIDTH: f32 = 304.0;
/// Touches on a row at or right of this x change its value instead of only opening it.
pub const VALUE_LEFT: f32 = 190.0;
pub const VALUE_CENTER: f32 = 250.0;

const REPEAT_DELAY: u32 = 20;
const REPEAT_EVERY: u32 = 5;
const DRAG_START: f32 = 6.0;
const HELP_WIDTH: usize = 56;

pub const COMPUTER_INSTRUCTIONS: &str = "github.com/rickfay/z17-randomizer";

#[derive(Clone, Copy, Debug, Default)]
pub struct Buttons {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub a: bool,
    pub b: bool,
    pub l: bool,
    pub r: bool,
    pub start: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// Buttons that went down this frame.
    pub pressed: Buttons,
    /// Buttons that are down now.
    pub held: Buttons,
    /// Where the screen is touched now.
    pub touch: Option<(f32, f32)>,
}

/// What the app must do after an update.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Exit,
    /// Open the number keyboard for the seed.
    EditSeed,
    /// Pick a new random seed.
    RandomSeed,
    /// Open the keyboard to name a preset, then save it.
    SavePreset,
    /// Start making the seed.
    Generate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    OldWarning,
    Home,
    Categories,
    Options(usize),
    Presets,
    Confirm,
    Message,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Open(Page),
    Option(usize),
    Preset(usize),
    SavePreset,
    EditSeed,
    RandomSeed,
    Generate,
    Back,
    Exit,
}

#[derive(Clone, Debug)]
struct Row {
    label: String,
    value: Option<String>,
    help: String,
    target: Target,
}

#[derive(Clone, Copy, Debug)]
struct Place {
    page: Page,
    focus: usize,
    scroll: usize,
}

/// One row for the app to draw.
#[derive(Clone, Debug)]
pub struct RowView {
    pub label: String,
    pub value: Option<String>,
    pub focused: bool,
    /// The row changes a value when moved left or right.
    pub adjustable: bool,
}

/// Everything the app needs to draw a frame.
#[derive(Clone, Debug)]
pub struct View {
    pub title: String,
    pub subtitle: String,
    pub body: Vec<String>,
    pub rows: Vec<RowView>,
    pub hint: String,
    /// Scroll position and total rows, for the scroll bar.
    pub scroll: usize,
    pub total_rows: usize,
}

#[derive(Clone, Copy, Debug)]
struct Touch {
    start: (f32, f32),
    last: (f32, f32),
    dragging: bool,
    scroll_at_start: usize,
}

pub struct Menu {
    stack: Vec<Place>,
    pub rules: Rules,
    pub seed: u32,
    pub preset_name: String,
    presets: Vec<Preset>,
    message: (String, Vec<String>),
    held_frames: u32,
    touch: Option<Touch>,
}

/// Splits text into lines of at most `width` characters, at spaces.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

impl Menu {
    /// `old_3ds` starts on a warning about what that console can do.
    pub fn new(seed: u32, presets: Vec<Preset>, old_3ds: bool) -> Self {
        let mut stack = vec![Place { page: Page::Home, focus: 0, scroll: 0 }];
        if old_3ds {
            stack.push(Place { page: Page::OldWarning, focus: 0, scroll: 0 });
        }
        Self {
            stack,
            rules: Rules::default(),
            seed,
            preset_name: "Standard".to_owned(),
            presets,
            message: (String::new(), Vec::new()),
            held_frames: 0,
            touch: None,
        }
    }

    pub fn page(&self) -> Page {
        self.place().page
    }

    fn place(&self) -> &Place {
        self.stack.last().expect("the home page is never removed")
    }

    fn place_mut(&mut self) -> &mut Place {
        self.stack.last_mut().expect("the home page is never removed")
    }

    pub fn set_presets(&mut self, presets: Vec<Preset>) {
        self.presets = presets;
    }

    /// Opens a message page on top of the current page.
    pub fn show_message(&mut self, title: &str, text: &str) {
        self.message = (title.to_owned(), wrap(text, HELP_WIDTH));
        self.open(Page::Message);
    }

    /// Returns to the home page, for after a run.
    pub fn go_home(&mut self) {
        self.stack.truncate(1);
    }

    fn open(&mut self, page: Page) {
        self.stack.push(Place { page, focus: 0, scroll: 0 });
    }

    fn back(&mut self) -> Action {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
        Action::None
    }

    fn option_row(&self, index: usize, option: &OptionDef) -> Row {
        Row {
            label: option.label.to_owned(),
            value: Some(self.rules.value_text(option)),
            help: option.help.to_owned(),
            target: Target::Option(index),
        }
    }

    fn rows(&self) -> Vec<Row> {
        let row = |label: &str, value: Option<String>, help: &str, target: Target| Row {
            label: label.to_owned(),
            value,
            help: help.to_owned(),
            target,
        };
        match self.page() {
            Page::OldWarning => vec![
                row("Continue anyway", None, "", Target::Back),
                row("Exit", None, "", Target::Exit),
            ],
            Page::Home => vec![
                row("Make a Seed", None, "Review your rules, then build a randomized game.", Target::Open(Page::Confirm)),
                row("Rules", None, "Change every option by hand.", Target::Open(Page::Categories)),
                row("Presets", Some(self.preset_name.clone()), "Load a set of rules, or save your own to share.", Target::Open(Page::Presets)),
                row("Seed Number", Some(self.seed.to_string()), "The same number and rules make the same game on this console. Press A to type one.", Target::EditSeed),
                row("New Random Seed", None, "Picks a new seed number.", Target::RandomSeed),
                row("About and Credits", None, "Who made this possible.", Target::Open(Page::About)),
                row("Exit", None, "Return to the Homebrew Launcher.", Target::Exit),
            ],
            Page::Categories => CATEGORIES
                .iter()
                .enumerate()
                .map(|(index, (name, help))| row(name, None, help, Target::Open(Page::Options(index))))
                .collect(),
            Page::Options(category) => OPTIONS
                .iter()
                .enumerate()
                .filter(|(_, option)| option.category == category)
                .map(|(index, option)| self.option_row(index, option))
                .collect(),
            Page::Presets => {
                let mut rows: Vec<Row> = self
                    .presets
                    .iter()
                    .enumerate()
                    .map(|(index, preset)| {
                        let marker = if preset.built_in { None } else { Some("SD card".to_owned()) };
                        row(&preset.name, marker, &preset.description, Target::Preset(index))
                    })
                    .collect();
                rows.push(row("Save Current Rules", None, "Saves to /3ds/albw-rando/presets. Share that file, and others can copy it to the same folder.", Target::SavePreset));
                rows
            }
            Page::Confirm => vec![
                row("Start", None, "", Target::Generate),
                row("Go Back", None, "", Target::Back),
            ],
            Page::Message | Page::About => vec![row("OK", None, "", Target::Back)],
        }
    }

    fn adjust(&mut self, index: usize, delta: i32) {
        self.rules.step(&OPTIONS[index], delta);
        self.preset_name = "Custom".to_owned();
    }

    fn activate(&mut self, target: Target) -> Action {
        match target {
            Target::Open(page) => {
                self.open(page);
                Action::None
            }
            Target::Option(index) => {
                self.adjust(index, 1);
                Action::None
            }
            Target::Preset(index) => {
                let preset = &self.presets[index];
                self.rules = Rules::with_overrides(&preset.overrides);
                self.preset_name = preset.name.clone();
                self.back()
            }
            Target::SavePreset => Action::SavePreset,
            Target::EditSeed => Action::EditSeed,
            Target::RandomSeed => Action::RandomSeed,
            Target::Generate => Action::Generate,
            Target::Back => self.back(),
            Target::Exit => Action::Exit,
        }
    }

    fn move_focus(&mut self, delta: i32, count: usize) {
        if count == 0 {
            return;
        }
        let place = self.place_mut();
        place.focus = (place.focus as i32 + delta).rem_euclid(count as i32) as usize;
        if place.focus < place.scroll {
            place.scroll = place.focus;
        } else if place.focus >= place.scroll + VISIBLE_ROWS {
            place.scroll = place.focus + 1 - VISIBLE_ROWS;
        }
    }

    /// True on the first frame of a press, then again at a steady rate while held.
    fn repeats(pressed: bool, held: bool, held_frames: u32) -> bool {
        pressed || (held && held_frames >= REPEAT_DELAY && (held_frames - REPEAT_DELAY) % REPEAT_EVERY == 0)
    }

    fn row_at(place: &Place, count: usize, x: f32, y: f32) -> Option<usize> {
        if !(ROW_LEFT..ROW_LEFT + ROW_WIDTH).contains(&x) || y < ROW_TOP {
            return None;
        }
        let slot = ((y - ROW_TOP) / (ROW_HEIGHT + ROW_GAP)) as usize;
        let inside = (y - ROW_TOP) % (ROW_HEIGHT + ROW_GAP) < ROW_HEIGHT;
        let index = place.scroll + slot;
        (slot < VISIBLE_ROWS && inside && index < count).then_some(index)
    }

    fn update_touch(&mut self, touch: Option<(f32, f32)>, rows: &[Row]) -> Action {
        match (touch, self.touch) {
            (Some(at), None) => {
                self.touch = Some(Touch { start: at, last: at, dragging: false, scroll_at_start: self.place().scroll });
                Action::None
            }
            (Some(at), Some(mut state)) => {
                state.last = at;
                let moved = at.1 - state.start.1;
                state.dragging |= moved.abs() >= DRAG_START;
                if state.dragging {
                    let rows_moved = (moved / (ROW_HEIGHT + ROW_GAP)) as i32;
                    let most = rows.len().saturating_sub(VISIBLE_ROWS) as i32;
                    let place = self.place_mut();
                    place.scroll = (state.scroll_at_start as i32 - rows_moved).clamp(0, most) as usize;
                    place.focus = place.focus.clamp(place.scroll, place.scroll + VISIBLE_ROWS - 1).min(rows.len().saturating_sub(1));
                }
                self.touch = Some(state);
                Action::None
            }
            (None, Some(state)) => {
                self.touch = None;
                if state.dragging {
                    return Action::None;
                }
                let (x, y) = state.last;
                let Some(index) = Self::row_at(self.place(), rows.len(), x, y) else {
                    return Action::None;
                };
                self.place_mut().focus = index;
                match rows[index].target {
                    // Left of the value's center steps back, and the rest steps forward.
                    Target::Option(option) => {
                        let delta = if (VALUE_LEFT..VALUE_CENTER).contains(&x) { -1 } else { 1 };
                        self.adjust(option, delta);
                        Action::None
                    }
                    target => self.activate(target),
                }
            }
            (None, None) => Action::None,
        }
    }

    pub fn update(&mut self, input: Input) -> Action {
        let rows = self.rows();
        let count = rows.len();
        let (pressed, held) = (input.pressed, input.held);

        if held.up || held.down || held.left || held.right {
            self.held_frames = self.held_frames.saturating_add(1);
        } else {
            self.held_frames = 0;
        }
        let frames = self.held_frames;

        if Self::repeats(pressed.up, held.up, frames) {
            self.move_focus(-1, count);
        }
        if Self::repeats(pressed.down, held.down, frames) {
            self.move_focus(1, count);
        }
        let focus = self.place().focus.min(count.saturating_sub(1));
        let focused = rows.get(focus).map(|row| row.target);
        if let Some(Target::Option(option)) = focused {
            if Self::repeats(pressed.left, held.left, frames) {
                self.adjust(option, -1);
            }
            if Self::repeats(pressed.right, held.right, frames) {
                self.adjust(option, 1);
            }
        }
        if let Page::Options(category) = self.page() {
            let step = i32::from(pressed.r) - i32::from(pressed.l);
            if step != 0 {
                let next = (category as i32 + step).rem_euclid(CATEGORIES.len() as i32) as usize;
                *self.place_mut() = Place { page: Page::Options(next), focus: 0, scroll: 0 };
                return Action::None;
            }
        }

        let touched = self.update_touch(input.touch, &rows);
        if touched != Action::None {
            return touched;
        }
        // The page may have changed from a tap.
        if self.rows().len() != count {
            return Action::None;
        }

        if pressed.a {
            if let Some(target) = focused {
                return self.activate(target);
            }
        }
        if pressed.b {
            if self.page() == Page::OldWarning {
                return Action::Exit;
            }
            return self.back();
        }
        if pressed.start && self.page() != Page::OldWarning && self.page() != Page::Confirm {
            self.open(Page::Confirm);
        }
        Action::None
    }

    pub fn view(&self) -> View {
        let rows = self.rows();
        let place = *self.place();
        let focus = place.focus.min(rows.len().saturating_sub(1));
        let help = rows.get(focus).map(|row| row.help.as_str()).unwrap_or("");
        let row_views = rows
            .iter()
            .enumerate()
            .skip(place.scroll)
            .take(VISIBLE_ROWS)
            .map(|(index, row)| RowView {
                label: row.label.clone(),
                value: row.value.clone(),
                focused: index == focus,
                adjustable: matches!(row.target, Target::Option(_)),
            })
            .collect();

        let (title, subtitle, body, hint): (String, String, Vec<String>, &str) = match place.page {
            Page::OldWarning => (
                "Old 3DS Warning".to_owned(),
                "This console may not finish".to_owned(),
                wrap(
                    &format!(
                        "An Old 3DS can run out of memory while building a seed, which crashes the app. A full build can also take 30 minutes or more. If it crashes, make your seed on a computer. Follow the instructions at {COMPUTER_INSTRUCTIONS}"
                    ),
                    HELP_WIDTH,
                ),
                "A Select   B Exit",
            ),
            Page::Home => ("A Link Between Worlds".to_owned(), "Randomizer".to_owned(), wrap(help, HELP_WIDTH), "A Select   START Make a Seed"),
            Page::Categories => ("Rules".to_owned(), format!("Preset: {}", self.preset_name), wrap(help, HELP_WIDTH), "A Open   B Back"),
            Page::Options(category) => (
                CATEGORIES[category].0.to_owned(),
                format!("Preset: {}", self.preset_name),
                wrap(help, HELP_WIDTH),
                "Left/Right Change   L/R Page   B Back",
            ),
            Page::Presets => ("Presets".to_owned(), format!("Current: {}", self.preset_name), wrap(help, HELP_WIDTH), "A Load   B Back"),
            Page::Confirm => {
                let mut lines = vec![format!("Seed {}   Preset: {}", self.seed, self.preset_name)];
                let changes = self.rules.summary();
                if changes.is_empty() {
                    lines.push("All rules are at their defaults.".to_owned());
                } else {
                    let shown = changes.len().min(4);
                    lines.extend(changes[..shown].iter().cloned());
                    if changes.len() > shown {
                        lines.push(format!("...and {} more changes", changes.len() - shown));
                    }
                }
                ("Make This Seed?".to_owned(), "Keep the console plugged in".to_owned(), lines, "A Select   B Back")
            }
            Page::Message => (self.message.0.clone(), String::new(), self.message.1.clone(), "A OK"),
            Page::About => (
                "About and Credits".to_owned(),
                "Open source under GPL-2.0-or-later".to_owned(),
                vec![
                    "Randomizer: rickfay and contributors".to_owned(),
                    "   github.com/rickfay/z17-randomizer".to_owned(),
                    "Archipelago work: randomsalience".to_owned(),
                    "   github.com/randomsalience/z17-randomizer".to_owned(),
                    "3DS tools: devkitPro and rust3ds".to_owned(),
                ],
                "A OK",
            ),
        };

        View {
            title,
            subtitle,
            body,
            rows: row_views,
            hint: hint.to_owned(),
            scroll: place.scroll,
            total_rows: rows.len(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;

    fn menu() -> Menu {
        Menu::new(1234, presets::built_in(), false)
    }

    fn press(menu: &mut Menu, set: impl Fn(&mut Buttons)) -> Action {
        let mut buttons = Buttons::default();
        set(&mut buttons);
        menu.update(Input { pressed: buttons, held: buttons, touch: None })
    }

    fn release(menu: &mut Menu) {
        menu.update(Input::default());
    }

    fn tap(menu: &mut Menu, x: f32, y: f32) -> Action {
        menu.update(Input { touch: Some((x, y)), ..Input::default() });
        menu.update(Input::default())
    }

    fn row_y(slot: usize) -> f32 {
        ROW_TOP + slot as f32 * (ROW_HEIGHT + ROW_GAP) + 5.0
    }

    #[test]
    fn dpad_wraps_around_the_list() {
        let mut menu = menu();
        press(&mut menu, |b| b.up = true);
        release(&mut menu);
        assert!(menu.view().rows.iter().any(|row| row.focused && row.label == "Exit"));
    }

    #[test]
    fn long_lists_scroll_to_keep_focus_visible() {
        let mut menu = menu();
        for _ in 0..6 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        let view = menu.view();
        assert_eq!(view.scroll, 1);
        assert!(view.rows.iter().any(|row| row.focused && row.label == "Exit"));
    }

    #[test]
    fn a_opens_and_b_returns() {
        let mut menu = menu();
        press(&mut menu, |b| b.down = true);
        release(&mut menu);
        press(&mut menu, |b| b.a = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Categories);
        press(&mut menu, |b| b.b = true);
        assert_eq!(menu.page(), Page::Home);
        press(&mut menu, |b| b.b = true);
        assert_eq!(menu.page(), Page::Home);
    }

    #[test]
    fn held_direction_waits_then_repeats() {
        let mut menu = menu();
        let held = Buttons { down: true, ..Buttons::default() };
        menu.update(Input { pressed: held, held, touch: None });
        for _ in 0..REPEAT_DELAY - 2 {
            menu.update(Input { held, ..Input::default() });
        }
        assert!(menu.view().rows[1].focused);
        for _ in 0..3 {
            menu.update(Input { held, ..Input::default() });
        }
        assert!(menu.view().rows[2].focused);
    }

    #[test]
    fn tapping_a_row_focuses_and_opens_it() {
        let mut menu = menu();
        tap(&mut menu, 100.0, row_y(1));
        assert_eq!(menu.page(), Page::Categories);
    }

    #[test]
    fn tapping_outside_rows_does_nothing() {
        let mut menu = menu();
        tap(&mut menu, 2.0, row_y(1));
        tap(&mut menu, 100.0, 230.0);
        assert_eq!(menu.page(), Page::Home);
    }

    #[test]
    fn tapping_a_value_steps_it_both_ways() {
        let mut menu = menu();
        menu.open(Page::Options(0));
        let value = |menu: &Menu| menu.view().rows[0].value.clone().unwrap();
        assert_eq!(value(&menu), "Normal");
        tap(&mut menu, 300.0, row_y(0));
        assert_eq!(value(&menu), "Hard");
        tap(&mut menu, 200.0, row_y(0));
        assert_eq!(value(&menu), "Normal");
        assert_eq!(menu.preset_name, "Custom");
    }

    #[test]
    fn dragging_scrolls_and_does_not_tap() {
        let mut menu = menu();
        let drag = |menu: &mut Menu, y: f32| menu.update(Input { touch: Some((100.0, y)), ..Input::default() });
        drag(&mut menu, 150.0);
        drag(&mut menu, 100.0);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Home);
        assert_eq!(menu.view().scroll, 1);
    }

    #[test]
    fn shoulder_buttons_change_category_with_wrap() {
        let mut menu = menu();
        menu.open(Page::Options(0));
        press(&mut menu, |b| b.l = true);
        assert_eq!(menu.page(), Page::Options(CATEGORIES.len() - 1));
    }

    #[test]
    fn every_option_appears_on_exactly_one_page() {
        let menu = menu();
        let mut total = 0;
        for category in 0..CATEGORIES.len() {
            let mut menu_page = Menu::new(1, Vec::new(), false);
            menu_page.open(Page::Options(category));
            total += menu_page.rows().len();
        }
        assert_eq!(total, OPTIONS.len());
        drop(menu);
    }

    #[test]
    fn loading_a_preset_applies_it_and_returns() {
        let mut menu = menu();
        menu.open(Page::Presets);
        tap(&mut menu, 100.0, row_y(1));
        assert_eq!(menu.preset_name, "Casual");
        assert_eq!(menu.page(), Page::Home);
        assert!(menu.rules.settings().start_with_merge);
    }

    #[test]
    fn confirm_starts_generation_and_lists_changes() {
        let mut menu = menu();
        menu.rules.step(&OPTIONS[0], 1);
        press(&mut menu, |b| b.a = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Confirm);
        assert!(menu.view().body.iter().any(|line| line == "Logic: Hard"));
        assert_eq!(press(&mut menu, |b| b.a = true), Action::Generate);
    }

    #[test]
    fn old_console_warns_first_and_can_continue_or_exit() {
        let mut menu = Menu::new(1, Vec::new(), true);
        assert_eq!(menu.page(), Page::OldWarning);
        assert!(menu.view().body.join(" ").contains("z17-randomizer"));
        assert_eq!(press(&mut menu, |b| b.b = true), Action::Exit);
        release(&mut menu);
        press(&mut menu, |b| b.a = true);
        assert_eq!(menu.page(), Page::Home);
    }

    #[test]
    fn wrap_respects_width() {
        let lines = wrap("one two three four five six", 9);
        assert!(lines.iter().all(|line| line.len() <= 9));
        assert_eq!(lines.join(" "), "one two three four five six");
    }
}
