//! A Link Between Worlds randomizer for the 3DS.
//!
//! The menus run on the main thread. Making a seed runs on a worker thread with lower priority. The
//! main thread sleeps between frames, so the kernel lets it interrupt the work on time. A New 3DS
//! gives the worker its own CPU core.

#![feature(allocator_api)]

mod art;
mod draw;
mod icons;
mod job;
mod memory;
mod menu_screen;
mod model_view;
mod progress;
mod share;
mod voice;

use std::fs;
use std::path::Path;

use ctru::applets::swkbd::{Button, ButtonConfig, Kind as KeyboardKind, SoftwareKeyboard};
use ctru::prelude::*;
use ui_core::menu::{Action, Buttons, Input, Menu, ModKind, Page};
use ui_core::presets::{self, Preset};

const MB: f32 = 1024.0 * 1024.0;
const WORKER_STACK: usize = 2 * 1024 * 1024;
/// Memory left outside the pool for small objects and thread stacks.
const OLD_3DS_RESERVE_MB: usize = 18;
const NEW_3DS_RESERVE_MB: usize = 32;
/// The heap measured on a real Old 3DS, used when simulating one.
const OLD_3DS_HEAP: usize = 52 * 1024 * 1024;

// libctru's default split leaves an Old 3DS about 24 MB of heap. Graphics here need only a few MB
// of linear memory, so the heap gets the rest. These override weak symbols in libctru.
#[no_mangle]
#[used]
static mut __ctru_linear_heap_size: u32 = 8 * 1024 * 1024;

extern "C" {
    static __ctru_heap_size: u32;
}

fn random_seed() -> u32 {
    // SAFETY: Reads the system clock only. The mix spreads similar times across the seed range.
    let time = unsafe { ctru_sys::osGetTime() };
    (time as u32).wrapping_mul(2_654_435_761) ^ (time >> 32) as u32
}

/// Built-in presets, then any files in the presets folder. Also returns notes about file problems.
fn load_presets() -> (Vec<Preset>, Vec<String>) {
    let mut list = presets::built_in();
    let mut notes = Vec::new();
    let Ok(entries) = fs::read_dir(job::PRESET_DIR) else {
        return (list, notes);
    };
    let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    files.sort();
    for path in files.iter().filter(|path| path.extension().is_some_and(|ext| ext == "json")) {
        let stem = path.file_stem().map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let parsed = fs::read_to_string(path).map_err(|err| err.to_string()).and_then(|text| presets::parse(&text, &stem));
        match parsed {
            Ok((mut preset, skipped)) => {
                preset.file = path.file_name().map(|name| name.to_string_lossy().into_owned());
                if !skipped.is_empty() {
                    notes.push(format!("{}: skipped {}.", preset.name, skipped.join(", ")));
                }
                list.push(preset);
            }
            Err(problem) => notes.push(format!("{stem}: {problem}")),
        }
    }
    (list, notes)
}

fn read_input(hid: &Hid) -> Input {
    let to_buttons = |keys: KeyPad| Buttons {
        up: keys.intersects(KeyPad::DPAD_UP | KeyPad::CPAD_UP),
        down: keys.intersects(KeyPad::DPAD_DOWN | KeyPad::CPAD_DOWN),
        left: keys.intersects(KeyPad::DPAD_LEFT | KeyPad::CPAD_LEFT),
        right: keys.intersects(KeyPad::DPAD_RIGHT | KeyPad::CPAD_RIGHT),
        a: keys.contains(KeyPad::A),
        b: keys.contains(KeyPad::B),
        l: keys.contains(KeyPad::L),
        r: keys.contains(KeyPad::R),
        x: keys.contains(KeyPad::X),
        y: keys.contains(KeyPad::Y),
        start: keys.contains(KeyPad::START),
    };
    let touch = hid.keys_held().contains(KeyPad::TOUCH).then(|| {
        let (x, y) = hid.touch_position();
        (f32::from(x), f32::from(y))
    });
    Input { pressed: to_buttons(hid.keys_down()), held: to_buttons(hid.keys_held()), touch }
}

fn edit_seed(menu: &mut Menu, seed_was_typed: &mut bool, apt: &Apt, gfx: &Gfx) {
    let mut keyboard = SoftwareKeyboard::new(KeyboardKind::Numpad, ButtonConfig::LeftRight);
    keyboard.set_hint_text(Some("Seed number"));
    if let Ok((text, Button::Right)) = keyboard.launch(apt, gfx) {
        match text.trim().parse() {
            Ok(seed) => {
                menu.seed = seed;
                *seed_was_typed = true;
            }
            Err(_) => menu.show_message("Not a Seed Number", "Use a whole number from 0 to 4294967295."),
        }
    }
}

fn search_spoiler(menu: &mut Menu, apt: &Apt, gfx: &Gfx) {
    let mut keyboard = SoftwareKeyboard::new(KeyboardKind::Normal, ButtonConfig::LeftRight);
    keyboard.set_hint_text(Some("Item or place, such as bow or eastern"));
    keyboard.set_initial_text(Some(menu.search().to_owned().into()));
    if let Ok((query, Button::Right)) = keyboard.launch(apt, gfx) {
        menu.set_search(&query);
    }
}

/// Where the Archipelago player name is kept between runs.
const PLAYER_NAME: &str = "sdmc:/3ds/albw-rando/player-name.txt";
/// The Archipelago options, as `key: value` lines.
const AP_OPTIONS: &str = "sdmc:/3ds/albw-rando/ap-options.yaml";

fn edit_player_name(menu: &mut Menu, apt: &Apt, gfx: &Gfx) {
    let mut keyboard = SoftwareKeyboard::new(KeyboardKind::Normal, ButtonConfig::LeftRight);
    keyboard.set_hint_text(Some("Your name in the multiworld, up to 16 letters"));
    keyboard.set_initial_text(Some(menu.player_name.clone().into()));
    if let Ok((name, Button::Right)) = keyboard.launch(apt, gfx) {
        menu.set_player_name(&name);
        if let Err(err) = fs::write(PLAYER_NAME, &menu.player_name) {
            job::log(&format!("Player name not saved: {err}"));
        }
    }
}

/// What the Archipelago page shows for the apworld on the SD card.
fn apworld_status(version: Option<String>) -> String {
    let needed = ap_patch::apworld_version_needed();
    match version {
        Some(version) if version == needed => version,
        Some(version) => format!("{version}, but the app needs {needed}"),
        None => "missing. The albw.local page links to GitHub".to_owned(),
    }
}

fn save_preset(menu: &mut Menu, apt: &Apt, gfx: &Gfx) {
    let mut keyboard = SoftwareKeyboard::new(KeyboardKind::Normal, ButtonConfig::LeftRight);
    keyboard.set_hint_text(Some("Preset name"));
    let Ok((name, Button::Right)) = keyboard.launch(apt, gfx) else {
        return;
    };
    let name = name.trim().to_owned();
    let Some(file_name) = presets::file_name(&name) else {
        menu.show_message("Name Not Usable", "Use at least one letter or number.");
        return;
    };
    match menu.preset_in_the_way(&name, &file_name) {
        Some(preset) if preset.built_in => {
            let text = format!("\"{}\" is built into the app and cannot be replaced. Pick another name.", preset.name);
            menu.show_message("Name Already Used", &text);
        }
        Some(preset) => {
            let existing = preset.name.clone();
            menu.ask_to_replace(&name, &existing);
        }
        // A file the app could not read still holds the name.
        None if Path::new(&format!("{}/{file_name}", job::PRESET_DIR)).exists() => {
            menu.ask_to_replace(&name, &file_name);
        }
        None => write_preset(menu, &name),
    }
}

/// Saves the rules as a preset, replacing any preset that has the same name or file.
fn write_preset(menu: &mut Menu, name: &str) {
    let Some(file_name) = presets::file_name(name) else {
        return;
    };
    let old_file = menu.preset_in_the_way(name, &file_name).filter(|preset| !preset.built_in).and_then(|preset| preset.file.clone());
    let path = format!("{}/{file_name}", job::PRESET_DIR);
    let written = fs::create_dir_all(job::PRESET_DIR).and_then(|()| fs::write(&path, presets::to_file_text(name, &menu.rules)));
    match written {
        Ok(()) => {
            // The old preset may sit in a file with another name, which would list it twice.
            if let Some(old_file) = old_file.filter(|old| *old != file_name) {
                let _ = fs::remove_file(format!("{}/{old_file}", job::PRESET_DIR));
            }
            menu.preset_name = name.to_owned();
            menu.set_presets(load_presets().0);
            menu.show_message("Preset Saved", &format!("Saved as /3ds/albw-rando/presets/{file_name}. Copy that file to share it."));
        }
        Err(err) => menu.show_message("Could Not Save", &err.to_string()),
    }
}

/// Starts the worker. Returns false if the thread could not start.
fn start_worker(is_new: bool, core_priority: i32) -> bool {
    let core = if is_new { 2 } else { -2 };
    // SAFETY: The worker takes no argument, and detaches when it ends.
    let thread = unsafe { ctru_sys::threadCreate(Some(job::worker), core::ptr::null_mut(), WORKER_STACK, core_priority + 1, core, true) };
    !thread.is_null()
}

fn main() {
    let gfx = Gfx::new().expect("failed to start GFX");
    let mut hid = Hid::new().expect("failed to start HID");
    let apt = Apt::new().expect("failed to start APT");
    let _ = fs::create_dir_all(job::APP_DIR);

    std::panic::set_hook(Box::new(|info| {
        job::log(&format!("PANIC: {info}"));
        progress::fail(&info.to_string());
    }));

    let mut is_new = false;
    let mut priority = 0;
    // SAFETY: Both pointers are valid, and the pseudo-handle refers to this thread.
    unsafe {
        let _ = ctru_sys::APT_CheckNew3DS(&mut is_new);
        let _ = ctru_sys::svcGetThreadPriority(&mut priority, ctru_sys::CUR_THREAD_HANDLE);
        if is_new {
            let _ = ctru_sys::osSetSpeedupEnable(true);
        }
    }
    let new_memory = is_new && !Path::new(job::SIMULATE_OLD).exists();

    // SAFETY: libctru sets this once during startup, before main.
    let heap = unsafe { __ctru_heap_size } as usize;
    let reserve = if new_memory { NEW_3DS_RESERVE_MB } else { OLD_3DS_RESERVE_MB };
    memory::start_pool(if new_memory { heap } else { heap.min(OLD_3DS_HEAP) }, reserve * 1024 * 1024);

    draw::start();
    let previews_work = model_view::start();

    // The first launch reads the icons from the game, which takes a moment.
    draw::frame(
        || draw::text("Looking for your game...", 200.0, 110.0, 0.6, draw::WHITE, draw::CENTER),
        || {},
    );
    let check = job::check_game();
    let game_location = check.summary.clone();
    let mut check_log = check.details.join("\n");
    check_log.push_str(&format!("\n{game_location}\n"));
    let (loaded_icons, icon_note) = icons::load(check);
    check_log.push_str(&format!("{icon_note}\n"));
    let (top_art, art_note) = art::load_top();
    let (skin, skin_note) = art::load_skin();
    let mut display = art::load_display();
    check_log.push_str(&format!("{skin_note}\n"));
    check_log.push_str(&format!("{art_note}\n{}\n", art::load_fonts(display.text)));
    let _ = fs::write(job::CHECK_LOG, check_log);

    let (preset_list, notes) = load_presets();
    let mut menu = Menu::new(random_seed(), preset_list, !new_memory);
    let mut voices = voice::Voices::new();
    menu.set_mods(job::list_mods(job::MODEL_DIR), job::list_mods(job::VOICE_DIR));
    menu.game_location = game_location;
    menu.display = display;
    if let Ok(name) = fs::read_to_string(PLAYER_NAME) {
        menu.set_player_name(&name);
    }
    menu.apworld_status = apworld_status(share::apworld_version());
    if let Ok(text) = fs::read_to_string(AP_OPTIONS) {
        menu.set_ap_options(ui_core::ap_yaml::ApOptions::load(&text));
    }
    // The albw.local page, while the sharing page shows, and what it serves.
    let mut sharing: Option<(share::Share, share::Content, Vec<String>)> = None;
    if !notes.is_empty() {
        menu.show_message("Preset Notes", &notes.join(" "));
    }
    let mut seed_was_typed = false;
    let mut working: Option<progress::Screen> = None;
    let mut vblanks = 0u32;
    // The model preview on the Models and Voices page: which mod it shows, and the loaded model.
    let mut preview: Option<(Option<String>, Option<model_view::Preview>)> = None;
    // Previews being replaced wait two frames before being freed, as the GPU may still read them.
    let mut retired: Vec<(u32, model_view::Preview)> = Vec::new();
    let mut frames = 0u32;
    let mut autostart = Path::new(job::AUTOSTART).exists();
    // For testing: the file's lines name a model, a voice, a seed number, and a preset.
    if let Ok(text) = fs::read_to_string(job::AUTOSTART) {
        let mut lines = text.lines();
        menu.select_mod(ModKind::Model, lines.next().unwrap_or(""));
        menu.select_mod(ModKind::Voice, lines.next().unwrap_or(""));
        if let Some(seed) = lines.next().and_then(|line| line.parse().ok()) {
            menu.seed = seed;
        }
        if let Some(preset) = lines.next() {
            menu.select_preset(preset);
        }
    }

    // For testing: opens a page of rules by number, or "mods" and an optional model name opens the
    // Models and Voices page, and "replace" and a name asks to replace that preset, to check how
    // they look. After "mods", a third line names a voice to play once, or "game" for the original.
    // The result goes to the run log.
    if let Ok(text) = fs::read_to_string(job::TEST_PAGE) {
        let mut lines = text.lines();
        let first = lines.next().unwrap_or("").trim();
        if first == "mods" {
            menu.select_mod(ModKind::Model, lines.next().unwrap_or("").trim());
            menu.open_mods();
            if let Some(voice) = lines.next().map(str::trim).filter(|line| !line.is_empty()) {
                menu.select_mod(ModKind::Voice, voice);
                let result = voices.play(menu.voice());
                job::log(&format!("Test voice {voice}: {result:?}"));
            }
        } else if first == "replace" {
            let name = lines.next().unwrap_or("My Rules").trim();
            menu.ask_to_replace(name, name);
        } else if let Ok(category) = first.parse() {
            menu.open_category(category);
        }
    }

    'app: while apt.main_loop() {
        hid.scan_input();

        if let Some(screen) = working.as_mut() {
            if progress::finished() {
                let down = hid.keys_down();
                if down.contains(KeyPad::START) {
                    break 'app;
                }
                if down.contains(KeyPad::A) {
                    working = None;
                    menu.go_home();
                    menu.seed = random_seed();
                    seed_was_typed = false;
                    continue;
                }
            }
            // Drawing takes time from the work on an Old 3DS, so it draws less often there.
            if vblanks % if is_new { 2 } else { 6 } == 0 {
                screen.draw();
            } else {
                gfx.wait_for_vblank();
            }
            vblanks = vblanks.wrapping_add(1);
            continue;
        }

        if let Some((server, content, received)) = sharing.as_mut() {
            if menu.page() == Page::Sharing {
                for event in server.poll(content) {
                    match event {
                        share::Event::Saved(name) => received.push(format!("Received {name}")),
                        share::Event::Apworld(version) => {
                            received.push(format!("Received apworld {version}"));
                            content.apworld = Some(version.clone());
                            menu.apworld_status = apworld_status(Some(version));
                        }
                    }
                }
                let mut lines = server.address_lines();
                lines.extend(received.iter().rev().take(3).cloned());
                if received.iter().any(|line| line.ends_with(&format!(".{}", ap_patch::EXTENSION))) {
                    lines.push("Press B, then open Patch Files to apply it.".to_owned());
                }
                menu.set_share_status(lines);
            } else {
                job::log("Sharing stopped");
                sharing = None;
            }
        }

        if let Some(text) = menu.take_ap_changes() {
            if let Err(err) = fs::write(AP_OPTIONS, text) {
                job::log(&format!("Could not save the Archipelago options: {err}"));
            }
        }

        let action = if autostart { Action::Generate } else { menu.update(read_input(&hid)) };
        autostart = false;
        match action {
            Action::None => {}
            Action::Exit => break 'app,
            Action::EditSeed => edit_seed(&mut menu, &mut seed_was_typed, &apt, &gfx),
            Action::RandomSeed => {
                menu.seed = random_seed();
                seed_was_typed = false;
            }
            Action::SavePreset => save_preset(&mut menu, &apt, &gfx),
            Action::ReplacePreset(name) => write_preset(&mut menu, &name),
            Action::ListSpoilers => menu.show_spoiler_files(job::list_spoilers()),
            Action::OpenSpoiler(name) => match job::read_spoiler(&name) {
                Ok(spoiler) => menu.show_spoiler(spoiler),
                Err(text) => menu.show_message("Can't Open the Log", &text),
            },
            Action::SearchSpoiler => search_spoiler(&mut menu, &apt, &gfx),
            Action::PlayVoice => {
                if let Err(text) = voices.play(menu.voice()) {
                    menu.show_message("Can't Play the Voice", &text);
                }
            }
            Action::ListPatches => menu.show_patches(job::list_patches()),
            Action::EditPlayerName => edit_player_name(&mut menu, &apt, &gfx),
            Action::StartSharing => match share::Share::start() {
                Ok(server) => {
                    let content = share::Content {
                        player: menu.player_name.clone(),
                        options_file: menu.options_file(),
                        apworld: share::apworld_version(),
                    };
                    let mut lines = server.address_lines();
                    lines.push(format!("Options for {} are ready to download.", menu.player_name));
                    menu.show_sharing(lines);
                    sharing = Some((server, content, Vec::new()));
                }
                Err(text) => menu.show_message("Can't Share", &text),
            },
            Action::OpenPatch(name) => match job::read_patch(&name) {
                Ok(summary) => menu.show_patch(summary),
                Err(text) => menu.show_message("Can't Use This Patch", &text),
            },
            Action::PatchArchipelago(file) => {
                let looks = ap_patch::Looks {
                    night_mode: menu.look(ui_core::menu::Look::NightMode),
                    change_freestanding_models: menu.look(ui_core::menu::Look::FreestandingModels),
                };
                job::set_request(job::Request {
                    seed: 0,
                    seed_was_typed: false,
                    settings: Default::default(),
                    preset_name: String::new(),
                    new_memory,
                    model: menu.model().map(str::to_owned),
                    voice: menu.voice().map(str::to_owned),
                    archipelago: Some((file, looks)),
                });
                progress::reset();
                randomizer::progress::set_hook(job::on_progress);
                working = Some(progress::Screen::new(is_new, heap as f32 / MB));
                if !start_worker(is_new, priority) {
                    progress::fail("Could not start the worker thread.");
                    progress::finish();
                }
                continue;
            }
            Action::Generate => {
                job::set_request(job::Request {
                    seed: menu.seed,
                    seed_was_typed,
                    settings: menu.rules.settings(),
                    preset_name: menu.preset_name.clone(),
                    new_memory,
                    model: menu.model().map(str::to_owned),
                    voice: menu.voice().map(str::to_owned),
                    archipelago: None,
                });
                progress::reset();
                randomizer::progress::set_hook(job::on_progress);
                working = Some(progress::Screen::new(is_new, heap as f32 / MB));
                if !start_worker(is_new, priority) {
                    progress::fail("Could not start the worker thread.");
                    progress::finish();
                }
                continue;
            }
        }
        if menu.display != display {
            art::resize_body_font(display.text, menu.display.text);
            display = menu.display;
            let _ = fs::write(art::DISPLAY, display.save());
        }
        frames = frames.wrapping_add(1);
        retired.retain(|(frame, _)| frames.wrapping_sub(*frame) < 2);
        if previews_work && menu.page() == Page::Mods {
            let wanted = menu.model().map(str::to_owned);
            if preview.as_ref().map(|(shown, _)| shown) != Some(&wanted) {
                let loaded = match model_view::Preview::load(wanted.as_deref()) {
                    Ok(model) => Some(model),
                    Err(err) => {
                        job::log(&format!("Model preview for {}: {err}", wanted.as_deref().unwrap_or("the game")));
                        None
                    }
                };
                if let Some((_, Some(old))) = preview.replace((wanted, loaded)) {
                    retired.push((frames, old));
                }
            }
        } else if let Some((_, Some(old))) = preview.take() {
            retired.push((frames, old));
        }
        let shown = preview.as_ref().and_then(|(_, model)| model.as_ref()).map(|model| (model, frames as f32 * 0.02));
        menu_screen::draw(&menu.view(), &loaded_icons, top_art, &skin, display, shown);
    }
    draw::stop();
}
