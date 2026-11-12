//! Menu screens, focus, buttons, and touch. The app feeds in input and draws the returned view.

use crate::ap_yaml::{ApKind, ApOptions, AP_CATEGORIES, AP_OPTIONS};
use crate::presets::Preset;
use crate::rules::{OptionDef, Rules, CATEGORIES, OPTIONS};
use crate::spoiler::Spoiler;

pub const ROW_TOP: f32 = 8.0;
/// The focused row is scrolled to stay above this line, with room for its glow.
pub const VIEW_BOTTOM: f32 = 193.0;
/// The hint bar starts here. Rows are drawn and tapped only above it, so a row can show cut off.
pub const HINT_TOP: f32 = 199.0;
pub const ROW_HEIGHT: f32 = 34.0;
pub const ROW_GAP: f32 = 3.0;
pub const VISIBLE_ROWS: usize = 5;
pub const ROW_LEFT: f32 = 8.0;
pub const ROW_WIDTH: f32 = 304.0;
/// Touches on a row at or right of this x change its value instead of only opening it.
pub const VALUE_LEFT: f32 = 208.0;
pub const VALUE_CENTER: f32 = 258.0;

/// Rules pages show each setting as a card: its name on top, then its choices as pills.
pub const CARD_HEIGHT: f32 = 56.0;
pub const CARD_GAP: f32 = 4.0;
pub const VISIBLE_CARDS: usize = 3;
/// The pills sit this far below a card's top, inside the card's 8 pixel padding.
pub const PILL_TOP: f32 = 28.0;
pub const PILL_HEIGHT: f32 = 20.0;
pub const PILL_LEFT: f32 = ROW_LEFT + 8.0;
pub const PILL_WIDTH: f32 = ROW_WIDTH - 16.0;
pub const PILL_GAP: f32 = 4.0;
/// From one line of pills to the next, on cards with two lines.
pub const PILL_LINE: f32 = PILL_HEIGHT + PILL_GAP;
/// Settings with this many choices or more put their pills on two lines, so long names fit.
pub const TWO_LINE_CHOICES: usize = 5;
/// The minus and plus buttons on either end of a number's slider.
pub const STEP_WIDTH: f32 = 34.0;

/// How a page stacks its rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub height: f32,
    pub gap: f32,
    pub visible: usize,
}

impl Layout {
    pub const ROWS: Self = Self { height: ROW_HEIGHT, gap: ROW_GAP, visible: VISIBLE_ROWS };
    pub const CARDS: Self = Self { height: CARD_HEIGHT, gap: CARD_GAP, visible: VISIBLE_CARDS };
    /// Two lines per row: a small label over a larger value. Spoiler entries use it.
    pub const LINES: Self = Self { height: 44.0, gap: 3.0, visible: 4 };

    pub fn of(page: Page) -> Self {
        match page {
            Page::Options(_) | Page::ApOptions(_) => Self::CARDS,
            Page::Spoiler | Page::About => Self::LINES,
            _ => Self::ROWS,
        }
    }

    /// The distance from one row's top to the next.
    pub fn pitch(self) -> f32 {
        self.height + self.gap
    }

    /// The top of the row in this visible slot, when every row is the usual height.
    pub fn top(self, slot: usize) -> f32 {
        ROW_TOP + slot as f32 * self.pitch()
    }

    /// Each row's top before scrolling, given each row's height.
    pub fn tops(self, heights: &[f32]) -> Vec<f32> {
        let mut y = ROW_TOP;
        heights
            .iter()
            .map(|height| {
                let top = y;
                y += height + self.gap;
                top
            })
            .collect()
    }

    /// The height of all the rows together, gaps included.
    pub fn content_height(self, heights: &[f32]) -> f32 {
        heights.iter().sum::<f32>() + self.gap * heights.len().saturating_sub(1) as f32
    }

    /// The furthest the list scrolls: far enough to show the last row whole.
    pub fn most_scroll(self, heights: &[f32]) -> f32 {
        (ROW_TOP + self.content_height(heights) - VIEW_BOTTOM).max(0.0)
    }
}

/// A setting's choices, drawn as pills under its name.
#[derive(Clone, Debug, PartialEq)]
pub enum Pills {
    Choices { names: Vec<String>, selected: usize },
    Number { value: i64, min: i64, max: i64 },
}

/// The left edge and width of each of `count` pills in a row.
pub fn pill_spans(count: usize) -> impl Iterator<Item = (f32, f32)> {
    let width = (PILL_WIDTH - PILL_GAP * count.saturating_sub(1) as f32) / count.max(1) as f32;
    (0..count).map(move |index| (PILL_LEFT + index as f32 * (width + PILL_GAP), width))
}

/// How many lines of pills a setting's choices take.
pub fn pill_lines(count: usize) -> usize {
    if count >= TWO_LINE_CHOICES { 2 } else { 1 }
}

/// Each of `count` pills as (left, top, width), with top measured from the first line of pills.
/// On two lines, the first line holds the extra pill when the count is odd.
pub fn pill_boxes(count: usize) -> Vec<(f32, f32, f32)> {
    let first = if pill_lines(count) == 2 { count.div_ceil(2) } else { count };
    pill_spans(first)
        .map(|(left, width)| (left, 0.0, width))
        .chain(pill_spans(count - first).map(|(left, width)| (left, PILL_LINE, width)))
        .collect()
}

impl Pills {
    /// How many lines of pills these take on a card.
    pub fn lines(&self) -> usize {
        match self {
            Pills::Choices { names, .. } => pill_lines(names.len()),
            Pills::Number { .. } => 1,
        }
    }
}

/// The left edges of a number's minus and plus buttons.
pub fn step_lefts() -> [f32; 2] {
    [PILL_LEFT, PILL_LEFT + PILL_WIDTH - STEP_WIDTH]
}

const REPEAT_DELAY: u32 = 20;
const REPEAT_EVERY: u32 = 5;
const DRAG_START: f32 = 6.0;
const MOD_NAME_WIDTH: usize = 16;

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
    pub x: bool,
    pub y: bool,
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

/// A menu sound effect. The menu picks one per update and the app plays it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Focus moved to another row.
    Hover,
    /// A value changed: a rule, an Archipelago option, a mod or a look.
    Choose,
    /// A row did something: opened a page, the keyboard, or a log.
    Select,
    /// L or R switched to the next page of the same list.
    PageChange,
    /// A page closed.
    Back,
    /// Building a seed or patching from Archipelago started.
    Start,
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
    /// Save the rules under this name, replacing the preset that has it.
    ReplacePreset(String),
    /// Start making the seed.
    Generate,
    /// Play a clip of the chosen voice.
    PlayVoice,
    /// List the saved spoiler logs, then call `show_spoiler_files`.
    ListSpoilers,
    /// Read the spoiler log saved under this name, then call `show_spoiler`.
    OpenSpoiler(String),
    /// Open the keyboard for a spoiler search, then call `set_search`.
    SearchSpoiler,
    /// List the Archipelago patch files, then call `show_patches`.
    ListPatches,
    /// Read the patch file with this name, then call `show_patch`.
    OpenPatch(String),
    /// Patch the game from the Archipelago patch file with this name.
    PatchArchipelago(String),
    /// Start the albw.local page. The app stops it when the sharing page closes.
    StartSharing,
    /// Open the keyboard for the Archipelago player name, then call `set_player_name`.
    EditPlayerName,
    /// Start the installed game with the plugin that copies its files to the SD card.
    CopyGameFiles,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    OldWarning,
    Home,
    Categories,
    Options(usize),
    Presets,
    /// Asks to save changed rules as a preset, after leaving the rules pages.
    OfferPreset,
    /// Asks whether to replace the preset that already has the chosen name.
    ReplacePreset,
    Mods,
    Confirm,
    Message,
    About,
    /// The saved spoiler logs, newest first.
    Spoilers,
    /// One spoiler log's entries, or the ones that match the search.
    Spoiler,
    /// Archipelago: sharing, the player name, and patch files.
    Archipelago,
    /// The Archipelago options pages, and the buttons to copy the rules or reset.
    ApCategories,
    /// One page of Archipelago options, by index into `AP_CATEGORIES`.
    ApOptions(usize),
    /// The albw.local page is open while this page shows.
    Sharing,
    /// The Archipelago patch files on the SD card.
    Patches,
    /// One Archipelago patch, ready to apply.
    Patch,
}

/// What the app read from an Archipelago patch, for the confirm page.
#[derive(Clone, Debug, Default)]
pub struct PatchSummary {
    pub file: String,
    pub player: String,
    pub seed: u32,
    pub version: String,
    /// Patch options worth showing, as "Name: value" lines.
    pub lines: Vec<String>,
}

/// A look-only option the player can change when applying an Archipelago patch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look {
    NightMode,
    FreestandingModels,
}

/// The seed page's last row, which starts the run.
const START_RANDOMIZING: &str = "Start Randomizing";
const CONFIRM_START: usize = 2;

const LOOK_CHOICES: [&str; 3] = ["As Patched", "On", "Off"];

/// The two kinds of mod a seed can include.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModKind {
    Model,
    Voice,
}

/// The mods found on the SD card, with the choice for one kind. Choice 0 is the original game.
#[derive(Clone, Debug, Default)]
struct ModChoice {
    names: Vec<String>,
    selected: usize,
}

impl ModChoice {
    fn step(&mut self, delta: i32) {
        let count = self.names.len() as i32 + 1;
        self.selected = (self.selected as i32 + delta).rem_euclid(count) as usize;
    }

    fn name(&self) -> Option<&str> {
        self.selected.checked_sub(1).map(|index| self.names[index].as_str())
    }

    fn text(&self) -> String {
        match self.name() {
            Some(name) if name.chars().count() > MOD_NAME_WIDTH => {
                format!("{}...", name.chars().take(MOD_NAME_WIDTH - 3).collect::<String>())
            }
            Some(name) => name.to_owned(),
            None => "Original".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Open(Page),
    Option(usize),
    Preset(usize),
    Mod(ModKind),
    SavePreset,
    ReplacePreset,
    EditSeed,
    RandomSeed,
    Generate,
    Back,
    Exit,
    ListSpoilers,
    SpoilerFile(usize),
    ListPatches,
    PatchFile(usize),
    StartSharing,
    EditPlayerName,
    /// An Archipelago option, by index into `AP_OPTIONS`.
    ApOption(usize),
    ApFromRules,
    ApDefaults,
    Look(Look),
    ApplyPatch,
    CopyGameFiles,
    /// A spoiler entry, or a row that does nothing.
    Nothing,
}

/// What a tap on a card's pills does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pick {
    Choice(usize),
    Step(i32),
}

#[derive(Clone, Debug)]
struct Row {
    label: String,
    value: Option<String>,
    help: String,
    target: Target,
    icon: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
struct Place {
    page: Page,
    focus: usize,
    /// How far the list is scrolled, in pixels.
    scroll: f32,
}

/// One row for the app to draw.
#[derive(Clone, Debug)]
pub struct RowView {
    pub label: String,
    pub value: Option<String>,
    pub focused: bool,
    /// The row changes a value when moved left or right.
    pub adjustable: bool,
    /// The name of a game texture to show beside the label.
    pub icon: Option<&'static str>,
    /// On rules pages, the setting's choices.
    pub pills: Option<Pills>,
    /// Where the row's top is, and how tall it is. Cards with two lines of pills are taller.
    pub top: f32,
    pub height: f32,
}

/// Everything the app needs to draw a frame.
#[derive(Clone, Debug)]
pub struct View {
    pub title: String,
    pub subtitle: String,
    /// Paragraphs and lines of text. The app wraps each one to fit its font and box.
    pub body: Vec<String>,
    pub rows: Vec<RowView>,
    pub hint: String,
    /// Scroll position in pixels, total rows, and the height of all rows, for the scroll bar.
    pub scroll: f32,
    pub total_rows: usize,
    pub content_height: f32,
    pub layout: Layout,
}

#[derive(Clone, Copy, Debug)]
struct Touch {
    start: (f32, f32),
    last: (f32, f32),
    dragging: bool,
    scroll_at_start: f32,
}

pub struct Menu {
    stack: Vec<Place>,
    pub rules: Rules,
    pub seed: u32,
    pub preset_name: String,
    /// Where the game was found, shown on the home page.
    pub game_location: String,
    /// True when the game is installed but its files are not on the SD card yet. The home page
    /// then offers to copy them.
    pub can_copy_game: bool,
    presets: Vec<Preset>,
    /// True once a rule changes on the rules pages, until the player leaves them.
    rules_changed: bool,
    /// The name waiting on the replace question, and the preset that has it now.
    replacing: Option<(String, String)>,
    model: ModChoice,
    voice: ModChoice,
    /// Names of the saved spoiler logs, newest first, without the folder or extension.
    spoiler_files: Vec<String>,
    spoiler: Spoiler,
    search: String,
    /// The spoiler entries that match the search.
    found: Vec<usize>,
    /// Names of the Archipelago patch files, without the folder.
    patch_files: Vec<String>,
    patch: PatchSummary,
    /// Choices for each `Look`, as indexes into `LOOK_CHOICES`.
    looks: [usize; 2],
    /// The Archipelago slot name for options files.
    pub player_name: String,
    /// The Archipelago options the options file is made from.
    pub ap_options: ApOptions,
    /// Set when the Archipelago options change, until the app saves them.
    ap_changed: bool,
    /// The apworld on the SD card, as the Archipelago page shows it: its version, or that it is missing.
    /// The app sets it when the Archipelago page opens and when a new apworld arrives.
    pub apworld_status: String,
    /// The app's version, shown on the About page.
    pub app_version: String,
    /// Lines the sharing page shows: the address, then what has happened.
    share_status: Vec<String>,
    message: (String, Vec<String>),
    held_frames: u32,
    touch: Option<Touch>,
    /// Counts value changes, so an update can tell whether one happened.
    value_changes: u32,
    /// The sound for the last update, until the app takes it.
    sound: Option<Sound>,
}

/// The About page, one row each: what they did, who they are, and more on the top screen.
const CREDITS: &[(&str, &str, &str)] = &[
    ("3DS App", "Faith Branch Software LLC", "Built this app: the menus, seed building on the console, spoiler logs, and model and voice mods."),
    ("Randomizer", "rickfay and contributors", "Made the A Link Between Worlds randomizer this app runs. github.com/rickfay/z17-randomizer"),
    ("Archipelago Original", "randomsalience", "Made the original Archipelago version of the randomizer and its apworld, the base this app's Archipelago support builds on. github.com/randomsalience/albw-archipelago"),
    ("Archipelago Update", "Faith Branch Software LLC", "Updated the Archipelago version for this app: new options, patching on the 3DS, and the albw.local page."),
    ("Archipelago", "Archipelago team", "The multiworld randomizer this works with. archipelago.gg"),
    ("3DS Toolchain", "devkitPro", "devkitARM, libctru, citro3d and citro2d. devkitpro.org"),
    ("Rust on 3DS", "rust3ds", "ctru-rs and cargo-3ds. github.com/rust3ds"),
    ("Title Font", "Omni Jacala", "Hylia Serif."),
    ("Body Font", "Fontworks", "Reggae One, under the SIL Open Font License 1.1."),
    ("Sound Effects", "friedspin", "Menu sounds from Cozy UI SFX. friedspin.itch.io"),
    ("Models and Voices", "Their authors", "Each mod belongs to the person who made it."),
    ("The Game", "Nintendo", "The Legend of Zelda: A Link Between Worlds. This app is not made or approved by Nintendo, and it ships no game data."),
];

/// One paragraph of body text. The app wraps paragraphs to fit its font and box.
fn paragraph(text: &str) -> Vec<String> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![text.to_owned()]
    }
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
    /// `old_3ds` starts on a notice about what that console can do.
    pub fn new(seed: u32, presets: Vec<Preset>, old_3ds: bool) -> Self {
        let mut stack = vec![Place { page: Page::Home, focus: 0, scroll: 0.0 }];
        if old_3ds {
            stack.push(Place { page: Page::OldWarning, focus: 0, scroll: 0.0 });
        }
        Self {
            stack,
            rules: Rules::default(),
            seed,
            preset_name: "Standard".to_owned(),
            game_location: String::new(),
            can_copy_game: false,
            presets,
            rules_changed: false,
            replacing: None,
            model: ModChoice::default(),
            voice: ModChoice::default(),
            spoiler_files: Vec::new(),
            spoiler: Spoiler::default(),
            search: String::new(),
            found: Vec::new(),
            patch_files: Vec::new(),
            patch: PatchSummary::default(),
            looks: [0; 2],
            player_name: "Player".to_owned(),
            ap_options: ApOptions::from_rules(&Rules::default().settings()),
            ap_changed: false,
            apworld_status: String::new(),
            app_version: String::new(),
            share_status: Vec::new(),
            message: (String::new(), Vec::new()),
            held_frames: 0,
            touch: None,
            value_changes: 0,
            sound: None,
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

    /// Opens a page of rules. For testing.
    /// Opens the Models and Voices page.
    pub fn open_mods(&mut self) {
        self.open(Page::Mods);
    }

    pub fn open_category(&mut self, category: usize) {
        if category < CATEGORIES.len() {
            self.stack.push(Place { page: Page::Options(category), focus: 0, scroll: 0.0 });
        }
    }

    /// Loads the preset with this name, if there is one. For testing.
    pub fn select_preset(&mut self, name: &str) {
        if let Some(preset) = self.presets.iter().find(|preset| preset.name == name) {
            self.rules = Rules::with_overrides(&preset.overrides);
            self.preset_name = preset.name.clone();
        }
    }

    /// Sets the mods found on the SD card.
    pub fn set_mods(&mut self, models: Vec<String>, voices: Vec<String>) {
        self.model = ModChoice { names: models, selected: 0 };
        self.voice = ModChoice { names: voices, selected: 0 };
    }

    fn mod_choice(&mut self, kind: ModKind) -> &mut ModChoice {
        match kind {
            ModKind::Model => &mut self.model,
            ModKind::Voice => &mut self.voice,
        }
    }

    /// Selects a mod by name, if the SD card has it.
    pub fn select_mod(&mut self, kind: ModKind, name: &str) {
        let choice = self.mod_choice(kind);
        if let Some(index) = choice.names.iter().position(|found| found == name) {
            choice.selected = index + 1;
        }
    }

    /// The selected player model's folder name, if any.
    pub fn model(&self) -> Option<&str> {
        self.model.name()
    }

    /// The selected voice pack's folder name, if any.
    pub fn voice(&self) -> Option<&str> {
        self.voice.name()
    }

    /// The preset a new preset with this name and file would replace. Names match without regard
    /// to case, and two names can share a file name once unsafe characters are swapped out.
    pub fn preset_in_the_way(&self, name: &str, file: &str) -> Option<&Preset> {
        let name = name.trim();
        self.presets
            .iter()
            .find(|preset| preset.name.trim().eq_ignore_ascii_case(name) || preset.file.as_deref() == Some(file))
    }

    /// Asks whether to replace `existing` with rules saved as `name`.
    pub fn ask_to_replace(&mut self, name: &str, existing: &str) {
        self.replacing = Some((name.to_owned(), existing.to_owned()));
        self.open(Page::ReplacePreset);
    }

    /// Lists saved spoiler logs by name, newest first. With none, says how to get one.
    pub fn show_spoiler_files(&mut self, names: Vec<String>) {
        if names.is_empty() {
            self.show_message("No Spoiler Logs Yet", "Each seed you make saves its spoiler log here. Make a seed, then come back.");
            return;
        }
        self.spoiler_files = names;
        self.open(Page::Spoilers);
    }

    /// Lists the Archipelago patch files. With none, says how to get one.
    pub fn show_patches(&mut self, names: Vec<String>) {
        if names.is_empty() {
            self.show_message(
                "No Patches Yet",
                "Put the .apalbw3ds file from your Archipelago host in /3ds/albw-rando/ap on the SD card. The host makes it with the A Link Between Worlds - 3DS apworld.",
            );
            return;
        }
        self.patch_files = names;
        self.open(Page::Patches);
    }

    /// Sets the Archipelago player name, as cleaned for an options file.
    pub fn set_player_name(&mut self, name: &str) {
        self.player_name = crate::ap_yaml::clean_name(name);
    }

    /// Opens the sharing page with its first status lines.
    pub fn show_sharing(&mut self, lines: Vec<String>) {
        self.share_status = lines;
        self.open(Page::Sharing);
    }

    /// Replaces the sharing page's status lines.
    pub fn set_share_status(&mut self, lines: Vec<String>) {
        self.share_status = lines;
    }

    /// The options file for the current rules and player name.
    /// Replaces the Archipelago options, as when the app loads saved ones.
    pub fn set_ap_options(&mut self, options: ApOptions) {
        self.ap_options = options;
        self.ap_changed = false;
    }

    /// The Archipelago options to save, once after each change.
    pub fn take_ap_changes(&mut self) -> Option<String> {
        std::mem::take(&mut self.ap_changed).then(|| self.ap_options.save())
    }

    fn ap_pills(&self, index: usize) -> Pills {
        let value = self.ap_options.value(index);
        match AP_OPTIONS[index].kind {
            ApKind::Toggle => Pills::Choices { names: vec!["Off".to_owned(), "On".to_owned()], selected: value as usize },
            ApKind::Choice(choices) => {
                Pills::Choices { names: choices.iter().map(|choice| choice.label.to_owned()).collect(), selected: value as usize }
            }
            ApKind::Range { min, max, .. } => Pills::Number { value, min, max },
        }
    }

    fn ap_step(&mut self, index: usize, delta: i32) {
        self.ap_options.step(index, delta);
        self.ap_changed = true;
        self.value_changes += 1;
    }

    pub fn options_file(&self) -> String {
        crate::ap_yaml::options_file(&self.ap_options, &self.player_name)
    }

    /// Shows a patch that was read, ready to apply.
    pub fn show_patch(&mut self, patch: PatchSummary) {
        self.patch = patch;
        self.open(Page::Patch);
    }

    /// The player's choice for a look-only option: `None` keeps what the patch says.
    pub fn look(&self, look: Look) -> Option<bool> {
        match self.looks[look as usize] {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    fn step_look(&mut self, look: Look, delta: i32) {
        let choice = &mut self.looks[look as usize];
        *choice = (*choice as i32 + delta).rem_euclid(LOOK_CHOICES.len() as i32) as usize;
        self.value_changes += 1;
    }

    fn step_mod(&mut self, kind: ModKind, delta: i32) {
        self.mod_choice(kind).step(delta);
        self.value_changes += 1;
    }

    /// Opens a spoiler log with every entry showing.
    pub fn show_spoiler(&mut self, spoiler: Spoiler) {
        self.found = (0..spoiler.entries.len()).collect();
        self.spoiler = spoiler;
        self.search.clear();
        self.open(Page::Spoiler);
    }

    /// Shows only the spoiler entries that match `query`.
    pub fn set_search(&mut self, query: &str) {
        self.search = query.trim().to_owned();
        self.found = self.spoiler.search(&self.search);
        let place = self.place_mut();
        place.focus = 0;
        place.scroll = 0.0;
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    /// Moves focus to the first entry of the next (`delta` 1) or previous (`delta` -1) group.
    fn jump_group(&mut self, delta: i32) {
        let entries = &self.spoiler.entries;
        let group = |slot: usize| self.found.get(slot).map(|&index| entries[index].group.as_str());
        let focus = self.place().focus;
        let Some(current) = group(focus) else { return };
        let target = if delta > 0 {
            (focus + 1..self.found.len()).find(|&slot| group(slot) != Some(current))
        } else {
            // The start of this group, then the start of the one before it.
            let start = (0..focus).rev().take_while(|&slot| group(slot) == Some(current)).last().unwrap_or(focus);
            start.checked_sub(1).map(|before| {
                let previous = group(before);
                (0..=before).rev().take_while(|&slot| group(slot) == previous).last().unwrap_or(before)
            })
        };
        // The group's first entry goes to the top of the list, where the list is long enough.
        if let Some(slot) = target {
            let layout = Layout::of(self.page());
            let heights = self.heights(&self.rows());
            let top = layout.tops(&heights).get(slot).copied().unwrap_or(ROW_TOP);
            let place = self.place_mut();
            place.focus = slot;
            place.scroll = (top - ROW_TOP).min(layout.most_scroll(&heights));
        }
    }

    /// Opens a message page on top of the current page.
    pub fn show_message(&mut self, title: &str, text: &str) {
        self.message = (title.to_owned(), paragraph(text));
        self.open(Page::Message);
    }

    /// Returns to the home page, for after a run.
    pub fn go_home(&mut self) {
        self.stack.truncate(1);
    }

    fn open(&mut self, page: Page) {
        // The seed page opens on Start Randomizing, so A then A still makes a seed.
        let focus = if page == Page::Confirm { CONFIRM_START } else { 0 };
        self.stack.push(Place { page, focus, scroll: 0.0 });
        self.follow_focus();
    }

    fn back(&mut self) -> Action {
        if self.stack.len() > 1 {
            let left = self.stack.pop().map(|place| place.page);
            // Leaving the rules after changing them offers to keep the result as a preset.
            if left == Some(Page::Categories) && std::mem::take(&mut self.rules_changed) && self.preset_name == "Custom" {
                self.open(Page::OfferPreset);
            }
        }
        Action::None
    }

    fn focused_target(&self, focus: usize, rows: &[Row]) -> Option<Target> {
        rows.get(focus).map(|row| row.target)
    }

    fn option_row(&self, index: usize, option: &OptionDef) -> Row {
        Row {
            label: option.label.to_owned(),
            value: Some(self.rules.value_text(option)),
            help: option.help.to_owned(),
            target: Target::Option(index),
            icon: crate::icons::for_option(option.key),
        }
    }

    fn rows(&self) -> Vec<Row> {
        let row = |label: &str, value: Option<String>, help: &str, target: Target| Row {
            label: label.to_owned(),
            value,
            help: help.to_owned(),
            target,
            icon: None,
        };
        match self.page() {
            Page::OldWarning => vec![
                row("Continue", None, "", Target::Back),
                row("Exit", None, "", Target::Exit),
            ],
            Page::Home => {
                let mut rows = Vec::new();
                if self.can_copy_game {
                    rows.push(row("Copy Game Files", None, "Starts your game once to copy the files the randomizer needs to the SD card, about 300 MB, then comes back here. Needs Luma's plugin loader.", Target::CopyGameFiles));
                }
                rows.extend([
                row("Make a Seed", None, "Pick the seed number, review your rules, then build a randomized game.", Target::Open(Page::Confirm)),
                row("Rules", None, "Change every option by hand.", Target::Open(Page::Categories)),
                row("Presets", Some(self.preset_name.clone()), "Load a set of rules, or save your own to share.", Target::Open(Page::Presets)),
                row("Models and Voices", None, "Choose a player model and voice pack for this seed.", Target::Open(Page::Mods)),
                row("Spoiler Logs", None, "Stuck? Look up where items are in seeds you made, and search by item or place.", Target::ListSpoilers),
                row("Archipelago", None, "Play in a multiworld: send your options to the host, then patch your game from their file.", Target::Open(Page::Archipelago)),
                row("About and Credits", None, "Who made this possible.", Target::Open(Page::About)),
                row("Exit", None, "Close the app.", Target::Exit),
                ]);
                rows
            }
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
            Page::OfferPreset => vec![
                row("Save as Preset", None, "Name these rules to load them again later or share them.", Target::SavePreset),
                row("Not Now", None, "Keep the rules for this seed without saving them.", Target::Back),
            ],
            Page::ReplacePreset => vec![
                row("Replace It", None, "The old preset's file is overwritten with these rules.", Target::ReplacePreset),
                row("Pick Another Name", None, "Keeps the old preset and opens the keyboard again.", Target::SavePreset),
                row("Cancel", None, "Nothing is saved.", Target::Back),
            ],
            Page::Mods => vec![
                row("Player Model", Some(self.model.text()), "Add a mod as /3ds/albw-rando/models/<name>/romfs. It then appears here.", Target::Mod(ModKind::Model)),
                row(
                    "Voice",
                    Some(self.voice.text()),
                    "Press Y to hear it. Add a mod as /3ds/albw-rando/voices/<name>/romfs. It then appears here.",
                    Target::Mod(ModKind::Voice),
                ),
            ],
            Page::Confirm => vec![
                row("Seed Number", Some(self.seed.to_string()), "The same number and rules make the same game on this console. Press A to type one.", Target::EditSeed),
                row("New Random Seed", None, "Picks a new seed number.", Target::RandomSeed),
                row(START_RANDOMIZING, None, "Builds the game. Keep the console plugged in.", Target::Generate),
            ],
            Page::Message => vec![row("OK", None, "", Target::Back)],
            Page::About => CREDITS.iter().map(|&(role, name, about)| row(role, Some(name.to_owned()), about, Target::Nothing)).collect(),
            Page::Spoilers => self
                .spoiler_files
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let label = match name.split_once("-seed-") {
                        Some((_, seed)) => format!("Seed {seed}"),
                        None => name.clone(),
                    };
                    row(&label, None, &format!("Saved as /3ds/albw-rando/spoilers/{name}.json"), Target::SpoilerFile(index))
                })
                .collect(),
            Page::Archipelago => vec![
                row(
                    "Share on Network",
                    None,
                    "Opens http://albw.local on your Wi-Fi. From a computer there, download your options file and the apworld, and send back your patch.",
                    Target::StartSharing,
                ),
                row("Player Name", Some(self.player_name.clone()), "Your name in the multiworld. Press A to type it.", Target::EditPlayerName),
                row(
                    "Options",
                    None,
                    "Set up your options file: every option the A Link Between Worlds - 3DS apworld has, plus Archipelago's own, like Death Link.",
                    Target::Open(Page::ApCategories),
                ),
                row("Patch Files", None, "Apply a patch your host sent. Patches go in /3ds/albw-rando/ap.", Target::ListPatches),
            ],
            Page::ApCategories => {
                let mut rows = vec![
                    row(
                        "Copy from Rules",
                        Some(self.preset_name.clone()),
                        "Sets these options to match your rules, so a multiworld plays like your single-player seeds.",
                        Target::ApFromRules,
                    ),
                    row("Reset to Defaults", None, "Sets every option to the apworld's default.", Target::ApDefaults),
                ];
                rows.extend(AP_CATEGORIES.iter().enumerate().map(|(index, name)| {
                    let names: Vec<&str> = AP_OPTIONS.iter().filter(|option| option.category == index).map(|option| option.label).collect();
                    row(name, None, &names.join(", "), Target::Open(Page::ApOptions(index)))
                }));
                rows
            }
            Page::ApOptions(category) => AP_OPTIONS
                .iter()
                .enumerate()
                .filter(|(_, option)| option.category == category)
                .map(|(index, option)| Row {
                    label: option.label.to_owned(),
                    value: Some(self.ap_options.label(index)),
                    help: self.ap_options.help(index),
                    target: Target::ApOption(index),
                    icon: None,
                })
                .collect(),
            Page::Sharing => vec![row("Stop Sharing", None, "", Target::Back)],
            Page::Patches => self
                .patch_files
                .iter()
                .enumerate()
                .map(|(index, name)| row(name.trim_end_matches(".apalbw3ds"), None, &format!("/3ds/albw-rando/ap/{name}"), Target::PatchFile(index)))
                .collect(),
            Page::Patch => vec![
                row("Patch the Game", None, "Builds the game for this multiworld. Play it with the Archipelago client on a computer.", Target::ApplyPatch),
                row("Night Mode", Some(LOOK_CHOICES[self.looks[0]].to_owned()), "Darker world. Looks only, so it never changes the multiworld.", Target::Look(Look::NightMode)),
                row(
                    "Freestanding Models",
                    Some(LOOK_CHOICES[self.looks[1]].to_owned()),
                    "Items on the ground show what they are. Looks only, so it never changes the multiworld.",
                    Target::Look(Look::FreestandingModels),
                ),
                row("Models and Voices", Some(format!("{} / {}", self.model.text(), self.voice.text())), "Your player model and voice go into this patch too. Only you see them.", Target::Open(Page::Mods)),
                row("Go Back", None, "", Target::Back),
            ],
            Page::Spoiler if self.found.is_empty() => vec![row("No Matches", None, "", Target::Nothing)],
            Page::Spoiler => self
                .found
                .iter()
                .map(|&index| {
                    let entry = &self.spoiler.entries[index];
                    row(&entry.place, Some(entry.item.clone()), &entry.group, Target::Nothing)
                })
                .collect(),
        }
    }

    fn adjust(&mut self, index: usize, delta: i32) {
        self.rules.step(&OPTIONS[index], delta);
        self.mark_changed();
        self.value_changes += 1;
    }

    fn pick(&mut self, index: usize, choice: usize) {
        self.rules.select(&OPTIONS[index], choice);
        self.mark_changed();
        self.value_changes += 1;
    }

    fn mark_changed(&mut self) {
        self.preset_name = "Custom".to_owned();
        self.rules_changed = true;
    }

    /// Closes a question page before the app acts on its answer.
    fn close_question(&mut self) {
        if matches!(self.page(), Page::OfferPreset | Page::ReplacePreset) {
            self.stack.pop();
        }
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
            Target::Mod(kind) => {
                self.step_mod(kind, 1);
                Action::None
            }
            Target::SavePreset => {
                self.close_question();
                Action::SavePreset
            }
            Target::ReplacePreset => {
                self.close_question();
                match self.replacing.take() {
                    Some((name, _)) => Action::ReplacePreset(name),
                    None => Action::None,
                }
            }
            Target::EditSeed => Action::EditSeed,
            Target::RandomSeed => Action::RandomSeed,
            Target::Generate => Action::Generate,
            Target::Back => self.back(),
            Target::Exit => Action::Exit,
            Target::CopyGameFiles => Action::CopyGameFiles,
            Target::ListSpoilers => Action::ListSpoilers,
            Target::SpoilerFile(index) => Action::OpenSpoiler(self.spoiler_files[index].clone()),
            Target::ListPatches => Action::ListPatches,
            Target::StartSharing => Action::StartSharing,
            Target::EditPlayerName => Action::EditPlayerName,
            Target::ApOption(index) => {
                self.ap_step(index, 1);
                Action::None
            }
            Target::ApFromRules => {
                self.ap_options = ApOptions::from_rules(&self.rules.settings());
                self.ap_changed = true;
                let skipped = crate::ap_yaml::unsupported(&self.rules.settings());
                let mut text = format!("Your Archipelago options now match the {} rules.", self.preset_name);
                if !skipped.is_empty() {
                    text.push_str(&format!(" Archipelago can't use: {}.", skipped.join(", ")));
                }
                self.show_message("Options Copied", &text);
                Action::None
            }
            Target::ApDefaults => {
                self.ap_options = ApOptions::default();
                self.ap_changed = true;
                Action::None
            }
            Target::PatchFile(index) => Action::OpenPatch(self.patch_files[index].clone()),
            Target::Look(look) => {
                self.step_look(look, 1);
                Action::None
            }
            Target::ApplyPatch => Action::PatchArchipelago(self.patch.file.clone()),
            Target::Nothing => Action::None,
        }
    }

    fn move_focus(&mut self, delta: i32, count: usize) {
        if count == 0 {
            return;
        }
        let place = self.place_mut();
        place.focus = (place.focus as i32 + delta).rem_euclid(count as i32) as usize;
        self.follow_focus();
    }

    /// Scrolls just enough to show the focused row whole.
    fn follow_focus(&mut self) {
        let layout = Layout::of(self.page());
        let heights = self.heights(&self.rows());
        let tops = layout.tops(&heights);
        let place = self.place_mut();
        let Some(&top) = tops.get(place.focus) else { return };
        let top = top - place.scroll;
        let bottom = top + heights[place.focus];
        if top < ROW_TOP {
            place.scroll -= ROW_TOP - top;
        } else if bottom > VIEW_BOTTOM {
            place.scroll += bottom - VIEW_BOTTOM;
        }
        place.scroll = place.scroll.clamp(0.0, layout.most_scroll(&heights));
    }

    /// A row's choices, on rules and Archipelago option pages.
    fn pills_of(&self, row: &Row) -> Option<Pills> {
        match row.target {
            Target::Option(option) => Some(self.rules.pills(&OPTIONS[option])),
            Target::ApOption(option) => Some(self.ap_pills(option)),
            _ => None,
        }
    }

    /// Each row's height on the current page.
    fn heights(&self, rows: &[Row]) -> Vec<f32> {
        let layout = Layout::of(self.page());
        rows.iter()
            .map(|row| {
                let extra = self.pills_of(row).map_or(0, |pills| pills.lines() - 1);
                layout.height + extra as f32 * PILL_LINE
            })
            .collect()
    }



    /// True on the first frame of a press, then again at a steady rate while held.
    fn repeats(pressed: bool, held: bool, held_frames: u32) -> bool {
        pressed || (held && held_frames >= REPEAT_DELAY && (held_frames - REPEAT_DELAY) % REPEAT_EVERY == 0)
    }

    /// The row under a touch, and the touch's height inside that row.
    fn row_at(place: &Place, heights: &[f32], x: f32, y: f32) -> Option<(usize, f32)> {
        let layout = Layout::of(place.page);
        if !(ROW_LEFT..ROW_LEFT + ROW_WIDTH).contains(&x) || y >= HINT_TOP {
            return None;
        }
        layout.tops(heights).into_iter().zip(heights).enumerate().find_map(|(index, (top, height))| {
            let top = top - place.scroll;
            (top..top + height).contains(&y).then_some((index, y - top))
        })
    }

    /// The value a tap on a card's pills picks, if it lands on one.
    fn pill_at(&self, option: usize, x: f32, within: f32) -> Option<Pick> {
        Self::pick_in(self.rules.pills(&OPTIONS[option]), x, within)
    }

    fn pick_in(pills: Pills, x: f32, within: f32) -> Option<Pick> {
        let hit = |(left, width): (f32, f32)| (left..left + width).contains(&x);
        match pills {
            Pills::Choices { names, .. } => pill_boxes(names.len())
                .into_iter()
                .position(|(left, top, width)| hit((left, width)) && (PILL_TOP + top..PILL_TOP + top + PILL_HEIGHT).contains(&within))
                .map(Pick::Choice),
            Pills::Number { .. } => {
                if !(PILL_TOP..PILL_TOP + PILL_HEIGHT).contains(&within) {
                    return None;
                }
                let [minus, plus] = step_lefts();
                if hit((minus, STEP_WIDTH)) {
                    Some(Pick::Step(-1))
                } else if hit((plus, STEP_WIDTH)) {
                    Some(Pick::Step(1))
                } else {
                    None
                }
            }
        }
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
                    let layout = Layout::of(self.page());
                    let heights = self.heights(rows);
                    let tops = layout.tops(&heights);
                    let place = self.place_mut();
                    place.scroll = (state.scroll_at_start - moved).clamp(0.0, layout.most_scroll(&heights));
                    // Keep focus on a row shown whole: the first one if it scrolled off the top,
                    // the last one if it scrolled off the bottom.
                    let whole: Vec<usize> = (0..heights.len())
                        .filter(|&index| tops[index] - place.scroll >= ROW_TOP - 0.5 && tops[index] - place.scroll + heights[index] <= VIEW_BOTTOM + 0.5)
                        .collect();
                    if let (Some(&first), Some(&last)) = (whole.first(), whole.last()) {
                        place.focus = place.focus.clamp(first, last);
                    }
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
                let Some((index, within)) = Self::row_at(self.place(), &self.heights(rows), x, y) else {
                    return Action::None;
                };
                self.place_mut().focus = index;
                self.follow_focus();
                match rows[index].target {
                    // A tap on a pill picks it. A tap anywhere else on the card only focuses it.
                    Target::Option(option) => {
                        match self.pill_at(option, x, within) {
                            Some(Pick::Choice(choice)) => self.pick(option, choice),
                            Some(Pick::Step(delta)) => self.adjust(option, delta),
                            None => {}
                        }
                        Action::None
                    }
                    Target::ApOption(option) => {
                        match Self::pick_in(self.ap_pills(option), x, within) {
                            Some(Pick::Choice(choice)) => {
                                self.ap_options.select(option, choice);
                                self.ap_changed = true;
                            }
                            Some(Pick::Step(delta)) => self.ap_step(option, delta),
                            None => {}
                        }
                        Action::None
                    }
                    // Left of the value's center steps back, and the rest steps forward.
                    Target::Mod(kind) => {
                        let delta = if (VALUE_LEFT..VALUE_CENTER).contains(&x) { -1 } else { 1 };
                        self.step_mod(kind, delta);
                        Action::None
                    }
                    Target::Look(look) => {
                        let delta = if (VALUE_LEFT..VALUE_CENTER).contains(&x) { -1 } else { 1 };
                        self.step_look(look, delta);
                        Action::None
                    }
                    target => self.activate(target),
                }
            }
            (None, None) => Action::None,
        }
    }

    /// Handles one frame of input. Afterwards, `take_sound` gives the sound it calls for.
    pub fn update(&mut self, input: Input) -> Action {
        let before = (self.stack.len(), self.page(), self.place().focus, self.value_changes);
        let action = self.handle(input);
        let after = (self.stack.len(), self.page(), self.place().focus, self.value_changes);
        let sound = if matches!(action, Action::Generate | Action::PatchArchipelago(_)) {
            Some(Sound::Start)
        } else if after.0 < before.0 {
            Some(Sound::Back)
        } else if after.0 > before.0 {
            Some(Sound::Select)
        } else if after.1 != before.1 {
            Some(Sound::PageChange)
        } else if after.3 != before.3 {
            Some(Sound::Choose)
        } else if action != Action::None {
            Some(Sound::Select)
        } else if after.2 != before.2 {
            Some(Sound::Hover)
        } else {
            None
        };
        if sound.is_some() {
            self.sound = sound;
        }
        action
    }

    /// The sound the last updates called for, once.
    pub fn take_sound(&mut self) -> Option<Sound> {
        self.sound.take()
    }

    fn handle(&mut self, input: Input) -> Action {
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
        if let Some(Target::Mod(kind)) = focused {
            if Self::repeats(pressed.left, held.left, frames) {
                self.step_mod(kind, -1);
            }
            if Self::repeats(pressed.right, held.right, frames) {
                self.step_mod(kind, 1);
            }
        }
        if let Some(Target::ApOption(option)) = focused {
            if Self::repeats(pressed.left, held.left, frames) {
                self.ap_step(option, -1);
            }
            if Self::repeats(pressed.right, held.right, frames) {
                self.ap_step(option, 1);
            }
        }
        if let Some(Target::Look(look)) = focused {
            if Self::repeats(pressed.left, held.left, frames) {
                self.step_look(look, -1);
            }
            if Self::repeats(pressed.right, held.right, frames) {
                self.step_look(look, 1);
            }
        }
        if self.page() == Page::Spoiler {
            let step = i32::from(pressed.r) - i32::from(pressed.l);
            if step != 0 {
                self.jump_group(step);
                return Action::None;
            }
            if pressed.y {
                return Action::SearchSpoiler;
            }
            if pressed.x && !self.search.is_empty() {
                self.set_search("");
                return Action::None;
            }
        }
        if let Page::ApOptions(category) = self.page() {
            let step = i32::from(pressed.r) - i32::from(pressed.l);
            if step != 0 {
                let next = (category as i32 + step).rem_euclid(AP_CATEGORIES.len() as i32) as usize;
                *self.place_mut() = Place { page: Page::ApOptions(next), focus: 0, scroll: 0.0 };
                return Action::None;
            }
        }
        if let Page::Options(category) = self.page() {
            let step = i32::from(pressed.r) - i32::from(pressed.l);
            if step != 0 {
                let next = (category as i32 + step).rem_euclid(CATEGORIES.len() as i32) as usize;
                *self.place_mut() = Place { page: Page::Options(next), focus: 0, scroll: 0.0 };
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
        if pressed.y && self.page() == Page::Mods {
            return Action::PlayVoice;
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
        let layout = Layout::of(place.page);
        let heights = self.heights(&rows);
        let tops = layout.tops(&heights);
        // Every row with any part above the hint bar, including ones cut off at either edge.
        let row_views = rows
            .iter()
            .enumerate()
            .filter(|&(index, _)| tops[index] - place.scroll < HINT_TOP && tops[index] - place.scroll + heights[index] > 0.0)
            .map(|(index, row)| RowView {
                label: row.label.clone(),
                value: row.value.clone(),
                focused: index == focus,
                adjustable: matches!(row.target, Target::Option(_) | Target::ApOption(_) | Target::Mod(_) | Target::Look(_)),
                icon: row.icon,
                pills: self.pills_of(row),
                top: tops[index] - place.scroll,
                height: heights[index],
            })
            .collect();

        let (title, subtitle, body, hint): (String, String, Vec<String>, &str) = match place.page {
            Page::OldWarning => (
                "Old 3DS Notice".to_owned(),
                "This console may not finish".to_owned(),
                paragraph(&format!(
                        "Building a seed takes much longer on an Old 3DS, so keep it plugged in. The Standard preset has been tested here, but other rules may need more memory. If the app crashes, make your seed on a computer. Follow the instructions at {COMPUTER_INSTRUCTIONS}"
                    )),
                "A Select   B Exit",
            ),
            Page::Home => {
                let mut lines = paragraph(&self.game_location);
                lines.push(String::new());
                lines.extend(paragraph(help));
                ("A Link Between Worlds".to_owned(), "Randomizer".to_owned(), lines, "A Select   START Make a Seed")
            }
            Page::Categories => ("Rules".to_owned(), format!("Preset: {}", self.preset_name), paragraph(help), "A Open   B Back"),
            Page::Options(category) => (
                CATEGORIES[category].0.to_owned(),
                format!("Preset: {}", self.preset_name),
                paragraph(help),
                "Left/Right Choose   L/R Page   B Back",
            ),
            Page::Presets => ("Presets".to_owned(), format!("Current: {}", self.preset_name), paragraph(help), "A Load   B Back"),
            Page::OfferPreset => (
                "Save These Rules?".to_owned(),
                String::new(),
                paragraph("You changed the rules. Save them as a preset to use them again or share them."),
                "A Select   B Not Now",
            ),
            Page::ReplacePreset => {
                let (name, existing) = self.replacing.clone().unwrap_or_default();
                let text = if name.eq_ignore_ascii_case(&existing) {
                    format!("A preset named \"{existing}\" already exists. Replace it with these rules?")
                } else {
                    format!("\"{name}\" would use the same file as the preset \"{existing}\". Replace that preset with these rules?")
                };
                ("Replace Preset?".to_owned(), String::new(), paragraph(&text), "A Select   B Cancel")
            }
            Page::Mods => (
                "Models and Voices".to_owned(),
                "Mods keep their authors' credit".to_owned(),
                paragraph(help),
                "Left/Right Change   Y Hear Voice   B Back",
            ),
            Page::Confirm => {
                let mut lines = vec![format!("Seed {}   Preset: {}", self.seed, self.preset_name)];
                if self.model().is_some() || self.voice().is_some() {
                    lines.push(format!("Model: {}   Voice: {}", self.model.text(), self.voice.text()));
                }
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
            Page::Archipelago => {
                let mut lines = paragraph(help);
                if self.focused_target(focus, &rows) == Some(Target::StartSharing) {
                    lines.push(format!("apworld on the SD card: {}.", self.apworld_status));
                }
                ("Archipelago".to_owned(), "Multiworld with other players".to_owned(), lines, "A Select   B Back")
            }
            Page::ApCategories => ("Archipelago Options".to_owned(), format!("For {}", self.player_name), paragraph(help), "A Select   B Back"),
            Page::ApOptions(category) => (
                AP_CATEGORIES[category].to_owned(),
                "Archipelago Options".to_owned(),
                paragraph(help),
                "Left/Right Choose   L/R Page   B Back",
            ),
            Page::Sharing => (
                "Sharing on Network".to_owned(),
                "Open this on a computer on your Wi-Fi".to_owned(),
                self.share_status.clone(),
                "B Stop Sharing",
            ),
            Page::Patches => ("Archipelago".to_owned(), "Patch files from your host".to_owned(), paragraph(help), "A Open   B Back"),
            Page::Patch => {
                let mut lines = vec![
                    format!("Player: {}", self.patch.player),
                    format!("Seed {}   apworld {}", self.patch.seed, self.patch.version),
                ];
                if self.focused_target(focus, &rows) == Some(Target::ApplyPatch) {
                    lines.extend(self.patch.lines.iter().cloned());
                } else {
                    lines.extend(paragraph(help));
                }
                ("Archipelago Patch".to_owned(), self.patch.file.clone(), lines, "A Select   Left/Right Change   B Back")
            }
            Page::Spoilers => ("Spoiler Logs".to_owned(), "Newest first".to_owned(), paragraph(help), "A Open   B Back"),
            Page::Spoiler => {
                let subtitle = if self.search.is_empty() {
                    format!("Seed {}   {} entries", self.spoiler.seed, self.found.len())
                } else {
                    format!("\"{}\": {} found", self.search, self.found.len())
                };
                let mut lines = Vec::new();
                if let Some(entry) = self.found.get(focus).map(|&index| &self.spoiler.entries[index]) {
                    lines.push(entry.group.clone());
                    lines.push(entry.place.clone());
                    lines.push(format!("Holds: {}", entry.item));
                } else {
                    lines.push("Nothing matches every word of the search. Try fewer or shorter words.".to_owned());
                }
                lines.push(String::new());
                lines.push(format!("Hash: {}", self.spoiler.hash));
                let hint = if self.search.is_empty() { "Y Search   L/R Group   B Back" } else { "Y Search   X Clear   L/R Group   B Back" };
                ("Spoiler Log".to_owned(), subtitle, lines, hint)
            }
            Page::About => {
                let mut lines = rows.get(focus).map(|row| vec![row.label.clone(), row.value.clone().unwrap_or_default()]).unwrap_or_default();
                lines.extend(paragraph(help));
                let title = if self.app_version.is_empty() { "About and Credits".to_owned() } else { format!("About and Credits  v{}", self.app_version) };
                (title, "Open source under GPL-2.0-or-later".to_owned(), lines, "Up/Down Scroll   B Back")
            }
        };

        View {
            title,
            subtitle,
            body,
            rows: row_views,
            hint: hint.to_owned(),
            scroll: place.scroll,
            total_rows: rows.len(),
            content_height: layout.content_height(&heights),
            layout,
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

    /// The middle of a pill on a rules card, as (x, y).
    fn pill(menu: &Menu, slot: usize, choice: usize, count: usize) -> (f32, f32) {
        let (left, top, width) = pill_boxes(count)[choice];
        (left + width / 2.0, menu.view().rows[slot].top + PILL_TOP + top + PILL_HEIGHT / 2.0)
    }

    #[test]
    fn mods_cycle_through_original_and_each_name() {
        let mut menu = Menu::new(1, Vec::new(), false);
        menu.set_mods(vec!["Wolf".to_owned(), "Deity".to_owned()], Vec::new());
        assert_eq!(menu.model(), None);
        menu.mod_choice(ModKind::Model).step(1);
        assert_eq!(menu.model(), Some("Wolf"));
        menu.mod_choice(ModKind::Model).step(1);
        assert_eq!(menu.model(), Some("Deity"));
        menu.mod_choice(ModKind::Model).step(1);
        assert_eq!(menu.model(), None);
        menu.mod_choice(ModKind::Model).step(-1);
        assert_eq!(menu.model(), Some("Deity"));
        // With no voices on the SD card, the choice stays on the original.
        menu.mod_choice(ModKind::Voice).step(1);
        assert_eq!(menu.voice(), None);
    }

    #[test]
    fn long_mod_names_are_shortened_for_the_row() {
        let choice = ModChoice { names: vec!["A very long mod folder name".to_owned()], selected: 1 };
        assert_eq!(choice.text(), "A very long m...");
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
        // Exit is the eighth Home row, below the screen until the list scrolls.
        for _ in 0..7 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        let view = menu.view();
        assert!(view.scroll > 0.0);
        let exit = view.rows.iter().find(|row| row.focused).unwrap();
        assert_eq!(exit.label, "Exit");
        assert!(exit.top >= ROW_TOP && exit.top + exit.height <= VIEW_BOTTOM);
    }

    #[test]
    fn updates_call_for_the_right_sounds() {
        let mut menu = menu();
        press(&mut menu, |b| b.down = true);
        assert_eq!(menu.take_sound(), Some(Sound::Hover));
        assert_eq!(menu.take_sound(), None);
        release(&mut menu);
        press(&mut menu, |b| b.a = true);
        assert_eq!(menu.take_sound(), Some(Sound::Select));
        release(&mut menu);
        // The first rules category, then its first option.
        press(&mut menu, |b| b.a = true);
        release(&mut menu);
        menu.take_sound();
        press(&mut menu, |b| b.right = true);
        assert_eq!(menu.take_sound(), Some(Sound::Choose));
        release(&mut menu);
        press(&mut menu, |b| b.r = true);
        assert_eq!(menu.take_sound(), Some(Sound::PageChange));
        release(&mut menu);
        press(&mut menu, |b| b.b = true);
        assert_eq!(menu.take_sound(), Some(Sound::Back));
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
    fn tapping_a_pill_picks_it() {
        let mut menu = menu();
        menu.open(Page::Options(0));
        let value = |menu: &Menu| menu.view().rows[0].value.clone().unwrap();
        assert_eq!(value(&menu), "Normal");
        // Logic has six choices. The fourth is Adv. Glitched.
        let (x, y) = pill(&menu, 0, 3, 6);
        tap(&mut menu, x, y);
        assert_eq!(value(&menu), "Adv. Glitched");
        assert_eq!(menu.preset_name, "Custom");
        // The card's name line only focuses it.
        let before = menu.view().rows[1].value.clone();
        let name_line = menu.view().rows[1].top + 10.0;
        tap(&mut menu, x, name_line);
        assert!(menu.view().rows[1].focused);
        assert_eq!(menu.view().rows[1].value, before);
    }

    #[test]
    fn toggles_and_numbers_tap_as_pills() {
        let mut menu = menu();
        menu.open(Page::Options(2));
        // Swordless is the fourth Items card; scroll it into view.
        for _ in 0..3 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        let view = menu.view();
        assert!(view.scroll > 0.0);
        let slot = view.rows.iter().position(|row| row.focused).unwrap();
        assert_eq!(view.rows[slot].pills, Some(Pills::Choices { names: vec!["Off".into(), "On".into()], selected: 0 }));
        let (x, y) = pill(&menu, slot, 1, 2);
        tap(&mut menu, x, y);
        assert!(menu.rules.settings().swordless_mode);
        // Maiamai Limit's plus button adds one step.
        menu.open(Page::Options(2));
        for _ in 0..7 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        let focused = |menu: &Menu| menu.view().rows.into_iter().find(|row| row.focused).unwrap();
        let before = focused(&menu).value.unwrap().parse::<i64>().unwrap();
        let plus = focused(&menu).top + PILL_TOP + 4.0;
        tap(&mut menu, step_lefts()[1] + 4.0, plus);
        let after = focused(&menu).value.unwrap().parse::<i64>().unwrap();
        assert_eq!(after, before + 5);
    }

    #[test]
    fn left_and_right_still_change_the_choice() {
        let mut menu = menu();
        menu.open(Page::Options(0));
        press(&mut menu, |b| b.right = true);
        release(&mut menu);
        assert_eq!(menu.view().rows[0].pills, Some(Pills::Choices {
            names: ["Normal", "Hard", "Glitched", "Adv. Glitched", "Hell", "No Logic"].map(String::from).to_vec(),
            selected: 1,
        }));
        press(&mut menu, |b| b.left = true);
        release(&mut menu);
        assert_eq!(menu.view().rows[0].value.as_deref(), Some("Normal"));
        assert_eq!(menu.view().layout, Layout::CARDS);
        // Logic's pills take two lines, so its card is taller and the third card shows cut off.
        assert_eq!(menu.view().rows[0].height, CARD_HEIGHT + PILL_LINE);
        assert_eq!(menu.view().rows.len(), 3);
    }

    #[test]
    fn dragging_scrolls_and_does_not_tap() {
        let mut menu = menu();
        let drag = |menu: &mut Menu, y: f32| menu.update(Input { touch: Some((100.0, y)), ..Input::default() });
        drag(&mut menu, 150.0);
        drag(&mut menu, 100.0);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Home);
        // The list follows the finger by the pixel.
        assert_eq!(menu.view().scroll, 50.0);
    }

    #[test]
    fn cards_below_show_cut_off_and_taps_on_them_scroll_them_in() {
        let mut menu = menu();
        // Logic's card is tall, so the third card on the page is only partly shown.
        menu.open(Page::Options(0));
        let view = menu.view();
        let last = view.rows.last().unwrap();
        assert_eq!(view.rows.len(), 3);
        assert!(last.top < HINT_TOP && last.top + last.height > VIEW_BOTTOM);
        tap(&mut menu, 100.0, last.top + 8.0);
        let view = menu.view();
        let focused = view.rows.iter().find(|row| row.focused).unwrap();
        assert_eq!(focused.label, last.label);
        assert!(focused.top + focused.height <= VIEW_BOTTOM);
        // Nothing under the hint bar can be tapped.
        assert!(Menu::row_at(menu.place(), &menu.heights(&menu.rows()), 100.0, HINT_TOP + 2.0).is_none());
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

    fn sd_preset(name: &str, file: &str) -> Preset {
        Preset { name: name.to_owned(), description: String::new(), overrides: Default::default(), built_in: false, file: Some(file.to_owned()) }
    }

    #[test]
    fn leaving_changed_rules_offers_a_preset_once() {
        let mut menu = menu();
        menu.open(Page::Categories);
        menu.open(Page::Options(0));
        let (x, y) = pill(&menu, 0, 1, 6);
        tap(&mut menu, x, y);
        press(&mut menu, |b| b.b = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Categories);
        press(&mut menu, |b| b.b = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::OfferPreset);
        assert_eq!(press(&mut menu, |b| b.a = true), Action::SavePreset);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Home);
        // Without new changes, leaving again asks nothing.
        menu.open(Page::Categories);
        press(&mut menu, |b| b.b = true);
        assert_eq!(menu.page(), Page::Home);
    }

    #[test]
    fn leaving_unchanged_or_preset_rules_offers_nothing() {
        let mut menu = menu();
        menu.open(Page::Categories);
        press(&mut menu, |b| b.b = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Home);
        // Changing a rule, then loading a preset, leaves nothing new to save.
        menu.open(Page::Categories);
        menu.open(Page::Options(0));
        let (x, y) = pill(&menu, 0, 1, 6);
        tap(&mut menu, x, y);
        menu.select_preset("Casual");
        menu.back();
        menu.back();
        assert_eq!(menu.page(), Page::Home);
    }

    #[test]
    fn names_in_use_are_found_by_name_or_file() {
        let mut menu = menu();
        menu.set_presets(vec![sd_preset("My Rules", "My_Rules.json")]);
        assert_eq!(menu.preset_in_the_way("my rules ", "my_rules_.json").map(|p| p.name.as_str()), Some("My Rules"));
        assert_eq!(menu.preset_in_the_way("My-Rules", "My_Rules.json").map(|p| p.name.as_str()), Some("My Rules"));
        assert!(menu.preset_in_the_way("Other", "Other.json").is_none());
    }

    #[test]
    fn replace_question_answers() {
        let mut menu = menu();
        menu.ask_to_replace("My Rules", "My Rules");
        assert!(menu.view().body[0].contains("already exists"));
        assert_eq!(press(&mut menu, |b| b.a = true), Action::ReplacePreset("My Rules".to_owned()));
        release(&mut menu);
        assert_eq!(menu.page(), Page::Home);

        menu.ask_to_replace("My Rules", "My Rules");
        press(&mut menu, |b| b.down = true);
        release(&mut menu);
        assert_eq!(press(&mut menu, |b| b.a = true), Action::SavePreset);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Home);

        menu.ask_to_replace("My Rules", "My Rules");
        press(&mut menu, |b| b.b = true);
        assert_eq!(menu.page(), Page::Home);
    }

    #[test]
    fn y_plays_a_voice_only_on_the_mods_page() {
        let mut menu = menu();
        assert_eq!(press(&mut menu, |b| b.y = true), Action::None);
        release(&mut menu);
        menu.open_mods();
        assert_eq!(press(&mut menu, |b| b.y = true), Action::PlayVoice);
    }

    #[test]
    fn confirm_starts_generation_and_lists_changes() {
        let mut menu = menu();
        menu.rules.step(&OPTIONS[0], 1);
        press(&mut menu, |b| b.a = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Confirm);
        assert!(menu.view().body.iter().any(|line| line == "Logic: Hard"));
        assert!(menu.view().rows.iter().any(|row| row.focused && row.label == START_RANDOMIZING));
        assert_eq!(press(&mut menu, |b| b.a = true), Action::Generate);
        release(&mut menu);
        // The seed rows sit above it.
        press(&mut menu, |b| b.up = true);
        release(&mut menu);
        assert_eq!(press(&mut menu, |b| b.a = true), Action::RandomSeed);
        release(&mut menu);
        press(&mut menu, |b| b.up = true);
        release(&mut menu);
        assert_eq!(press(&mut menu, |b| b.a = true), Action::EditSeed);
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

    fn sample_spoiler() -> Spoiler {
        let entry = |group: &str, place: &str, item: &str| crate::spoiler::Entry { group: group.into(), place: place.into(), item: item.into() };
        Spoiler {
            seed: 77,
            hash: "(B) (Y)".into(),
            entries: vec![
                entry("Hyrule: Desert", "Desert Chest", "Bow+"),
                entry("Hyrule: Desert", "Desert Ledge", "Red Rupee"),
                entry("Lorule: Mire", "Mire Chest", "Hookshot+"),
                entry("Dungeons: Eastern Palace", "[EP] Entrance", "Bow of Light"),
            ],
        }
    }

    #[test]
    fn spoiler_logs_open_from_home_and_search() {
        let mut menu = menu();
        menu.place_mut().focus = 4;
        assert_eq!(press(&mut menu, |b| b.a = true), Action::ListSpoilers);
        release(&mut menu);
        menu.show_spoiler_files(vec!["0002-seed-77".into(), "0001-seed-5".into()]);
        assert_eq!(menu.page(), Page::Spoilers);
        assert_eq!(menu.view().rows[0].label, "Seed 77");
        assert_eq!(press(&mut menu, |b| b.a = true), Action::OpenSpoiler("0002-seed-77".into()));
        release(&mut menu);

        menu.show_spoiler(sample_spoiler());
        let view = menu.view();
        assert_eq!(view.layout, Layout::LINES);
        assert_eq!(view.rows.len(), 4);
        assert_eq!((view.rows[0].label.as_str(), view.rows[0].value.as_deref()), ("Desert Chest", Some("Bow+")));
        assert_eq!(press(&mut menu, |b| b.y = true), Action::SearchSpoiler);
        release(&mut menu);

        menu.set_search("bow");
        let labels = |menu: &Menu| menu.view().rows.iter().map(|row| row.label.clone()).collect::<Vec<_>>();
        assert_eq!(labels(&menu), ["Desert Chest", "[EP] Entrance"]);
        assert!(menu.view().subtitle.contains("2 found"));
        menu.set_search("zzz");
        assert_eq!(labels(&menu), ["No Matches"]);
        press(&mut menu, |b| b.x = true);
        release(&mut menu);
        assert_eq!(labels(&menu).len(), 4);
        assert_eq!(menu.search(), "");
    }

    #[test]
    fn shoulder_buttons_jump_between_spoiler_groups() {
        let mut menu = menu();
        menu.show_spoiler(sample_spoiler());
        let focused = |menu: &Menu| menu.view().rows.iter().find(|row| row.focused).map(|row| row.label.clone());
        press(&mut menu, |b| b.r = true);
        release(&mut menu);
        assert_eq!(focused(&menu).as_deref(), Some("Mire Chest"));
        press(&mut menu, |b| b.r = true);
        release(&mut menu);
        assert_eq!(focused(&menu).as_deref(), Some("[EP] Entrance"));
        press(&mut menu, |b| b.l = true);
        release(&mut menu);
        assert_eq!(focused(&menu).as_deref(), Some("Mire Chest"));
        press(&mut menu, |b| b.l = true);
        release(&mut menu);
        assert_eq!(focused(&menu).as_deref(), Some("Desert Chest"));
    }

    #[test]
    fn credits_scroll_instead_of_shrinking() {
        let mut menu = menu();
        menu.open(Page::About);
        let view = menu.view();
        assert_eq!(view.layout, Layout::LINES);
        assert_eq!(view.total_rows, CREDITS.len());
        assert!(view.body.iter().any(|line| line == "Faith Branch Software LLC"));
        for _ in 1..CREDITS.len() {
            press(&mut menu, |buttons| buttons.down = true);
            release(&mut menu);
        }
        let view = menu.view();
        assert!(view.scroll > 0.0);
        assert!(view.body.iter().any(|line| line == "Nintendo"));
    }

    #[test]
    fn archipelago_options_change_and_save_once() {
        let mut menu = menu();
        menu.open(Page::Archipelago);
        for _ in 0..2 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        press(&mut menu, |b| b.a = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::ApCategories);
        // The first category, Multiworld, is under Copy from Rules and Reset to Defaults.
        for _ in 0..2 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        press(&mut menu, |b| b.a = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::ApOptions(0));
        assert_eq!(menu.take_ap_changes(), None);
        // Death Link is the third option on that page.
        for _ in 0..2 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        press(&mut menu, |b| b.right = true);
        release(&mut menu);
        assert!(menu.options_file().contains("  death_link: true\n"));
        assert!(menu.take_ap_changes().unwrap().contains("death_link: true\n"));
        assert_eq!(menu.take_ap_changes(), None);
        // R moves to the next page.
        press(&mut menu, |b| b.r = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::ApOptions(1));
    }

    #[test]
    fn archipelago_patches_open_from_home_and_apply() {
        let mut menu = menu();
        let row = menu.rows().iter().position(|row| row.target == Target::Open(Page::Archipelago)).unwrap();
        for _ in 0..row {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        press(&mut menu, |b| b.a = true);
        release(&mut menu);
        assert_eq!(menu.page(), Page::Archipelago);
        for _ in 0..3 {
            press(&mut menu, |b| b.down = true);
            release(&mut menu);
        }
        assert!(matches!(press(&mut menu, |b| b.a = true), Action::ListPatches));
        release(&mut menu);
        menu.show_patches(vec!["AP_1_P1_Link.apalbw3ds".to_owned()]);
        assert_eq!(menu.page(), Page::Patches);
        assert!(matches!(press(&mut menu, |b| b.a = true), Action::OpenPatch(name) if name == "AP_1_P1_Link.apalbw3ds"));
        release(&mut menu);
        menu.show_patch(PatchSummary { file: "AP_1_P1_Link.apalbw3ds".to_owned(), player: "Link".to_owned(), seed: 7, version: "0.4.0".to_owned(), lines: vec![] });
        // Night Mode is the second row: change it with Right.
        press(&mut menu, |b| b.down = true);
        release(&mut menu);
        assert_eq!(menu.look(Look::NightMode), None);
        press(&mut menu, |b| b.right = true);
        release(&mut menu);
        assert_eq!(menu.look(Look::NightMode), Some(true));
        press(&mut menu, |b| b.up = true);
        release(&mut menu);
        assert!(matches!(press(&mut menu, |b| b.a = true), Action::PatchArchipelago(name) if name == "AP_1_P1_Link.apalbw3ds"));
    }

    #[test]
    fn no_patches_shows_where_to_put_them() {
        let mut menu = menu();
        menu.show_patches(Vec::new());
        assert_eq!(menu.page(), Page::Message);
        assert!(menu.view().body.iter().any(|line| line.contains("/3ds/albw-rando/ap")));
    }

    #[test]
    fn no_spoiler_logs_shows_a_message() {
        let mut menu = menu();
        menu.show_spoiler_files(Vec::new());
        assert_eq!(menu.page(), Page::Message);
    }

    #[test]
    fn wrap_respects_width() {
        let lines = wrap("one two three four five six", 9);
        assert!(lines.iter().all(|line| line.len() <= 9));
        assert_eq!(lines.join(" "), "one two three four five six");
    }
}
