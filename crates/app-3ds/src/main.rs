//! A Link Between Worlds randomizer for the 3DS.
//!
//! The menus run on the main thread. Making a seed runs on a worker thread with lower priority. The
//! main thread sleeps between frames, so the kernel lets it interrupt the work on time. A New 3DS
//! gives the worker its own CPU core.

mod art;
mod draw;
mod icons;
mod job;
mod memory;
mod menu_screen;
mod progress;

use std::fs;
use std::path::Path;

use ctru::applets::swkbd::{Button, ButtonConfig, Kind as KeyboardKind, SoftwareKeyboard};
use ctru::prelude::*;
use ui_core::menu::{Action, ModKind, Buttons, Input, Menu};
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
            Ok((preset, skipped)) => {
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
    let path = format!("{}/{file_name}", job::PRESET_DIR);
    if Path::new(&path).exists() {
        menu.show_message("Name Already Used", "Pick another name, or delete that file from the presets folder.");
        return;
    }
    let written = fs::create_dir_all(job::PRESET_DIR).and_then(|()| fs::write(&path, presets::to_file_text(&name, &menu.rules)));
    match written {
        Ok(()) => {
            menu.preset_name = name;
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
    let mut display = art::load_display();
    check_log.push_str(&format!("{art_note}\n{}\n", art::load_fonts(display.text)));
    let _ = fs::write(job::CHECK_LOG, check_log);

    let (preset_list, notes) = load_presets();
    let mut menu = Menu::new(random_seed(), preset_list, !new_memory);
    menu.set_mods(job::list_mods(job::MODEL_DIR), job::list_mods(job::VOICE_DIR));
    menu.game_location = game_location;
    menu.display = display;
    if !notes.is_empty() {
        menu.show_message("Preset Notes", &notes.join(" "));
    }
    let mut seed_was_typed = false;
    let mut working: Option<progress::Screen> = None;
    let mut vblanks = 0u32;
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

    // For testing: opens a page of rules, to check how it looks.
    if let Some(category) = fs::read_to_string(job::TEST_PAGE).ok().and_then(|text| text.trim().parse().ok()) {
        menu.open_category(category);
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
            Action::Generate => {
                job::set_request(job::Request {
                    seed: menu.seed,
                    seed_was_typed,
                    settings: menu.rules.settings(),
                    preset_name: menu.preset_name.clone(),
                    new_memory,
                    model: menu.model().map(str::to_owned),
                    voice: menu.voice().map(str::to_owned),
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
        menu_screen::draw(&menu.view(), &loaded_icons, top_art, display);
    }
    draw::stop();
}
